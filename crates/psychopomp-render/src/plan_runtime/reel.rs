//! A reel plays independently prepared Scene Plans on one clock. Each segment keeps
//! its own actors and local time; crossfades blend the outgoing and incoming frames,
//! and every segment's media is retimed onto the reel clock for one audio mix.
use std::path::Path;

use anyhow::{Context, Result};
use psychopomp::{
    composition::{Duration, MediaPlacement, Time, TimeRange},
    plan::{ReelPlan, ReelZoom},
};
use serde_json::{Value, json};

use super::{PreparedPlan, VisualSampleKey, inspect_plan, validate_renderer_plan};
use crate::{
    exposure::{HEIGHT, WIDTH},
    render::HeadlessRenderer,
};

pub(super) fn validate(reel: &ReelPlan) -> Result<()> {
    for segment in &reel.segments {
        validate_renderer_plan(&segment.plan)
            .with_context(|| format!("reel segment '{}'", segment.plan.id))?;
    }
    Ok(())
}

pub(super) fn inspect(reel: &ReelPlan) -> Value {
    let segments = reel
        .segments
        .iter()
        .zip(reel.spans())
        .map(|(segment, span)| {
            json!({
                "startNanos": span.start_nanos,
                "endNanos": span.end_nanos,
                "transitionNanos": span.transition_nanos,
                "plan": inspect_plan(&segment.plan),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "id": reel.id,
        "version": reel.version,
        "durationNanos": reel.duration_nanos(),
        "segments": segments,
    })
}

pub(super) fn segment_window(reel: &ReelPlan, id: &str) -> Result<TimeRange> {
    let (_, span) = reel
        .segments
        .iter()
        .zip(reel.spans())
        .find(|(segment, _)| segment.plan.id == id)
        .with_context(|| format!("reel has no segment '{id}'"))?;
    Ok(TimeRange::new(
        Time::from_nanos(span.start_nanos),
        Time::from_nanos(span.end_nanos),
    ))
}

pub(super) struct PreparedReel {
    reel: ReelPlan,
    segments: Vec<PreparedPlan>,
    media: Vec<MediaPlacement>,
    /// Recent segment frames keyed by their visual sample key.
    segment_frames: SegmentFrames,
}

#[derive(Debug, PartialEq)]
pub(super) struct ReelSampleKey(Vec<(usize, u32, Option<u32>, VisualSampleKey)>);

impl PreparedReel {
    pub(super) fn prepare(
        reel: ReelPlan,
        base: &Path,
        renderer: &mut HeadlessRenderer,
    ) -> Result<Self> {
        let spans = reel.spans();
        let mut segments = Vec::with_capacity(reel.segments.len());
        let mut media = Vec::new();
        for (segment, span) in reel.segments.iter().zip(spans) {
            let prepared = PreparedPlan::prepare(segment.plan.clone(), base, renderer)
                .with_context(|| format!("prepare reel segment '{}'", segment.plan.id))?;
            let offset = Duration::from_nanos(span.start_nanos);
            media.extend(
                prepared
                    .media
                    .iter()
                    .map(|placement| placement.shifted(offset)),
            );
            segments.push(prepared);
        }
        Ok(Self {
            reel,
            segments,
            media,
            segment_frames: SegmentFrames::default(),
        })
    }

    pub(super) fn duration(&self) -> Duration {
        Duration::from_nanos(self.reel.duration_nanos())
    }

    pub(super) fn media(&self) -> &[MediaPlacement] {
        &self.media
    }

    pub(super) fn visual_sample_key(&self, time: f64) -> Result<ReelSampleKey> {
        self.reel
            .layers_at(time)
            .into_iter()
            .map(|layer| {
                // A zoom or wipe moves pixels even when both segments hold still.
                Ok((
                    layer.segment,
                    layer.weight.to_bits(),
                    layer
                        .zoom
                        .map(|phase| phase.progress)
                        .or(layer.wipe.map(|phase| phase.position))
                        .or(layer.transition.map(|phase| phase.progress))
                        .map(f32::to_bits),
                    self.segments[layer.segment].visual_sample_key(layer.local_seconds)?,
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(ReelSampleKey)
    }

    /// The segment shown alone through a whole exposure, with its local
    /// times, or `None` while segments mix or zoom.
    fn sole_segment(&self, exposure: &[(f64, f32)]) -> Option<(usize, Vec<(f64, f32)>)> {
        let mut segment = None;
        let mut local = Vec::with_capacity(exposure.len());
        for &(time, weight) in exposure {
            let layers = self.reel.layers_at(time);
            let [layer] = layers.as_slice() else {
                return None;
            };
            if layer.weight < 1.0
                || layer.zoom.is_some()
                || layer.wipe.is_some()
                || layer.transition.is_some()
                || *segment.get_or_insert(layer.segment) != layer.segment
            {
                return None;
            }
            local.push((layer.local_seconds, weight));
        }
        Some((segment?, local))
    }

    pub(super) fn temporal_samples(&self, center: f64) -> u32 {
        match self.sole_segment(&[(center, 1.0)]) {
            Some((segment, local)) => self.segments[segment].temporal_samples(local[0].0),
            None => crate::exposure::plan_temporal_samples(center)
                .max(crate::exposure::TRANSITION_TEMPORAL_SAMPLES),
        }
    }

    /// One exposed frame. A segment shown alone renders its own exposure (a
    /// Stage accumulates on the GPU); mixes, zooms, wipes, and composited
    /// transitions average on the CPU.
    pub(super) fn render_exposure(
        &self,
        renderer: &mut HeadlessRenderer,
        exposure: &[(f64, f32)],
    ) -> Result<Vec<u8>> {
        if let Some((segment, local)) = self.sole_segment(exposure) {
            let prepared = &self.segments[segment];
            renderer.set_file_name(prepared.file_name());
            return prepared.render_exposure(renderer, &local);
        }
        crate::exposure::accumulate(renderer, exposure, |renderer, time| {
            self.render_sample(renderer, time)
        })
    }

    pub(super) fn render_sample(
        &self,
        renderer: &mut HeadlessRenderer,
        time: f64,
    ) -> Result<Vec<u8>> {
        let layers = self.reel.layers_at(time);
        let mut blended = match layers.first() {
            Some(first) if first.weight >= 1.0 => None,
            // A dip, or an instant with nothing visible, starts from the theme's
            // empty background.
            _ => Some(renderer.render_title_card("", None, 0.0)),
        };
        for layer in layers {
            if layer.weight <= 0.0 {
                continue;
            }
            let prepared = &self.segments[layer.segment];
            renderer.set_file_name(prepared.file_name());
            let pixels = self.segment_sample(renderer, layer.segment, layer.local_seconds)?;
            if let (Some(wipe), Some(below)) = (layer.wipe, blended.as_mut()) {
                let labels = self.reel.segments[layer.segment]
                    .transition_wipe
                    .as_ref()
                    .and_then(|wipe| wipe.labels.as_ref());
                renderer.composite_wipe(below, &pixels, wipe, labels);
                continue;
            }
            if let (Some(phase), Some(below)) = (layer.transition, blended.as_mut()) {
                renderer.composite_transition(below, &pixels, phase);
                continue;
            }
            let (pixels, coverage) = match layer.zoom {
                Some(phase) => {
                    let zoom = ReelZoom::at(
                        phase.focus,
                        WIDTH as f32,
                        HEIGHT as f32,
                        phase.progress,
                        phase.incoming,
                    );
                    let (warped, coverage) = warp(&pixels, zoom);
                    (warped, Some(coverage))
                }
                None => (pixels, None),
            };
            blended = Some(match (blended, coverage) {
                (None, None) => pixels,
                (None, Some(coverage)) => {
                    let mut below = renderer.render_title_card("", None, 0.0);
                    mix_covered(&mut below, &pixels, &coverage, layer.weight);
                    below
                }
                (Some(mut below), None) => {
                    crossfade(&mut below, &pixels, layer.weight);
                    below
                }
                (Some(mut below), Some(coverage)) => {
                    mix_covered(&mut below, &pixels, &coverage, layer.weight);
                    below
                }
            });
        }
        blended.context("reel has no visible segment at this time")
    }

    /// One segment's frame at its local time, reused when an earlier sample of
    /// the same segment had an equal visual sample key: the key that already
    /// merges equal shutter samples. A segment with ambient time (every Stage)
    /// keys on its exact time, so it is rendered and never stored. Debug
    /// builds re-render each hit and check it.
    fn segment_sample(
        &self,
        renderer: &mut HeadlessRenderer,
        segment: usize,
        local_seconds: f64,
    ) -> Result<Vec<u8>> {
        let prepared = &self.segments[segment];
        let key = prepared.visual_sample_key(local_seconds)?;
        if key.ambient_time.is_some() {
            return prepared.render_sample(renderer, local_seconds);
        }
        if let Some(hit) = self.segment_frames.get(segment, &key) {
            if cfg!(debug_assertions) {
                let fresh = prepared.render_sample(renderer, local_seconds)?;
                assert!(
                    fresh == hit,
                    "segment {segment} at {local_seconds}s differs from its cached frame"
                );
            }
            return Ok(hit);
        }
        let pixels = prepared.render_sample(renderer, local_seconds)?;
        self.segment_frames.put(segment, key, pixels.clone());
        Ok(pixels)
    }
}

/// A small most-recent-first store of rendered segment frames. Entries are
/// exact: a hit requires the same segment and an equal visual sample key.
#[derive(Default)]
struct SegmentFrames(std::sync::Mutex<Vec<(usize, VisualSampleKey, Vec<u8>)>>);

impl SegmentFrames {
    /// Enough for both sides of a transition, plus a few held poses.
    const CAPACITY: usize = 4;

    fn get(&self, segment: usize, key: &VisualSampleKey) -> Option<Vec<u8>> {
        let entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries
            .iter()
            .find(|(s, k, _)| *s == segment && k == key)
            .map(|(_, _, pixels)| pixels.clone())
    }

    fn put(&self, segment: usize, key: VisualSampleKey, pixels: Vec<u8>) {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.insert(0, (segment, key, pixels));
        entries.truncate(Self::CAPACITY);
    }
}

/// A frame magnified or shrunk on screen: `output = source * scale + offset`,
/// sampled bilinearly, with per-pixel coverage that is zero outside the source
/// and rounded at `radius` (in output pixels) while the frame is card-sized.
fn warp(source: &[u8], zoom: ReelZoom) -> (Vec<u8>, Vec<f32>) {
    let (width, height) = (WIDTH as usize, HEIGHT as usize);
    let mut pixels = vec![0_u8; width * height * 4];
    let mut coverage = vec![0.0_f32; width * height];
    let inverse = 1.0 / zoom.scale;
    let radius = zoom.radius * inverse;
    let (w, h) = (width as f32, height as f32);
    for y in 0..height {
        let sy = (y as f32 + 0.5 - zoom.offset[1]) * inverse - 0.5;
        if sy < -1.0 || sy > h {
            continue;
        }
        for x in 0..width {
            let sx = (x as f32 + 0.5 - zoom.offset[0]) * inverse - 0.5;
            if sx < -1.0 || sx > w {
                continue;
            }
            // Distance inside the (rounded) source rectangle, in source pixels.
            let qx = (sx + 0.5 - w * 0.5).abs() - (w * 0.5 - radius);
            let qy = (sy + 0.5 - h * 0.5).abs() - (h * 0.5 - radius);
            let outside = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            let cover = (0.5 - outside * zoom.scale).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let x0 = sx.floor().clamp(0.0, w - 1.0) as usize;
            let y0 = sy.floor().clamp(0.0, h - 1.0) as usize;
            let x1 = (x0 + 1).min(width - 1);
            let y1 = (y0 + 1).min(height - 1);
            let fx = (sx - x0 as f32).clamp(0.0, 1.0);
            let fy = (sy - y0 as f32).clamp(0.0, 1.0);
            let index = (y * width + x) * 4;
            for channel in 0..3 {
                let at = |px: usize, py: usize| f32::from(source[(py * width + px) * 4 + channel]);
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
                pixels[index + channel] = (top + (bottom - top) * fy).round() as u8;
            }
            pixels[index + 3] = 255;
            coverage[y * width + x] = cover;
        }
    }
    (pixels, coverage)
}

/// Mix `above` over `below` where it covers, by `weight`.
fn mix_covered(below: &mut [u8], above: &[u8], coverage: &[f32], weight: f32) {
    for (index, cover) in coverage.iter().enumerate() {
        let amount = (cover * weight).clamp(0.0, 1.0);
        if amount <= 0.0 {
            continue;
        }
        for channel in 0..3 {
            let i = index * 4 + channel;
            let mixed = f32::from(below[i]) + (f32::from(above[i]) - f32::from(below[i])) * amount;
            below[i] = mixed.round() as u8;
        }
    }
}

/// Mix the opaque `above` frame over the opaque `below` frame by `weight`.
fn crossfade(below: &mut [u8], above: &[u8], weight: f32) {
    let weight = weight.clamp(0.0, 1.0);
    for (dst, src) in below.iter_mut().zip(above) {
        let mixed = f32::from(*dst) + (f32::from(*src) - f32::from(*dst)) * weight;
        *dst = mixed.round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::{SegmentFrames, VisualSampleKey, crossfade};
    use serde_json::json;

    fn key() -> VisualSampleKey {
        VisualSampleKey {
            motion: vec![[1, 2, 3, 4]],
            states: vec![json!("a")],
            video_frames: vec![7],
            ambient_time: None,
            anchors: vec![[5, 6]],
        }
    }

    #[test]
    fn segment_frames_hit_only_on_equal_segment_and_key() {
        let frames = SegmentFrames::default();
        frames.put(0, key(), vec![9; 4]);
        assert_eq!(frames.get(0, &key()), Some(vec![9; 4]));
        assert_eq!(frames.get(1, &key()), None);
        let changes: [fn(&mut VisualSampleKey); 6] = [
            |k| k.motion[0][0] += 1,
            |k| k.motion[0][3] += 1,
            |k| k.states[0] = json!("b"),
            |k| k.video_frames[0] += 1,
            |k| k.ambient_time = Some(0),
            |k| k.anchors[0][1] += 1,
        ];
        for change in changes {
            let mut changed = key();
            change(&mut changed);
            assert_eq!(frames.get(0, &changed), None);
        }
    }

    #[test]
    fn segment_frames_keep_only_the_most_recent_entries() {
        let frames = SegmentFrames::default();
        for segment in 0..=SegmentFrames::CAPACITY {
            frames.put(segment, key(), vec![segment as u8]);
        }
        assert_eq!(frames.get(0, &key()), None);
        assert_eq!(frames.get(1, &key()), Some(vec![1]));
    }

    #[test]
    fn crossfade_mixes_linearly_between_opaque_frames() {
        let mut below = vec![0, 100, 200, 255];
        crossfade(&mut below, &[200, 100, 0, 255], 0.25);
        assert_eq!(below, vec![50, 100, 150, 255]);
        let mut unchanged = vec![10, 20, 30, 255];
        crossfade(&mut unchanged, &[250, 250, 250, 255], 0.0);
        assert_eq!(unchanged, vec![10, 20, 30, 255]);
    }
}
