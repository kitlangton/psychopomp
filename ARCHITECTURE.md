# Prototype Architecture

Psychopomp is organized around a small set of Modules whose Interfaces correspond to demonstrated change seams: code identity, media composition, motion trajectories, scalar tracks, rendering, and encoding.

Video export and native interactive presentation share Scene Plan preparation, scalar tracks, and visual recipes. Winit owns the native window and input; a wgpu surface displays Rust-rendered RGBA pixels. These supported paths have no browser playback layer or second animation implementation.

## Module Map

Lightweight crate (`crates/psychopomp/src`):

- `crates/psychopomp/src/lib.rs`: lightweight public library boundary used by Rust Scene Programs
- `crates/psychopomp/src/code.rs`: stable line identity, code documents and snapshots, validation, and sampled line placement
- `crates/psychopomp/src/composition.rs`: exact media time, immutable assets and clips, script/layer media placements, and cues
- `crates/psychopomp/src/editor.rs`: typed editor recipe data lowering stable inline parts and logical ranges into Code Transitions
- `crates/psychopomp/src/editor/compiled.rs`: shared validated catalog, reveal ranges, and legacy/keyed placement used by inspection and rendering
- `crates/psychopomp/src/editor/stability.rs`: GPU-free step deltas and heuristic common-text stability warnings
- `crates/psychopomp/src/editor/diff.rs`: Stepped Diff recipe builder (keep/add/remove lines, room-opening snapshots, Line Mark warnings)
- `crates/psychopomp/src/ide.rs`: Diagnostic, Hover Card, and Cursor payloads and handles (`DiagnosticActor`, `HoverActor`, `CursorActor`), Inlay Hint insertion (`insert_inlay`, `InlayHint`), and their GPU-free geometry: the wave, the caret blink, and hover layout
- `crates/psychopomp/src/highlight.rs`: line-local TypeScript highlighting into editor spans
- `crates/psychopomp/src/task.rs`: `TaskState` and typed planned Task state schedules
- `crates/psychopomp/src/grid.rs`: finite keyed product catalogs and semantic Grid Snapshots
- `crates/psychopomp/src/value.rs`: immutable Value Token recipe data for finite teaching diagrams
- `crates/psychopomp/src/component_prototype.rs`: provisional Typeset, width-text, Collection, Connector, rich-text, header, and Venn payloads
- `crates/psychopomp/src/author.rs`: typed Scene Plan builder, stable actor/channel handles, and timing vocabulary (`millis`, `stagger`, `spread`, `PlanTime::not_before`, named `SpringPlan` feels) for lightweight Scene Programs; `sample` and `destination` read a channel's authored value so helpers move from the pose written so far
- `crates/psychopomp/src/plan.rs`: versioned renderer-independent Scene Plan, Deck, and Reel values and structured validation
- `crates/psychopomp/src/plan/channels.rs`: exact scalar-event lowering and opt-in snapshot-destination reduction; raw event ordering remains distinct
- `crates/psychopomp/src/plan/wipe.rs`: Reel wipe values (direction, mid-frame holds, labels) and the closed-form divider position
- `crates/psychopomp/src/plan/transition.rs`: Composited Transition phases and their closed-form timing and geometry (travel curves, iris radius, rect-to-rect match camera, card and cube turns, cut envelopes)
- `crates/psychopomp/src/state.rs`: deterministic arbitrary-time discrete State Tracks
- `crates/psychopomp/src/playback.rs`: interruptible step destinations, continuous track retargeting, and a pausable local presentation clock
- `crates/psychopomp/src/timeline.rs`: explicit-time continuous Property Track compilation
- `crates/psychopomp/src/timeline/retarget.rs`: shared cancellation-safe numeric schedule for Playback and authored resting entrances
- `crates/psychopomp/src/motion.rs`: deterministic arbitrary-time analytic spring sampling with position and velocity
- `crates/psychopomp/src/transcript.rs`: word timing ingestion, word and phrase cue lookup, every occurrence of a phrase
- `crates/psychopomp/src/narration.rs`: narration clips (from `scripts/narrate.ts` manifests or any timed audio), scheduled back to back (`Narration::reading`, `Reading::new`), placed whole or as split ranges (`place_range`, `split`) as Script Clips, with panicking phrase lookups
- `crates/psychopomp/src/sfx.rs`: the `assets/` sound-effect catalog with exact lengths, placed whole as Layer Clips (`Sfx::play`)
- `crates/psychopomp/src/tone.rs`: semantic Tone roles shared by explainer recipes
- `crates/psychopomp/src/sequence.rs`: Sequence Diagram recipe values and row constructors, slot, header, and row-box geometry, validation, and the `SequenceActor` authoring handle
- `crates/psychopomp/src/caption.rs`: Caption recipe values and the `CaptionActor` authoring handle (typing, show, hide)
- `crates/psychopomp/src/chrome.rs`: an explainer film's fixed captions (header, chips, footer) at their shared positions
- `crates/psychopomp/src/rolling.rs`: Rolling Number recipe values, value tokenization, the closed-form roll compiler, and the `RollingNumberActor` handle (`roll`, show, hide)
- `crates/psychopomp/src/tree.rs`: Tree recipe values, JSONPath identity, the fold-driven pure layout, and the `TreeActor` handle (`open`, `close`, `highlight`, `set`, `scroll_to`, `reveal`, show, hide)
- `crates/psychopomp/src/axis.rs`: `AxisPlan` (range, ticks, label, unit), `nice_ticks`, and tabular tick labels shared by Plot and Lanes
- `crates/psychopomp/src/plot.rs`: Plot recipe values (frame, axes, sampled series with optional exact slopes, marks), interpolation, strict channel matching, and the `PlotActor` handle (`show`, `draw`, `fade`, `ride`, `velocity`, `mark`)
- `crates/psychopomp/src/lanes.rs`: Lanes recipe values (time axis, lanes with keys and sparklines, cues), `LanesPlan::from_scene_plan`, and the `LanesActor` handle (`show`, `scrub`, `emphasize`)
- `crates/psychopomp/src/callout.rs`: Callout recipe values, anchor edges, leader shape and frame-avoiding layout, and the `CalloutActor` handle (`show`, `hide`, `move_to`, `emphasize`)
- `crates/psychopomp/src/readout.rs`: `ReadoutFormat` and the channel-driven odometer (`odometer`, `cells`, `wheel_rate`) shared by Meters, Benchmark Bars, and checklist counts
- `crates/psychopomp/src/checklist.rs`: Checklist recipe values, row layout, `ItemPose` (the look derived from an item's four channels), strict channel matching, and the `ChecklistActor` handle (`reveal`, `start`, `resolve`, `skip`, show, hide)
- `crates/psychopomp/src/meter.rs`: Meter recipe values (ring, countdown, bar), angle and threshold-tone math, and the `MeterActor` handle (`set`, `sweep`, `countdown`, `flash`, show, hide)
- `crates/psychopomp/src/bars.rs`: Benchmark Bars recipe values, row geometry, delta chip text, stable `ranking` and crossing `paint_order`, and the `BarsActor` handle (`grow`, `set`, `sort`, `reveal_rows`, `reveal_deltas`, show, hide)
- `crates/psychopomp/src/subtitles.rs`: Subtitles recipe values (from a placed narration clip), page chunking and line balancing, and the time-sampled page, word-ink, pill, and backing poses
- `crates/psychopomp/src/face.rs`: the typefaces a Stage label or Subtitles can be set in (`Face`): bundled CommitMono, regular/bold Helvetica, or an installed display face
- `crates/psychopomp/src/confetti.rs`: Confetti recipe values and the `ConfettiActor` handle (`burst`)
- `crates/psychopomp/src/lens.rs`: Lens recipe values, the sampled `Glass` (outline, rim bend, source mapping, bounds), and the `LensActor` handle (`show`, `hide`, `move_to`, `slide`, `magnify`, `resize`, `focus`)
- `crates/psychopomp/src/video.rs`: Video Card recipe values (footage size, card rect, title), focus-window math, placement helper, and the `VideoActor` handle (`fly_in`, `focus`, `unfocus`, `hide`)
- `crates/psychopomp/src/stage.rs`: Stage elements and their builder constructors, strict channels and their one table of defaults (`StagePlan::channel_default`), orb geometry, form shapes and morphs (`form_points`, `morph_point`), the packet clock and relay legs (`stage::packet`), `reply_after`, and the `StageActor` authoring handle: primitives (`to`, `spring`, `ease`, `glide`, `bounce`, `set`, `fade_in`/`fade_out`, `clock`/`clock_for`), beats (`settle_in`, `connect`/`connect_contacting`/`disconnect`, `send`/`send_arriving`, `relay`, `morph`, `hit`, `kick`, `jolt`, `twang`, `land`, `orb_in`, `glitch`, `rewind`, `unburst`, `shock_kick`, `resolve_spinner`, `swap_status`, `swap_labels`, `dim`, `halo`, `ring_timer`), effect beats (`zap`, `charge`, `hum`, `dissolve`, `materialize`, `scan`, `raise`, `lower`), and `camera` for the `CameraRig`
- `crates/psychopomp/src/stage/camera.rs`: the Stage `Camera` pose and projection (`project`, `project_rect`; pan, dolly, orbit about a pivot, zoom, roll, billboard screen boxes, framing) shared by the renderer, callouts, and Scene Programs, and the `CameraRig` shots (`frame`, `move_to`, `establish`, `push_in`, `pull_back`, `drift`, `whip`, `crash`, `orbit`, `dolly_zoom`, `roll`, `focus_on`, `aperture`, `follow`, `release`, `handheld`)
- `crates/psychopomp/src/effects/`: GPU-free special-effect clocks and particle poses (combustion, lightning, dissolve, shield, surface, shake, the status spinner, confetti); shared dynamics stay in `psychopomp::math::dynamics`
- `crates/psychopomp/src/window.rs`: the text surfaces' shared Window channels (`opacity`, `x`, `y`, `scale`, `content`), title-bar height, `settle_in`/`dismiss`, and dot-free sub-channel IDs
- `crates/psychopomp/src/terminal.rs`: Terminal recipe values, the reveal-driven pure layout with its scroll floor, deterministic `keystrokes`, and the `TerminalActor` handle (`type_command`, `prompt`, `idle`, `print`, `stream`, `spin`, `resolve`, `highlight`, `clear`, `scroll_to`)
- `crates/psychopomp/src/chat.rs`: Chat Thread recipe values (Slack and bubbles styles), per-style geometry, the composer-anchored pure layout over renderer-measured `ChatMetrics`, typing-dot poses, and the `ChatActor` handle (`typing`, `say`, `stream`, `react`, `highlight`, `stamp`)
- `crates/psychopomp/src/lower_third.rs`: Lower Third recipe values, bar and text geometry, and the `LowerThirdActor` handle (`show`, `hide`)
- `crates/psychopomp/src/changed_files.rs`: Changed Files recipe values, GitHub's `diffstat`, fixed row slots, the totals schedule, and the `ChangedFilesActor` handle (`reveal`, `reveal_row`, `focus`, `unfocus`, `highlight`, `scroll_to`)
- `crates/psychopomp/src/anchor.rs`: the shared Anchor model: `Edge` (re-exported as `CalloutSide`), `AnchorPlan` (point, stage, editor, with offsets), the borrowed `AnchorTarget` every renderer path resolves, weight channel names, validation, `blend`, and `move_to`
- `crates/psychopomp/src/text.rs`: typed `text` recipe values in the hand-built JSON shape and the `TextActor` handle (`show`, `hide`, `show_during`, `swap`, `move_to`)
- `crates/psychopomp/src/image.rs`: Image recipe values (bare or framed, title, radius, anchors), the image placement helper, and the `ImageActor` handle (`fly_in`, `hide`, `move_to`)
- `crates/psychopomp/src/footage.rs`: Footage: the `Clip` (trim, rate, hold/loop/bounce, reverse, freeze, decode rate and width) and its exact nanosecond playhead mapping, `Fit` and focus windows, `Mask`, color `Treatment`, the `footage` overlay recipe (`FootagePlan`), placement and audio helpers, the `Playhead` handle on a `time` channel (`freeze`, `play`, `ramp`, `retime`, `seek`, `stutter`), the `FootageActor` handle (`fly_in`, `toss_in`, `glide`, `move_to`, `focus`, `drift`, `treat`, `hide`, `audio`), and `probe` (ffprobe at authoring time)
- `crates/psychopomp/src/lipsync.rs`: Sprite Sheet lip sync: letters and pauses to `Mouth` visemes on a fixed sprite tick, deterministic blinks, the expression × mouth frame layout, and the `Sprite` handle that cuts a footage playhead to frames
- `crates/psychopomp/src/footage/layout.rs`: GPU-free collage layouts returning `Tile`s: `grid`, `masonry`, `scatter`, `pile`, `filmstrip`, and `by_distance` for ripple staggers
- `crates/psychopomp/src/math.rs` and `math/`: shared motion and geometry math (glam vectors, lerp/remap/smoothstep, easing, closed-form dynamics such as the settling spring, arc-length curves, shape ports and connectors, rounded-box distance fields, thin-surface glass optics, deterministic hash)

Media crate (`crates/psychopomp-media/src`), Generated Resources for Scene Programs:

- `crates/psychopomp-media/src/media.rs`: `Media`, the reconciler (declare, resolve against the lock, generate, `finish`), `Mode`, `Report`, and the `Audio` resource handle (`place`, `play`, `derive`)
- `crates/psychopomp-media/src/spec.rs`: `Voice`, `Line`, `Sound`, `Effect`, and the canonical `Spec` whose SHA-256 is the Resource Key; effect retiming
- `crates/psychopomp-media/src/lock.rs`: the Media Lock schema, atomic writes, and its one-word-per-line formatter
- `crates/psychopomp-media/src/words.rs`: words from provider character alignment (directions filtered), from Whisper, and estimated from text
- `crates/psychopomp-media/src/studio.rs`: the `Generator` seam and `Studio`, the real one (providers, loudness, Whisper, `say`), plus credentials from the environment or `.env`
- `crates/psychopomp-media/src/eleven.rs` and `fish.rs`: ElevenLabs Text to Speech/Dialogue/Sound Effects and Fish Audio requests
- `crates/psychopomp-media/src/ffmpeg.rs`: loudness, sound-effect finishing, silence, effects, and exact durations
- `crates/psychopomp-media/src/adopt.rs` and `main.rs`: adopting `scripts/narrate.ts` narration into a lock; the `adopt` and `show` commands

Renderer crate (`crates/psychopomp-render/src`), plan runtime:

- `crates/psychopomp-render/src/main.rs`: command parsing and the default hero render
- `crates/psychopomp-render/src/plan_runtime.rs`: Scene Plan inspection, validation, rendering, and persistent JSON server
- `crates/psychopomp-render/src/plan_runtime/preflight.rs`: owned typed recipe inputs, root selection, references, and native eligibility before resources
- `crates/psychopomp-render/src/plan_runtime/generated.rs`: generated-channel reservation/insertion, including the explicit Task position override
- `crates/psychopomp-render/src/plan_runtime/attachments.rs`: private companion-track compilation for layout-aware semantic coordinates
- `crates/psychopomp-render/src/plan_runtime/delivery.rs`: PNG and MP4 delivery from a prepared scene
- `crates/psychopomp-render/src/plan_runtime/still.rs`: plan or reel delivery as video, single frames, or compared frame snapshots
- `crates/psychopomp-render/src/plan_runtime/render_queue.rs`: machine-wide render slots (advisory file locks in the temporary directory, `PSYCHOPOMP_RENDER_SLOTS`) that `plan render` waits on, so concurrent renders queue instead of contending
- `crates/psychopomp-render/src/plan_runtime/verify.rs`: `psychopomp verify`: builds and runs the Scene Programs in the root `verify.json`, loads each plan, renders its key frames on one renderer, and stores or compares plans and pixels; `verify/manifest.rs` parses the manifest and `verify/diff.rs` summarizes plan changes by segment, actor, channel, cue, and media ID
- `crates/psychopomp-render/src/plan_runtime/reel.rs`: Reel preparation, layer mixing, media retiming, and reel frame/video delivery
- `crates/psychopomp-render/src/plan_runtime/proof.rs`: small test primitives for recipe-specific pixel and interruption checks
- `crates/psychopomp-render/src/plan_runtime/presentation.rs`: native winit window, step navigation, and smooth/pixelated display filtering
- `crates/psychopomp-render/src/plan_runtime/presentation/worker.rs`: persistent render worker with bounded in-flight sampling and immutable timeline revisions
- `crates/psychopomp-render/src/plan_runtime/presentation/scheduler.rs`: GPU-free request eligibility, invalidation, completion freshness, and deadlines
- `crates/psychopomp-render/src/plan_runtime/presentation/debug.rs`: sample-coherent optional native motion diagnostics
- `crates/psychopomp-render/src/plan_runtime/presentation/gpu.rs`: native wgpu surface and GPU-backed smooth/pixelated frame presentation
- `crates/psychopomp-render/src/plan_runtime/presentation/preferences.rs` and `benchmark.rs`: saved theme preference and the native interruption benchmark
- `crates/psychopomp-render/src/plan_runtime/editor.rs`: concrete editor and attached pointer Scene Plan recipe
- `crates/psychopomp-render/src/plan_runtime/task.rs`: Task state schedules lowered into interruptible scalar visual destinations
- `crates/psychopomp-render/src/plan_runtime/grid.rs`: GPU-free keyed grid layout and continuous destination compilation
- `crates/psychopomp-render/src/plan_runtime/grid/table.rs` and `grid/disclosure.rs`: fixed-anchor table placement over the same grid catalog, and growth-edge label disclosure
- `crates/psychopomp-render/src/plan_runtime/value.rs`: Value Token validation and ordinary scalar-channel sampling
- `crates/psychopomp-render/src/plan_runtime/component_prototype.rs`, `rich_text.rs`, `venn.rs`: provisional overlay preparation
- `crates/psychopomp-render/src/plan_runtime/header.rs`: header word tracks and opt-in resting-entrance delays with cancellation
- `crates/psychopomp-render/src/plan_runtime/sequence.rs` and `caption.rs`: strict-channel preflight for the explainer overlays
- `crates/psychopomp-render/src/plan_runtime/rolling.rs`: Rolling Number preflight and compilation
- `crates/psychopomp-render/src/plan_runtime/tree.rs`: Tree per-path channel preflight
- `crates/psychopomp-render/src/plan_runtime/plot.rs` and `lanes.rs`: Plot and Lanes strict-channel preflight
- `crates/psychopomp-render/src/plan_runtime/ide.rs`: strict preflight of Diagnostics, Hover Cards, and Cursors attached to the editor root, and their per-sample frames from measured targets
- `crates/psychopomp-render/src/plan_runtime/viz.rs` and `viz/`: the visualization overlays as one group (`VizInputs`, `PreparedViz`) with strict-channel preflight per recipe in `viz/{checklist,meter,bars,subtitles,confetti}.rs`
- `crates/psychopomp-render/src/plan_runtime/callout.rs`: callout poses from anchors validated and resolved through the shared `plan_runtime/anchor.rs`
- `crates/psychopomp-render/src/plan_runtime/lens.rs`: Lens preflight, anchor validation, and per-sample glass from blended anchors
- `crates/psychopomp-render/src/plan_runtime/footage.rs`: Video Card, image, footage-overlay, and Stage-footage preflight lowered into one `FootagePlan` path; decode-size choice; the per-sample playhead-to-frame mapping; cached overlay layers; Stage footage frames
- `crates/psychopomp-render/src/plan_runtime/anchor.rs`: shared anchor validation against the root, per-sample resolution (`render::stage_anchor`, `PreparedEditor::anchor`), and weighted pinning for every pinnable overlay
- `crates/psychopomp-render/src/plan_runtime/stage.rs`: Stage root preflight and preparation
- `crates/psychopomp-render/src/plan_runtime/lower_third.rs`: Lower Third strict-channel preflight
- `crates/psychopomp-render/src/plan_runtime/terminal.rs`, `chat.rs`, and `changed_files.rs`: text-surface preflight (per-line, per-message, per-reaction, and per-row channels checked against their IDs and kinds); chat preparation measures wrapped text, changed-files preparation measures columns and compiles its rolling totals

Renderer crate, pixels and delivery:

- `crates/psychopomp-render/src/render.rs`: concrete headless `wgpu` renderer and sprite compositor
- `crates/psychopomp-render/src/scene.wgsl`: editor geometry and focus shader
- `crates/psychopomp-render/src/render/theme.rs`: named native/export paint palettes; no layout or motion
- `crates/psychopomp-render/src/render/fonts.rs`: bundled CommitMono faces, and the installed faces a `Face` selects
- `crates/psychopomp-render/src/render/text.rs` and `text/raster.rs`: typed plain-text cache and exact native glyph rasterization
- `crates/psychopomp-render/src/render/rich_text.rs`: bounded Markdown shaping, decoration, and theme-aware glyph cache
- `crates/psychopomp-render/src/render/line_marks.rs`: coverage union for editor diff backgrounds
- `crates/psychopomp-render/src/render/ui.rs`: private bounds and inset primitives for pixel UI
- `crates/psychopomp-render/src/render/ui/card.rs`: shared immediate-mode RGBA composition and projected card presentation used by editor and Video Card producers
- `crates/psychopomp-render/src/render/task.rs` and `task/content.rs`: concrete Effect Task recipe, sampled content poses, and compositing
- `crates/psychopomp-render/src/render/grid.rs`: opaque connected 3D grid, sampled-bounds centering, and cached symbols/labels
- `crates/psychopomp-render/src/render/grid/edges.rs` and `grid/palette.rs`: centered screen-space grid strokes with shared-edge coverage union, and native line-color auditions
- `crates/psychopomp-render/src/render/value.rs`: Value Token tiles using shared card coverage and cached fractional text
- `crates/psychopomp-render/src/render/component_prototype.rs`, `venn.rs`, `header.rs`: provisional measured glyph runs and Bezier ink, rounded-set hatching, and fixed-edge header rises with reflections
- `crates/psychopomp-render/src/render/sequence.rs` and `caption.rs`: Sequence Diagram and Caption pixels
- `crates/psychopomp-render/src/render/rolling.rs`: Rolling Number masked, smeared wheels
- `crates/psychopomp-render/src/render/tree.rs`: Tree rows, chevrons, guides, highlight bars, and rolling values
- `crates/psychopomp-render/src/render/plot.rs`, `lanes.rs`, and `chart.rs`: Plot and Lanes pixels over the shared chart ink (snapped labels, axis rulers, dashes, dots, diamonds, readout tabs)
- `crates/psychopomp-render/src/render/callout.rs`: callout mark, leader, and label pixels
- `crates/psychopomp-render/src/render/ide.rs`: selection, Inlay Hint chip, diagnostic wave and gutter icon, caret, and Hover Card pixels on the flat editor surface
- `crates/psychopomp-render/src/render/viz.rs` and `viz/`: checklist, meter, bars, subtitles, and confetti pixels over the chart ink, plus the shared Readout painter (`viz/readout.rs`), a weighted stroke, arcs, and rotated rectangles
- `crates/psychopomp-render/src/render/lens.rs`: Lens pixels: linear-light refraction of the composed frame, rim softening, specular light, and contact shadow
- `crates/psychopomp-render/src/render/footage.rs`: projected footage pixels for Video Cards, images, and footage (fit and focus window, mask, color treatment, title bar), drawn directly or as a cached layer, and the banded layer blend
- `crates/psychopomp-render/src/render/image.rs`: PNG/JPEG/WebP decoding by signature and premultiplied halving
- `crates/psychopomp-render/src/render/wipe.rs`: Reel wipe pixels: antialiased split, divider line and shadow, riding labels
- `crates/psychopomp-render/src/render/window.rs`: the text surfaces' Window shell (composed once per pose through the projected card and cached as a layer), title bar, CommitMono span runs, and weighted strokes
- `crates/psychopomp-render/src/render/terminal.rs`, `chat.rs`, and `changed_files.rs`: Terminal, Chat Thread, and Changed Files pixels
- `crates/psychopomp-render/src/render/lower_third.rs`: Lower Third pixels: the accent bar and sans name and role clipped at a stationary edge
- `crates/psychopomp-render/src/render/transition.rs` and `transition/`: Composited Transition pixels in linear light: `travel` (push, slide, whip), `reveal` (iris, ink), `turn` (match, flip, cube), `light` (glitch, flash, light leak)
- `crates/psychopomp-render/src/render/stage.rs`, `stage.wgsl`, `stage_post.wgsl`: Stage primitives, HDR bloom, and composite; `PSYCHOPOMP_SHADER_DIR` loads the WGSL live
- `crates/psychopomp-render/src/render/effects/*.wgsl`: binding-free noise, combustion, pressure, rewind, lightning, dissolve, shield, and scan Modules, composed by the Stage shaders; see `EFFECTS.md`
- `crates/psychopomp-render/src/render/debug.rs`: optional native debug HUD
- `crates/psychopomp-render/src/video.rs`: FFmpeg-decoded seekable RGBA frame cache for input video and image sequences, keyed by source content and decode contract
- `crates/psychopomp-render/src/footage.rs`: the footage store: every source a plan shows, opened once and shared, with frames read through one bounded LRU
- `crates/psychopomp-render/src/render/bands.rs`: row-band parallelism for CPU rasterisation; each row is painted once with serial arithmetic, so output does not depend on thread count, and a fill takes threads only when its area (rows times columns) is worth them
- `crates/psychopomp-render/src/render/flat_key.rs`: the flat editor key, every input to the flat editor frame compared exactly (floats by bits, plan text by `Eq`); an equal key reuses the last flat frame and its card layers, and the shapes pass separately reuses its readback while its uniform bytes are equal
- `crates/psychopomp-render/src/exposure.rs`: delivery dimensions, shutter samples and weights, linear-light accumulation, and encoding a timeline one exposed frame at a time; a `FrameWriter` thread owns the FFmpeg encoder behind a two-frame channel, so encoding overlaps the next frame's render in order
- `crates/psychopomp-render/src/encode.rs`: concrete FFmpeg subprocess, raw RGBA protocol, and compiled audio placement

Scene Programs (`scenes/`), each emitting a Scene Plan, Deck, or Reel:

- `scenes/agent-demo/`: smallest Scene Program: one title card, a state channel, and cues
- `scenes/hero/`: canonical editor-heavy Scene Program and generated plan used by the default render command
- `scenes/quark-before-after/`: compact narrated Solid Store versus Quark keyed-identity tutorial
- `scenes/effect-succeed-slides/`: Effect Institute code-reveal adaptation proving manual presentation and video export from one source
- `scenes/interactive-showcase/`: four-slide native deck covering inline reveals, Task lifecycle/retry, parallel Tasks, and keyed code edits
- `scenes/keyed-grid/`: native row/table/3D-layer growth and product-reassociation proof
- `scenes/data-modeling/`: seven-slide types/cardinality, finite correspondence, joystick, sum/product, and illegal-state adaptation
- `scenes/component-prototypes/`: provisional reusable Typeset, Collection, and Connector showroom (plus the `--slideshow` typography deck); payloads and adapters remain in the three `component_prototype.rs` modules until visual approval
- `scenes/opencode-session-tool/`: rapid-fire OpenCode v2 hot-reload proof using a split Vim/OpenCode Video Card, layered SFX, and text
- `scenes/opencode-jr-architecture/`: narrated Stage-film teaching reel of the OpenCode Jr Slack bot, with one condensed code zoom
- `scenes/pr-walkthrough/`: narrated PR explainer reels; `src/film.rs` is the shared PR-film template (header, chips, behavior and code segments)
- `scenes/shape-of-openness/`: flat editorial design film; exact SVG artwork, Helvetica, thick graphic strokes, explicit pigments, image plates, a hand stencil composited from registered footage layers, and phrase-timed transformations on the Stage
- `scenes/config-migration/`: narrated reels of two OpenCode config pull requests built on the PR-film template
- `scenes/pr-50231/`: narrated Stage film of the Effect rc.112 → rc.117 upgrade and the three behaviors the compiler could not catch
- `scenes/rolling-number/`: Rolling Number showroom: roll up and down, a mid-roll redirect, a carry into a new place, and a shrink
- `scenes/tree/`: Tree showroom: the plan `agent-demo` emits, opened node by node, scrolled, highlighted, a value rolled, then folded
- `scenes/charts/`: Plot and Lanes showroom: critically damped vs bouncy springs, a riding playhead with its velocity arrow, a retarget beside a restart from rest (all from compiled Property Tracks), then Lanes of the plot's own channels
- `scenes/callouts/`: Callout showroom reel: callouts pinned to Stage cards through a dolly, a jolt, and a glide between anchors, then to a code range that moves as lines are inserted and the panel zooms
- `scenes/diagnostics/`: IDE annotation showroom: an Effect program's error wave, inferred-type Inlay Hint, Hover Card, caret selection, and a Stepped Diff fix that the error rides down with before it clears
- `scenes/video/`: Video Card showroom: a screen recording flies in, zooms into the prompt, and back out
- `scenes/compare/`: wipe showroom: a held before/after wipe between two Stage frames, then a plain wipe
- `scenes/generated-media/`: Generated Resource showroom: an ElevenLabs line, a Fish Audio chant whose every "balls" spawns an orb and a generated pop, and a pitched-down derivation, all in `media.lock.json`
- `scenes/camera/`: camera showroom: establish, frame, follow a packet, rack focus, orbit an orb, dolly zoom on an impact, whip, handheld drift, and a push-in, all `CameraRig` shots
- `scenes/effects-showroom/`: Stage effects reel: a charged build zaps a deploy, a shield blocks an attack and passes a request, a stale config burns away and its replacement materializes and is scanned, a live link hums
- `scenes/stage-forms/`: Stage diagram vocabulary showroom: shapes, icons, and arrowed paths; a packet relaying through a stop; a dot-matrix plane morphing into a tumbling cube and a sphere over a slab; a cube that bursts
- `scenes/text-surfaces/`: text-surface showroom reel: an agent, introduced by a Lower Third, in a Terminal session that overflows, scrolls, and clears; a Slack-style Chat Thread reacting while the agent streams its fix; the bubbles style; and a pull request's Changed Files with rolling totals and focus
- `scenes/viz-components/`: visualization showroom reel: a CI checklist that fails, retries, and celebrates with confetti; a countdown ring, a gauge, and an upload bar; a before/after benchmark that grows and re-sorts; and word-timed subtitles over a narrated Stage clip
- `scenes/anchors/`: Anchor showroom reel: a caption, a Rolling Number, text labels, a callout, and a framed image riding Stage cards through a dolly, a jolt, and glides between anchors; then a caption and a Rolling Number on code ranges while lines insert and the panel zooms; then a cursor caption, a counter, and a callout on Sequence Diagram rows and headers as the diagram slides
- `scenes/loupe/`: Lens showroom reel: a loupe reads code ranges (glide, capsule scan, floating focus), then follows a Stage card's changing status through a dolly
- `scenes/transitions/`: transitions showroom: every reel transition between Stage, code, and title frames, each named in a chip
- `scenes/high-council/`: High Council demo reel: a Civilization II-style FMV council screen whose six portraits lip-sync to designed ElevenLabs voices as Sprite Sheets, with chrome, speech box, agenda, and banners drawn by `scripts/frames.py` as frame-cut image sequences, and a Stage segment explaining the pipeline
- `scenes/footage/`: Footage showroom reel: a masonry wall of stills and clips tossed in, one clip freezing, ramping, and stuttering while the rest recede to reference footage; footage at Stage depths under an orbiting camera with a circular clip pinned to a card; a pile of prints drawn into a filmstrip (`--bench` writes a twenty-clip measuring wall)

## Scene Programs And Rendering Compile Separately

The workspace has two demonstrated package seams. `crates/psychopomp` is a lightweight library containing authoring values, versioned Scene Plans, validation, exact composition time, continuous Property Tracks, discrete State Tracks, and stable code identity. `crates/psychopomp-render` contains `wgpu`, `cosmic-text`, video decoding, FFmpeg encoding, built-in renderer recipes, and the CLI. `crates/psychopomp-media` generates audio for the Scene Programs that declare it; it carries the HTTP and TLS dependencies that neither the lightweight crate nor the renderer needs.

```text
Rust Scene Program -> Scene Plan -> persistent psychopomp-render process
```

A Scene Program remains ordinary Rust and may perform arbitrary calculations before emitting a plan. The Scene Plan is compiled output rather than a replacement authoring language. Its renderer-independent Interface contains stable actors, continuous channels, state channels, exact cues, and media placements. Renderer Recipe payloads remain opaque to the core and are interpreted only by concrete adapters in `psychopomp-render`.

`crates/psychopomp-render/src/plan_runtime.rs` validates and inspects plans without initializing a GPU, renders exact global Render Windows, and provides a newline-delimited JSON server that retains the GPU device and font caches across requests. Intersecting media is trimmed and rebased to output zero while visual sampling remains on the original global clock. Explicit event selection uses a `f64` seconds clock derived from serialized integer nanoseconds; scalar spring positions and velocities remain `f32`.

`plan_runtime/preflight.rs` owns decoded recipe inputs, exclusive root selection,
references, and native eligibility before resource creation. Resource preparation
consumes those inputs to construct one complete `PreparedPlan`; it does not install
independent optional roots afterward. `CompiledPlan` keeps numeric/state/audio
compilation separate from resource ownership. Plain text and bounded Markdown are
parsed once, not interpreted from actor JSON during sampling. Typed current-state
projections consider only observable values; raw State Tracks retain distinct
equal-time predecessors for snapshot semantics and exact visual keys.
State Tracks keep predecessor clones rather than borrowing another segment's
current payload: `Clone` may deliberately isolate interior-mutable values.

Shared rules have narrow owners rather than a recipe registry:

- `psychopomp::plan::compile_channels` lowers scalar events exactly. Its callers
  choose channels, resolve semantic scalars, and retain diagnostic context.
- `destination_channel` and `effective_snapshots` opt snapshot recipes into final
  equal-time destinations and unchanged-target suppression. Raw continuous events
  and distinct State Track transitions keep their authored ordering.
- `plan_runtime/generated.rs` reserves generated channel IDs and actor/property
  pairs. Task x/y is the sole explicit authored override, not last-writer-wins.

Planned audio and visual media share the same exact source and timeline ranges but have different concrete consumers. Audio lowers into `Composition` and the FFmpeg encoder. Video and image placements are accepted only when footage consumes them (a `video`, `image`, or `footage` actor, or a Stage `footage` element); unconsumed video and image media remain errors. Video never reaches the audio-only encoder, and there is no generic video layer or media graph: footage audio is an ordinary audio placement of the same file (see Footage).

Scene Plan v2 allows a scalar initial value or event target to reference one component of a stable Semantic Target plus an offset. The core validates target identity, component names, and finite offsets without interpreting the target selector. During renderer preparation, the actor's concrete recipe resolves selectors into geometry; only then are ordinary numeric Property Tracks compiled. This keeps font measurement renderer-owned while preserving pointer and highlight trajectories as inspectable general channels.

`crates/psychopomp/src/timeline.rs` compiles explicit-time events into deterministic scalar property tracks. Its renderer-neutral Interface drives the hero scene's panel, code-layout, code-content, focus, highlight, and pointer properties while preserving velocity across spring retargeting.

```text
Code snapshots ──> CodeTransition.sample(progress) ──┐
                                                     ├─> EditorFrame
Motion cues ─────> Spring.sample(state, target, t) ──┘       │
                                                            v
                                                 HeadlessRenderer.render
                                                            │ RGBA
                                                            v
                                                 FfmpegEncoder.write_frame
```

## Stable Code Is Independent of Rendering

`crates/psychopomp/src/code.rs` owns stable line IDs, code documents, snapshots, validation, and line placement. Its Interface is `CodeTransition::compile` followed by `CodeTransition::sample`.

The Module does not know about glyphs, GPUs, colors, FFmpeg, or absolute screen coordinates. This gives the identity and layout rules locality and makes snapshot behavior testable without external systems.

The Effect Institute corpus and hand-authored lesson ports demonstrated flat stable inline identity. `CodeLine` therefore owns ordered `InlinePart` values and validated logical semantic ranges while retaining a derived flattened span view for the current renderer. The model intentionally stops before a recursive slot AST, automatic diffing, or glyph geometry.

Maximum Stability uses authored identity, not an inferred text diff. `CodeTransition::compile` still matches line IDs between two row maps. The planned editor preserves that legacy path when `snapshots` is empty. A timed `EditorSnapshotPlan` schedule instead lowers through `CompiledEditor::snapshot_channels` into private per-line y and opacity channels. Lines absent from both endpoints remain available between them; equal-time snapshots coalesce before layout, and unchanged row/presence targets do not restart. Native playback includes those generated channels in ordinary destination compilation, so insertion, removal, reordering, and re-entry use the same velocity-preserving tracks as video.

`psychopomp::editor::compiled` owns the validated catalog, legacy/keyed placement,
and reveal span/part ranges used by both inspection and renderer preparation.
Keyed sampling does not fabricate a two-snapshot transition.

`psychopomp/src/editor/stability.rs` provides GPU-free `inspect_steps`, exposed by `psychopomp plan steps` and the persistent server's `steps` command. It reports before/after text, changed-part markers, retained-line movement, unsettled or partial holds, and heuristic common-text warnings for exchanged parts or replaced lines. It never assigns identity automatically. The ordinary Scene Plan JSON diff remains a structural plan diff, not an animation-stability analysis.

## Motion Carries Position and Velocity

`crates/psychopomp/src/motion.rs` owns the analytic damped-spring equation. Its Interface accepts an initial motion state, target, and arbitrary elapsed time.

This Interface provides leverage beyond easing: deterministic out-of-order sampling and momentum-preserving interruptions use the same Implementation. Explicit-time Scene Plan events compile through `crates/psychopomp/src/timeline.rs` into one segment representation.

Each compiled spring has a deterministic settling time. Underdamped motion uses decaying amplitude bounds; critical damping checks the remaining extrema of its exponential-polynomial solution. After that time the segment stays exactly at rest, even when another channel resumes the local clock. The event boundary retains the full initial state.

## Composition Owns Cross-Media Time

`crates/psychopomp/src/composition.rs` keeps immutable source assets separate from their uses in an edit. A `Clip` selects an exact source range; a `MediaPlacement` places that range on the output timeline without modifying the asset. It distinguishes transcript-bearing script clips from accompanying layer clips such as sound effects, music, and B-roll.

Media time is stored as integer nanoseconds, so repeated source-range edits retain exact boundaries.

`crates/psychopomp-render/src/video.rs` is the narrow input-video boundary. FFmpeg decodes a video (or a printf-pattern image sequence through `image2`, or a VP8/VP9 WebM's side-channel alpha through libvpx) into an ignored seekable straight-alpha RGBA cache under `target/psychopomp-cache/footage`, named by the bytes of every file it reads and the decode contract (size, rate, decoder, trimmed range); fixed-size frame offsets then provide deterministic arbitrary-time sampling without codec bindings or retaining the decoded recording in memory. Any actor or plan showing the same source at the same size shares one decode; the directory is safe to delete.

The decoder receives null stdin, never the persistent server's request stream.
A failed seek/read invalidates the in-memory frame identity before it can expose
partially overwritten bytes as a cache hit.

## Transcript Cues Drive Choreography

`crates/psychopomp/src/transcript.rs` ingests word timing sidecars and resolves exact word occurrences into cue ranges.

Transcript parsing does not understand code, actors, or rendering. It only connects semantic words to the shared media clock.

`Transcript::phrase` and `phrase_after` match consecutive normalized words (case,
punctuation, and number words versus digits are ignored); `phrases` finds every
occurrence, so a chant can cue one beat per word. `psychopomp::narration` places
a clip as a Script Clip whose phrase lookups return plan-clock times. Clips come
from two producers. `scripts/narrate.ts` writes loudness-normalized MP3s, Whisper
word timings, and a manifest of exact durations (`--draft` uses macOS `say`);
`Narration::load` reads it. `psychopomp-media` produces the same clips from
declarations in the Scene Program itself (see below). The `pr-walkthrough` Scene
Program keys every reveal to a phrase and fails with the clip and phrase when
narration no longer says it.

## Generated Media Is Reconciled

`crates/psychopomp-media` treats generated audio the way infrastructure-as-code
tools treat cloud resources: the Scene Program declares desired state, a lock
records actual state, and each run acts on the delta. The declaration is the
whole interface:

```rust
let media = Media::open(env!("CARGO_MANIFEST_DIR"))?;
let kit = Voice::eleven(KIT).v4().stability(0.2);
let hush = media.say("hush", &kit, "[soft ASMR whisper] Oh... I hear you like... balls.")?;
let pop = media.sfx("pop", "a single soft glassy pop", seconds(0.5))?;
let demon = hush.derive(Effect::pitch(-6.0))?;
media.finish()?;
let said = hush.place(&mut scene, SECOND); // a Script Clip; `said.at_every("balls")`
```

Each declaration lowers to a canonical `Spec`, serialized with sorted keys and
absent options omitted, so builder order and later optional fields never rekey
existing resources. The Resource Key is the first 64 bits of its SHA-256. The
spec includes post-processing and alignment as versioned strings: changing the
loudness pass or the Whisper model changes them and so regenerates honestly.
Keys identify recipes, not bytes; providers are not deterministic, so a key
match, not a content hash of the audio, decides reuse.

Declarations resolve eagerly, one at a time, like Alchemy rather than a
Terraform plan/apply: the call returns an `Audio` with the real duration and
words, so the choreography after it uses real timings in the same run. The
reconciler checks the lock entry for the id (`=` when its key matches and the
file exists), then any entry with the same key (renamed or duplicated ids
reuse audio), and otherwise generates (`+` or `~`, naming the changed spec
fields). Each generation checkpoints the lock, so a later failure keeps
paid work. `finish` runs after the last declaration: ids the lock holds but nothing
declared are orphans (`-`), deleted only in prune mode along with unreferenced
store files. Files outside the store (adopted narration) are never deleted.

Offline modes still run the whole Scene Program. A plan cannot pause on
"known after apply" values, so missing speech gets estimated words (each spoken
word of the text, 0.4 s apart) and missing sound its declared length; every
phrase lookup still resolves, the delta and its estimated characters and sound
seconds are complete, and `finish` fails before the scene writes its plan.
Draft mode substitutes macOS `say` for speech and silence for sound effects;
the lock then holds the draft's key, which a draft run accepts and an apply run
replaces.

`Generator` is the one seam: `Studio` produces real audio and the tests' fake
records jobs without HTTP, ffmpeg, or Whisper. ElevenLabs and Fish Audio are
two concrete branches inside `Studio`, not plugins. ElevenLabs Text to Speech
with timestamps returns character alignment; bracketed directions and lone
pauses are filtered and words keep the script's spelling, so lookups need no
speech-recognition alternatives. Fish Audio and `say` return audio only, and
Whisper times them as `narrate.ts` does. Speech is loudness-normalized like
`narrate.ts`; sound effects are trimmed to their onset, peak-matched, and
faded like the intro's stems; derived resources run one ffmpeg filter and
retime their source's words. HTTP is blocking `ureq`; a Scene Program is a
short batch process, and generation is sequential so request stitching can
pass a predecessor's request IDs.

Word times computed by the reconciler are rounded to whole microseconds before
they reach the lock, because serde_json's default float parser is exact only
for short decimals; Whisper's values already are, and adoption carries them
through the lock bit for bit. The lock and the `media/` store are the state
seam for a later remote backend (for example R2: the lock written with
conditional puts, objects keyed `<key>.<ext>`, the local directory a cache);
nothing remote exists yet. `scripts/narrate.ts` remains for scenes that still
use manifests; `psychopomp-media adopt` moves such a scene into a lock without
regenerating it.

## Rendering Is One Concrete Adapter

`render/theme.rs` owns the presentation paint tokens. Native T/Shift+T selects a
theme and `presentation/preferences.rs` atomically saves it under the user's
configuration directory. Worker requests, cached frames, and results include the
theme as well as the grid palette, so stale appearance cannot win a race. Theme
changes clear colored editor resources, but never recompile a Timeline or change
glyph metrics. Grid clear/material/ink colors are GPU uniforms; text colors are
resolved before coverage blending. Existing neutral RGB typography and known
syntax/showroom colors have a compatibility mapping; other literal art/status
colors remain authored. This is not a final-frame color filter. Native appearance
does not mutate plans. `plan frame` and `plan render --theme NAME` use the same
renderer with an explicit theme; omitted export themes remain Original.

The provisional `prototype-rich-text` adapter parses a bounded Markdown subset
with `pulldown-cmark` and shapes bold/italic/monospace runs with `cosmic-text`.
Paragraph wrapping is measured once, with a lazy current-theme sprite cache.
Inline code backgrounds, list markers, quote rules, links, and strikethroughs share
those measurements. Actor x/y/opacity/reveal/blur and `block.N.opacity`/`block.N.y`
are ordinary scalar channels. A stationary vertical mask supports rising headers.
Rich-text fades are sharp by default; `fade_blur` is an explicit optical opt-in
for titles, independent of the channel's motion profile. The showroom uses
160 ms zero-bounce prose fades and retains the approved 400 ms header rise.
HTML and images are rejected; no network, browser, Markdown-table layout, or
dynamic document diffing is introduced. Fenced code is monospaced, not highlighted.

`prototype-width-text` reuses Typeset identity, measured advances, and connector
anchors with visual-types-style width/blur disclosure rather than a uniform fade.
Optional `TextPart.spans` reuses editor `StyledSpan` roles inside a single measured
and animated part. Token style no longer has to equal animation-part identity;
preflight rejects runs whose concatenation disagrees with the backing text.
`prototype-venn` samples two rounded boundaries and unions stroke coverage; hatch
coverage is their actual signed-distance intersection, including the square morph.
Callout endpoints derive from the sampled boundaries and stay outside both sets.
Both remain provisional overlays. `scenes/component-prototypes --slideshow`
combines these with rich text, full-divider tables, and the existing composition.

`prototype-header` adds a bounded bold single-line entrance, optionally partitioned
into word regions of the **same shaped line**. Split boundaries fall in whitespace;
staggering changes only vertical pose, not word placement or text shaping. A mirror
sprite shares the sampled pose and reflects across the fixed reveal edge. Its
below-edge clip and one-sided linear alpha fade never paint over other actors.
Reflection strength, depth, and gap are presentation values, not independent timers.

`plan_runtime/header.rs` compiles word entrances into reserved `__header.*` tracks
and supplies opt-in `Playback::with_start_delays` metadata. The core's `StartDelay`
contains a resting source, destination, and duration. Only matching resting
entrances wait; in-flight redirections begin immediately. The shared
`timeline::retarget::RetargetSchedule` records event ownership and removes superseded
future starts only for changed channels.
Unchanged destinations retain pending starts, executed history remains immutable,
and pause/replay/reduced-motion use the same local clock. The authored header
compiler uses that same schedule for rapid authored events. Authored nanosecond
arithmetic and native seconds arithmetic remain distinct, and rejected batches
leave the previous writes, targets and Timeline revision intact. Existing
recipes opt into no delays and keep their previous behavior.

`component_prototype` in the lightweight crate, plan runtime, and renderer holds
the provisional Typeset / Collection / Connector trials. Typeset measures authored
inline parts; Collection measures a finite keyed row/column catalog; both lower
snapshot changes to reserved `__component.*` scalar tracks. Connectors reference
those recipes' item IDs and derive Bezier paths from their currently sampled
visible bounds, rather than independently springing endpoints. Stroke segments
union analytic coverage before alpha composition. These are foreground overlays,
not additional roots, and do not yet extend editor Semantic Targets. Their
font/anchor/payload interfaces are intentionally experimental; the native
`scenes/component-prototypes` showroom is the approval surface.
Fading prototype glyphs use the existing fractional text-blur kernel with a
sampling offset of `4 * (1 - opacity)` output pixels. This optical pose follows
the same fade in either direction, without changing the scalar timeline or
measured bounds. Fully present glyphs use the identical sharp path.

`psychopomp::value::ValueTokenPlan` is an immutable labeled tile in a finite
teaching diagram. `plan_runtime/value.rs` parses it once and samples ordinary
authored x/y/opacity/emphasis channels; it generates no extra timeline or State
Tracks. `render/value.rs` composites the tile with the existing `UiCanvas` card
coverage and cached fractional text, before foreground text annotations. Hidden
glyphs are warmed too. Border emphasis does not change text presence. This is
an overlay recipe, not another exclusive root or a general graph renderer.
`scenes/data-modeling/src/stage.rs` keeps concrete token layout, pair annotations,
and step destinations scene-local. Its second and subsequent uses are finite
sets, representation comparisons, tagged alternatives, and the nullable-pair
state table; the existing Keyed Grid still owns Cartesian-product geometry.

`psychopomp::grid` describes the immutable finite-product catalog and Grid Snapshots.
Its optional `GridStylePlan` is immutable presentation data, not another recipe.
Omitting it preserves the original JSON and pixels. `plan_runtime/grid/table.rs`
provides unequal-width, fixed-anchor table placement and conventional column
headings; the same extent and presence tracks still drive growth. Table layout
supports one depth layer in straight-on or angled view, not reassociation.
`render/grid.rs` keys its glyph atlas by labels, text style, and available width;
table text is rasterized at its chosen size and clipped to padded cells rather
than squeezed into the old symbol tile. Row rules select front-plane horizontal
edges in the existing centered-stroke pass. Background-matching fills keep opaque
depth writes but bypass face lighting. Default cube placement, typography,
lighting, and stroke behavior are unchanged.
`plan_runtime/grid.rs` validates these without a GPU, computes concrete table,
volume, and reassociation layouts, and lowers cell x/y/z, presence, emphasis,
labels, extents, slice cutaways, and camera parameters into reserved `__grid.*`
continuous channels. Equal-time snapshots
coalesce and unchanged destinations do not restart. The same prepared Timeline
drives native navigation and export; generic State Channels remain unsupported
in native presentation.

`render/grid.rs` and `render/grid.wgsl` are the first true 3D recipe: GPU-instanced
connected cells, a Depth32Float attachment, four-sample spatial AA, an orthographic
orbit, and cached two-line label textures. Detached colored blocks were rejected
in favor of the old talk's adjoining grid; a subsequent wireframe pass was made
opaque to hide rear lines. There are no gaps or per-cell scale animations.
Continuous axis extents clip growth from a leading corner, while the projection
centers the sampled visible cell bounds at canvas center on every frame. This
deliberate recentering includes fractional growth, rotation, and slice cutaways;
side headings do not skew the cell bounds. The catalog determines base scale,
with a separate continuous zoom-out for labeled reassociation groups.
An optional authored `scale` channel multiplies that conservative fit for slides
that need a larger diagram without changing the existing default. Strokes remain
output-pixel sized. A singleton depth catalog has no outside Z heading, avoiding
a misleading extra column label in a two-dimensional product.

Optional `GridCellLabelPlan` values supply primary symbols and secondary text
without replacing tuple identity. The chess example demonstrates this separation.
Cell labels share geometry and depth testing; every tuple retains its ink until
its actual face is hidden or removed. Cell ink does not fade toward the next
focused slice: doing so left a blank opaque outgoing face in front of the new
labels. Outside row/column/depth headings stay upright at projected anchors.
A selected slice clips away the other layers without dimming their still-visible
strokes. The former 18% focus-emphasis target is no longer scheduled; contrast
does not anticipate a layer's physical removal. Group headers remain text,
not backing cards. Headings use a separate premultiplied-alpha overlay after
the completed material and strokes, with no depth attachment. Their cached glyph
coverage and spatial feather blend into the existing frame; faint outgoing
headings neither overwrite cells with background-colored glyphs nor block lines.
Cell material and attached ink retain opaque depth-tested rendering.
The depth range fits sampled geometry, back faces are culled,
and a tiny stable-order depth bias resolves coplanar front faces during regrouping
without moving their geometry. This fixes a pixel regression in rapid navigation.
`render/grid/edges.rs` and its shaders draw centered 1.7-output-pixel strokes
separately from the opaque material and text. A stroke-depth prepass selects
visible edges, an RGBA16Float max-coverage pass unions coincident strokes, and a
linear-light composite resolves them over the material. Thus silhouettes,
shared grid lines, and two-face creases have the same width. Edge-local ray/box
depth and per-sample coordinates preserve coverage at grazing angles. Material
keeps catalog-order depth priority; strokes uniformly clear that priority budget
and merge shared coverage rather than choosing between neighboring opacities.
The final composite uses the existing RGBA texture and shared readback/encoding;
native presentation still uploads those pixels, rather than sharing a zero-copy
surface. Resources are lazy and only the latest grid's label atlas is retained.
This reopens procedural 3D for one concrete diagram, not arbitrary meshes,
Blender materials, a public camera graph, or a renderer abstraction.

Growth-edge disclosure is the default (`plan_runtime/grid/disclosure.rs`),
selected from the former A/B/C comparison. Disclosure tracks use ordinary native
retargeting for semantic visibility, while the feather samples existing extent
and slice bounds, with no separate reveal timer or competing growth fade.
Headings disclose in catalog order along projected X, −Y, or −Z; cell ink follows
the clipped face edge. Upright headings use a glyph-fitting aperture when their
projected cell interval is too small. The 8-output-pixel linear ramp affects ink,
not material or borders; it is neither another easing nor a blur filter.
Geometry timing is unchanged; the losing motion treatments and prototype hint
have been removed. The earlier Euclidean per-face border correction fixed
diagonal expansion but not half-width silhouettes; centered strokes replace it.

`render/grid/palette.rs` contains five native line-color audition choices. C and
Shift+C select a preview-only palette without modifying Playback or Scene Plans.
The worker's `FrameCache` keys pixels by slide, visual sample, theme, and palette; frame
results carry that palette so a stale in-flight render cannot overwrite a newer
choice. Paused/held frames can change color without advancing their clock. The
renderer override changes cell borders only; exports keep the recipe's default color.
Orange remains the Original theme's default after auditioning the alternatives.
Other themes start with their accent; C auditions override colors for the current
player lifetime, and T restores the newly selected theme's default line treatment.

`crates/psychopomp-render/src/render.rs` is the concrete `wgpu` and `cosmic-text` Adapter. `HeadlessRenderer::render_shapes` renders transparent editor-local geometry into tightly packed RGBA pixels. Cached stable-line sprites and pointers are added to that flat editor surface before the shared card compositor presents it.

The Adapter also resolves semantic token targets from `cosmic-text` glyph cluster hitboxes. Token highlights and the pointer consume those measured bounds; choreography does not estimate monospace character widths or hardcode target coordinates.

Code-target measurement shapes each inline partition once for both its advance
and selected cluster bounds, without rasterizing discarded pixels. The separate
`render/text/raster.rs` leaf owns exact glyph rasterization. `render/text.rs` owns
the typed plain-text cache; root, Task, and overlay callers retain their different width, line-height, crop, and color
policies. Debug, inline-code, SVG, and bubble resources keep their own lifetimes.

Editor panel translation, three-axis rotation, and scale are sampled properties. The editor shader remains flat and transparent; `render/ui/card.rs` is the sole perspective implementation for both editor and recorded-video surfaces. Depth-weighted Gaussian sampling softens the near edge during the opening pose, while increased entrance shutter sampling keeps fast perspective motion continuous.

Prepared Scene Plans derive an exact visual key from sampled motion position and velocity, prior pointer-motion state, discrete State Track values, video source-frame identity, and recipe-owned internal tracks. Shutter samples with the same key render once and contribute their multiplicity to linear-light accumulation. This optimization preserves arbitrary-time semantics and cannot collapse active motion merely because neighboring encoded frames happen to look similar.

SVG assets are parsed and rasterized once into reusable cached sprites, then transformed and composited for each temporal sample. The pointer uses the exact filled Phosphor `HandPointingIcon` path selected by default in `effect-institute`; the same path supports future SVG actors without adding asset-specific shader geometry.

SVG sprites are rasterized at four times their display resolution and coverage-sampled during rotation. The compositor also supports animated scale, opacity, and blur, which the pointer uses for its Effect Institute-style entrance.

Pointer translation uses mildly underdamped scalar property tracks; rotation is derived from sampled velocity and acceleration so the hand leans against acceleration and follows through while decelerating. The complete transform remains deterministic and participates in temporal accumulation.

Stable code lines can be split into cached stable and variable sprites. An inline reveal opens or closes a variable span's layout width while opacity and blur resolve; every following stable or variable span derives its position from the sampled widths before it. Multiple non-overlapping reveals can exchange Effect and function alternatives horizontally while `const getTime`, ` = `, `getTime`, and `)` retain identity. This is the concrete maximum-stability seam demonstrated by Effect Institute ports and remains narrower than a general recursive slot AST.

Editor text uses fractional glyph sampling. Premultiplied bilinear sampling preserves subpixel translation; source-range coverage clips fractional reveal columns once, without double-attenuating raster borders. Weighted blur taps move continuously rather than rounding their offsets. Intersecting line pixels clip against the viewport instead of dropping a complete line at its boundary. Planned text actors use this path too. This intentionally changes preview and export pixels; final-window smooth/pixelated filtering remains separate.

Planned text actors may declare a stationary canvas-space `verticalMask`. The compositor integrates its linear top/bottom fades over each pixel row and applies that coverage to sampled text alpha before blending. Text moves through the aperture; the mask does not follow its center or darken already-composited pixels. The same optional path serves native preview and shutter-sampled export, while unmasked actors retain their original pixels. The rolling showcase captions demonstrate this narrow recipe property without a public clipping tree or new Scene Plan version.

The compositor supports multiple non-overlapping reveals on one stable line. Focus ranges and token highlights carry independent vertical geometry, so cursor-only cues do not accidentally move or resize focus.

`crates/psychopomp-render/src/render/task.rs` owns the concrete Effect Task visual recipe: compressed running nodes, energy sweeps, icons, result text, error bubbles, and labels, composited as `TaskVisualFrame` actors over other pixels. All moving pixel layers share one fractional transform, rounded signed-distance edge, and analytic coverage so the body, sweep, border, and glow remain one coherent material. Result content keeps its natural transform but is analytically masked by the current sampled rounded body after text blur, preventing spring intermediates from leaking outside the Task without scaling content to fit.


`render/footage.rs` draws a Video Card in one direct pass: `ContentFit::Region` maps the focus window (fractional source pixels) onto the footage rectangle below the optional title bar, so zoomed footage is resampled once through the projection. The title bar is rasterized at twice its size into a small transparent strip and composited through `FrameUi::card_layer`, which reuses the card's projection and rounded clip without a second shell or shadow. Material, border, and shadow come from the theme palette. The lightweight `video.rs` owns the focus math: a window center and size, clamped inside the frame, whose corners move linearly on one spring so every point of the focused region travels monotonically.

`scenes/opencode-session-tool` combines planned video, layered SFX, continuous card motion, and named cues through the process seam. Its immutable source recording aligns a real Vim session on the left with one already-running OpenCode v2 client on the right; a titled Video Card preserves those source pixels and their aspect ratio while ordinary `text` actors carry explanatory overlays. The 20.5-second rapid-fire artifact demonstrates live command, agent, project-skill, reference, model, permission, ambient-instruction, and local-plugin generation changes without restarting the OpenCode service, client, or session.

`scenes/quark-before-after` is a compact narrated tutorial contrasting Solid Store reconciliation with explicit keyed identity. It uses the existing editor, text, and Script Clip recipes rather than introducing a reactive-system visualization or another renderer abstraction. The scene also demonstrates the planned editor's shared perspective-card entrance and multiple channel-driven Inline Reveals: one new Stable Line enters before opposing variable parts exchange inside otherwise stable lines, then the update call changes on its own narration cue.

`crates/psychopomp-render/src/render/ui.rs` is a private, GPUI-inspired immediate-mode vocabulary for renderer-owned pixel interfaces. Immutable `Bounds` and `Edges` values place and inset child regions. `render/ui/card.rs` adds nested rounded clips, fills, strokes, packed or strided RGBA sources, fit modes, card-local overlays, and projected cards with one material, border, shadow, surface blur, and depth-dependent near-edge blur. Draw order is z-order and closures provide local composition without retaining public nodes. Flattened editor pixels and decoded Video Card pixels enter through this Module. A direct borrowed-source operation preserves the same presentation semantics without first rasterizing a second full-card intermediate; the closure path remains available when a card needs multiple composed layers. The Module intentionally stops before an element tree, flexbox engine, event model, retained widgets, renderer trait, or public UI framework.

The Adapter keeps these details private:

- headless Metal adapter and device creation
- WGSL pipeline and uniforms
- bundled CommitMono (`render/fonts.rs`: compiled-in faces replace any installed
  CommitMono; system fonts are glyph fallback only) and raster-sprite caching by
  stable line ID
- texture-to-buffer row alignment
- asynchronous mapping and GPU polling
- the flat dark editor treatment

There is no renderer trait. One Adapter is a hypothetical seam; a second backend would make it real.

### Explainer overlays

`sequence` and `caption` are CPU overlays with strict payloads (`deny_unknown_fields`)
and strict channel names: `plan_runtime/sequence.rs` and `plan_runtime/caption.rs`
reject any property on their actor that does not name a real participant, row, or
recipe channel, because a typo would otherwise do nothing. Geometry and validation
live in the lightweight crate (`sequence.rs`, `caption.rs`), so authoring helpers
(`SequenceActor`, `CaptionActor`) and tests need no GPU. Pixels come from
`render/sequence.rs` and `render/caption.rs` through the shared primitives: cached
CommitMono plain-text sprites, `UiCanvas` fills, and the analytic polyline stroke.
Sequences draw with the other diagram surfaces (after Venn and Value Tokens);
captions draw above rich text and below plain text. Semantic colors resolve through
`Theme::tone`, the one place status colors are fixed (Neutral overrides them).

A `rolling-number` overlay (drawn just after captions) carries its own timed
values, like editor or grid snapshots, rather than a State Channel. The
lightweight `rolling.rs` tokenizes each value into stable digit places,
separators, and literals, then folds the changes in order into closed-form
settling tracks (`math::dynamics::settle`) per column: x, wheel position,
opacity, and rise. Each change samples the earlier tracks at its time, so a
redirect keeps position and velocity, and any time samples without history.
The renderer supplies only glyph advances to that compilation, then paints each
wheel's two straddling faces through a stationary `VerticalMask` window with a
speed-driven vertical smear (`TextFilter::Smear`). Because its motion lives
outside Continuous Channels, a settling number marks its samples distinct
(`ambient_time`), including over a Stage, so shutter samples are not merged. Its
schedule follows the authored clock, so plans using it are export-only.

A `tree` overlay (drawn just before captions) shows a JSON value as a foldable
outline. The lightweight `tree.rs` flattens it in pre-order with JSONPath
identities and computes the layout as a pure function of the per-path `open`
channels: a node's block is `1 + open × (children + 1)` rows, children sit at
full pitch inside the room it opens, and the closing bracket rides that room's
bottom edge. Rows above never move and rows below shift by exactly the room, so
no step re-lays out the tree. `render/tree.rs` paints rows on a monospace column
grid with cached CommitMono sprites, a `VerticalMask` per room (and the scroll
window), `UiCanvas` bars and guides, and chevrons stroked with
`math::shapes::segment_distance`. Value changes are a variant-index channel that
rolls the text through its row's window. Everything is a Continuous Channel, so
trees retarget like any other channel and run in native playback.

`plot` and `lanes` are chart overlays (drawn just after sequences) that share an
axis vocabulary. The lightweight `axis.rs` owns `AxisPlan` (range, ticks, label,
unit) with tick choice (`every`, `nice`) and tabular tick labels; `plot.rs` and
`lanes.rs` own payloads, validation, strict channel matching (`accepts`, which
matches `series.<id>.…` and `lane.<id>.…` by prefix and suffix so ids may contain
dots), interpolation (`PlotSeriesPlan::y_at`, `slope_at`), and the `PlotActor`
and `LanesActor` handles. A plot's curves are points the Scene Program sampled,
optionally with exact slopes; the renderer never evaluates a function.
`LanesPlan::from_scene_plan` compiles selected channels with the ordinary
`compile_channels` to sample their sparklines, so a Lanes view shows the tracks
the renderer would play. Pixels come from `render/plot.rs` and `render/lanes.rs`
over `render/chart.rs`, which holds the shared ink: snapped CommitMono labels,
`chart_axis` (ticks disclosed as the line reaches them, fading beside a playhead
readout), dashes along arc length, dots, diamonds, and readout tabs. Both draw
only from channels, so they are native-presentable.

### Text surfaces

`terminal`, `chat`, and `changed-files` are overlays (drawn just above Video
Cards, beneath diagrams and text) that stand in for familiar interfaces where
scenes used to fake them with Stage cards and cycled statuses. Each lives in a
Window: the lightweight `window.rs` owns its five channels and `settle_in` (the
Stage card entrance: 16 px drift, 1.035 scale, content 65 ms behind), and
`render/window.rs` draws the shell through the shared projected card
(`FrameUi::card_source` with an empty source, plus `card_layer` for the title
bar). Composing that shell costs tens of milliseconds, and a window holds still
while its content moves, so the composed shell is cached per pose and theme as a
cropped layer and blended at the body's opacity; Porter-Duff over is
associative, so the layer equals drawing directly. Content is drawn unscaled
onto the frame with the shared text and fill primitives once the body has
nearly settled.

Each recipe keeps the Tree's contract: stable IDs, strict per-ID channels, and a
layout that is a pure function of channels. A Terminal line's
`line.<id>.reveal` opens its row; rows stack by the sum of reveals above them and
the window shows the last `rows`, so a full window slides older lines up by
exactly the new room, and `scroll` is a floor (`clear` springs it to the content
height). Typing writes one exact step per keystroke at times from
`terminal::keystrokes`, a hashed, deterministic cadence. Task spinners reuse
`effects::spinner`; their `spin` and `mark` clocks stop once the mark has cooled,
so holds merge into one sample. A Chat Thread's rooms are computed from
`typing`, `reveal`, and reaction presence over heights the renderer measured once
(`ChatMetrics`), stacked up from the composer; a typing slot and its message are
one ID, so the indicator grows into the bubble. Wrapped sans-serif text with
code chips is shaped once at preparation and recolored per theme. Changed Files
rows hold fixed slots; its header totals are three Rolling Numbers built at
preparation from a `totals` schedule the handle writes as rows land, reusing the
Rolling Number compiler and painter. Terminals and chats are channel-only and run
in `plan present`; a changed-files card whose totals roll is export-only, like a
Rolling Number.

A `lower-third` overlay (drawn just after Rolling Numbers) is a name and role
beside an accent bar. Its `bar`, `name`, and `role` channels are independent
phases: the bar draws up on the draw-on curve, and each line slides out from
behind it through a stationary clip edge, so nothing ever shows on the bar's far
side. Sans sprites are cached per theme. It is channel-only and presents
natively; it is not a header or chapter template.

### Callouts

A `callout` overlay (drawn after Rolling Numbers, below plain text) points at
something whose position only the renderer knows: a Stage element seen through
the sampled camera, or an editor code range after `cosmic-text` measurement,
line motion, and the panel's card projection. The seam is the prepared root.
The lightweight `callout.rs` owns the payload, validation, anchor edges on a
`math::shapes::Shape`, the linear leader shape (`CalloutLeg`), frame-avoiding
`layout`, and `CalloutActor`; none of it needs a GPU. `plan_runtime/callout.rs`
validates anchors against the root at preflight and, at every sample, asks the
prepared root for each weighted anchor's canvas point: `render::stage_anchor`
projects the element through the same `Scene` camera and placement the Stage
paints with, then applies the develop pass's roll and punch-in (at the overlay's
own sample rather than the exposure's central one, a sub-pixel difference);
`PreparedEditor::anchor` samples the Semantic Target's companion geometry and
maps it through `render::editor_canvas_point`, which shares `EditorCard` and
`CardProjection::project` with the editor compositor. Unlike editor attachments,
callouts compile no companion tracks: the anchor is resolved fresh each sample,
and `move_to` springs ordinary `anchor.<id>` weights whose normalized blend
carries velocity across redirects. `render/callout.rs` paints the mark, the
analytic leader (`composite_prototype_path`), and the label. Over a Stage, the
overlay sample key includes each stage-pinned callout's resolved anchor, so
overlays re-render per shutter sample exactly while the camera moves and merge
again once it rests. When only callouts differ across a frame's samples, the
per-sample composite and linear-light average cover just the row spans the
moving callouts ink (`HeadlessRenderer::callout_bounds`, `exposure::accumulate_region`);
still callouts outside them draw once, and every other pixel takes the same
weighted average through a per-value table, so the exposure is bit-identical.
Callouts may also point at Sequence Diagram participants and rows (see Anchors).

### Anchors

Callouts were the first overlay to pin to things; captions, Rolling Numbers,
text, and images now share the same Anchor model rather than literal canvas
coordinates. The lightweight `anchor.rs` owns `Edge` (callouts re-export it as
`CalloutSide`), the serializable `AnchorPlan` (`point`, `stage`, `editor`,
`participant`, and `row`, each with an optional `offset`), and the borrowed `AnchorTarget` that both
`AnchorPlan` and the callout-specific `CalloutAnchorPlan` (which adds a label
`side`) lower into, so callout JSON is unchanged. It also owns `anchor.<id>`
weight names, strict acceptance, validation, the weighted `blend`, and
`move_to`, which every handle's `move_to` delegates to.

`plan_runtime/anchor.rs` is the renderer seam: `validate` checks each anchor
against what the plan can place (`Placeable`: the root, its Semantic Targets,
and its Sequence Diagrams) at preflight, `resolve` asks the prepared root or
Sequence Diagram for a point (`render::stage_anchor`, `PreparedEditor::anchor`,
`PreparedSequence::anchor`), and `pin` blends an overlay's weighted anchors
plus their offsets. Sequence Diagrams are overlays, not roots: preparation
measures their header and note text once (`HeadlessRenderer::sequence_widths`,
sharing `SequencePlan::header_width` and `note_span` with the painter), and the
lightweight `header_box` and `row_box` place a participant's header or a row's
span, moved by the diagram's `x`/`y` channels. Their channels are already in
every sample key, so anchors on a moving diagram need nothing more. `PreparedPlan::pin` returns the
literal origin while an overlay has no anchors, so unpinned overlays take the
exact pixels they always did. A pinned caption or Rolling Number draws its
origin at the pin; text and images put their center there (text's absolute
`x`/`y` channels move it by their displacement from `center`). Nothing pinned
is compiled into tracks: the anchor is resolved fresh each sample, so it never
lags, and an overlay pinned to a Stage element follows its position, not its
perspective scale.

Over a Stage, every Stage-pinned overlay's resolved origin joins the overlay
sample key (`PreparedPlan::stage_pins`), so its shutter samples stay apart
while the camera moves and merge once it rests. The callout-only region
exposure applies only while nothing else differs: `only_callouts_differ` also
compares those origins, so a pinned caption riding a dolly takes the full
per-sample composite. Other overlays adopt anchors the same way: add
`anchors: Vec<AnchorPlan>` to the payload (validated with
`anchor::validate`, channels with `anchor::accepts`), list the actor in
`PreparedPlan::pinnable` and in preflight's anchor validation, and draw at
`PreparedPlan::pin`.

### Footage

Images, videos, and image sequences are one capability rather than parallel
recipes. The lightweight `footage.rs` owns the `Clip` (which placement, its
trim, rate, hold/loop/bounce, reverse, freeze) and its playhead mapping in
integer nanoseconds: the natural playhead runs from the placement's timeline
start at the clip's rate, a written `time` channel replaces it, and
`Clip::source_nanos` holds, wraps, bounces, and mirrors it inside the trim.
Freezes, ramps, stutters, and scrubs are therefore ordinary channel events
(`Playhead`), never per-frame integration; a speed ramp is one
`Ease::Decelerate` whose slopes are the two rates. A footage placement names
the file and the span it is available in, not the trim, because placement
validation ties source and timeline durations together and a rate would
break that.

The renderer lowers all three overlay payloads into a `FootagePlan`
(`plan_runtime/footage.rs`): a Video Card is a framed rectangle whose trim is
its placement's source range and whose decode is its authored size and rate;
an image is a bare or framed rectangle whose height follows its decoded
pixels. Video Cards draw first, then images and footage in declaration order,
so old plans keep their order and their exact pixels (they take the original
direct compositor path). New footage chooses a decode width on a shared
ladder at about twice the widest it is shown (its largest `scale` over its
tightest `focus-size`, cropped by its fit), never past the source, so tiles
of nearby sizes share one decode. `FootageStore` opens each source once:
stills decode in memory; videos and sequences decode once to disk and read
frames through one LRU bounded by `PSYCHOPOMP_FOOTAGE_CACHE_MB` (384 MiB).

`render/footage.rs` draws through the shared projected card. `CardShape`
generalizes the card's rounded outline to a circle or polygon (its border and
shadow follow it), and `SourceTreatment` desaturates, tints, or dims source
pixels before they meet the material; the rounded, untreated path is the
original code. Each footage overlay is drawn alone into a cached layer keyed
by its pose, its frame identity, and the theme, and a sample blends every
run of layers in one banded pass; a still overlay among moving ones costs a
blend, not a projection. Over is associative, so this equals drawing
directly to within 8-bit rounding; sub-0.2 px blur normalizes to none,
exactly as the compositor samples. Each overlay's frame identity joins the
visual sample key and its playing `time` channel leaves the motion key, so
samples within one source frame merge and a playing clip renders every new
frame.

A Stage `footage` element is a camera-facing quad (a billboard, like a
card), so it gets depth, parallax, draw order, depth of field, follows,
and anchors from the Stage's own placement. Every footage element's frame
lives in one `Rgba8UnormSrgb` atlas slot; each shutter sample uploads only
slots whose frame identity changed (writes land before the next
submission), and primitive kind 13 samples its fit and focus window with
premultiplied bilinear taps, spreading 16 golden-angle taps for depth of
field or minification, cut by a rounded box, circle, or polygon and treated
like an overlay. The Stage already renders every sample, so its footage
needs no key. Plans with footage carry media, so they remain export-only.

### IDE annotations

Diagnostics (`diagnostic`), Hover Cards (`hover-card`), and Cursors (`cursor`)
are separate actors attached to the editor root by Semantic Target ID, as a
pointer attaches by editor ID; they add no fields to `EditorRecipePlan`.
`plan_runtime/ide.rs` decodes each once with strict channels, and preflight
attaches it to the `PreparedEditor` after checking its targets belong to that
editor. At every sample `PreparedEditor::render` measures each target through
the same `MeasuredTarget::sample` that attachments and callouts use, so an
annotation follows Stable Line motion, Inline Reveals (including an Inlay Hint
opening before it), and the line's opacity, with no companion tracks. The
frames enter `EditorFrame::annotations`; `render/ide.rs` paints them on the flat
editor surface, so the card projection carries them and the interactive preview
path draws them too: selections under the code, waves, gutter icons, and carets
over it, Hover Cards last (before the pointer), clamped inside the code body.
The wave is `psychopomp::ide::wave`, a sine phased from the range start and
sliced by arc length for `draw`; hover placement is `ide::hover_layout`, which
slides rather than flips; the caret blink is `ide::caret_blink` of a `blink`
clock channel. Hover text reuses the editor's syntax sprites (smaller) and the
plain-text cache for toned prose.

An Inlay Hint is not an actor: `insert_inlay` adds an `inlay:<id>` part, a
semantic range, and an Inline Reveal on the `inlay.<id>` editor channel, so
measurement, room-opening, Maximum Stability, and `plan steps` all treat it as
the Inline Reveal it is. The compositor recognizes the part-ID prefix to draw a
chip behind the segment's ghost ink. Stepped Diff lines name ranges by text
(`Line::range`), carry hints (`Line::inlay`), and may override their mark;
`Diff::declare` returns a `DiffEditor` whose `target` pins Semantic Targets to
those ranges, so callouts and annotations pin to diff code directly.

### Visualization overlays
`checklist`, `meter`, `bars`, `subtitles`, and `confetti` are CPU overlays in the
mould of Plot and Lanes: strict payloads and channel names, geometry and
validation in the lightweight crate, and pixels over `render/chart.rs` ink and
cached CommitMono sprites (regular weight: only 400 and 700 are bundled, and a
semibold request silently falls back to an installed face). `plan_runtime/viz.rs`
holds them as one group, so shared preflight and preparation each take one
field and two render calls: checklists, meters, and bars draw with the charts;
confetti and then subtitles draw last, above every other overlay.
A **Readout** (`readout.rs`) is the numeric display they share. It is a pure
function of a channel's value rather than authored changes, so it runs in
native playback where Rolling Numbers cannot: `odometer` rests on the rounded
value and rolls across a window at the rounding boundary (shifted for
round-up countdowns), higher places carry only while lower ones roll over from
9, and leading places roll in with their room. The renderer reads the channel's
velocity from its compiled track (`motion_value`) and smears wheels by their
`wheel_rate`, because fast odometers otherwise strobe between ghosted faces.
Checklist items reuse `effects::spinner` exactly as Stage cards do (`spinner`,
`mark` clocks; a skip's `mark` releases the motor), and `ChecklistActor`
remembers start times so a resolution lands on the next handoff crossing.
`ItemPose` derives the pending ring, label brightness, strike, rail, and result
from the four channels. Meters and bars are channel-only. Bars sort by
springing `row.<id>.slot`; while rows move, `paint_order` draws rising rows
last over an opaque band that fades in with slot speed, and the grid is redrawn
inside each band, so crossing rows occlude instead of interleaving.
Subtitles measure words once at preparation (`SubtitlesPlan::layout`), chunk
them into pages (sentence ends, pauses of 0.55 s, a 6 s cap, width), and
balance each page's lines by the narrowest wrap with the same line count. Pages
swap directly while speech continues and hold 0.7 s after a pause. Like a
Rolling Number, their schedule follows the authored clock, so `SubtitleLayout::moving`
marks transition samples distinct (`ambient_time`, including over a Stage) and
plans using them are export-only. Confetti poses come from
`effects::confetti::Burst` over `math::dynamics::ballistic` with seeded
`random::hash` per piece; reversing the clock reassembles the burst.
### Lenses
A `lens` overlay is thick glass over the composed frame. The lightweight
`lens.rs` owns the payload and the sampled `Glass`: a `math::shapes::RoundedBox`
outline (circle, capsule, or rounded box), a superellipse rim whose surface
slope (`math::optics::superellipse_slope`) refracts a vertical ray by Snell's law
(`math::optics::refraction_offset`) toward the center over a page `depth` below,
and an even magnification about a focus point on the flat top. `Glass::source`
maps a canvas point to the page point it shows; the bend is zero on the flat
top, so the middle is undistorted and only the rim splits color. Presence
condenses the glass: size, rim depth, and magnification grow together. None of
this needs a GPU, and the optics are tested directly.
The lens is a pass over the composed frame, not a root feature, so one
implementation serves every root. `render_overlays` applies each visible lens
after callouts and before plain text and Tasks: `render/lens.rs` copies the
page under the source bounds into linear light (plus a two-pixel softened copy
from running box sums), then shades each pixel inside `Glass::bounds` in row
bands on scoped threads. The flat top samples with a Keys cubic that sharpens
from Catmull-Rom toward `a = -0.75` as magnification rises, clamped to the four
nearest texels so enlarged strokes neither ring nor halo; the rim softens where
it compresses the page so moving text does not crawl. Light is additive in
linear light: a fresnel sheen of a sky brighter above, a crisp specular line
with a soft glow where the rim faces the upper-left light, a fainter line and
inner glow opposite, and an edge hairline; a drop shadow and contact darkening
fall outside the outline. Pixels outside the bounds are untouched, so plans
without a lens keep identical pixels.
Each temporal sample refracts its own frame. On CPU roots with nothing above
the glass (no plain text or Tasks), `render_lensed_exposure` renders the page
beneath the lenses once per distinct non-lens sample key and refracts a copy per
sample, so a lens gliding over still code costs a lens per sample rather than a
page; a GPU test proves it bit-identical to whole samples. Otherwise the lens is
part of `render_sample`. Over a Stage it refracts the developed exposure (a
motion-blurred base, like every overlay there) at each overlay sample. A Stage lens's
resolved anchor joins the overlay key, so a lens riding a card through a dolly
re-composites per shutter sample even while its own channels rest, and any
visible lens takes the whole-sample composite rather than the callout-region
shortcut, because it reads pixels beyond its own ink. Anchors resolve through
`callout::resolve` (and `LensAnchorPlan` is the callout anchor type), so moving
to a shared anchor Module is a rename. Stage grain is developed before overlays,
so a lens enlarges it with the page.
`Glass::pane` reuses the same optics as a panel material: no magnification, a
thick pill rim, heavy frost (an eight-pixel softened page), and a dim so light
text reads over light pixels. A caption with `glass` composites that pane in
place of its solid chip, before its text, so the scene behind refracts at the
pill's rim and diffuses through it, the way a packet's light passes under a
label on its wire.
### Stage

`stage` is an exclusive root recipe. The lightweight crate (`stage.rs`) owns the
element model, strict channel names and their one table of defaults
(`StageElement::channel_defaults`, `StagePost::channel_default`; `StageActor`
declares new channels at them and the renderer falls back to them, so authoring
and pixels agree), the perspective `Camera`, element outlines, and
the deterministic orb geometry (Fibonacci points, shatter trajectories), so authoring
helpers (`StageActor`: `to`, `ease`, `clock`, `hit`, `send`, `type_in`, the
composed beats such as `orb_in` and `rewind`, and the effect beats `zap`, `charge`,
`hum`, `dissolve`, `materialize`, `scan`, `raise`, `lower`) and tests need no GPU.
Every renderer read of a Stage channel falls back to `StagePlan::channel_default`,
the same value `StageActor` declares a new channel at.
`render/stage.rs` is small pieces: `Scene` samples the camera, every element's
placement, and every beam's path once per sample; `Painter` has one method per
element kind; `StageFrame` owns primitive helpers and depth-sorted layers. A beam is
a `math::shapes::connect` connector between the two outlines on screen: it leaves the
middle of the card side that faces the other end, perpendicular to it, and enters an
orb radially beneath its shell, with a socket where it plugs into a card. A dark
orb body occludes submerged endpoints and packet landing rings. Measured packet
label boxes stay outside the full bodies' port tangent planes (`math::shapes::fit_between_ports`),
not the submerged wire endpoints; the label stops early while the packet finishes.
Labels still fade before an orb contact and do not render when the complete text
cannot fit. Packets sort behind the connected bodies too.
A packet is one clock (`age`, `flight`); `stage::packet` derives its phases (gather,
minimum-jerk quintic flight by arc length, landing ring, and a trail whose points cool with
the time since the packet crossed them, found by inverting the ease), so rewinding
the clock un-cools the trail. Lights are collected per sample from packets and
explicit beam surges: a reflection (edges only, the diagrams' radial falloff) and pools
(ember, flood, surge) that also enter the glass. Each card takes its strongest
reflection and strongest pool as two shader lights; the orb's shell points sum
them. Polyline points carry a heat that scales their light. A beam
sorts behind both of its ends, so it never crosses the cards it connects.
`StageActor::connect` draws a matte wire with softly revealed, fixed-size sockets,
then holds; it emits no travelling bead and schedules no impact, twang, or flow.
Those remain separate authored channels for deliberate physical beats. The
renderer emits depth-sorted signed-distance primitives (rounded rect, circle, arc, polyline with drawn length,
dash, flow, and fade, atlas text, backdrop gradient) into one storage buffer;
`stage.wgsl` draws them as instanced quads into an `Rgba16Float` target with
premultiplied blending, where glow adds light at zero alpha. `stage_post.wgsl`
thresholds and blooms that light through a five-level 13-tap downsample and tent
upsample, then composites with highlight rolloff (identity below 0.8, so authored
UI colors stay exact), chromatic aberration, vignette, and grain locked to the
output frame. A Stage exposes a frame on the GPU: each of its 24 temporal samples
draws into the HDR target and adds, weighted, into an HDR exposure target; bloom,
rolloff, and grain then develop that exposure once, with the post settings of the
central sample. Light therefore integrates before the response curve (a bright
ember keeps its streak's energy), and no sample is read back. Plan overlays drawn
over a Stage (headers, chips, captions) are composited once, or averaged on the
CPU only across samples where their own state differs. Stage text is rasterized
once at twice its size into an R8 atlas and drawn with a soft background-colored
backing for legibility over light. A stage root marks every temporal sample as
distinct (`ambient_time`), because spin, flow, and grain always move.

Every root renders a frame from one exposure, `exposure::exposure`: stratified
times across a 180-degree shutter whose weights ease off over the outer quarter
at each end, so streaks fade rather than ending on a hard copy. Samples with
equal visual keys merge their weights. Roots without their own exposure average
sRGB samples in linear light on the CPU (`exposure::accumulate`).

The Stage separates material response from transforms: a pulse lights the orb
without moving its shell or ports, and a card flash lifts ink and rim while its
substrate stays dark. An overhead key shades panel fills and borders; moving
reflections stay local. `settle_in` uses small, damped scale/position springs with
a separate delayed content spring; `ease(.., Ease::Smootherstep)` suits deliberate
camera compositions, and `clock` starts an effect rig's elapsed-seconds channel. Packet travel uses the same acceleration-continuous quintic,
with its inverse in shared math providing trail crossing times.

#### The Stage camera

`stage/camera.rs` owns one `Camera` value used by everything that projects: the
renderer's `Scene`, `render::stage_anchor` for callouts, and Scene Programs that
compute Reel zoom rectangles or framing. The pose is a rig: pan, a dolly along
the view axis, yaw and pitch about a pivot on that axis at world depth
`camera.pivot`, a focal-length `zoom`, and an image `roll`. When yaw and pitch
are zero, `Camera::project` and `Camera::depth` take the original arithmetic
path, so existing plans render bit for bit; a test keeps the old projection as
an oracle. Cards, labels, and rings stay screen-aligned billboards at their
projected centers: the Stage's primitives are screen-space signed distances with
analytic edges, bloom, and light pools, and a foreshortened card would cost
legibility and a homography in every primitive kind. Orb points, embers, surface
rings, and beam endpoints project individually, so they show true parallax. A
`Placement` carries `Camera::depth` (world z when unturned) for draw order and
depth of field; orb dots sort by it, and their facing and contact directions are
taken in the camera's frame. The roll is applied as each shutter sample is
added into the exposure (`accumulate` in `stage_post.wgsl`), magnified just
enough to cover the corners, so a rolling camera motion-blurs; a zero roll keeps
the exact `textureLoad` path. The develop pass's shake roll and punch compose
after it, and `stage_anchor` applies both in the same order.

Following is resolved by the renderer, not baked into channels: a packet's
position exists only on screen (its beam is a connector between projected
outlines), so `Scene::tracked` blends the authored pan toward
`Camera::aim(point)` for each `camera.track.<id>` weight, where a packet's point
is its head unprojected at the depth interpolated between the beam's ends. Three
passes settle the parallax when the ends differ in depth. Weights are ordinary
channels, so catching and releasing a follow carry velocity, and handheld sway
(`effects::shake::handheld`), kick, and rumble add afterward. Callouts pinned to
Stage elements see the same followed camera.

`CameraRig` writes shots as ordinary `camera.*` channels. It reads the pose a
shot starts from with `PlanBuilder::sample` (the channels as written so far) and
skips any channel whose authored destination already matches, so unchanged
channels keep their trajectories. `Camera::framed` fits element footprints
(billboard boxes from the plan's geometry and each element's own x/y/z/scale
channels) inside a padded frame: Newton steps on the pan with a numeric
Jacobian center the bounds, and bisection on the dolly finds the closest fit.
A dolly zoom writes `z` and `zoom` on one curve; because both are affine in the
same progress and zoom is proportional to the subject's distance at both ends,
the subject's scale is exactly constant throughout.

Orb `rotation` is an angular offset, independent of ambient `spin`; `blur` is a
separate defocus pose. `burst` is an opt-in age in seconds (-1 means intact): a
120 ms collapse, combustion, smoke, and embers over 5.2 seconds. The private
`effects/combustion.wgsl` and `effects/noise.wgsl` are concatenated with the primitive shader and raymarch a
domain-warped procedural density with emission and Beer-Lambert absorption.
It is an analytic appearance, not a fluid simulation. Embers use shared
`effects::combustion::Burst` over `math::dynamics::ballistic` (constant gravity and linear drag), while the composite
refracts the scene with an inward pinch and expanding pressure wave. The first
active orb in element order drives this screen-space wave; volumes and embers
render for every bursting orb. Reversing the age reconstructs the effect without
simulation history; ambient spin remains on the scene clock.
The intact shell keeps its material and occlusion through compression, then
hands presence to the hot particles over a 55 ms ignition envelope. Combustion
casts an age-driven local rim reflection through the existing light path; it
does not wash card fills. The procedural density has compact support, reaching
zero before the ray interval and screen-space rejection bounds.
The binding-free `effects/pressure.wgsl` returns a displacement field for the
composite. These concrete Modules form the initial [effects library](EFFECTS.md);
effect physics and shader optics can be reused without Stage identities.
`effects/rewind.wgsl` adds the blog's VHS tape interference (tear, snow on ink,
scanlines) to the same composite, controlled by `post.rewind`; it needs no
previous-frame textures or feedback. The composite's radial zoom streak
(`post.zoom`) and white flash (`post.flash`) sample the same developed exposure,
and `camera.quake` adds sustained trauma to the jolt rumble. Card deletion (glitch bands, hairline cut)
clips copies of the card's primitives by their bounding quads, since every
primitive rasterizes only inside its box. `Theme::Neutral` is the blog's "clear
neutral" palette: quiet frames and wires, ivory signals, and desaturated
semantic inks; it is the one theme whose status tones differ.
Incoming packets also sample `effects::surface` from the first visible-shell
contact, found by `Circle::entry_fraction` and the inverse packet travel curve.
The local dimple, particle emission, and hemisphere-masked spherical trace travel
outward from that contact; they do not scale the receiver or shift its ports.

Bolts, charge, shields, dissolve, and scans follow the same split
(`EFFECTS.md`): `effects::lightning`, `dissolve`, and `shield` own clocks,
seeded geometry, and particles; the Stage places them and adds their lights to
the sample's local-light list, and binding-free WGSL owns the optics.
Primitive kinds 10 (a plasma channel: a polyline whose points carry energy,
pure emission), 11 (a shield bubble with up to four contacts), and 12 (a scan
line) are emitted like the others. A bolt's endpoints are each outline's
crossing of the straight line between them (`Shape::boundary_toward`); a shield
is placed after the element it surrounds, so bolts can strike it. Struck orbs
and shields find each stroke with `Scene::strikes_on`, beside packet contacts.
Any primitive may carry a dissolve mask (`Prim::mask`, zero for none): a card
stamps it on its own primitives after drawing them, so fills, rims, and glyphs
burn along one field. That field is integer-hashed value noise evaluated
identically on the CPU, so ash leaves exactly where the rim passes. Charge
crackle and scans draw after the mask and after glitch/cut copies, so neither
is clipped or duplicated.

#### Forms, shapes, paths, and icons
The diagram vocabulary beyond cards and orbs is four more element kinds, each an
arm of `StageElement` and a method on `Painter`, with no new pipeline:
- `form` reuses the orb's material. `stage::form_points` generates each shape's
  deterministic points (`math::shapes`: `fibonacci_sphere`, `box_points`,
  `grid_points`, `cylinder_points`, `torus_points`) and pairs every shape with
  the one before it (`match_points`: greedy nearest claims, then pairwise swaps
  that shorten squared travel), once at preparation (`StageGpu::forms`). Each
  sample, `Scene::form` evaluates `stage::morph_point` for every point, turns it
  by tilt, spin, pitch, and roll, and takes the projected convex hull
  (`math::shapes::Polygon`, collinear points dropped, at most 32 vertices) as the
  form's outline, so beams, paths, packet labels, and lights meet its actual
  silhouette. `Polygon::port_toward` leaves where the ray toward the target
  crosses the outline, with the normal rounded over each corner, so a port
  slides continuously while a form tumbles. The painter shares the orb's dot
  loop (`Painter::particles`) and burst embers (`Painter::embers`); the dark body
  is the hull as a filled polygon. The orb itself is unchanged and keeps its
  exact pixels; the surface ripple stays spherical. Callout anchors resolve
  through `Scene::place` and use a form's resting outline, not its hull.
- `shape` flattens its figure to an outline (`figure_outline`, rounded corners
  as cubic quarter-curves), fills it as a polygon, and strokes it as a
  polyline, so draw-on, dashes, and arrowheads come from the existing polyline
  primitive. Unrotated rectangles attach like cards (side midpoints); a turned
  rectangle or polygon attaches to its hull. Wires meet a shape's outline
  rather than submerging. Like a card, its stroke catches the strongest
  reflection and its fill the strongest pool.
- `path` resolves waypoints each sample (`Scene::route`) into legs split at its
  stops. A hop touching an element is `math::shapes::connect` between outlines,
  exactly as a beam; point-to-point runs are straight and joined with rounded
  corners (`join_rounded`), or a Catmull-Rom or authored Bézier chain. Each leg
  is a `Link`. `Link::landing` marks open ends that show an arrival (cards,
  shapes, icons, free ends; for beams it equals the card sockets, so existing
  plans are unchanged) and `Link::tips` where a body's silhouette cuts a
  submerged leg, so arrowheads sit on the silhouette. A packet asks
  `Scene::packet_legs` for its legs: one for a beam, one per path leg on the
  same clock offset by `stage::packet::leg_start`; each leg paints and lights
  exactly as a beam packet.
- `icon` SVG (bundled Phosphor from `assets/icons`, compiled in, or path data)
  rasterizes into the Stage's R8 atlas after the text, so text-only plans pack
  identically, and draws as an atlas quad. Preflight parses icon SVG without a
  GPU.
Primitive kind 7 is a filled polygon (Quilez's crossing-count signed distance,
optional border, glow, and a flood pool); its points share the polyline point
buffer. Kinds 8 and 9 are reserved for this vocabulary.
Editor diff backgrounds union their weighted vertical intervals before pixel
coverage (`render/line_marks.rs`). Adjacent fractional rows therefore share a
single tint instead of double-blending an antialiased seam. Gutter signs remain
attached to the sampled line positions.

### Eased events

`TrackEventPlan::Ease` lowers through `TimedEvent::ease` to a `SegmentKind::Ease`
segment: position from `math::easing::Ease::sample`, velocity from its derivative
`slope`, settled exactly at the end. It replaces stepped `set` approximations of
timed curves, which stutter at 60 fps, and hands its velocity to a later spring.

### Math

`psychopomp::math` is the shared vocabulary for motion and geometry, grouped like
pmndrs `math`: scalar `lerp`, `inverse_lerp`, `remap`, `remap_clamp`, and
`smoothstep`; glam's `Vec2`, `Vec3`, and `Quat`; `easing` curves; `curve`
(`CubicBezier`, and `Polyline` with arc-length sampling and slicing); `shapes`
(`Box2`, `Circle`, convex `Polygon`, `Shape` outlines with facing `Port`s,
`connect`, deterministic 3D point sets from `fibonacci_sphere` to
`torus_points`, the `r2` low-discrepancy sequence, and `match_points`); and
`random::hash`. Renderers and Scene Programs compose these
instead of carrying private lerps, easings, or geometry.

Live shaders: when `PSYCHOPOMP_SHADER_DIR` is set, the stage reads `stage.wgsl` and
`stage_post.wgsl` from that directory instead of the compiled-in copies, so each
`plan frame` or sheet reflects shader edits without recompiling Rust.

The editor adds `panel-x` (a translation of the projected card), `panel-opacity`,
and Line Marks. Marks draw in the shared text pass before code, so the native
preview path and the projected export path agree; a non-zero `panel-x` or partial
opacity disables the preview shortcut. `inlineReveal` is optional.

`psychopomp::highlight::typescript` compiles one TypeScript line into styled spans
for editor recipes. It is a line-local approximation for explainers, not a parser.
`psychopomp::editor::diff` builds a Stepped Diff on top of it: an `editor` actor
whose lines keep identity across steps, with room-opening snapshots and removed
lines' `mark.*` channels turning red just before they leave.

## Encoding Is One Concrete Adapter

`plan_runtime/delivery.rs` owns PNG and MP4 delivery separately from `PreparedPlan` preparation and sampling. Video exports keep the authored timeline, full visual quality, shutter samples, and original audio placements.

A Reel (`plan::ReelPlan`) is delivery, not a scene. The lightweight crate owns its
validation and timing: `spans` places segments on one clock and `layers_at` returns
the one or two segments visible at a time with eased mix weights. `plan_runtime/reel.rs`
prepares every segment once on one renderer, samples each visible layer at its local
time, and mixes opaque frames (a dip mixes over the theme's empty background). Each
segment's media placements are shifted onto the reel clock with
`MediaPlacement::shifted` and encoded through `exposure::encode_exposures`, the same
exposure and FFmpeg path as a single plan. During a transition each segment's frame is
reused across shutter samples and frames while its visual sample key holds
(a few recent frames, keyed by segment and key); Stage segments carry ambient
time and always render. A segment shown alone through a frame
renders its own exposure (so a Stage keeps its GPU shutter); mixes and zooms take
16 samples averaged on the CPU. `plan render`, `frame`, `snapshot`, `validate`,
and `inspect` recognize a reel by its `segments` key; `--cue` selects
one segment by scene ID. A zoom transition resolves each layer's
`ReelZoom` transform (geometric scale, focus-to-center travel, rounded corners on
the incoming card) and warps the rendered frames bilinearly before mixing; zoom
progress is part of the sample key so the move keeps its motion blur.

A wipe is the same kind of reel transition, not a plan recipe: comparing two
frames needs two independently prepared segments, which only a reel has (a plan
has one root, and recursive child plans would be the scene graph this project
avoids). `plan/wipe.rs` computes the divider position in closed form from the
transition's elapsed time: minimum-jerk sweeps between rests, each taking time in
proportion to its distance, with optional holds where the divider rests
mid-frame. During a hold both segments keep their own running clocks, so a held
compare is just a long transition whose two neighbors hold still.
`render/wipe.rs` mixes the incoming frame behind an antialiased divider, darkens
the outgoing side with a soft Gaussian shadow, draws a two-pixel theme-ink line
that fades near the frame edges, and places optional labels as caption chips that
ride the divider and fade as their side narrows. The divider position is part of
the reel sample key, so sweeps get motion blur and holds collapse to one sample.

Composited Transitions are reel transitions too. Their settings live in the
style itself (`Push(direction)`, `Match(target)`, `Iris { ring }`), so older reel
JSON and struct literals are unchanged; `layers_at` returns the outgoing layer
and the incoming layer carrying a `TransitionPhase` (style, linear progress,
duration, focus). `plan/transition.rs` owns their poses in closed form: travel
curves with exact rates (minimum-jerk push, critically damped slide, a whip
that is minimum-jerk travel through a minimum-jerk clock), the iris radius,
the match camera (geometric scale, straight-line travel of the matched center,
one transform for both frames so the shared element agrees), card and cube
turns with their pull-back, and the glitch, flash, and leak envelopes.
`render/transition.rs` composites one sample's two frames in linear light,
row-parallel over `std::thread::scope` and deterministic:

- `travel`: each frame line becomes running sums, so a box smear of any length
  costs the same; smears fill the gap between the 16 temporal samples (a whip
  exposes 1.4 shutters longer, with a faint long streak), so samples join into
  one streak. A slide dims and shadows the frame it covers.
- `reveal`: an iris opens from the focus center with a soft, speed-widened
  edge, a shadow on the outgoing side, an optional accent ring, and a slight
  settle of the incoming frame. Ink thresholds a domain-warped fractal noise
  field (cached per frame size and focus) that is rank-equalized, so a
  threshold covers exactly that share of the frame and the edge width is
  measured in pixels from the field's slope.
- `turn`: frames are sampled through inverse transforms from a mip chain. A
  match fades the element into its counterpart before the scene around it,
  and feathers a shrunken frame's border so its vignette never draws a box.
  Flips and cubes ray-cast faces in a turned space (vertical motion swaps x
  and y), shade them from an overhead key, light their rims, and average
  anisotropic footprints with taps along the long axis.
- `light`: a glitch holds discrete corruption frames (24 per second) of torn
  bands, misread color planes, and displaced macroblocks around a hard cut; a
  flash overexposes then washes to the theme's ink; a light leak screens
  elliptical warm glows in from the left edge while the frames swap beneath.

Transition progress joins the reel sample key, so every style is exposed
through the shutter. At most two segments are visible at any instant; J- and
L-cuts are overlaps whose picture cuts at one end, so both segments' media
play through the overlap.

`psychopomp/src/playback.rs` derives numeric step destinations from a renderer-prepared Timeline, after semantic geometry has resolved. Next, Previous, First, and Last append only changed channel targets through the shared Timeline compiler. Each spring therefore inherits position and velocity, including mid-flight reversals; unchanged destinations do not restart motion. Per-channel motion profiles come from the destination's latest authored spring (or its first spring before any event; set-only channels use a 0.4-second zero-bounce default). Replay alone resets to the entry pose. The local clock freezes on pause or once all channels settle, without retiming the authored video. Immutable `Arc<Timeline>` revisions make sampling history-independent even while input creates a newer revision.

`plan_runtime/presentation.rs` owns winit lifecycle, slide/step navigation, full screen, letterboxed resizing, and smooth/pixelated display filtering. `presentation/worker.rs` retains the deck's prepared scenes, fonts, and GPU. At most one render is in flight; requests and results carry slide identity and immutable timeline revisions, so switching slides cannot display a stale result from another scene. Each slide retains its selected step and paused local clock while inactive. Explicit pause stays paused on return; previously running motion resumes. Held/paused scenes sleep unless a planned Task requests ambient clock advancement. Generic State Channels, recorded media, and Rolling Numbers (whose changes follow the authored clock, including Changed Files totals that roll) remain unsupported by interruptible playback; video export supports them.
`plan_runtime/presentation.rs` owns winit lifecycle, slide/step navigation, full screen, letterboxed resizing, and smooth/pixelated display filtering. `presentation/worker.rs` retains the deck's prepared scenes, fonts, and GPU. At most one render is in flight; requests and results carry slide identity and immutable timeline revisions, so switching slides cannot display a stale result from another scene. Each slide retains its selected step and paused local clock while inactive. Explicit pause stays paused on return; previously running motion resumes. Held/paused scenes sleep unless a planned Task requests ambient clock advancement. Generic State Channels, recorded media, and Rolling Numbers and Subtitles (whose changes follow the authored clock) remain unsupported by interruptible playback; video export supports them.

The GPU-free `presentation/scheduler.rs` owns request eligibility, complete-sample
equality, invalidation, completion freshness, and phase-preserving deadlines.
`RequestStamp` provenance is distinct from the worker's visual cache key and the
uploaded pixel `Arc`. A stale completion releases the one-in-flight slot without
replacing front pixels or clearing a pending repaint.

Native inspection uses `PlaybackSpeed` (1x, 0.5x, 0.25x, 0.1x) to divide elapsed
wall time. A speed change reanchors at the same local time without rebuilding the
Timeline. Frame steps change only the paused sample cursor by 16.667 ms and stop
backward movement at the latest navigation boundary. Both operations increment
the presentation revision, rejecting old in-flight frames. Shift+R uses ordinary
Replay followed by pause so the first frame and delayed word starts can be inspected.

`presentation/debug.rs` captures local/navigation time, speed, phase, and pending
start counts with the render request. Header readouts sample each word's actual
spring progress. `render/debug.rs` paints a bounded optional HUD over a clone of
the worker's clean cached frame; hiding it restores the exact clean cache object.
The HUD has at most eight replace-in-place glyph-cache slots and never enters
Scene Plans, visual sample keys, or file exports. Speed/debug are session-local,
not saved alongside theme preferences; benchmark mode rejects slow/debug options.

Semantic coordinates retain a numeric expanded-layout baseline plus private companion weights compiled by `plan_runtime/attachments.rs`. Sampling applies each weight to the difference between expanded and visible target geometry, including product-rule velocity. Target measurements use the same span partitions and shaping as inline painting; weight tolerances are normalized to the measured coordinate extent. Companion tracks participate in native retargeting, preserving continuity when switching between moving targets and literal coordinates. Line-layout and reveal drivers must use literals: semantic feedback into their own geometry is rejected. These are prepared renderer tracks, not new public Scene Plan fields.

`psychopomp/src/task.rs` describes planned Task states, and `plan_runtime/task.rs` lowers them into continuous geometry, color weights, activity, and independent content/bubble channels. `TaskVisualFrame` reuses the existing Rust Task material, icons, result clipping, error bubbles, jitter, and energy sweep instead of importing a browser runtime. The source Pixi profiles remain distinct: 0.2-second/bounce-0.5 height, 0.35-second/bounce-0.35 width, approximately 0.167-second icon opacity/scale/blur, and 0.25-second/bounce-0.4 result scale with 0.15-second deblur. State content can reverse mid-transition through the shared Timeline. The activity track keeps the local clock and visual sample key advancing; pause, slide departure, and reduced motion freeze it. This is a scoped native adaptation of Effect Institute's Pixi blocks, not a general implementation of browser component state or actual Effect execution.

`render/task/content.rs` contains sampled `ContentPose`, `BubblePose`, and `TaskContentFrame` values. It does not remap a shared state-progress spring through a second easing or delayed visibility window. Preparation compiles `content.<state>.*` and `bubble.<state>.*` channels, including the source's independent bubble rise/fade/deblur. These channels enter native Playback like any other trajectory; no edge-triggered timer or direction branch can reset a reversing pose. The symbol rotates independently while its clip remains aligned with the body. Error text, background, and tail share one cached bubble sprite so its entrance transforms the whole surface. An outgoing bubble is not culled by its faster icon fade. Unchanged channels keep content still. No public Scene Plan fields or generic transition framework are added.

The source's instantaneous running toggle becomes a short 0.06-second continuity ramp, without waiting for content to disappear. Retained symbols use a symmetric 0.7 hidden scale instead of the source's new-icon mount resets and 0.9 exit scale; selective symbol rotation remains an intentional native flourish. Results retain a 0.5 hidden scale. These adaptations preserve arbitrary-step continuity, so the timing tests claim source spring-curve parity, not complete source-renderer pixel equivalence. `crates/psychopomp-render/tests/fixtures/effect-task-timing.json` comes from the source project's pinned Motion DOM 12.42.2 generator and tests actual compiled channel samples, including overshoot and separate result deblur.

Transformed sprites use denser 5×5 binomial filter taps: the larger content defocus exposed visible displaced copies with the former 3×3 taps, particularly in error-bubble text. Tap positions still vary continuously, and the unblurred identity case retains direct bilinear sampling. This improves a sampled optical effect, not the trajectory or the export shutter model.

`presentation/gpu.rs` owns the final native surface, independently of scene recipes. It uploads changed RGBA frames to one sRGB texture and draws a letterboxed triangle with linear or nearest filtering. FIFO presentation requests one-frame latency. Resizing or switching filters reuses uploaded pixels; lost/outdated surfaces are rebuilt and zero-sized/occluded windows defer presentation. This replaces the measured CPU scaling/softbuffer bottleneck without migrating choreography or code layout to another renderer. `presentation/benchmark.rs` measures one warmup plus nine native interruption rounds; `perf/native-playback.md` records the comparison and its limits.

Native sampling follows the current monitor's reported millihertz refresh rate (60 Hz fallback), with an explicit `--fps` override independent of video timing. Moving, resizing, or refocusing the window rechecks the rate. Neither setting changes FIFO synchronization or the one-in-flight worker bound. `--benchmark` records display rate and acquisition wait; `--benchmark-gpu` additionally waits for each submitted draw to finish and reports scene sampling plus completed upload/draw work, excluding drawable acquisition. That diagnostic wait is never enabled during ordinary playback. Display refresh, rendering headroom, and verified scanout FPS are distinct measurements.

The native preview is a deliberate quality profile, not a second choreography implementation. A neutral editor with no visible pointer reuses static composed chrome and paints code directly, omitting export's final optical resampling. Dynamic focus/highlight overlays use the same WGSL recipe in a chrome-free pass, avoiding an optical-card rebuild whenever an attached highlight moves. Unsupported transforms/effects fall back to the full renderer. `--full-quality` disables this shortcut; export never enables it. Pixel equality is required across sampling order within a profile, not between profiles. Higher-DPI glyph rasterization, presentation audio, and live source reloading remain future work.

The cheap overlay pass is limited to bounds safely inside the card body; overlays that can overlap title/border pixels or escape its clip use the full path. Two filename-keyed chrome images are retained, and each slide's initial resources are warmed before the native window opens. Frame deadlines retain their phase across late wakes rather than drifting relative to the previous request. Missed slots are skipped, not queued.

`crates/psychopomp-render/src/encode.rs` owns the FFmpeg process, raw-frame protocol, audio placement filters, argument construction, and exit validation. The Interface accepts tightly packed RGBA frames plus compiled audio media placements. FFmpeg trims immutable source ranges, applies each clip's non-destructive gain, shifts clips onto the composition clock, mixes overlapping layers through a peak limiter, and encodes AAC beside H.264.

FFmpeg remains a subprocess because it avoids unsafe bindings and codec linkage while preserving access to the installed encoder set. A second encoder is not currently justified.

## Scene Programs Own Choreography

Each Scene Program under `scenes/` keeps its documents, snapshots, semantic targets, and choreography local and emits a Scene Plan. There is no scene trait, registry, or generic lifecycle. `crates/psychopomp/src/author.rs` provides stable handles for those programs; continuous changes lower into the Timeline compiler and discrete values into generic State Tracks. Shared authoring operations move into `psychopomp` only when repeated usage reveals a deeper interface.

Scalar targets may remain semantic while authoring. Scene Plans use stable `SemanticTargetPlan` declarations and `ScalarPlan` target references to request target edges, widths, centers, line positions, or attached offsets; the renderer resolves geometry once before compiling the numeric Timeline.

`scenes/hero` contains a stable editor document, logical semantic ranges, actors, channels, and exact choreography. It emits `scenes/hero/hero.plan.json`; a byte-equality test keeps that generated canonical plan synchronized with Rust source. `crates/psychopomp-render/src/main.rs` embeds the plan as the default render, resolves its editor targets through `plan_runtime/editor.rs`, and renders through the shared plan path. Delivery dimensions, shutter exposure, and accumulation live in `crates/psychopomp-render/src/exposure.rs`, shared by Scene Plans and reels.

Scene Programs may serialize generated Scene Plans as JSON for the process protocol, but JSON and TypeScript are not source authoring languages.

## Current Stack Decisions

- [`wgpu 30`](https://github.com/gfx-rs/wgpu) is the headless GPU substrate. Rendering targets an offscreen texture without a window or surface.
- [`cosmic-text`](https://github.com/pop-os/cosmic-text) shapes and rasterizes CommitMono lines into stable cached sprites.
- [`glyphon`](https://github.com/grovesNL/glyphon) was evaluated and removed: repeatedly preparing a dynamic GPU glyph atlas across temporal samples can invalidate glyph coordinates during atlas growth. Cached line sprites fit Psychopomp's stable-identity model better.
- WGSL remains the shader language because it is native to wgpu and translated by Naga.
- An FFmpeg subprocess handles H.264 encoding. [`ffmpeg-next`](https://github.com/zmwangx/rust-ffmpeg) is maintenance-only and adds an unnecessary FFI seam.
- [`Vello`](https://github.com/linebender/vello) remains deferred because its API and wgpu compatibility are still moving. `lyon` is the likely addition if authored vector paths become necessary.
- The current RGBA8 render target is sufficient for the visual prototype. Temporal samples are decoded to linear light before CPU accumulation and converted back to sRGB once per output frame, avoiding dark gamma-space motion trails. A production compositor should render and accumulate directly in linear `Rgba16Float`, then tone-map into the delivery color space.

## Explicit Non-Abstractions

The prototype does not have a generic scene graph, renderer trait, plugin interface, render graph, dynamically loaded Rust library, or recursive slot AST. The core crates exist only to keep lightweight Scene Programs independent from the heavyweight persistent renderer and from the HTTP stack that generating media needs. Further package seams require another demonstrated compilation or deployment need. `psychopomp-media` has one `Generator` seam, justified by its test fake and by the offline modes that must never reach a provider; its two providers are match arms, not implementations of a provider trait.
