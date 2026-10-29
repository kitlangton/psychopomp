//! Pixels for the explainer visualization overlays: checklists, meters,
//! benchmark bars, subtitles, and confetti. They share the chart ink
//! (`render/chart.rs`), cached CommitMono sprites, and these few helpers:
//! plain labels, a stroke whose ink varies along its length, arcs, and
//! rotated rectangles.
mod bars;
mod checklist;
mod confetti;
mod meter;
mod readout;
mod subtitles;
pub(crate) use confetti::confetti_bounds;

use psychopomp::{face::Face, math::lerp};

use super::{
    HeadlessRenderer, PlainTextSpec, TextDraw, blend_pixel, chart::Anchor, composite_text,
};

/// One line of bundled CommitMono at `size`, for overlay labels. Only the
/// regular and bold faces are bundled, and a semibold request falls back to
/// an installed font, so these overlays stay at the regular weight.
pub(super) fn text_spec(size: f32, color: [u8; 3]) -> PlainTextSpec {
    PlainTextSpec {
        font_size: size,
        color,
        size: [1600, (size * 1.5).ceil() as u32],
        semibold: false,
        crop_to_advance: true,
    }
}

impl HeadlessRenderer {
    fn viz_canvas(&self) -> [u32; 2] {
        [self.spec.width, self.spec.height]
    }

    fn text_advance(&mut self, text: &str, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.plain_text_sprite(text, text_spec(size, [255; 3]))
            .advance
    }

    /// One line of text vertically centered on `at[1]` and anchored at
    /// `at[0]`, at a fractional position; returns its width.
    #[allow(clippy::too_many_arguments)]
    fn viz_text(
        &mut self,
        pixels: &mut [u8],
        text: &str,
        size: f32,
        color: [u8; 3],
        anchor: Anchor,
        at: [f32; 2],
        opacity: f32,
    ) -> f32 {
        self.viz_text_in(Face::Mono, pixels, text, size, color, anchor, at, opacity)
    }

    /// [`Self::viz_text`] set in `face`.
    #[allow(clippy::too_many_arguments)]
    fn viz_text_in(
        &mut self,
        face: Face,
        pixels: &mut [u8],
        text: &str,
        size: f32,
        color: [u8; 3],
        anchor: Anchor,
        at: [f32; 2],
        opacity: f32,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let canvas = self.viz_canvas();
        let spec = text_spec(size, color);
        let sprite = self.plain_text_sprite_in(face, text, spec);
        let advance = sprite.advance;
        if opacity <= 0.001 {
            return advance;
        }
        let left = match anchor {
            Anchor::Left => at[0],
            Anchor::Center => at[0] - advance * 0.5,
            Anchor::Right => at[0] - advance,
        };
        composite_text(
            pixels,
            canvas,
            TextDraw {
                clip_width: advance.ceil() + 1.0,
                opacity,
                ..TextDraw::new(sprite, [left, at[1] - spec.size[1] as f32 * 0.5])
            },
        );
        advance
    }
}

/// A round-capped stroke whose ink weight varies along it, as a spinner's
/// wake fades toward its tail. Coverage is unioned across segments.
pub(super) fn weighted_stroke(
    pixels: &mut [u8],
    [width, height]: [u32; 2],
    points: &[([f32; 2], f32)],
    stroke: f32,
    [r, g, b]: [u8; 3],
    opacity: f32,
) {
    if points.len() < 2 || opacity <= 0.001 {
        return;
    }
    let pad = stroke * 0.5 + 1.0;
    let fold = |axis: usize, f: fn(f32, f32) -> f32, start: f32| {
        points.iter().map(|(p, _)| p[axis]).fold(start, f)
    };
    let x0 = (fold(0, f32::min, f32::INFINITY) - pad).floor().max(0.0) as i32;
    let x1 = (fold(0, f32::max, f32::NEG_INFINITY) + pad)
        .ceil()
        .min(width as f32) as i32;
    let y0 = (fold(1, f32::min, f32::INFINITY) - pad).floor().max(0.0) as i32;
    let y1 = (fold(1, f32::max, f32::NEG_INFINITY) + pad)
        .ceil()
        .min(height as f32) as i32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let stride = (x1 - x0) as usize;
    let mut mask = vec![0_f32; stride * (y1 - y0) as usize];
    for pair in points.windows(2) {
        let ((a, wa), (b, wb)) = (pair[0], pair[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = d[0] * d[0] + d[1] * d[1];
        let lo = [
            ((a[0].min(b[0]) - pad).floor() as i32).max(x0),
            ((a[1].min(b[1]) - pad).floor() as i32).max(y0),
        ];
        let hi = [
            ((a[0].max(b[0]) + pad).ceil() as i32).min(x1),
            ((a[1].max(b[1]) + pad).ceil() as i32).min(y1),
        ];
        for y in lo[1]..hi[1] {
            for x in lo[0]..hi[0] {
                let p = [x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]];
                let t = if length > 0.0 {
                    ((p[0] * d[0] + p[1] * d[1]) / length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance = (p[0] - d[0] * t).hypot(p[1] - d[1] * t);
                let coverage = (stroke * 0.5 + 0.5 - distance).clamp(0.0, 1.0) * lerp(wa, wb, t);
                let i = (y - y0) as usize * stride + (x - x0) as usize;
                mask[i] = mask[i].max(coverage);
            }
        }
    }
    for y in y0..y1 {
        for x in x0..x1 {
            let coverage = mask[(y - y0) as usize * stride + (x - x0) as usize];
            if coverage > 0.0 {
                let i = (y as usize * width as usize + x as usize) * 4;
                blend_pixel(&mut pixels[i..i + 4], [r, g, b, 255], coverage * opacity);
            }
        }
    }
}

/// Points along a circular arc from `from` to `to` radians (clockwise from
/// twelve o'clock), about every 2 px.
pub(super) fn arc(center: [f32; 2], radius: f32, from: f32, to: f32) -> Vec<[f32; 2]> {
    let steps = ((to - from).abs() * radius / 2.0).ceil().clamp(1.0, 720.0) as usize;
    (0..=steps)
        .map(|step| {
            let angle = lerp(from, to, step as f32 / steps as f32);
            [
                center[0] + radius * angle.sin(),
                center[1] - radius * angle.cos(),
            ]
        })
        .collect()
}

/// A filled rectangle of `size` centered on `center`, rotated by `angle`,
/// with analytic edge coverage.
pub(super) fn rotated_rect(
    pixels: &mut [u8],
    [width, height]: [u32; 2],
    center: [f32; 2],
    size: [f32; 2],
    angle: f32,
    [r, g, b]: [u8; 3],
    opacity: f32,
) {
    if opacity <= 0.001 || size[0] <= 0.0 || size[1] <= 0.0 {
        return;
    }
    let half = [size[0] * 0.5, size[1] * 0.5];
    let (sin, cos) = angle.sin_cos();
    let reach_x = half[0] * cos.abs() + half[1] * sin.abs() + 1.0;
    let reach_y = half[0] * sin.abs() + half[1] * cos.abs() + 1.0;
    let x0 = (center[0] - reach_x).floor().max(0.0) as i32;
    let x1 = (center[0] + reach_x).ceil().min(width as f32) as i32;
    let y0 = (center[1] - reach_y).floor().max(0.0) as i32;
    let y1 = (center[1] + reach_y).ceil().min(height as f32) as i32;
    let thin = size[1].min(size[0]).min(1.0);
    for y in y0..y1 {
        let py = y as f32 + 0.5 - center[1];
        for x in x0..x1 {
            let px = x as f32 + 0.5 - center[0];
            let local = [px * cos + py * sin, -px * sin + py * cos];
            let d = [local[0].abs() - half[0], local[1].abs() - half[1]];
            if d[0] >= 0.5 || d[1] >= 0.5 {
                continue;
            }
            let distance = if d[0] <= 0.0 || d[1] <= 0.0 {
                d[0].max(d[1])
            } else {
                d[0].hypot(d[1])
            };
            // Thin pieces keep their area as they turn edge-on.
            let coverage = (0.5 - distance).clamp(0.0, 1.0) * thin;
            if coverage > 0.0 {
                let i = (y as usize * width as usize + x as usize) * 4;
                blend_pixel(&mut pixels[i..i + 4], [r, g, b, 255], coverage * opacity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_run_clockwise_from_twelve_and_strokes_fade_with_their_weights() {
        let points = arc([0.0, 0.0], 100.0, 0.0, std::f32::consts::FRAC_PI_2);
        assert!((points[0][1] + 100.0).abs() < 1e-3);
        let last = points.last().unwrap();
        assert!((last[0] - 100.0).abs() < 1e-3 && last[1].abs() < 1e-3);
        let mut pixels = vec![0_u8; 40 * 10 * 4];
        weighted_stroke(
            &mut pixels,
            [40, 10],
            &[([2.0, 5.0], 0.0), ([38.0, 5.0], 1.0)],
            4.0,
            [255, 255, 255],
            1.0,
        );
        let alpha = |x: usize| pixels[(5 * 40 + x) * 4 + 3];
        assert!(alpha(5) < alpha(20) && alpha(20) < alpha(35));
        let mut quad = vec![0_u8; 20 * 20 * 4];
        rotated_rect(
            &mut quad,
            [20, 20],
            [10.0, 10.0],
            [8.0, 4.0],
            0.0,
            [255; 3],
            1.0,
        );
        assert_eq!(quad[(10 * 20 + 10) * 4 + 3], 255);
        assert_eq!(quad[(10 * 20 + 16) * 4 + 3], 0);
    }
}
