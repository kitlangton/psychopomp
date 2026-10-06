//! Frame exposure: the output format, how many shutter samples a frame takes,
//! their weights across a 180-degree shutter, and encoding a timeline one
//! exposed frame at a time. Every root and the legacy scenes share it.
use std::{num::NonZeroU32, ops::Range, path::Path, time::Instant};

use anyhow::{Result, bail};
use psychopomp::{
    composition::{Duration, MediaPlacement, Time, TimeRange},
    math::{Vec2, shapes::Box2, vec2},
};

use crate::{
    encode::{FfmpegEncoder, VideoSpec},
    render::HeadlessRenderer,
};

pub(crate) const WIDTH: u32 = 1920;
pub(crate) const HEIGHT: u32 = 1080;
const FPS: u32 = 60;
const TEMPORAL_SAMPLES: u32 = 8;
const ENTRANCE_TEMPORAL_SAMPLES: u32 = 16;
const SHUTTER_ANGLE: f32 = 180.0;

/// Video export quality; omitted samples retain each recipe's native schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RenderOptions {
    pub fps: NonZeroU32,
    pub samples: Option<NonZeroU32>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            fps: NonZeroU32::new(FPS).unwrap(),
            samples: None,
        }
    }
}

/// How long a frame's shutter stays open.
pub(crate) const SHUTTER_SECONDS: f64 = SHUTTER_ANGLE as f64 / 360.0 / FPS as f64;
/// The fewest samples a reel frame takes while its segments mix or move.
pub(crate) const TRANSITION_TEMPORAL_SAMPLES: u32 = 16;

/// Samples per frame for Scene Plans: more in the first second, where
/// entrances move fastest.
pub(crate) fn plan_temporal_samples(center: f64) -> u32 {
    if center < 1.0 {
        ENTRANCE_TEMPORAL_SAMPLES
    } else {
        TEMPORAL_SAMPLES
    }
}

/// Encode a timeline one exposed frame at a time. `samples_at` chooses how
/// many shutter samples a frame centered at a time takes; samples with equal
/// `sample_key`s merge their weights; `render_exposure` turns one frame's
/// weighted samples into pixels.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_exposures<K: PartialEq>(
    renderer: &mut HeadlessRenderer,
    output: &Path,
    duration: Duration,
    media: &[MediaPlacement],
    window: TimeRange,
    options: RenderOptions,
    mut samples_at: impl FnMut(f64) -> u32,
    mut sample_key: impl FnMut(f64) -> Result<K>,
    mut render_exposure: impl FnMut(&mut HeadlessRenderer, &[(f64, f32)]) -> Result<Vec<u8>>,
) -> Result<()> {
    if window.duration() == psychopomp::composition::Duration::ZERO {
        bail!("render window must have positive duration");
    }
    let scene_end = Time::ZERO.after(duration);
    if window.end() > scene_end {
        bail!(
            "render window {}..{} exceeds scene duration {}",
            window.start(),
            window.end(),
            duration
        );
    }
    let started = Instant::now();
    let fps = options.fps.get();
    let frame_count = window.duration().frame_count(fps);
    let media = media
        .iter()
        .filter_map(|placement| placement.for_window(window))
        .collect::<Vec<_>>();
    let mut encoder = FfmpegEncoder::start_with_media(
        output,
        VideoSpec {
            width: WIDTH,
            height: HEIGHT,
            fps,
        },
        &media,
    )?;
    for frame in 0..frame_count {
        let frame_start = window.start().as_seconds() + frame as f64 / f64::from(fps);
        let frame_end = (window.start().as_seconds() + (frame + 1) as f64 / f64::from(fps))
            .min(window.end().as_seconds());
        let center = (frame_start + frame_end) * 0.5;
        let samples = options
            .samples
            .map_or_else(|| samples_at(center).max(1), NonZeroU32::get);
        let exposure = merge_equal_samples(
            exposure_at_fps(center, frame_end - frame_start, samples, fps),
            &mut sample_key,
        )?;
        encoder.write_frame(&render_exposure(renderer, &exposure)?)?;
        if frame % u64::from(fps) == 0 || frame + 1 == frame_count {
            eprintln!(
                "Rendered {:>3}/{frame_count} frames ({center:.1}s, {samples} samples, {} unique)",
                frame + 1,
                exposure.len(),
            );
        }
    }
    encoder.finish()?;
    eprintln!(
        "Wrote {} in {:.1}s",
        output.display(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

/// One frame's shutter: `samples` stratified times across a 180-degree
/// shutter centered on `center` (clamped to `span`), with weights summing
/// to 1. The weights ease off over the outer quarter at each end, so a fast
/// highlight's streak fades out instead of ending on a hard copy.
pub(crate) fn exposure(center: f64, span: f64, samples: u32) -> Vec<(f64, f32)> {
    exposure_at_fps(center, span, samples, FPS)
}

fn exposure_at_fps(center: f64, span: f64, samples: u32, fps: u32) -> Vec<(f64, f32)> {
    let shutter = (f64::from(SHUTTER_ANGLE) / 360.0 / f64::from(fps)).min(span);
    let (start, end) = (center - span * 0.5, center + span * 0.5);
    let mut weighted = (0..samples)
        .map(|sample| {
            let phase = (f64::from(sample) + 0.5) / f64::from(samples) - 0.5;
            let edge = ((0.5 - phase.abs()) / 0.25).clamp(0.0, 1.0);
            let weight = if samples < 4 {
                1.0
            } else {
                edge * edge * (3.0 - 2.0 * edge)
            };
            ((center + phase * shutter).clamp(start, end), weight as f32)
        })
        .collect::<Vec<_>>();
    let total: f32 = weighted.iter().map(|(_, weight)| weight).sum();
    for (_, weight) in &mut weighted {
        *weight /= total;
    }
    weighted
}

/// Merge samples whose visual state is identical, keeping the first time and
/// the summed weight, so a still frame renders once.
pub(crate) fn merge_equal_samples<K: PartialEq>(
    samples: impl IntoIterator<Item = (f64, f32)>,
    mut sample_key: impl FnMut(f64) -> Result<K>,
) -> Result<Vec<(f64, f32)>> {
    let mut merged: Vec<(K, f64, f32)> = Vec::new();
    for (time, weight) in samples {
        let key = sample_key(time)?;
        match merged.iter_mut().find(|(candidate, ..)| candidate == &key) {
            Some((_, _, total)) => *total += weight,
            None => merged.push((key, time, weight)),
        }
    }
    Ok(merged
        .into_iter()
        .map(|(_, time, weight)| (time, weight))
        .collect())
}

/// Average sRGB frames in linear light by weight: the CPU exposure every
/// root supports.
pub(crate) fn accumulate(
    renderer: &mut HeadlessRenderer,
    exposure: &[(f64, f32)],
    mut render_sample: impl FnMut(&mut HeadlessRenderer, f64) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    if let [(time, _)] = exposure {
        return render_sample(renderer, *time);
    }
    let tables = linear_tables();
    let mut sum = vec![0.0_f32; FRAME_BYTES];
    for &(time, weight) in exposure {
        let pixels = render_sample(renderer, time)?;
        check_frame(&pixels)?;
        let weighted = WeightedLinear::new(tables, weight);
        for (sum, pixel) in sum
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels.as_chunks::<4>().0)
        {
            weighted.add(sum, pixel);
        }
    }
    Ok(encode_frame(tables, &sum))
}

/// `accumulate` for samples that differ only inside `region`. `first` is the
/// first sample's frame; `render_sample` repaints each later sample into a
/// frame that holds an earlier one, and only its `region` is read back.
/// Outside `region` every pixel takes the same weighted average through a
/// per-value table, so the result is bit-identical to `accumulate`.
pub(crate) fn accumulate_region(
    exposure: &[(f64, f32)],
    region: &Region,
    first: Vec<u8>,
    mut render_sample: impl FnMut(&mut [u8], f64) -> Result<()>,
) -> Result<Vec<u8>> {
    check_frame(&first)?;
    if exposure.len() == 1 {
        return Ok(first);
    }
    let tables = linear_tables();
    let mut frame = first.clone();
    let mut sum = vec![0.0_f32; region.pixel_count() * 4];
    for (index, &(time, weight)) in exposure.iter().enumerate() {
        if index > 0 {
            render_sample(&mut frame, time)?;
        }
        let weighted = WeightedLinear::new(tables, weight);
        let pixels = region.rows().flat_map(|row| frame[row].as_chunks::<4>().0);
        for (sum, pixel) in sum.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            weighted.add(sum, pixel);
        }
    }
    let constant: [[u8; 4]; 256] = std::array::from_fn(|value| {
        let mut sum = [0.0_f32; 4];
        for &(_, weight) in exposure {
            add_linear(tables, &mut sum, &[value as u8; 4], weight);
        }
        encode_linear(tables, &sum)
    });
    let mut exposed = first;
    for pixel in exposed.as_chunks_mut::<4>().0 {
        for (channel, value) in pixel.iter_mut().enumerate() {
            *value = constant[*value as usize][channel];
        }
    }
    let mut sums = sum.as_chunks::<4>().0.iter();
    for row in region.rows() {
        for (pixel, sum) in exposed[row]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&mut sums)
        {
            *pixel = encode_linear(tables, sum);
        }
    }
    Ok(exposed)
}

/// The frame pixels some set of boxes touches, as disjoint row spans.
pub(crate) struct Region {
    rows: Vec<Range<usize>>,
}

impl Region {
    /// The frame pixels any of `bounds` touches; empty for none.
    pub(crate) fn covering(bounds: impl IntoIterator<Item = Box2>) -> Self {
        let frame = vec2(WIDTH as f32, HEIGHT as f32);
        let boxes = bounds
            .into_iter()
            .map(|bounds| {
                let min = bounds.min.floor().clamp(Vec2::ZERO, frame);
                let max = bounds.max.ceil().clamp(min, frame);
                (
                    min.x as usize..max.x as usize,
                    min.y as usize..max.y as usize,
                )
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        for y in 0..HEIGHT as usize {
            let mut spans = boxes
                .iter()
                .filter(|(x, rows)| rows.contains(&y) && !x.is_empty())
                .map(|(x, _)| x.clone())
                .collect::<Vec<_>>();
            spans.sort_by_key(|span| span.start);
            let row = y * WIDTH as usize;
            let mut spans = spans.into_iter();
            let Some(mut merged) = spans.next() else {
                continue;
            };
            for span in spans {
                if span.start <= merged.end {
                    merged.end = merged.end.max(span.end);
                } else {
                    rows.push((row + merged.start) * 4..(row + merged.end) * 4);
                    merged = span;
                }
            }
            rows.push((row + merged.start) * 4..(row + merged.end) * 4);
        }
        Self { rows }
    }

    fn pixel_count(&self) -> usize {
        self.rows.iter().map(|row| row.len() / 4).sum()
    }

    /// Each span's byte range in an RGBA frame.
    fn rows(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.rows.iter().cloned()
    }

    /// Copy this region of `from` into `to`.
    pub(crate) fn copy(&self, from: &[u8], to: &mut [u8]) {
        for row in self.rows() {
            to[row.clone()].copy_from_slice(&from[row]);
        }
    }
}

const FRAME_BYTES: usize = WIDTH as usize * HEIGHT as usize * 4;

fn check_frame(pixels: &[u8]) -> Result<()> {
    if pixels.len() != FRAME_BYTES {
        bail!(
            "renderer returned {} bytes for a {FRAME_BYTES}-byte RGBA frame",
            pixels.len()
        );
    }
    Ok(())
}

/// Pre-multiplied per-byte lookup table for one shutter sample's `weight`.
/// Replaces 3 float multiplications and 1 int-to-float + division + multiply
/// per pixel with 4 table lookups and 4 additions, while preserving exact
/// IEEE-754 `f32` bit identity with [`add_linear`].
struct WeightedLinear {
    rgb: [f32; 256],
    alpha: [f32; 256],
}

impl WeightedLinear {
    #[inline]
    fn new(tables: &LinearTables, weight: f32) -> Self {
        Self {
            rgb: std::array::from_fn(|v| tables.to_linear[v] * weight),
            alpha: std::array::from_fn(|v| f32::from(v as u8) / 255.0 * weight),
        }
    }

    #[inline]
    fn add(&self, sum: &mut [f32; 4], pixel: &[u8; 4]) {
        sum[0] += self.rgb[pixel[0] as usize];
        sum[1] += self.rgb[pixel[1] as usize];
        sum[2] += self.rgb[pixel[2] as usize];
        sum[3] += self.alpha[pixel[3] as usize];
    }
}

#[inline]
fn add_linear(tables: &LinearTables, sum: &mut [f32; 4], pixel: &[u8; 4], weight: f32) {
    for channel in 0..3 {
        sum[channel] += tables.to_linear[pixel[channel] as usize] * weight;
    }
    sum[3] += f32::from(pixel[3]) / 255.0 * weight;
}

#[inline]
fn encode_linear(tables: &LinearTables, sum: &[f32; 4]) -> [u8; 4] {
    let encode = |linear: f32| tables.to_srgb[(linear.clamp(0.0, 1.0) * 65535.0).round() as usize];
    [
        encode(sum[0]),
        encode(sum[1]),
        encode(sum[2]),
        (sum[3] * 255.0).round() as u8,
    ]
}

fn encode_frame(tables: &LinearTables, sum: &[f32]) -> Vec<u8> {
    let mut exposed = vec![0_u8; sum.len() / 4 * 4];
    for (out, sum) in exposed
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(sum.as_chunks::<4>().0)
    {
        *out = encode_linear(tables, sum);
    }
    exposed
}

pub(crate) struct LinearTables {
    pub(crate) to_linear: [f32; 256],
    to_srgb: Vec<u8>,
}

impl LinearTables {
    /// One linear-light value as an sRGB byte.
    pub(crate) fn encode(&self, linear: f32) -> u8 {
        self.to_srgb[(linear.clamp(0.0, 1.0) * 65535.0).round() as usize]
    }
}

pub(crate) fn linear_tables() -> &'static LinearTables {
    static TABLES: std::sync::LazyLock<LinearTables> = std::sync::LazyLock::new(|| LinearTables {
        to_linear: std::array::from_fn(|value| {
            let encoded = value as f32 / 255.0;
            if encoded <= 0.04045 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            }
        }),
        to_srgb: (0..65536)
            .map(|value| {
                let linear = value as f32 / 65535.0;
                let encoded = if linear <= 0.003_130_8 {
                    linear * 12.92
                } else {
                    1.055 * linear.powf(1.0 / 2.4) - 0.055
                };
                (encoded * 255.0).round() as u8
            })
            .collect(),
    });
    &TABLES
}

#[cfg(test)]
mod tests {
    use psychopomp::math::{shapes::Box2, vec2};

    use super::{
        FRAME_BYTES, Region, WeightedLinear, accumulate_region, add_linear, encode_frame,
        encode_linear, exposure, exposure_at_fps, linear_tables, merge_equal_samples,
    };

    #[test]
    fn default_fps_preserves_exposure_times_and_weights() {
        for samples in [1, 4, 8, 16, 24] {
            assert_eq!(
                exposure(2.0, 1.0 / 60.0, samples),
                exposure_at_fps(2.0, 1.0 / 60.0, samples, 60),
            );
        }
    }

    #[test]
    fn export_fps_scales_the_shutter_without_crossing_partial_frames() {
        let native = exposure_at_fps(2.0, 1.0 / 60.0, 4, 60);
        let slower = exposure_at_fps(2.0, 1.0 / 30.0, 4, 30);
        for ((native_time, native_weight), (slower_time, slower_weight)) in
            native.into_iter().zip(slower)
        {
            assert!(((slower_time - 2.0) - 2.0 * (native_time - 2.0)).abs() < 1e-12);
            assert_eq!(native_weight, slower_weight);
        }
        let partial = exposure_at_fps(12.0005, 0.001, 4, 30);
        assert!(
            partial
                .iter()
                .all(|(time, _)| (12.0..=12.001).contains(time))
        );
        assert!((partial.iter().map(|(_, weight)| weight).sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn partial_frame_samples_stay_inside_the_render_window() {
        let samples = exposure(12.0005, 0.001, 8);
        assert!(
            samples
                .iter()
                .all(|(time, _)| (12.0..=12.001).contains(time))
        );
        let total: f32 = samples.iter().map(|(_, weight)| weight).sum();
        assert!((total - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_shutter_eases_off_at_both_ends() {
        let samples = exposure(1.0, 1.0 / 60.0, 16);
        assert!(samples[0].1 < samples[8].1 * 0.2);
        assert!((samples[0].1 - samples[15].1).abs() < 1e-6, "symmetric");
        assert!(samples.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn region_exposure_matches_the_whole_frame_bit_for_bit() {
        let boxes = [
            Box2 {
                min: vec2(100.4, 50.0),
                max: vec2(300.0, 90.6),
            },
            Box2 {
                min: vec2(250.0, 80.0),
                max: vec2(420.2, 140.0),
            },
            Box2 {
                min: vec2(1800.0, 1000.0),
                max: vec2(2100.0, 1200.0),
            },
        ];
        let region = Region::covering(boxes);
        let base = (0..FRAME_BYTES)
            .map(|index| (index * 7 % 251) as u8)
            .collect::<Vec<_>>();
        let sample = |time: f64| {
            let mut frame = base.clone();
            for row in region.rows() {
                for (offset, value) in frame[row.clone()].iter_mut().enumerate() {
                    *value = value.wrapping_add((time * 97.0) as u8 ^ (row.start + offset) as u8);
                }
            }
            frame
        };
        let samples = exposure(2.0, 1.0 / 60.0, 7);

        let tables = linear_tables();
        let mut sum = vec![0.0_f32; FRAME_BYTES];
        for &(time, weight) in &samples {
            for (sum, pixel) in sum
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(sample(time).as_chunks::<4>().0)
            {
                add_linear(tables, sum, pixel, weight);
            }
        }
        let whole = sum
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|sum| encode_linear(tables, sum))
            .collect::<Vec<_>>();

        let exposed = accumulate_region(&samples, &region, sample(samples[0].0), |frame, time| {
            region.copy(&sample(time), frame);
            Ok(())
        })
        .unwrap();
        assert!(exposed == whole);
    }

    #[test]
    fn identical_temporal_states_are_weighted_once() {
        let samples = merge_equal_samples(
            [(0.1, 0.25), (0.2, 0.25), (0.3, 0.25), (0.4, 0.25)],
            |time| Ok::<_, anyhow::Error>((time * 10.0_f64).round() as u32 % 2),
        )
        .unwrap();
        assert_eq!(samples, vec![(0.1, 0.5), (0.2, 0.5)]);
    }

    #[test]
    fn weighted_table_accumulation_is_bit_identical_to_reference() {
        let tables = linear_tables();
        let samples = exposure(0.5, 1.0 / 60.0, 8);
        let frame_pixels: Vec<u8> = (0..FRAME_BYTES)
            .map(|i| ((i.wrapping_mul(131) ^ (i >> 8)) & 0xFF) as u8)
            .collect();

        let mut ref_sum = vec![0.0_f32; FRAME_BYTES];
        for &(_, weight) in &samples {
            for (sum, pixel) in ref_sum
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(frame_pixels.as_chunks::<4>().0)
            {
                add_linear(tables, sum, pixel, weight);
            }
        }
        let reference: Vec<u8> = ref_sum
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|sum| encode_linear(tables, sum))
            .collect();

        let mut opt_sum = vec![0.0_f32; FRAME_BYTES];
        for &(_, weight) in &samples {
            let weighted = WeightedLinear::new(tables, weight);
            for (sum, pixel) in opt_sum
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(frame_pixels.as_chunks::<4>().0)
            {
                weighted.add(sum, pixel);
            }
        }
        let optimized = encode_frame(tables, &opt_sum);
        assert_eq!(
            ref_sum.len(),
            opt_sum.len(),
            "accumulated float buffers match length"
        );
        assert!(
            ref_sum
                .iter()
                .zip(&opt_sum)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "accumulated f32 sums are IEEE-754 bit-for-bit identical"
        );
        assert_eq!(optimized, reference);
    }
}
