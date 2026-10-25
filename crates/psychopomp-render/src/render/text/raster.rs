//! Exact CPU glyph rasterization. Font provisioning, text identity, theme mapping and layout belong to callers.
use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Wrap};

#[derive(Clone)]
pub(crate) struct TextSprite {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) advance: f32,
    pub(crate) pixels: Vec<u8>,
}

/// How far a glyph's edge coverage `a` is lifted, as the exponent of
/// `1 - (1 - a)^lift`, for text set at `em` pixels. macOS browsers embolden
/// text a little more as it grows; this matches Brave's stems from 24 to
/// 72 px. The Stage shader's `text_weight` uses the same curve.
pub(crate) fn ink_lift(em: f32) -> f32 {
    1.0 + ((em - 16.0) * 0.06).clamp(0.3, 2.0)
}

/// A glyph pixel from cosmic-text with its coverage set to a browser's
/// weight. The CPU composites in sRGB bytes, so the square root first stands
/// in for the GPU's linear-light blend of light ink on a dark ground.
pub(crate) fn glyph_ink(color: Color, lift: f32) -> [u8; 4] {
    let alpha = match color.a() {
        0 => 0,
        255 => 255,
        a => {
            let coverage = (f32::from(a) / 255.0).sqrt();
            ((1.0 - (1.0 - coverage).powf(lift)) * 255.0).round() as u8
        }
    };
    [color.r(), color.g(), color.b(), alpha]
}

/// One line of text, its edges lifted to a browser's weight ([`ink_lift`]).
pub(crate) fn make_sprite<'a>(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    spans: Vec<(&'a str, Attrs<'a>)>,
    base: Attrs<'a>,
    metrics: Metrics,
    width: u32,
    height: u32,
) -> TextSprite {
    let lift = ink_lift(metrics.font_size);
    rasterize(
        font_system,
        swash_cache,
        spans,
        base,
        metrics,
        [width, height],
        lift,
    )
}

/// One line of text with its coverage unlifted (still gamma-2 encoded, as
/// [`glyph_ink`] does), for a supersampled atlas whose shader filters it and
/// then sets the weight.
pub(crate) fn make_coverage_sprite<'a>(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    spans: Vec<(&'a str, Attrs<'a>)>,
    base: Attrs<'a>,
    metrics: Metrics,
    width: u32,
    height: u32,
) -> TextSprite {
    rasterize(
        font_system,
        swash_cache,
        spans,
        base,
        metrics,
        [width, height],
        1.0,
    )
}

fn rasterize<'a>(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    spans: Vec<(&'a str, Attrs<'a>)>,
    base: Attrs<'a>,
    metrics: Metrics,
    [width, height]: [u32; 2],
    lift: f32,
) -> TextSprite {
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(width as f32), Some(height as f32));
    buffer.set_wrap(Wrap::None);
    buffer.set_rich_text(spans, &base, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    let advance = buffer.layout_runs().next().map_or(0.0, |run| run.line_w);
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    buffer.draw(
        font_system,
        swash_cache,
        Color::rgb(228, 228, 231),
        |x, y, rect_width, rect_height, color| {
            paint_rect(
                &mut pixels,
                width,
                height,
                x,
                y,
                rect_width,
                rect_height,
                glyph_ink(color, lift),
            );
        },
    );
    TextSprite {
        width,
        height,
        advance,
        pixels,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    rect_width: u32,
    rect_height: u32,
    color: [u8; 4],
) {
    for row in 0..rect_height as i32 {
        for column in 0..rect_width as i32 {
            blend_pixel_at(pixels, width, height, x + column, y + row, color, 1.0);
        }
    }
}

/// Blends one pixel of a `width × height` RGBA canvas, ignoring points outside it.
pub(crate) fn blend_pixel_at(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    color: [u8; 4],
    opacity: f32,
) {
    if !(0..width as i32).contains(&x) || !(0..height as i32).contains(&y) {
        return;
    }
    let index = (y as usize * width as usize + x as usize) * 4;
    blend_pixel(&mut pixels[index..index + 4], color, opacity);
}

pub(crate) fn blend_pixel(destination: &mut [u8], source: [u8; 4], opacity: f32) {
    let source_alpha = f32::from(source[3]) / 255.0 * opacity.clamp(0.0, 1.0);
    if source_alpha <= 0.0 {
        return;
    }
    let destination_alpha = f32::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let source_channel = f32::from(source[channel]) / 255.0;
        let destination_channel = f32::from(destination[channel]) / 255.0;
        let output = (source_channel * source_alpha
            + destination_channel * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = (output * 255.0).round() as u8;
    }
    destination[3] = (output_alpha * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_rectangles_keep_straight_alpha_bytes_and_clip_at_the_original_origin() {
        let mut pixel = [0; 4];
        blend_pixel(&mut pixel, [255, 0, 0, 128], 1.);
        blend_pixel(&mut pixel, [0, 0, 255, 128], 1.);
        assert_eq!(pixel, [85, 0, 170, 192]);
        let mut pixels = vec![0; 3 * 2 * 4];
        paint_rect(&mut pixels, 3, 2, -1, -1, 3, 3, [12, 34, 56, 128]);
        for (i, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(
                pixel,
                if [0, 1, 3, 4].contains(&i) {
                    &[12, 34, 56, 128]
                } else {
                    &[0, 0, 0, 0]
                }
            );
        }
    }
}
