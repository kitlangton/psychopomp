---
name: psychopomp
description: >
  Make explainer reels and deterministic motion graphics with Psychopomp:
  walkthrough videos of pull requests, code changes, protocols, or lifecycles,
  with GPU Stage choreography, callouts, rolling numbers, sequence diagrams,
  animated diffs, and optional ElevenLabs v4 narration in Kit's voice.
---

# Psychopomp explainer reels

An **explainer reel** is several independently authored Scene Plans joined on one
clock: behavior stories on the GPU Stage (orbs, cards, beams, packets, camera,
bloom), pinned Callouts, Rolling Numbers, changes as animated Stepped Diffs,
captions in a terminal voice, and optional spoken narration. When narrated,
every visual beat waits for a phrase in the transcript, so re-voicing re-times
the film automatically.

The working examples are in [`scenes/pr-walkthrough`](https://github.com/kitlangton/psychopomp/tree/main/scenes/pr-walkthrough): [`src/flagship.rs`](https://github.com/kitlangton/psychopomp/blob/main/scenes/pr-walkthrough/src/flagship.rs) (#50825 as a
Stage film that zooms into its code), [`src/stop_stage.rs`](https://github.com/kitlangton/psychopomp/blob/main/scenes/pr-walkthrough/src/stop_stage.rs) (#50042 combining Stage,
Callouts, Rolling Number, and a Zoom into an annotated code diff), and [`src/lib.rs`](https://github.com/kitlangton/psychopomp/blob/main/scenes/pr-walkthrough/src/lib.rs)
(the five-PR reel with sequence diagrams). Copy their shape; do not start from a
blank crate. Component payloads, channels, and commands live in [`SCENE_PLANS.md`](https://github.com/kitlangton/psychopomp/blob/main/SCENE_PLANS.md#make-a-narrated-explainer-reel)
("Make A Narrated Explainer Reel") and terms in [`CONTEXT.md`](https://github.com/kitlangton/psychopomp/blob/main/CONTEXT.md); read both before
authoring. Engineering rules are in [`AGENTS.md`](https://github.com/kitlangton/psychopomp/blob/main/AGENTS.md).

## Steps

1. **Establish the facts.** For each change, read the actual diff and PR description
   (`gh pr diff`, `gh pr view`) and write one sentence each for the broken behavior,
   the fixed behavior, and the change. Done when every claim you will narrate is
   traceable to code, a test, or a reproduced run; mark anything you could not verify
   and keep it out of the script.

2. **Write the script** as `scenes/<name>/narration/script.json`, one clip per part:
   intro, then per change `<slug>-before`, `<slug>-after`, `<slug>-code`, then outro.
   Follow [STORY.md](STORY.md) when writing it. Done when each clip is under about 30
   seconds spoken and every planned visual beat has a distinct anchor phrase in it.

3. **Voice it (optional).** Draft first with no credentials (`--draft` uses macOS
   `say`), then synthesize final narration with **ElevenLabs v4 in Kit's voice**:
   `eleven_v4` through Text to Dialogue with Kit's Professional Voice Clone
   `8olojUk4IXpvKgaOCHXj` (`use_pvc_as_ivc: false`, stability 0.5, similarity
   0.75). That is narrate.ts's default; spell it out in the script anyway. Fish
   Audio is retired: never use it.
   ```json
   {
     "engine": "elevenlabs",
     "model": "eleven_v4",
     "voice": "8olojUk4IXpvKgaOCHXj",
     "settings": { "stability": 0.5, "similarity": 0.75 },
     "clips": [
       { "id": "change-before", "text": "[warm, conversational voice] ..." }
     ]
   }
   ```
   ```sh
   # Draft (no API key required)
   bun scripts/narrate.ts scenes/<name>/narration/script.json --draft

   # Final: Kit's voice on Eleven v4 (requires ELEVENLABS_API_KEY)
   2password run --env 'ELEVENLABS_API_KEY=op://…' -- \
     bun scripts/narrate.ts scenes/<name>/narration/script.json
   ```
   [`scripts/narrate.ts`](https://github.com/kitlangton/psychopomp/blob/main/scripts/narrate.ts) normalizes loudness, transcribes word timings with Whisper,
   and writes `<id>.mp3`, `<id>.words.json`, and `narration.json`. When switching
   voices or engines, rebuild the Scene Plan against the new word timings.

   **Or declare the narration in the Scene Program** with `psychopomp-media`
   ([`SCENE_PLANS.md`](https://github.com/kitlangton/psychopomp/blob/main/SCENE_PLANS.md#declare-narration-and-sound), "Declare Narration And Sound"): `media.say(id, &voice, text)`,
   `media.dialogue`, `media.sfx`, and `audio.derive(Effect::pitch(..))`, then
   `media.finish()?`. Each line is generated once and recorded in
   `media.lock.json`; later runs call nothing unless a declaration changed, and
   ElevenLabs words come from its character alignment with the script's spelling.
   Check the cost first with `PSYCHOPOMP_MEDIA=plan cargo run -p <crate>` (zero
   API calls), time with `PSYCHOPOMP_MEDIA=draft`, and clean up with
   `PSYCHOPOMP_MEDIA=prune`. Move a narrate.ts scene with
   `cargo run -p psychopomp-media -- adopt scenes/<name>/narration` (no
   regeneration). Done when a plain run reports every resource `=`.

4. **Author the Scene Program** in `scenes/<name>` (the workspace picks it up; add
   it to [`verify.json`](https://github.com/kitlangton/psychopomp/blob/main/verify.json) with a few key times).
   Use `psychopomp::score` ([`SCENE_PLANS.md`](https://github.com/kitlangton/psychopomp/blob/main/SCENE_PLANS.md#author-with-the-score-dsl), "Author With The Score DSL") so
   `PlanBuilder` is the only `mut` binding and every actor (`Stage`, `Camera`,
   `Caption`, `Callout`, `RollingNumber`, `Checklist`, `Meter`, `Bars`,
   `Subtitles`, `Confetti`, `Terminal`, `Chat`, `ChangedFiles`, `LowerThird`,
   `Lens`) is an immutable value whose methods return composable `Beat`s
   (`.then`, `.also`, `.with`, `.on_end`, `.after`, `.early`, `stagger`, `each`,
   `at!(scene, time => ...)`). Schedule clips with `Narration::reading`, build
   elements with `StageElement::card|orb|beam|packet|label|ring|rect|icon` and relative
   `psychopomp::layout::Placement` envelopes on `StagePost::RESTRAINED`, frame
   with `Caption::header|chip|footer`, and play sounds with `sfx::*.beat(id, gain_db)`
   (or `audio.beat(gain_db)` for `psychopomp-media`). Do not turn UI, interaction,
   or geometry stories into abstract node diagrams of cards and beams: draw the
   actual control anatomy on Stage (`StageElement::rect` with `width`/`height`
   channels and `dash` hit-box outlines, plus `StageElement::icon` cursor shapes
   `"cursor"`, `"hand-pointing"`, `"cursor-text"`), and pair with `VideoActor` +
   `ReelSegmentPlan::wiped` (`BEFORE` / `AFTER` held wipe) for real screen
   recordings. Load the `explainer-motion`
   skill and apply it to every beat: cards `settle_in`, beams `connect`,
   messages `send`, replies wait for `.reply()`, impacts `hit`, the hero `orb_in`,
   the fix's `rewind`; use `Callout` to pin annotations to Stage elements or
   Editor code ranges and `RollingNumber` for live counters/timers; drive camera
   moves through `stage.camera()` (`establish`, `frame`, `follow`, `release`,
   `orbit`, `whip`, `dolly_zoom`). Reach for the visualization and text overlays
   instead of hand-building them from Stage labels and rings: `Checklist` for
   checks that run and resolve, `Meter` (`MeterPlan::countdown`) for a timeout
   ring, `Bars` for before/after numbers, `Confetti` for a success beat, and
   `Subtitles` (`SubtitlesPlan::from_spoken`) to burn in word-timed captions.
   Changes are `Diff`s of `keep`, `add(step)`, and `remove(step)` lines, each step
   keyed to a phrase. Done when `cargo run -p <crate>` writes the reel and
   `cargo run --release -- plan validate <reel>` reports valid. A missing phrase panics
   with its clip: change the anchor to words the transcript actually contains.

5. **Review before rendering.** Get segment spans from `plan inspect <reel>`, then
   contact-sheet each segment at its before, switch, after, and code beats, and use
   `plan onion` over key motion windows to inspect trajectories, spacing, and
   cursor/shape morphs in one composite image:
   ```sh
   bun scripts/sheet.ts <reel> t1,t2,... --theme neutral --shutter --out output/sheet.jpg
   cargo run --release -- plan onion <reel> from:to:step output/onion.png --theme neutral
   ```
   When tuning the look, set `PSYCHOPOMP_SHADER_DIR=crates/psychopomp-render/src/render`
   and edit [`stage.wgsl`](https://github.com/kitlangton/psychopomp/blob/main/crates/psychopomp-render/src/render/stage.wgsl)/[`stage_post.wgsl`](https://github.com/kitlangton/psychopomp/blob/main/crates/psychopomp-render/src/render/stage_post.wgsl): every frame and sheet picks up shader edits
   without a Rust rebuild.
   Then render one behavior segment with audio (`plan render <reel> out.mp4 --cue <scene-id>`)
   or a 2–3 second window (`--range a..b`; pass `--draft` for a fast single-sample
   timing/audio check before the full-shutter render) and inspect frames extracted
   during motion. Done when every segment has been looked at and no text overlaps,
   clips, or reads against the wrong chip.

6. **Render and verify** the whole reel once, after the sheets and short windows
   pass. Run it in the foreground with a long timeout and never end a turn while it
   is still running:
   ```sh
   cargo run --release -- plan render <reel> output/<name>.mp4 --theme neutral
   ```
   Done when `ffprobe` shows 1920x1080, 60 fps, AAC audio, and a duration matching
   `plan inspect`, and loudness is near -16 LUFS with peaks under -1 dBFS.

7. **Deliver** the MP4 path. Commit the scene, narration assets, and any engine
   changes in Psychopomp with conventional messages.

## Improving Psychopomp

When a reel needs something the engine cannot do, add it to Psychopomp rather than
working around it in one scene: plan types and validation in [`crates/psychopomp`](https://github.com/kitlangton/psychopomp/tree/main/crates/psychopomp),
strict-channel preflight in `plan_runtime`, pixels in `render`, GPU-free tests plus
an `#[ignore]` GPU test, and the docs [`AGENTS.md`](https://github.com/kitlangton/psychopomp/blob/main/AGENTS.md) asks you to keep current. Run
`cargo run --release -- verify baseline` before the change and `verify compare
--expect <scenes you changed>` after it to prove every other showroom and film is
untouched.

Build from small reusable pieces. Interpolation, easing, curves, shape ports, and
connectors belong in `psychopomp::math` (organized like pmndrs `math`: core `lerp`/
`remap`/`smoothstep`, `easing`, `curve`, `shapes`, `random`; glam vectors). Extend it
instead of inlining math in a renderer or scene, and split renderers into one small
helper per element, as [`render/stage.rs`](https://github.com/kitlangton/psychopomp/blob/main/crates/psychopomp-render/src/render/stage.rs) does. Keep
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
and `cargo test --workspace` green.
