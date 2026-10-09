# Psychopomp

Code-first motion graphics in Rust. A small Rust program describes a scene;
Psychopomp renders it as a 1080p60 video with real motion blur, or plays it as
an interactive presentation.

Every frame is a pure function of time, so any frame renders identically in any
order, and interrupted motion keeps its velocity. It is an early, thoroughly
vibe-coded prototype, tested on macOS (Metal).

## What it draws

- **Stage**: a 2.5D camera over particle orbs and forms (cubes, slabs, dot
  matrices, tori that tumble and morph), cards, shapes, icons, arrowed paths,
  travelling packets, labels, and rings, with bloom, depth of field, screen
  shake, zoom streaks, explosions, and a VHS rewind. The camera frames, follows
  packets, racks focus, orbits, dolly-zooms, whips, and sways handheld.
- **Code**: an editor that animates diffs while every line keeps its identity.
- **Overlays**: callouts pinned to anything, rolling numbers, captions, sequence
  diagrams, charts, trees, video cards, footage, checklists, meters, benchmark bars,
  word-timed subtitles, and confetti.
- **Text surfaces**: a terminal, a Slack or iMessage thread, and a pull request's
  changed files, drawn natively so they follow the theme and re-time with the
  scene, plus lower thirds that introduce who or what is on screen.
- **Narration and sound**: optional ElevenLabs or Fish Audio voice-over and
  generated sound effects, declared in the Scene Program and generated once.
  Each beat waits for the word that triggers it, so re-voicing re-times the film.

## Example

```rust
let plan = StagePlan {
    post: Default::default(),
    elements: vec![
        StageElement::card("client", [560.0, 540.0, 0.0], [300.0, 110.0], "client"),
        StageElement::orb("server", [1360.0, 540.0, 0.0], 140.0),
        StageElement::beam("link", "client", "server"),
        StageElement::packet("hello", "link").labeled("GET /hello"),
    ],
};
let mut scene = PlanBuilder::new("hello", 4 * SECOND);
let mut stage = StageActor::declare(&mut scene, "stage", &plan)?;
let ready = stage.settle_in(&mut scene, "client", 0); // the card drifts into place
let wired = stage.connect(&mut scene, "link", ready, 0.6); // the wire draws on
let landed = stage.send(&mut scene, "hello", wired + SECOND / 2, 0.8); // a packet flies
stage.land(&mut scene, "server", landed); // the orb lights up
stage.jolt(&mut scene, landed, [1.0, 0.0], 0.6); // and the camera takes the hit
std::fs::write("target/hello.json", serde_json::to_string_pretty(&scene.finish()?)?)?;
```

The full program is [`scenes/hello`](scenes/hello/src/main.rs). Run it and render
the plan it writes (you need a recent Rust toolchain, a GPU, and FFmpeg with
`libx264`):

```sh
cargo run -p psychopomp-hello
cargo run --release -- plan render target/hello.json output/hello.mp4 --theme neutral
```

For quicker video exports, override the frame rate and shutter samples:

```sh
cargo run --release -- plan render target/hello.json output/preview.mp4 --fps 30 --samples 4 --theme neutral
```

`--fps` and `--samples` take positive integers and apply only to `plan render`,
including reels and `--cue` / `--range` exports. Without them, exports keep 60 fps
and the native sample schedule (24 for Stage scenes, 8 normally or 16 on entrances
for other roots, and at least 16 during reel transitions). `--samples` overrides
that schedule, including transitions; identical visual samples still merge.
Lower values trade temporal smoothness and motion-blur quality for render speed.
The 180-degree export shutter follows the selected fps; procedural shader effects
retain their native calibration. Stills, snapshots, presentation and server
requests are unchanged.

Check frames without encoding a video:

```sh
bun scripts/sheet.ts target/hello.json 0.5,1.5,2.7 --theme neutral --shutter
```

## Examples

| Scene | What it shows |
| --- | --- |
| [`psychopomp-intro`](scenes/psychopomp-intro) | This library introducing itself, loudly |
| [`2password`](scenes/2password) | A narrated product explainer on the Stage |
| [`pr-walkthrough`](scenes/pr-walkthrough) | Pull requests as Stage films that zoom into their diffs |
| [`shape-of-openness`](scenes/shape-of-openness) | A deadpan design film: flat Helvetica, exact SVG artwork, drawn geometry, photographic plates, and British narration |
| [`balls-with-dots`](scenes/balls-with-dots), [`balls-v3`](scenes/balls-v3) | One reply film in two cuts: a whisper about a ball with dots that builds through a chant and a theory into a drum-cut montage of every effect, then one small ball |
| [`camera`](scenes/camera) | A Stage diagram shot like a film: every camera move |
| [`generated-media`](scenes/generated-media) | Speech, a chant, sound effects, and a derived voice, generated once and timed to their words |
| [`callouts`](scenes/callouts), [`rolling-number`](scenes/rolling-number), [`charts`](scenes/charts), [`tree`](scenes/tree), [`diagnostics`](scenes/diagnostics), [`text-surfaces`](scenes/text-surfaces), [`viz-components`](scenes/viz-components), [`transitions`](scenes/transitions) | Component showrooms |
| [`effects-showroom`](scenes/effects-showroom) | Lightning, charge, shields, dissolve, and scans on the Stage |
| [`loupe`](scenes/loupe) | A glass loupe reading code and a Stage card's status |
| [`footage`](scenes/footage) | Collages of stills and clips, retimed footage, and footage inside a Stage |
| [`interactive-showcase`](scenes/interactive-showcase) | A native, steppable presentation (`plan present`) |
| [`stage-forms`](scenes/stage-forms) | Stage diagram vocabulary: particle forms that morph and tumble, shapes, icons, arrows, a relaying packet |

## Use it with a coding agent

The repo ships two skills in [`.agents/skills`](.agents/skills): `psychopomp` (the
reel workflow: facts, script, narration, choreography, review, render) and
`explainer-motion` (how to make diagrams move like physical things). Agents that
read `.agents/skills` pick them up inside this repository. To install them
elsewhere:

```sh
npx skills add kitlangton/psychopomp
```

## How it fits together

```text
crates/psychopomp-media   declared speech and sound, reconciled against media.lock.json
   ↓ exact durations and words
scenes/*              Rust Scene Programs: meaning, timing, choreography
   ↓ Scene Plan (JSON)
crates/psychopomp     plans, validation, timelines, springs; no GPU
   ↓
crates/psychopomp-render   wgpu rendering → native presentation or FFmpeg video
```

| Question | Read |
| --- | --- |
| What do the terms mean? | [CONTEXT.md](CONTEXT.md) |
| Where does a behavior live? | [ARCHITECTURE.md](ARCHITECTURE.md) |
| How do I author, present, or render? | [SCENE_PLANS.md](SCENE_PLANS.md) |
| Which effects exist, and how are they built? | [EFFECTS.md](EFFECTS.md) |
| What inspired the motion? | [PRIOR_ART.md](PRIOR_ART.md) |

## Develop

```sh
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

[AGENTS.md](AGENTS.md) has the engineering and verification rules. Psychopomp is
licensed under the [MIT License](LICENSE). CommitMono is bundled under the SIL
Open Font License (`assets/fonts/OFL.txt`); Phosphor icons under the MIT License
(`assets/icons/LICENSE`).
