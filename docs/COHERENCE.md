# Psychopomp Public API Coherence Audit (`docs/COHERENCE.md`)

This document records the public API coherence audit after merging all parallel branches (`authoring`, `anchors`, `camera`, `components-text`, `components-viz`, `diagnostics`, `effects`, `lens`, `media`, `modernize`, `stage-forms`, `transitions`) with `dsl`.

---

## 1. Completed Unifications on `dsl`

1. **`PlanBuilder` Is the Single Mutable Context (`author.rs`, `score.rs`):**
   - `Beat` plays directly against `&mut PlanBuilder` (`scene.at(time, beat)`, `scene.play_cue(id, time, beat)`, `at!(scene, time => ...)`).
   - `PlanBuilder::actor_data::<T>(&self, actor)` and `PlanBuilder::latest_literal(&self, actor, property)` (alongside `sample` and `destination`) allow stateless value handles (`RollingNumberActor`, `TreeActor`) to store their evolving schedule in `PlanBuilder`.
   - `PlanBuilder::finish` stably sorts continuous and state channel events by `at_nanos`, making out-of-order parallel writes valid and commutative while preserving equal-timestamp source order.

2. **All 25 Actor Handles Are `Clone` Values with Uniform Accessors and Score Wrappers:**
   Every actor handle in `crates/psychopomp` now derives `Clone, Debug`, exposes `actor(&self) -> &ActorHandle` and `id(&self) -> &str`, and provides a value handle in `psychopomp::score` (plus `.beats()` conversion) whose methods take `&self` and return `impl Beat + '_`:

   | Module | Imperative Handle | Score Value Handle | Score Beat Methods |
   | --- | --- | --- | --- |
   | `stage` | `StageActor` | `score::Stage` | `channel`, `channels`, `set`, `to`, `bounce`, `spring`, `ease`, `glide`, `fade_in`, `fade_out`, `orb_in`, `clock`, `clock_for`, `hit`, `kick`, `jolt`, `settle_in`, `type_in`, `connect`, `connect_contacting`, `disconnect`, `send`, `send_arriving`, `relay`, `morph`, `twang`, `land`, `glitch`, `zap`, `charge`, `hum`, `dissolve`, `materialize`, `scan`, `raise`, `lower`, `rewind`, `unburst`, `shock_kick`, `resolve_spinner`, `halo`, `halo_out`, `ring_timer`, `dim`, `swap_labels`, `swap_status`, `camera()` |
   | `stage::camera` | `CameraRig` | `score::Camera` | `establish`, `frame`, `push_in`, `pull_back`, `drift`, `whip`, `orbit`, `dolly_zoom`, `roll`, `focus_on`, `aperture`, `follow`, `release`, `handheld` |
   | `caption` / `chrome` | `CaptionActor` | `score::Caption` | `header`, `chip`, `footer`, `show`, `hide`, `move_to`, `type_in` |
   | `callout` | `CalloutActor` | `score::Callout` | `show`, `hide`, `move_to`, `emphasize` |
   | `rolling` | `RollingNumberActor` | `score::RollingNumber` | `show`, `hide`, `roll`, `move_to` |
   | `tree` | `TreeActor` | `score::Tree` | `show`, `hide`, `open`, `close`, `reveal`, `scroll_to`, `highlight`, `set` |
   | `plot` | `PlotActor` | `score::Plot` | `show`, `hide`, `draw`, `fade`, `ride`, `stop_ride`, `velocity`, `mark` |
   | `lanes` | `LanesActor` | `score::Lanes` | `show`, `hide`, `scrub`, `emphasize` |
   | `sequence` | `SequenceActor` | `score::Sequence` | `show`, `hide`, `to`, `animate`, `reveal`, `fade`, `strike`, `participant` |
   | `video` | `VideoActor` | `score::Video` | `fly_in`, `focus`, `unfocus`, `hide` |
   | `terminal` | `TerminalActor` | `score::Terminal` | `show`, `hide`, `prompt`, `idle`, `type_command`, `type_at`, `print`, `print_text`, `stream`, `spin`, `resolve`, `highlight` |
   | `chat` | `ChatActor` | `score::Chat` | `show`, `hide`, `stamp`, `typing`, `say`, `say_text`, `stream`, `react`, `highlight` |
   | `changed_files` | `ChangedFilesActor` | `score::ChangedFiles` | `show`, `hide`, `reveal_row`, `reveal`, `highlight`, `focus`, `unfocus` |
   | `lower_third` | `LowerThirdActor` | `score::LowerThird` | `show`, `hide` |
   | `checklist` | `ChecklistActor` | `score::Checklist` | `show`, `hide`, `reveal`, `reveal_item`, `start`, `resolve` |
   | `meter` | `MeterActor` | `score::Meter` | `show`, `hide`, `set`, `sweep` |
   | `bars` | `BarsActor` | `score::Bars` | `show`, `hide`, `reveal_rows`, `set`, `grow`, `sort`, `reveal_deltas` |
   | `subtitles` | `SubtitlesActor` | `score::Subtitles` | `show`, `hide` |
   | `confetti` | `ConfettiActor` | `score::Confetti` | `burst` |
   | `text` | `TextActor` | `score::Text` | `show`, `hide`, `swap`, `move_to` |
   | `image` | `ImageActor` | `score::Image` | `fly_in`, `hide`, `move_to` |
   | `lens` | `LensActor` | `score::Lens` | `show`, `hide`, `move_to`, `slide`, `magnify`, `resize`, `focus` |
   | `ide` | `DiagnosticActor`, `HoverActor`, `CursorActor`, `InlayHint` | `score::{Diagnostic, Hover, Cursor}`, `InlayHint::{show_beat, hide_beat}` | `show`, `clear`, `hide`, `move_to`, `select` |
   | `sfx` / `media` | `Sfx`, `Audio` | `Sfx::beat(id, gain_db)`, `Audio::beat(gain_db)` | Layer clip placement as a `Beat` |

3. **Timing & Feel Deduplication (`author.rs`, `score.rs`):**
   - `score` re-exports `MILLISECOND`, `millis`, and `spread` from `author`.
   - `score::Feel` aliases every named `SpringPlan` (Kit's palette and the explainer roles) directly.
   - `score::CueTime` covers `PlanTime::not_before` plus `.after`, `.early`, and `.reply` (`stage::reply_after`) for both `u64` and `Span`.

---

## 2. Intentionally Retained Distinctions & Deferred Items

1. **`math::random::lattice_noise2` vs `value_noise2`:**
   - Retained both: `lattice_noise2` uses integer lattice hashing bit-identical to `fx_lattice2` in `effects/dissolve.wgsl` so CPU ash particles match GPU shader pixels, while `value_noise2` / `fractal_noise2` uses rotated octaves and minimum-jerk (`smootherstep`) interpolation for organic transition edges.
2. **Migrating Legacy Imperative `scenes/*` to `score::*`:**
   - Existing scenes (`pr-walkthrough`, `pr-50231`, `psychopomp-intro`, component showrooms) continue to compile unchanged and emit byte-identical plans via the imperative `*Actor` methods. New scenes and incremental refactors can adopt `score::{Stage, Camera, Caption, ...}` and `at!` directly, as proven in `scenes/hello/src/bin/hello_dsl.rs` and `scenes/2password/src/bin/2password_dsl.rs`.
