# Craft

What the best explainer and launch films do, and where each technique lives in
Psychopomp: **built** (a primitive or check enforces it), **rule** (the
`psychopomp` skill says it; nothing enforces it), or **gap** (not built yet).

Sizes are pixels on a 1920×1080 frame. Claims marked ✓ were checked against the
primary source on 2026-11-05; the rest come from secondary breakdowns and are
labelled as such.

## Why templated films look samey

The reference schools are more templated than Psychopomp, not less: Linear
repeats a few motion behaviours across dozens of films, Manim has one colour map,
Apple has one typeface. They escape sameness because their templates are loud:
titles four times the body size, colour that means one thing for the whole
film, a camera that pushes into the thing being named, and rooms that change at
act breaks. Psychopomp's old defaults were timid instead (22 px tables, 12–19 px
strip labels, one grey mono voice, thin floating boxes, one dark background), and
timidity is uniform. The fix is fewer, louder defaults.

## Type

| Technique | Source | Where |
|---|---|---|
| Body 40–60 px at 1080p, titles ≥ 50% larger, ≤ 30 chars a line, ≤ 3 lines ✓ | [legibility.info](https://legibility.info/rules-for-text-in-videos) | **built** `kit::scale` (`BODY` 52, `HEADLINE` 112) |
| A type scale with an 8–10× range: hero 140–240, headline 80–120, body 40–56, label 28–36 | [showtime typography](https://faviovazquez.github.io/showtime/docs/typography.html) | **built** `kit::scale` (`HERO` 220 … `LABEL` 36) |
| Text under 2.6% of the short side is too small ✓ | HyperFrames [`craftAudit.ts`](https://github.com/heygen-com/hyperframes/blob/main/packages/cli/src/utils/craftAudit.ts) (`typeFloorFraction: 0.026`) | **built** `kit::scale::FLOOR`, `plan check` `small_text` (33318d6) |
| Dwell ≥ 13 characters a second, so a 30-char line holds ≥ 2.3 s ✓ | [legibility.info](https://legibility.info/rules-for-text-in-videos) | **rule**; **built** helper `kit::scale::read_hold` (0.4 s + 0.2 s a word, from [Claude-Video-Skills](https://github.com/Neelshah1810/Claude-Video-Skills)); **gap** no check warns on a short hold |
| On-screen text is an index, not a transcript: 1–3 word stamps on the term as it's said, no burned-in captions ✓ | [PandaStudio, Fireship measured](https://www.writepanda.ai/blog/fireship-editing-style-measured/) | **rule** never repeat the narration word for word; **built** `kit::moves::stamp` |
| Emphasise with weight, colour or scale, one at a time | [showtime §4](https://faviovazquez.github.io/showtime/docs/typography.html) | **rule** |
| A statement card: 1–3 words, hero size, nothing else in frame | Apple keynote breakdowns (secondary) | **rule**; `kit::broadsheet` headline on a colour field |

## Code

| Technique | Source | Where |
|---|---|---|
| At most 12 lines at once, ≥ 32 px, real theme colours | [showtime](https://faviovazquez.github.io/showtime/docs/typography.html) | **built** `kit::scale::MAX_CODE_LINES`, `CODE` 42, `syntax` highlighting (b62a465) |
| Focus a range, dim the rest | [Code Hike focus](https://codehike.org/docs/layouts/scrollycoding), [OpenSlides](https://github.com/codewiththiha/OpenSlides) | **built** `CodeBlock` focus and `SyntaxTheme.dim` (0343009) |
| Punch in to the line being discussed, never the whole window | [Fireship-style notes](https://skillsmp.com/creators/kevinbadi/open-edits/skills-fireship-style-edit) (secondary) | **built** `CodeBlock::frame` pushes in on lines but never crops a column (0343009) |
| Token morphs cap at 900 ms, staggered per token ✓ | Code Hike [`smooth-pre.tsx`](https://github.com/code-hike/codehike/blob/main/apps/web/components/smooth-pre.tsx) (`MAX_TRANSITION_DURATION = 900`) | **gap** keyed token morphs; today a diff is marks plus focus |
| Show a change as a diff, not two slides | [Shiki Magic Move](https://shiki.style/packages/magic-move) | **built** `CodeBlock` `Deleted`/`Added` marks; **gap** animated insert/delete height |

## Colour and rooms

| Technique | Source | Where |
|---|---|---|
| Colour as meaning: a few semantic slots reused all film | Manim [`default_config.yml`](https://raw.githubusercontent.com/3b1b/manim/master/manimlib/default_config.yml) (`COLORMAP_3B1B`) | **built** `kit::ide` and `kit::broadsheet` palettes, one accent each |
| A background change marks an act break; a film has 3–4 rooms | Kurzgesagt process (secondary), Manim | **built** `kit::field` (`field`, `slab`, `flood`, `drain`, `wipe`) |
| Flat, solid fields behind text; no gradients or busy backgrounds ✓ | [legibility.info](https://legibility.info/rules-for-text-in-videos) | **built** `kit::field`; **built** `diagram::ink` draws edges flat on a field (0343009) |
| Alternate layouts every 20–40 s | [7 laws of retention editing](https://www.writepanda.ai/blog/retention-editing-7-laws/) | **rule** films alternate rooms; one visual idea per beat |

## Camera and motion

| Technique | Source | Where |
|---|---|---|
| Push the camera to the thing being named | [Linear Pulse breakdown](https://impractical.ai/launch-videos/linear-pulse) | **built** `StageCamera::frame` by element ids (ab75688), `CodeBlock::frame` |
| Eases by role: arrivals long-tailed, travel in-out, exits accelerating | School of Motion [cameras](https://schoolofmotion.com/blog/cameras-after-effects) (secondary) | **built** `Ease::ARRIVE`, `TRAVEL`, `LEAVE` (b62a465) |
| Three or so motion behaviours, repeated, read as a house style | [Frame24 on Linear](https://frame24.app/blog/how-linear-makes-launch-videos) (secondary) | **built** `kit::moves` (`arrive`, `slide`, `stamp`, `pop`, `show`, `out`, `dim`) (51aab76) |
| Land each move on its word, lead ≤ 100 ms | [Fireship-style notes](https://skillsmp.com/creators/kevinbadi/open-edits/skills-fireship-style-edit) (secondary) | **built** `kit::moves::on` with `LEAD_WORD` 80 ms (51aab76) |
| Diagrams build clause by clause, never shown finished first ✓ | [PandaStudio](https://www.writepanda.ai/blog/fireship-editing-style-measured/) | **rule**; `on()` anchors each element to a word |
| Two speeds: dense 3 s beats, plus one long build that moves inside ✓ | [PandaStudio](https://www.writepanda.ai/blog/fireship-editing-style-measured/) (median shot ~3 s, longest 24–47 s) | **rule** |
| Hold the result longer than feels right | [Frame24](https://frame24.app/blog/how-linear-makes-launch-videos) (secondary) | **rule**; segment `TAIL` |
| Match cut: one element keeps its place across a cut | [Vox/Harris breakdown](https://www.youtube.com/watch?v=74p5B7GrnlM) | **gap** cross-segment identity |
| Data before annotation, fewer things moving at once | [NYT 512 paths](https://source.opennews.org/articles/nyts-512-paths-white-house/), [Red Planet](https://source.opennews.org/articles/how-we-made-rewind-red-planet/) | **rule** |

## Sound

| Technique | Source | Where |
|---|---|---|
| Loudness −16 to −14 LUFS, true peak ≤ −1 dBTP | [inference.sh explainer guide](https://github.com/inference-sh/skills) (secondary) | **built** `plan check --audio` reports LUFS and peaks |
| Sound marks events; it doesn't fill time | [Frame24](https://frame24.app/blog/how-linear-makes-launch-videos) (secondary) | **rule** |

## Authoring safety

These came out of building the two proof films, not out of the research.

- **Text paints over shapes.** Labels close at depth −0.5, so a slab or field
  never covers text. A room that should be empty hides its own elements.
- **Unknown properties panic while authoring.** `StageActor::channel` rejects a
  property the renderer's preflight would reject, so the scene line that wrote
  it is on the stack (2435734).

## Not verified

Kurzgesagt, Vox, Apple, Linear and Stripe publish no written craft guide; claims
about them come from third-party breakdowns. Rive and Lottie have no craft docs
worth citing.
