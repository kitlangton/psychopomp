# Psychopomp Agent Guide

Psychopomp is a Rust prototype for deterministic, code-first motion graphics:
`wgpu` rendering, `cosmic-text` shaping, temporal sampling for motion blur, and
FFmpeg encoding.

## Read first

- `README.md`: the working example and documentation map.
- `CONTEXT.md`: domain terms. Use them in code and docs.
- `ARCHITECTURE.md`: the module map, boundaries, and intentional non-abstractions.
- `SCENE_PLANS.md`: authoring, presenting, rendering, and every recipe's channels.
- `EFFECTS.md` and `PRIOR_ART.md`: effect implementations and motion references.
- `docs/history/` and `perf/`: past experiments, as evidence rather than policy.
- `.agents/skills/psychopomp` and `.agents/skills/explainer-motion`: the reel
  workflow and motion rules. Load them before authoring a film.

Keep these accurate when a change alters a term, boundary, or finding.

## Commands

```sh
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p <scene-crate>                                   # write a plan or reel
cargo run --release -- plan validate <plan-or-reel.json>
cargo run --release -- plan motion-gate <plan-or-reel.json>
cargo run --release -- plan check <plan-or-reel.json> [--pops] [--audio] --theme neutral
cargo run --release -- plan audio <plan-or-reel.json> [range | --at TIMES]
cargo run --release -- plan strip <plan-or-reel.json> [range|times] out.png [-n 16] --shutter
cargo run --release -- plan render <plan-or-reel.json> output/<name>.mp4 --theme neutral [--draft]
cargo run --release -- plan present <deck.json>              # native presentation
cargo run -p psychopomp-studio -- [--plan <plan-or-reel.json>] # Rune Studio preview + ⌘⇧K knobs
scripts/rune-hot --manifest crates/psychopomp-studio/Cargo.toml --patch src/main.rs
cargo run --release -- plan frame <plan-or-reel.json> <time[,time...]> out.png [--crop x,y,w,h] --shutter
cargo run --release -- plan onion <plan-or-reel.json> <range|times> out.png --shutter
cargo run --release -- plan snapshot <plan-or-reel.json> <times> <dir> [--compare]
cargo run --release -- verify baseline [LABEL | --out DIR]   # every plan in verify.json + key frames
cargo run --release -- verify compare [LABEL | DIR] [--expect NAME,..]
bun scripts/narrate.ts <scene>/narration/script.json [--draft]
```

Rendering needs a working `wgpu` adapter and `ffmpeg` with `libx264`; fonts are
bundled. Do not render full films as routine validation; render when rendering,
typography, choreography, sampling, or encoding changes.

## Architecture

- `crates/psychopomp`: authoring values, Scene Plans, validation, timelines, and
  recipe data; no GPU.
- `crates/psychopomp-render` (binary `psychopomp`): `plan_runtime/` preflights and
  prepares plans, `render/` paints recipes, `exposure.rs` and `encode.rs` deliver video.
- `scenes/`: one Rust Scene Program per film or showroom. The workspace globs
  `scenes/*`, so a new scene needs no `Cargo.toml` edit; list it in `verify.json`.
- A new recipe adds values and an authoring handle in `crates/psychopomp`, strict
  preflight in `plan_runtime/<x>.rs`, pixels in `render/<x>.rs`, and a showroom in
  `scenes/<x>/`.

Do not add a generic scene graph, renderer traits, plugins, or crate splits for
future flexibility. Add an abstraction when a second real use exposes the seam.

## Rules

- Make the smallest change that improves a real scene or answers a stated question.
- Keep choreography visible in its Scene Program; promote an operation into
  `psychopomp` only after repeated use.
- Sampling is a pure function of time. Never integrate state frame by frame.
  Interrupted motion carries both position and velocity.
- Reusable interpolation, easing, and geometry belong in `psychopomp::math`; no
  private lerps or easings in renderers or scenes.
- Code identity and layout stay independent of glyphs, GPUs, colors, and screen
  coordinates.
- A new Stage channel gets its rest value in the one defaults table
  (`StageElement::channel_defaults` or `StagePost::channel_default` in
  `crates/psychopomp/src/stage.rs`), which authoring and the renderer both read;
  `every_stage_property_has_exactly_one_default` fails until it is there.
- Generated plans and media go in ignored `target/` and `output/`.
- Never commit secrets. Narration keys come from the environment.

## Stable code between steps

Viewers must be able to follow the same code through a change.

- Keep stable line and part IDs. Reveal only what changes: for
  `Effect<number>` → `Effect<number, Error>`, reveal `, Error` and keep the rest.
- A retained part may move to make room, but never disappears and reappears.
- Retarget interrupted motion from its current position and velocity; settled
  channels stay settled unless retargeted.
- Verify forward, backward, skipped, and rapid `A → B → A → C` navigation down to
  pixels. Run `psychopomp plan steps <plan.json>` for code presentations.

## Verify

- Add focused unit tests beside pure logic for invariants: identity, endpoints,
  velocity continuity, validation failures, and out-of-order sampling.
- For renderer changes, inspect a targeted artifact at full scale.
- Run `verify baseline` before an engine or scene change and `verify compare`
  after, naming the scenes you meant to change with `--expect`. It re-runs every
  Scene Program in `verify.json`, loads (validates) each plan, compares plan bytes
  with a summary of changed actors and channels, and compares key frames rendered
  on one renderer, in about 15 seconds. Rendering is deterministic, so any changed
  pixel is real. Use `plan snapshot --compare` only for times outside the manifest;
  do not hand-write snapshot loops.
- `verify` builds Scene Programs with the dev profile, as `cargo run -p` does.
  Release and dev builds can differ in a plan's last float digits
  (constant-folded `sin`/`cos`), so emit a baseline and its comparison with the
  same profile; compare refuses a baseline that recorded another.
- A manifest scene's `requires` names ignored inputs, such as generated sprites.
  Without them, `verify` compares that plan but renders none of its frames, and
  says so.
- Compare performance only at identical resolution, frames, samples, and profile.

Before finishing, run the three checks above and `verify compare`, and say if
GPU, font, or FFmpeg limits prevented artifact-level verification.

## Working alongside other agents

- Never end a turn while a background command runs: a subagent is not resumed
  when it finishes. Run builds and renders in the foreground with a long timeout.
- Shell commands run in zsh. An unquoted `$flags` stays one word
  (`"--theme neutral"`), so write flags out or use an array
  (`flags=(--theme neutral)`, then `"${flags[@]}"`), and quote globs
  (`--include='*.rs'`).
- Share one baseline. Before fanning out, run `verify baseline --out <dir>` once
  on the base commit; each agent runs `verify compare <dir>` (any argument with a
  `/` is a directory, otherwise a label under `target/verify/`) instead of
  building the base or rendering its own. Plans store workspace paths as `$ROOT`, so one
  baseline serves every worktree.
- The machine is shared. Check frames with `plan frame`, `plan snapshot`, sheets,
  and short `plan render --range a..b` or `--cue` windows; render a full film
  once, after those pass. Cap compile jobs (`CARGO_BUILD_JOBS=4`) when several
  agents build at once.
- `plan render` waits for one machine-wide render slot and names the PID holding
  it; set `PSYCHOPOMP_RENDER_SLOTS=2` to allow two, or `0` to skip the queue.
  Frames, snapshots, and `verify` never wait.
- Brief subagents from `docs/AGENT_BRIEF.md`.
