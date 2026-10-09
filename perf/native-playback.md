# Native playback frame pacing

## Target and benchmark

Target: a steady 60 Hz native presentation, with the median of nine per-round p95
frame-submission intervals below 18 ms. These are CPU submission times, not
hardware display scanout timestamps; visual inspection still matters.

```bash
cargo build --release -p psychopomp-render
target/release/psychopomp plan present target/effect-succeed-slides.json --benchmark \
  > output/native-perf/run.json 2> output/native-perf/run.log
```

The native window stays on top for ten seconds: one warmup second and nine measured
one-second rounds. Every round repeats Next, Last, Previous, First at 250 ms
intervals, exercising interruption and several simultaneous inline reveals.
Resolution, window size, renderer profile, source, and build profile must stay the
same. Incomplete/occluded rounds are rejected rather than counted as zero latency.

## Baseline, Apple M2 Max

- Authored canvas: 1920×1080; physical window: 2560×1440; release build.
- Fast flat-editor preview, one temporal sample, smooth display scaling.
- Two complete runs: **45.15 ms** and **43.53 ms** median-round p95 submission interval.
- Roughly 22–24 frames submitted per second.
- Typical scene sample: 0.8 ms; CPU scaling: 31–32 ms; softbuffer presentation: 9–10 ms.
- Raw results: `output/native-perf/baseline-3.json`, `baseline-4.json`.
- Earlier occluded runs were invalid and were not used as a baseline.

## Hypothesis 1

The CPU scales millions of display pixels and then converts/copies them into the
window. A wgpu surface can upload the existing scene RGBA once and let a textured
triangle do smooth or nearest-neighbor scaling on the GPU. This changes only final
window presentation, not code layout, animation, interruption, or video export.

**Kept.** Two complete GPU-presentation runs at the same resolution and workload:

- Median-round p95: **17.88 ms**, **17.89 ms** (from 43.53–45.15 ms).
- 59–61 submitted frames per second (from 22–24).
- Typical upload/preparation: about 1.1 ms (from 31–32 ms of CPU scaling).
- Raw results: `output/native-perf/gpu-1.json`, `gpu-2.json`.

The native window now uses a FIFO wgpu surface with a one-frame latency request.
An sRGB texture and either a linear or nearest sampler handle smooth/pixelated
scaling on the GPU. Only changed source buffers are uploaded; resizing and filter
changes reuse the texture. The obsolete CPU blitter and softbuffer dependency
were removed. No further tuning was justified after this met the 18 ms target.

## Earlier prerequisite

The full-quality editor compositor takes about 116–138 ms per 1080p sample. A
separate explicit live-preview profile caches stationary chrome and skips final
optical glyph resampling, reducing the warmed code-only sample to 0.4–0.5 ms.
This is a **quality-profile change**, not a claim of pixel-equivalent optimization.
Full-quality export stays unchanged, and unsupported preview poses/effects fall
back to that renderer. Pixel equality across interrupted and out-of-order samples
is tested independently within each supported profile.

> Note (2026-10-07): the 116–138 ms figure predates threading the full-quality
> compositor. It now rasterises card and text rows across threads and skips
> rebuilding an unchanged flat editor frame, with pixels unchanged; re-measure
> before quoting a per-sample cost.

## 120 Hz investigation

Target: 120 distinct frames/sec, with an 8.33 ms frame budget. Keep the same
1920×1080 preview, 2560×1440 window, interruption workload, and release profile.

Both connected Studio Displays report **60.0 Hz** through CoreGraphics. The
player also has a fixed 60 Hz sampling timer. Increasing that timer to 120 Hz
while retaining FIFO tests whether the current display, rather than scene cost,
is the next limit. It cannot prove 120 Hz scanout on this hardware.

Fresh baseline (`120-baseline-1.json`, `120-baseline-2.json`):

- Exactly 60 submissions in each measured second.
- Median-round p95 intervals: 17.81 / 17.13 ms.
- Scene rendering p50: 0.84 / 0.80 ms; p95: 1.20 / 1.17 ms.
- Upload p50: 1.12 / 1.04 ms.
- Frame-age p95: 8.20 / 15.25 ms, depending on phase against display refresh.

The next experiment changes only the sampling cap, not rendering quality,
surface synchronization, frame latency, or animation tracks.

### Experiment: raise sampling to 120 Hz, retain FIFO

`120-fifo-1.json` / `120-fifo-2.json` still have **60 frames in every round**.
Median-round p95 intervals: 16.97 / 17.04 ms. Presentation now spends about
14.5 ms per frame waiting/submitting; frame-age p95 is about 17.1 ms, worse than
the baseline's phase-dependent 8.2–15.3 ms. This is the display limit, not a
rendering throughput limit.

**Discarded as a default for 60 Hz screens.** Instead, sampling follows the current
monitor's millihertz refresh rate, with 60 Hz fallback and an explicit `--fps`
override. No unsynchronized/tearing mode or extra frame queue was added.
The high-refresh scheduling interval is unit-tested; 120 Hz scanout remains
unverified because no connected display supports it.

### Diagnostic: measure completed work, not just submission

```bash
target/release/psychopomp plan present target/effect-succeed-slides.json \
  --fps 120 --benchmark-gpu > output/native-perf/completed.json
```

This separate benchmark mode explicitly waits for the display GPU submission to
complete before presenting. `completed_work_ms` sums worker scene-sampling time
and upload/command-encoding/GPU-completion time. It excludes acquisition of the
next drawable, event-loop dispatch, and scanout. Upload is already included in
the completed display work and must not be counted twice.

- `120-completed-1.json` / `120-completed-2.json`: completed work's median-round
  p95 **5.12 / 4.61 ms**; per-round medians roughly 2.8–4.0 ms.
- Per-round p95 spread: 4.00–5.75 / 3.53–6.51 ms.
- Typical drawable-acquisition wait: 13–14 ms.
- This is below the 8.33 ms rendering budget, but measured with a 60 Hz display
  supplying drawables. It does not prove sustained 120 fps, thermal behavior,
  or performance for the full-quality compositor or more complex scenes.
- The diagnostic wait perturbs pacing (18.42 / 18.00 ms median-round p95), so do
  not compare its frame intervals with the normal asynchronous benchmark.
- Ordinary playback and `--benchmark` do not add a GPU-completion wait.

**Stop point:** this scene has rendering headroom, while a 120+ Hz display is
required for the next user-visible FPS test. More copies, caching, or shader
changes cannot make the connected 60 Hz panels scan out at 120 Hz. Keep the same
visual quality and interruption model rather than add complexity for an inflated
submission counter.

### Final checks

With automatic monitor pacing, `auto-refresh-3.json` selected 60 Hz, submitted
58–60 frames per measured second, and recorded a 17.84 ms median-round p95 interval:
within the earlier 60 Hz baseline range, not a claimed speedup. GPU-completion
measurement was disabled. A second final run became occluded and was rejected;
its partial samples were not used. Initial surface unavailability during warmup
is allowed, but occlusion or refresh changes during measurement invalidate a run.

Workspace tests, formatting, strict Clippy, and both ignored GPU tests passed.
The full-quality 10.5-second endpoint PNG remained byte-identical to the earlier
highlighting artifact. Native screenshot/accessibility automation was unavailable
during final verification; actual 120 Hz presentation remains a hardware-level
verification gap, not a result inferred from the work budget.

## Stability and multi-slide follow-up

The renderer now preserves fractional text positions, filtered reveal edges and
blur, and Task icon/result transforms. These are quality/correctness changes, not
pixel-equivalent speed optimizations. The narrow 24-sample editor test rose from
about 0.4 ms to about 1.2 ms before later edge fixes; native interruption workloads
are heavier and must not be equated with that microbenchmark.

The four-slide `interactive-showcase` adds Task lifecycle/retry, parallel Tasks,
and keyed code edits with attached highlights. A running Task advances an ambient
clock even after scalar destinations settle. Its bounded smooth noise has constant
sampling work rather than replaying all prior jitter intervals.

### Fixed cadence instead of accumulating timer lateness

The player previously scheduled each request relative to the last actual request.
Late OS wakes accumulated into roughly 56 submissions/sec with the richer render
path. It now advances absolute deadlines, skips missed slots, and preserves the
cadence when input requests an immediate frame. No stale-frame queue or busy loop
was added.

Matched release / 1920×1080 canvas / 2560×1440 window / single-sample runs:

| Workload | Before p95 interval | After median-round p95 | Submitted frames/sec after |
| --- | --- | --- | --- |
| Code reveal | 19.00 ms | 16.95 / 16.92 ms | 60 in every round |
| Parallel Tasks | 22.01 ms | 18.15 / 17.17 ms | 59–60 |
| Keyed edits + highlight | 19.34 ms | 16.96 ms | 59–60 |

Raw files: `output/native-perf/final-*.json` and `cadence-*.json`. Correctness fixes
to filtered bounds and Task sampling also landed in this interval; only the
deadline mechanism is attributed to scheduling, not every difference in CPU work.
Parallel Task rendering is around 10 ms/sample in this stress workload, so the
earlier code-only 120 Hz headroom result does **not** establish 120 fps for it.

### Avoid cold editor work on slide switches

Static editor chrome is independent of dynamic focus/highlight overlays. The
preview keeps two chrome images (about 16 MiB total at 1080p) keyed by filename,
and warms each slide's initial glyph/chrome resources before opening the window.
This trades some startup work for smooth switching without an unbounded cache.

Deck benchmarks exercise all four slides and return to the first. Before warming,
editor-switch rounds dropped to 53–54 submissions/sec. `warm-deck-1.json` recorded
60 frames in every measured round, p95 17.01 ms. `warm-deck-2.json` recorded p95
16.92 ms, with every editor-switch round at 60; one Task round fell to 50 with
33–39 ms render-time outliers. That variability is retained, not presented as a
perfect 60 fps guarantee. No hardware scanout timestamps were collected.

## Expressive Task content

The content-motion pass adds scale/blur/opacity poses, symbol rotation, staged
running energy, and assembled error-bubble transitions. It also increases the
transformed-sprite filter from 3×3 to 5×5 taps to reduce visibly displaced text
copies under defocus. These are deliberate pixel and choreography changes, not
an equivalent-work performance optimization.

Matched parallel-Task interruption benchmarks remain release / 1920×1080 authored
canvas / 2560×1440 window / one temporal sample. The pre-change run recorded 60
submissions in every round and 17.64 ms median-round p95. The final run also
recorded 60 in every round, with 21.62 ms median-round p95 and per-round render
medians of 8.75–9.17 ms. The higher p95 is retained; this does not establish
uniform frame pacing or 120 Hz capability. Raw results and intermediate runs are
under `output/task-content-motion/`. Reduced drawing during staged content
handoffs changes the workload, so lower render time is not a renderer speedup claim.

### Source-timing correction

Kit judged that first content pass stale compared with Effect Institute. It is
superseded by the source's operation-specific tracks: 200 ms/bounce-0.5 height,
approximately 167 ms icons, 250 ms/bounce-0.4 result pop with 150 ms deblur, and
independent bubble rise/fade/blur. The added visibility waits were removed.
`PRIOR_ART.md` records the exact source paths, and a fixture from the source's
pinned Motion DOM 12.42.2 generator checks actual compiled Rust samples.

The matched parallel-Task interruption run in `output/task-timing/benchmark.json`
recorded 57–59 submissions/sec, median-round p95 22.29 ms, and per-round render
medians 10.25–10.95 ms. Source timing fidelity is not a frame-pacing improvement;
the busier overlap is retained, and native throughput remains a separate concern.
