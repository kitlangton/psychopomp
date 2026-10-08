//! What a Scene Program declares (voices, lines, sounds, effects) and the
//! canonical [`Spec`] each declaration lowers into. A spec names everything
//! that affects the generated audio; its hash is the resource's content key.
use anyhow::{Result, bail};
use psychopomp::transcript::WordTiming;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{media::Audio, words};

/// ElevenLabs' model for finished narration.
pub const ELEVEN_V4: &str = "eleven_v4";
/// ElevenLabs' sound-effect model.
pub const ELEVEN_SOUND: &str = "eleven_text_to_sound_v2";
/// Fish Audio's free S2.1 model, the `fish-say` default.
pub const FISH_FREE: &str = "s2.1-pro-free";
/// The Whisper model that times speech without provider timestamps
/// (`scripts/narrate.ts`'s default).
pub const WHISPER: &str = "mlx-community/whisper-large-v3-mlx";
/// The macOS voice that drafts speech.
pub const SAY_DEFAULT: &str = "Samantha";

pub(crate) const ELEVEN_FORMAT: &str = "mp3_44100_192";
pub(crate) const FISH_FORMAT: &str = "wav_44100";
pub(crate) const SAY_FORMAT: &str = "wav_48000";
/// Post-processing is part of the key: change the code, change the string.
pub(crate) const SPEECH_POST: &str = "loudnorm=I=-16:TP=-1.5:LRA=11|mp3=48000:mono:160k";
pub(crate) const SOUND_POST: &str =
    "trim-onset=0.02:-0.003|peak=0.5|fade-out=0.06|wav=48000:mono:s16";
pub(crate) const SILENCE_POST: &str = "wav=48000:mono:s16";
pub(crate) const DERIVE_POST: &str = "ffmpeg-effect/1";
pub(crate) const PROVIDER_ALIGN: &str = "provider";

/// Bumped only when the canonical form itself changes, which rekeys everything.
const KEY_SCHEMA: &str = "psychopomp-media/1";

/// Who generates audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    ElevenLabs,
    Fish,
    /// macOS `say`: free, local drafts for timing a scene.
    Say,
}

impl Backend {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ElevenLabs => "ElevenLabs",
            Self::Fish => "Fish Audio",
            Self::Say => "say",
        }
    }
}

/// Which speech endpoint renders a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    /// Text to Speech (ElevenLabs with character timestamps).
    Speech,
    /// ElevenLabs Text to Dialogue: one or more voices in one performance.
    Dialogue,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub ivc: bool,
}

impl Settings {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A speaker and how to call it. Settings are part of every key they voice.
///
/// ```ignore
/// let kit = Voice::eleven(KIT).v4().stability(0.2).similarity(0.65);
/// let fish = Voice::fish(FISH_KIT);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    backend: Backend,
    id: String,
    model: Option<String>,
    settings: Settings,
    whisper: bool,
    format: Option<String>,
}

impl Voice {
    /// An ElevenLabs voice on `eleven_v4`, timed by its character alignment.
    pub fn eleven(id: impl Into<String>) -> Self {
        Self::new(Backend::ElevenLabs, id, Some(ELEVEN_V4))
    }

    /// A Fish Audio voice on the free S2.1 model at normal speed, timed by Whisper.
    pub fn fish(id: impl Into<String>) -> Self {
        let mut voice = Self::new(Backend::Fish, id, Some(FISH_FREE));
        voice.settings.speed = Some(1.0);
        voice
    }

    /// A macOS `say` voice, such as `"Samantha"`, timed by Whisper.
    pub fn say(name: impl Into<String>) -> Self {
        Self::new(Backend::Say, name, None)
    }

    fn new(backend: Backend, id: impl Into<String>, model: Option<&str>) -> Self {
        Self {
            backend,
            id: id.into(),
            model: model.map(str::to_owned),
            settings: Settings::default(),
            whisper: false,
            format: None,
        }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// `eleven_v4`, the default ElevenLabs model, said out loud.
    pub fn v4(self) -> Self {
        self.model(ELEVEN_V4)
    }

    /// ElevenLabs: lower is more expressive, higher more consistent.
    pub fn stability(mut self, stability: f64) -> Self {
        self.settings.stability = Some(stability);
        self
    }

    /// ElevenLabs: adherence to the reference voice (`similarity_boost` in
    /// Text to Speech, `similarity` in Text to Dialogue).
    pub fn similarity(mut self, similarity: f64) -> Self {
        self.settings.similarity = Some(similarity);
        self
    }

    /// Fish Audio speaking rate, 0.5 to 2. Eleven v4 has no speed control;
    /// direct pace in the text instead.
    pub fn speed(mut self, speed: f64) -> Self {
        self.settings.speed = Some(speed);
        self
    }

    /// ElevenLabs best-effort deterministic sampling.
    pub fn seed(mut self, seed: u32) -> Self {
        self.settings.seed = Some(seed);
        self
    }

    /// ElevenLabs ISO 639-1 language, enforcing pronunciation and normalization.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.settings.language = Some(language.into());
        self
    }

    /// ElevenLabs `use_pvc_as_ivc`: a professional clone's instant version,
    /// sometimes more expressive.
    pub fn ivc(mut self) -> Self {
        self.settings.ivc = true;
        self
    }

    /// Time words with Whisper instead of the provider's alignment.
    pub fn whisper(mut self) -> Self {
        self.whisper = true;
        self
    }

    /// ElevenLabs output format, an `mp3_*` such as `mp3_44100_128`, for
    /// accounts whose tier cannot request the default `mp3_44100_192`.
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    pub(crate) fn backend(&self) -> Backend {
        self.backend
    }

    fn check(&self) -> Result<()> {
        let s = &self.settings;
        if let Some(format) = &self.format
            && (self.backend != Backend::ElevenLabs || !format.starts_with("mp3_"))
        {
            bail!("only ElevenLabs voices choose an output format, and it must be an mp3_* format");
        }
        match self.backend {
            Backend::ElevenLabs if s.speed.is_some() => {
                bail!("ElevenLabs v4 has no speed control; direct pace in the text")
            }
            Backend::Fish
                if s.stability.is_some()
                    || s.similarity.is_some()
                    || s.seed.is_some()
                    || s.language.is_some()
                    || s.ivc =>
            {
                bail!("Fish Audio voices take only a model and a speed")
            }
            Backend::Say if !s.is_default() || self.model.is_some() => {
                bail!("`say` voices take only a name")
            }
            _ => Ok(()),
        }
    }

    /// The canonical spec for `lines` in this voice's settings.
    pub(crate) fn speech(
        &self,
        route: Route,
        lines: Vec<Said>,
        previous: Vec<String>,
    ) -> Result<SpeechSpec> {
        self.check()?;
        if route == Route::Dialogue && self.backend != Backend::ElevenLabs {
            bail!("Text to Dialogue needs ElevenLabs voices");
        }
        if !previous.is_empty() && self.backend != Backend::ElevenLabs {
            bail!("only ElevenLabs stitches a line after another");
        }
        let eleven = self.format.as_deref().unwrap_or(ELEVEN_FORMAT);
        let (format, align) = match self.backend {
            Backend::ElevenLabs if !self.whisper => (eleven, PROVIDER_ALIGN.to_owned()),
            Backend::ElevenLabs => (eleven, whisper_align()),
            Backend::Fish => (FISH_FORMAT, whisper_align()),
            Backend::Say => (SAY_FORMAT, whisper_align()),
        };
        Ok(SpeechSpec {
            backend: self.backend,
            model: self.model.clone(),
            route,
            lines,
            settings: self.settings.clone(),
            previous,
            format: format.to_owned(),
            post: SPEECH_POST.to_owned(),
            align,
        })
    }

    /// Whether one Text to Dialogue request can voice both: dialogue applies
    /// one model, one set of settings, and one alignment to every line.
    pub(crate) fn shares_request(&self, other: &Self) -> bool {
        self.backend == other.backend
            && self.model == other.model
            && self.settings == other.settings
            && self.whisper == other.whisper
            && self.format == other.format
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

pub(crate) fn whisper_align() -> String {
    format!("whisper:{WHISPER}")
}

/// What to say, optionally stitched after an earlier ElevenLabs line so the
/// two read as one performance (`previous_request_ids`).
///
/// ```ignore
/// let toys = media.say("toys", &kit, Line::new(TOYS).after(&hush))?;
/// ```
#[derive(Clone, Debug)]
pub struct Line {
    pub(crate) text: String,
    pub(crate) after: Option<(String, String)>,
}

impl Line {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            after: None,
        }
    }

    /// Continue from `previous`: its key joins this line's key, so
    /// regenerating it regenerates this line too.
    pub fn after(mut self, previous: &Audio) -> Self {
        self.after = Some((previous.id().to_owned(), previous.key().to_owned()));
        self
    }
}

impl From<&str> for Line {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Line {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

/// A sound-effect prompt. Describe source, material, attack, body, and decay.
///
/// ```ignore
/// media.sfx("bed", Sound::new("light rain on a tin roof").looping(), seconds(8.0))?;
/// ```
#[derive(Clone, Debug)]
pub struct Sound {
    prompt: String,
    influence: Option<f64>,
    looping: bool,
}

impl Sound {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            influence: None,
            looping: false,
        }
    }

    /// How literally to follow the prompt, 0 to 1 (ElevenLabs default 0.3).
    pub fn influence(mut self, influence: f64) -> Self {
        self.influence = Some(influence);
        self
    }

    /// A seamless loop, for an ambient bed.
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    pub(crate) fn spec(self, duration_nanos: u64) -> Result<SoundSpec> {
        let seconds = duration_nanos as f64 / 1e9;
        if !(0.5..=30.0).contains(&seconds) {
            bail!("sound effects last 0.5 to 30 seconds, not {seconds}");
        }
        Ok(SoundSpec {
            backend: Backend::ElevenLabs,
            model: ELEVEN_SOUND.to_owned(),
            prompt: self.prompt,
            duration_nanos,
            influence: self.influence,
            looping: self.looping,
            format: ELEVEN_FORMAT.to_owned(),
            post: SOUND_POST.to_owned(),
        })
    }
}

impl From<&str> for Sound {
    fn from(prompt: &str) -> Self {
        Self::new(prompt)
    }
}

impl From<String> for Sound {
    fn from(prompt: String) -> Self {
        Self::new(prompt)
    }
}

/// An ffmpeg transformation that derives one resource from another. Word
/// timings follow the transformation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    /// Shift pitch at constant duration. Formants move too: a demon or a chipmunk.
    Pitch { semitones: f64 },
    /// Play `factor` times faster at constant pitch.
    Tempo { factor: f64 },
    /// Play backwards.
    Reverse,
    /// Keep only source time `start..end`, in nanoseconds.
    #[serde(rename_all = "camelCase")]
    Trim { start_nanos: u64, end_nanos: u64 },
    /// Change level by `db` decibels.
    Gain { db: f64 },
}

impl Effect {
    pub fn pitch(semitones: f64) -> Self {
        Self::Pitch { semitones }
    }

    pub fn tempo(factor: f64) -> Self {
        Self::Tempo { factor }
    }

    pub fn reverse() -> Self {
        Self::Reverse
    }

    pub fn trim(start: u64, end: u64) -> Self {
        Self::Trim {
            start_nanos: start,
            end_nanos: end,
        }
    }

    pub fn gain(db: f64) -> Self {
        Self::Gain { db }
    }

    pub(crate) fn check(&self) -> Result<()> {
        match *self {
            Self::Pitch { semitones } if !semitones.is_finite() || semitones.abs() > 24.0 => {
                bail!("pitch shifts at most two octaves")
            }
            Self::Tempo { factor } if !(0.25..=4.0).contains(&factor) => {
                bail!("tempo factors run from 0.25 to 4")
            }
            Self::Trim {
                start_nanos,
                end_nanos,
            } if end_nanos <= start_nanos => bail!("a trim must end after it starts"),
            Self::Gain { db } if !db.is_finite() => bail!("gain must be finite"),
            _ => Ok(()),
        }
    }

    /// The derived resource's id suffix, such as `pitch(-6)`.
    pub(crate) fn label(&self) -> String {
        match *self {
            Self::Pitch { semitones } => format!("pitch({semitones})"),
            Self::Tempo { factor } => format!("tempo({factor})"),
            Self::Reverse => "reverse".to_owned(),
            Self::Trim {
                start_nanos,
                end_nanos,
            } => format!(
                "trim({}-{})",
                start_nanos as f64 / 1e9,
                end_nanos as f64 / 1e9
            ),
            Self::Gain { db } => format!("gain({db})"),
        }
    }

    /// The derived duration, before the file exists to measure.
    pub(crate) fn duration(&self, source: u64) -> u64 {
        match *self {
            Self::Tempo { factor } => (source as f64 / factor) as u64,
            Self::Trim {
                start_nanos,
                end_nanos,
            } => end_nanos.min(source).saturating_sub(start_nanos),
            _ => source,
        }
    }

    /// Where the source's words land in the derived audio.
    pub(crate) fn retime(&self, words: &[WordTiming], source: u64) -> Vec<WordTiming> {
        let seconds = source as f64 / 1e9;
        let word = |w: &WordTiming, start: f64, end: f64| WordTiming {
            word: w.word.clone(),
            start,
            end,
        };
        let moved = match *self {
            Self::Pitch { .. } | Self::Gain { .. } => words.to_vec(),
            Self::Tempo { factor } => words
                .iter()
                .map(|w| word(w, w.start / factor, w.end / factor))
                .collect(),
            Self::Reverse => words
                .iter()
                .rev()
                .map(|w| word(w, (seconds - w.end).max(0.0), (seconds - w.start).max(0.0)))
                .collect(),
            Self::Trim {
                start_nanos,
                end_nanos,
            } => {
                let (from, to) = (start_nanos as f64 / 1e9, end_nanos as f64 / 1e9);
                words
                    .iter()
                    .filter(|w| w.end > from && w.start < to)
                    .map(|w| word(w, w.start.max(from) - from, w.end.min(to) - from))
                    .collect()
            }
        };
        words::ordered(moved)
    }
}

/// One voiced line of a speech spec.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Said {
    pub voice: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpeechSpec {
    pub backend: Backend,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub route: Route,
    pub lines: Vec<Said>,
    #[serde(skip_serializing_if = "Settings::is_default")]
    pub settings: Settings,
    /// Keys of the stitched predecessors.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub previous: Vec<String>,
    pub format: String,
    pub post: String,
    pub align: String,
}

impl SpeechSpec {
    pub(crate) fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SoundSpec {
    pub backend: Backend,
    pub model: String,
    pub prompt: String,
    pub duration_nanos: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub influence: Option<f64>,
    #[serde(rename = "loop", skip_serializing_if = "is_false")]
    pub looping: bool,
    pub format: String,
    pub post: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeriveSpec {
    /// The source resource's key.
    pub source: String,
    pub effect: Effect,
    pub post: String,
}

/// Everything that affects one resource's audio, in canonical form.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum Spec {
    Speech(SpeechSpec),
    Sound(SoundSpec),
    /// A draft stand-in for a sound effect: silence of the declared length.
    #[serde(rename_all = "camelCase")]
    Silence {
        duration_nanos: u64,
        post: String,
    },
    Derive(DeriveSpec),
}

impl Spec {
    /// Sorted-key JSON: field order and absent options never change a key.
    pub(crate) fn canonical(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("specs serialize")
    }

    /// The content key: the first 16 hex digits of SHA-256 over the canonical
    /// spec. Equal keys mean interchangeable audio.
    pub(crate) fn key(&self) -> String {
        let canonical = serde_json::to_string(&self.canonical()).expect("specs serialize");
        Sha256::digest(format!("{KEY_SCHEMA}\n{canonical}"))
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// The stored file's extension (derived audio keeps its source's).
    pub(crate) fn extension(&self) -> Option<&'static str> {
        match self {
            Self::Speech(_) => Some("mp3"),
            Self::Sound(_) | Self::Silence { .. } => Some("wav"),
            Self::Derive(_) => None,
        }
    }

    /// The free local stand-in a draft run generates instead.
    pub(crate) fn draft(&self, say_voice: &str) -> Option<Spec> {
        match self {
            Self::Speech(speech) if speech.backend != Backend::Say => {
                let text = speech.text();
                Voice::say(say_voice)
                    .speech(
                        Route::Speech,
                        vec![Said {
                            voice: say_voice.to_owned(),
                            text,
                        }],
                        Vec::new(),
                    )
                    .ok()
                    .map(Self::Speech)
            }
            Self::Sound(sound) => Some(Self::Silence {
                duration_nanos: sound.duration_nanos,
                post: SILENCE_POST.to_owned(),
            }),
            _ => None,
        }
    }

    /// The paid provider this spec calls, if any.
    pub(crate) fn provider(&self) -> Option<Backend> {
        match self {
            Self::Speech(speech) if speech.backend != Backend::Say => Some(speech.backend),
            Self::Sound(sound) => Some(sound.backend),
            _ => None,
        }
    }

    /// Billable characters: speech text, including direction tags.
    pub(crate) fn characters(&self) -> usize {
        match self {
            Self::Speech(speech) if speech.backend != Backend::Say => speech
                .lines
                .iter()
                .map(|line| line.text.chars().count())
                .sum(),
            _ => 0,
        }
    }

    /// One line for the delta report.
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Speech(speech) => {
                let model = speech.model.as_deref().unwrap_or("local");
                let route = match speech.route {
                    Route::Speech => "speech",
                    Route::Dialogue => "dialogue",
                };
                format!(
                    "{route}  {}/{model}  \"{}\"",
                    speech.backend.name(),
                    preview(&speech.text(), 48)
                )
            }
            Self::Sound(sound) => format!(
                "sound  {}/{}  {}s  \"{}\"",
                sound.backend.name(),
                sound.model,
                sound.duration_nanos as f64 / 1e9,
                preview(&sound.prompt, 48)
            ),
            Self::Silence { duration_nanos, .. } => {
                format!("silence  {}s (draft sound)", *duration_nanos as f64 / 1e9)
            }
            Self::Derive(derive) => {
                format!("derive  {} of {}", derive.effect.label(), derive.source)
            }
        }
    }
}

pub(crate) fn preview(text: &str, limit: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= limit {
        return flat;
    }
    flat.chars().take(limit - 1).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIT: &str = "8olojUk4IXpvKgaOCHXj";

    fn line(voice: &Voice, text: &str) -> Spec {
        Spec::Speech(
            voice
                .speech(
                    Route::Speech,
                    vec![Said {
                        voice: voice.id().to_owned(),
                        text: text.to_owned(),
                    }],
                    Vec::new(),
                )
                .unwrap(),
        )
    }

    #[test]
    fn keys_are_canonical_and_stable() {
        let kit = Voice::eleven(KIT).stability(0.2).similarity(0.65);
        let spec = line(&kit, "[soft] Hi.");
        // Builder order never matters; the canonical form sorts keys.
        let same = line(
            &Voice::eleven(KIT).similarity(0.65).stability(0.2),
            "[soft] Hi.",
        );
        assert_eq!(spec.key(), same.key());
        assert_eq!(spec.key().len(), 16);
        let canonical = serde_json::to_string(&spec.canonical()).unwrap();
        assert_eq!(
            canonical,
            format!(
                r#"{{"align":"provider","backend":"elevenlabs","format":"mp3_44100_192","kind":"speech","lines":[{{"text":"[soft] Hi.","voice":"{KIT}"}}],"model":"eleven_v4","post":"{SPEECH_POST}","route":"speech","settings":{{"similarity":0.65,"stability":0.2}}}}"#
            )
        );
        // A golden key: changing the canonical form or hashing rekeys every lock.
        assert_eq!(spec.key(), "78ad04e30e06ec0a");
        for changed in [
            line(&kit, "[soft] Hi!"),
            line(&kit.clone().stability(0.3), "[soft] Hi."),
            line(&kit.clone().seed(7), "[soft] Hi."),
            line(&kit.clone().whisper(), "[soft] Hi."),
            line(&kit.clone().model("eleven_v4_turbo"), "[soft] Hi."),
            line(&kit.clone().format("mp3_44100_128"), "[soft] Hi."),
            line(
                &Voice::eleven("other").stability(0.2).similarity(0.65),
                "[soft] Hi.",
            ),
        ] {
            assert_ne!(changed.key(), spec.key(), "{changed:?}");
        }
    }

    #[test]
    fn absent_options_are_not_part_of_the_key() {
        let plain = line(&Voice::eleven(KIT), "Hi.");
        let canonical = serde_json::to_string(&plain.canonical()).unwrap();
        assert!(!canonical.contains("settings"), "{canonical}");
        assert!(!canonical.contains("previous"), "{canonical}");
    }

    #[test]
    fn voices_reject_settings_their_backend_lacks() {
        let said = || {
            vec![Said {
                voice: "v".into(),
                text: "Hi".into(),
            }]
        };
        assert!(
            Voice::eleven(KIT)
                .speed(1.2)
                .speech(Route::Speech, said(), vec![])
                .is_err()
        );
        assert!(
            Voice::fish("f")
                .stability(0.3)
                .speech(Route::Speech, said(), vec![])
                .is_err()
        );
        assert!(
            Voice::fish("f")
                .speech(Route::Dialogue, said(), vec![])
                .is_err()
        );
        assert!(
            Voice::fish("f")
                .format("mp3_44100_128")
                .speech(Route::Speech, said(), vec![])
                .is_err()
        );
        assert!(
            Voice::eleven(KIT)
                .format("pcm_44100")
                .speech(Route::Speech, said(), vec![])
                .is_err()
        );
        assert!(Sound::new("pop").spec(100_000_000).is_err());
    }

    #[test]
    fn an_eleven_voice_requests_its_chosen_format() {
        let said = vec![Said {
            voice: KIT.into(),
            text: "Hi.".into(),
        }];
        let spec = Voice::eleven(KIT)
            .format("mp3_44100_128")
            .speech(Route::Speech, said.clone(), vec![])
            .unwrap();
        assert_eq!(spec.format, "mp3_44100_128");
        let default = Voice::eleven(KIT)
            .speech(Route::Speech, said, vec![])
            .unwrap();
        assert_eq!(default.format, "mp3_44100_192");
        assert!(
            !Voice::eleven(KIT).shares_request(&Voice::eleven(KIT).format("mp3_44100_128")),
            "one dialogue request has one format"
        );
    }

    #[test]
    fn derived_keys_follow_their_source_and_effect() {
        let derive = |source: &str, effect| {
            Spec::Derive(DeriveSpec {
                source: source.to_owned(),
                effect,
                post: DERIVE_POST.to_owned(),
            })
            .key()
        };
        let demon = derive("aaaa", Effect::pitch(-6.0));
        assert_eq!(demon, derive("aaaa", Effect::pitch(-6.0)));
        assert_ne!(demon, derive("bbbb", Effect::pitch(-6.0)), "source changed");
        assert_ne!(demon, derive("aaaa", Effect::pitch(-5.0)), "effect changed");
        assert_eq!(Effect::pitch(-6.0).label(), "pitch(-6)");
        assert_eq!(
            Effect::trim(500_000_000, 2_000_000_000).label(),
            "trim(0.5-2)"
        );
    }

    #[test]
    fn effects_move_words_with_the_audio() {
        let words = vec![
            WordTiming {
                word: "Oh".into(),
                start: 0.5,
                end: 1.0,
            },
            WordTiming {
                word: "balls".into(),
                start: 2.0,
                end: 3.0,
            },
        ];
        let reversed = Effect::reverse().retime(&words, 4_000_000_000);
        assert_eq!(
            reversed
                .iter()
                .map(|w| (w.word.as_str(), w.start, w.end))
                .collect::<Vec<_>>(),
            [("balls", 1.0, 2.0), ("Oh", 3.0, 3.5)]
        );
        let fast = Effect::tempo(2.0).retime(&words, 4_000_000_000);
        assert_eq!((fast[1].start, fast[1].end), (1.0, 1.5));
        let trimmed = Effect::trim(1_500_000_000, 4_000_000_000).retime(&words, 4_000_000_000);
        assert_eq!(trimmed.len(), 1);
        assert_eq!((trimmed[0].start, trimmed[0].end), (0.5, 1.5));
        assert_eq!(Effect::pitch(-6.0).retime(&words, 4_000_000_000), words);
        assert_eq!(
            Effect::trim(1_500_000_000, 9_000_000_000).duration(4_000_000_000),
            2_500_000_000
        );
    }
}
