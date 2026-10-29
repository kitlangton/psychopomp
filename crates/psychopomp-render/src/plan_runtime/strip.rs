//! Native contact-sheet (`plan strip`) and region-crop helpers, using the
//! engine's own `HeadlessRenderer` and bundled `CommitMono` font.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::json;

use super::{Theme, delivery::write_rgba_png, still::Loaded};
use crate::exposure::{HEIGHT, WIDTH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CropRect {
    pub(crate) fn parse(spec: &str) -> Result<Self> {
        let parts: Vec<u32> = spec
            .split(',')
            .map(|part| {
                part.trim().parse::<u32>().with_context(|| {
                    format!("invalid crop coordinate in '{spec}' (expected x,y,w,h)")
                })
            })
            .collect::<Result<_>>()?;
        let &[x, y, width, height] = parts.as_slice() else {
            bail!("--crop '{spec}' must have four integers: x,y,w,h");
        };
        ensure!(
            width > 0 && height > 0,
            "--crop width and height must be positive"
        );
        ensure!(
            x < WIDTH && y < HEIGHT,
            "--crop origin ({x},{y}) is outside {WIDTH}x{HEIGHT}"
        );
        Ok(Self {
            x,
            y,
            width: width.min(WIDTH - x),
            height: height.min(HEIGHT - y),
        })
    }
}

pub(crate) fn crop_rgba(pixels: &[u8], src_w: u32, src_h: u32, crop: CropRect) -> Vec<u8> {
    let w = crop.width.min(src_w.saturating_sub(crop.x));
    let h = crop.height.min(src_h.saturating_sub(crop.y));
    let mut out = vec![0_u8; (w * h * 4) as usize];
    for row in 0..h {
        let src_start = (((crop.y + row) * src_w + crop.x) * 4) as usize;
        let dst_start = (row * w * 4) as usize;
        let row_bytes = (w * 4) as usize;
        out[dst_start..dst_start + row_bytes]
            .copy_from_slice(&pixels[src_start..src_start + row_bytes]);
    }
    out
}

/// Area-averaging RGBA box filter for clean contact-sheet thumbnails and pop scans.
pub(crate) fn downscale_rgba(
    pixels: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<u8> {
    if src_w == dst_w && src_h == dst_h {
        return pixels.to_vec();
    }
    let dst_w = dst_w.max(1);
    let dst_h = dst_h.max(1);
    let mut out = vec![0_u8; (dst_w * dst_h * 4) as usize];
    for dy in 0..dst_h {
        let sy0 = (u64::from(dy) * u64::from(src_h) / u64::from(dst_h)) as u32;
        let sy1 = ((u64::from(dy + 1) * u64::from(src_h)).div_ceil(u64::from(dst_h)))
            .min(u64::from(src_h))
            .max(u64::from(sy0 + 1)) as u32;
        for dx in 0..dst_w {
            let sx0 = (u64::from(dx) * u64::from(src_w) / u64::from(dst_w)) as u32;
            let sx1 = ((u64::from(dx + 1) * u64::from(src_w)).div_ceil(u64::from(dst_w)))
                .min(u64::from(src_w))
                .max(u64::from(sx0 + 1)) as u32;
            let mut sum = [0_u32; 4];
            let mut count = 0_u32;
            for sy in sy0..sy1 {
                let row_offset = (sy * src_w) as usize;
                for sx in sx0..sx1 {
                    let idx = (row_offset + sx as usize) * 4;
                    for c in 0..4 {
                        sum[c] += u32::from(pixels[idx + c]);
                    }
                    count += 1;
                }
            }
            let dst_idx = ((dy * dst_w + dx) * 4) as usize;
            let half = count / 2;
            for c in 0..4 {
                out[dst_idx + c] = ((sum[c] + half) / count) as u8;
            }
        }
    }
    out
}

pub(crate) struct StripOptions<'a> {
    pub plan_path: &'a Path,
    pub spec: &'a str,
    pub output: &'a Path,
    pub count: usize,
    pub columns: usize,
    pub cell_width: u32,
    pub crop: Option<CropRect>,
    pub shutter: bool,
    pub theme: Theme,
}

pub(crate) fn render_strip(options: StripOptions<'_>) -> Result<()> {
    ensure!(options.columns >= 1, "--cols must be at least 1");
    ensure!(
        (64..=WIDTH).contains(&options.cell_width),
        "--width must be in 64..={WIDTH}"
    );
    let (loaded, mut renderer) =
        pollster::block_on(Loaded::load(options.plan_path, options.theme))?;
    let index = loaded.time_index();
    let times = index.resolve_strip_times(options.spec, options.count)?;
    ensure!(!times.is_empty(), "strip resolved zero frames");

    let (src_w, src_h) = match options.crop {
        Some(c) => (c.width, c.height),
        None => (WIDTH, HEIGHT),
    };
    let cell_w = options.cell_width.min(src_w);
    let cell_h = ((u64::from(cell_w) * u64::from(src_h) + u64::from(src_w / 2)) / u64::from(src_w))
        .max(1) as u32;
    let cols = options.columns.min(times.len()) as u32;
    let rows = (times.len() as u32).div_ceil(cols);
    let gap = 10_u32;
    let label_h = (cell_w / 17).clamp(22, 34);
    let sheet_w = cols * cell_w + (cols + 1) * gap;
    let sheet_h = rows * (cell_h + label_h) + (rows + 1) * gap;

    let mut sheet = vec![0_u8; (sheet_w * sheet_h * 4) as usize];
    for px in sheet.chunks_exact_mut(4) {
        px.copy_from_slice(&[17, 17, 16, 255]);
    }

    let mut frame_meta = Vec::with_capacity(times.len());
    for (idx, &at) in times.iter().enumerate() {
        let full = loaded.still(&mut renderer, at, options.shutter)?;
        let region = match options.crop {
            Some(c) => crop_rgba(&full, WIDTH, HEIGHT, c),
            None => full,
        };
        let thumb = downscale_rgba(&region, src_w, src_h, cell_w, cell_h);

        let col = idx as u32 % cols;
        let row = idx as u32 / cols;
        let ox = gap + col * (cell_w + gap);
        let oy = gap + row * (cell_h + label_h + gap);

        blit_opaque(&mut sheet, sheet_w, ox, oy, &thumb, cell_w, cell_h);
        stroke_border(
            &mut sheet,
            sheet_w,
            sheet_h,
            ox,
            oy,
            cell_w,
            cell_h,
            [44, 44, 41, 255],
        );

        let label = index.format_label(at);
        let font_size = (label_h as f32 * 0.56).clamp(12.0, 19.0);
        renderer.draw_mono_label(
            &mut sheet,
            sheet_w,
            sheet_h,
            [(ox + 4) as i32, (oy + cell_h + 4) as i32],
            font_size,
            [226, 223, 217],
            cell_w.saturating_sub(8),
            &label,
        );
        frame_meta.push(json!({
            "time": (at * 1000.0).round() / 1000.0,
            "segment": index.segment_at(at).map(|s| s.id.clone()),
            "label": label,
        }));
    }

    write_rgba_png(options.output, sheet_w, sheet_h, &sheet)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "path": options.output,
            "columns": cols,
            "rows": rows,
            "cellSize": [cell_w, cell_h],
            "sheetSize": [sheet_w, sheet_h],
            "frames": frame_meta,
        }))?
    );
    Ok(())
}

fn blit_opaque(dst: &mut [u8], dst_w: u32, ox: u32, oy: u32, src: &[u8], src_w: u32, src_h: u32) {
    let row_bytes = (src_w * 4) as usize;
    for row in 0..src_h {
        let s = (row * src_w * 4) as usize;
        let d = (((oy + row) * dst_w + ox) * 4) as usize;
        dst[d..d + row_bytes].copy_from_slice(&src[s..s + row_bytes]);
    }
}

#[allow(clippy::too_many_arguments)]
fn stroke_border(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    ox: u32,
    oy: u32,
    w: u32,
    h: u32,
    rgba: [u8; 4],
) {
    let mut put = |x: u32, y: u32| {
        if x < dst_w && y < dst_h {
            let i = ((y * dst_w + x) * 4) as usize;
            dst[i..i + 4].copy_from_slice(&rgba);
        }
    };
    for x in ox..ox + w {
        put(x, oy);
        put(x, oy + h - 1);
    }
    for y in oy..oy + h {
        put(ox, y);
        put(ox + w - 1, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_and_downscale_preserve_constant_colors_and_bounds() {
        let mut frame = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
        for px in frame.chunks_exact_mut(4) {
            px.copy_from_slice(&[40, 120, 200, 255]);
        }
        let crop = CropRect::parse("100, 200, 400, 200").unwrap();
        let cropped = crop_rgba(&frame, WIDTH, HEIGHT, crop);
        assert_eq!(cropped.len(), 400 * 200 * 4);
        let thumb = downscale_rgba(&cropped, 400, 200, 100, 50);
        assert_eq!(thumb.len(), 100 * 50 * 4);
        assert_eq!(&thumb[..4], &[40, 120, 200, 255]);
    }
}
