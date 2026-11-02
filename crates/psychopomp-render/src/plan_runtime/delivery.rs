//! File delivery is separate from preparing and sampling a scene.
use std::{fs, io::BufWriter, path::Path};

use anyhow::{Context, Result, bail};
use psychopomp::composition::{Time, TimeRange};

use super::{PreparedPlan, reel::PreparedReel};
use crate::{
    exposure::{HEIGHT, WIDTH, encode_exposures_for},
    render::HeadlessRenderer,
};

pub(super) fn render_video(
    prepared: &PreparedPlan,
    renderer: &mut HeadlessRenderer,
    output: &Path,
    window: TimeRange,
) -> Result<()> {
    render_video_with(prepared, renderer, output, window, false)
}

pub(super) fn render_video_with(
    prepared: &PreparedPlan,
    renderer: &mut HeadlessRenderer,
    output: &Path,
    window: TimeRange,
    draft: bool,
) -> Result<()> {
    renderer.set_file_name(prepared.file_name());
    encode_exposures_for(
        prepared.plan.canvas,
        renderer,
        output,
        prepared.duration(),
        &prepared.media,
        window,
        |center| {
            if draft {
                1
            } else {
                prepared.temporal_samples(center)
            }
        },
        |time| prepared.visual_sample_key(time),
        |renderer, exposure| prepared.render_exposure(renderer, exposure),
    )?;
    prepared.report_footage();
    Ok(())
}

pub(super) fn render_reel_with(
    prepared: &PreparedReel,
    renderer: &mut HeadlessRenderer,
    output: &Path,
    window: TimeRange,
    draft: bool,
) -> Result<()> {
    encode_exposures_for(
        prepared.reel().canvas(),
        renderer,
        output,
        prepared.duration(),
        prepared.media(),
        window,
        |center| {
            if draft {
                1
            } else {
                prepared.temporal_samples(center)
            }
        },
        |time| prepared.visual_sample_key(time),
        |renderer, exposure| prepared.render_exposure(renderer, exposure),
    )
}

pub(super) fn render_frame(
    prepared: &PreparedPlan,
    renderer: &mut HeadlessRenderer,
    output: &Path,
    at: Time,
) -> Result<()> {
    renderer.set_file_name(prepared.file_name());
    let pixels = prepared.render_sample(renderer, at.as_seconds())?;
    let [w, h] = renderer.size();
    write_rgba_png(output, w, h, &pixels)
}

pub(crate) fn write_png(path: &Path, pixels: &[u8]) -> Result<()> {
    write_rgba_png(path, WIDTH, HEIGHT, pixels)
}

pub(crate) fn write_rgba_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
    let expected = width as usize * height as usize * 4;
    if pixels.len() != expected {
        bail!(
            "renderer returned {} bytes for a {width}x{height} RGBA frame; expected {expected}",
            pixels.len()
        );
    }
    let file =
        fs::File::create(path).with_context(|| format!("create frame {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // Stage grain makes the default deflate level spend hundreds of
    // milliseconds per frame compressing noise; Fast stays lossless.
    encoder.set_compression(png::Compression::Fast);
    encoder
        .write_header()
        .context("write PNG header")?
        .write_image_data(pixels)
        .context("write PNG pixels")?;
    Ok(())
}
