//! Generated audio as declared resources.
//!
//! A Scene Program declares the speech and sound it wants; [`Media`]
//! reconciles each declaration against `media.lock.json` beside the scene.
//! The first run generates what is missing and every run times its
//! choreography to the real words, while later runs call nothing unless a
//! declaration changed. Each resource has an author-chosen id and a content
//! key: a hash of everything that affects its audio (backend, model, voice,
//! exact text with its direction tags, settings, seed, format, and the
//! post-processing and alignment that follow). The delta is `+` create, `~`
//! replace, `=` up to date, and `-` orphan.
//!
//! ```ignore
//! let media = Media::open(env!("CARGO_MANIFEST_DIR"))?;
//! let kit = Voice::eleven(ELEVEN_KIT).v4().stability(0.2);
//! let hush = media.say("hush", &kit, "[extremely soft ASMR whisper] Oh... I hear you like... balls.")?;
//! let pop = media.sfx("pop", "a single soft glassy pop, tiny and dry", seconds(0.5))?;
//! let demon = hush.derive(Effect::pitch(-6.0))?;
//! media.finish()?;
//!
//! let said = hush.place(&mut scene, SECOND);
//! for at in said.at_every("balls") {
//!     pop.play(&mut scene, at, -18.0);
//! }
//! ```
//!
//! `PSYCHOPOMP_MEDIA` selects a [`Mode`]: `apply` (default), `plan` (report
//! the delta and its cost, call nothing), `draft` (stand-ins from macOS
//! `say`), or `prune` (delete orphans). Credentials come from
//! `ELEVENLABS_API_KEY` (and, for retired Fish voices, `FISH_AUDIO_API_KEY`),
//! or the nearest `.env`.
mod adopt;
mod eleven;
mod ffmpeg;
mod fish;
mod lock;
mod media;
mod spec;
mod studio;
mod words;

pub use adopt::{FISH_KIT, adopt};
pub use media::{Audio, Change, Media, Mode, Report};
pub use spec::{
    Backend, ELEVEN_KIT, ELEVEN_SOUND, ELEVEN_V4, Effect, FISH_FREE, Line, Route, SAY_DEFAULT,
    Sound, Voice, WHISPER,
};

/// Print the lock in `root`: one line per resource.
pub fn show(root: &std::path::Path) -> anyhow::Result<String> {
    let lock = lock::Lock::read(root)?;
    let mut out = String::new();
    for (id, entry) in &lock.resources {
        let kind = entry.spec["kind"].as_str().unwrap_or("?");
        out += &format!(
            "{id}  {kind}  {:.3}s  {}  {}{}{}\n",
            entry.duration_nanos as f64 / 1e9,
            entry.key,
            entry.file,
            entry
                .requests
                .first()
                .map(|request| format!("  request {request}"))
                .unwrap_or_default(),
            if entry.adopted.is_some() {
                "  (adopted)"
            } else {
                ""
            }
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// A fresh scratch directory for one test.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "psychopomp-media-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
