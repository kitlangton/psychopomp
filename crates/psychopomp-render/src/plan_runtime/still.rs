//! A loaded plan or reel delivered as video, single frames, or frame
//! snapshots; stills may be exposed through the same shutter as video export
//! and compared pixel by pixel.
use std::{fs, path::Path};

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;

use psychopomp::{
    composition::{MediaPlacement, TimeRange},
    time_spec::TimeIndex,
};

use super::{
    DECK_UNSUPPORTED, PlanFile, PreparedPlan, Theme, check, delivery, new_renderer, preflight, reel,
};
use crate::{
    exposure::{HEIGHT, WIDTH, exposure, merge_equal_samples},
    render::HeadlessRenderer,
};

/// A loaded plan or reel, ready to sample.
pub(super) enum Loaded {
    Plan(Box<PreparedPlan>),
    Reel(reel::PreparedReel),
}

impl Loaded {
    pub(super) async fn load(path: &Path, theme: Theme) -> Result<(Self, HeadlessRenderer)> {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        Self::prepare(PlanFile::read(path)?, base, theme).await
    }

    pub(super) async fn prepare(
        file: PlanFile,
        base: &Path,
        theme: Theme,
    ) -> Result<(Self, HeadlessRenderer)> {
        match file {
            PlanFile::Reel(plan) => {
                let mut renderer = new_renderer(&plan.id).await?;
                renderer.set_theme(theme);
                let prepared = reel::PreparedReel::prepare(plan, base, &mut renderer)?;
                Ok((Self::Reel(prepared), renderer))
            }
            PlanFile::Plan(plan) => {
                let input = preflight::Plan::new(plan)?;
                let mut renderer = new_renderer(&input.plan.id).await?;
                renderer.set_theme(theme);
                let prepared = PreparedPlan::prepare_preflight(input, base, &mut renderer)?;
                renderer.set_file_name(prepared.file_name());
                Ok((Self::Plan(Box::new(prepared)), renderer))
            }
            PlanFile::Deck(_) => bail!(DECK_UNSUPPORTED),
        }
    }

    /// Load onto an existing renderer, as `verify` does for many plans with
    /// one GPU device and font system.
    pub(super) fn load_on(
        path: &Path,
        theme: Theme,
        renderer: &mut HeadlessRenderer,
    ) -> Result<Self> {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        match PlanFile::read(path)? {
            PlanFile::Reel(plan) => {
                renderer.set_theme(theme);
                renderer.set_file_name(&plan.id);
                Ok(Self::Reel(reel::PreparedReel::prepare(
                    plan, base, renderer,
                )?))
            }
            PlanFile::Plan(plan) => {
                let input = preflight::Plan::new(plan)?;
                renderer.set_theme(theme);
                renderer.set_file_name(&input.plan.id);
                let prepared = PreparedPlan::prepare_preflight(input, base, renderer)?;
                renderer.set_file_name(prepared.file_name());
                Ok(Self::Plan(Box::new(prepared)))
            }
            PlanFile::Deck(_) => bail!(DECK_UNSUPPORTED),
        }
    }

    /// Encode `window` of the global clock to `output`.
    pub(super) fn render_video(
        &self,
        renderer: &mut HeadlessRenderer,
        output: &Path,
        window: TimeRange,
    ) -> Result<()> {
        self.render_video_with(renderer, output, window, false)
    }

    /// Encode `window` to `output`; with `draft`, each frame takes one instant
    /// sample instead of a full shutter exposure, for fast audio/timing review.
    pub(super) fn render_video_with(
        &self,
        renderer: &mut HeadlessRenderer,
        output: &Path,
        window: TimeRange,
        draft: bool,
    ) -> Result<()> {
        match self {
            Self::Plan(plan) => delivery::render_video_with(plan, renderer, output, window, draft),
            Self::Reel(reel) => delivery::render_reel_with(reel, renderer, output, window, draft),
        }
    }

    pub(super) fn duration_seconds(&self) -> f64 {
        match self {
            Self::Plan(plan) => plan.duration().as_seconds(),
            Self::Reel(reel) => reel.duration().as_seconds(),
        }
    }

    pub(super) fn time_index(&self) -> TimeIndex {
        match self {
            Self::Plan(plan) => TimeIndex::from_plan(&plan.plan),
            Self::Reel(reel) => TimeIndex::from_reel(reel.reel()),
        }
    }

    pub(super) fn media(&self) -> &[MediaPlacement] {
        match self {
            Self::Plan(plan) => &plan.media,
            Self::Reel(reel) => reel.media(),
        }
    }

    pub(super) fn inspect_bounds_at(
        &self,
        renderer: &mut HeadlessRenderer,
        at: f64,
        plan_path: &Path,
    ) -> Vec<check::Finding> {
        match self {
            Self::Plan(plan) => {
                check::inspect_prepared_plan(plan, renderer, at, at, None, plan_path)
            }
            Self::Reel(reel) => {
                check::inspect_reel_at(reel.reel(), reel.segments(), renderer, at, plan_path)
            }
        }
    }

    /// The frame at `at`: one instant, or with `shutter` the exposure a video
    /// frame centered there receives on export.
    pub(super) fn still(
        &self,
        renderer: &mut HeadlessRenderer,
        at: f64,
        shutter: bool,
    ) -> Result<Vec<u8>> {
        ensure!(
            (0.0..=self.duration_seconds()).contains(&at),
            "frame time {at}s is outside 0..{:.3}s",
            self.duration_seconds()
        );
        let span = 1.0 / 60.0;
        match self {
            Self::Plan(plan) if shutter => {
                let samples = exposure(at, span, plan.temporal_samples(at));
                let samples = merge_equal_samples(samples, |time| plan.visual_sample_key(time))?;
                plan.render_exposure(renderer, &samples)
            }
            Self::Reel(reel) if shutter => {
                let samples = exposure(at, span, reel.temporal_samples(at));
                let samples = merge_equal_samples(samples, |time| reel.visual_sample_key(time))?;
                reel.render_exposure(renderer, &samples)
            }
            Self::Plan(plan) => plan.render_sample(renderer, at),
            Self::Reel(reel) => reel.render_sample(renderer, at),
        }
    }
}

/// `a,b,c` seconds, or `from:to:step`.
#[cfg(test)]
pub(super) fn parse_times(value: &str) -> Result<Vec<f64>> {
    let number = |text: &str| {
        text.trim()
            .parse::<f64>()
            .with_context(|| format!("parse time '{text}'"))
    };
    if let [from, to, step] = value.split(':').collect::<Vec<_>>()[..] {
        let (from, to, step) = (number(from)?, number(to)?, number(step)?);
        ensure!(
            step > 0.0 && to >= from,
            "times must be from:to:step with a positive step"
        );
        let count = ((to - from) / step + 1e-9).floor() as usize;
        return Ok((0..=count)
            .map(|index| from + index as f64 * step)
            .collect());
    }
    value.split(',').map(number).collect()
}

/// A stable file name for a frame time.
pub(super) fn frame_name(at: f64) -> String {
    format!("{at:09.3}.png")
}

/// Write frames at `times_spec` into `dir`, or with `compare` render them again
/// and compare against the frames already there.
pub(super) fn snapshot(
    path: &Path,
    times_spec: &str,
    dir: &Path,
    compare: bool,
    shutter: bool,
    theme: Theme,
) -> Result<()> {
    let (loaded, mut renderer) = pollster::block_on(Loaded::load(path, theme))?;
    let times = loaded.time_index().resolve_times(times_spec)?;
    if !compare {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let mut reports = Vec::new();
    let mut differing = 0;
    for &at in &times {
        let pixels = loaded.still(&mut renderer, at, shutter)?;
        let file = dir.join(frame_name(at));
        if !compare {
            delivery::write_png(&file, &pixels)?;
            reports.push(json!({ "time": at, "path": file }));
            continue;
        }
        let baseline = read_png(&file)?;
        let change = Change::between(&baseline, &pixels);
        if change.pixels > 0 {
            differing += 1;
        }
        reports.push(change.report(at));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "frames": reports,
            "differing": differing,
            "identical": differing == 0,
        }))?
    );
    if compare && differing > 0 {
        bail!(
            "{differing} of {} frames differ from {}",
            times.len(),
            dir.display()
        );
    }
    Ok(())
}

/// How two RGBA frames differ in color.
pub(super) struct Change {
    pub(super) pixels: usize,
    pub(super) max: u8,
    /// x0, y0, x1, y1 of the changed pixels, inclusive.
    pub(super) bounds: Option<[usize; 4]>,
}

impl Change {
    pub(super) fn between(before: &[u8], after: &[u8]) -> Self {
        let mut change = Self {
            pixels: 0,
            max: 0,
            bounds: None,
        };
        for (index, (a, b)) in before
            .as_chunks::<4>()
            .0
            .iter()
            .zip(after.as_chunks::<4>().0)
            .enumerate()
        {
            let delta = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            if delta == 0 {
                continue;
            }
            let (x, y) = (index % WIDTH as usize, index / WIDTH as usize);
            change.pixels += 1;
            change.max = change.max.max(delta);
            change.bounds = Some(match change.bounds {
                None => [x, y, x, y],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
            });
        }
        change
    }

    fn report(&self, at: f64) -> serde_json::Value {
        json!({
            "time": at,
            "identical": self.pixels == 0,
            "changedPixels": self.pixels,
            "maxDelta": self.max,
            "bounds": self.bounds,
        })
    }
}

pub(super) fn read_png(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).with_context(|| format!("open baseline {}", path.display()))?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .with_context(|| format!("read {}", path.display()))?;
    let mut pixels = vec![0; reader.output_buffer_size().context("baseline PNG size")?];
    let info = reader.next_frame(&mut pixels)?;
    ensure!(
        info.width == WIDTH && info.height == HEIGHT && info.color_type == png::ColorType::Rgba,
        "{} is not a {WIDTH}x{HEIGHT} RGBA frame",
        path.display()
    );
    pixels.truncate(info.buffer_size());
    Ok(pixels)
}

/// Composite `times_spec` into one onion-skin PNG: stationary pixels keep their
/// median color, while moving pixels leave progressively brighter ghosts
/// leading into the final sample at full opacity.
pub(super) fn onion(
    path: &Path,
    times_spec: &str,
    output: &Path,
    shutter: bool,
    theme: Theme,
) -> Result<()> {
    let (loaded, mut renderer) = pollster::block_on(Loaded::load(path, theme))?;
    let index = loaded.time_index();
    let times = if times_spec.contains(',') || times_spec.contains(':') {
        index.resolve_times(times_spec)?
    } else {
        index.resolve_strip_times(times_spec, 6)?
    };
    ensure!(
        times.len() >= 2,
        "plan onion needs at least two sample times"
    );
    let frames = times
        .iter()
        .map(|&at| loaded.still(&mut renderer, at, shutter))
        .collect::<Result<Vec<_>>>()?;
    let blended = blend_onion(&frames);
    delivery::write_png(output, &blended)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "output": output,
            "frames": times.len(),
            "times": times,
        }))?
    );
    Ok(())
}

pub(super) fn blend_onion(frames: &[Vec<u8>]) -> Vec<u8> {
    let Some(first) = frames.first() else {
        return Vec::new();
    };
    let n = frames.len();
    if n == 1 {
        return first.clone();
    }
    let len = first.len();
    let mut out = vec![0_u8; len];
    let mut scratch = Vec::with_capacity(n);
    for idx in (0..len).step_by(4) {
        let mut base = [0_u8; 4];
        for c in 0..3 {
            scratch.clear();
            scratch.extend(frames.iter().map(|f| f[idx + c]));
            scratch.sort_unstable();
            base[c] = scratch[n / 2];
        }
        base[3] = 255;
        let mut rgb = [f32::from(base[0]), f32::from(base[1]), f32::from(base[2])];
        for (step, frame) in frames.iter().enumerate() {
            let delta = (0..3)
                .map(|c| frame[idx + c].abs_diff(base[c]))
                .max()
                .unwrap_or(0);
            if delta <= 4 {
                continue;
            }
            let alpha = if step + 1 == n {
                1.0
            } else {
                0.22 + 0.48 * (step as f32 / (n - 1) as f32)
            };
            let weight = alpha * ((f32::from(delta) - 4.0) / 24.0).clamp(0.0, 1.0);
            for c in 0..3 {
                rgb[c] += (f32::from(frame[idx + c]) - rgb[c]) * weight;
            }
        }
        out[idx] = rgb[0].round().clamp(0.0, 255.0) as u8;
        out[idx + 1] = rgb[1].round().clamp(0.0, 255.0) as u8;
        out[idx + 2] = rgb[2].round().clamp(0.0, 255.0) as u8;
        out[idx + 3] = 255;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Change, blend_onion, parse_times};

    #[test]
    fn times_parse_as_lists_or_ranges() {
        assert_eq!(parse_times("1,2.5").unwrap(), vec![1.0, 2.5]);
        assert_eq!(parse_times("0:1:0.5").unwrap(), vec![0.0, 0.5, 1.0]);
        assert!(parse_times("1:0:1").is_err());
    }

    #[test]
    fn changes_report_count_size_and_bounds() {
        let before = vec![0; 1920 * 4 * 2];
        let mut after = before.clone();
        after[(1920 + 3) * 4 + 1] = 9;
        let change = Change::between(&before, &after);
        assert_eq!((change.pixels, change.max), (1, 9));
        assert_eq!(change.bounds, Some([3, 1, 3, 1]));
    }

    #[test]
    fn onion_retains_ghosts_and_final_pose() {
        let mut f0 = vec![10, 10, 10, 255, 10, 10, 10, 255, 10, 10, 10, 255];
        let mut f1 = f0.clone();
        let mut f2 = f0.clone();
        f0[0..3].copy_from_slice(&[240, 240, 240]);
        f1[4..7].copy_from_slice(&[240, 240, 240]);
        f2[8..11].copy_from_slice(&[240, 240, 240]);
        let out = blend_onion(&[f0, f1, f2]);
        assert!(out[0] > 40 && out[0] < out[4]);
        assert!(out[4] > out[0] && out[4] < out[8]);
        assert_eq!(out[8], 240);
    }
}
