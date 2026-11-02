//! Agent preflight & continuity inspection (`psychopomp plan check`):
//! - projected screen-space bounds (`clipped_by_canvas`, `off_canvas`, `nan_pose`)
//! - Stage card text fit (`card_text_overflow`)
//! - 1-frame visual discontinuity scan (`--pops`: step pops & flash pops)
//! - optional audio mix loudness/clipping/missing-file verification (`--audio`)

use std::path::Path;

use anyhow::{Result, bail, ensure};
use psychopomp::{composition::TimeRange, plan::ReelPlan, time_spec::TimeIndex};
use serde::Serialize;
use serde_json::json;

use super::{
    PreparedPlan, PreparedRoot, Theme,
    audio_check::{self, AudioReport},
    still::Loaded,
    strip::downscale_rgba,
};
use crate::render::HeadlessRenderer;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailOn {
    Error,
    Warning,
    Never,
}

impl FailOn {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "error" => Ok(Self::Error),
            "warning" => Ok(Self::Warning),
            "never" => Ok(Self::Never),
            other => bail!("--fail-on must be error, warning, or never (got '{other}')"),
        }
    }

    fn triggers(self, errors: usize, warnings: usize) -> bool {
        match self {
            Self::Error => errors > 0,
            Self::Warning => errors > 0 || warnings > 0,
            Self::Never => false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Finding {
    pub severity: Severity,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<String>,
    pub target: String,
    pub message: String,
    pub first_seconds: f64,
    pub last_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[i32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspect_hint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PopKind {
    Step,
    Flash,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PopFinding {
    pub frame: usize,
    pub seconds: f64,
    pub label: String,
    pub kind: PopKind,
    pub bbox: [u32; 4],
    pub strength: f32,
    pub inspect_hint: String,
}

const POP_SCALE_DIV: u32 = 4;
const POP_BLOCK: u32 = 8;
const POP_SUDDEN: f32 = 20.0;
const POP_STILL: f32 = 3.0;
const POP_CUT_SHARE: f32 = 0.30;

/// Constant-memory 4-frame sliding block detector for 1-frame pops and flashes.
pub(crate) struct PopDetector {
    width: u32,
    height: u32,
    cols: usize,
    rows: usize,
    frames: Vec<Vec<u8>>,
    diffs: Vec<Vec<f32>>,
    pushed: usize,
    pops: Vec<(usize, PopKind, [u32; 4], f32)>,
}

impl PopDetector {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            cols: width.div_ceil(POP_BLOCK).max(1) as usize,
            rows: height.div_ceil(POP_BLOCK).max(1) as usize,
            frames: Vec::new(),
            diffs: Vec::new(),
            pushed: 0,
            pops: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, rgba: Vec<u8>) {
        if let Some(prev) = self.frames.last() {
            self.diffs
                .push(block_diffs(prev, &rgba, self.width, self.cols, self.rows));
        }
        self.frames.push(rgba);
        self.pushed += 1;
        if self.frames.len() > 4 {
            self.frames.remove(0);
            self.diffs.remove(0);
        }
        self.detect();
    }

    pub(crate) fn finish(self) -> Vec<(usize, PopKind, [u32; 4], f32)> {
        self.pops
    }

    fn around(&self, values: &[f32], block: usize) -> f32 {
        let (col, row) = ((block % self.cols) as isize, (block / self.cols) as isize);
        let mut best = 0.0_f32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (c, r) = (col + dx, row + dy);
                if c >= 0 && r >= 0 && (c as usize) < self.cols && (r as usize) < self.rows {
                    best = best.max(values[r as usize * self.cols + c as usize]);
                }
            }
        }
        best
    }

    fn detect(&mut self) {
        let total = self.cols * self.rows;
        let n = self.diffs.len();
        if n < 3 {
            return;
        }
        let step_blocks: Vec<(usize, f32)> = {
            let (before, change, after) =
                (&self.diffs[n - 3], &self.diffs[n - 2], &self.diffs[n - 1]);
            (0..total)
                .filter(|&b| {
                    change[b] >= POP_SUDDEN
                        && self.around(before, b) < POP_STILL
                        && self.around(after, b) < POP_STILL
                })
                .map(|b| (b, change[b]))
                .collect()
        };
        self.collect_regions(&step_blocks, PopKind::Step, self.pushed - 2);

        if self.frames.len() >= 4 {
            let flash_blocks: Vec<(usize, f32)> = {
                let (before, change, after) =
                    (&self.diffs[n - 3], &self.diffs[n - 2], &self.diffs[n - 1]);
                let returned = block_diffs(
                    &self.frames[self.frames.len() - 3],
                    &self.frames[self.frames.len() - 1],
                    self.width,
                    self.cols,
                    self.rows,
                );
                (0..total)
                    .filter(|&b| {
                        change[b] >= POP_SUDDEN
                            && after[b] >= POP_SUDDEN
                            && returned[b] < POP_STILL
                            && self.around(before, b) < POP_STILL
                    })
                    .map(|b| (b, change[b]))
                    .collect()
            };
            self.collect_regions(&flash_blocks, PopKind::Flash, self.pushed - 2);
        }
    }

    fn collect_regions(&mut self, flagged: &[(usize, f32)], kind: PopKind, frame: usize) {
        let total = self.cols * self.rows;
        if flagged.is_empty() || (flagged.len() as f32) > POP_CUT_SHARE * total as f32 {
            return;
        }
        let mut rest = flagged.to_vec();
        while let Some(seed) = rest.pop() {
            let mut region = vec![seed];
            let mut idx = 0;
            while idx < region.len() {
                let b = region[idx].0;
                let (col, row) = (b % self.cols, b / self.cols);
                rest.retain(|&(other, strength)| {
                    let (c, r) = (other % self.cols, other / self.cols);
                    let adj = c.abs_diff(col) <= 1 && r.abs_diff(row) <= 1;
                    if adj {
                        region.push((other, strength));
                    }
                    !adj
                });
                idx += 1;
            }
            let min_c = region.iter().map(|(b, _)| b % self.cols).min().unwrap_or(0) as u32;
            let max_c = region.iter().map(|(b, _)| b % self.cols).max().unwrap_or(0) as u32;
            let min_r = region.iter().map(|(b, _)| b / self.cols).min().unwrap_or(0) as u32;
            let max_r = region.iter().map(|(b, _)| b / self.cols).max().unwrap_or(0) as u32;
            let x = min_c * POP_BLOCK;
            let y = min_r * POP_BLOCK;
            let w = ((max_c + 1) * POP_BLOCK).min(self.width) - x;
            let h = ((max_r + 1) * POP_BLOCK).min(self.height) - y;
            let strength = region.iter().map(|&(_, s)| s).fold(0.0_f32, f32::max);
            self.pops.push((frame, kind, [x, y, w, h], strength));
        }
    }
}

fn block_diffs(a: &[u8], b: &[u8], width: u32, cols: usize, rows: usize) -> Vec<f32> {
    let mut sums = vec![0.0_f32; cols * rows];
    let mut counts = vec![0_u32; cols * rows];
    let w = width as usize;
    for (i, (pa, pb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        let (x, y) = (i % w, i / w);
        let blk =
            (y / POP_BLOCK as usize).min(rows - 1) * cols + (x / POP_BLOCK as usize).min(cols - 1);
        let diff = u32::from(pa[0].abs_diff(pb[0]))
            + u32::from(pa[1].abs_diff(pb[1]))
            + u32::from(pa[2].abs_diff(pb[2]));
        sums[blk] += diff as f32 / 3.0;
        counts[blk] += 1;
    }
    sums.into_iter()
        .zip(counts)
        .map(|(s, c)| s / c.max(1) as f32)
        .collect()
}

pub(crate) struct CheckOptions<'a> {
    pub plan_path: &'a Path,
    pub range_spec: &'a str,
    pub every_spec: &'a str,
    pub pops: bool,
    pub audio: bool,
    pub fail_on: FailOn,
    pub theme: Theme,
}

pub(crate) fn run_check(options: CheckOptions<'_>) -> Result<()> {
    let (loaded, mut renderer) =
        pollster::block_on(Loaded::load(options.plan_path, options.theme))?;
    let index = loaded.time_index();
    let range = index.resolve_range(options.range_spec)?;
    let every_nanos = index.resolve_nanos(options.every_spec)?;
    ensure!(every_nanos > 0, "--every step must be positive");

    let sample_times = build_sample_times(&index, range, every_nanos);
    let mut findings: Vec<Finding> = Vec::new();
    for &t in &sample_times {
        let raw = loaded.inspect_bounds_at(&mut renderer, t, options.plan_path);
        for item in raw {
            merge_finding(&mut findings, item, (every_nanos as f64 / 1e9) * 1.5);
        }
    }

    let mut pop_findings = Vec::new();
    if options.pops {
        let canvas = loaded.canvas();
        let fps = canvas.fps.max(1) as f64;
        let [cw, ch] = canvas.size();
        let small_w = (cw / POP_SCALE_DIV).max(16);
        let small_h = (ch / POP_SCALE_DIV).max(16);
        let mut detector = PopDetector::new(small_w, small_h);
        let start_frame = (range.start().as_seconds() * fps).round() as usize;
        let end_frame = (range.end().as_seconds() * fps).round() as usize;
        for f in start_frame..end_frame {
            let t = ((f as f64 + 0.5) / fps).min(loaded.duration_seconds());
            let full = loaded.still(&mut renderer, t, false)?;
            detector.push(downscale_rgba(&full, cw, ch, small_w, small_h));
        }
        for (rel_frame, kind, [sx, sy, sw, sh], strength) in detector.finish() {
            let frame_idx = start_frame + rel_frame;
            let sec = (frame_idx as f64 / fps * 1000.0).round() / 1000.0;
            let [vx, vy, vw, vh] = [
                (sx * POP_SCALE_DIV).saturating_sub(16),
                (sy * POP_SCALE_DIV).saturating_sub(16),
                (sw * POP_SCALE_DIV + 32).min(cw),
                (sh * POP_SCALE_DIV + 32).min(ch),
            ];
            pop_findings.push(PopFinding {
                frame: frame_idx,
                seconds: sec,
                label: index.format_label(sec),
                kind,
                bbox: [vx, vy, vw, vh],
                strength: (strength * 10.0).round() / 10.0,
                inspect_hint: format!(
                    "psychopomp plan frame {} {sec:.3}s --crop {vx},{vy},{vw},{vh}",
                    options.plan_path.display()
                ),
            });
        }
    }

    let audio_report: Option<AudioReport> = if options.audio {
        let report = audio_check::analyze_placements(loaded.media(), &index, range)?;
        for missing in &report.missing_files {
            findings.push(Finding {
                severity: Severity::Error,
                kind: "missing_audio",
                segment: None,
                target: missing.clone(),
                message: format!("audio file '{missing}' does not exist"),
                first_seconds: range.start().as_seconds(),
                last_seconds: range.end().as_seconds(),
                bbox: None,
                inspect_hint: None,
            });
        }
        if report.clipped_samples > 0 {
            findings.push(Finding {
                severity: Severity::Warning,
                kind: "audio_clipping",
                segment: None,
                target: "mix".into(),
                message: format!(
                    "audio mix clips ({} samples >= 0 dBFS, true peak {:?} dBTP)",
                    report.clipped_samples, report.true_peak_dbtp
                ),
                first_seconds: range.start().as_seconds(),
                last_seconds: range.end().as_seconds(),
                bbox: None,
                inspect_hint: None,
            });
        }
        Some(report)
    } else {
        None
    };

    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count()
        + pop_findings.len();
    let infos = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count();
    let failed = options.fail_on.triggers(errors, warnings);

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": !failed,
            "checkedFrames": sample_times.len(),
            "errors": errors,
            "warnings": warnings,
            "infos": infos,
            "findings": findings,
            "pops": pop_findings,
            "audio": audio_report,
        }))?
    );
    if failed {
        bail!("plan check failed ({errors} errors, {warnings} warnings)");
    }
    Ok(())
}

fn build_sample_times(index: &TimeIndex, range: TimeRange, every_nanos: u64) -> Vec<f64> {
    let start = range.start().as_nanos();
    let end = range.end().as_nanos();
    let mut nanos = Vec::new();
    let mut cur = start;
    while cur < end {
        nanos.push(cur);
        cur = cur.saturating_add(every_nanos);
    }
    for sec in index.sections() {
        for mark in [
            sec.start_nanos,
            sec.start_nanos + sec.duration_nanos() / 2,
            sec.end_nanos.saturating_sub(16_666_667),
        ] {
            if mark >= start && mark < end {
                nanos.push(mark);
            }
        }
    }
    nanos.sort_unstable();
    nanos.dedup();
    nanos.into_iter().map(|n| n as f64 / 1e9).collect()
}

fn merge_finding(list: &mut Vec<Finding>, incoming: Finding, max_gap: f64) {
    if let Some(prev) = list.iter_mut().rev().find(|f| {
        f.kind == incoming.kind
            && f.segment == incoming.segment
            && f.target == incoming.target
            && incoming.first_seconds - f.last_seconds <= max_gap
    }) {
        prev.last_seconds = incoming.last_seconds;
        return;
    }
    list.push(incoming);
}

pub(crate) fn inspect_prepared_plan(
    plan: &PreparedPlan,
    renderer: &mut HeadlessRenderer,
    local_time: f64,
    global_time: f64,
    segment_id: Option<&str>,
    plan_path: &Path,
) -> Vec<Finding> {
    renderer.set_palette_override(plan.plan.palette);
    let mut out = Vec::new();
    let size = plan.plan.canvas.size();
    let timeline = &plan.timeline;
    let value = |actor: &str, property: &str, default: f32| {
        plan.property_value(timeline, actor, property, local_time, default)
    };
    let round_sec = (global_time * 1000.0).round() / 1000.0;
    let seg = segment_id.map(str::to_owned);

    if let PreparedRoot::Stage(stage) = &plan.root {
        for item in stage.bounds(local_time, size, value) {
            let target = format!("{}.{}", stage.id(), item.id);
            if item.non_finite {
                out.push(Finding {
                    severity: Severity::Error,
                    kind: "nan_pose",
                    segment: seg.clone(),
                    target,
                    message: format!(
                        "stage {} '{}' has a NaN or infinite pose",
                        item.kind, item.id
                    ),
                    first_seconds: round_sec,
                    last_seconds: round_sec,
                    bbox: None,
                    inspect_hint: None,
                });
                continue;
            }
            if let Some((text, text_w, inner_w)) = item.card_overflow {
                let ibox = to_ibox(item.bbox);
                out.push(Finding {
                    severity: Severity::Warning,
                    kind: "card_text_overflow",
                    segment: seg.clone(),
                    target: target.clone(),
                    message: format!(
                        "card '{}' text \"{text}\" ({text_w:.0}px) exceeds card inner width ({inner_w:.0}px)",
                        item.id
                    ),
                    first_seconds: round_sec,
                    last_seconds: round_sec,
                    bbox: Some(ibox),
                    inspect_hint: Some(crop_cmd(plan_path, round_sec, ibox, size)),
                });
            }
            // Check text-bearing Stage items (`card`, `label`) against canvas edges.
            if matches!(item.kind, "card" | "label")
                && let Some(finding) = classify_canvas_box(
                    item.bbox,
                    size,
                    &target,
                    item.text.as_deref().unwrap_or(&item.id),
                    seg.clone(),
                    round_sec,
                    plan_path,
                )
            {
                out.push(finding);
            }
        }
    }

    for caption in &plan.captions {
        if let Some(origin) = plan.pin(
            caption.id(),
            caption.anchors(),
            caption.origin(),
            local_time,
            timeline,
            size,
        ) && let Some(box2) = caption.bounds(renderer, origin, value)
        {
            let bbox = [
                box2.min.x,
                box2.min.y,
                (box2.max.x - box2.min.x).max(0.0),
                (box2.max.y - box2.min.y).max(0.0),
            ];
            let label = caption.text();
            if let Some(finding) = classify_canvas_box(
                bbox,
                size,
                caption.id(),
                &label,
                seg.clone(),
                round_sec,
                plan_path,
            ) {
                out.push(finding);
            }
        }
    }

    for callout in &plan.callouts {
        if let Some(pose) = plan.callout_pose(callout, local_time, timeline, size)
            && let Some(box2) = callout.bounds(renderer, pose)
        {
            let bbox = [
                box2.min.x,
                box2.min.y,
                (box2.max.x - box2.min.x).max(0.0),
                (box2.max.y - box2.min.y).max(0.0),
            ];
            if let Some(finding) = classify_canvas_box(
                bbox,
                size,
                callout.id(),
                callout.id(),
                seg.clone(),
                round_sec,
                plan_path,
            ) {
                out.push(finding);
            }
        }
    }

    out
}

/// Inspect a `ReelPlan` at `global_time`, skipping layers in the middle of a zoom/wipe/push transition
/// where elements intentionally cross the frame edge.
pub(crate) fn inspect_reel_at(
    reel: &ReelPlan,
    segments: &[PreparedPlan],
    renderer: &mut HeadlessRenderer,
    global_time: f64,
    plan_path: &Path,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for layer in reel.layers_at(global_time) {
        if layer.weight < 0.5
            || layer.zoom.is_some()
            || layer.wipe.is_some()
            || layer.transition.is_some()
        {
            continue;
        }
        let seg_plan = &segments[layer.segment];
        out.extend(inspect_prepared_plan(
            seg_plan,
            renderer,
            layer.local_seconds,
            global_time,
            Some(&seg_plan.plan.id),
            plan_path,
        ));
    }
    out
}

fn classify_canvas_box(
    bbox: [f32; 4],
    [cw, ch]: [u32; 2],
    target: &str,
    label: &str,
    segment: Option<String>,
    sec: f64,
    plan_path: &Path,
) -> Option<Finding> {
    let [x, y, w, h] = bbox;
    if w <= 0.5 || h <= 0.5 {
        return None;
    }
    let right = x + w;
    let bottom = y + h;
    let fw = cw as f32;
    let fh = ch as f32;
    let disjoint = right <= 0.0 || bottom <= 0.0 || x >= fw || y >= fh;
    let inside = x >= -2.0 && y >= -2.0 && right <= fw + 2.0 && bottom <= fh + 2.0;
    let ibox = to_ibox(bbox);
    if disjoint {
        Some(Finding {
            severity: Severity::Info,
            kind: "off_canvas",
            segment,
            target: target.to_owned(),
            message: format!(
                "'{target}' (\"{label}\") is completely outside {cw}x{ch} (x={x:.0} y={y:.0} w={w:.0} h={h:.0})"
            ),
            first_seconds: sec,
            last_seconds: sec,
            bbox: Some(ibox),
            inspect_hint: None,
        })
    } else if !inside {
        Some(Finding {
            severity: Severity::Warning,
            kind: "clipped_by_canvas",
            segment,
            target: target.to_owned(),
            message: format!(
                "'{target}' (\"{label}\") is clipped by canvas edge (x={x:.0} y={y:.0} w={w:.0} h={h:.0})"
            ),
            first_seconds: sec,
            last_seconds: sec,
            bbox: Some(ibox),
            inspect_hint: Some(crop_cmd(plan_path, sec, ibox, [cw, ch])),
        })
    } else {
        None
    }
}

fn to_ibox([x, y, w, h]: [f32; 4]) -> [i32; 4] {
    [
        x.round() as i32,
        y.round() as i32,
        w.round() as i32,
        h.round() as i32,
    ]
}

fn crop_cmd(plan_path: &Path, sec: f64, [x, y, w, h]: [i32; 4], [cw, ch]: [u32; 2]) -> String {
    let cx = (x - 16).clamp(0, cw as i32 - 32) as u32;
    let cy = (y - 16).clamp(0, ch as i32 - 32) as u32;
    let rw = ((w + 32).max(64) as u32).min(cw - cx);
    let rh = ((h + 32).max(64) as u32).min(ch - cy);
    format!(
        "psychopomp plan frame {} {sec:.3}s --crop {cx},{cy},{rw},{rh}",
        plan_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame(w: u32, h: u32, square: Option<(u32, u32, u32)>) -> Vec<u8> {
        let mut px = vec![40_u8; (w * h * 4) as usize];
        for p in px.chunks_exact_mut(4) {
            p[3] = 255;
        }
        if let Some((sx, sy, sz)) = square {
            for y in sy..(sy + sz).min(h) {
                for x in sx..(sx + sz).min(w) {
                    let i = ((y * w + x) * 4) as usize;
                    px[i..i + 4].copy_from_slice(&[240, 240, 240, 255]);
                }
            }
        }
        px
    }

    #[test]
    fn pop_detector_catches_step_and_flash_while_ignoring_smooth_motion_and_cuts() {
        let (w, h) = (64, 48);
        // Step pop at frame 2: absent at 0,1 -> jumps in at 2,3
        let mut det = PopDetector::new(w, h);
        det.push(test_frame(w, h, None));
        det.push(test_frame(w, h, None));
        det.push(test_frame(w, h, Some((16, 16, 16))));
        det.push(test_frame(w, h, Some((16, 16, 16))));
        let pops = det.finish();
        assert!(
            pops.iter()
                .any(|&(f, k, _, _)| f == 2 && k == PopKind::Step)
        );

        // 1-frame flash at frame 2: absent at 0,1 -> present at 2 -> absent at 3
        let mut flash = PopDetector::new(w, h);
        flash.push(test_frame(w, h, None));
        flash.push(test_frame(w, h, None));
        flash.push(test_frame(w, h, Some((16, 16, 16))));
        flash.push(test_frame(w, h, None));
        let flashes = flash.finish();
        assert!(
            flashes
                .iter()
                .any(|&(f, k, _, _)| f == 2 && k == PopKind::Flash)
        );

        // Whole-frame cut (>30% changed) is not reported as a pop.
        let mut cut = PopDetector::new(w, h);
        cut.push(test_frame(w, h, None));
        cut.push(test_frame(w, h, None));
        cut.push(test_frame(w, h, Some((0, 0, 56))));
        cut.push(test_frame(w, h, Some((0, 0, 56))));
        assert!(cut.finish().is_empty());
    }
}
