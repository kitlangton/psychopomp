# Prototype Findings

The opening sections retain the **initial headless prototype** findings, not the
current duration, performance, or capability list. Current contracts live in
`ARCHITECTURE.md` and `SCENE_PLANS.md`; the dated/ordered experiments below record
what each revision established. Historical review ports are not live-service status.

## Question

Can a minimal headless Rust stack render attractive, stable technical-video frames and encode them without a browser?

## Answer

Yes. The initial prototype rendered a three-second 1920x1080 video at 60 fps on an Apple M2 Max, read frames back from Metal, and streamed raw RGBA pixels to FFmpeg. Its 180-degree shutter render evaluated eight complete temporal samples per frame and finished in about 25 seconds.

The output uses:

- a fullscreen WGSL shader for a flat, gradient-free dark editor and focus treatment
- `cosmic-text` for shaped, syntax-colored CommitMono line sprites
- stable line IDs and compiled before/after code snapshots
- analytic damped springs for panel movement and focus intensity
- separate layout and content progress so space opens before new lines enter
- eight complete scene samples per output frame for shutter-based motion blur
- an offscreen `Rgba8UnormSrgb` texture with GPU-to-CPU readback
- FFmpeg and `libx264` for a deterministic 1080p60 H.264 artifact

Run the prototype with:

```bash
cargo run --release
```

The default artifact is `output/psychopomp-prototype.mp4`.

## What This Proves

- `wgpu` works headlessly through Metal without a window or browser.
- `cosmic-text` produces crisp CommitMono typography with per-span styling.
- Cached line sprites avoid dynamic glyph-atlas corruption and make opacity deterministic.
- Stable closing lines move to new rows without being replaced when code enters.
- GPU rendering and synchronous readback are already fast enough for a short offline-rendered prototype.
- Temporal sampling blurs moving geometry and text while settled code remains crisp.
- FFmpeg can remain a narrow encoding boundary rather than part of the rendering engine.
- Arbitrary-time analytic motion fits frame-independent rendering.

## What This Does Not Prove

- camera choreography
- a scene IR or TypeScript authoring frontend
- GPU composition of cached line sprites
- fast interactive preview

## Current Stability And Native Presentation

- The second simplification pass removed duplicate relative-animation traversal,
  repeated deployment catalog validation, discarded raster work during code-target
  measurement, unused private layout/clipping paths, and deck-index-based scene
  reuse. Focused published-code tests now read only their section fixtures:
  measured FFprobe launches for six tests fell from 205 to zero while full
  chapter/media integration coverage remains. This is removed work, not a measured
  rendering-speed claim. A proposed single-copy State Track was rejected because
  it changed predecessor clone isolation for interior-mutable payloads.

- Media verification reproduced two lifecycle failures. A partial frame read
  overwrote bytes while leaving the previous cached identity valid; reads now
  invalidate that identity before fallible I/O. FFmpeg's inherited interactive
  stdin also consumed a brace from pipelined renderer requests on a cold decode.
  The decoder now receives null stdin, with unchanged decoded bytes and filters.
  Evidence is under `output/simplify-pass2/`.

- One published-code artifact has pre-existing cross-process pixel variation:
  `abort-signal-infusion` at 7.970 seconds, with overlapping running statuses on
  adjacent rows. Unchanged baseline executions produced both observed hashes;
  the initial comparison differed in 1,609 RGBA components by at most two levels.
  Unordered status iteration and rounded overlapping effect blending remain
  unchanged. A later exact match is not proof of cross-process determinism; this
  pass leaves that separate paint-order question open.

- The session-story A/B/C video trial did not improve the architecture article:
  Kit preferred the original simulation. `experiments/session-story-prototype`
  records that rejected direction and its technical evidence. It is not an
  approved UI Surface, live-session importer, or supported WASM recipe; the live
  article was not modified.

- Formation follow-up: Isometric boxes now open in width as well as depth, rather
  than fading in at full width across a retained neighbor. The 340 ms zero-bounce
  width reveal leaves clearance for the projected side faces while retaining the
  220 ms rise and 120 ms deblur. Labels disclose inside the sampled top face at
  their original font size. The client's labels are now TUI 1, TUI 2 and DESKTOP
  in both views; Flat's choreography is unchanged. The scalar formation test and
  narrow-face ink test both failed before the change. `perf/iso-polish.md` records
  this follow-up under `output/iso-width/`. Workspace checks, all 40 GPU tests and
  the unchanged 48-case browser gate pass (43 exact, ±1 maximum). The held pose is
  unchanged after accounting for the requested labels. Latest browser review: 5206.

- Isometric follow-up: Kit likes the styling, but rejected the foreground overlap
  and opposing grow-up / move-down entrance. Whole-box paint packets now sort by
  sampled camera depth, with authored order only breaking equal-depth ties. The
  middle pair no longer has blanket foreground priority. A minimized GPU test
  originally changed 100,648 components when catalog order alone was reversed.
  The entrance now grows from a fixed base on a 220 ms critically damped spring,
  with 120 ms deblur; the descending lift and bounce are gone. Flat is unchanged.
  Workspace/static checks, all 39 GPU tests and the 48-case browser comparison
  gate pass (45 exact, ±1 maximum; tolerance unchanged). All 24 Flat captures
  remain exact. The current browser revision is isolated on port 5205.
  See the follow-up in `perf/iso-polish.md`; the earlier lift trial remains below
  as historical evidence, not the current choreography.

- Kit likes Isometric, and requested side-center connections plus livelier box
  entrances. The port bug had two causes: top-plane coordinates (14 px off the
  old 28-depth midpoint) and blanket wire-behind-box paint order. Pure and pixel
  regressions failed before the fixes; height-aware visibility now exposes the
  near side's midpoint without painting through opaque rear faces. New Iso-only
  depth/lift tracks grow and settle the volume, with clearer face/rim shading and
  much less entrance blur. All 24 Flat reference frames remain identical.
  `perf/iso-polish.md` records the 38 GPU tests, unchanged 48-case browser gate,
  artifacts and performance limits. The updated browser review is isolated on
  port 5204; the user's previous 5203 page/assets are left alone.

- Kit rejected the dotted/enclosing UI around Daemon / merge: keep only labeled
  boxes and attached lines, with Isometric as a possible alternative. The scene
  now emits bare Flat and shallow Isometric plans/deck with identical motion and
  identity. The diagram renderer is a shared GPU pass with R8 labels and 4× spatial
  AA, replacing its CPU backdrop/card painting. The isolated browser probe stages
  that same renderer, shader and Start Delay preparation; `/diagram.html` exposes
  both views without canvas chrome. Initial native runs are mostly 59–60 frame
  submissions/sec with occasional spikes; browser completed batches measure about
  2.25/2.77 ms per frame, not scanout. The first 48 cross-host diagram captures
  include 44 exact PNGs and four with only 1–16 components differing by ±1.
  `perf/diagram-gpu.md` records the scope, measurements and limits. Isometric is an
  audition; no aesthetic approval or production browser API is implied.
  The final GPU filter uses 5×5 support to remove coarse wide-blur copies. Native
  runs remain 59–60 submissions/sec for Flat and 57–60 for Isometric, with pacing
  spikes (p95 18.50/19.88 ms). The denser browser batches measure 3.08/4.86 ms,
  CPU submission about 0.06/0.07 ms. Final cross-host proof is 45/48 exact captures;
  three differ only by ±1 in 3–84 components. The reviewed color-rounding strip
  and the bounded comparison gate are documented, not claimed as exact parity.

- The earlier Daemon / merge CPU speed follow-up fixed measured rendering lag, not the
  authored animation speed. A current-theme static backdrop cache and exact
  constant-texel sampling shortcut improve native throughput from **16–17 to
  44–49 submissions/sec** (mostly 47–48). Final repeats reduce median worker
  rendering from 44.50/45.26 ms to 15.01/14.78 ms and median-round p95 submission
  intervals from 105.79/105.99 ms to 38.24/37.45 ms. It is not locked 60 Hz under
  rapid blur-heavy reversal. The fixed-clock median is 14.836 ms versus 45.892;
  all **195 final before/after PNGs** and the Scene Plan are byte-identical.
  Cache cost is one 7.91 MiB RGBA backdrop per prepared diagram. The shared sampler
  keeps the original mixed-edge/alpha filtering; blur weighting is unchanged.
  Extra blur accumulation and inline-hint trials were discarded for lack of a
  durable pacing win. `perf/daemon-diagram.md` records all trials and uncertainty.
  That CPU diagram/backdrop implementation has since been replaced by the bare GPU
  recipe above; the shared card sampler optimization remains for other consumers.
  Workspace checks and all **38 GPU/artifact tests** pass. The two 72-frame
  shutter-sampled clips and 132-frame interrupted native-path movie decode to
  identical frames before/after; a perspective/blurred hero-editor PNG also
  remains byte-identical. No artifact-level GPU/font/FFmpeg blocker occurred.

- `scenes/opencode-architecture` adapts the supplied **00A · DAEMON / MERGE**
  reference into a native four-step scene: one pair, two pairs, three pairs, then
  a shared daemon. `prototype-diagram` keeps finite boxes/ports/wires and paint
  separate from the scene's OpenCode choreography. The original project was only
  read. Its 450/320 ms spatial/merge profiles, entrance offsets, visual styling
  and sharp rolling caption are retained or explicitly adapted; CSS easing/filter
  parity is not claimed. Wires attach to sampled bounds, and the retained server
  paints in front to occlude departing labels. The new scene exposed a shared
  card bug: a zero-width border painted exact fractional boundary pixels. A pure
  regression test failed before adding a positive-width guard in both card paths.
  Native recipe admission also has a regression test, not only a renderer test.
  Workspace checks and all **36 GPU tests** pass. The diagram proof covers exact
  native/export onset pixels, unchanged client pixels throughout the merge,
  cancelled pending starts, all-channel position/velocity and pixel continuity
  through rapid retargets, and deterministic out-of-order sampling. Full-scale
  frames, two 72-frame 1080p60 shutter-sampled exports and a 132-frame interrupted
  native-path movie were decoded/inspected under `output/daemon-merge/`.
  The native viewer was opened in Original; aesthetic approval and automated live
  scanout remain unclaimed. Performance was measured in the follow-up above.
  It is not part of the browser-grid probe.

- Browser autoresearch retained direct canvas delivery and a shared R8 label atlas.
  Same-session candidate/control testing passed 27 exact pixel cases at unchanged
  1080p/4×AA. Explicit requested textures fell from 168.7 to 152.9 MiB including
  labels, excluding swapchain/browser/driver overhead (not measured physical VRAM).
  Frame-time changes were small/noisy; no new displayed-FPS claim. A property-ID
  cache and bounded composite were discarded for failing to improve the primary
  completed-work metric consistently. See `perf/browser-grid.md`. The source,
  motion, and shader behavior remain shared; the atlas saving also applies native.

- An isolated `experiments/browser-grid-prototype` now runs the two prepared chess
  diagrams through WASM/WebGPU without changing native motion or rendering. It
  reuses the actual grid recipe and shaders, bakes labels, and delivers GPU pixels
  to a canvas without per-frame CPU readback. The interactive core is ~298 KiB
  gzipped (demo HTML/JS and an ~89 KiB static fallback are additional). Six authored
  growth/rotation/cutaway frames, three regrouping frames and sixteen interrupted
  navigation samples closely match native pixels, with sparse edge differences;
  this is not bit equality across backends. In Chrome 152 on this Mac, one short
  post-build stress run had rAF interval p50/p95 16.7/17.5 ms and CPU sample/submit
  p50 1 ms. The current 1080p GPU target budget is still large (~158 MiB before
  atlas/swapchain); mobile, battery, multiple embeds and other browsers are not
  proved. See the experiment's `NOTES.md` for full scope and measurement caveats.
  This is prepared-grid feasibility, not a browser library or live-code renderer.

- Native motion inspection now offers S/Shift+S speed (1x/0.5x/0.25x/0.1x),
  paused comma/period frame steps, Shift+R replay-paused, and a D debug HUD.
  Speed reanchors the existing local clock; it does not retime springs or delays.
  The HUD reports the rendered sample's time, phase, pending starts, and header
  word progress without contaminating clean cached pixels. Debug text caches are
  bounded; speed/debug are not persisted or applied to exports/benchmarks.
  The word-stagger inspection confirms separate but heavily overlapping starts:
  at 120 ms scene time, Every is at 47%, word at 19%, and later words at 0%.
  The 60 ms gaps are not sequential word completion, and fast reversals deliberately
  redirect already-moving words without reapplying entrance waits. Choreography
  was left unchanged; slow fresh Replay makes the distinction inspectable.
  Workspace checks and all 35 GPU checks pass. Native/export poses match at the
  same scene time; HUD-off restores the same clean cache object. Full-scale HUD
  frames and native-path single-sample 1080p60 review movies at normal/quarter speed
  (84/336 frames) were decoded and inspected as strips under `output/slow-motion/`.
  Automated live scanout and a performance claim remain outside this verification.

- The rise-title follow-up adds a separate eighth showroom slide: reflection rise,
  60 ms word staggering, and a tighter 25 ms reflected stagger with 280 ms motion.
  The original three header trials and the sharp/snappy prose treatment remain.
  Reflection is a vertically mirrored copy of the same sampled glyph pose, clipped
  below the fixed edge and faded linearly away from it. Words partition one shaped
  line at whitespace rather than changing layout as they appear.
  This required a real scheduling seam: optional resting-source `StartDelay`
  metadata in Playback. Superseded pending starts are cancelled; unchanged pending
  starts keep their due times; moving words redirect immediately with velocity.
  Pausing freezes waits, reduced motion cancels them, and Replay restarts explicitly.
  Existing recipes remain unstaggered unless their renderer opts in. This does not
  turn arbitrary authored delays into automatic native choreography.
  Workspace tests, formatting, strict Clippy, and all 34 GPU checks pass. The
  native stagger matches authored/export pixels at its onsets; reflected ink is
  confined below the edge; cancelled words leave no residue. The original 120 ms
  rise frame remains byte-identical. Full-scale samples and decoded 1080p60
  entrance/exit exports (84/72 frames) were inspected as stepped strips under
  `output/header-variants/`, and the eight-slide native viewer was refreshed.
  Automated live scanout and a new performance benchmark remain unverified.

- Review of the slideshow showroom caught incorrect syntax coloring: whole union
  segments included their separators, and the third literal had the Type color.
  `TextPart.spans` now separates syntax runs from animation identity; `type` is
  Keyword, `Color` is Type, all literals are String, and separators/punctuation are
  Plain, without replacing the stable parts or changing their width trajectories.
  The emitted-scene regression reproduced the original failure. Normal rich-text
  paragraphs no longer inherit blur: the default `fade_blur` is zero and prose
  uses 160 ms zero-bounce fades. The user liked the rise-title treatment; all three
  header trials explicitly retain 4 px fade softness and their previous timing.
  A GPU regression compares default paragraph fades with the explicit sharp path.
  `plan steps` has no Editor deltas for this width-text recipe; its identity and
  syntax are checked by the recipe-specific tests instead of claiming that CLI
  alone validates it.
  Workspace checks and all 32 GPU tests pass. The 120 ms header PNG is
  byte-identical before/after; styled runs retain the solid part's exact glyph
  alpha geometry. Entrance/exit prose exports (48 frames each) and a corrected
  width reveal (72 frames), all 1080p60, were decoded and inspected as stepped
  strips under `output/paragraph-polish/`. The refreshed native showcase retains
  the saved-theme control; live scanout is still not automatically captured.

- The seven-slide `slideshow-components` showroom adds native bounded Markdown
  (bold/italic, inline/fenced code, headings, lists, quotes, links, strike/rules),
  three header entrances, stable width-revealing type segments, and a two-set
  Venn diagram adapted from visual-types. The Venn hatch uses sampled intersection
  geometry, including nesting and circle-to-square changes; label leaders attach
  to their boundaries and remain outside the combined extent. Normal tables now
  default to full dividers, with row-only rules retained as a configurable option.
  Original, Evergreen, Tokyo Night, and Pure Black paint the native recipes;
  T/Shift+T saves the choice across restarts without changing Playback. Explicit
  `--theme` delivery shares those paints without reading personal preferences.
  Workspace tests, formatting, strict Clippy, and all 30 GPU checks pass. Theme
  round-trips restore original pixels, Pure Black backgrounds are exact zero,
  and interrupted/skip/reduced-motion tests cover the new components. The legacy
  board-II PNG remains byte-identical. Full-scale themed typography/table/editor
  frames plus decoded 72-frame 1080p60 header, width, and Venn exports were inspected
  under `output/slideshow-components/`. The native showroom is open for aesthetic
  review; this is not a measured performance result or automated scanout proof.
  Markdown documents remain immutable, fenced code is not syntax-highlighted,
  and HTML/images/Markdown tables/math remain outside this bounded text recipe.

- The existing `keyed-grid` recipe now has optional `GridStylePlan` presentation:
  checkerboard, background-matching, uniform, and row-banded fills; full, row-only,
  or no rules; and a conventional table layout with unequal columns, padded
  proportional text, alignment, and independent display headings. Table growth
  anchors against the complete catalog so retained rows and headers do not move.
  Table headings rotate with their plane; row rules omit the rear slab rim.
  Unfilled material remains opaque and unlit, preventing rear label/edge leaks.
  The comparison deck is emitted by `psychopomp-keyed-grid --styles`.
  All 28 GPU checks pass, including fixed retained pixels, paint/rule selection,
  hidden-payload canaries, interrupted navigation, and reduced-motion endpoints.
  The original board-II held PNG remains byte-identical. Full-scale frames and
  three decoded 72-frame 1080p60 exports (growth, rotation, unfilled cutaway) were
  inspected under `output/grid-styles/`. This is a presentation extension over a
  fixed catalog, not general record sorting, middle insertion, or cell editing.

- Prototype Typeset and Collection words now soften as they fade in/out, using
  the existing opacity with a maximum 4-output-pixel blur sampling offset. No
  layout, timing, connector, or focus track changed. GPU checks compare partial
  fades with the prior sharp compositor and confirm that fully present words
  remain pixel-identical even while moving. The held Typeset PNG is byte-identical
  to the pre-blur artifact. Matched entrance/exit frames and decoded, stepped
  exports are under `output/component-word-blur/`; all 26 GPU checks pass.

- The seven-slide data-modeling adaptation was judged too ugly despite passing
  its technical checks. More lesson-specific layouts are not the next milestone.
  `scenes/component-prototypes` instead trials reusable measured Typeset,
  Collection, and Connector overlays, plus one composition using all three.
  Large proportional text, no item cards, quiet underlines, and genuine anchored
  Bezier ink are the proposed direction, not a validated aesthetic win. Code and
  Grid still have the separate exclusive-root composition limitation.
  The four component trials pass interrupted forward/back/skip pixel and anchor
  continuity checks, reduced-motion/held-frame equality, and an insertion check
  that new ink fits the currently opened gap. Three 72-frame 1080p60 exports
  (typeset insertion, collection folding, and composition) fully decode; full-size
  frames and stepped strips were inspected under `output/component-prototypes/`.
  The native showroom is open for normal-speed aesthetic review. The recipes
  remain provisional despite passing all 25 ignored GPU checks.

- `scenes/data-modeling` is a seven-slide, 30-step native adaptation of the
  original types/cardinality opening, Boolean/Toggle pairing, joystick fit,
  sum/product counting, and nullable-pair error model. It is not the full talk.
  A small Value Token overlay reuses card coverage and fractional cached text;
  candidate representations stay in separate columns rather than crossfading
  readable alternatives over one another. The code edit retains the signature
  row, prefix, and suffix. `plan steps` reports one semantically intentional
  `Error` substring warning in the `UserOrError` replacement, no unsettled holds.
  GPU checks cover every slide through interrupted forward/back/skip navigation,
  position/velocity and boundary-pixel continuity, out-of-order sampling, and
  exact equality between reduced-motion destinations and authored held frames.
  Token tests cover fractional coverage, hidden glyph warming, and zero-opacity
  endpoints. The product slide's optional scale channel has a pixel-bounds proof;
  the prior chess board-II held PNG remains byte-identical with the default fit.
  Full-scale frames and validation logs are under `output/data-modeling/`.
  The return-type change (72 frames), forward/back mapping (252 frames), and
  table growth (72 frames) also render and fully decode as 1920×1080/60 fps MP4s
  with the existing eight shutter samples; stepped strips were inspected.
  This is offscreen/presentation-path evidence, not automated native scanout or
  a new performance benchmark.

- The board-II cutaway still had a legacy 18% emphasis target on unselected
  layers. This dimmed the visible outgoing front grid while the selected layer's
  top/side edges stayed bright. Slice focus now changes clipping only, not cell
  contrast. GPU checks compare the actual transition with identical geometry at
  normal contrast, including reverse/skip/interrupted native navigation. Evidence
  lives under `output/grid-cutaway-contrast/`; stroke width, palette, geometry
  timing, and the shader's existing depth shading are unchanged.

- A regroup-to-volume transition exposed dark remnants of outgoing headings.
  The old cell shader returned alpha 1 even for almost-transparent text, painting
  background-colored glyphs over the grid and writing depth that blocked strokes.
  Headings now use a premultiplied-alpha overlay after the material/stroke
  composite, with no depth writes. Fade timing and growth feathers are unchanged.
  GPU proofs check fractional opacity over real material and lines, transparent
  endpoints, interrupted navigation, and out-of-order regrouping samples.
  Isolated text over the same dark background and held endpoints had hidden this
  bug. The matched transition evidence is under `output/grid-heading-alpha/`.

- Growth-edge disclosure (former experiment C) was selected as the default.
  The first synchronized left-to-right feather was rejected: labels must stagger
  as the grid edge reaches them. The selected aperture samples extents/slice
  bounds and follows X, −Y, or projected −Z, with an 8-pixel ink-only feather and
  no competing growth fade. Heading visibility retains its 220 ms spring;
  cell ink now follows physical depth occlusion rather than target-slice opacity.
  No delayed callbacks or Gaussian blur. Losing motion treatments and
  the comparison-deck generator have been removed; see `scenes/keyed-grid/README.md`.

- The first, incomplete grid line-width fix was reproduced on the real GPU:
  the same shared edge widened from 1.707 px straight-on to 2.197 px at the
  deck angle and 2.394 px at a steeper view. Replacing the shader's Manhattan
  derivative norm with Euclidean distance keeps that proof at 1.691–1.707 px
  across the tested rotations/zooms, with geometry and color unchanged. This
  measures integrated linear-light coverage, not a thresholded bounding box.
  C / Shift+C now auditions five preview-only border colors without changing
  motion, text, or fills. The palette participates in worker cache identity and
  stale-frame rejection. Evidence lives under `output/grid-style/`.

- Further user review exposed the missing case: an outer silhouette measured
  about 0.85 px while the same edge as a two-face crease measured 1.70 px. The
  shared-edge-only test had missed that distinction. Per-face borders are now
  replaced by centered strokes with nearest-depth selection and max-coverage
  union. The silhouette/crease proof measures 1.695–1.701 px, while the existing
  shared-edge, opacity/occlusion, centering, and interruption proofs remain.
  Stroke coverage uses real MSAA sample coordinates; per-cell depth priority
  must not select between coincident strokes with different opacities.
  Orange was preferred after the palette comparison and remains the default.
  Current evidence is under `output/grid-edges/`.

- Fading target-slice cell labels ahead of the cutaway left an empty opaque
  outgoing face occluding the selected board. At 200 ms into the transition,
  measured cell ink fell to 7.9% of the held slice. Every tuple now keeps its
  own ink; physical clipping and depth testing determine what is visible.
  The cutaway proof checks intermediate authored samples and rapid native
  reversals, not just matching endpoints. Heading disclosure stays separate.

- The `keyed-grid` proof now uses an opaque, connected orthographic grid rather
  than detached colored blocks or an X-ray wireframe. The example illustrates
  chess pieces × sides × boards, with primary symbols, short secondary labels,
  and outside row/column/depth headings. Sampled visible cell bounds stay centered
  at the canvas center during fractional growth, rotation, and slice isolation;
  a GPU test measures the rendered silhouette within 0.75 pixels on both axes.
  Coplanar fronts exposed depth fighting during rapid regrouping. A tiny stable
  catalog-order depth bias resolves their priority without moving their geometry;
  the original failing pixel-neighborhood regression then passed unchanged.
  Catalog/label validation, connected boundaries, group membership, equal-time
  coalescing, settled holds, position/velocity continuity, exact boundary pixels,
  sampling order, and reduced-motion holds have focused checks. Command+Left/Right
  switches slides while unmodified arrows retain step navigation.
  Current images and targeted shutter-sampled clips are under `output/grid-chess/`.
  Earlier solid-block (`output/keyed-grid/`) and wireframe (`output/connected-grid/`)
  videos are historical, superseded design evidence, not the current appearance.
  Native screenshot/accessibility automation remains unverified; offscreen
  frames and the native submission benchmark are the automated pixel/performance
  evidence, not a claim of verified display scanout or normal-speed human review.

- The **initial solid-block** grid-deck run submitted 60 frames in each of nine measured seconds
  after a one-second warmup, at 1920×1080 / a 2560×1440 window / 60 Hz FIFO.
  Round render medians were 4.73–5.19 ms; median-round p95 submission interval
  was 19.83 ms, so cadence still varies. This is a historical workload baseline, not
  a measured improvement to Effect Tasks. Readback and native texture upload
  still occur; zero-copy composition, 120 Hz scanout, and imported meshes remain
  unproved.

- Rolling captions now use a fixed canvas-space aperture with linear edge fades, following Visual Types' `CyclingSection`/`FadeOverlays`. Row-integrated mask coverage preserves fractional edges and only affects text alpha, not the scene behind it. GPU checks cover unchanged held text, native/video parity, clipping throughout rapid reversals and skipped steps, out-of-order sampling, and reduced motion. Caption and Task timing profiles are unchanged by this mask.

- Expressive content needs independent property timings, not only a pose separate from its container. The first content pass slowed the body to 0.34 seconds and drove all content through one 0.32-second spring plus extra visibility windows; Kit judged this stale against Effect Institute. The correction restores the source Pixi body's 0.2-second/bounce-0.5 response, roughly 0.167-second icons, and a 0.25-second/bounce-0.4 result pop with independent 0.15-second deblur. Bubble rise/fade/deblur likewise have separate source profiles, without a staged wait. Native and authored entry poses, reversal continuity, unchanged sibling pixels, reduced-motion destinations, and actual channel samples against the source's pinned Motion generator are tested. See the timing table in `PRIOR_ART.md`.

- Compiled springs now use deterministic remaining-motion bounds. Once settled, an unchanged channel cannot wake when another channel resumes. Tests cover the original underdamped and tiny critical-move failures without frame-history-dependent state.
- Editor text, bright regions, and transformed Task content preserve fractional coordinates through premultiplied sampling and continuous blur. Source masks include the full filtered footprint. Regression tests cover reveal edges, alpha conservation, spotlight registration, transformed-parent alignment, and viewport boundaries.
- Semantic attachments follow sampled inline widths and keyed line positions. Private companion tracks retain position/velocity across target/literal switches; coordinate-normalized thresholds also apply to native set-only fallback motion. Moving highlights use a lightweight WGSL overlay pass when their bounds are safely inside the card; unsupported overlap falls back to full clipping.
- Timed editor snapshots support insertion, removal, reordering, and re-entry. Equal-time snapshots coalesce before layout. `plan steps` reports changed parts, common-text warnings, and partial/unsettled holds. `plan validate` runs editor and Task preflight checks before GPU preparation.
- A four-slide Presentation Deck combines code reveals, Effect Task lifecycle/retry, parallel Tasks, and keyed code edits. Slides retain their step and clock. Running Tasks keep animating until paused or left; their native ambient signal has bounded sampling cost after hours, rather than replaying an ever-growing history. The blocks reuse the Rust Pixi adaptation, not a browser or actual Effect execution.
- Native pacing uses fixed deadlines rather than accumulating late OS wakes. Static editor chrome has a two-entry cache, and initial slide resources warm before the window opens. `perf/native-playback.md` records measured pacing and remaining variability; the 60 Hz displays still cannot verify 120 Hz output.

## Subsequent Findings

The first four bullets below retain the **pre-fix review evidence**. Their unresolved-gap wording is superseded by the implementation above; the original diagnostic artifacts remain useful before/after references.

- The shared spring threshold currently has a settling-invariant gap: `Segment::sample` snaps independently whenever position and velocity are inside their thresholds, even if the analytic trajectory leaves that region later. A valid response-0.5/damping-0.8 profile reports exact rest at local time 0.410; after an unrelated channel restarts the presentation clock, the unchanged channel samples position 1.013488 and velocity -0.0659364 at 0.460. A tiny 0 → 0.0009 move also reproduces non-permanent settling with the default critical profile. The fix must make settling permanent after a deterministic time without introducing frame-history-dependent state. This was not reproduced in the current binary, common-profile slide choreography: a 2,000-command stress probe accepted 1,642 navigations with zero measured command-boundary position/velocity jumps. Reproduction and logs are under `target/playback-stability-review/` and `output/stability-review/motion-probe.log`; this review did not change the engine.
- Maximum Stability is not guaranteed by smooth scalar tracks. A review probe using the current compositor functions found that a 0.02-pixel position change across a rounding boundary moves a sprite by one full pixel, a 0.01-pixel reveal-width change can expose one fully opaque column, and a blur-radius change from 0.49 to 0.51 switches the rounded 3×3 kernel abruptly. These are single-sample pixel discontinuities, not frame-pacing failures; final GPU window filtering cannot reconstruct discarded fractional coordinates. The review-only probe is under `target/stability-review/`; the renderer has not yet been changed to fix these findings.
- Semantic attachment still uses static expanded geometry in the planned editor. A review fixture targeting the stable `equals` part of `effect-succeed` renders its highlight at the final expanded x-coordinate even in the initial step, where the type annotation is hidden. At the expanded step it lines up again. `output/stability-review/equals-initial.png` and `equals-expanded.png` demonstrate the mismatch. The native demo does not currently show that highlight, so this is a preparation/layout gap exposed by the fixture, not a claim that its existing code-only steps display an incorrect pointer.
- The native demo's seven destinations are independent Inline Reveal channel values inside one fixed set of lines. `CodeTransition` matches authored line IDs across a single initial/final snapshot pair; it does not infer correspondence between arbitrary source strings or support a general sequence of line-order snapshots through the current editor recipe. Psychopomp also lacks the source project's changed-slot Delta inspector and common-text stability warnings. The project-wide acceptance rules now live in `AGENTS.md`, with the domain contract in `CONTEXT.md`.
- A faster renderer cannot overcome the connected display's refresh limit. Both Studio Displays report 60 Hz; raising native sampling from 60 to 120 fps still produced exactly 60 submissions in every measured second and increased time waiting for a drawable. GPU-completion diagnostics measured scene sampling plus completed upload/draw work at 4.61–5.12 ms median-round p95, within an 8.33 ms budget for this flat-editor scene. This is headroom measured on a 60 Hz display, not proof of sustained 120 Hz playback. The fixed software cap is now replaced by monitor-aware pacing with an explicit override; actual high-refresh delivery still needs a 120+ Hz display for verification.
- Native interaction is not reverse movie playback. `Playback` derives step destinations from the renderer-resolved authored Timeline, then compiles new springs from current position and velocity. The Effect Institute `effect-succeed` adaptation demonstrates interrupted forward/backward inline reveals, pause/resume, and explicit replay; a GPU test requires byte-identical frames immediately before and after a mid-flight reversal and deterministic out-of-order sampling afterward.
- Rendering and displaying a live frame are separate performance costs. A cached flat-editor preview reduced warmed single-sample rendering from roughly 116–138 ms to 0.4–0.5 ms, but CPU scaling plus softbuffer still limited the native 2560×1440 window to 22–24 submissions per second. Replacing only final presentation with an sRGB wgpu texture, GPU filtering, and FIFO surface raised that to 59–61 submissions per second; two matched runs reduced median-round p95 intervals from 43.53–45.15 ms to 17.88–17.89 ms. These are submission measurements, not display scanout timestamps. `perf/native-playback.md` records the workload and quality-profile distinction. (2026-10-07: the 116–138 ms full-quality figure predates threaded card and text rasterisation and the flat-editor reuse; re-measure before quoting it.)
- The live preview deliberately skips the full compositor's last optical resampling of neutral editor glyphs, while retaining cached chrome and all code trajectories. It is not pixel-identical to full quality; camera/pointer/annotation combinations fall back to the ordinary renderer. A preexisting full-quality endpoint PNG remained byte-identical after extracting delivery and adding this opt-in preview path. Native higher-DPI glyph rasterization and state/media playback remain unproved.

- Rust computation and renderer iteration do not need to share one compilation unit. A lightweight Scene Program can emit a deterministic, validated Scene Plan while a separate `psychopomp-render` process retains Metal, font caches, and FFmpeg delivery. The `agent-demo` producer compiled independently, and a 200-millisecond global Render Window rendered in 0.7 seconds without weakening Rust authoring to a data-only language.
- Continuous and discrete animation are orthogonal core channels. Explicit-time set/spring events now use the same Timeline segment compiler as relative Rust Animation values, while generic `StateTrack<T>` sampling supplies previous value, age, and interval history to both authored Tasks and imported component snapshots. Task and title-card semantics remain adapters rather than core timeline variants.
- The editor-heavy hero can cross the complete process seam without flattening renderer measurements into authored pixels. Scene Plan v2 scalar values retain stable Semantic Target references; the concrete editor recipe shapes the canonical code, resolves logical range geometry, and only then compiles the shared numeric Timeline. The standalone `psychopomp-hero` Scene Program and the removed direct hero path produced identical 300-frame MP4 bytes: `d9d394018b8df895d093fe075e37ddf933d6c85f24b81010b4271ac35c189801`.
- OpenCode v2 can hot-reload a newly created plugin tool into the session that created it. In one isolated conversation driven by the local `opencode-drive` v0.5.0 checkout, the first provider request did not advertise `session_greeting`; the model used the built-in `patch` tool to create `.opencode/plugins/session-tool.ts`; the location watcher loaded it; and a second prompt in the same session advertised and executed the new tool. Both tool calls and successes share session ID `ses_094a7d8c0ffePHFPA7htnul0jc` in the retained server log. The deterministic fixture, raw recording, screenshots, and log live under `experiments/opencode-v2-session-tool/` and `assets/opencode-v2-session-tool/`.
- The earlier narration-rich `opencode-v2-session-tool` artifact proved that a Scene Plan can own authentic terminal footage and audio without a generic video graph. That plan scheduled one 12.28-second edited recording, five independently placed ElevenLabs script clips, two SFX, six continuous card-pose channels, discrete recording state, and five cues. Its verified output was 930 H.264 frames at 1920x1080/60 with a 14.907-second 44.1 kHz mono AAC mix and SHA-256 `c7d8612779d8f90c8bf6d5a5e47ca87e1eba6a6150bb587bb3c2d9f160a644e0`.
- OpenCode v2 commit `4a7f760d25` supports a broader honest live-reload proof in one running client and session. A real Vim recording edits the same Drive fixture while OpenCode reacts to inline commands, agents, project skills, references, providers/models, agent permissions, ambient `AGENTS.md`, and two local-plugin generations. The final generation replaces `STAGING` with `READY` and adds `deployment_url` beside `release_status`. Psychopomp aligns the two immutable recordings into a 42/58 split, uniformly accelerates the 48.1-second evidence interval to a 19.216667-second 1920x760/60 H.264 source, and presents it through the existing Terminal Recording recipe. The Scene Program adds nine contiguous cues, save/confirmation layers, and a critically damped six-channel card entrance over a 20.5-second delivery without claiming restart-bound MCP or external TUI configuration as live. The verified output fully decodes as 1,230 H.264 frames at 1920x1080/60 with 48 kHz mono AAC, and has SHA-256 `2dac4c24bc2e69158bc02ae14a424d5549a8fd1c02f03bbd89a0bdb8350e1d2d`.
- Uniform slow motion is the wrong mapping for product footage with asynchronous waits. Applying 6x slowdown to the complete raw OpenCode capture expanded an unchanged 1.96-second model wait into 11.76 seconds of static video and left 6.44 seconds of measured silence between narration clips. Preserving slow motion around visible events while omitting the unchanged wait reduced the lesson from 22 to 15.5 seconds without removing evidence. The Scene Program now rejects narration placements that leave more than one authored second between adjacent script clips.
- Editor pixels and recorded-video pixels can share one presentation Module without a retained scene graph or renderer trait. `render/ui/card.rs` accepts packed or strided borrowed RGBA, supports nested clips, fills, strokes, fit modes, card-local overlays, and call-order composition, then owns rounded clipping, material, border, shadow, perspective, surface blur, and depth-dependent near-edge blur. A direct borrowed-source path avoids a redundant full-card raster pass for simple RGBA producers while preserving the composable closure path for cards assembled from multiple operations.
- Opaque borrowed RGBA can bypass material alpha composition without changing rendered pixels. On the same settled one-second, 1920x1080/60, eight-sample range, the direct card path improved from 26.2 seconds to 18.6 seconds; decoded frame MD5s remained identical across all 60 frames.
- Prepared Scene Plans can avoid rerendering identical shutter samples without weakening temporal sampling. An exact key includes current and prior sampled motion state, discrete state values, and terminal source-frame identity; repeated keys render once and retain their multiplicity during linear-light accumulation. The settled one-second OpenCode range fell from a median 20.707 seconds to 3.519 seconds across five measured runs, a 5.88x improvement, while all 60 decoded frame MD5s remained identical. The opening motion range retained all 16 unique samples. The complete 15.5-second artifact fell from 418.9 to 257.1 seconds and retained the exact encoded SHA-256.
- Two card-pixel micro-optimizations were measured and discarded after shutter deduplication. An explicit precomputed source-fit mapping regressed the settled benchmark, while direct writes for opaque full-coverage pixels remained inside run-to-run noise. The simpler compositor stayed in place.
- A simulated UI does not require a public widget tree or frame-driven entity runtime. The `deployment-queue` proof uses one stable actor, typed semantic snapshots, recipe-local item IDs, private fixed-canvas flow layout, and private keyed position, presence, progress, and feedback tracks. Recipe-owned visual keys retain current and outgoing phases plus every internal moving scalar, so active shutter samples remain distinct while held dashboard states can settle to one reusable visual state.
- Artifact review is a semantic test, not merely an encoding check. The first deployment proof fully decoded but was rejected after frame-by-frame review: phase events enlarged every row discontinuously, a contrived interrupted reorder hid service identity, retry progress ran backward, aggregate health changed before row content, and layered blue gradients obscured hierarchy. Browser-controlled study of the animations.dev theory and walkthrough lessons led to a smaller motion vocabulary: stable service order, no-bounce product springs, monotonic progress, immediate-snapshot phase presentation, bounded 45-millisecond replacement staggering, event-boundary continuity, and solid graphite materials on a neutral field.
- The corrected deployment proof contains 540 H.264 frames at 1920x1080/60, rendered in 318.3 seconds, fully decodes, and has SHA-256 `460e0692ff7b8c79d8244259062834f03075854b8987fba784f80fd265eb8b60`. Exact checks at insertion, failure, retry, verification, and health boundaries retain the immediately preceding complete UI Snapshot; the full contact sheet and normal-speed playback preserve stable service identity and monotonic readiness.

- Shaped glyph cluster hitboxes provide exact semantic token targets without estimating monospace advances.
- Token highlights and an independently moving pointer can target those ranges through ordinary property tracks.
- The exact Effect Institute Phosphor hand can be loaded through a reusable SVG sprite pipeline, transformed per temporal sample, and accumulated with motion blur.
- A stable line can reveal `, NotFound` by expanding the inserted spans, resolving opacity and blur, and moving the existing `>` suffix without replacing it.
- A pure Rust DSL can preserve semantic text targets until scene compilation and lower typed actor operations into the same scalar property tracks.
- Cross-media sequence, parallel, delay, and hold can schedule visual motion together with non-destructive audio or video clips.
- Transcript-bearing script clips and accompanying layer clips require distinct roles even though they share the same composition clock.
- Integer-nanosecond media time avoids source-range drift that appears immediately when decimal clip boundaries are repeatedly subtracted as floating-point values.
- The complete 31.7-second Effect Institute `effect-shows-errors` lesson can use its original Opus narration and word timing sidecar to drive structural code changes, focus ranges, pointer motion, multiple inline reveals, an error squiggle, and a celebration burst.
- FFmpeg can trim and place compiled audio clips on the composition clock while continuing to receive raw rendered video through stdin.
- A separate success sound can be scheduled as a layer clip at the same transcript cue that clears the error and reveals `VeryBadRoll`.
- Semantic targets after a collapsing inline slot must resolve against the sampled slot layout, not the backing line containing every slot alternative; otherwise highlights inherit the hidden span's width.
- Porting an Effect Institute flow requires its operation-specific motion profiles: 0.5-second bouncy cursor travel, 0.4-second critical inline parts, 0.45-second line movement, 0.3-second spotlight changes, and a 0.4-second burst entrance.
- Short semantic annotations can be first-class composition leaves: scheduling resolves their target once, arbitrary-time sampling derives normalized phase without retained particle state, and a closed recipe enum swaps prismatic bloom for focus pulse without exposing renderer plugins or drawing parameters.
- Per-clip decibel gain lets a quiet sound effect remain an immutable source asset while cue-local parallel composition synchronizes it with a semantic annotation.
- Mix level alone does not predict whether a sound effect reads under narration: the original narrow 1 kHz success tap remained masked after an 18 dB boost, while a quieter upward stereo chime with high-frequency transients stays distinct from speech.
- The complete 31.1-second `promises-only-happy-path` lesson ports without a new renderer feature: two independent inline reveals on one stable call line model `???` being replaced by `throws SomeError`, while the original narration retimes focus, pointer motion, and the optional call line through semantic cues.
- A semantic target after a hidden alternative on the same stable line needs its measured x-position adjusted by that alternative's sampled width. This is the same unresolved layout seam exposed by collapsing slots in `effect-shows-errors`, now reproduced by a second scene.
- Semantic text targets must currently use unique text within their stable line: targeting `ship` selected the earlier substring in `shipment`, while `ship(payment)` resolves the intended call. Occurrence-aware targets remain a future DSL seam.
- Applying one sampled translation, three-axis rotation, and scale to both shader geometry and the CPU-composited foreground produces a coherent 0.7-second perspective entrance whose temporal blur comes from scene motion rather than a post-process blur.
- Extreme perspective entrances expose sampling quality quickly: a five-tap cross reads as repeated glyph copies, and eight shutter samples reveal ghost contours during a 150%-to-100% pullback. A depth-weighted 3x3 Gaussian kernel plus 16 entrance samples produces a smoother near-plane blur while settled frames retain the normal eight-sample cost.
- Published narration bytes can still produce a different browser mix when a lesson flow schedules synthesized sounds separately. The Promise lesson's narration asset is byte-identical to production; its descending E5-to-C5 Tone.js cue must be represented as a cue-local layer clip.
- Effect Task states can be authored as ordinary Composition leaves while stable IDs preserve nodes across idle, running, success, failure, death, retry, and hidden intervals. Arbitrary-time sampling resets entrance age after a hidden interval rather than depending on prior rendered frames.
- Porting Pixi task pixels requires one shared fractional transform for every moving layer. Integer-snapped or independently transformed body, sweep, border, flash, and glow edges visibly separate under subpixel jitter even when their high-level spring targets match.
- Motion's width and height springs are intentionally independent in the Task recipe: running height changes over 0.2 seconds, completed result width over 0.35 seconds, and running scale returns from 0.95 without a one-frame geometry jump.
- Task pose changes and semantic state changes need independent ages even before they become separate compiled tracks. Restating an unchanged success or idle state to recenter a row must move the stable Task without replaying its flash, pulse, sound-equivalent visual accent, or content entrance.
- Product motion guidance favors one dominant action per Task transition: compression and sweep for running, result resolution for success, a brief horizontal impact for failure, and loss of energy for death. Continuous running shake and multi-axis random failure noise made state meaning less clear despite adding more motion.
- Positive `asetpts` offsets do not place delayed layer audio reliably through FFmpeg's `amix`; explicit `adelay` placement is required. A band-limited comparison against narration confirmed that the old path silently mixed task sounds at the wrong time even though the output contained an AAC stream.
- Task entrance defocus reads coherently only when blur applies to the assembled node layer. Blurring the icon independently while leaving its body and label sharp separates one stable actor into unrelated optical planes; container blur plus temporal sampling keeps defocus and motion blur distinct.
- Task success audio works better as a compact confirmation than a musical reward: a quiet two-tone interval with a short 420 ms decay leaves narration space and matches the brief result-resolution motion better than the earlier sustained triad.
- Task layout is a real motion channel rather than repeated state authoring. Dedicated pose leaves compile into velocity-preserving x/y tracks, while completed widths are measured from the same `cosmic-text` recipe used to render results and feed one centered row calculation.
- Averaging encoded sRGB bytes darkens glow and motion-blurred edges. Decoding temporal samples through a lookup table, accumulating RGB in linear light, and encoding once per output frame preserves energy without requiring a new GPU target.
- The sequential lesson reads more causally with two subdued authored links and short traveling handoff pulses. Keeping these links scene-specific avoids implying that every Task row is a generic dataflow graph.
- OpenCode Drive can provide compact deterministic product footage for Psychopomp: the command-hot-reload fixture records a fixed 1200x720 viewport at 25 fps, creates `.opencode/commands/fire-the-missiles.md` while autocomplete remains open, and observes the new command 215 ms later through the real watcher and command-update path. Drive 0.5.0 preserves the loaded first frame and corrected box-drawing geometry, so the committed source needs no startup trim or glyph repair.
- Short terminal recordings do not require codec bindings or a generic media graph. Decoding once through FFmpeg into a seekable raw cache preserves arbitrary-time frame lookup, keeps the checked-in H.264 source tiny, and avoids retaining hundreds of megabytes of RGBA frames in memory.
- Product footage and motion-graphics framing have different responsibilities. Keeping the OpenCode pixels authentic while Psychopomp owns rounded presentation, whole-card camera movement, the split command-file editor, and the missile payoff makes the feature demonstrable without reconstructing the TUI.
- Product footage reads more clearly when a detail move transforms the complete terminal card rather than zooming pixels inside a stationary mask. Scale, position, rotation, rounded silhouette, and shadow must remain one material; explicit bright border strokes produce repeated contour lines under temporal sampling and are better omitted.
- The planned `terminal-recording` root was generalized into the `video` Video Card overlay. Its command-file and missile presentation only ever ran in the legacy scene, so the plan recipe's remaining value (framing one placement, card motion, source-time mapping) is exactly a Video Card plus ordinary text actors; the session-tool port changes pixels (theme material, a title bar, no radial backdrop) but keeps its entrance channels. The focus window zooms footage *inside* a still card, which the finding above warns can read worse than moving the whole card; both remain available (`focus-*` versus `x`/`y`/`scale`), and the window's corners move linearly so the focused region never swings outward on its way in.
- The command-file cause needs enough screen space to show time. A compact split-screen editor can reveal the actual checked-in `.opencode/commands/fire-the-missiles.md` fixture, transition from writing to saved, and remain beside the live autocomplete without inventing another terminal recording.
- Short generated narration can remain an ordinary script clip. The ElevenLabs line is mixed beside cue-local typing, save, launch, dual impact, and confirmation layers; no sound is renderer-triggered.
- Six concrete render targets made scene ownership a demonstrated seam rather than a hypothetical abstraction. Moving each complete choreography behind `crates/psychopomp-render/src/scenes/<name>.rs::render` reduces `crates/psychopomp-render/src/main.rs` to command dispatch while keeping documents, assets, targets, and sampling local; no scene trait or registry is needed.
- Repeated transcript choreography justified `Cue::at`: one cue can place Motion, Task changes, annotations, media, or nested composition at its exact start without every scene reconstructing a delay wrapper.
- Repeated structural edits justified a `CodeEdit` actor that coordinates layout/content initialization, enter/exit Motion, progress, and `CodeTransition` sampling. The hero remains an important counterexample: its layout opens before content arrives, so direct independent tracks remain part of the authoring vocabulary rather than being forced through the coordinated helper.
- The complete 30.366-second `effect-is-a-description` lesson combines three structural Code Edits and stable inline alternatives with the reusable `getTime` Task on top of editor pixels. The original narration cues drive type revelation, explicit execution, timestamp success, function equivalence, and the final `Effects are LAZY` return without a lesson-specific visual actor.
- Effect Institute maximum stability must be preserved during a port, not reconstructed from rendered snapshots. In `effect-is-a-description`, `const getTime`, ` = `, `getTime`, `)`, and the comment suffix remain stable spans while only type, implementation, call-prefix, opening-parenthesis, and comment-subject alternatives collapse or expand horizontally. Separate line IDs for each visible state made the whole definition leave and re-enter even though the screenshots looked semantically equivalent.
- Published line and variable-part motion are separate contracts. Effect Institute lines move only on `y` with a 0.45-second zero-bounce spring while opacity loss derives a 4-pixel exit blur; inline variables use a 0.4-second zero-bounce width/opacity/blur spring. Reusing a generic 0.48-second structural spring and horizontal line offset made both transitions feel unrelated to the source lesson.
- Running Tasks need one coherent charging transform rather than independent decoration. Deterministic irregular target holds and analytic damping move the body, glow, border, and sweep together, allowing temporal sampling to create motion blur without softening the static surface or label. Glow and border strength follow this unpredictable charge energy rather than independent sine waves. The energy sweep is an infinite 122-pixel train moving at 500 pixels per second, so pulses remain exactly 0.244 seconds apart instead of acquiring a larger gap when a finite three-band group wraps.
- Task content transitions need both the previous and current state in the same sampled frame. `TaskFrame` now carries the previous state's completed duration so running jitter and sweep phase settle continuously into the next state. Outgoing results fade and blur beneath incoming icons while an analytic rounded-body mask clips final destination coverage after text blur; the text keeps its natural size instead of being scaled to the springing container. Incoming icons rotate and deblur from 0.7 scale, with reset staggered 100 milliseconds so the sparkle lands on the second descending reset note. Task SFX remain cue-local composition media; `effect-is-a-description` schedules running, success, and the source-inspired reset cue beside the corresponding semantic state changes.
- Effect Institute's compiled production artifacts preserve the source DSL's useful identity seam: stable line IDs, ordered part IDs, alternate token versions, exact step times, annotations, and component snapshots. Pinning those artifacts lets Psychopomp port a 30-section published corpus without re-authoring its DSL or flattening maximum-stability slots into replacement lines.
- Chapter composition is editorial scheduling, not one giant Scene. Intro and Basics retain section-local time and namespaced actors while one chapter encoder places canonical narration in manifest order around title and gap intervals. The selected non-onboarding Intro corpus is 14 sections and 450.381 seconds; Basics is 16 sections and 676.634 seconds.
- Compiled frame annotations are targets, not poses. Imported cursors use the published 0.5-second spring and enter from 40 pixels right and 30 pixels below; cursor, highlight, focus, and long-section camera tracks preserve velocity when cues retarget before settling. Highlights expand from their target center and soften through opacity-coupled edge blur. Long code sections follow active semantic targets within a clipped editor viewport rather than drawing rows outside the panel.
- Component snapshots need stable hidden actors and previous payloads. Keeping alternate sync/async and expanded/collapsed Task IDs mounted prevents row jumps, and measuring result bodies avoids clipping timestamps and object values. Reconstructing the previous snapshot's payload, rather than applying the current payload to its state name, keeps overlapping Task content transitions semantically correct.
- Published sound names remain section-local composition layers rather than renderer side effects. The 20 Intro and 7 Basics cues are scheduled at their compiled step times and mapped onto Psychopomp's concrete sad, failure, success, confirmation, whoosh, alarm, and death recipes; clips are trimmed at section boundaries so they cannot alter chapter timing. Published emoji bursts retain semantic polarity through separate celebratory, danger, and sad effects rather than turning every cue into a positive bloom.
- Small pixel interfaces need composable layout before they need more chrome. Rebuilding the command-file explanation from GPUI-inspired `Bounds`, edge insets, splits, and vertical flow reduced it to one compact header, gutter, and four evenly spaced rows; the earlier explorer, breadcrumb, tab, and mode bar amplified coordinate drift without improving the explanation.

- A narrated code explainer is delivery of several scenes, not one scene with many roots. The `pr-walkthrough` reel joins twelve Scene Plans (intro, five behavior stories, five diffs, outro) on one clock; each keeps local time and namespaced actors. A crossfade between two text-dense frames was unreadable at its midpoint, so the reel uses dips through the empty background. The 44-second errors story rendered with audio in 93 seconds at 60 fps with eight shutter samples (about 2x realtime), because static holds collapse to one unique sample.
- Phrase-keyed choreography survives re-voicing only if anchors avoid words speech recognition reformats. Whisper wrote "fifteen" as "15", "TUI" as "2e", and "SIGKILL" as "a kill" for the draft voice; number words now normalize to digits, and anchors avoid acronyms. A missing phrase fails the Scene Program with its clip and phrase instead of silently mistiming.
- Replaying the fixed behavior in the same Sequence Diagram slots reads as a direct comparison: shared rows stay, broken-only rows fade, and fixed rows arrive where the broken ones were. Strikes reset at the switch so a dropped message reappears intact.

- Explainer quality came from motion craft, not more diagram types. Remaking #50825 on the GPU Stage (particle orb, floating cards, flowing beams, packets with comet trails, perspective camera with depth of field, HDR bloom, rewind, and a zoom-through into the code) replaced a flat sequence diagram with a readable physical story: the SIGTERM visibly shatters the service and snaps every connection, and the fix replays the same moment intact. The 62-second film rendered in 547 seconds with every stage sample distinct (spin, flow, and grain always move).
- Stage tuning notes: mixing accent into a near-black backdrop in linear light reads brown, so the backdrop is neutral and warmth comes only from bloom of lit objects; a dashed glow must be measured along the path too or it renders as bars across the line; snapped beams must recoil fully or their stubs meet as an asterisk; per-frame grain costs x264 about 24 Mbit/s at CRF 17, so the encoder now caps the rate at 14 Mbit/s (a 6 Mbit/s share copy of the flagship is visually equivalent at 39 MB).

- Kinetic explainers need every light to have a source. Porting the opencode-architecture diagrams' vocabulary (gather, cubic-in-out flight, cooling trail, dot-to-ring landing, border reflections that follow the packet, floods from the socket, embers at the port, frame sweep, port pop, bead draw, settle-in) made each beat read as energy moving from one place to another. Whole-card flashes and scaling receivers read as noise by comparison.
- Stepped `set` approximations of an ease (24 steps) judder visibly at 60 fps even with motion blur; the `ease` operation fixed packets and wire draws. A cable twang must build over about 60 ms (a jump pops) and should sag downward on every beam.

## Procedural Stage Burst Study

- A bounded raymarched volume makes the service destruction legible as fire
  cooling into smoke, rather than an expanding particle shell. Domain-warped
  density, spatial temperature contrast, and absorption preserve dark folds;
  uniformly emissive density washed out under HDR rolloff in the first study.
- A pressure wave reads more physically when it displaces the actual scene.
  Reducing its emitted ring light kept attention on combustion. The 120 ms
  compression, volume, and analytic gravity/drag embers share a reversible age.
- Rendered `output/burst-impact.mp4`, `burst-entrance.mp4`, and `burst-rewind.mp4`
  at 1080p60. Inspected exact full-scale frames, encoded impact/rewind strips at
  40 ms, and the entrance strip at 100 ms. GPU out-of-order sampling reproduces
  the same fire pixels after sampling smoke; this is not a real-time performance
  measurement or a fluid simulation validation.
- Label grouping exposed a serialization bug: Stage omitted centered alignment
  but decoded missing alignment as left. A three-alignment round-trip regression
  now protects it, and `output/burst-established.png` verifies the centered
  service label and connections disappearing beneath the shell at full scale.
- Perimeter sweeps are now opt-in channels rather than the default `connect`
  choreography. Soft port reveals preserve the stagger without racing highlights.
- A bounded follow-up quality loop (`perf/stage-burst-quality.md`) measured three
  concrete defects. Keeping shell material through compression reduced entry
  pixel MAE from 9.35/255 to zero. Combustion now lifts the facing card rim by
  6.17 RGB8 red levels in the fixed fixture while the far rim stays unchanged.
  Compact density support removed straight-cut fire lobes; edge-step p99 in
  the affected region fell from 33 to 17. These are defect-specific measurements,
  not an overall art score; each version used seven deterministic measured pairs.

## Next Question

- Follow-up contact/code studies made the packet's visible-shell intersection
  the source of an emissive surface wave and local dimple. The request no longer
  relies on an extra center pulse. Camera channels in the flagship now use
  critically damped springs; minimum-jerk packet travel is unchanged.
- Diff row seams were doubled antialias coverage, not authored separators.
  Weighted interval union reduced the blank-gutter green variation from 6 RGB8
  levels to zero at the final code hold. Unit tests cover fractional adjacency
  and overlapping marks with different opacity.
- The first version-check edit now retains its declaration and expression parts
  while their lines trade places. The diagnostic copy of the video code scene
  uses its actual snapshot times as presentation steps; `plan steps` reports no
  common-text or unsettled-hold warnings at two-second holds. This checks authored
  identity and destinations, not a claim of interactive navigation QA.

- The first kinetic Stage pass overused light: a whole-card halo, dense luminous
  orb points, and perpetual flow beads competed with the narrated packet. The
  refinement keeps substrates dark, reserves light for local events, and stops
  flow after the connection beat. Springs now express small panel settling and
  cable recoil; minimum-jerk quintics carry packets and camera moves.
- A pulse used to instantly enlarge the orb by 8%, displacing all attached ports.
  The renderer now keeps pulse response in illumination; a geometry regression
  test covers it. Camera composition tests cover the formerly clipped clients.

Can native interactive playback retain its responsiveness and interruption semantics for more complex editor, UI Surface, and media-backed scenes without forking the authored visual model?
