//! File delivery is separate from preparing and sampling a scene.
use std::{fs, io::BufWriter, path::Path};

use anyhow::{Context, Result, bail};
use psychopomp::composition::{Time, TimeRange};

use super::{PreparedPlan, reel::PreparedReel};
use crate::{
    exposure::{HEIGHT, RenderOptions, WIDTH, encode_exposures},
    render::HeadlessRenderer,
};

pub(super) fn render_video(
    prepared: &PreparedPlan,
    renderer: &mut HeadlessRenderer,
    output: &Path,
    window: TimeRange,
    options: RenderOptions,
) -> Result<()> {
    renderer.set_file_name(prepared.file_name());
    encode_exposures(
        renderer,
        output,
        prepared.duration(),
        &prepared.media,
        window,
        options,
        |center| prepared.temporal_samples(center),
        |time| prepared.visual_sample_key(time),
        |renderer, exposure| prepared.render_exposure(renderer, exposure),
    )?;
    prepared.report_footage();
    Ok(())
}

pub(super) fn render_reel(
    prepared: &PreparedReel,
    renderer: &mut HeadlessRenderer,
    output: &Path,
    window: TimeRange,
    options: RenderOptions,
) -> Result<()> {
    encode_exposures(
        renderer,
        output,
        prepared.duration(),
        prepared.media(),
        window,
        options,
        |center| prepared.temporal_samples(center),
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
    write_png(output, &pixels)
}

pub(crate) fn write_png(path: &Path, pixels: &[u8]) -> Result<()> {
    let expected = WIDTH as usize * HEIGHT as usize * 4;
    if pixels.len() != expected {
        bail!(
            "renderer returned {} bytes for a {WIDTH}x{HEIGHT} RGBA frame; expected {expected}",
            pixels.len()
        );
    }
    let file =
        fs::File::create(path).with_context(|| format!("create frame {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .context("write PNG header")?
        .write_image_data(pixels)
        .context("write PNG pixels")?;
    Ok(())
}
