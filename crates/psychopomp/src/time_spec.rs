//! Scene- and cue-relative time addressing for plans and reels.
//!
//! Supported point specs:
//! - `3.2`, `3.2s`, `500ms`, `1:05.5` — timestamps
//! - `120f` — frame index at `fps` (default 60)
//! - `50%` — percentage of the plan, reel, or named section
//! - `start`, `end` — endpoints (`0` and `duration`)
//! - `intro`, `#2`, `intro[1]` — start of a named segment or cue (or end on the right side of `..`)
//! - `intro@1.2s`, `intro@50%`, `intro@end`, `intro@30f` — offset inside a named segment or cue
//! - `segment/cue@0.5s` — cue inside a reel segment
//!
//! Supported range specs:
//! - `all` or `*` — full `0..duration`
//! - `intro` — `section.start..section.end`
//! - `a..b`, `a..`, `..b` — half-open range; a bare section name on the right side means `section.end`

use anyhow::{Context, Result, bail, ensure};

use crate::{
    composition::{Time, TimeRange},
    plan::{ReelPlan, ScenePlan},
};

const NANOS_PER_SEC: f64 = 1_000_000_000.0;
const DEFAULT_FPS: u32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    Segment,
    Cue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeSection {
    pub index: usize,
    pub id: String,
    pub kind: SectionKind,
    pub start_nanos: u64,
    pub end_nanos: u64,
}

impl TimeSection {
    pub fn duration_nanos(&self) -> u64 {
        self.end_nanos.saturating_sub(self.start_nanos)
    }
}

#[derive(Clone, Debug)]
pub struct TimeIndex {
    duration_nanos: u64,
    fps: u32,
    sections: Vec<TimeSection>,
}

#[derive(Clone, Copy)]
enum Side {
    Point,
    RangeEnd,
}

impl TimeIndex {
    pub fn new(duration_nanos: u64, fps: u32, sections: Vec<TimeSection>) -> Self {
        Self {
            duration_nanos,
            fps: fps.max(1),
            sections,
        }
    }

    pub fn from_plan(plan: &ScenePlan) -> Self {
        let sections = plan
            .cues
            .iter()
            .enumerate()
            .map(|(index, cue)| TimeSection {
                index,
                id: cue.id.clone(),
                kind: SectionKind::Cue,
                start_nanos: cue.start_nanos,
                end_nanos: cue.end_nanos,
            })
            .collect();
        Self::new(plan.duration_nanos, DEFAULT_FPS, sections)
    }

    pub fn from_reel(reel: &ReelPlan) -> Self {
        let spans = reel.spans();
        let mut sections = Vec::new();
        for (index, (segment, span)) in reel.segments.iter().zip(&spans).enumerate() {
            sections.push(TimeSection {
                index,
                id: segment.plan.id.clone(),
                kind: SectionKind::Segment,
                start_nanos: span.start_nanos,
                end_nanos: span.end_nanos,
            });
            for cue in &segment.plan.cues {
                sections.push(TimeSection {
                    index,
                    id: format!("{}/{}", segment.plan.id, cue.id),
                    kind: SectionKind::Cue,
                    start_nanos: (span.start_nanos + cue.start_nanos).min(span.end_nanos),
                    end_nanos: (span.start_nanos + cue.end_nanos).min(span.end_nanos),
                });
            }
        }
        Self::new(reel.duration_nanos(), DEFAULT_FPS, sections)
    }

    pub fn duration_nanos(&self) -> u64 {
        self.duration_nanos
    }

    pub fn duration_seconds(&self) -> f64 {
        self.duration_nanos as f64 / NANOS_PER_SEC
    }

    pub fn sections(&self) -> &[TimeSection] {
        &self.sections
    }

    /// Resolve a single point spec to clamped nanoseconds within `0..=duration_nanos`.
    pub fn resolve_nanos(&self, spec: &str) -> Result<u64> {
        self.resolve_bound(spec.trim(), Side::Point)
    }

    /// Resolve a single point spec to seconds within `0..=duration_seconds()`.
    pub fn resolve_seconds(&self, spec: &str) -> Result<f64> {
        Ok(self.resolve_nanos(spec)? as f64 / NANOS_PER_SEC)
    }

    /// Resolve a range spec (`all`, `intro`, `1.5s..4s`, `intro..outro`, `50%..`, `..2s`).
    pub fn resolve_range(&self, spec: &str) -> Result<TimeRange> {
        let spec = spec.trim();
        let (start, end) = if let Some((left, right)) = spec.split_once("..") {
            let start = if left.trim().is_empty() {
                0
            } else {
                self.resolve_bound(left.trim(), Side::Point)?
            };
            let end = if right.trim().is_empty() {
                self.duration_nanos
            } else {
                self.resolve_bound(right.trim(), Side::RangeEnd)?
            };
            (start, end)
        } else if matches!(spec, "" | "all" | "*") {
            (0, self.duration_nanos)
        } else {
            let section = self.find_section(spec)?;
            (section.start_nanos, section.end_nanos)
        };
        ensure!(
            end > start,
            "time range '{spec}' must have positive duration (resolved {:.3}s..{:.3}s)",
            start as f64 / NANOS_PER_SEC,
            end as f64 / NANOS_PER_SEC
        );
        Ok(TimeRange::new(
            Time::from_nanos(start),
            Time::from_nanos(end),
        ))
    }

    /// Resolve a comma-separated list of point specs or a `from:to:step` sweep into seconds.
    ///
    /// Note: colons inside `mm:ss` timestamps carry no third colon unless `hh:mm:ss` is used,
    /// so we distinguish `from:to:step` by checking whether all three parts resolve as points/durations
    /// when `step` has a unit or decimal, or fall back to clock parsing first on single tokens.
    pub fn resolve_times(&self, spec: &str) -> Result<Vec<f64>> {
        let spec = spec.trim();
        ensure!(!spec.is_empty(), "time specification must not be empty");
        if !spec.contains(',')
            && let Some((from, to, step)) = self.try_parse_step_sweep(spec)?
        {
            ensure!(
                step > 0 && to >= from,
                "times must be from:to:step with a positive step"
            );
            let count = (to - from) / step;
            return Ok((0..=count)
                .map(|index| (from + index * step) as f64 / NANOS_PER_SEC)
                .collect());
        }
        spec.split(',')
            .map(|part| self.resolve_seconds(part.trim()))
            .collect()
    }

    /// Spread `count` evenly spaced frame times across a range spec (`all`, `intro`, `1s..5s`),
    /// or resolve explicit comma-separated / `from:to:step` times when given.
    pub fn resolve_strip_times(&self, spec: &str, count: usize) -> Result<Vec<f64>> {
        let spec = spec.trim();
        ensure!(count >= 1, "frame count must be at least 1");
        if spec.contains(',') || self.try_parse_step_sweep(spec)?.is_some() {
            return self.resolve_times(spec);
        }
        if !matches!(spec, "" | "all" | "*")
            && !spec.contains("..")
            && self.find_section(spec).is_err()
        {
            return Ok(vec![self.resolve_seconds(spec)?]);
        }
        let range = self.resolve_range(spec)?;
        let start = range.start().as_nanos();
        let end = range.end().as_nanos();
        // Keep the last sample one 60fps frame inside the exclusive end so reel segment
        // boundaries sample the outgoing segment rather than the next segment's frame 0.
        let frame_nanos = 1_000_000_000_u64 / u64::from(self.fps);
        let inclusive_end = if end > start + frame_nanos {
            end - frame_nanos
        } else {
            start
        };
        if count == 1 {
            return Ok(vec![start as f64 / NANOS_PER_SEC]);
        }
        Ok((0..count)
            .map(|i| {
                let numer = (inclusive_end - start) as f64 * i as f64;
                let nanos = start + (numer / (count - 1) as f64).round() as u64;
                nanos.min(self.duration_nanos) as f64 / NANOS_PER_SEC
            })
            .collect())
    }

    /// Format a timestamp with its active segment or cue (`4.20s · roast@1.20s`).
    pub fn format_label(&self, seconds: f64) -> String {
        let nanos = (seconds.max(0.0) * NANOS_PER_SEC).round() as u64;
        if let Some(seg) = self
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::Segment)
            .find(|s| {
                nanos >= s.start_nanos
                    && (nanos < s.end_nanos
                        || (nanos == self.duration_nanos && s.end_nanos == self.duration_nanos))
            })
        {
            let local = nanos.saturating_sub(seg.start_nanos) as f64 / NANOS_PER_SEC;
            return format!("{seconds:.2}s · {}@{local:.2}s", seg.id);
        }
        if let Some(cue) = self
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::Cue)
            .find(|s| nanos >= s.start_nanos && nanos <= s.end_nanos)
        {
            let local = nanos.saturating_sub(cue.start_nanos) as f64 / NANOS_PER_SEC;
            return format!("{seconds:.2}s · {}@{local:.2}s", cue.id);
        }
        format!("{seconds:.2}s")
    }

    /// Active top-level segment id at `seconds`, if any.
    pub fn segment_at(&self, seconds: f64) -> Option<&TimeSection> {
        let nanos = (seconds.max(0.0) * NANOS_PER_SEC).round() as u64;
        self.sections
            .iter()
            .filter(|s| s.kind == SectionKind::Segment)
            .find(|s| {
                nanos >= s.start_nanos
                    && (nanos < s.end_nanos
                        || (nanos == self.duration_nanos && s.end_nanos == self.duration_nanos))
            })
    }

    fn try_parse_step_sweep(&self, spec: &str) -> Result<Option<(u64, u64, u64)>> {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() != 3 {
            return Ok(None);
        }
        // Distinguish `hh:mm:ss` (where middle and last are 2-digit integers without units/dots,
        // wait: `0:2:0.5` is a sweep! In `hh:mm:ss`, hours/mins are integers and middle < 60,
        // whereas in `0:2:0.5` the third part is the step; treat `a:b:c` as a sweep whenever any
        // part contains `@`, `%`, `s`, `ms`, `f`, `start`, `end`, a section name, or a decimal in `a`/`b`,
        // or when `a <= b` and `c` parses as a positive duration).
        // Psychopomp's existing `parse_times` always treated 3 colon-separated fields as `from:to:step`.
        let from = self
            .resolve_bound(parts[0].trim(), Side::Point)
            .with_context(|| format!("parse sweep start '{}'", parts[0]))?;
        let to = self
            .resolve_bound(parts[1].trim(), Side::RangeEnd)
            .with_context(|| format!("parse sweep end '{}'", parts[1]))?;
        let step = self
            .parse_duration_nanos(parts[2].trim(), self.duration_nanos)
            .with_context(|| format!("parse sweep step '{}'", parts[2]))?;
        Ok(Some((from, to, step)))
    }

    fn resolve_bound(&self, spec: &str, side: Side) -> Result<u64> {
        if let Some((section_query, offset_spec)) = spec.split_once('@') {
            let section = self.find_section(section_query.trim())?;
            let span = section.duration_nanos();
            let offset = self.resolve_within(offset_spec.trim(), span)?;
            let nanos = section.start_nanos.saturating_add(offset);
            ensure!(
                nanos <= self.duration_nanos,
                "time '{spec}' ({:.3}s) exceeds duration {:.3}s",
                nanos as f64 / NANOS_PER_SEC,
                self.duration_seconds()
            );
            return Ok(nanos);
        }

        if matches!(spec, "start" | "end") || spec.ends_with('%') {
            return self.resolve_within(spec, self.duration_nanos);
        }

        if let Ok(nanos) = self.parse_literal_nanos(spec) {
            ensure!(
                nanos <= self.duration_nanos,
                "time '{spec}' ({:.3}s) is outside 0..{:.3}s",
                nanos as f64 / NANOS_PER_SEC,
                self.duration_seconds()
            );
            return Ok(nanos);
        }

        let section = self.find_section(spec)?;
        Ok(match side {
            Side::Point => section.start_nanos,
            Side::RangeEnd => section.end_nanos,
        })
    }

    fn resolve_within(&self, spec: &str, span_nanos: u64) -> Result<u64> {
        match spec {
            "start" => Ok(0),
            "end" => Ok(span_nanos),
            _ if spec.ends_with('%') => {
                let pct: f64 = spec[..spec.len() - 1]
                    .trim()
                    .parse()
                    .with_context(|| format!("invalid percentage '{spec}'"))?;
                ensure!(
                    (0.0..=100.0).contains(&pct),
                    "percentage '{spec}' must be in 0%..=100%"
                );
                Ok(((pct / 100.0) * span_nanos as f64).round() as u64)
            }
            _ => {
                let nanos = self.parse_duration_nanos(spec, span_nanos)?;
                ensure!(
                    nanos <= span_nanos,
                    "offset '{spec}' ({:.3}s) exceeds section duration {:.3}s",
                    nanos as f64 / NANOS_PER_SEC,
                    span_nanos as f64 / NANOS_PER_SEC
                );
                Ok(nanos)
            }
        }
    }

    fn parse_duration_nanos(&self, spec: &str, _span_nanos: u64) -> Result<u64> {
        self.parse_literal_nanos(spec)
    }

    fn parse_literal_nanos(&self, spec: &str) -> Result<u64> {
        let spec = spec.trim();
        if spec.is_empty() {
            bail!("empty time specification");
        }
        if let Some(ms) = spec.strip_suffix("ms") {
            let value: f64 = ms
                .trim()
                .parse()
                .with_context(|| format!("invalid milliseconds '{spec}'"))?;
            ensure!(
                value.is_finite() && value >= 0.0,
                "time '{spec}' must be non-negative"
            );
            return Ok((value * 1_000_000.0).round() as u64);
        }
        if let Some(sec) = spec.strip_suffix('s') {
            let value: f64 = sec
                .trim()
                .parse()
                .with_context(|| format!("invalid seconds '{spec}'"))?;
            ensure!(
                value.is_finite() && value >= 0.0,
                "time '{spec}' must be non-negative"
            );
            return Ok((value * NANOS_PER_SEC).round() as u64);
        }
        if let Some(frames) = spec.strip_suffix('f') {
            let frame: u64 = frames
                .trim()
                .parse()
                .with_context(|| format!("invalid frame index '{spec}'"))?;
            return Ok(frame.saturating_mul(1_000_000_000) / u64::from(self.fps));
        }
        if let Some((mins, secs)) = spec.split_once(':')
            && !secs.contains(':')
        {
            let m: u64 = mins
                .trim()
                .parse()
                .with_context(|| format!("invalid minutes in '{spec}'"))?;
            let s: f64 = secs
                .trim()
                .parse()
                .with_context(|| format!("invalid seconds in '{spec}'"))?;
            ensure!(
                s.is_finite() && (0.0..60.0).contains(&s),
                "invalid mm:ss '{spec}'"
            );
            return Ok(m * 60 * 1_000_000_000 + (s * NANOS_PER_SEC).round() as u64);
        }
        let value: f64 = spec
            .parse()
            .with_context(|| format!("invalid time '{spec}'"))?;
        ensure!(
            value.is_finite() && value >= 0.0,
            "time '{spec}' must be non-negative"
        );
        Ok((value * NANOS_PER_SEC).round() as u64)
    }

    fn find_section(&self, query: &str) -> Result<&TimeSection> {
        if let Some(index_str) = query.strip_prefix('#') {
            let index: usize = index_str
                .parse()
                .with_context(|| format!("invalid section index '{query}'"))?;
            let top_level: Vec<&TimeSection> = self
                .sections
                .iter()
                .filter(|s| s.kind == SectionKind::Segment)
                .collect();
            let pool = if top_level.is_empty() {
                self.sections.iter().collect::<Vec<_>>()
            } else {
                top_level
            };
            return pool
                .get(index)
                .copied()
                .ok_or_else(|| self.unknown_section(query));
        }

        let (name, nth) = match query.strip_suffix(']').and_then(|q| q.split_once('[')) {
            Some((name, nth)) => (
                name,
                Some(
                    nth.parse::<usize>()
                        .with_context(|| format!("invalid occurrence index in '{query}'"))?,
                ),
            ),
            None => (query, None),
        };

        let normalize = |s: &str| {
            s.chars()
                .filter(|c| !matches!(c, '_' | '-' | ' '))
                .collect::<String>()
                .to_ascii_lowercase()
        };
        let wanted = normalize(name);
        let matches: Vec<&TimeSection> = self
            .sections
            .iter()
            .filter(|s| normalize(&s.id) == wanted)
            .collect();
        match (matches.as_slice(), nth) {
            ([], _) => Err(self.unknown_section(query)),
            ([single], None) => Ok(*single),
            (many, None) => bail!(
                "section '{query}' matches {} entries; disambiguate with {name}[0]..{name}[{}]",
                many.len(),
                many.len() - 1
            ),
            (many, Some(i)) => many
                .get(i)
                .copied()
                .ok_or_else(|| self.unknown_section(query)),
        }
    }

    fn unknown_section(&self, query: &str) -> anyhow::Error {
        let available: Vec<String> = self
            .sections
            .iter()
            .map(|s| match s.kind {
                SectionKind::Segment => format!("#{} {}", s.index, s.id),
                SectionKind::Cue => s.id.clone(),
            })
            .collect();
        if available.is_empty() {
            anyhow::anyhow!(
                "unknown time or section '{query}' (plan defines no segments or cues; use seconds like 1.5s, 500ms, 50%, start, or end)"
            )
        } else {
            anyhow::anyhow!(
                "unknown time or section '{query}'; available: {}",
                available.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        author::{PlanBuilder, seconds},
        plan::{ReelPlan, ReelSegmentPlan},
    };

    fn sample_reel() -> ReelPlan {
        let mut intro = PlanBuilder::new("intro-card", seconds(4.0));
        intro.cue("hook", seconds(0.5), seconds(2.0));
        let intro = intro.finish().unwrap();

        let mut roast = PlanBuilder::new("roast_stage", seconds(6.0));
        roast.cue("spring-bug", seconds(1.0), seconds(3.5));
        let roast = roast.finish().unwrap();

        let outro = PlanBuilder::new("outro", seconds(3.0)).finish().unwrap();

        ReelPlan::new(
            "demo",
            vec![
                ReelSegmentPlan::cut(intro),
                ReelSegmentPlan::dipped(roast, seconds(1.0)),
                ReelSegmentPlan::cut(outro),
            ],
        )
        .unwrap()
    }

    #[test]
    fn resolves_points_units_percentages_and_section_offsets() {
        let index = TimeIndex::from_reel(&sample_reel());
        // Total duration: 4.0 + (6.0 - 1.0) + 3.0 = 12.0s; roast starts at 3.0s, outro at 9.0s.
        assert_eq!(index.duration_nanos(), seconds(12.0));
        assert_eq!(index.resolve_nanos("start").unwrap(), 0);
        assert_eq!(index.resolve_nanos("end").unwrap(), seconds(12.0));
        assert_eq!(index.resolve_nanos("50%").unwrap(), seconds(6.0));
        assert_eq!(index.resolve_nanos("1.5").unwrap(), seconds(1.5));
        assert_eq!(index.resolve_nanos("1.5s").unwrap(), seconds(1.5));
        assert_eq!(index.resolve_nanos("500ms").unwrap(), seconds(0.5));
        assert_eq!(index.resolve_nanos("0:07.25").unwrap(), seconds(7.25));
        assert_eq!(index.resolve_nanos("30f").unwrap(), seconds(0.5));

        assert_eq!(index.resolve_nanos("intro-card").unwrap(), 0);
        assert_eq!(
            index.resolve_nanos("IntroCard@1.25s").unwrap(),
            seconds(1.25)
        );
        assert_eq!(index.resolve_nanos("#1").unwrap(), seconds(3.0));
        assert_eq!(
            index.resolve_nanos("roast-stage@50%").unwrap(),
            seconds(6.0)
        );
        assert_eq!(
            index.resolve_nanos("roast_stage@end").unwrap(),
            seconds(9.0)
        );
        assert_eq!(
            index.resolve_nanos("roast-stage/spring-bug@500ms").unwrap(),
            seconds(4.5)
        );
    }

    #[test]
    fn resolves_ranges_and_section_spans() {
        let index = TimeIndex::from_reel(&sample_reel());
        let roast = index.resolve_range("roast-stage").unwrap();
        assert_eq!(roast.start().as_nanos(), seconds(3.0));
        assert_eq!(roast.end().as_nanos(), seconds(9.0));

        let both = index.resolve_range("intro-card..roast-stage").unwrap();
        assert_eq!(both.start().as_nanos(), 0);
        assert_eq!(both.end().as_nanos(), seconds(9.0));

        let tail = index.resolve_range("outro@1s..").unwrap();
        assert_eq!(tail.start().as_nanos(), seconds(10.0));
        assert_eq!(tail.end().as_nanos(), seconds(12.0));
    }

    #[test]
    fn resolves_lists_sweeps_and_strip_spreads() {
        let index = TimeIndex::from_reel(&sample_reel());
        let list = index
            .resolve_times("start, roast-stage@1s, outro@50%, end")
            .unwrap();
        assert_eq!(list, vec![0.0, 4.0, 10.5, 12.0]);

        let sweep = index
            .resolve_times("roast-stage@0s:roast-stage@2s:500ms")
            .unwrap();
        assert_eq!(sweep, vec![3.0, 3.5, 4.0, 4.5, 5.0]);

        let spread = index.resolve_strip_times("outro", 3).unwrap();
        assert_eq!(spread.len(), 3);
        assert!((spread[0] - 9.0).abs() < 1e-6);
        assert!(spread[2] > 11.9 && spread[2] < 12.0);
    }
}
