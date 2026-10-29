//! Rune-calibrated CoreText font smoothing (`rune/research/23-text-quality.md`):
//! 1. Centered `swash` outline dilation (`0.1` pt × raster scale, offset back by
//!    `-dilation` on both axes so glyph centers never shift) + `1/256 px` pen
//!    snapping and 4 quarter-pixel X phases (`0, 0.25, 0.5, 0.75`).
//! 2. Continuous foreground-luminance perceptual coverage exponent (`glyph_gamma`),
//!    giving dark ink on paper/chalk its true CoreText stem weight without linear-light
//!    washout (~3% weight error vs 30% uncalibrated) while keeping bright ink on
//!    obsidian free of halo bloat.

use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source},
    zeno::{Format, Vector},
};

use super::text::{TextSprite, blend_pixel_at, paint_rect};

const SUBPIXEL_STEPS: f32 = 4.0;
/// Rune's calibrated nominal outline emboldening in points (`0.1 pt` -> `0.2 px` at `2×`).
const DILATION_POINTS: f32 = 0.1;

/// Continuous CoreText-calibrated coverage exponent for linear-light blending
/// (`rune/crates/rune/src/text.rs::glyph_coverage_exponent`).
pub(crate) fn glyph_coverage_exponent(linear_rgb: [f32; 3], scale: f32) -> f32 {
    let lin_luma =
        (0.2126 * linear_rgb[0] + 0.7152 * linear_rgb[1] + 0.0722 * linear_rgb[2]).clamp(0.0, 1.0);
    let srgb_luma = if lin_luma <= 0.003_130_8 {
        lin_luma * 12.92
    } else {
        1.055 * lin_luma.powf(1.0 / 2.4) - 0.055
    };
    0.35 + (1.15 + 0.2 * (scale - 1.0).clamp(0.0, 1.0)) * srgb_luma
}

/// CoreText-calibrated coverage exponent from 8-bit sRGB foreground color.
pub(crate) fn glyph_coverage_exponent_srgb(rgb: [u8; 3], scale: f32) -> f32 {
    let lin = super::theme::linear(rgb);
    glyph_coverage_exponent(lin, scale)
}

/// Shape and rasterize a single-line or multi-span run with Rune's centered
/// `swash` outline dilation (`0.1 * raster_scale`), `1/256 px` pen snapping,
/// quarter-pixel X phases, and optional extra `tracking_px` per glyph!
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_smoothed_sprite<'a>(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    spans: Vec<(&'a str, Attrs<'a>)>,
    base: Attrs<'a>,
    metrics: Metrics,
    width: u32,
    height: u32,
    raster_scale: f32,
    tracking_px: f32,
) -> TextSprite {
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(width as f32), Some(height as f32));
    buffer.set_wrap(Wrap::None);
    buffer.set_rich_text(spans, &base, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);

    let dilation = (DILATION_POINTS * raster_scale.max(1.0)).clamp(0.0, 0.5);
    let mut scale_cx = ScaleContext::new();
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let mut max_advance = 0.0_f32;

    let default_rgba = base
        .color_opt
        .map_or([228, 228, 231], |c| [c.r(), c.g(), c.b()]);

    // Collect glyph draw commands first so `font_system` isn't mutably borrowed
    // while `buffer.layout_runs()` iterates.
    struct DrawGlyph {
        font_id: cosmic_text::fontdb::ID,
        glyph_id: u16,
        font_size: f32,
        x: f32,
        y: f32,
        color: [u8; 3],
        cache_key: cosmic_text::CacheKey,
    }
    let mut glyphs = Vec::new();
    for run in buffer.layout_runs() {
        let mut cursor_extra = 0.0_f32;
        for (idx, glyph) in run.glyphs.iter().enumerate() {
            if idx > 0 {
                cursor_extra += tracking_px;
            }
            let gx = glyph.x + cursor_extra;
            let gy = run.line_y + glyph.y;
            let physical = glyph.physical((cursor_extra, run.line_y), 1.0);
            let rgb = glyph
                .color_opt
                .map_or(default_rgba, |c| [c.r(), c.g(), c.b()]);
            glyphs.push(DrawGlyph {
                font_id: glyph.font_id,
                glyph_id: glyph.glyph_id,
                font_size: glyph.font_size,
                x: gx,
                y: gy,
                color: rgb,
                cache_key: physical.cache_key,
            });
        }
        let extra_total = if run.glyphs.len() > 1 {
            tracking_px * (run.glyphs.len() - 1) as f32
        } else {
            0.0
        };
        max_advance = max_advance.max((run.line_w + extra_total).max(0.0));
    }

    for g in glyphs {
        // Snap pen to 1/256 px before flooring to quarter-pixel X phases (`rune` t841).
        let snapped_x = (g.x * 256.0).round() / 256.0;
        let base_x = snapped_x.floor() as i32;
        let frac_x = snapped_x - base_x as f32;
        let subpx_step = ((frac_x * SUBPIXEL_STEPS).floor() / SUBPIXEL_STEPS).clamp(0.0, 0.75);
        let base_y = g.y.round() as i32;

        let mut drew_outline = false;
        font_system
            .db()
            .with_face_data(g.font_id, |font_bytes, face_index| {
                let Some(font_ref) = FontRef::from_index(font_bytes, face_index as usize) else {
                    return;
                };
                let mut scaler = scale_cx
                    .builder(font_ref)
                    .size(g.font_size)
                    .hint(false)
                    .build();
                if scaler.has_color_bitmaps() || scaler.has_color_outlines() {
                    return;
                }
                let Some(image) = Render::new(&[Source::Outline])
                    .format(Format::Alpha)
                    .embolden(dilation)
                    .offset(Vector::new(subpx_step - dilation, -dilation))
                    .render(&mut scaler, g.glyph_id)
                else {
                    return;
                };
                drew_outline = true;
                let (gw, gh) = (image.placement.width, image.placement.height);
                if gw == 0 || gh == 0 || !image.data.iter().any(|&cov| cov != 0) {
                    return;
                }
                let ox = base_x + image.placement.left;
                let oy = base_y - image.placement.top;
                for row in 0..gh as i32 {
                    for col in 0..gw as i32 {
                        let a = image.data[(row as u32 * gw + col as u32) as usize];
                        if a > 0 {
                            blend_pixel_at(
                                &mut pixels,
                                width,
                                height,
                                ox + col,
                                oy + row,
                                [g.color[0], g.color[1], g.color[2], a],
                                1.0,
                            );
                        }
                    }
                }
            });

        if !drew_outline && let Some(img) = swash_cache.get_image_uncached(font_system, g.cache_key)
        {
            let ox = base_x + img.placement.left;
            let oy = base_y - img.placement.top;
            match img.content {
                cosmic_text::SwashContent::Mask => {
                    for row in 0..img.placement.height as i32 {
                        for col in 0..img.placement.width as i32 {
                            let a =
                                img.data[(row as u32 * img.placement.width + col as u32) as usize];
                            if a > 0 {
                                paint_rect(
                                    &mut pixels,
                                    width,
                                    height,
                                    ox + col,
                                    oy + row,
                                    1,
                                    1,
                                    [g.color[0], g.color[1], g.color[2], a],
                                );
                            }
                        }
                    }
                }
                cosmic_text::SwashContent::Color => {
                    for row in 0..img.placement.height as i32 {
                        for col in 0..img.placement.width as i32 {
                            let idx =
                                ((row as u32 * img.placement.width + col as u32) * 4) as usize;
                            let rgba = [
                                img.data[idx],
                                img.data[idx + 1],
                                img.data[idx + 2],
                                img.data[idx + 3],
                            ];
                            if rgba[3] > 0 {
                                blend_pixel_at(
                                    &mut pixels,
                                    width,
                                    height,
                                    ox + col,
                                    oy + row,
                                    rgba,
                                    1.0,
                                );
                            }
                        }
                    }
                }
                cosmic_text::SwashContent::SubpixelMask => {}
            }
        }
    }

    TextSprite {
        width,
        height,
        advance: max_advance,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coretext_exponent_lifts_dark_ink_and_tames_bright_ink() {
        let dark_ink = glyph_coverage_exponent_srgb([22, 21, 20], 2.0);
        let light_ink = glyph_coverage_exponent_srgb([232, 229, 223], 2.0);
        assert!(
            (0.35..0.55).contains(&dark_ink),
            "dark ink should lift edge coverage (<1.0): got {dark_ink}"
        );
        assert!(
            (1.30..1.75).contains(&light_ink),
            "bright ink in linear space should prevent halo bloat (>1.3): got {light_ink}"
        );
        // At 50% raw coverage: dark ink lifts to >70%, bright ink tightens to <41% linear.
        assert!(0.5_f32.powf(dark_ink) > 0.68);
        assert!(0.5_f32.powf(light_ink) < 0.42);
    }

    #[test]
    fn smoothed_sprite_shapes_and_applies_tracking() {
        let mut fonts = crate::render::fonts::font_system();
        let mut swash = SwashCache::new();
        let attrs = crate::render::fonts::attrs(psychopomp::face::Face::SansBold)
            .color(Color::rgb(255, 255, 255));
        let tight = make_smoothed_sprite(
            &mut fonts,
            &mut swash,
            vec![("MOTION", attrs.clone())],
            attrs.clone(),
            Metrics::new(96.0, 110.0),
            800,
            120,
            2.0,
            -4.0,
        );
        let wide = make_smoothed_sprite(
            &mut fonts,
            &mut swash,
            vec![("MOTION", attrs.clone())],
            attrs,
            Metrics::new(96.0, 110.0),
            800,
            120,
            2.0,
            8.0,
        );
        assert!(wide.advance > tight.advance + 40.0);
        assert!(tight.pixels.chunks_exact(4).any(|px| px[3] > 0));
    }
}
