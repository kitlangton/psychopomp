//! Adoption: record narration that `scripts/narrate.ts` already produced in a
//! Media Lock without regenerating it. Each clip's key is computed from the
//! recipe narrate.ts used (its engine, voice, model, settings, and text, plus
//! its Text to Dialogue route, loudness pass, and Whisper timing), so a scene
//! that declares the same recipe finds it up to date. The files stay where
//! they are.
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use psychopomp::transcript::Transcript;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    lock::{Entry, Lock, Word},
    spec::{
        Backend, ELEVEN_FORMAT, ELEVEN_KIT, ELEVEN_V4, FISH_FORMAT, FISH_FREE, Route, SAY_DEFAULT,
        SAY_FORMAT, SPEECH_POST, Said, Settings, Spec, SpeechSpec, whisper_align,
    },
};

/// `fish-say`'s default voice: Kit's Fish Audio clone.
pub const FISH_KIT: &str = "ebda56b7ebe44911bc1d1eabe41d53e3";

#[derive(Deserialize)]
pub(crate) struct Script {
    voice: Option<String>,
    speed: Option<f64>,
    model: Option<String>,
    settings: Option<Ordered>,
    clips: Vec<ScriptClip>,
}

#[derive(Deserialize)]
struct ScriptClip {
    id: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    clips: Vec<ManifestClip>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestClip {
    id: String,
    file: String,
    words: String,
    duration_nanos: u64,
    text_hash: String,
    engine: String,
    request_id: Option<String>,
}

impl Script {
    /// The voice narrate.ts used: ElevenLabs defaults to Kit's clone.
    fn voice(&self, engine: &str) -> Option<&str> {
        self.voice
            .as_deref()
            .or((engine == "elevenlabs").then_some(ELEVEN_KIT))
    }

    /// The Dialogue settings narrate.ts used, in its default's key order.
    fn settings(&self, engine: &str) -> Option<Ordered> {
        self.settings.clone().or_else(|| {
            (engine == "elevenlabs").then(|| {
                Ordered(vec![
                    ("stability".to_owned(), Value::from(0.5)),
                    ("similarity".to_owned(), Value::from(0.75)),
                ])
            })
        })
    }
}

/// A JSON object in document order, as JavaScript's `JSON.stringify` sees it.
#[derive(Clone)]
struct Ordered(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> Visitor<'de> for Entries {
            type Value = Ordered;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Ordered(entries))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}

impl Ordered {
    fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    }
}

/// Adopt every clip of the narrate.ts narration in `narration` into the lock
/// in `root`, returning the adopted ids. Refuses a clip whose recorded text
/// hash no longer matches `script.json`: that audio says something else.
pub fn adopt(narration: &Path, root: &Path) -> Result<Vec<String>> {
    let read = |name: &str| {
        let path = narration.join(name);
        fs::read(&path).with_context(|| format!("read {}", path.display()))
    };
    let script: Script = serde_json::from_slice(&read("script.json")?)?;
    let manifest: Manifest = serde_json::from_slice(&read("narration.json")?)?;
    let folder = relative(narration, root)?;
    let mut lock = Lock::read(root)?;
    let mut adopted = Vec::new();
    for clip in &manifest.clips {
        let text = &script
            .clips
            .iter()
            .find(|line| line.id == clip.id)
            .with_context(|| format!("script.json has no clip '{}'", clip.id))?
            .text;
        let hash = narrate_hash(&script, &clip.engine, text);
        if hash != clip.text_hash {
            bail!(
                "narration.json records '{}' for different text or settings than script.json; regenerate it instead",
                clip.id
            );
        }
        let spec = Spec::Speech(recipe(&script, &clip.engine, text)?);
        let words = Transcript::load(&narration.join(&clip.words))?;
        let key = spec.key();
        if let Some(existing) = lock.resources.get(&clip.id) {
            if existing.key == key {
                continue;
            }
            bail!("the lock already holds '{}' for other audio", clip.id);
        }
        lock.resources.insert(
            clip.id.clone(),
            Entry {
                key,
                file: join(&folder, &clip.file),
                duration_nanos: clip.duration_nanos,
                spec: spec.canonical(),
                requests: clip.request_id.iter().cloned().collect(),
                credits: None,
                adopted: Some(join(&folder, "narration.json")),
                words: words.words().iter().map(Word::from).collect(),
            },
        );
        adopted.push(clip.id.clone());
    }
    lock.write(root)?;
    Ok(adopted)
}

fn relative(narration: &Path, root: &Path) -> Result<PathBuf> {
    let (narration, root) = (narration.canonicalize()?, root.canonicalize()?);
    narration
        .strip_prefix(&root)
        .map(Path::to_owned)
        .with_context(|| {
            format!(
                "{} is not inside {}; plan media paths could not reach it",
                narration.display(),
                root.display()
            )
        })
}

fn join(folder: &Path, file: &str) -> String {
    folder.join(file).to_string_lossy().replace('\\', "/")
}

/// The spec of the pipeline narrate.ts ran for `engine`.
fn recipe(script: &Script, engine: &str, text: &str) -> Result<SpeechSpec> {
    let line = |voice: &str| {
        vec![Said {
            voice: voice.to_owned(),
            text: text.to_owned(),
        }]
    };
    Ok(match engine {
        "elevenlabs" => {
            let mut settings = Settings::default();
            if let Some(ordered) = &script.settings(engine) {
                if let Some((unknown, _)) = ordered
                    .0
                    .iter()
                    .find(|(name, _)| !matches!(name.as_str(), "stability" | "similarity"))
                {
                    bail!("script.json setting '{unknown}' has no Voice equivalent");
                }
                settings.stability = ordered.get("stability").and_then(Value::as_f64);
                settings.similarity = ordered.get("similarity").and_then(Value::as_f64);
            }
            SpeechSpec {
                backend: Backend::ElevenLabs,
                model: Some(script.model.clone().unwrap_or_else(|| ELEVEN_V4.to_owned())),
                route: Route::Dialogue,
                lines: line(script.voice(engine).unwrap_or(ELEVEN_KIT)),
                settings,
                previous: Vec::new(),
                format: ELEVEN_FORMAT.to_owned(),
                post: SPEECH_POST.to_owned(),
                align: whisper_align(),
            }
        }
        "fish" => SpeechSpec {
            backend: Backend::Fish,
            model: Some(FISH_FREE.to_owned()),
            route: Route::Speech,
            lines: line(script.voice.as_deref().unwrap_or(FISH_KIT)),
            settings: Settings {
                speed: Some(script.speed.unwrap_or(1.0)),
                ..Settings::default()
            },
            previous: Vec::new(),
            format: FISH_FORMAT.to_owned(),
            post: SPEECH_POST.to_owned(),
            align: whisper_align(),
        },
        "say" => SpeechSpec {
            backend: Backend::Say,
            model: None,
            route: Route::Speech,
            lines: line(SAY_DEFAULT),
            settings: Settings::default(),
            previous: Vec::new(),
            format: SAY_FORMAT.to_owned(),
            post: SPEECH_POST.to_owned(),
            align: whisper_align(),
        },
        other => bail!("unknown narration engine '{other}'"),
    })
}

/// narrate.ts's change hash, byte for byte:
/// `sha256("engine|voice|speed|text" + JSON.stringify({ model, settings }))`.
pub(crate) fn narrate_hash(script: &Script, engine: &str, text: &str) -> String {
    let mut hashed = format!(
        "{engine}|{}|{}|{text}",
        script.voice(engine).unwrap_or(""),
        js_number(script.speed.unwrap_or(1.0))
    );
    if engine == "elevenlabs" {
        let model = script.model.as_deref().unwrap_or(ELEVEN_V4);
        write!(hashed, "{{\"model\":{}", Value::from(model)).unwrap();
        if let Some(settings) = &script.settings(engine) {
            hashed.push_str(",\"settings\":{");
            for (index, (name, value)) in settings.0.iter().enumerate() {
                if index > 0 {
                    hashed.push(',');
                }
                write!(hashed, "{}:{}", Value::from(name.as_str()), js_value(value)).unwrap();
            }
            hashed.push('}');
        }
        hashed.push('}');
    }
    Sha256::digest(hashed)
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn js_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e21 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

fn js_value(value: &Value) -> String {
    match value.as_f64() {
        Some(number) => js_number(number),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUSH: &str = "[soft, sweet, almost whispering] Hi. [tender, gentle, smiling] This is Psychopomp. A tiny motion graphics library, in Rust. [warm, sugary, calm] Every frame is a pure function of time. [sudden, explosive SHOUTING] ANY FRAME! ANY ORDER!! [instantly sweet again, cooing softly] ...isn't that nice?";

    fn script(json: &str) -> Script {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn narrate_hashes_match_the_recorded_ones() {
        // The values scenes/psychopomp-intro/narration/narration.json records.
        let eleven = script(
            r#"{ "engine": "elevenlabs", "model": "eleven_v4", "voice": "8olojUk4IXpvKgaOCHXj",
                 "settings": { "stability": 0.2, "similarity": 0.65 }, "clips": [] }"#,
        );
        assert_eq!(
            narrate_hash(&eleven, "elevenlabs", HUSH),
            "73a05e9b7cd9c5e7"
        );
        let reordered = script(
            r#"{ "voice": "8olojUk4IXpvKgaOCHXj", "settings": { "similarity": 0.65, "stability": 0.2 }, "clips": [] }"#,
        );
        assert_ne!(
            narrate_hash(&reordered, "elevenlabs", HUSH),
            "73a05e9b7cd9c5e7",
            "JSON.stringify keeps document order"
        );
        let defaulted = script(r#"{ "clips": [] }"#);
        let explicit = script(
            r#"{ "voice": "8olojUk4IXpvKgaOCHXj", "settings": { "stability": 0.5, "similarity": 0.75 }, "clips": [] }"#,
        );
        assert_eq!(
            narrate_hash(&defaulted, "elevenlabs", HUSH),
            narrate_hash(&explicit, "elevenlabs", HUSH),
            "narrate.ts fills in Kit's voice and settings before hashing"
        );
        assert_eq!(js_number(1.0), "1");
        assert_eq!(js_number(0.65), "0.65");
    }
}
