//! Headless audio mix analysis: ITU-R BS.1770-4 / EBU R128 loudness (LUFS),
//! 4x polyphase true peak (dBTP), clipping, silent stretches, per-segment
//! loudness, and active-clip timeline queries (`plan audio --at`).

use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use psychopomp::{
    composition::{MediaPlacement, MediaRole, Time, TimeRange},
    time_spec::{SectionKind, TimeIndex},
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::encode::audio_filter_graph;

const MIX_SAMPLE_RATE: usize = 48_000;

#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// ITU-R BS.1770-4 analog-prototype K-weighting stages (pre-filter shelf + RLB high-pass).
fn k_weighting(sample_rate: f64) -> [Biquad; 2] {
    let shelf = {
        let f0 = 1_681.974_450_955_533;
        let g = 3.999_843_853_973_347;
        let q = 0.707_175_236_955_419_6;
        let k = (std::f64::consts::PI * f0 / sample_rate).tan();
        let vh = 10_f64.powf(g / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [
                (vh + vb * k / q + k * k) / a0,
                2.0 * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            z: [0.0; 2],
        }
    };
    let high_pass = {
        let f0 = 38.135_470_876_024_44;
        let q = 0.500_327_037_323_877_3;
        let k = (std::f64::consts::PI * f0 / sample_rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [1.0 / a0, -2.0 / a0, 1.0 / a0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            z: [0.0; 2],
        }
    };
    [shelf, high_pass]
}

/// BS.1770-4 Annex 2: 4x oversampling polyphase FIR (4 phases of 12 taps).
const TRUE_PEAK_PHASES: [[f64; 12]; 4] = [
    [
        0.001_708_984_375_0,
        0.010_986_328_125_0,
        -0.019_653_320_312_5,
        0.033_203_125_000_0,
        -0.059_448_242_187_5,
        0.137_329_101_562_5,
        0.972_167_968_750_0,
        -0.102_294_921_875_0,
        0.047_607_421_875_0,
        -0.026_611_328_125_0,
        0.014_892_578_125_0,
        -0.008_300_781_250_0,
    ],
    [
        -0.029_174_804_687_5,
        0.029_296_875_000_0,
        -0.051_757_812_500_0,
        0.089_111_328_125_0,
        -0.166_503_906_250_0,
        0.465_087_890_625_0,
        0.779_785_156_250_0,
        -0.200_317_382_812_5,
        0.101_562_500_000_0,
        -0.058_227_539_062_5,
        0.033_081_054_687_5,
        -0.018_920_898_437_5,
    ],
    [
        -0.018_920_898_437_5,
        0.033_081_054_687_5,
        -0.058_227_539_062_5,
        0.101_562_500_000_0,
        -0.200_317_382_812_5,
        0.779_785_156_250_0,
        0.465_087_890_625_0,
        -0.166_503_906_250_0,
        0.089_111_328_125_0,
        -0.051_757_812_500_0,
        0.029_296_875_000_0,
        -0.029_174_804_687_5,
    ],
    [
        -0.008_300_781_250_0,
        0.014_892_578_125_0,
        -0.026_611_328_125_0,
        0.047_607_421_875_0,
        -0.102_294_921_875_0,
        0.972_167_968_750_0,
        0.137_329_101_562_5,
        -0.059_448_242_187_5,
        0.033_203_125_000_0,
        -0.019_653_320_312_5,
        0.010_986_328_125_0,
        0.001_708_984_375_0,
    ],
];

fn true_peak_linear(channel: &[f32]) -> f64 {
    let mut peak: f64 = channel.iter().fold(0.0, |m, s| m.max(f64::from(s.abs())));
    for n in 0..channel.len() {
        for phase in &TRUE_PEAK_PHASES {
            let mut acc = 0.0;
            for (k, &coeff) in phase.iter().enumerate() {
                if n >= k {
                    acc += coeff * f64::from(channel[n - k]);
                }
            }
            peak = peak.max(acc.abs());
        }
    }
    peak
}

fn round2(val: f64) -> Option<f64> {
    val.is_finite().then_some((val * 100.0).round() / 100.0)
}

fn energy_to_lufs(mean_sq: f64) -> f64 {
    if mean_sq > 0.0 {
        -0.691 + 10.0 * mean_sq.log10()
    } else {
        f64::NEG_INFINITY
    }
}

fn linear_to_db(val: f64) -> f64 {
    if val > 0.0 {
        20.0 * val.log10()
    } else {
        f64::NEG_INFINITY
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SilentSpan {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SegmentLoudness {
    pub id: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AudioReport {
    pub track_count: usize,
    pub missing_files: Vec<String>,
    pub duration_seconds: f64,
    pub integrated_lufs: Option<f64>,
    pub short_term_max_lufs: Option<f64>,
    pub momentary_max_lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    pub clipped_samples: usize,
    pub silent_ranges: Vec<SilentSpan>,
    pub segments: Vec<SegmentLoudness>,
}

/// Pure-Rust BS.1770 analysis on de-interleaved stereo `left` / `right` buffers.
pub(crate) fn analyze_stereo_samples(
    left: &[f32],
    right: &[f32],
    sample_rate: usize,
    window_start_sec: f64,
    segments: &[(String, f64, f64)],
) -> AudioReport {
    let n = left.len().min(right.len());
    let hop = (sample_rate / 10).max(1);
    let mut hops = vec![0.0_f64; n / hop];
    for ch in [&left[..n], &right[..n]] {
        let [mut shelf, mut hp] = k_weighting(sample_rate as f64);
        for (i, &s) in ch.iter().enumerate().take(hops.len() * hop) {
            let y = hp.process(shelf.process(f64::from(s)));
            hops[i / hop] += y * y;
        }
    }

    let windows = |range: std::ops::Range<usize>, win_hops: usize| -> Vec<f64> {
        if range.len() < win_hops {
            return Vec::new();
        }
        (range.start..=range.end - win_hops)
            .map(|i| hops[i..i + win_hops].iter().sum::<f64>() / (win_hops * hop) as f64)
            .collect()
    };

    let integrated_for = |sample_range: std::ops::Range<usize>| -> f64 {
        let hr =
            (sample_range.start / hop).min(hops.len())..(sample_range.end / hop).min(hops.len());
        let blocks = windows(hr, 4);
        let abs_gate = 10_f64.powf((-70.0 + 0.691) / 10.0);
        let gated: Vec<f64> = blocks.into_iter().filter(|&z| z >= abs_gate).collect();
        if gated.is_empty() {
            return f64::NEG_INFINITY;
        }
        let rel_gate = gated.iter().sum::<f64>() / gated.len() as f64 * 0.1;
        let kept: Vec<f64> = gated.into_iter().filter(|&z| z >= rel_gate).collect();
        if kept.is_empty() {
            f64::NEG_INFINITY
        } else {
            energy_to_lufs(kept.iter().sum::<f64>() / kept.len() as f64)
        }
    };

    let integrated = integrated_for(0..n);
    let momentary_max = windows(0..hops.len(), 4)
        .into_iter()
        .map(energy_to_lufs)
        .fold(f64::NEG_INFINITY, f64::max);
    let short_term_max = windows(0..hops.len(), 30)
        .into_iter()
        .map(energy_to_lufs)
        .fold(f64::NEG_INFINITY, f64::max);

    let tp = true_peak_linear(&left[..n]).max(true_peak_linear(&right[..n]));
    let clipped_samples = left[..n]
        .iter()
        .chain(&right[..n])
        .filter(|&&s| s.abs() >= 0.999_9)
        .count();

    // Detect silent stretches >= 0.6s (6 hops of 100ms below -60 dBFS RMS).
    let silence_thresh = 10_f64.powf(-60.0 / 10.0) * 2.0; // stereo sum of squares
    let mut silent_ranges = Vec::new();
    let mut run_start: Option<usize> = None;
    for h in 0..hops.len() {
        let start_s = h * hop;
        let end_s = (h + 1) * hop;
        let mut sq = 0.0_f64;
        for i in start_s..end_s {
            sq +=
                f64::from(left[i]) * f64::from(left[i]) + f64::from(right[i]) * f64::from(right[i]);
        }
        let mean_sq = sq / hop as f64;
        if mean_sq < silence_thresh {
            run_start.get_or_insert(h);
        } else if let Some(s0) = run_start.take()
            && h - s0 >= 6
        {
            let a = window_start_sec + s0 as f64 * 0.1;
            let b = window_start_sec + h as f64 * 0.1;
            silent_ranges.push(SilentSpan {
                start_seconds: round2(a).unwrap_or(a),
                end_seconds: round2(b).unwrap_or(b),
                duration_seconds: round2(b - a).unwrap_or(b - a),
            });
        }
    }
    if let Some(s0) = run_start
        && hops.len() - s0 >= 6
    {
        let a = window_start_sec + s0 as f64 * 0.1;
        let b = window_start_sec + hops.len() as f64 * 0.1;
        silent_ranges.push(SilentSpan {
            start_seconds: round2(a).unwrap_or(a),
            end_seconds: round2(b).unwrap_or(b),
            duration_seconds: round2(b - a).unwrap_or(b - a),
        });
    }

    let seg_reports = segments
        .iter()
        .filter_map(|(id, seg_start, seg_end)| {
            let rel_a = (seg_start - window_start_sec).max(0.0);
            let rel_b = (seg_end - window_start_sec).max(0.0);
            if rel_b <= rel_a {
                return None;
            }
            let sa = ((rel_a * sample_rate as f64).round() as usize).min(n);
            let sb = ((rel_b * sample_rate as f64).round() as usize).min(n);
            if sb <= sa {
                return None;
            }
            let seg_lufs = integrated_for(sa..sb);
            let seg_tp = true_peak_linear(&left[sa..sb]).max(true_peak_linear(&right[sa..sb]));
            Some(SegmentLoudness {
                id: id.clone(),
                start_seconds: round2(*seg_start).unwrap_or(*seg_start),
                end_seconds: round2(*seg_end).unwrap_or(*seg_end),
                integrated_lufs: round2(seg_lufs),
                true_peak_dbtp: round2(linear_to_db(seg_tp)),
            })
        })
        .collect();

    AudioReport {
        track_count: 0,
        missing_files: Vec::new(),
        duration_seconds: round2(n as f64 / sample_rate as f64).unwrap_or(0.0),
        integrated_lufs: round2(integrated),
        short_term_max_lufs: round2(short_term_max),
        momentary_max_lufs: round2(momentary_max),
        true_peak_dbtp: round2(linear_to_db(tp)),
        clipped_samples,
        silent_ranges,
        segments: seg_reports,
    }
}

pub(crate) fn analyze_placements(
    media: &[MediaPlacement],
    index: &TimeIndex,
    window: TimeRange,
) -> Result<AudioReport> {
    let active: Vec<MediaPlacement> = media.iter().filter_map(|p| p.for_window(window)).collect();
    let duration_sec = window.duration().as_seconds();
    let mut missing = Vec::new();
    for p in &active {
        let path = p.clip().asset().path();
        if !path.exists() {
            let display = path.to_string_lossy().into_owned();
            if !missing.contains(&display) {
                missing.push(display);
            }
        }
    }
    if active.is_empty() || !missing.is_empty() {
        return Ok(AudioReport {
            track_count: active.len(),
            missing_files: missing,
            duration_seconds: round2(duration_sec).unwrap_or(duration_sec),
            integrated_lufs: None,
            short_term_max_lufs: None,
            momentary_max_lufs: None,
            true_peak_dbtp: None,
            clipped_samples: 0,
            silent_ranges: Vec::new(),
            segments: Vec::new(),
        });
    }

    let mut args = vec!["-v".to_owned(), "error".to_owned()];
    for p in &active {
        args.push("-i".to_owned());
        args.push(p.clip().asset().path().to_string_lossy().into_owned());
    }
    args.extend([
        "-filter_complex".to_owned(),
        format!(
            "{};[aout]apad=whole_dur={duration_sec:.6},atrim=0:{duration_sec:.6}[afinal]",
            audio_filter_graph(&active, 0)
        ),
        "-map".to_owned(),
        "[afinal]".to_owned(),
        "-ar".to_owned(),
        MIX_SAMPLE_RATE.to_string(),
        "-ac".to_owned(),
        "2".to_owned(),
        "-f".to_owned(),
        "f32le".to_owned(),
        "-".to_owned(),
    ]);

    let out = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .context("run ffmpeg for audio analysis")?;
    if !out.status.success() {
        bail!(
            "ffmpeg audio mix failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let sample_pairs = out.stdout.len() / 8;
    let mut left = Vec::with_capacity(sample_pairs);
    let mut right = Vec::with_capacity(sample_pairs);
    for frame in out.stdout.chunks_exact(8) {
        left.push(f32::from_le_bytes(frame[0..4].try_into().unwrap()));
        right.push(f32::from_le_bytes(frame[4..8].try_into().unwrap()));
    }

    let seg_spans: Vec<(String, f64, f64)> = index
        .sections()
        .iter()
        .filter(|s| s.kind == SectionKind::Segment)
        .map(|s| {
            (
                s.id.clone(),
                s.start_nanos as f64 / 1e9,
                s.end_nanos as f64 / 1e9,
            )
        })
        .collect();

    let mut report = analyze_stereo_samples(
        &left,
        &right,
        MIX_SAMPLE_RATE,
        window.start().as_seconds(),
        &seg_spans,
    );
    report.track_count = active.len();
    Ok(report)
}

/// Pure metadata query: which audio placements are playing at each timestamp.
pub(crate) fn active_at(media: &[MediaPlacement], index: &TimeIndex, times: &[f64]) -> Value {
    let queries: Vec<Value> = times
        .iter()
        .map(|&sec| {
            let at = Time::try_seconds(sec).unwrap_or(Time::ZERO);
            let tracks: Vec<Value> = media
                .iter()
                .filter(|p| {
                    let r = p.timeline_range();
                    at >= r.start() && at < r.end()
                })
                .map(|p| {
                    let offset = (at
                        .as_nanos()
                        .saturating_sub(p.timeline_range().start().as_nanos())
                        + p.clip().source_range().start().as_nanos())
                        as f64
                        / 1e9;
                    let path = p.clip().asset().path();
                    json!({
                        "id": p.clip().asset().id().as_str(),
                        "role": match p.role() {
                            MediaRole::Script => "script",
                            MediaRole::Layer => "layer",
                        },
                        "gainDb": p.clip().audio_gain_db(),
                        "sourceSeconds": round2(offset),
                        "timelineStart": round2(p.timeline_range().start().as_seconds()),
                        "timelineEnd": round2(p.timeline_range().end().as_seconds()),
                        "exists": path.exists(),
                    })
                })
                .collect();
            json!({
                "time": round2(sec),
                "label": index.format_label(sec),
                "activeCount": tracks.len(),
                "tracks": tracks,
            })
        })
        .collect();
    json!({ "queries": queries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_full_scale_997hz_stereo_sine_near_zero_lufs_and_zero_dbtp() {
        let sr = 48_000;
        let len = sr * 2;
        let sine: Vec<f32> = (0..len)
            .map(|n| (2.0 * std::f64::consts::PI * 997.0 * n as f64 / sr as f64).sin() as f32)
            .collect();
        let report = analyze_stereo_samples(&sine, &sine, sr, 0.0, &[("tone".into(), 0.0, 2.0)]);
        // By ITU-R BS.1770-4, a full-scale 997 Hz stereo sine measures ~0.0 LUFS (-0.691 + 10*log10(1.0) + shelf ~+0.7 = ~0.0) and ~0.0 dBTP.
        let lufs = report.integrated_lufs.unwrap();
        let dbtp = report.true_peak_dbtp.unwrap();
        assert!((lufs - 0.0).abs() < 0.25, "unexpected LUFS: {lufs}");
        assert!((dbtp - 0.0).abs() < 0.1, "unexpected dBTP: {dbtp}");
        assert!(report.silent_ranges.is_empty());
        assert_eq!(report.segments.len(), 1);
    }

    #[test]
    fn detects_leading_and_trailing_silence() {
        let sr = 48_000;
        let mut left = vec![0.0_f32; sr * 2];
        for s in &mut left[sr..sr + sr / 2] {
            *s = 0.25;
        }
        let report = analyze_stereo_samples(&left, &left, sr, 0.0, &[]);
        // 0.0..1.0s is silent (>= 0.6s), 1.5..2.0s is 0.5s (< 0.6s threshold).
        assert_eq!(report.silent_ranges.len(), 1);
        assert_eq!(report.silent_ranges[0].start_seconds, 0.0);
        assert_eq!(report.silent_ranges[0].end_seconds, 1.0);
    }
}
