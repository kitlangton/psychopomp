# Psychopomp Domain Language

## Code Document

The complete set of code lines that may appear in a scene. Every line has a stable line ID and styled spans.

## Code Snapshot

An ordered list of stable line IDs describing one meaningful state of a code document. A snapshot contains state, not motion.

## Stable Line

A code line whose identity survives between snapshots. Its screen position may move when surrounding lines enter or leave, but its text object is not replaced.

## Maximum Stability

The authoring and rendering contract that preserves common code identity across step destinations. Only changed content enters, exits, or is replaced. Retained lines and inline parts may move to accommodate changed layout, but do not unnecessarily disappear, reappear, fade, or blur. Stability concerns the smallest meaningful content delta, not keeping every screen coordinate fixed.

## Code Transition

The compiled relationship between two code snapshots. Sampling a code transition places every stable, entering, and exiting line at an arbitrary progress value.

## Motion State

The position and velocity of one animated scalar at a specific time. Carrying both values allows a later trajectory to preserve momentum.

## Scene Program

A lightweight Rust executable that may perform arbitrary calculations, imports, data loading, and control flow before emitting one Scene Plan. A Scene Program is durable authoring source; its emitted plan is compiled output.

## Score

The authoring surface (`psychopomp::score`) that schedules composable **Beats** onto a `PlanBuilder`. `PlanBuilder` is the sole mutable target; actor handles (`Stage`, `Camera`, `Caption`, `Callout`, `RollingNumber`, `Tree`, `Plot`, `Lanes`, `Sequence`, `Video`, `Terminal`, `Chat`, `ChangedFiles`, `LowerThird`, `Checklist`, `Meter`, `Bars`, `Subtitles`, `Confetti`, `Text`, `Image`, `Lens`, `Diagnostic`, `Hover`, `Cursor`) are cloneable values whose methods take `&self` and return Beats.

## Beat

A composable unit of choreography with denotation `Time (start_nanos) → (Span, Writes)`. Beats combine via `.then` / `chain!` (sequential), `.also` / `all!` / `at!` (parallel), `.with` (accompaniment at start), `.on_end` (reaction at completion), `.after` / `.early` (time offsets), and `stagger` / `each`, lowering directly to ordinary `PlanBuilder` events without changing the Scene Plan format.

## Span

The choreographic interval `[start, end]` in integer nanoseconds occupied by a Beat's primary action (for example when a panel is ready to wire, when a beam makes contact, or when a packet arrives), distinct from how long its physical springs or trails continue settling afterward. A `Span` implements `CueTime` (`.after`, `.early`, `.not_before`, `.reply`) so later beats can chain from or guard against it.

## Scene Plan

A versioned, renderer-independent value containing stable actor declarations, continuous channels, state channels, exact cues, and media placements. Agents may inspect, validate, diff, and render a Scene Plan without recompiling or restarting the renderer.

## Scalar Plan

One compiled scalar source in a Scene Plan. It is either a finite literal or a reference to one component of a stable Semantic Target plus an offset. Renderer preparation resolves target references before the ordinary numeric Property Track is compiled.

## Continuous Channel

One named scalar property of a stable actor. Ordered set, spring, and ease events compile into a deterministic Property Track while preserving equal-time source order. An ease moves from the current value to a target along a named curve over an exact duration, carrying the curve's velocity so a later spring continues without a jump.

## State Channel

One named discrete property of a stable actor. Its compiled State Track retains the previous and current value, completed previous duration, transition time, and current age without depending on frame history.

## Keyed Grid

A finite product of three immutable ordered value axes. Each tuple has stable recipe-local identity, independent of its visibility or arrangement. A row or table can show a slice of that catalog; revealing another axis value adds visible tuples without replacing retained ones.

## Grid Snapshot

The visible prefix of each axis, an Arrangement, and optional focused depth slice. Straight-on and angled orthographic arrangements view the same connected 3D geometry; a flat-looking grid can still contain depth. Reassociated arrangements regroup the tuples. Layout supplies position, extent, and cutaway targets; ordinary Property Tracks supply motion. Extents reveal a connected grid without scaling individual cells; the projection deliberately centers its currently sampled visible bounds during growth. A focused slice clips away other layers without deleting their identities. Reassociation changes `((a, b), c)` into `(a, (b, c))` without adding or losing tuples. Growing a product is not an isomorphism between the smaller and larger sets.

A cell's primary symbol and secondary label are representations of its tuple,
not its identity. Row, column, and depth headings describe the values along the
edges and remain separate from the cell labels.
Their growth-edge disclosure follows the sampled extent along each catalog axis,
so supporting labels appear as the grid reaches them rather than on independent
timers. Semantic visibility (for example regrouping) remains a Property Track.

`GridStylePlan` controls presentation without changing tuple identity: checkerboard,
background-matching, uniform, or row-banded material; full, row-only, or no rules;
and optional table layout. Background-matching material remains opaque to rear
faces and labels. `GridTableLayout` supplies unequal column widths, row height,
padding, alignment, and display headings independent of catalog names. This
one-layer view fixes its placement against the complete catalog rather than
recentering visible prefixes, so headers and retained rows stay still during
growth. Table headings belong to the table plane; the cube's outside headings
remain upright. A conventional keyed-record update/sort model is not yet provided.

## Value Token

A stable actor depicting one value or one explicitly labeled case shape in a
finite teaching diagram. Its immutable label and optional detail describe that
role; equal text in different roles does not imply shared identity. Position,
presence, and border emphasis are ordinary Continuous Channels. A group of Value
Tokens does not imply automatic enumeration or a generic mathematical set API.

## Renderer Recipe

A concrete rendering adapter selected by an actor declaration. Recipe payloads and pixels remain renderer-owned; the Scene Plan core validates identity and timing without understanding their visual implementation.

## Render Window

A positive exact range on the global scene clock selected for delivery. Output begins at time zero, intersecting media is trimmed and rebased, and visual sampling retains global time so ongoing trajectories do not restart.

## Presentation Step

An explicitly ordered destination in a Scene Plan, with a stable ID, title, entry start, and held endpoint on the authored scene clock. Entry start may equal the held endpoint for a still step. Unlike a Cue, a Presentation Step is a navigation destination, and step entry ranges cannot overlap. The native player derives scalar target poses at held endpoints, then animates between those destinations on its own pausable clock. Entry start supplies the explicit Replay pose. Authors choose meaningful endpoint poses; live waiting never alters the automatic video schedule.

## Presentation Deck

An ordered collection of titled Scene Plans. Each slide owns its Presentation Steps and local Playback clock. Changing slides preserves the selected step and pauses the departed slide; returning resumes only motion that was running when it was left. Slide navigation is separate from step navigation.

## Reel

An ordered sequence of independently authored Scene Plans delivered as one video
on a single clock. Each segment keeps its own actors and local time, so a segment's
choreography never depends on its position in the reel. A transition overlaps a
segment with its predecessor: a crossfade mixes the incoming frame over the
outgoing one, while a dip fades the outgoing segment to the empty background before
the incoming one appears, so dense frames never overlap. A zoom flies into a focus
rectangle of the outgoing frame, such as a card, while the incoming segment grows
out of it, so a detail visibly becomes the next scene. A **Wipe** sweeps a divider
across the frame with the incoming segment behind it; its holds rest the divider
mid-frame so a before/after comparison shows both segments side by side, each on
its own running clock, before the sweep goes on. At most two segments are
visible at any instant. Segment media is retimed onto the reel clock for one audio
mix. Unlike a Presentation Deck, a reel is delivered rather than navigated.

A **Composited Transition** is one the renderer draws from both frames at once,
posed by a pure function of the transition's progress: a push, slide, or whip
moves both frames along a direction, smeared by their speed; an iris or ink
reveals the incoming frame through a growing circle or blot; a **Match** flies
one camera so a rectangle of the outgoing frame lands on a rectangle of the
incoming one, and the element visibly becomes its counterpart (a **Round Match**
carries the ellipse inside each rectangle rather than a card, so a dot opens as a
circle onto the ball it becomes); a flip or cube
turns the frames in perspective; a glitch, flash, or light leak hides a cut
under corruption or light. A **J-cut** and an **L-cut** overlap two segments'
sound but cut their pictures at the end or the start of the overlap.

## Playback

Interactive navigation among Presentation Steps. Next and Previous retarget continuous channels from their sampled position and velocity through the same Property Track compiler used for video. Unchanged channel destinations retain their trajectories. Pause freezes the local clock without losing motion state; Replay deliberately restarts from an entry pose. Playback retains an immutable compiled timeline for each navigation revision, so late rendered frames cannot change the current destination. It is not reverse playback of a movie.

An explicitly opted-in Start Delay may stagger a property only from a specified
resting pose toward a specified destination. Changing that destination cancels
its unstarted writes; unchanged destinations keep their due times. Once moving,
redirection is immediate and preserves position/velocity. Pausing freezes both
motion and pending starts on the same local clock; reduced motion cancels waits.
Header word entrances demonstrate this contract without callbacks or a second clock.

Playback Speed scales wall-clock elapsed time into local scene time without
changing trajectories, scene-time velocity, or scheduled starts. Diagnostic
frame-stepping explicitly samples that same Timeline backward/forward while
paused, bounded below by the latest navigation time. It is not Previous navigation
and does not reconstruct or reverse earlier destination decisions. Export timing
and sampling FPS remain independent of these native inspection controls.

## Presentation Theme

A named paint palette, independent of Scene Plan identity, typography measurement,
and motion. Original, Evergreen, Tokyo Night, Pure Black, and OpenCode (the OpenCode
TUI's dark tokens) can be selected during
native playback without advancing its clock. The native preference is saved;
file delivery selects a theme explicitly so a personal preference cannot silently
change an export. Original preserves existing scene colors. Semantic status colors
and explicit non-palette art colors are not indiscriminately tinted. A **Theme File**
is a JSON Presentation Theme: a palette, status inks, a card-shadow scale, and an
optional font that replaces CommitMono for the whole run.

## Rich Text

Immutable Markdown in a bounded overlay, shaped as proportional rich runs with
monospaced code. Paragraphs, headings, list items, and quotations retain their
measured block placement while ordinary actor/block opacity and position channels
animate them. This is not automatic identity matching between edited Markdown
documents. A width-revealing expression instead uses authored stable inline parts.

## Venn Diagram

Two stable, explicitly sized set boundaries with independently sampled position,
radius, and roundness. Hatching represents the intersection of their current
geometry, not a delayed overlay or a relationship guessed from label spelling.
The recipe is a bounded overlay, not a type checker or arbitrary set-layout engine.

## Sequence Diagram

Participants with dashed lifelines and time-ordered rows: messages between
lifelines (or a loop to the same one), notes spanning lifelines, and End marks that
stop a participant. Rows are recipe-local identities revealed by ordinary
Continuous Channels (`row.<id>.reveal`, `.opacity`, `.strike`); a revealed message
travels as a packet before its arrowhead and label land. Rows default to a slot per
row, but several rows may share a slot, so a scene can play the broken behavior and
then replay the fixed behavior in the same places by fading one set out. The recipe
depicts a protocol; it does not simulate one.

## Stage

A 2.5D motion-graphics surface for explainers, rendered on the GPU. Elements sit at
world positions seen through a perspective camera: floating cards with status
lines, a particle orb that spins, breathes, and can shatter, curved light beams (connectors that attach to the side of a card facing the other
end and leave it head-on, and enter an orb radially)
that draw, flow, and snap, packets whose light gathers at a port, travels with a
cooling trail, and lands as a small ring, labels, and rings for timers and
shockwaves. Light is local: a packet or a drawing beam lights only the borders it
nears, and an arrival floods in from its socket. World x/y are canvas
pixels at depth zero, so the default camera is pixel exact; depth gives parallax,
depth of field, and draw order. Bright color blooms; the frame gets highlight
rolloff, vignette, chromatic pulses, and grain. Ambient motion (spin, flow, grain)
is a pure function of time, so any frame renders identically in any order.
An orb pulse is an illumination response, independent of its scale and attached
ports. A card's content can settle after its body; its `content` channel controls
the ink's presence, small vertical offset, and sharpening together.
A card's **Status Swap** cross-fades its status line straight from one entry
to another; a fractional `status` instead passes through every entry between.
Every Stage channel has one **Channel Default**, its resting value, which both
authoring and rendering read when nothing writes it: an element is visible and
whole at rest.
A label may be set in a **Face** other than bundled CommitMono: a display serif
for quiet titles, a light sans, or a condensed black for shouting. These are
faces macOS installs, so another machine substitutes its own.
An orb's **Burst** is a reversible destruction clock: gravitational collapse,
hot combustion, an expanding refractive pressure wave, cooling smoke, and
ballistic embers. Its procedural volume and trajectories need no simulation
history. Wires and arrivals pass beneath the intact orb's occluding shell.
A **Bolt** is lightning between two elements or points, run by one **Discharge**
clock: a stepped leader, then return strokes that strobe a few frames apart,
each re-rolling the path's detail around a persisting channel, flashing both
contacts, and throwing sparks, then a cooling afterglow. A bolt can also
**hum**, a sustained arc. **Charge** is crackle crawling an outline; it lights
the rim it crawls. A **Shield** is a forcefield bubble around an element that
ripples where packets cross it or bolts strike it. A card's **Dissolve** is a
reversible burn clock: a noisy front with a hot rim and ash; played backwards it
materializes the card. A **Scan** sweeps a line down a card.

## Stage Camera

The Stage's viewpoint at one Temporal Sample. Its **pose** pans (`camera.x|y`),
dollies along its view axis (`camera.z`), swings around a **pivot** on that
axis (`camera.yaw|pitch`, about world depth `camera.pivot`), magnifies
(`camera.zoom`, a multiple of the focal length), and rolls the delivered image
(`camera.roll`). At the default pose the projection is the original
translation-only one, pixel for pixel. Cards, labels, and rings are
**billboards**: their centers move in 3D but they keep facing the lens, so text
stays legible from any angle. Orb particles, embers, surface rings, and wires
are projected point by point, so an orbit shows their true depth. Draw order and
depth of field follow view depth. A **Follow** blends the authored pan toward
the pan that centers a packet or element, by `camera.track.<id>` weights the
renderer resolves at every sample, so a followed packet stays exactly centered.
**Handheld** sway and a jolt's kick and rumble add on top of the authored and
followed pose; neither fights a shot. A **Shot** is one `CameraRig` move (frame,
push in, drift, whip, orbit, dolly zoom, focus pull, follow) written as ordinary
channels from the pose authored so far; a **Dolly Zoom** trades zoom against
dolly so its subject holds still while the depth around it stretches.

## Particle Form

The orb's material, glowing points over a dark occluding body, on any of a few
deterministic shapes: a sphere, a box (a cube with emphasized edges, or a flat
slab), a dot-matrix plane, a lattice, a cylinder, a torus, a double helix with
rungs, or a `(p, q)` torus knot. A form turns in 3D
(ambient spin, an authored `rotation`, and `pitch` and `roll` for tumbling),
pulses, shatters, and bursts like an orb. Wires attach to its sampled, turned
silhouette (a convex hull), not a fixed circle. The orb's surface ripple is
spherical and stays the orb's alone. A form is solid at rest, its dark body
hiding what passes behind it; a **Hollow** form (`solid` 0), such as a ring of
dots, hides nothing.

A **Morph** carries every point of a form from one of its shapes to the next.
Point identity is stable: point `i` of every shape is the same particle, and each
shape's points are paired with the previous shape's so each point travels a short
way. The `morph` channel is a fractional index into the shapes; each point leaves
at a seed-staggered moment, bows slightly outward, and lands exactly on the next
shape at the next whole value.

## Stage Shape

A flat Stage element with no card chrome, drawn as a **Figure**: a rectangle,
circle, arc, or polygon
with an optional fill (a tone, or the card `surface` for an opaque panel) and a
stroke that draws on along its outline from twelve o'clock. Figures turn about
their center; an arc may carry arrowheads. Like a card, a figure's outline catches
a passing packet's reflection and its fill takes an arrival's flood; a flash lifts
its stroke, not the whole fill.

## Path

A drawn Stage connection through **Waypoints**: world points and positioned
elements. Hops that leave or enter an element attach like a beam; runs between
points are straight with rounded corners, a Catmull-Rom curve, or an authored
cubic Bézier chain. A path draws on and can be trimmed from its start; its
**Arrowheads** ride the drawn tip, so an arrow grows as it draws. An element
waypoint between a path's ends is a **Stop**: it splits the path into legs.

A packet rides a beam or a path. On a path it **Relays**: each leg is a whole
packet life (gather, flight, landing) on the one packet clock, and the next leg
gathers at the stop's far side a moment after the previous leg lands, so one
packet element crosses a whole chain. A packet whose life has ended can be sent
again; the new dispatch restarts its clock.

## Icon

A monochrome SVG drawn on the Stage through the camera: a bundled Phosphor icon
by name, or SVG path data. It is rasterized once into the Stage's text atlas and
tinted by its Tone, so it sizes in world pixels and defocuses like text.
An optional explicit sRGB **Pigment** overrides the theme's Tone for artwork whose
color is part of its identity. It still passes through the Stage's exposure and
highlight rolloff. Multiple paths can form a multicolored composition without
changing its geometry. `StagePost::FLAT` removes ambient optical treatment for
editorial graphics; regular and bold Helvetica labels share the same camera.

## Sprite Sheet

An image sequence of one portrait's expressions crossed with mouth shapes and a
blink, laid out in `lipsync::SpriteSheet` slot order. A **Sprite** plays it by
cutting a footage playhead to one frame at a time, so lip sync, blinks, and
expression changes are pure functions of plan time. A **Viseme** is the mouth
shape (`lipsync::Mouth`) a letter or pause shows on the sprite tick.

## Caption

Short lines of styled CommitMono text in an explainer's terminal voice. Spans carry
a Tone, so one keyword can take the accent while the rest stays plain. The `typed`
channel reveals characters in order and the accent block caret marks the typing
position; alignment uses each line's full width, so centered text never slides
while it appears. An optional chip draws a rounded surface behind the text.

## Rolling Number

A value such as `rc.112`, `0/8`, or `1,383` whose digits roll in place when it
changes, ported from `@kitlangton/rolling-number`. Each digit place (by numeric
run and place value, independent of separators) is a wheel; a change turns every
changed wheel the way that number moved, while unchanged digits stay still. New
places rise in after their room opens and old ones fade out as every glyph glides
to its new position; separators and literals fade rather than roll. Static prefix
and suffix spans slide with the layout. The values and their times belong to the
recipe; a later change redirects wheels from their current position and velocity.
It is a display of authored values, not a numeric tween.

## Tree

A JSON value shown as a foldable, syntax-tinted outline: a plan a program emits,
a config file, an API payload, or a state shape. Every row's identity is its
JSONPath. Opening a node opens room for its children, so rows below slide down
while rows above stay still; folding reverses it and leaves a summary such as
`{…} 4 keys`. A scalar's later values roll in place in its row. Highlighting a
path lights its row. A tree scrolls through a fixed window when it has more rows.

## Plot

A function chart for explaining motion and metrics: curves over an x and a y
axis, each the Scene Program's own sampled points (for a motion, position with
its exact velocity as the slope). The renderer draws a curve on along its
length, rides a dot along it at the shared playhead, and shows the tangent
there as a velocity arrow; it never evaluates a function. Marks label a
moment, such as a retarget. A Plot depicts computed values; it does not compute
them.

## Lanes

A track view of channels over time: a seconds ruler, one lane per channel with
keyframe diamonds at its event times and a sparkline of its values, cue
brackets above the ruler, and a scrubbing playhead. Keys the playhead has
crossed light and cool by playhead distance, so scrubbing either way samples
deterministically. Built from a Scene Plan, its sparklines are the compiled
Property Tracks the renderer would play. Plot and Lanes share one **Axis**
vocabulary (range, ticks, label, unit).

## Callout

A short label on a crisp leader line pinned to something on screen: a dot and
ring mark the Callout Anchor, the leader draws out from it, and the label rises
in at its end. A **Callout Anchor** is a fixed canvas point, an edge of a
positioned Stage element seen through the camera, an edge of an editor
Semantic Target, or a Sequence Diagram participant header or row. Anchors are resolved by the root recipe at every Temporal
Sample, so the callout follows its target with no lag; moving to another
anchor springs weight channels that blend the two resolved positions. Labels
slide back inside the frame rather than leave it, and the leader follows.

## Text Surface

A natively drawn stand-in for a familiar interface (a terminal, a chat app, a
pull request's file list) inside a floating **Window**: a themed panel whose
body settles in like a Stage card while its content follows about 65 ms later.
Unlike a Video Card, a text surface follows the Presentation Theme and re-times
with the scene; it depicts a session rather than recording one.

## Terminal

A text surface of CommitMono rows: commands typed after a prompt at a natural,
deterministic keystroke cadence, output printed or streamed line by line, and
task lines led by a spinner that resolves into a check or a cross. Every line has
a stable ID and opens its own row, so once the window is full a new line slides
every older one up by exactly its room; `clear` lifts everything through a
scroll floor. A line is never re-laid out or replaced.

## Chat Thread

A conversation in a Slack-like or iMessage-like window. Messages have stable IDs
and authors with avatars; a run by one author shows its name once. A typing
indicator holds the next message's slot with dots on their own clock and grows
into the message when it is said, so the slot keeps its identity. The thread is
anchored to its composer: a new message pushes every older one up by exactly the
room it opens. Reactions pop in beneath a message and open their own row.

## Changed Files

The opener of a pull-request film: file paths with added, modified, deleted, or
renamed badges, `+N −M` counts, and GitHub's five-block **Diffstat**. Rows hold
fixed slots, so revealing one never moves another; focus lights one row while the
rest recede. The header's totals are Rolling Numbers that roll as rows land.

## Lower Third

A name and an optional role beside an accent bar, introducing a speaker or
subject. The bar draws up, the name slides out from behind it, and the role
follows; leaving reverses the order. Text never shows beyond the bar.

## Readout
A number whose digit wheels follow one channel's continuous value, like an
odometer: each displayed unit holds still for most of its interval and rolls
to the next near the rounding boundary, a place turns only while every place
below it rolls over from 9, and a new leading place rolls in as it opens its
room. A wheel turning too fast to read smears instead of strobing. Unlike a
Rolling Number, which plays authored changes on its own schedule, a Readout is
a pure function of the value, so a bar's label counts with the bar and a timer
counts with its ring.
## Checklist
Rows of items that wait, run, and resolve, like CI checks. Each item has four
channels: `reveal`, the status spinner's `spinner` and `mark` clocks, and an
`outcome` (pending, done, failed, skipped). A resolution waits for the
spinner's next handoff crossing, then the same tip draws the ✓ or ✕; a skip
coasts the spinner out, draws a dash, and strikes the label through. An
optional rail fills below each resolved item, and a title counts what passed.
Starting a resolved item again clears its mark, as for a retry.
## Meter
A circular gauge, a closed countdown ring, or a linear bar driven by one
`value` channel: the arc, its head, the lit ticks, the tone, and a centered
Readout all follow it. Thresholds hand the value to another Tone, whole at the
threshold itself; a countdown sweeps the ring closed on a clock and flashes as
it crosses each threshold and runs out.
## Benchmark Bars
A horizontal bar chart comparing one or more series per row (before and
after) on one shared Axis. Bars grow on springs with Readouts at their ends; a
delta chip compares two series (`−34%`, tone by whether lower or higher is
better); and rows re-sort by springing their own `slot` channels, so a row
keeps its identity while it races to its new place, passing over the rows it
overtakes.
## Subtitles
Burned-in captions driven by narration word timings. Words chunk into pages of
balanced lines no wider than a maximum, breaking at sentence ends, pauses, and
width; each page replaces the last with a short fade and rise, its backing
surface morphing rather than blinking. The spoken word takes the highlight
tone with a pill that glides from word to word. Revealed **word by word**, a
page shows only what has been said, each line centered on it; a tilted page
steps its words up or down while each stays upright. Everything after measuring
is a pure function of time and the word list.
## Confetti
A success burst: seeded paper pieces and sparkles launched in a cone under
gravity and drag, fluttering and tumbling as they fall, all closed-form from
one `burst` clock, so the burst samples in any order and is the same for its
seed every time.
## Anchor
A place an overlay pins to whose position only the renderer knows: a fixed
canvas point, an edge of a positioned Stage element seen through the camera
(including a jolt's roll and punch-in), an edge of an editor Semantic Target
after line motion and the panel's projection, or an edge of a Sequence Diagram
participant's measured header or a row's span (a message's arrow, a note, an
End mark) as the diagram moves. An overlay lists its anchors, each
with an optional pixel offset, and `anchor.<id>` weight channels choose among
them; the first holds the overlay until a weight moves. The renderer resolves
every weighted anchor at every Temporal Sample and the overlay draws at the blended
point, so it never lags its target. Moving between anchors springs the weights,
so an interrupted move keeps its velocity. Callouts, captions, Rolling Numbers,
text, and images share this model. An anchor names a target; it does not parent
one actor to another, so there is no hierarchy, and a pinned overlay follows its
target's position, not its scale.
## Image
One planned image media placement (PNG, JPEG, or WebP) drawn bare or inside a
framed card, through the same projected card as a Video Card, so it can move,
scale, rotate, tilt, defocus, and pin to an Anchor. The file is decoded once;
its width at rest is authored and its height follows the image.
## Footage
Any image, video, or image sequence used as scene material, drawn as a
screen-space overlay (through the projected card, with anchors) or as a Stage
element seen through the camera (with depth, parallax, and depth of field).
Images and Video Cards are footage too. Footage is cut to a **Mask** (a
rounded box, a circle, or a polygon), fills its box by a **Fit** (cover,
contain, or fill), can sit on a card's frame, and takes a color **Treatment**
(desaturate, tint toward a Tone, dim) that marks it as reference material.
Its focus window zooms into a region; easing it between regions is a Ken
Burns move.

A **Clip** is how a source plays: the part of the file it trims to, a rate,
whether it holds, loops, or bounces at the trim's ends, whether it runs in
reverse, or one frame it freezes on. Its placement names the file and the span
it is available in, and its timeline start is where the **Playhead** starts.
The playhead is a channel of seconds into the trim: unwritten, the clip plays
naturally; written, it freezes, ramps between speeds, stutters, or scrubs,
each a pure function of plan time. A clip's own audio is an ordinary audio
placement of the same file that follows its trim and start (and its loops),
not its rate or retimes.

A **Collage** is footage laid out by a layout (a grid, masonry columns, a
scatter, a pile with seeded turns and overlaps, or a filmstrip) whose tiles a
Scene Program staggers in. Layouts are values, not containers: each piece is
its own actor.
## Lens
A loupe of thick glass laid over the frame: a circle, or a capsule for reading
along a line. Its flat top enlarges a focus point evenly, so what it shows stays
legible; its rounded rim bends sight inward by Snell's law, strongest at the
edge, splits color slightly there, catches a specular light, and casts a soft
contact shadow. A lens refracts whatever is composed beneath it (any root and
the overlays drawn before it) at every Temporal Sample. It pins to the same
anchors as a Callout and glides between them like a puck of glass, following
its card or code range as they move. A lens **condenses** rather than fades:
its presence grows its size, rim, and magnification together. Its focus can
sit away from its center, so the glass can float beside what it reads. The
same glass, frosted and unmagnified, is a material for chips: a glass caption
refracts the scene behind its text instead of covering it.
## Tone

A semantic color role shared by explainer recipes: plain, request, success,
error, warning, muted, and accent. Status tones keep one color in every
Presentation Theme; the others follow the palette.

## Line Mark

A diff decoration on one editor line: added or removed. The mark tints the row,
draws an accent bar and a vector +/- sign in the gutter, and joins consecutive
marked rows into one band. Its presence is the `mark.<line-id>` Continuous Channel,
so a removed line can turn red just before a Code Snapshot removes it.

## Stepped Diff

One code change told as ordered steps over Stable Lines: each line is kept,
added in a step, or removed in a step. Added lines carry an added Line Mark;
removed lines turn red just before their step. A pure insertion or removal first
holds blank rows so moving code never crosses entering or leaving code.

A Stepped Diff line may name ranges of its text and carry Inlay Hints; its
declared editor then pins Semantic Targets to those ranges like any editor.

## Diagnostic

A severity-toned wave (error, warning, info) under one Semantic Target, with a
gutter icon beside its line. The wave draws on along the range's length, keeps
its shape as the range moves, and follows line motion and Inline Reveals because
the editor measures its target at every Temporal Sample. Clearing it relaxes the
wave flat as it fades. It depicts a compiler's report; it does not type-check.

## Hover Card

An IDE tooltip pinned to a Semantic Target: highlighted code lines (a type
signature) and toned prose (a diagnostic message) in sections divided by rules,
with a small pointer aimed at the range. It pops in by fading up and rising into
place, sits above or below its range as authored, and slides to stay inside the
editor rather than flip sides, so a moving range moves it continuously.

## Inlay Hint

Ghost text, such as an inferred `: Effect<User, NotFound, Database>`, that opens
room inline after a range. It is an Inline Reveal of its own inline part (part ID
`inlay:<id>`) drawn dim on a faint chip, so the line keeps its identity, every
other part keeps its own, and code after it moves aside rather than being replaced.

## Cursor

A text caret and its selection over Semantic Targets. The caret sits at a fraction
(`head`) of a weighted target and the selection spans `tail` to `head`; moving
between targets springs anchor weights like a Callout's. Its blink is a pure
function of a `blink` clock that restarts whenever the caret moves, so the caret
holds solid while it moves and for a moment after, then blinks.

## Asset

Immutable source material identified independently from any use on the timeline. Audio, video, and image assets retain their original files while edits refer to them non-destructively.

## Clip

One positive-duration source range from an audio or video asset. Moving, copying, removing, or changing an audio clip's gain changes the edit without changing its source asset.

## Script Clip

A clip on the primary spoken-media track. Its transcript may drive structural edits, captions, and semantic timing.

## Layer Clip

Accompanying timed media such as music, sound effects, or B-roll. Layer clips share the media clock but do not implicitly become part of the editable transcript.

## Cue

A named timeline range. Cues may be authored or imported from transcript word and phrase timing; their start and end can synchronize motion and media.

## Transcript

An ordered set of words with source start and end times. Looking up a word occurrence produces a cue range on the same exact media clock used by script clips. A phrase lookup matches consecutive whole words, ignoring case and punctuation and treating number words as digits, so choreography can be keyed to what narration says rather than to seconds; re-voicing a clip re-times the scene.

## Media Placement

A compiled relationship between a clip's immutable source range and its scheduled timeline range. Media placement time uses integer nanoseconds so edit boundaries remain exact across repeated edits.

## Generated Resource

Audio a Scene Program declares rather than supplies: a spoken line, a
dialogue, a sound effect, or a resource derived from another by an effect
such as a pitch shift. It has an author-chosen id that names its role in the
scene, and a Resource Key that names its content. Once reconciled it is an
Asset with an exact duration and, for speech, a Transcript, so choreography is
timed to what was actually generated.

## Resource Key

The hash of everything that affects a Generated Resource's audio: backend,
model, voice, the exact text with its direction tags, settings, seed, the
lines it is stitched after, output format, post-processing, and how its words
were timed. Equal keys mean interchangeable audio; a derived resource's key
includes its source's, so regenerating a source regenerates what derives from
it. A key identifies a recipe, not the bytes: providers are not deterministic.

## Media Lock

The recorded state of a scene's Generated Resources (`media.lock.json`): for
each id, the key it was generated for, its file, exact duration, words, and
provenance such as request IDs and billed credits. Files live in a
content-addressed store beside it. Narration made before the lock existed can
be adopted into it, keyed by the recipe that made it, without regenerating.

## Reconcile

Comparing declared Generated Resources with the Media Lock and acting on the
delta: `=` up to date (the lock holds this key and its file), `+` create (no
entry for the id), `~` replace (the entry was made for another key), and `-`
orphan (an entry nothing declares). A run generates what is missing; a plan
reports the delta and its cost and calls nothing; a draft stands in free local
audio; a prune deletes orphans. Only declarations, never elapsed time or
file dates, decide what is generated.

## Property Track

The compiled trajectory of one scalar actor property. A later spring on the same track begins from the earlier trajectory's sampled position and velocity.

A spring's settling time is deterministic. After that time the segment stays exactly at rest unless another segment retargets it; starting an unrelated channel cannot wake it.

## Semantic Target

A stable named selection owned by an actor recipe, such as a logical range inside a stable code line. The plan core preserves identity and validates references; the renderer recipe measures concrete geometry. Highlights and pointers attach to Semantic Targets rather than authored screen coordinates.

Attachment follows sampled visible layout, including collapsed Inline Reveals and moving Stable Lines. Expanded text measurements are a preparation baseline, not final coordinates for every step.

## Inline Reveal

A transition that expands or collapses authored spans inside a Stable Line. Variable spans animate width, opacity, and blur while common prefix, infix, and suffix spans retain identity and move to their new positions without replacement. Opposing reveals can exchange slot alternatives horizontally while preserving maximum stability.

## Pointer

A stable visual actor that directs attention to a semantic target. Its position and opacity are ordinary property tracks, so retargeting and temporal sampling use the same motion system as every other actor.

## Task

A stable visual actor representing one Effect computation. The `effect-task` recipe schedules its idle, running, succeeded, failed, death, hidden, and retry state changes; the stable task ID preserves identity across those changes.

## Task State

One meaningful snapshot of a Task. A task state selects semantic content and visual targets, while the renderer derives the transition from the preceding state at arbitrary media time. A succeeded task may carry a result or represent payload-free completion.

Planned interactive Tasks lower state changes into continuous geometry and content-presence tracks. A running Task can keep the local playback clock active after those tracks settle, while pause and reduced motion still stop its ambient animation. It illustrates a computation rather than executing an actual Effect.

## Video Card

One planned video media placement played inside a framed card: rounded corners,
a theme-aware material, border, and shadow, and an optional title bar. The plan
clock maps through the placement into source time, so a cue or range render shows
the same footage frame as the full film. Its focus window crops into a region of
the frame (a zoom into part of a screen recording) while the card itself can move,
scale, and tilt. Recordings remain evidence of actual product behavior rather than
a reconstructed simulation; explanatory text is ordinary overlay actors.

## Temporal Sample

One evaluation of the complete scene within an output frame's shutter interval. Psychopomp combines a frame's weighted temporal samples (its exposure) to produce motion blur from real scene movement; a Stage adds their light on the GPU before developing the frame.

## Editor Frame

The renderer-neutral description of one sampled editor scene: panel position, focus state, and placed code lines.
