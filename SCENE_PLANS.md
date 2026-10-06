# Scene Programs Compile Into Inspectable Plans

Psychopomp keeps Rust as its authoring language. A Scene Program can import libraries, load data, run calculations, use loops, and define helpers. Running that program emits a versioned Scene Plan containing only stable actors, continuous channels, state channels, cues, and media placements.

```text
Rust Scene Program -> Scene Plan -> persistent psychopomp-render process
```

The separation keeps scene compilation lightweight and lets one renderer process retain its GPU device, font caches, and rendering implementation across repeated agent requests.

## Build An Explainer

Start from the closest existing Scene Program and change its content, not its
machinery:

| You are explaining | Start from | Library pieces |
| --- | --- | --- |
| A pull request: broken behavior, the fix, the diff | `scenes/config-migration` (smallest) or `scenes/pr-walkthrough` | `psychopomp_pr_walkthrough::film`, `narration`, `editor::diff`, `sequence` rows |
| A system, as a 3D film of cards, orbs, and packets | `scenes/opencode-jr-architecture`, `scenes/pr-walkthrough/src/flagship.rs` | `stage::StageActor` (`settle_in`, `send`, `hit`, `jolt`, `orb_in`, `rewind`), `StageElement` constructors, `sfx`, `chrome` |
| Lightning, charge, a forcefield, a burn-away, a scan | `scenes/effects-showroom` | `stage::StageActor` (`zap`, `charge`, `hum`, `raise`, `dissolve`, `materialize`, `scan`) |
| A diagram of shapes, icons, arrows, and particle forms | `scenes/stage-forms` | `stage` `form`/`shape`/`path`/`icon`, `StageActor` (`connect`, `relay`, `morph`) |
| Code changing step by step, presented live | `scenes/effect-succeed-slides`, `scenes/interactive-showcase` | `editor` recipes, `PresentationStepPlan` |
| Springs, easing, retargeting, or a metric as curves; a plan's channels over time | `scenes/charts` | `plot::PlotActor` (`draw`, `ride`, `velocity`), `lanes::LanesPlan::from_scene_plan` |
| A payload, config, or emitted plan as structured data | `scenes/tree` | `tree::TreeActor` (`open`, `reveal`, `highlight`, `set`) |
| Pointing at a card or code range while it moves | `scenes/callouts` | `callout::CalloutActor` (`show`, `move_to`, `emphasize`) |
| Labels, counters, or images riding a card or code range | `scenes/anchors` | `anchor::AnchorPlan` on captions, Rolling Numbers, `text::TextActor`, `image::ImageActor` (`move_to`) |
| Magnifying a code range or a card's status | `scenes/loupe` | `lens::LensActor` (`show`, `move_to`, `slide`, `resize`, `focus`) |
| Real product behavior from a screen recording | `scenes/video`, `scenes/opencode-session-tool` | `video::VideoActor` (`fly_in`, `focus`, `unfocus`) |
| Collages of stills and clips; footage frozen, ramped, or placed in a Stage | `scenes/footage` | `footage::FootageActor` (`toss_in`, `treat`, `freeze`, `ramp`, `stutter`, `drift`), `footage::layout`, Stage `footage`, `StageActor::footage_playhead` |
| Talking-head portraits lip-synced to narration; image swaps on the narration clock | `scenes/high-council` | `lipsync::visemes`, `blinks`, `frames`, `SpriteSheet`, `Sprite` (`show`, `perform`) over a frame-cut `footage` image sequence |
| Before and after, side by side | `scenes/compare` | `ReelSegmentPlan::wiped` with `ReelWipePlan` holds and labels |
| Changing scenes: pushes, irises, matched zooms, flips, cuts | `scenes/transitions` | `ReelPlan::new` with `ReelSegmentPlan::pushed`, `matched`, `irised`, `flipped`, ... |
| A version or count changing | `scenes/rolling-number` | `rolling::RollingNumberActor::roll` |
| A CLI session, an agent run, or a build in a terminal | `scenes/text-surfaces` | `terminal::TerminalActor` (`type_command`, `print`, `stream`, `spin`, `resolve`, `clear`) |
| A Slack thread or text conversation reacting | `scenes/text-surfaces` | `chat::ChatActor` (`typing`, `say`, `stream`, `react`, `highlight`) |
| Who is speaking, or what a subject is | `scenes/text-surfaces` | `lower_third::LowerThirdActor` (`show`, `hide`) |
| A pull request's changed files and diffstat | `scenes/text-surfaces` | `changed_files::ChangedFilesActor` (`reveal`, `focus`, `highlight`) |
| Steps or checks running, failing, retrying, passing | `scenes/viz-components` (`checklist`) | `checklist::ChecklistActor` (`reveal`, `start`, `resolve`, `skip`), `confetti::ConfettiActor::burst` |
| A timeout, a load, or progress | `scenes/viz-components` (`meters`) | `meter::MeterActor` (`countdown`, `set`, `sweep`) |
| Before/after numbers from a perf change | `scenes/viz-components` (`bars`) | `bars::BarsActor` (`grow`, `reveal_deltas`, `sort`) |
| Captions for a narrated film | `scenes/viz-components` (`subtitles`) | `subtitles::SubtitlesPlan::from_spoken` |
| A single titled idea | `scenes/agent-demo` | `PlanBuilder` channels and cues |

1. Declare the narration in the Scene Program with `psychopomp-media`
   ([Declare Narration And Sound](#declare-narration-and-sound)): `media.say(..)`
   generates each line once and records it in `media.lock.json`. Schedule lines
   with `Reading::new(lead, [(audio.clip(), gap_after), ..])` (or
   `Narration::reading(lead, [(id, gap_after), ..])` for `narrate.ts` clips): its
   `duration()` sizes the `PlanBuilder` and `place(&mut scene)` returns one
   `Spoken` per clip. Time everything from `spoken.at("phrase")`, so re-voicing
   re-times the film. A clip can be split across segments with
   `clip.split(seconds, earlier, later)` and `place_range`. (`bun
   scripts/narrate.ts` and `Narration::load` still work for scenes not yet moved.)
2. Declare actors with `PlanBuilder`; write motion through typed handles. Time
   literals use `author::SECOND`, `author::seconds(f64)`, and
   `author::millis(u64)`; see [Author with timing helpers](#author-with-timing-helpers).
3. Emit with `ScenePlan::write_or_print`, `DeckPlan::write_with_slides`, or
   `ReelPlan::new(id, segments)` / `ReelPlan::dipped(..)`, then `plan validate`
   and `plan inspect`.
4. Review exact frames before encoding: `bun scripts/sheet.ts <plan> 0:10:0.5
   [--crop x,y,w,h] [--shutter]`, `plan frame <plan> <t> out.png --shutter`, and for
   code steps `plan steps`.
5. Render one cue with audio (`plan render <plan> out.mp4 --cue <id>`), then the
   whole film. When refactoring, `plan snapshot <plan> <times> <dir>` before and
   `--compare` after proves the pixels did not change.

The sections below cover presentation, narrated reels, [every recipe's payload and
channels](#recipe-payloads-and-channels), and the persistent renderer.

## Run The Example

Emit a plan from the lightweight example Scene Program:

```bash
cargo run -p agent-demo -- target/agent-demo.json
```

Emit the canonical editor-heavy hero plan:

```bash
cargo run -p psychopomp-hero -- target/hero.json
```

Emit the narration-rich OpenCode session-tool lesson:

```bash
cargo run -p psychopomp-opencode-session-tool -- \
  scenes/opencode-session-tool/opencode-session-tool.plan.json
```

Emit the minimal Solid Store to Quark before-and-after:

```bash
cargo run -p psychopomp-quark-before-after -- \
  scenes/quark-before-after/quark-before-after.plan.json
```

A Scene Program's `main` is usually one line:
`build_plan()?.write_or_print(std::env::args().nth(1))?` writes the plan to the
given path (or stdout), and `DeckPlan::write_with_slides(path)` writes a deck plus
one `<scene id>.json` per slide beside it.

Inspect or validate the result without initializing a GPU:

```bash
cargo run -- plan schema
cargo run -- plan validate target/agent-demo.json
cargo run -- plan inspect target/agent-demo.json
```

Validation owns recipe decoding, references, exclusive root selection, and
generated-channel collisions before opening fonts, GPU resources, or video caches.
Preparation retains those typed inputs. Native admission is checked separately:
a plan can be valid for export while still using unsupported interactive state or
media. See [the preparation boundary](ARCHITECTURE.md#scene-programs-and-rendering-compile-separately).

Compare two validated plans or render one exact PNG frame:

```bash
cargo run -- plan diff target/before.json target/after.json
cargo run --release -- plan frame target/agent-demo.json 1.25 output/frame.png
cargo run --release -- plan frame target/agent-demo.json 1.25 output/frame.png --shutter  # as exported, with motion blur
cargo run --release -- plan snapshot target/agent-demo.json 0:4:0.5 output/snap           # write frames
cargo run --release -- plan snapshot target/agent-demo.json 0:4:0.5 output/snap --compare # fail if any pixel changed
```

Render only one cue or exact range on the original scene clock:

```bash
cargo run --release -- plan render target/agent-demo.json output/intro.mp4 --cue intro
cargo run --release -- plan render target/agent-demo.json output/window.mp4 --range 0.2..0.4
cargo run --release -- plan render target/agent-demo.json output/preview.mp4 --range 0.2..0.4 --fps 30 --samples 4
```

`plan render` accepts optional positive-integer `--fps` and `--samples` overrides,
for both scenes and reels. Omit them to keep 60 fps and the native per-recipe
sample schedule. `--samples` replaces that schedule even during reel transitions;
identical visual samples still merge. Lower settings reduce temporal smoothness
and motion-blur quality in exchange for faster rendering. The 180-degree export
shutter follows fps; procedural shader effects retain their native calibration.
Frame, snapshot, presentation and server commands keep their existing behavior.

A Render Window trims and rebases intersecting media to output time zero, but visual sampling remains on the original global scene clock. Starting a window in the middle of a spring therefore preserves its position and velocity.

Delivery remains frame-based: a window whose duration is not exactly frame-aligned emits one final frame sampled only within the remaining window interval. At the selected fps (60 by default), the encoded duration therefore rounds up to the next frame boundary.

## Play As A Presentation

The themed slideshow-component showroom runs natively:

```sh
cargo run -p psychopomp-component-prototypes -- --slideshow
cargo run --release -- plan present target/slideshow-components/deck.json
```

Use **1–8** for rich text, lists/quotes, header entrances, width-revealing type
expressions, Venn diagrams, full-divider tables, an existing composition, and
reflection/stagger header variants. Slide **3** retains the original rise trial;
slide **8** compares a mirrored rise, 60 ms word offsets, and a quicker reflected
variant with 25 ms offsets. Left/Right reverses these entrances without orphaned
delayed words. An unchanged hold does not replay the header.
**T / Shift+T** cycles Original, Evergreen, Tokyo Night, and Pure Black in either
direction. The choice is saved immediately in
`$XDG_CONFIG_HOME/psychopomp/preferences.json`, or
`~/.config/psychopomp/preferences.json` when XDG is unset. Malformed preferences
produce a warning, not a crash or silent overwrite. The window title names the
active theme. Held/paused frames repaint without retargeting or advancing motion.
**C** remains a temporary grid-line audition; changing theme restores its accent.

`--theme original|evergreen|tokyo-night|black` overrides the starting presentation
theme without saving it until T is used. File export ignores personal preferences
and accepts the same explicit option:

```sh
cargo run --release -- plan frame target/slideshow-components/rich-text-showcase.json 8 output/rich.png --theme tokyo-night
cargo run --release -- plan render target/slideshow-components/venn-showcase.json output/venn.mp4 --range 3..4.2 --theme evergreen
```

Headers can fade/deblur, reveal across their measured width, or rise through a
stationary clip. Separate Presentation Steps provide deliberate header-only
holds; there are no callbacks that might fire after a skipped or reversed step.

### Inspect Motion Slowly

- **S / Shift+S:** cycle forward/backward through **1x / 0.5x / 0.25x / 0.1x**.
- **P:** pause/resume without changing the selected destination.
- **. / ,:** step forward/backward by **16.667 ms of scene time**, then remain paused.
  Backward inspection stops at the current navigation boundary; arrows still
  animate between step destinations. Frame-stepping is disabled in reduced motion.
- **Shift+R:** replay the current step and pause immediately at its entry pose.
- **D:** toggle the debug HUD: local time, time since navigation, current speed,
  pending starts/next due time, and per-word header spring progress.

For the stagger investigation:

```sh
cargo run --release -- plan present target/slideshow-components/header-variations.json --speed 0.25 --debug
```

Press **Right** once to choose the entrance step, then **Shift+R** and tap **.**
to inspect it from the beginning, or **P** to watch at quarter speed. The 60 ms
word gaps are overlapping starts, not one word finishing before the next. Rapid
reversals redirect already-moving words immediately; Replay resets them to rest
and replays the original stagger. The 25 ms quick variant deliberately overlaps
even more. No choreography timing is changed by these debug controls.

Slow motion keeps the normal display sampling cadence; it does not lower FPS.
Speed/debug are not persisted and do not affect exports. `--benchmark` and
`--benchmark-gpu` reject non-normal speed or an enabled debug HUD.

Compare plain and row-banded tables with unfilled and original 3D volumes:

```bash
cargo run -p psychopomp-keyed-grid -- --styles
cargo run --release -- plan present target/grid-styles/deck.json
```

These four scenes share `keyed-grid`; `GridStylePlan::plain_table` adds table
placement and paint, not another playback engine. Use **1–4** to select a treatment.

The reusable-component visual trials run with:

```bash
bash scenes/component-prototypes/run.sh
```

Use **1–4** to compare Typeset, Collection, Connector, and their composition.
These are explicitly provisional native recipes; see the showroom's README for
limits and the pending aesthetic verdict. This does not change the lesson deck.

The seven-slide functional-data-modeling adaptation includes the opening
types/cardinality sequence, Boolean ↔ Toggle, joystick representation fit, OR,
AND, and an illegal-state code edit:

```bash
cargo run -p psychopomp-data-modeling
cargo run --release -- plan present target/data-modeling/deck.json
cargo run -- plan steps target/data-modeling/illegal-states.json
```

It adds a small `value-token` overlay recipe with ordinary continuous channels;
no generic State Channel or recorded-media playback support is implied. See
`scenes/data-modeling/README.md` for source fidelity, controls, and counting limits.

Scene Plan v2 has optional `presentationSteps` metadata. Omitting it preserves
existing serialized plans and video behavior. Steps are separate from cues: each
has a stable ID, title, entry start, and exact held endpoint. They are ordered and
non-overlapping, and may be still-only (`startNanos == holdNanos`). Authors must
choose meaningful hold times; validation checks timing, not visual settling.

```rust
scene.presentation_step("initial", "Start with a value", 0, 0);
scene.presentation_step("reveal", "Reveal the type", 1_000_000_000, 2_500_000_000);
```

A deck of evenly spaced steps is `scene.steps("step", titles, 3 * SECOND, 2 *
SECOND)`: step `i` enters at `i × 3 s` and holds 2 s later (the first is a still
at zero); it returns the entry times. `scene.cue_steps(3 * SECOND)` adds a Cue
per step so `--cue step-2` renders one.

Build the Effect Institute `effect-succeed` adaptation:

```bash
cargo run -p psychopomp-effect-succeed-slides -- target/effect-succeed-slides.json
cargo run --release -- plan present target/effect-succeed-slides.json
```

This opens a native Rust window and samples the prepared scene directly, without
exporting clips. Left/Right animate toward the previous/next held pose; another
keypress immediately redirects the current springs without dropping velocity.
R explicitly restarts from the current step's entry pose. Space/P pause or resume,
Home/End select first/last, F toggles full screen, and Escape closes the window.

X toggles smooth/pixelated scaling; M toggles reduced motion. `--reduced-motion`
starts without transition motion, and `--full-quality` disables the fast flat-editor
preview profile. The default profile caches unchanged chrome and omits final
optical resampling of code glyphs, while preserving the same motion tracks. Resizing
scales the authored canvas rather than reflowing it.

Fractional glyph sampling, reveal-edge coverage, and continuous blur happen before
window scaling. Dynamic focus/highlight overlays reuse the WGSL recipe without
rebuilding static chrome through the optical compositor. Semantic coordinates
follow sampled visible widths and line positions; literal coordinates remain literal.

Final window scaling is GPU-backed with an sRGB texture and linear/nearest
sampling. `--benchmark` runs a ten-second native interruption sequence and prints
frame-submission timing; it keeps its window on top and exits automatically.

Native pacing follows the current monitor's reported refresh rate (including
120/144 Hz), falling back to 60 Hz when unavailable. `--fps 120` overrides the
sampling cap without changing video FPS or disabling FIFO synchronization. A
60 Hz display still cannot show 120 distinct frames/sec. `--benchmark-gpu` adds
explicit GPU-completion waits to the benchmark and separates completed rendering
work from waiting for a drawable; it is a diagnostic mode, not normal playback.

The interruptible player accepts any plan driven only by Continuous Channels:
Task, grid, editor, and Tree snapshots lower into continuous visual tracks.
Numbers (including Changed Files totals that roll) are still rejected until their interactive timing is defined. The same Scene Plan still exports as MP4
Plans with generic State Channels, media (including Video Cards), Rolling
Numbers, or Subtitles are still rejected until their interactive timing is defined. The same Scene Plan still exports as MP4
through `plan render`, with its original timing and media placements. Live source
reloading, native higher-DPI glyph rasterization, and presentation audio remain open.

### Present A Deck

```bash
cargo run -p psychopomp-interactive-showcase
cargo run --release -- plan present target/interactive-showcase/deck.json
```

The four-slide demo includes the original code reveal, Effect Task lifecycle/retry,
parallel Tasks, and keyed code insertion/removal with an attached highlight.
`DeckPlan` v1 contains an ID and titled `SlidePlan` values, each embedding an
independent Scene Plan. Single-plan presentation remains supported.

- **⌘→ / ⌘←:** next/previous slide, wrapping around (`'` / Shift+`'` remain aliases).
- **1–9:** jump directly to a slide.
- **Left/Right:** previous/next step within the active slide.
- **Home/End:** first/last step within that slide.
- **C / Shift+C:** next/previous grid line color on grid slides (preview-only).
- **T / Shift+T:** next/previous presentation theme on every slide (saved).
- **S / Shift+S:** playback speed; **D:** debug HUD; **, / .:** paused frame inspection;
  **Shift+R:** replay paused at the current entry pose.
- **Space/P, R, M, X, F, Escape:** pause, replay, reduced motion, filtering,
  fullscreen, and close, as in single-plan presentation.

Inactive slides retain their step and freeze their local clock. Returning resumes
only motion that was running when the slide was left; explicit pause stays paused.
Running Tasks keep animating after their dimensions settle, until paused or advanced.
Reduced motion freezes their ambient animation too. The showcase program emits
individual slide JSON files beside the deck for `plan frame`, `plan render`, and
`plan steps`; export those Scene Plans, not the deck wrapper.

For a deck, `--benchmark` also switches slides every two measured seconds. Those
results include slide-switch costs and are not directly comparable with a
single-scene throughput benchmark.

### Present A Growing 3D Grid

```sh
cargo run -p psychopomp-keyed-grid
cargo run --release -- plan present target/keyed-grid/deck.json
```

This two-slide proof grows a connected row/table/3D line lattice, then regroups
the same 24 tuples as `(A × B) × C` and `A × (B × C)`. Straight-on and angled
orthographic views share identical 3D geometry. A camera-only step reveals
existing depth; separate extent tracks grow the grid from its fixed leading
corner without scaling individual cells. The projection centers the currently
visible geometry through growth and rotation. Opaque cells hide rear lines;
slice focus reveals one layer through continuous cutaway tracks. Optional
`GridCellLabelPlan` values provide symbols and secondary text, with row, column,
and depth headings drawn along the sides. Existing navigation,
pause, replay, and reduced motion apply to cell motion and camera angles alike.
Labels use the selected growth-edge disclosure: their feather follows the sampled
X/Y/depth extent rather than simultaneous per-label wipes. C cycles Orange, Muted
copper, Slate blue, Sage, and Chalk without changing the current motion. The
palette is a native preview preference, not a Scene Plan or export mutation.
`scenes/keyed-grid/README.md` explains the slice semantics and export commands.

`GridRecipePlan` contains three immutable `GridAxisPlan` values, an initial
`GridSnapshotPlan`, and timed snapshots. The concrete recipe caps the catalog at
256 cells and supports one root grid with text/Task overlays. Cells are keyed by
their indices in that immutable catalog; no automatic matching occurs. Do not
author in the generated `__grid.*` channel namespace. No mesh import, picking,
free orbit, or general-purpose correspondence API is exposed by this proof.

### Mask Rolling Text

The `text` recipe accepts an optional canvas-space `verticalMask`:

```json
{"text":"Run the computation","center":[960,780],"fontSize":30,
 "verticalMask":{"top":750,"bottom":810,"fade":12}}
```

The mask stays fixed while the actor's `y` channel moves the text through it.
Coverage ramps linearly from zero at `top` to full opacity at `top + fade`, stays
full in the middle, and falls to zero at `bottom`. Everything outside is clipped.
`fade` may be zero for a hard aperture, but cannot exceed half its height.
`plan validate` checks finite ordered bounds and the fade range without a GPU.

This is a text alpha mask, not a dark rectangle composited over the scene.
Other actors and the background remain unchanged. The showcase captions use a
shared 60-pixel aperture and 12-pixel edge fades, following `visual-types`' rolling
content treatment. Existing opacity channels still prevent skipped, unselected
captions from appearing while their y destinations change.

### Inspect Maximum Stability

```bash
cargo run -- plan steps target/effect-succeed-slides.json
```

No GPU is initialized. Each editor step reports before/after text, `beforeDelta`,
`delta`, changed part IDs, retained-line movement, and before/after y positions.
`«…»` marks changed parts, not unchanged text displaced by neighboring layout.
Warnings identify common text inside exchanged ranges or replaced lines, and
held endpoints that still contain moving or partially visible code. Warnings are
heuristics: they do not merge semantically different IDs. For partial holds,
reported text describes participating parts, not the exact clipped glyphs.
The persistent server accepts `{"id":1,"command":"steps","plan":"path.json"}`.

### Schedule Several Line-Order Changes

Leave `EditorRecipePlan.snapshots` empty for the original shared `layout`/`content`
placement. For keyed edits, declare every possible line once, retain the opening
`initial_line_ids`, and schedule later orders:

```rust
recipe.snapshots = vec![
    EditorSnapshotPlan {
        at_nanos: 1_000_000_000,
        line_ids: vec!["import".into(), "helper".into(), "definition".into()],
    },
    EditorSnapshotPlan {
        at_nanos: 3_000_000_000,
        line_ids: vec!["import".into(), "definition".into()],
    },
];
```

The last order must equal `final_line_ids`. A line may be absent from both initial
and final orders but present between them. Preparation derives ordinary per-line
y/opacity tracks with the 0.45-second zero-bounce profile. Equal-time snapshots
coalesce before computing layout; removed lines fade in place. Generated tracks
replace legacy shared placement in this mode; Inline Reveals remain independent.
Use presentation holds after settling (two seconds after a change is ample for
these examples). `plan validate` checks recipe ranges, snapshot timing, and generated
channel collisions without a GPU.

Old JSON plans need no new fields. Rust recipe literals use `snapshots: Vec::new()`
to retain the old behavior. Do not author in the generated `line.<id>.y`,
`line.<id>.opacity`, or `__attachment-*` namespaces.

## Declare Narration And Sound

`psychopomp-media` makes generated audio part of the Scene Program. Declare
what you want; the first run generates it, every run times the choreography to
the real words, and later runs call nothing until a declaration changes:

```rust
use psychopomp_media::{Effect, FISH_KIT, Line, Media, Sound, Voice};

let media = Media::open(env!("CARGO_MANIFEST_DIR"))?; // media.lock.json + media/
let kit = Voice::eleven(KIT).v4().stability(0.2).similarity(0.65);
let guest = Voice::eleven(GUEST).v4().stability(0.2).similarity(0.65);
let intro = media.say("intro", &kit, "[warm, conversational] When a client reconnects...")?;
let fix = media.say("fix", &kit, Line::new("[relieved] Nothing gets killed.").after(&intro))?;
let chant = media.say("chant", &Voice::fish(FISH_KIT), "Balls! Balls! BALLS!")?;
let duet = media.dialogue("duet", [(&kit, "You first."), (&guest, "No, you.")])?;
let pop = media.sfx("pop", "a single soft glassy pop, tiny and dry", seconds(0.5))?;
let bed = media.sfx("bed", Sound::new("light rain on a tin roof").looping(), seconds(8.0))?;
let demon = intro.derive(Effect::pitch(-6.0))?; // id "intro.pitch(-6)"
media.finish()?; // reports orphans; fails an offline plan that found missing audio

let said = intro.place(&mut scene, SECOND); // Script Clip "narration-intro"
for at in said.at_every("balls") { pop.play(&mut scene, at, -18.0); } // Layer Clips
scene.cue("intro-end", said.end(), said.end());
```

- **Voices.** `Voice::eleven(id)` uses `eleven_v4`; `.stability`, `.similarity`,
  `.seed`, `.language`, `.ivc()` (`use_pvc_as_ivc`), and `.whisper()` (time with
  Whisper instead of ElevenLabs' character alignment). v4 has no speed or style:
  direct performance with bracketed tags in the text, and `/IPA/` for
  pronunciation. `Voice::fish(id)` uses `s2.1-pro-free` with `.speed`;
  `Voice::say(name)` is a free macOS voice.
- **Lines.** `media.say` is one Text to Speech request; `Line::new(text).after(&previous)`
  stitches it to an earlier ElevenLabs line with `previous_request_ids` (the
  predecessor's key joins this line's, so re-voicing it re-voices this one).
  `media.dialogue` is one Text to Dialogue performance of several voices that
  share one model, settings, and alignment (2,000 characters at most).
- **Sound effects.** `media.sfx(id, prompt, duration)` uses
  `eleven_text_to_sound_v2` (0.5 to 30 s; `Sound::new(..).influence(x).looping()`);
  the result is trimmed to its onset, peak-matched to -6 dBFS, and faded, so
  `play` at the moment of contact.
- **Derived audio.** `audio.derive(Effect::pitch | tempo | reverse | trim | gain)`
  runs ffmpeg and moves the words with the audio. Pitch shifts formants too.
- **Placement.** `audio.place(scene, at)` returns `Spoken` (`at`, `at_after`,
  `at_any`, `at_every`, `words`, `end`); `audio.play(scene, at, gain_db)` adds a Layer Clip
  `<id>@<time>`. `duration()` is exact, so a reel's length is known before the
  scene exists.

`PSYCHOPOMP_MEDIA` selects the mode:

```sh
PSYCHOPOMP_MEDIA=plan cargo run -p <scene>   # print the delta (+ ~ = -) and cost; zero API calls
cargo run -p <scene>                         # generate what is missing or changed
PSYCHOPOMP_MEDIA=draft cargo run -p <scene>  # time it with macOS say and silent sfx first
PSYCHOPOMP_MEDIA=prune cargo run -p <scene>  # zero calls; delete orphans and superseded files
cargo run -p psychopomp-media -- show scenes/<scene>
```

Credentials come from `ELEVENLABS_API_KEY` and `FISH_AUDIO_API_KEY`, or the
nearest `.env` above the scene; only a run that generates needs them. Commit
`media.lock.json` and `media/` with the scene. Media paths are relative to the
scene directory, so write the plan there. Each id names one role: give a new
line a new id, keep the id when rewording a line, and prune orphans when done.

To move a scene from `scripts/narrate.ts`, run
`cargo run -p psychopomp-media -- adopt scenes/<scene>/narration`, then declare
each clip as narrate.ts made it, one-voice dialogue timed by Whisper, with the
same text: `media.dialogue(id, [(&Voice::eleven(V).stability(s).similarity(m).whisper(), TEXT)])`
(Fish clips: `media.say(id, &Voice::fish(V), TEXT)`). Adopted lines report `=`
and keep their files; `scenes/psychopomp-intro` is the worked example, and its
reel stayed byte-identical.

## Make A Narrated Explainer Reel

`scenes/pr-walkthrough` walks through five pull requests: for each, a Sequence
Diagram plays the broken behavior and replays the fix in the same slots, then an
editor animates the actual change as a diff with Line Marks. The workflow is
reusable for any code explainer:

- `psychopomp_media::Media` declares narration ([above](#declare-narration-and-sound));
  for scenes still voiced by `scripts/narrate.ts`,
  `psychopomp::narration::Narration::load(dir)` reads `narration.json`. Either way
  `place(&mut scene, start)` adds the Script Clip and returns a `Spoken` whose
  `at(phrase)`, `at_any`, `at_after`, and `at_every` give plan-clock times,
  and `words()` every word with its start and end.
- `psychopomp::editor::diff::Diff` of `keep`/`add(step, ..)`/`remove(step, ..)`
  lines declares the stepped editor; `declare(scene, step_times, warning, entrance)`.
  A hand-built editor recipe (one with semantic ranges to pin a callout, say)
  gets the same room-opening steps from `diff::step_snapshots(previous, next,
  at)` plus `diff::gap_lines(&snapshots)`.
- A diff line can name a range of its text (`keep("..").range("sigkill",
  "signal(..)")`), carry an Inlay Hint after one (`.inlay(id, after, text)`), and
  override its Line Mark (`.marked(LineMarkPlan::Added)`). `declare` returns a
  `DiffEditor`: `target(scene, id, line_index, range)` declares a Semantic Target
  for callouts, diagnostics, hovers, and cursors, and `inlay(scene, id)` reveals a
  hint. `film::code_with(pr, narration, change, entrance, |scene, editor, spoken|
  ..)` annotates a PR film's code segment (see `stop_stage.rs`).
- `SequenceRowPlan::message|reply|note|end(..)` with `.in_slot(n)` and
  `.with_aside(text)` build rows; `SequenceParticipantPlan::new(id, label, detail)`
  builds participants; `SequenceActor::row_channel`/`participant_channel` address
  their channels.
- `ReelPlan::dipped(id, plans, transition_nanos)` joins segments with dips.
- `psychopomp::chrome` places a film's fixed captions: `header(scene, label,
  title)` top left, `chip(scene, id, dot, text)` top right, and `footer(scene,
  id, spans)` bottom left; each returns its Caption to type in, show, or hide.
- `psychopomp_pr_walkthrough::film` is the PR-film template itself (`header`
  for a `Pr`, `behavior`, `code`); `scenes/config-migration` reuses it.

```sh
# 1. Voice the script (Fish Audio via 1Password; --draft uses macOS `say`).
2password run --env 'FISH_AUDIO_API_KEY=op://…' -- \
  bun scripts/narrate.ts scenes/pr-walkthrough/narration/script.json
# 2. Emit the reel; phrase lookups fail loudly if narration changed.
cargo run -p psychopomp-pr-walkthrough
cargo run --release -- plan validate scenes/pr-walkthrough/pr-walkthrough.reel.json
cargo run --release -- plan inspect scenes/pr-walkthrough/pr-walkthrough.reel.json
# 3. Review exact frames, then one segment with audio, then everything.
bun scripts/sheet.ts scenes/pr-walkthrough/pr-walkthrough.reel.json 4,30,60 --theme opencode
cargo run --release -- plan render scenes/pr-walkthrough/pr-walkthrough.reel.json output/errors.mp4 --cue errors-behavior --theme opencode
cargo run --release -- plan render scenes/pr-walkthrough/pr-walkthrough.reel.json output/pr-walkthrough.mp4 --theme opencode
```

`narrate.ts` regenerates only clips whose text, voice, engine, or settings changed and writes
`narration.json` with exact decoded durations. Draft and final clips share file
names, so switching voices re-times the reel without touching the Scene Program.

For ElevenLabs v4, use `"engine": "elevenlabs"`, `"model": "eleven_v4"`, the
voice ID, and optional `"settings": { "stability": 0.5, "similarity": 0.7 }` in
the script. Inject `ELEVENLABS_API_KEY`; `speed` is unsupported. Directions in
square brackets guide performance; Whisper timestamps the resulting speech.
The script records model/request IDs and checkpoints completed clips. Clips are
normalized to -16 LUFS; the default evens the level within each clip, while
`"loudness": "linear"` applies one gain per clip and limits peaks, so a whisper
that builds to a scream keeps its swell (`scenes/balls-with-dots`). The
flagship's directed example is `scenes/pr-walkthrough/narration-v4/script.json`.
Copy it under `output/` before generating to keep alternate audio there:

```sh
mkdir -p output/eleven-v4/narration
cp scenes/pr-walkthrough/narration-v4/script.json output/eleven-v4/narration/script.json
2password run --env 'ELEVENLABS_API_KEY=op://…' -- bun scripts/narrate.ts output/eleven-v4/narration/script.json
cargo run -p psychopomp-pr-walkthrough -- pr-50825 --narration output/eleven-v4/narration --output output/eleven-v4/reel.json
```

The selected reel is built lazily, so a flagship-only script needs only its three
clips. Alternate exports resolve audio against the selected narration directory
and original scene assets. The explicit `sig term`/`sigterm` cue alternatives
handle ASR word segmentation without changing recorded timings. Rebuild and
review the new clock before rendering; replacing just the audio desynchronizes it.

A reel is `{ "version": 1, "id", "segments": [{ "transitionNanos", "transitionStyle", "transitionFocus"?, "transitionWipe"?, "plan" }] }`.
`transitionStyle` is a name (`"crossfade"`, `"dip"`, `"zoom"`, `"wipe"`, `"j-cut"`,
`"l-cut"`, `"ink"`, `"glitch"`, `"flash"`, `"light-leak"`) or, for styles with a
setting, a one-key object; see [Transitions](#transitions).
A `zoom` needs `transitionFocus: [x, y, width, height]` in the outgoing frame; compute
it with `stage::Camera::project_rect(card_at, card_size)` at the closing camera
(or `CameraRig::screen_box`) so it matches the card the camera flies into.
Relative media paths resolve against the reel file. Prefer `dip` between frames
that are both dense with text; a crossfade between two editors turns both unreadable.

A `wipe` sweeps a divider across with the incoming segment behind it. Its optional
`transitionWipe` is `{ "direction"?: "right" | "left" | "down" | "up", "holds"?:
[{ "position", "holdNanos" }], "labels"?: [outgoing, incoming] }`: the divider
travels in `direction` (`left` leaves the outgoing frame on the left, the usual
before/after order), eases minimum-jerk into each hold `position` (0..1 of its
travel), rests there for `holdNanos`, and sweeps on. Holds must leave time to sweep
within `transitionNanos`. During a hold both segments keep running on their own
clocks, so author the outgoing segment's tail and the incoming segment's head as
still frames. A held wipe suits frames that compare spatially (the same diagram
with different status, two looks of one layout); halves of the same code lines
read poorly side by side, so prefer a Stepped Diff for code.

```rust
ReelSegmentPlan::wiped(after, seconds(4.4), ReelWipePlan::new(WipeDirection::Left)
    .hold(0.5, seconds(2.6))
    .labeled("BEFORE", "AFTER"))
```

The showroom is `cargo run -p psychopomp-compare` (writes `target/compare.json`):
a held before/after between two Stage frames, then a plain downward wipe.

### Transitions

Every transition has a `ReelSegmentPlan` constructor; `ReelPlan::new(id,
segments)` builds and validates the reel. Each constructor takes the incoming
plan and the overlap in nanoseconds; at most two segments are ever visible.

| Constructor | `transitionStyle` | What happens |
| --- | --- | --- |
| `cut(plan)` | `"crossfade"`, 0 ns | A hard cut |
| `j_cut(plan, lead)` / `l_cut(plan, tail)` | `"j-cut"` / `"l-cut"` | The incoming sound leads the picture cut, or the outgoing sound trails it |
| `crossfaded` / `dipped` | `"crossfade"` / `"dip"` | Mix, or fade through the background |
| `zoomed(plan, ns, focus)` | `"zoom"` | Fly into `focus` while the segment grows out of it |
| `wiped(plan, ns, wipe)` | `"wipe"` | A divider sweeps across, optionally resting with labels |
| `pushed(plan, ns, direction)` | `{ "push": "left" }` | Both frames travel together, motion-blurred |
| `slid(plan, ns, direction)` | `{ "slide": "up" }` | The segment slides over the dimming outgoing frame and settles |
| `whipped(plan, ns, direction)` | `{ "whip": "right" }` | A whip pan: lean in, tear across in a streak, catch |
| `irised(plan, ns, ring)` | `{ "iris": { "ring": true } }` | A soft circle opens from the focus center or the frame's |
| `matched(plan, ns, from, to)` | `{ "match": [x, y, w, h] }` | `from` (the focus) in the outgoing frame flies onto `to` in this one, carried as a card with rounded corners |
| `matched_round(plan, ns, from, to)` | `{ "match-round": [x, y, w, h] }` | A match whose element is round (a ball, an orb, a dot): the ellipse inside each rectangle is carried, so a square opens as a circle |
| `flipped(plan, ns, direction)` | `{ "flip": "left" }` | The frame turns over like a card, this segment on its back |
| `cubed(plan, ns, direction)` | `{ "cube": "up" }` | The frames are faces of a turning cube |
| `inked(plan, ns)` | `"ink"` | The segment spreads in like ink, from the focus if set |
| `glitched` / `flashed` / `leaked` | `"glitch"` / `"flash"` / `"light-leak"` | Corruption, a white-out, or a warm light leak hides a cut |

Directions are `"left"`, `"right"`, `"up"`, and `"down"`: the way the motion
travels. `.focused(rect)` sets `transitionFocus` for an iris or ink origin;
`zoom` and `match` require it, and other new styles reject it. Rectangles are
`[x, y, width, height]` in canvas pixels; a Stage card under the default camera
is `[x - w / 2, y - h / 2, w, h]` from its `at` and `size`, and under a moved
camera use `stage::Camera::project`. A match moves one camera for both frames, so the
element lands exactly on its counterpart; give its target frame the element
already at rest at time zero.

```rust
ReelPlan::new("film", vec![
    ReelSegmentPlan::cut(overview),
    ReelSegmentPlan::matched(detail, seconds(1.3), api_card, api_card_large),
    ReelSegmentPlan::pushed(code, seconds(0.75), WipeDirection::Left),
    ReelSegmentPlan::irised(title, seconds(1.1), true).focused(orb_rect),
])?
```

Push, slide, and whip suit frames that sit side by side in one space; a match
or zoom suits a detail opening into its own scene; a flip suits a before and
after of the same thing; ink, flashes, and leaks suit section breaks; a glitch
suits failure. Keep glitches and flashes short (0.4 to 0.7 seconds).

The showroom is `cargo run -p psychopomp-transitions` (writes
`target/transitions.json`), every transition between Stage, code, and title
frames, each naming itself in a chip:

```sh
cargo run -p psychopomp-transitions
bun scripts/sheet.ts target/transitions.json 4.35:5.55:0.15 --shutter --theme neutral
cargo run --release -- plan render target/transitions.json output/transitions.mp4 --theme neutral
```

`scenes/pr-walkthrough` also emits `pr-50825.reel.json`, a Stage film of #50825
that zooms from the client card into its code:

```sh
cargo run -p psychopomp-pr-walkthrough pr-50825
PSYCHOPOMP_SHADER_DIR=crates/psychopomp-render/src/render \
  bun scripts/sheet.ts scenes/pr-walkthrough/pr-50825.reel.json 2,13,19,46 --theme neutral
cargo run --release -- plan render scenes/pr-walkthrough/pr-50825.reel.json output/pr-50825.mp4 --theme neutral
```

## Recipe Payloads And Channels

Every renderer recipe, by actor `recipe` name. The first two bullets point to
their fuller documentation elsewhere.

- `title-card` (root, `{ title }`), `text` (`text`, `center`, `fontSize`, and options
  such as [`verticalMask`](#mask-rolling-text)), `editor` (root, `EditorRecipePlan`) with an optional
  attached `pointer` (`PointerRecipePlan`), `effect-task` (`TaskRecipePlan`),
  `keyed-grid` (root, `GridRecipePlan`), and `value-token` (`ValueTokenPlan`).
- The provisional `prototype-typeset`, `prototype-width-text`,
  `prototype-collection`, `prototype-connector`, `prototype-rich-text`,
  `prototype-header`, and `prototype-venn` overlays (`psychopomp::component_prototype`);
  see `scenes/component-prototypes/README.md`.
- `sequence`: `participants` (`id`, `label`, `detail`) and `rows` of `kind`
  `message` (`from`, `to`, `label`, `tone`, `reply`), `note` (`over`, `text`), and
  `end` (`participant`, `label`), each with optional `slot` and `aside`. Channels:
  `opacity`, `x`, `y`, `lifelines`, `participant.<id>.opacity|emphasis`,
  `row.<id>.reveal|opacity|strike`. Use `SequenceActor` to write reveals by row.
- `caption`: `origin`, `align`, `size`, `lines` of `{ text, tone }` spans, `chip`,
  and optional [`anchors`](#pin-overlays-to-anchors).
  Channels: `opacity`, `x`, `y`, `typed`, `caret`, `anchor.<id>`. `CaptionActor::type_in` writes
  one exact step per character; `show` and `hide` fade; `move_to` glides between anchors.
  and `glass` (the chip is a frosted liquid-glass pane that refracts the scene
  behind it and condenses in with `opacity`; `CaptionPlan::glass()`).
- `rolling-number`: `origin` (aligned edge x, center y), `align`, `size`, `bold`,
  `tone`, static `prefix`/`suffix` spans (`{ text, tone }`), the initial `value`,
  and `rolls` of `{ atNanos, value }` in increasing time. Optional
  `durationNanos` (500 ms), `stagger` (`outward` | `start` | `end` | `none`),
  `direction` (`auto` | `up` | `down`), `blur` (smear strength, 1; 0 disables),
  `chip`, and [`anchors`](#pin-overlays-to-anchors). Channels: `opacity`, `x`,
  `y`, `anchor.<id>`. Digits roll; `,` between digits
  groups and `.` between digits starts a fraction, so `rc.` and `/` are literals.
  `RollingNumberActor::roll` appends a change at a phrase's time; `show`/`hide`
  fade like a caption. Plans using it are export-only (not `plan present`).
  ```rust
  let mut version = RollingNumberActor::declare(&mut scene, "version",
      RollingNumberPlan::new([960.0, 300.0], 72.0, "rc.112")
          .aligned(CaptionAlign::Center).tone(Tone::Accent)
          .prefix(vec![CaptionSpanPlan::new("opencode ", Tone::Muted)]).chip())?;
  version.show(&mut scene, at);
  version.roll(&mut scene, phrase_start, "rc.117")?;
  ```
  The showroom is `cargo run -p psychopomp-rolling-number` (writes
  `target/rolling-number.json`); render it with
  `cargo run --release -- plan render target/rolling-number.json output/rolling-number.mp4 --theme opencode`.
- `tree`: `origin` (top-left of the first row), `width` (highlight extent and
  truncation), `size` (24), `maxRows` (a scrolling window), `indent` (2 columns),
  the JSON `value`, `expanded` (JSONPaths open at time zero), and `changes` of
  `{ path, value }` (later scalar values, in order per path). Rows are keyed by
  JSONPath (`$`, `$.actors[0].id`, `$["odd\u0020key"]`). Channels: `opacity`,
  `x`, `y`, `scroll` (rows), and per path `node.<path>.open` (0 folded to 1 open,
  non-empty objects and arrays only), `node.<path>.highlight`, and
  `node.<path>.value` (variant index: 0 is `value`'s, `n` is that path's `n`th
  change). An opening node's closing bracket slides out from under its row and
  rows below move by exactly the room it opens; its children fade in at full
  pitch inside that room, and nothing above it moves. A value change rolls up
  through its row's window. `TreeActor` writes `open`/`close` (0.45 s
  zero-bounce), `highlight(path, at, seconds)`, `set(path, value, at)`,
  `scroll_to(row, at)`, and `reveal(path, at)`, which scrolls the least distance
  that shows a path's block once the authored folds settle. Object keys display
  in `serde_json::Value` order (sorted). Trees are channel-only, so they run in
  `plan present` too.
  ```rust
  let mut tree = TreeActor::declare(&mut scene, "plan",
      TreePlan::new([560.0, 150.0], 900.0, emitted).max_rows(22).expanded(["$"]))?;
  tree.open(&mut scene, "$.continuousChannels", at)?;
  tree.reveal(&mut scene, "$.continuousChannels", at)?;
  tree.highlight(&mut scene, "$.continuousChannels[0].property", later, 2.0)?;
  tree.set(&mut scene, "$.continuousChannels[0].initial", json!(1.0), later)?;
  ```
  The showroom is `cargo run -p psychopomp-tree` (writes `target/tree.json`, the
  plan `agent-demo` emits); render it with
  `cargo run --release -- plan render target/tree.json output/tree.mp4 --theme neutral`.
- Axes (shared by `plot` and `lanes`): `{ range: [start, end], ticks?, label?, unit? }`.
  `AxisPlan::new(range).every(step)` or `.nice(count)` chooses ticks; labels share
  the fewest exact decimals and append `unit`.
- `plot`: `origin` (top-left of the data frame), `size`, `x` and `y` axes,
  `series` of `{ id, label?, tone?, dashed?, points: [[x, y]], slopes? }` (x never
  decreasing; a repeated x draws a step; `slopes` are exact dy/dx per point, else
  central differences), and `marks` of `{ id, x, label? }`. Channels: `opacity`,
  `x`, `y`, `axes` (draw-on), `playhead` (an x value; no playhead without the
  channel), `playhead.opacity`, `series.<id>.draw|opacity|ride|velocity`, and
  `mark.<id>.opacity`. A riding series puts a dot at the playhead; `velocity`
  adds its tangent arrow (where the dot will be 8% of the x range later) and a
  signed `v` readout above the frame. The Scene Program computes every curve:
  `PlotSeriesPlan::sampled(id, label, tone, range, samples, |x| ..)` or
  `::motion(.., |t| MotionState)` for exact velocities. Sample motions from the
  compiled Property Track to show what the engine actually does.
  ```rust
  let mut plot = PlotActor::declare(&mut scene, "plot", &PlotPlan::new(
      [250.0, 250.0], [1420.0, 560.0],
      AxisPlan::new([0.0, 1.6]).every(0.2).label("time (s)"),
      AxisPlan::new([0.0, 1.3]).every(0.5).label("position"))
      .series(PlotSeriesPlan::motion("bouncy", "bouncy", Tone::Accent, [0.0, 1.6], 321, &track)))?;
  plot.show(&mut scene, at, 1.0);               // fade in, draw the axes
  let drawn = plot.draw(&mut scene, "bouncy", at, 1.4);
  let arrived = plot.ride(&mut scene, "bouncy", [0.0, 1.6], drawn, 3.0);
  plot.velocity(&mut scene, "bouncy", drawn, 1.0);
  plot.stop_ride(&mut scene, "bouncy", arrived);
  ```
- `lanes`: `origin` (top-left, above the cue row), `width`, `time` axis (seconds),
  optional `labelWidth` (400) and `laneHeight` (52), `lanes` of `{ id, label,
  tone?, keys?: [seconds], curve?: [[seconds, value]] }` (sparklines scale to
  their own range), and `cues` of `{ id, start, end, label? }`. Channels:
  `opacity`, `x`, `y`, `reveal` (ruler, lanes top to bottom, then cues),
  `playhead` (seconds), `playhead.opacity`, and `lane.<id>.opacity|emphasis`.
  The cue under the playhead brightens and crossed keys light, then cool.
  `LanesPlan::from_scene_plan(&plan, origin, width, |channel| Some(label))` builds
  lanes from a plan's continuous channels: keys at their event times, sparklines
  from the compiled tracks, and the plan's cues. `LanesActor::show`, `scrub`, and
  `emphasize` write the channels.
  The showroom is `cargo run -p psychopomp-charts` (writes `target/charts.json`);
  render it with
  `cargo run --release -- plan render target/charts.json output/charts.mp4 --theme neutral`.
- `video` (Video Card): `mediaId` (a planned `video` media placement), `size`
  (decoded `[width, height]`), `fps`, `center`, `width` (card width at scale 1;
  height follows the footage aspect), and optional `title` (a 44 px title bar).
  Channels: `x`, `y` (offsets from `center`), `scale`, `opacity`, `rotation`,
  `tilt-x`, `tilt-y`, `blur` (near-edge defocus of a tilted card), and the focus
  window `focus-x`, `focus-y` (its center, as fractions of the frame; 0.5) and
  `focus-size` (fraction of the frame visible; 1). The footage's source time follows
  its placement on the plan clock, holding the first frame before it and the last
  after it. `VideoActor::declare(scene, id, plan, video::media(id, path, (from, to),
  at))` adds the card and its placement; `fly_in`, `focus(region_in_source_px)`,
  `unfocus`, and `hide` write the motion. Video Cards draw beneath other overlays,
  over any root, and plans using them are export-only.
  ```rust
  let mut card = VideoActor::declare(&mut scene, "recording",
      VideoPlan::new("session", [1920, 760], 60).at([960.0, 520.0], 1520.0).titled("vim  /  opencode v2"),
      video::media("session", "../assets/opencode-v2-session-tool/max-hot-reload-split.mp4", (0, 9 * SECOND), 0))?;
  card.fly_in(&mut scene, seconds(0.25));
  card.focus(&mut scene, seconds(3.0), [920.0, 225.0, 900.0, 356.0], 0.9);
  ```
  The showroom is `cargo run -p psychopomp-video` (writes `target/video.json`).
- `text` (typed with `psychopomp::text::TextPlan`, in the JSON shape hand-built
  `text` actors always used): `text`, `center`, `fontSize` (28), `color`
  (`[r, g, b]`, white), optional `verticalMask`, and
  [`anchors`](#pin-overlays-to-anchors). Its content may follow a `content`
  State Channel. Channels are strict: `opacity`, `x`, `y` (absolute canvas
  coordinates defaulting to `center`; while pinned, their displacement from
  `center` moves the text from its anchor), and `anchor.<id>`. `TextActor`
  writes `show` (fade and rise; text whose first write is `show` starts hidden),
  `hide`, `show_during(from, until)`, `swap(next, at)` (this text lifts and
  fades as `next` rises into the same place 80 ms later), and `move_to`.
  ```rust
  let region = TextPlan::new("us-east-1", [0.0, 0.0]).size(22.0).color([150, 160, 178])
      .anchor(AnchorPlan::stage("api", "api", Edge::Bottom).with_offset([0.0, 34.0]));
  let mut cold = TextActor::declare(&mut scene, "region", &region)?;
  let mut warm = TextActor::declare(&mut scene, "region-warm",
      &TextPlan { text: "us-east-1 · warm".into(), ..region.clone() })?;
  cold.show(&mut scene, at);
  cold.swap(&mut scene, &mut warm, landed);
  ```
- `image`: `mediaId` (a planned `image` media placement: PNG, JPEG, or WebP,
  recognized by content), `center`, `width` (at scale 1; the height follows the
  image), `framed` (a card with the theme's material, border, and shadow),
  `title` (a 44 px title bar; framed only), `radius` (corners of a bare image),
  and [`anchors`](#pin-overlays-to-anchors), which replace `center`. Channels:
  `x`, `y` (offsets), `scale`, `opacity`, `rotation`, `tilt-x`, `tilt-y`, `blur`
  (near-edge defocus), and `anchor.<id>`. The file is decoded once and halved
  until it is at most twice its shown width. `ImageActor::declare(scene, id,
  &plan, image::media(id, path, from, until))` adds the actor and its placement;
  `fly_in`, `hide`, and `move_to` write the motion. Images draw with the Video
  Cards, beneath other overlays; plans using them are export-only.
  ```rust
  let mut trace = ImageActor::declare(&mut scene, "trace",
      &ImagePlan::new("trace", [960.0, 300.0], 340.0).titled("trace.png"),
      image::media("trace", "../assets/anchors/trace.png", 0, scene.duration_nanos()))?;
  trace.fly_in(&mut scene, at);
  ```
- `footage`: any image, video, or image sequence as an overlay. `clip` is how
  the source plays: `media` (a planned `video` or `image` placement, made with
  `footage::media(id, path, from, until)` or `footage::still`; an image
  sequence is a printf pattern such as `frames/%03d.png`), `trim` (`[from, to]`
  seconds into the source; the whole file by default), `rate` (1, up to 16),
  `repeat` (`hold`, `loop`, or `bounce` at the trim's ends), `reverse`,
  `freeze` (seconds into the trim, shown throughout), `fps` (decode rate: the
  video's own up to 60, an image sequence's playback rate, 24), and
  `resolution` (decoded width; by default about twice the widest it is shown).
  The playhead starts at the placement's timeline start; the placement is the
  span the file is available in, not the trim. Then `center`, `size` (the box at
  scale 1), `rotation` (rest angle), `fit` (`cover`, `contain`, `fill`),
  `mask` (`{ "shape": "rect", "radius": r }`, `{ "shape": "circle" }`, or
  `{ "shape": "polygon", "points": [[x, y], ...] }` as fractions of the box),
  `framed` (the Video Card's card), `title` (framed rectangles), `tint` (the
  tone the `tint` channel moves toward, accent), and
  [`anchors`](#pin-overlays-to-anchors). Channels: `x`, `y` (offsets),
  `scale`, `opacity`, `rotation` (from rest), `tilt-x`, `tilt-y`, `blur`
  (near-edge), `defocus` (whole surface), `focus-x`, `focus-y`, `focus-size`
  (the Ken Burns window, fractions of the fitted frame), `time` (the playhead,
  seconds into the trim; unwritten it plays naturally), `saturation` (1),
  `tint` (0), `dim` (0), and `anchor.<id>`. `FootageActor` writes `fly_in`,
  `toss_in(at, from, spin)`, `glide`, `to`, `move_to`, `focus`, `unfocus`,
  `drift(at, until, from, to)`, `treat(at, Treatment::REFERENCE, seconds)`,
  `hide`, the playhead beats `freeze`, `play(at, rate)`, `ramp(at, seconds,
  rate)`, `retime(at, target, seconds, curve)`, `stutter(at, seconds, times)`
  (each returns when it settles), and `audio(gain)`: the video's own audio as
  placements that follow its trim and start, and its loops (not a rate, a
  reverse, or retimes). `footage::layout` arranges collages as `Tile`s:
  `grid`, `masonry`, `scatter`, `pile`, `filmstrip`, and `by_distance` (a ripple
  order to stagger in); `FootagePlan::in_tile` places a clip in one.
  `footage::probe(path, fps)` reads a source's size, rate, alpha, and audio
  with ffprobe. Footage draws with the Video Cards and images, beneath other
  overlays; plans using it are export-only.
  ```rust
  let tiles = layout::masonry([150.0, 90.0, 1620.0, 880.0], &aspects, 4, 22.0);
  let wall = FootageActor::declare(&mut scene, "wall-3",
      &FootagePlan::in_tile(Clip::new("countdown").looping(), tiles[3]).framed(),
      footage::media("countdown", "../assets/footage/countdown.mp4", 0, scene.duration_nanos()))?;
  wall.toss_in(&mut scene, at, [0.0, 900.0], 0.4);
  wall.freeze(&mut scene, seconds(3.7));
  wall.ramp(&mut scene, seconds(5.1), 1.2, 3.0); // from a standstill to 3x
  ```
  The showroom is `cargo run -p psychopomp-footage` (writes the reel
  `target/footage.json` and its segments beside it); `-- --bench` writes a
  twenty-clip wall to `target/footage-bench.json`. Decoded videos are cached
  under `target/psychopomp-cache/footage` (safe to delete);
  `PSYCHOPOMP_FOOTAGE_CACHE_MB` bounds frames in memory (384) and
  `PSYCHOPOMP_FOOTAGE_STATS=1` reports the store after a render.
- Lip sync (`psychopomp::lipsync`) plays a **Sprite Sheet** (an image
  sequence of expressions × mouth shapes, `SpriteSheet::SLOTS` frames each:
  `Mouth::ALL`, then a blink) through a `footage` overlay decoded at
  `SPRITE_FPS` frames a second, cutting its `time` channel with `Set` events
  instead of playing it. `visemes(words, TICK)` turns placed word timings into
  mouth cues on a fixed 110 ms sprite tick (closed in pauses of at least
  `PAUSE`, at rest after the last word); `blinks(from, until, salt)` scatters
  deterministic blinks; `frames(sheet, expressions, mouths, blinks)` layers them
  into frame cuts; `Sprite::perform` writes them. The same frame-cut trick
  swaps any image layer on the narration clock (the council's speech box,
  agenda, and banners). `scenes/high-council` is the worked example.
- `callout`: `anchors` (one to eight; the first is where it starts), `lines` (one
  to three lines of `{ text, tone }` spans), `size` (24), `side` (where the label
  sits: `top`, `bottom`, `left`, `right`, `top-left`, `top-right` (default),
  `bottom-left`, `bottom-right`), `reach` (first-leg length, 64 px), `elbow`
  (diagonals turn into a 28 px horizontal shelf), `tone` (leader and mark,
  `accent`), and `chip`. Each anchor has an `id` and a `kind`: `point` (`at`),
  `stage` (`element`, a positioned element of the Stage root), or `editor`
  (`target`, a Semantic Target of the editor root), with an optional `edge`
  (`center` by default, or a side or corner of the outline) and an optional
  per-anchor `side`. Channels: `opacity`, `draw` (leader draw-on; the mark
  appears with it), `label` (fade and 8 px rise), `emphasis`, and
  `anchor.<id>` weights (1 for the first anchor, 0 otherwise). Anchors may also
  be `participant` (`sequence`, `participant`) or `row` (`sequence`, `row`) of a
  Sequence Diagram actor. The renderer
  resolves every weighted anchor at every sample from the prepared root and
  blends them, so the leader stays on its card through camera moves, jolts, and
  shutter samples, and on its code range as lines move or the panel zooms.
  `CalloutActor` writes `show` (draw on, then the label), `hide` (label out,
  then retract), `move_to(anchor)` (every weight springs on one critically
  damped profile, so interrupted moves keep their velocity), and `emphasize`
  (instant flare, convex decay).
  ```rust
  let mut note = CalloutActor::declare(&mut scene, "retries",
      &CalloutPlan::new(CalloutAnchorPlan::Stage { id: "client".into(),
          element: "client".into(), edge: CalloutSide::Top, side: None },
          vec![CaptionSpanPlan::new("retries 3×", Tone::Plain)])
          .anchor(CalloutAnchorPlan::Stage { id: "api".into(), element: "api".into(),
              edge: CalloutSide::Top, side: Some(CalloutSide::TopLeft) })
          .elbow())?;
  note.show(&mut scene, at);
  note.move_to(&mut scene, "api", later)?;
  ```
  The showroom is `cargo run -p psychopomp-callouts` (writes the reel
  `target/callouts.json` and its segments under `target/callouts/`); render it
  with `cargo run --release -- plan render target/callouts.json output/callouts.mp4 --theme neutral`.
- `diagnostic`: `target` (a Semantic Target of the editor root), `severity`
  (`error` by default, `warning`, `info`), and `gutter` (true). Channels: `draw`
  (0..1 of the wave's length; the gutter icon pops in with it), `opacity`, and
  `wave` (amplitude, 1). `DiagnosticActor` writes `show` (draw on) and `clear`
  (the wave relaxes flat as it fades).
- `hover-card`: `target`, `sections` (one to four; each `{ "code": [[spans]] }`
  of highlighted lines or `{ "text": [[{ text, tone }]] }` of prose, ten lines in
  all, divided by rules), and `side` (`above` by default, or `below`). Channel:
  `presence` (fade and an 8 px rise from the range). `HoverPlan::new(target)
  .code(line).text(spans).below()` builds it; `HoverActor` writes `show` (a pop
  with slight overshoot) and `hide`. The card slides to stay inside the editor.
- `cursor`: `anchors` of `{ id, target }`. Channels: `opacity`, `head` (the
  caret's fraction of the weighted range, 1), `tail` (the selection's other end;
  defaults to `head`), `blink` (seconds since the caret moved; -1 holds it solid),
  and `anchor.<id>` weights. `CursorActor` writes `show`, `hide`, `move_to(anchor,
  head, at)`, `select(anchor, at, seconds)` (the caret sweeps the range as the
  selection grows), and `collapse`. Several cursors make a multi-cursor.
- Inlay Hints are editor parts, not actors: `EditorRecipePlan::insert_inlay(id,
  line, after_range, ide::ghost(": Effect<User>"))` (or a diff line's `.inlay`)
  inserts an `inlay:<id>` part revealed by the editor channel `inlay.<id>`;
  `InlayHint::on(scene, &editor, id).show(scene, at)` opens it. Code after it moves
  aside and Semantic Targets after it follow.
  ```rust
  let editor = diff.declare(&mut scene, &[fix], 0, true)?;   // a line has .range("run", ..)
  editor.target(&mut scene, "run", 10, "run")?;
  DiagnosticActor::declare(&mut scene, "error", &DiagnosticPlan::error("run"))?.show(&mut scene, at);
  HoverActor::declare(&mut scene, "why", &HoverPlan::new("run")
      .code("const program: Effect<User, NotFound, Database>")
      .text(vec![CaptionSpanPlan::new("Type 'Database' is not assignable to type 'never'.", Tone::Plain)]))?
      .show(&mut scene, later);
  ```
  The showroom is `cargo run -p psychopomp-diagnostics` (writes
  `target/diagnostics.json`); render it with
  `cargo run --release -- plan render target/diagnostics.json output/diagnostics.mp4 --theme opencode`.
- Text surfaces (`terminal`, `chat`, `changed-files`) share the Window
  channels `opacity`, `x`, `y` (offsets), `scale`, and `content` (everything
  inside, 0 to 1). Each handle's `show` settles the window in like a Stage card
  (16 px drift, 1.035 scale, content 65 ms behind) and `hide` fades it.
  Sub-channel IDs are letters, digits, `-`, and `_`. Lines, messages, and rows
  already in a recipe are shown from time zero; the handles append them as the
  scene is authored and reveal them with channels.
- `terminal`: `origin` (window top-left), `width`, `rows` (visible text rows),
  `size` (22), optional `title` (title bar), `prompt` spans (`❯ ` in the accent
  by default), and `lines` of `kind` `command` (`id`, `text`), `output` (`id`,
  `spans` of `{ text, tone }`; none is a blank line), or `task` (`id`, `spans`,
  `done` spans, `mark`: `check` | `cross`). Channels: Window channels, `scroll`
  (a floor on the first visible row), `caret` (block caret on the last visible
  command), and per line `line.<id>.reveal` (opens its row and fades it in),
  `line.<id>.typed` (fraction of characters, commands and output),
  `line.<id>.highlight`, and for tasks `line.<id>.spin` (spinner motor age,
  seconds, -1 before), `line.<id>.mark` (seconds since the handoff), and
  `line.<id>.status` (0 shows `spans`, 1 `done`). Rows stack by their reveals
  and the window shows the last `rows`, so a full window slides older lines up
  by exactly the new room. `TerminalActor` writes `type_command` (a prompt opens,
  keystrokes land at a deterministic natural cadence, the caret leaves on Enter,
  which it returns), `prompt` and `idle` (a waiting, blinking caret), `print`
  (lines 45 ms apart), `stream` (one line by character), `spin`/`resolve` (the
  mark draws at the motor's next top-right crossing; its clocks stop once it
  cools), `highlight`, `clear`, and `scroll_to`. Channel-only: runs in
  `plan present`.
  let mut term = TerminalActor::declare(&mut scene, "term",
      TerminalPlan::new([300.0, 170.0], 1320.0, 15).titled("~/code/opencode — zsh"))?;
  let at = term.show(&mut scene, 0);
  let entered = term.type_command(&mut scene, at, "bun test")?;
  let task = term.spin(&mut scene, entered, vec![CaptionSpanPlan::new("Running tests", Tone::Muted)])?;
  term.resolve(&mut scene, &task, entered + 2 * SECOND, Mark::Check,
      vec![CaptionSpanPlan::new("24 pass", Tone::Success)])?;
- `chat`: `origin`, `size` (window), `style` (`slack` | `bubbles`), `textSize`
  (22), optional `title`, `subtitle`, `composer` (placeholder), and `me` (whose
  bubbles sit on the right), `people` of `{ id, name, tone, initials?, badge? }`,
  and `messages` of `{ id, author, spans: [{ text, tone, code }], time?,
  reactions: [{ id, label, count }] }`. Text wraps in sans-serif; `code` spans
  are monospaced on a chip. Channels: Window channels, per message
  `message.<id>.typing` (indicator presence), `.wait` (the dots' clock, seconds),
  `.reveal` (grows the slot into the message), `.typed` (streams the text), and
  `.highlight`, and per reaction `reaction.<message>.<reaction>` (pop). Rooms
  stack up from the composer, so a new message pushes every older one up by
  exactly its room; a run by one author shows its avatar and name once.
  `ChatActor` writes `typing` (opens the next slot), `say`/`say_text` (fills the
  author's typing slot or opens one; returns the message ID), `stream` (the
  text by character), `react`, `highlight`, and `stamp` (timestamps for later
  messages). Channel-only: runs in `plan present`.
  let mut chat = ChatActor::declare(&mut scene, "thread", ChatPlan::new([460.0, 110.0], [1000.0, 860.0],
      vec![ChatPersonPlan::new("dax", "Dax Raad", Tone::Warning)])
      .titled("# opencode-dev", None).composer("Message #opencode-dev"))?;
  chat.typing(&mut scene, at, "dax")?;
  let said = chat.say_text(&mut scene, at + SECOND, "dax", "it races compaction")?;
  chat.react(&mut scene, at + 2 * SECOND, &said, "👀", 2)?;
- `changed-files`: `origin`, `width`, `size` (22), optional `maxRows` (a
  scrolling window) and `title` (title bar), `files` of `{ id, path, status:
  added | modified | deleted | renamed, added, removed, from? }`, and `totals`
  of `{ atNanos, files, added, removed }` in increasing time (zero before the
  first; without any, the sum of every file). Channels: Window channels,
  `scroll`, and per file `row.<id>.reveal` (fade and rise into its fixed slot;
  diffstat blocks pop in left to right), `row.<id>.highlight`, and
  `row.<id>.dim`. The diffstat follows GitHub: a change of under five lines
  shows a block per line, a larger one splits all five by the share of
  additions. The header's file count and `+N −M` totals are Rolling Numbers.
  `ChangedFilesActor` writes `reveal` (every row, staggered; totals roll as each
  lands), `reveal_row`, `focus`/`unfocus` (one row lit, the rest receding),
  `highlight`, and `scroll_to`. A card whose totals roll is export-only, like a
  Rolling Number; one with static totals is channel-only.
  let mut files = ChangedFilesActor::declare(&mut scene, "files", ChangedFilesPlan::new([310.0, 190.0], 1300.0, vec![
      ChangedFilePlan::new("compaction", "src/session/compaction.ts", FileStatus::Modified, 38, 11),
      ChangedFilePlan::new("sleep", "src/util/sleep.ts", FileStatus::Deleted, 0, 17),
  ]).titled("opencode #51842 · fix(session): await compaction"))?;
  let shown = files.show(&mut scene, 0);
  let landed = files.reveal(&mut scene, shown, 0.11)?;
  files.focus(&mut scene, "compaction", landed + SECOND)?;
- `lower-third`: `origin` (the bar's left edge, the name's vertical center),
  `name`, optional `role`, `tone` (the bar's, `accent`), and `size` (46; the
  role is set at about half). Channels: `opacity`, `x`, `y`, and the phases
  `bar` (draws up from its foot), `name`, and `role` (each slides out from
  behind the bar), 0 to 1. `LowerThirdActor::show` draws the bar on
  `cubic-bezier(.45, 0, .2, 1)` with the name 140 ms and the role 280 ms behind;
  `hide` reverses them. Channel-only: runs in `plan present`.
  let mut intro = LowerThirdActor::declare(&mut scene, "intro",
      &LowerThirdPlan::new([120.0, 905.0], "opencode").role("coding agent"))?;
  intro.show(&mut scene, at);
  intro.hide(&mut scene, at + 4 * SECOND);
  The showroom is `cargo run -p psychopomp-text-surfaces` (writes the reel
  `target/text-surfaces.json` and its segments under `target/text-surfaces/`);
  render it with
  `cargo run --release -- plan render target/text-surfaces.json output/text-surfaces.mp4 --theme opencode`.
- Readouts (shared by meters, bars, and checklist counts): `{ decimals? (0–3),
  grouping?, rounding?: "nearest" | "up" | "down", prefix?, unit? }`. Digit
  wheels follow the sampled value like an odometer and smear by the value's
  velocity, so a label counts with its bar. `ReadoutFormat::new(0).grouped().unit("ms")`.
- `checklist`: `origin` (top-left), `width` (results right-align there), `size`
  (28), `rowHeight` (1.9 × size), `items` of `{ id, label, result?, failure? }`
  (`failure` replaces `result` when the item fails), `title` (a heading with a
  done count), `rail`, and `panel`. Channels: `opacity`, `x`, `y`, and per item
  `item.<id>.reveal` (0..1), `item.<id>.spinner` and `item.<id>.mark` (clocks in
  seconds, -1 inactive), and `item.<id>.outcome` (0 pending, 1 done, 2 failed,
  3 skipped; rounded). `ChecklistActor` writes `reveal` (rows 120 ms apart),
  `start(item, at)`, `resolve(item, at, Outcome | Mark)` (waits for the
  spinner's handoff crossing; returns when the mark finishes), `skip`, `show`,
  and `hide`. Starting a resolved item again is a retry.
  let mut checks = ChecklistActor::declare(&mut scene, "checks",
      &ChecklistPlan::new([580.0, 300.0], 760.0).title("checks").rail().panel()
          .item(ChecklistItemPlan::new("unit", "unit tests").result("812 passed").failure("2 failed")))?;
  checks.reveal(&mut scene, at)?;
  checks.start(&mut scene, "unit", at)?;
  let failed = checks.resolve(&mut scene, "unit", later, Outcome::Failed)?;
  checks.start(&mut scene, "unit", failed + seconds(0.7))?; // retry
- `meter`: `kind` (`ring` | `bar`), `center`, `size` (ring radius or bar length),
  `scale` (an Axis: range, ticks, unit), `sweep` (270°; 360 closes the ring at
  twelve), `thickness`, `readout` (a Readout format; omitted hides the number),
  `label`, `tone` (accent), `thresholds` of `{ at, tone }` (whole at `at`,
  blending just below it), and `tickLabels`. Channels: `opacity`, `x`, `y`,
  `value`, `reveal` (track and ticks draw on), and `flash`. `MeterPlan::countdown(center,
  radius, seconds)` is a closed ring with a tick per second, rounded-up whole
  seconds, and warning/error tones near the end. `MeterActor::declare(.., initial)`
  then `set` (spring), `sweep` (linear, for timers), `countdown` (sweeps to zero
  and flashes at each threshold crossing and at zero), `flash`, `show`, `hide`.
  let mut timer = MeterActor::declare(&mut scene, "timer",
      &MeterPlan::countdown([640.0, 500.0], 170.0, 8.0).label("approval expires"), 8.0)?;
  timer.show(&mut scene, at);
  timer.countdown(&mut scene, at + SECOND, 8.0);
- `bars`: `origin` (where bars start, top of the first row), `width` (axis
  length), `axis`, `series` of `{ id, label?, tone? }` (1–4), `rows` of `{ id,
  label }` (1–12; ids without dots), `rowHeight` (64), `size` (24), `readout`,
  and `delta` `{ from, to, better?: "lower" | "higher", format?: "percent" | "factor" }`.
  Channels: `opacity`, `x`, `y`, `axes` (draw-on), `row.<id>.slot` (display
  position; defaults to declaration order), `row.<id>.opacity`,
  `bar.<row>.<series>` (the bar's value), and `delta.<row>` (chip presence).
  `BarsActor` writes `grow(at, series, &[(row, value)])` (staggered down the
  display order), `set`, `sort(at, series, SortOrder)` (springs only rows that
  move; ties keep declaration order), `reveal_rows`, `reveal_deltas`, `show`, `hide`.
  let mut bench = BarsActor::declare(&mut scene, "bench",
      &BarsPlan::new([520.0, 330.0], 900.0, AxisPlan::new([0.0, 2000.0]).every(500.0))
          .series(BarSeriesPlan::new("before", "before", Tone::Muted))
          .series(BarSeriesPlan::new("after", "after", Tone::Accent))
          .row("cold", "cold start")
          .readout(ReadoutFormat::new(0).grouped().unit("ms"))
          .delta(BarDeltaPlan::new("before", "after")))?;
  bench.grow(&mut scene, at, "before", &[("cold", 1840.0)])?;
  bench.grow(&mut scene, later, "after", &[("cold", 1214.0)])?;
  bench.reveal_deltas(&mut scene, later + SECOND)?;
  bench.sort(&mut scene, later + 2 * SECOND, "after", SortOrder::Ascending)?;
- `subtitles`: `origin` (lines' center x, the bottom line's center y),
  `maxWidth`, `size` (40), `maxLines` (2), `highlight` (accent), `backing`
  (true), `upcoming` (ink of words not yet said, 0.5; 0, or `.word_by_word()`,
  reveals each word as it is said, the line centered on what has been said),
  `face` (any Stage label face, `mono` by default; `.face(face)`), and `words`
  of `{ text, startNanos, endNanos }` on the plan clock.
  Channels: `opacity`, `x`, `y`, and `tilt` (radians, 0; each word steps up or
  down by its distance from the center and stays upright, and the backing grows
  to hold them). Pages break at sentence ends, pauses, and
  width, with balanced lines; the spoken word takes the highlight with a
  gliding pill. `SubtitlesPlan::from_spoken(&spoken, origin, max_width)` takes a
  placed narration clip's words (`.spoken(&other)` appends another). Plans
  using subtitles are export-only, like Rolling Numbers.
  let spoken = narration.clip("layer")?.place(&mut scene, start);
  SubtitlesActor::declare(&mut scene, "subtitles",
      &SubtitlesPlan::from_spoken(&spoken, [960.0, 900.0], 1300.0).size(44.0))?;
- `confetti`: `origin`, `count` (140), `seed`, `speed` (1700 px/s), `angle`
  (degrees clockwise from up), `spread` (55° half-angle), `gravity` (1200 px/s²),
  and `tones` (accent, success, request, warning, plain). Channels: `opacity`,
  `x`, `y`, and `burst` (seconds since launch, -1 before). `ConfettiActor::burst(at)`
  runs the 3.6 s clock; the same seed always gives the same burst.
  The showroom for all five is `cargo run -p psychopomp-viz-components` (writes
  `target/viz-components/reel.json`, each segment beside it, and the narration
  it plays); render it with
  `cargo run --release -- plan render target/viz-components/reel.json output/viz-components.mp4 --theme opencode`.
  Callout anchors are the shared [Anchor](#pin-overlays-to-anchors) targets
  plus a per-anchor label `side`; `CalloutSide` is `psychopomp::anchor::Edge`.
- `lens`: a loupe of thick glass that magnifies and refracts the frame beneath
  it. `anchors` (one to eight, callout anchors without a `side`; the first is
  where it starts), `size` (`[width, height]` at full presence), optional
  `corner` (omitted: fully round, so a square is a circle and a wide lens a
  capsule at every size), `magnification` (1.6; 0.5..4), optional `bevel` (rim
  width in px; 28% of the shorter half side), `refraction` (page depth in rim
  widths, 0.6: how hard the rim bends), `dispersion` (0.04), `frost` (0), and
  `shadow` (0.5). Channels: `presence` (0 absent, 1 full; it condenses rather
  than fades), `x` and `y` (offsets from the blended anchor), `width`,
  `height`, `magnification`, `focus-x` and `focus-y` (the point shown at the
  center, from the center), `frost`, and `anchor.<id>` weights. A lens draws
  after callouts and before plain text and Tasks, refracting the root and every
  overlay beneath it at each temporal sample, over any root.
  `LensActor` writes `show` (a springy condense), `hide`, `move_to(anchor)`
  (weights on one critically damped profile, so a redirected glide keeps its
  velocity), `slide([dx, dy])`, `magnify`, `resize([w, h])` (a round loupe
  stretches into a capsule), and `focus([dx, dy])`, which with an opposite
  `slide` floats the glass beside what it reads.
  let mut loupe = LensActor::declare(&mut scene, "loupe",
      &LensPlan::circle(CalloutAnchorPlan::Editor { id: "call".into(),
          target: "call".into(), edge: CalloutSide::Center, side: None }, 250.0)
          .anchor(schedule).magnification(1.7))?;
  loupe.show(&mut scene, at);
  loupe.move_to(&mut scene, "schedule", later)?;
  loupe.resize(&mut scene, [560.0, 96.0], later + SECOND);  // read along the line
  loupe.slide(&mut scene, [150.0, 0.0], later + 2 * SECOND);
  The showroom is `cargo run -p psychopomp-loupe` (writes the reel
  `target/loupe.json` and its segments under `target/loupe/`); render it with
  `cargo run --release -- plan render target/loupe.json output/loupe.mp4 --theme neutral`.
- Editor Line Marks: `"mark": "added" | "removed"` on a line, with presence
  channel `mark.<line-id>`; `panel-x`, `panel-y`, and `panel-opacity` move and fade the card (the Stepped Diff
  enters on `panel-y`).
- `stage` (root): up to 128 `elements` of `kind` `card` (`at`, `size`, `title`, `status`,
  `tone`), `orb` (`at`, `radius`, `points`), `beam` (`from`, `to`, `bend`),
  `packet` (`beam`, `reverse`, `label`), `label` (`at`, `size`, `spans`, and an
  optional `face`: `mono` (bundled CommitMono, the default), `sans` and `sans-bold`
  (Helvetica Neue Regular and Bold), `serif` and
  `serif-italic` (Didot), `light` (Helvetica Neue Light), or `shout`
  (Helvetica Neue Condensed Black), set with `StageElement::label(..).face(Face::Serif)`;
  the non-mono faces are installed by macOS, and a machine without them shapes the
  text in whatever face its font database substitutes, so the same plan renders
  in another typeface there), `ring`
  (`at`, `radius`, `thickness`), `bolt` and `shield` (see Effects below), and the
  diagram vocabulary below (`form`, `shape`, `path`, `icon`), plus `post` (`bloom`,
  `grain`, `vignette`,
  `backdrop`). Channels are `<element>.<property>` (for example `service.shatter`,
  `link.draw`, `probe.age`, `client.blur|content`) and `camera.x|y|z|focus|dof|shake|quake|kick-x|kick-y|punch`,
   `camera.yaw|pitch|roll|zoom|pivot|handheld`, `camera.track.<id>`,
   `post.bloom|chroma|exposure|vignette|rewind|zoom|flash`. `post.rewind` is a 1.4-second local
   age for VHS rewind interference (-1 inactive). `camera.quake` is sustained
   trauma (0..2) added to a jolt's `shake`; `post.zoom` is a radial streak toward
   the frame center (0..0.5); `post.flash` washes the frame toward white (0..1). Cards also take the deletion
   channels `cool|damage|glitch|cut|ghost`, the status-spinner clocks
   `spinner|release|mark` (seconds; -1 inactive), with `mark: "check" | "cross"`,
   and `status-from|swap`: while `status-from` names an entry (-1 is unset), the
   status line cross-fades straight from it to `status` by `swap` (0..1), so
   `StageActor::swap_status(card, at, [from, to], seconds)` never passes the
   entries between them as the fractional `status` channel does; plus
   `charge|dissolve|scan` (orbs take `charge`).
   A packet is one clock: `age` (seconds since
  dispatch, -1 before) and `flight`; the renderer derives its gather, flight, trail,
  landing ring, and light from them. Beams choose their own ports and curve; leave
  `bend` at 0 unless two beams need separating.
- `StageActor` implements the `explainer-motion` beats: `settle_in` (a panel drifts
  16 px into place, scales from 1.035, and sharpens; its content follows 65 ms later),
  `connect` (soft fixed-size port reveal, eased wire draw, then a quiet hold),
  `send` (gather, flight, landing), `hit` (instant attack, convex decay),
  `kick` (a two-frame shove that springs back past rest), `jolt` (an impact: the
  camera kicks along the blow, a squared-trauma noise rumble with slight roll
  decays, and the frame punches in about 2%),
  `twang`, and `land`. `glide` is the minimum-jerk (smootherstep) move of
  exact duration between resting compositions. `bounce` and `to` spring any channel (an undeclared
  channel starts at its resting value, the same Stage channel default the
  renderer reads when nothing writes it: opacity, scale, `content`, `draw`,
  `fill`, `typed`, ring `sweep`, `spin`, shield `up`, and `camera.zoom` rest at
  1, the `burst`/`age`/`dissolve`/spinner clocks and `post.rewind` at -1,
  `flight` at 0.8, `post.bloom`/`post.vignette`
  at the plan's `post`, everything else at 0; declare other starting poses with
  `channel`, and fade something in from hidden with `fade_in`), `ease` follows
  any curve, and `clock` starts an elapsed-seconds channel that runs to the scene's
  end for effect rigs such as the card spinner (`clock_for` stops it after a fixed
  lifetime, as for `burst` or `post.rewind`). Use a `Smootherstep` ease for staged
  camera moves with exact timing, springs for responsive camera/panel settling,
  and instant-attack fades for light.
  Composed beats return when they settle: `orb_in(orb, at, OrbEntrance::HERO)`
  (the hero entrance: scale, blur, and angular offset gather in while it fades
  in), `glitch(card, at, seeds)` (three layouts 27 ms apart, then still),
  `rewind(at, chroma)` (`post.rewind`'s 1.4 s of tape interference with a
  chromatic hit) and `unburst(orb, at, seconds)` (the burst clock plays back
  to intact), `shock_kick(source, at, card, push, falloff)` (a card is shoved
  away as the burst's pressure front passes it; returns when it passes),
  `resolve_spinner(card, started, done)` (the spinner draws its mark at its
  next crossing; returns when the mark is drawn, where its sound belongs),
  `swap_status(card, at, [from, to], seconds)`, `swap_labels([from, to], at,
  gap)` (one label out, the other in `gap` later), `dim(cards, at, amount,
  seconds)`, `halo([(inner, opacity),
  (outer, opacity)], at, seconds)` and `halo_out`
  (two rings 60 ms apart in, 80 ms apart out), `ring_timer(ring, at, seconds,
  sweep)`, and `disconnect(beam, at, seconds)` (the reverse of `connect`).
  Build elements with `StageElement::card|orb|beam|packet|label|ring` and their
  options (`.tone`, `.statuses`, `.mark`, `.points`, `.bend`, `.reversed`,
  `.labeled`, `.align`, `.thickness`); `StagePost::RESTRAINED` is the explainer films' look.
  `StagePost::FLAT` removes bloom, grain, vignette, and backdrop light for editorial
  graphics. Pair it with `Face::Sans`/`SansBold`, ordinary shapes and SVG paths,
  and camera pans/zooms. Labels allow 10–320 px type; artwork icons allow 8–2048 px
  sides (their atlas coverage caps at 2048 px). Thick shape strokes allow up to
  256 px; connector strokes retain their 24 px maximum. The narrated example is
  `scenes/shape-of-openness`, with drawn geometry, image plates, and a hand stencil
  composited from registered footage layers.
  Orb `pulse` changes illumination, not geometry or attached beam ports. Card
  `flash` lifts ink and rim, not the entire fill. Connecting does not implicitly
  trigger `land`, `twang`, `surge`, or `flow`; author those only when the story
  calls for an impact or ongoing traffic. Packet labels use measured text bounds
  to stop short of the endpoint bodies while their packets finish travelling.
  A label that cannot fit in the corridor is omitted rather than partially hidden.
  Packets entering an orb trigger a directional surface ripple at the visible
  shell, before reaching the submerged endpoint. The flagship now uses critical
  `to` springs for camera moves; a `Smootherstep` ease remains available for
  minimum-jerk timing.
- The Stage camera (see "Stage Camera" in `CONTEXT.md`). `camera.x|y` pan and
  `camera.z` dollies along the view axis (0, pixel exact at depth 0).
  `camera.yaw|pitch` (radians, 0) swing it around the pivot at world depth
  `camera.pivot` (0): positive yaw moves the camera right, positive pitch raises
  it to look down. `camera.zoom` (1) multiplies the focal length. `camera.roll`
  (radians, 0) turns the image clockwise and crops just enough to cover the
  corners; it motion-blurs. `camera.handheld` (0, off; about 1 for a gentle
  operator) sways pan and angle. `camera.track.<id>` (0) weights follow a packet
  or positioned element, resolved at every sample. `camera.focus` is measured
  in world z as the unturned camera sees it. Cards, labels, and rings face the
  lens (billboards); orb particles and wires show true parallax. Undeclared
  camera channels take these defaults (`stage::CAMERA_CHANNELS`); declare
  `camera.zoom` at 1 before springing it with `StageActor::to`. Write shots
  with the `CameraRig` from `StageActor::camera`; each reads the pose written so
  far, moves by a `Move` (`Spring(seconds)` critically damped, `Glide(seconds)`
  minimum jerk, `Ease(seconds, curve)`, `Cut`), writes only channels whose
  destination changes, and returns when it settles:
  ```rust
  let camera = stage.camera();
  camera.establish(sc, 0, 380.0, 2.6);                      // dolly in to open
  camera.frame(sc, &["client", "api"], 150.0, at, Move::Spring(1.6))?;
  camera.follow(sc, "request", at, Move::Spring(0.7))?;     // a packet in flight
  camera.release(sc, landed, Move::Spring(1.0))?;           // hold where it landed
  camera.aperture(sc, at, 1.0, Move::Spring(1.0));
  camera.focus_on(sc, "archive", at, Move::Spring(1.3))?;   // rack focus
  camera.orbit(sc, "core", at, [0.48, -0.16], Move::Glide(3.4))?;
  camera.dolly_zoom(sc, "core", hit, 560.0, Move::Glide(2.4))?; // vertigo
  camera.roll(sc, hit, 0.04, Move::Glide(1.6));             // Dutch angle
  camera.whip(sc, &["queue", "drain"], 170.0, at, 0.6)?;    // with a post.zoom streak
  camera.handheld(sc, at, 1.0, 1.0);
  camera.drift(sc, "drain", at, 4.2)?;                      // a slow lean
  camera.push_in(sc, at, 120.0, Move::Glide(3.6));
  ```
  `framing` returns the fitted pose without writing it, `pose` reads the
  authored pose at a time, and `screen_box` gives an element's rectangle under a
  pose (for a Reel zoom's `transitionFocus`). Shots, jolts, quakes, follows, and
  handheld sway add without fighting: kick, rumble, and sway apply on top of
  the authored and followed pose. The showroom is `cargo run -p psychopomp-camera`
  (writes `target/camera.json`); render it with
  `cargo run --release -- plan render target/camera.json output/camera.mp4 --theme neutral`.
- Stage diagram vocabulary (`scenes/stage-forms`; `cargo run -p psychopomp-stage-forms`
  writes `target/stage-forms.json`):
  ```json
  { "kind": "form", "id": "store", "at": [1500, 470, 0], "points": 720, "tone": "accent", "tilt": 0.42,
    "shapes": [{ "shape": "plane", "size": [340, 184] },
               { "shape": "box", "size": [210, 210, 210], "edges": 0.5 },
               { "shape": "sphere", "radius": 142 }] }
  { "kind": "shape", "id": "gateway", "at": [900, 500, 0], "shape": { "rect": [232, 132] },
    "corner": 22, "fill": "surface", "fillOpacity": 1, "stroke": "muted", "width": 1.4, "dash": [6, 7] }
  { "kind": "shape", "id": "retry", "at": [900, 392, 0],
    "shape": { "arc": { "radius": 30, "start": 0.6, "sweep": 0.8 } }, "arrow": "end" }
  { "kind": "path", "id": "write", "through": ["client", [700, 640, 0], "gateway", "store"],
    "curve": "straight", "corner": 24, "bend": 0, "tone": "plain", "arrow": "end" }
  { "kind": "icon", "id": "db", "at": [900, 482, -1], "size": 46, "icon": "database", "tone": "plain" }
  { "kind": "packet", "id": "put", "beam": "write", "label": "PUT /doc" }
  - `form`: `shapes` (one to eight) of `sphere` (`radius`), `box` (`size` w/h/d; a
    cube or a slab; `edges`, the share of points on its edges, 0.5), `plane`
    (`size`, a dot matrix facing the camera at tilt 0), `lattice` (`size`), `cylinder`
    (`radius`, `height`), and `torus` (`radius`, `tube`); `points` (720; a plane or
    lattice needs a count that factors into its grid, such as 720 = 36 × 20 or
    729 = 9³), `tone` (accent), `tilt` (0.42 radians, the orb's view). Channels and
    their defaults: `opacity` 1, `x`/`y`/`z` 0, `scale` 1, `blur` 0, `rotation` 0
    (about the vertical axis, plus ambient `spin` 1), `pitch` 0 and `roll` 0
    (radians; ease them to tumble), `morph` 0 (a fractional index into `shapes`),
    `burst` -1, `shatter` 0, `pulse` 0, `hurt` 0, and `solid` 1 (the dark silhouette
    that hides what passes behind; 0 leaves a ring of dots see-through).
    `StageActor::morph(form, at, index, seconds)` eases to a shape on a minimum-jerk
    curve; `land` pulses a form.
  - `shape`: `shape` is `{ "rect": [w, h] }`, `{ "circle": r }`, `{ "arc": { radius,
    start, sweep } }` (turns clockwise from twelve o'clock), or `{ "polygon": [[x,
    y], ...] }` relative to `at`. `corner` rounds a rectangle or polygon; `fill` is a
    tone or `surface`/`background` (an opaque panel), scaled by `fillOpacity`;
    `stroke` is a tone (muted) or `null`; `width` (1.4), `dash` ([on, off] pixels),
    and `arrow` (`none`, `start`, `end`, `both`; arcs only). Channels: `opacity` 1,
    `x`/`y`/`z` 0, `scale` 1, `rotation` 0 (radians), `blur` 0, `draw` 1 (the stroke
    draws on from twelve o'clock, clockwise), `fill` 1, `emphasis` 0, `flash` 0
    (`land` flashes it). Beams and paths attach to shapes.
  - `path`: `through` is two to 32 waypoints, each a positioned element's id or a
    world point `[x, y, z]`. Element hops attach like beams (`bend` bows them);
    point runs follow `curve`: `straight` with `corner` rounding, `smooth`
    (Catmull-Rom through points only), or `bezier` (points only: start, then two
    controls and an end per curve). `tone`, `width` (1.4), `dash`, and `arrow` as
    for shapes. Channels: `opacity` 1, `draw` 1, `trim` 0 (erases from the start),
    `flow` 0, `emphasis` 0, `surge` 0. Arrowheads ride the drawn tip and sit on a
    body's silhouette. `StageActor::connect` draws a path without a port.
  - A `packet` rides a beam or a path (its `beam` field names either). An element
    waypoint between a path's ends is a stop: each leg is a whole packet life,
    the route's `flight` shared equally by its legs, and each next leg gathers
    `packet::RELAY` (0.12 s) after the previous lands. `StageActor::relay(packet,
    at, seconds)` sends it and lands every element it reaches, returning each
    leg's arrival; `send` returns the last. A packet can be sent again once its
    previous life (`packet::lifetime`) has ended.
  - `icon`: `size` in world pixels, one of `icon` (a bundled Phosphor name:
    `stage::ICONS`, from `assets/icons`, MIT) or `path` (SVG path data, filled, in a
    `view`-unit square, 256), and `tone` (plain draws in the text color).
    Optional `ink: [r, g, b]` is an explicit sRGB-byte pigment for artwork: it
    overrides the theme's tone, while retaining the Stage's normal exposure and
    highlight rolloff. It is useful for a logo whose color is part of the subject.
    Channels: `opacity` 1, `x`/`y`/`z` 0, `scale` 1, `blur` 0, `flash` 0.
  - `footage`: `size` (world pixels), `clip`, `fit`, and `mask` as for the
    [`footage`](#recipe-payloads-and-channels) overlay, `framed` (a card's mat
    and rim), and `tint` (a tone, accent). A camera-facing quad, like a card,
    so it takes depth, parallax, draw order, depth of field, follows, and
    anchors. Channels: `opacity` 1, `x`/`y`/`z` 0, `scale` 1, `blur` 0,
    `rotation` 0, `focus-x`/`focus-y` 0.5, `focus-size` 1, `time` (the
    playhead; unwritten it plays naturally), `saturation` 1, `tint` 0, `dim` 0.
    Its clip's media is a plan placement (`scene.media(footage::media(..))`);
    `StageActor::footage_playhead(element, &placement)` returns a `Playhead`
    for `freeze`, `play`, `ramp`, `retime`, `seek`, and `stutter`.
- Orb `rotation` is an angular offset in radians; animate it for a spin entrance
  rather than changing the ambient `spin` multiplier. `blur` adds defocus in world
  pixels. `burst` defaults to -1 (intact): set 0 on impact and ease linearly to
  5.2 over 5.2 seconds for collapse, fire, smoke, and ballistic embers. Reverse
  that clock to reassemble, then set -1 when it reaches zero. The first active
  burst also supplies the composite's gravity pinch and refractive shockwave.
  Volumes and sparks respect orb opacity; the pressure wave is a scene response.
- Effects ([EFFECTS.md](EFFECTS.md)). `bolt` (`from`, `to`: an element or shield
  ID, or a world point `[x, y, z]`; `strikes` 1..8, default 3; `branching`
  0..1.5, default 0.6; `tone`, default `request`) is lightning. Its channels are
  `age` (the discharge clock in seconds, -1 idle), `seed` (an integer that
  re-rolls the path; 0), `opacity` (1), and `hum` (a sustained arc, 0..1.5; 0).
  `shield` (`around`, a positioned element; `radius`; `tone`, default `accent`)
  is a hex bubble with `opacity` (1), `up` (raised, 0..1; 1), and `scale` (1);
  packets crossing into it and bolts striking it ripple it. Cards and orbs take
  `charge` (crawling crackle, 0..1.5; 0). Cards take `dissolve` (a burn clock in
  seconds, -1 intact, gone at 1.1, ash cold at 2.05) and `scan` (a sweep, 0..1,
  invisible at both ends; 0). `StageActor` beats:
  `zap(bolt, at) -> contact` (the leader sets out at `at`, the first stroke
  lands 75 ms later; pair it with `land` or `jolt`), `charge(element, at,
  intensity, seconds)`, `hum(bolt, at, intensity, seconds)`, `dissolve(card, at)
  -> gone`, `materialize(card, at, seconds) -> whole` (the dissolve reversed,
  decelerating), `scan(card, at, seconds) -> done`, and `raise`/`lower(shield, at,
  seconds)` (raising first declares the shield down).
  ```rust
  stage.charge(&mut scene, "build", at, 1.0, 1.4);       // crackle builds
  let contact = stage.zap(&mut scene, "strike", later);  // leader, then strokes
  stage.charge(&mut scene, "build", contact, 0.0, 0.0);  // it discharges
  stage.land(&mut scene, "deploy", contact);
  ```
  The showroom is `cargo run -p psychopomp-effects-showroom` (writes the reel
  `target/effects-showroom.json`); render it with
  `cargo run --release -- plan render target/effects-showroom.json output/effects-showroom.mp4 --theme neutral`.

Continuous channel events are `set`, `spring`, and `ease`
(`{ "operation": "ease", "atNanos", "target", "durationNanos", "curve" }` with
`curve` one of `linear`, `smoothstep`, `smootherstep`, `cubic-out`, `cubic-in-out`,
`{ "decelerate": s }`, or `{ "cubic-bezier": [x1, y1, x2, y2] }`). Use `ease` for
timed curves; never approximate one with stepped `set` events, which stutter.
Beats authored independently may write one channel out of time order, and a
segment cut from a longer gesture may write past its end:
`scene.sort_events()` orders each channel's events by time (keeping the source
order at one instant), and `scene.drop_events_after_end()` drops events that
start after the plan's duration, before `finish` (`scenes/balls-v3`).
`scene.channel_ids()` lists continuous channels in declaration order, the index a
`continuousChannels[n]` validation path names.
Reusable math is `psychopomp::math` (`lerp`, `remap_clamp`, `smoothstep`, `easing`,
`dynamics::settle`, `curve::Polyline`, `shapes::connect`, glam vectors); use it in
Scene Programs too. `--theme opencode` renders with the OpenCode TUI's tokens;
`--theme neutral` with the OpenCode blog's clear-neutral diagram palette (the
#50825 film's look).

### Pin Overlays To Anchors

Captions, Rolling Numbers, text, and images take the same optional `anchors` as
callouts, instead of hand-computed canvas coordinates. Each anchor has an `id`, a
`kind` (`point` with `at`; `stage` with `element`, a positioned element of the
Stage root; `editor` with `target`, a Semantic Target of the editor root;
`participant` with `sequence` and `participant`, that participant's header in a
Sequence Diagram actor; `row` with `sequence` and `row`, the span of a message
arrow, a note, or an End mark), an optional `edge` (`center` by default, or a
side or corner), and an optional `offset` in canvas pixels:

```json
"anchors": [
  { "id": "client", "kind": "stage", "element": "client", "edge": "bottom", "offset": [0, 40] },
  { "id": "api", "kind": "stage", "element": "api", "edge": "bottom", "offset": [0, 84] }
]
```

While an overlay has anchors, the blended point replaces its `origin` (captions,
Rolling Numbers) or `center` (text, images); its other channels still move it
from there. `anchor.<id>` weights choose the anchor (1 for the first, 0
otherwise), and every handle's `move_to(anchor, at)` springs them on one
critically damped profile, so an interrupted move keeps its velocity. The root
resolves each weighted anchor at every Temporal Sample, so pinned overlays ride
camera dollies, jolts, line insertions, and panel zooms without lag; they follow
position, not perspective scale. Anchors must name something the root can
place: preflight rejects a `stage` anchor without a Stage root, a beam or packet
element, an unknown Semantic Target, sequence, participant, or row, and weight
channels for undeclared anchors. Sequence anchors follow the diagram's `x`/`y`
channels and its measured header and note widths.

```rust
let mut request = CaptionActor::declare(&mut scene, "request",
    &CaptionPlan::line([0.0, 0.0], 24.0, spans).aligned(CaptionAlign::Center).chip()
        .anchor(AnchorPlan::stage("client", "client", Edge::Bottom).with_offset([0.0, 40.0]))
        .anchor(AnchorPlan::stage("api", "api", Edge::Bottom).with_offset([0.0, 84.0])))?;
request.type_in(&mut scene, at, 30.0, 0.6);
request.move_to(&mut scene, "api", launch)?; // glides beside the packet
```

The showroom is `cargo run -p psychopomp-anchors` (writes the reel
`target/anchors.json` and its segments beside it); render it with
`cargo run --release -- plan render target/anchors.json output/anchors.mp4 --theme neutral`.

## Keep The Renderer Running

`psychopomp plan serve` reads one JSON request per line from standard input and writes one JSON response per line to standard output. Progress and GPU diagnostics use standard error, leaving standard output machine-readable.

```bash
cargo run --release -- plan serve
```

Inspect a plan:

```json
{"id":1,"command":"inspect","plan":"target/agent-demo.json"}
```

Render one frame while retaining the initialized renderer:

```json
{"id":2,"command":"frame","plan":"target/agent-demo.json","output":"output/frame.png","at_nanos":1250000000}
```

Render an exact range:

```json
{"id":3,"command":"render","plan":"target/agent-demo.json","output":"output/window.mp4","start_nanos":200000000,"end_nanos":400000000}
```

Render a cue:

```json
{"id":4,"command":"render","plan":"target/agent-demo.json","output":"output/intro.mp4","cue":"intro"}
```

Stop the process:

```json
{"id":5,"command":"shutdown"}
```

## Author With Typed Handles

`PlanBuilder` creates stable handles and derives channel IDs from actor identity:

```rust
let mut scene = PlanBuilder::new("lesson", 3_000_000_000);
let title = scene.actor(
    "title",
    "title-card",
    serde_json::json!({ "title": "Effect" }),
)?;
let opacity = scene.continuous(&title, "opacity", 0.0);
scene.spring(&opacity, 200_000_000, 1.0, 0.4, 0.0);
scene.cue("intro", 0, 2_000_000_000);
let plan = scene.finish()?;
```

Renderer Recipe payloads remain adapter-owned. The lightweight core validates stable IDs, channel references, event ordering, finite values, cue ranges, exact media ranges, and Scene Plan versioning without knowing what a Task, editor, Video Card, or title card looks like.

### Author With The Score DSL

`psychopomp::score` is the primary way to author choreography. `PlanBuilder` is the only mutable binding; actor handles (`Stage`, `Camera`, `Caption`, `Callout`, `RollingNumber`, `Tree`, `Plot`, `Lanes`, `Sequence`, `Video`, `Terminal`, `Chat`, `ChangedFiles`, `LowerThird`, `Checklist`, `Meter`, `Bars`, `Subtitles`, `Confetti`, `Text`, `Image`, `Lens`, `Diagnostic`, `Hover`, `Cursor`) and sounds (`sfx::*.beat(id, gain_db)`, `audio.beat(gain_db)`) are immutable `Clone` values whose methods take `&self` and return composable `Beat`s (`Time -> (Span, Writes)`). Every combinator compiles directly to ordinary `PlanBuilder` events—none changes the Scene Plan format:

```rust
use psychopomp::{
    all, at,
    author::{PlanBuilder, millis, seconds, spread},
    plan::SpringPlan,
    score::{Beat, Caption, CueTime, Stage, each, stagger},
    sfx,
    stage::Move,
};

// Narration: a lead, then each clip and the gap after it.
let reading = narration.reading(seconds(1.6), [("before", seconds(2.4)), ("after", seconds(2.4))])?;
let mut scene = PlanBuilder::new("film", reading.duration());
let [before, after] = reading.place(&mut scene);

// Value actor handles: `scene` is the only `mut`.
let s = Stage::declare(&mut scene, "stage", &stage_plan)?;
let cam = s.camera();
let hdr = Caption::header(&mut scene, "#50825", "await compaction")?;

// Rows ripple 120 ms apart; `scene.at` / `at!` returns the occupied `Span`.
let settled = scene.at(
    before.at("three services"),
    stagger(millis(120), ["api", "db", "cache"], |card| s.settle_in(card)),
);

// Sequence (`.then`), accompaniment (`.with`), arrival reactions (`.on_end`), and camera shots:
let find = at!(scene, before.at("asks") =>
    hdr.type_in(55.0, 0.6),
    s.send("find", 0.6)
        .with(sfx::SEND.beat("find", -11.0))
        .with(cam.follow("find", Move::Spring(0.5)))
        .then(s.land("api").also(cam.release(Move::Spring(0.8)))),
);

// A beat keyed to a word that must still wait for its cause (`Span::reply` = 340 ms gather + 80 ms reaction).
scene.at(
    after.at("just once").not_before(find.reply()),
    s.send("lookup", 0.55)
        .with(sfx::SEND.beat("lookup", -11.0))
        .then(s.land("db")),
);

// Time a beat by where it lands rather than where it starts.
scene.at(before.at("sigterm"), s.send_arriving("kill", 0.55).then(s.land("api")));
scene.at(before.at("plugs in"), s.connect_contacting("link", 0.4));
scene.at(at, s.spring("camera.x", -110.0, SpringPlan::CAMERA));
```

- **Combinators (`psychopomp::score`)**: `.then(b)` / `chain![...]` (sequence), `.then_after(gap, b)`, `.also(b)` / `all![...]` / `at!(scene, time => ...)` (parallel), `.with(b)` (accompany at start, preserving primary span), `.on_end(b)` (trigger at end, preserving primary span), `.after(d)` / `.early(d)` (time shift), `stagger(gap, items, f)`, and `each(items, f)`.
- **Spatial envelopes (`psychopomp::layout::Placement`)**: `Placement::card(at, size)`, `Placement::orb(at, radius)`, `Placement::of(&stage_plan, id)`, with relative anchors `.below(gap)`, `.above(gap)`, `.beside_right(gap, size)`, `.beside_left(gap, size)`, `.stack_below(gap, size)`, `.align_left(inset, y_offset)`, and distributions `layout::row`, `layout::column`, `layout::spread_x`.
- **Sound effects (`psychopomp::sfx`)**: `TICK`, `SEND`, `FAILURE`, `LAUNCH`, `IMPACT`, `DEATH`, `GLITCH`, `MARK`, `BLOOM`, `SEVER`, `RESET`, `SUCCESS`, `CONFIRM`, `RISER`, `BOOM`, `WHOOSH`, `SPARKLE` with exact 48 kHz sample lengths. Use `sfx::IMPACT.beat("kill-impact", -5.0)` inside a score (or `sfx::IMPACT.play(&mut scene, "kill-impact", arrival, -5.0)` imperatively). Generated `psychopomp_media::Audio` resources also expose `audio.beat(gain_db)`.
- **`CueTime::reply()` / `reply_after(arrival)`**: a reply's gather begins once its request has landed (`340 ms` gather + `80 ms` reaction).
- **Named spring feels (`SpringPlan` / `score::Feel`) and timing tokens (`author`)**: `PANEL` (0.6 s, bounce 0.12), `CONTENT` (0.36 s, no bounce), `ENTER` (0.45 s, no bounce), `EXIT` (0.24 s, no bounce), `MOVE` (0.6 s, no bounce; `.with_thresholds(1e-5, 1e-5)` for weights), `SNAP` (0.3 s, no bounce), `POP` (0.32 s, bounce 0.18), `CAMERA` (1.6 s, critically damped), `LIVELY` (0.85 s, bounce 0.2), `Ease::DRAW` (`cubic-bezier(0.45, 0, 0.2, 1)`), `Ease::GLIDE` (`smootherstep`), `author::STAGGER` (120 ms), `author::CONTENT_LAG` (65 ms), and `author::DRAW_SECONDS` (0.42 s).

Scene Plan v2 scalar values may reference a component of a stable Semantic Target. The target's selector remains recipe-owned; for the hero, the editor recipe resolves logical code range IDs through `cosmic-text` before compiling highlight and pointer channels into the shared Timeline.

The plan runtime's recipes are listed under [Recipe Payloads And Channels](#recipe-payloads-and-channels). Planned audio lowers into exact script or layer placements for FFmpeg. Planned video is accepted only when a `video` actor consumes its media ID, and image media only when an `image` actor draws it; other unconsumed video and image media still return request errors. The Video Card maps the global scene clock through the media placement into source time, so cue and range renders do not restart footage. Editor and Video Card recipes independently produce RGBA content but delegate framing to the same private immediate-mode card compositor; this reuse does not add recursive presentation nodes to Scene Plan.

## Package Direction

```text
scenes/* ------------> psychopomp
                           ^
                           |
psychopomp-render ----------+
```

- `crates/psychopomp`: lightweight plans, authoring values, motion, composition, stable code, and validation
- `crates/psychopomp-render`: concrete renderer, encoder, development server, and CLI
- `scenes/*`: lightweight Rust Scene Programs

The default hero command embeds `scenes/hero/hero.plan.json` for compatibility. A workspace test regenerates the plan from `scenes/hero/src/lib.rs` and requires byte equality, so the checked artifact cannot drift from its Rust source.

`scenes/opencode-session-tool/opencode-session-tool.plan.json` is likewise checked against its Rust Scene Program. It demonstrates one planned split Vim/OpenCode Video Card, layered SFX, continuous card motion, nine live-capability text overlays, and named cue selection through the same renderer process.

The plan and authoring Modules intentionally share one lightweight crate. They should become separate crates only after another language, protocol consumer, or independent version lifecycle demonstrates that seam.
