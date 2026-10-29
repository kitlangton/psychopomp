# Stage and Overlay Render Throughput

## Goal

Reduce render and frame-snapshot latency for Stage scenes with overlays (such as
`weekly-ten` at 1920×1080, 60 fps, 24 shutter samples, `neutral` theme) without
reducing temporal sample count or changing rendered pixels.

## Benchmark

```bash
CARGO_BUILD_JOBS=4 cargo build --release -p psychopomp-render
bun scripts/bench-render.ts <label>
```

Each run performs 1 warmup iteration and 7 measured iterations over bounded
windows of `output/weekly-ten/plan.json`:

- `quiet_window_ms`: `plan render --range 3.15..3.65 --theme neutral` (30 frames,
  camera jolt and label bounce, confetti inactive)
- `burst_window_ms`: `plan render --range 4.15..4.65 --theme neutral` (30 frames,
  `core` orb combustion + 1,100 ballistic embers + pressure wave + `bolt0`
  lightning + camera jolt + 360 confetti pieces)
- `confetti_window_ms`: `plan render --range 5.10..5.60 --theme neutral` (30
  frames, `bolt0` restrike + 360 confetti pieces + 110 Stage elements)
- `render_windows_ms` (**primary metric**): sum of the three 30-frame render
  windows per iteration (90 frames / 1.50 s total)
- `snapshot_shutter_ms`: `plan snapshot 1.0,3.5,4.3,5.2,6.8,9.4 --shutter --theme neutral`
  (6 shutter-exposed PNGs, exercising the frame/snapshot/contact-sheet review loop)

Guardrails:

- Identical resolution (`1920×1080`), frame rate (`60`), temporal samples (`24`),
  theme (`neutral`), and release profile across all runs.
- `plan snapshot ... --compare` against `output/weekly-ten/bench-baseline-frames/{shutter,still}`
  and `verify compare target/verify/weekly-before --expect weekly-ten`.

## Baseline, Apple M2 Max

| Metric | Median (ms) | MAD (ms) | Min (ms) | Max (ms) |
|---|---:|---:|---:|---:|
| `render_windows_ms` (primary, 90 frames) | 27,519.27 | 2,195.63 | 19,304.52 | 32,427.55 |
| `burst_window_ms` (`4.15..4.65`, 30 frames) | 15,765.66 | 1,363.40 | 11,936.28 | 21,021.39 |
| `confetti_window_ms` (`5.10..5.60`, 30 frames) | 9,808.52 | 875.73 | 6,170.54 | 10,914.28 |
| `quiet_window_ms` (`3.15..3.65`, 30 frames) | 1,518.42 | 258.97 | 1,003.00 | 2,178.25 |
| `snapshot_shutter_ms` (6 shutter PNGs) | 4,729.29 | 458.10 | 3,795.39 | 6,254.08 |

Single-frame diagnostic profile (`PSYCHOPOMP_PROFILE=1`):

- `t = 3.5s` (quiet/jolt): `stage = 94.86 ms` (cold first frame), `overlays = 1.99 ms` (`n = 1` unique overlay sample)
- `t = 4.3s` (peak burst): `stage = 77.41 ms` (`cpu_sample = 15.77 ms`, `submit = 10.11 ms`, `develop_wait = 51.49 ms`), `overlays = 133.38 ms` (`ov_draw = 34.86 ms`, `ov_accum = 98.52 ms`, `n = 24`)
- `t = 5.2s` (bolt + confetti): `stage = 42.48 ms` (`cpu_sample = 10.77 ms`, `submit = 2.43 ms`, `develop_wait = 29.24 ms`), `overlays = 91.80 ms` (`ov_draw = 16.34 ms`, `ov_accum = 75.46 ms`, `n = 24`)
- `t = 6.8s` (weeks + confetti): `stage = 18.84 ms` (`cpu_sample = 11.12 ms`, `submit = 2.04 ms`, `develop_wait = 5.65 ms`), `overlays = 71.48 ms` (`ov_draw = 5.10 ms`, `ov_accum = 66.38 ms`, `n = 24`)

## Hypothesis Loop

### Hypothesis 1: Region-scoped overlay exposure for Confetti + row-bucketed `Region::covering`

`PreparedPlan::render_exposure` only recognized `callouts` in `only_callouts_differ`.
When `confetti` (`party0`, `party1`) burst over a Stage, every frame fell back to
full-frame CPU `exposure::accumulate`: cloning `base` (8.3 MB) 24 times and
accumulating all `2,073,600` pixels × 24 samples into a 33.1 MB `Vec<f32>` buffer.
In addition, `Region::covering` scanned all boxes for every row `0..1080` (`O(boxes × height)`).

Change:

- Bucket boxes by row in `Region::covering` (`O(boxes × box_height)`).
- Report per-piece and ignition `Box2` bounds from `PreparedConfetti::bounds`
  (`confetti_bounds` in `render/viz/confetti.rs`) and include confetti in the
  region-scoped `accumulate_region` path over a Stage.

Result (**Kept**):

- `render_windows_ms`: **27,519.27 → 7,929.47 ms** (3.47× speedup, MAD 81.65 ms)
- `burst_window_ms`: **15,765.66 → 4,573.31 ms** (3.45× speedup)
- `confetti_window_ms`: **9,808.52 → 2,445.11 ms** (4.01× speedup)
- 0 changed pixels across all 13 shutter reference frames; covered by
  `confetti_bounds_cover_every_inked_pixel`.

### Hypothesis 2: Fast lossless PNG compression in `delivery::write_png`

`verify.rs` used `png::Compression::Fast` because Stage film grain makes the
default deflate level spend ~500 ms per 1080p frame compressing noise, but
`delivery::write_png` (used by `plan frame`, `plan snapshot`, and `scripts/sheet.ts`)
still used default compression.

Change:

- Set `encoder.set_compression(png::Compression::Fast)` in `delivery::write_png`
  and share it from `verify.rs`.

Result (**Kept**):

- `snapshot_shutter_ms`: **3,431.37 → 481.22 ms** (7.13× speedup vs H1, 9.83× vs
  4,729.29 ms baseline, MAD 7.92 ms).

### Hypothesis 4: Early compact-support and zero-density skip in `combustion.wgsl`

In `combustion_volume`, each of the 28 raymarch steps evaluated 8 calls to 3D
value noise (`fx_noise3` / `fx_fbm3`) before computing `distance` and `support =
1.0 - smoothstep(1.03, 1.22, distance)`, even though nearly half the steps lie
outside the ellipsoid `distance < 1.22` where `support == 0.0`.

Change:

- Compute `distance` and `support` before `fx_noise3`/`fx_fbm3` and `continue`
  when `support <= 0.0` or `density <= 0.0`.

Result (**Kept**):

- `burst_window_ms`: **4,510.22 → 4,421.16 ms** (min **4,236.59 ms**).
- 0 changed pixels across all 13 shutter and 7 still frames; `bench-stage-burst.py`
  `burst_entry_mae_rgb8 = 0.000000`.

### Hypothesis 5: Oriented strip culling for plasma channels (`case 10u`) and segment AABB pruning in `stage.wgsl`

A diagonal lightning bolt (`bolt0` from `(100, 100)` to `(960, 540)`) has a
`910 × 490` axis-aligned quad, causing every pixel in the empty corners of the
quad to loop over all 64 polyline segments in `path_nearest`.

Change:

- Store the bolt chord's perpendicular unit vector and `[min_perp, max_perp]`
  band (padded by `reach + 2`) in the unused `light` / `light_color` fields of
  `Prim` (`case 10u`) to reject pixels outside the diagonal strip before
  `path_nearest`.
- In `path_nearest` (`case 10u`) and `case 3u` (polylines), skip segments whose
  AABB expanded by `best` misses `px`.

Result (**Kept**):

- `render_windows_ms`: **8,762.79 → 6,642.43 ms** (MAD 36.93 ms)
- `burst_window_ms`: **4,421.16 → 3,554.77 ms** (MAD 18.16 ms)
- `confetti_window_ms`: **3,098.55 → 2,104.94 ms** (MAD 24.51 ms)
- 0 changed pixels across all 20 reference frames.

### Hypothesis 6: Contiguous per-row slice accumulation in `accumulate_region` + `stage_atlas` sprite deduplication

`accumulate_region` zipped `sum` with a `FlatMap` iterator over `region.rows()`,
preventing slice autovectorization, cloned `first` (8.3 MB) for sample 0, and
remapped all 2,073,600 pixels through `constant` even when `constant` was the
identity map. Meanwhile `stage_atlas` shaped and rasterized 100 identical `"10"`
labels separately.

Change:

- Accumulate and encode row slices directly in `accumulate_region`, read `first`
  directly at sample 0, and skip the full-frame `constant` pass when `constant`
  is identity.
- Deduplicate identical `(text, size, face)` rasterizations in `stage_atlas`.

Result (**Kept**):

- `confetti_window_ms`: **2,104.94 → 1,977.30 ms** (MAD 19.87 ms)
- `render_windows_ms`: **6,642.43 → 6,575.89 ms** (min 6,416.33 ms)
- 0 changed pixels across all 20 reference frames.

### Hypothesis 7: Tight rotated rectangle AABB and corner-only `hypot` in `rotated_rect` and `confetti_bounds`

`rotated_rect` and `confetti_bounds` used `half[0].hypot(half[1]) + 1.0` for
both axes instead of the rotated extents `[half_x * |cos| + half_y * |sin| + 1,
half_x * |sin| + half_y * |cos| + 1]`, and called `f32::hypot` (`hypotf`) on
every pixel inside that square even though `coverage == 0` whenever `d[0] >= 0.5
|| d[1] >= 0.5` and `distance == d[0].max(d[1])` whenever `d[0] <= 0 || d[1] <= 0`.

Change:

- Use tight rotated extents in `rotated_rect` and `confetti_bounds`, and call
  `hypot` only on the corner pixels where `0 < d[0] < 0.5 && 0 < d[1] < 0.5`.

Result (**Kept**):

- `render_windows_ms`: **6,575.89 → 5,884.69 ms** (min 5,625.42 ms, **4.68× speedup vs baseline**)
- `burst_window_ms`: **3,549.63 → 3,069.89 ms** (min 2,997.81 ms, **5.14× speedup vs baseline**)
- `confetti_window_ms`: **1,977.30 → 1,765.04 ms** (min 1,699.95 ms, **5.56× speedup vs baseline**)
- `quiet_window_ms`: **1,518.42 → 938.80 ms** (**1.62× speedup vs baseline**)
- `snapshot_shutter_ms`: **4,729.29 → 418.40 ms** (MAD 2.52 ms, **11.30× speedup vs baseline**)
- `--range 3..6` (180 frames, 24 shutter samples): **37.9 s → 12.5 s** (**3.0× speedup**)
- 0 changed pixels across all 20 reference frames.

## Dead Ends

- **H3 (`Scene::camera` per-sample element `HashMap` + `CompiledPlan` actor/property `HashMap`)**:
  Allocating a per-sample `HashMap<&str, &StageElement>` in `Scene::camera` (and
  cloning it in `Scene::tracked`) was inside the noise floor (`11,961.92 ms` vs
  `12,358.06 ms` under background load) because Stage rendering is GPU-bound and
  `plan.elements` has at most 128 elements. Discarded.
- **Tightening `Prim` `bbox` of `combustion_volume` (`case 6u`) on the CPU**:
  Changing `prim.bbox` alters hardware vertex interpolation of `in.px` across the
  quad by 1 ULP, causing `maxDelta: 1` on 389 pixels at `t = 4.3s`. Discarded in
  favor of early compact-support culling inside `combustion.wgsl`, which is
  bit-identical (`changedPixels: 0`).
- **Skipping `Stage` channels in `visual_sample_key_using` (`H8`)**:
  Inside the noise floor on GPU-bound Stage windows; kept only the single-sample
  overlay fast path (`if let &[(time, _)] = overlays.as_slice()`) which avoids an
  8.3 MB `base.clone()` when overlays do not move across the shutter or in `--draft` mode.

## Workflow Improvements

- `psychopomp plan render ... --draft`: explicit opt-in single-sample preview
  mode for fast audio/choreography checks on a segment or range (`--range 3..6
  --draft` renders 180 frames with audio in 4.0 s) while default `plan render`
  retains full 24-sample shutter exposure.
- `scripts/sheet.ts`: builds `-p psychopomp-render` rather than the entire
  workspace in `--release` mode when generating a contact sheet.
