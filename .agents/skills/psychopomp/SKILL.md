---
name: psychopomp
description: >
  Make videos, explainer reels, architecture breakdowns, PR walkthroughs, product demos,
  and deterministic motion graphics in Rust with Psychopomp: wgpu linear-HDR Stage, 24x
  shutter motion blur, keyed code diffs with IDE diagnostics, native chat/terminal
  surfaces, ElevenLabs v4 narration, and headless layout, pop, strip, and LUFS checks.
  Use whenever asked to generate, edit, check, or render a programmatic video or extend
  Psychopomp's recipes.
---

# Psychopomp Video Playbook

Psychopomp compiles lightweight, GPU-free Rust **Scene Programs** (`scenes/<name>`)
in under a second into declarative JSON **Scene Plans** and **Reels**, then renders
`Time -> Pixels` on `wgpu` in linear HDR (`Rgba16Float`) with 24-sample shutter
motion blur, `cosmic-text` shaping (bundled `CommitMono`), and FFmpeg H.264/AAC
delivery. Every visual beat is a pure function of `u64` nanosecond time; interrupted
springs carry both position and velocity.

Before authoring, load `explainer-motion` and read [STORY.md](STORY.md) for
narrative structure, visual form selection, and anti-slop rules. Canonical
reference scenes in the repo:
- `scenes/opc18-catalog`: measured performance story with card-clone decks,
  `RollingNumberActor`, `MeterActor`, and stepped `Diff`.
- `scenes/pr-walkthrough/src/flagship.rs`: service orb + client topology zooming
  into stable inline code transitions.
- `scenes/diagnostics` & `scenes/text-surfaces`: IDE error waves, hover cards,
  inlay hints, native chat threads, terminals, and diffstats.

## Steps

1. **Pin down facts and receipts.** Read the actual source files, diffs, or
   measurements (`gh pr diff`, `gh pr view`). Write down the exact identifiers,
   line numbers, and numbers you will show on screen.
   *Done when:* every narrated claim and code line traces to real source or a
   reproduced measurement, with zero invented identifiers or placeholder metrics.

2. **Author and reconcile voice first (`psychopomp-media`).** When narrated,
   declare speech inside `scenes/<name>/src/main.rs` with `psychopomp-media` so
   ElevenLabs `eleven_v4` (`stability(0.45..0.5)`, `similarity(0.75)`) returns
   character-aligned word timestamps directly into `media.lock.json` (`PROVIDER_ALIGN`):
   ```rust
   let media = Media::open(&root)?;
   let voice = Voice::eleven(VOICE_ID).v4().stability(0.45).similarity(0.75);
   let intro = media.dialogue("intro", [(&voice, "[calm, conversational] ...")])?;
   media.finish()?;
   ```
   Dry-run character cost with zero API calls, then synthesize with
   `ELEVENLABS_API_KEY` in the environment:
   ```sh
   PSYCHOPOMP_MEDIA=plan cargo run -p psychopomp-<name>
   ELEVENLABS_API_KEY=... cargo run -p psychopomp-<name>
   ```
   Later edits to choreography reuse `media.lock.json` (`=`) with zero API calls.
   *Done when:* `media.lock.json` is up to date (`=`) and `words` contains no
   spoken `[direction]` bracket tags.

3. **Build the Scene Program (`scenes/<name>`).** The workspace globs `scenes/*`
   automatically (`psychopomp` + `psychopomp-media` deps only; compiles in <1s).
   Choose the concrete recipe that matches each act instead of defaulting to
   generic node boxes:
   - **Chat / Slack / messaging context:** `ChatActor` (`ChatPlan::new`,
     `.titled("#channel", None)` or `ChatStyle::Bubbles`, `.say`, `.stream`,
     `.highlight`, `.react`).
   - **Code receipts and fixes:** `editor::diff::Diff` (`keep`, `remove`, `add`,
     `<= 76` cols, `<= 14` rows) with `.range(id, text)` + `.inlay(id, range, ghost)`,
     paired with `DiagnosticActor` (`DiagnosticPlan::error`), `HoverActor`, or
     `CalloutActor`.
   - **Systems, pipelines, and physical metaphors:** `StageActor` on
     `StagePost::RESTRAINED` (`card`, `orb`, `Form` 3D shapes, `beam`, `Path`,
     `packet`, `Bolt` + `zap`, `dissolve` / `materialize`, `burst` + `rewind` +
     `unburst`, `CameraRig` moves).
   - **UI and hit-testing bugs:** magnified control anatomy (`Material::Well`,
     `Material::Raised`, cursor icons `"cursor"`, `"hand-pointing"`) +
     `VideoActor` + `ReelSegmentPlan::wiped`.
   - **Benchmarks, checks, and counters:** `ChecklistActor`, `BarsActor`,
     `MeterActor`, `RollingNumberActor`, `ConfettiActor`, `TerminalActor`,
     `ChangedFilesActor`.
   Key every beat to spoken phrases with `spoken.at_any(&["phrase", "fallback"])`
   and causal guards (`.not_before(send.reply())`), frame segments with
   `chrome::{header, chip, footer}`, and Layer SFX (`sfx::TICK`, `MARK`,
   `CONFIRM`, `WHOOSH`, `GLITCH`, `IMPACT`, `BOOM`, `SPARKLE`) at `-10..-18 dB`.
   *Done when:* `cargo run -p psychopomp-<name>` emits `<name>.reel.json` and
   `plan validate` reports `"valid": true`.

4. **Run the headless check loop before encoding video.** Address any point or
   segment with `psychopomp::time_spec` (`3.2s`, `500ms`, `120f`, `50%`, `intro`,
   `intro@1.5s`, `intro..outro`):
   ```sh
   cargo run --release -- plan check scenes/<name>/<name>.reel.json --pops --audio --theme neutral
   cargo run --release -- plan strip scenes/<name>/<name>.reel.json all output/strip.png -n 16 --cols 4 --shutter --theme neutral
   cargo run --release -- plan frame scenes/<name>/<name>.reel.json "intro@2s,receipts@50%,verdict@end" output/stills --shutter --theme neutral
   ```
   - `plan check [--pops] [--audio]` verifies projected canvas bounds
     (`clipped_by_canvas`, `off_canvas`, `nan_pose`), **Stage card text fit**
     (`card_text_overflow`), 1-frame step/flash discontinuities (`--pops`),
     missing audio files, digital clipping, and ITU-R BS.1770-4 loudness (aim
     for `-14..-18 LUFS` integrated, true peak `< -1 dBTP`, segments within `2 LU`).
   - `plan audio <reel.json> --at "intro@2s,verdict@8s"` lists active clips,
     offsets, and gains at exact instants.
   - Inspect `output/strip.png` and full-resolution stills (or sharp crops via
     `plan frame ... --crop x,y,w,h` and motion trails via `plan onion`). Fix
     cramped margins, blank frame-0 starts, scroll-clipped chat rows, or
     overlapping labels before encoding MP4.
   *Done when:* `plan check --pops --audio` reports `0` errors and `0` unintended
   warnings, and you have visually inspected the contact sheet and key stills.

5. **Render and probe.** Encode the full-shutter 1080p60 MP4 once after sheets
   and checks pass (`--draft` is for fast single-sample timing checks only):
   ```sh
   cargo run --release -- plan render scenes/<name>/<name>.reel.json output/<name>.mp4 --theme neutral
   ffprobe -v error -show_entries stream=codec_type,codec_name,width,height,r_frame_rate,duration -of json output/<name>.mp4
   ```
   *Done when:* `ffprobe` confirms `1920x1080`, `60/1` fps, and `h264` + `aac`
   matching the reel duration.

## Extending Psychopomp

When a film needs a capability the engine lacks, build it cleanly in Psychopomp
rather than hacking around it in one scene:
- Pure types, validation, and math in `crates/psychopomp` (`psychopomp::math`,
  zero GPU deps; Stage defaults in `StagePlan::channel_default`).
- Strict preflight in `crates/psychopomp-render/src/plan_runtime/` and pixels in
  `src/render/` (hot-reload WGSL via `PSYCHOPOMP_SHADER_DIR`).
- Focused unit tests (`cargo test -p psychopomp ...`, `cargo test -p psychopomp-render ...`)
  and `verify baseline` / `verify compare --expect <scenes>` when touching shared
  rendering paths.
