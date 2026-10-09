use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::math::lerp;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LineId(String);

impl LineId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PartId(String);

impl PartId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RangeId(String);

impl RangeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyntaxStyle {
    Plain,
    Keyword,
    Type,
    String,
    Accent,
    Rgb(u8, u8, u8),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StyledSpan {
    pub text: String,
    pub style: SyntaxStyle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlinePart {
    id: PartId,
    spans: Vec<StyledSpan>,
}

impl InlinePart {
    pub fn new(id: impl Into<String>, spans: Vec<StyledSpan>) -> Self {
        Self {
            id: PartId::new(id),
            spans,
        }
    }

    pub fn id(&self) -> &PartId {
        &self.id
    }

    pub fn spans(&self) -> &[StyledSpan] {
        &self.spans
    }

    fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalOffset {
    Start,
    End,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalPosition {
    part_id: PartId,
    offset: LogicalOffset,
}

impl LogicalPosition {
    pub fn start(part_id: impl Into<String>) -> Self {
        Self {
            part_id: PartId::new(part_id),
            offset: LogicalOffset::Start,
        }
    }

    pub fn end(part_id: impl Into<String>) -> Self {
        Self {
            part_id: PartId::new(part_id),
            offset: LogicalOffset::End,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalRange {
    start: LogicalPosition,
    end: LogicalPosition,
}

impl LogicalRange {
    pub fn new(start: LogicalPosition, end: LogicalPosition) -> Self {
        Self { start, end }
    }

    pub fn spanning(first: impl Into<String>, last: impl Into<String>) -> Self {
        Self::new(LogicalPosition::start(first), LogicalPosition::end(last))
    }
}

#[derive(Clone, Debug)]
pub struct SemanticRange {
    id: RangeId,
    range: LogicalRange,
}

impl SemanticRange {
    pub fn new(id: impl Into<String>, range: LogicalRange) -> Self {
        Self {
            id: RangeId::new(id),
            range,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedLogicalRange {
    pub start_part: usize,
    pub start_byte: usize,
    pub end_part: usize,
    pub end_byte: usize,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>, style: SyntaxStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLine {
    pub id: LineId,
    parts: Vec<InlinePart>,
    spans: Vec<StyledSpan>,
    part_spans: Vec<Range<usize>>,
    ranges: HashMap<RangeId, LogicalRange>,
}

impl CodeLine {
    pub fn new(id: impl Into<String>, spans: Vec<StyledSpan>) -> Self {
        Self::with_parts(id, vec![InlinePart::new("content", spans)], [])
            .expect("one legacy content part is valid")
    }

    pub fn with_parts(
        id: impl Into<String>,
        parts: Vec<InlinePart>,
        ranges: impl IntoIterator<Item = SemanticRange>,
    ) -> Result<Self> {
        let id = LineId::new(id);
        let mut part_ids = HashSet::new();
        let mut spans = Vec::new();
        let mut part_spans = Vec::with_capacity(parts.len());
        for part in &parts {
            if part.id.as_str().is_empty() || part.id.as_str().chars().any(char::is_whitespace) {
                bail!(
                    "code line '{}' has invalid inline part ID '{}'",
                    id.as_str(),
                    part.id.as_str()
                );
            }
            if !part_ids.insert(part.id.clone()) {
                bail!(
                    "code line '{}' repeats inline part '{}'",
                    id.as_str(),
                    part.id.as_str()
                );
            }
            let start = spans.len();
            spans.extend(part.spans.iter().cloned());
            part_spans.push(start..spans.len());
        }
        let mut line = Self {
            id,
            parts,
            spans,
            part_spans,
            ranges: HashMap::new(),
        };
        for semantic in ranges {
            if semantic.id.as_str().is_empty()
                || semantic.id.as_str().chars().any(char::is_whitespace)
            {
                bail!(
                    "code line '{}' has invalid semantic range ID '{}'",
                    line.id.as_str(),
                    semantic.id.as_str()
                );
            }
            line.resolve_logical_range(&semantic.range)?;
            if line
                .ranges
                .insert(semantic.id.clone(), semantic.range)
                .is_some()
            {
                bail!(
                    "code line '{}' repeats semantic range '{}'",
                    line.id.as_str(),
                    semantic.id.as_str()
                );
            }
        }
        Ok(line)
    }

    pub fn spans(&self) -> &[StyledSpan] {
        &self.spans
    }

    pub fn parts(&self) -> &[InlinePart] {
        &self.parts
    }

    pub fn semantic_range(&self, id: &RangeId) -> Option<&LogicalRange> {
        self.ranges.get(id)
    }

    pub fn semantic_span_range(&self, id: &RangeId) -> Result<Range<usize>> {
        let logical = self.ranges.get(id).with_context(|| {
            format!(
                "code line '{}' has no semantic range '{}'",
                self.id.as_str(),
                id.as_str()
            )
        })?;
        let resolved = self.resolve_logical_range(logical)?;
        if resolved.start_byte != 0
            || resolved.end_byte != self.parts[resolved.end_part].text().len()
        {
            bail!(
                "semantic range '{}' on line '{}' is not aligned to whole inline parts",
                id.as_str(),
                self.id.as_str()
            );
        }
        Ok(self.part_spans[resolved.start_part].start..self.part_spans[resolved.end_part].end)
    }

    pub fn resolve_logical_range(&self, range: &LogicalRange) -> Result<ResolvedLogicalRange> {
        let start_part = self
            .parts
            .iter()
            .position(|part| part.id == range.start.part_id)
            .with_context(|| {
                format!(
                    "code line '{}' has no inline part '{}'",
                    self.id.as_str(),
                    range.start.part_id.as_str()
                )
            })?;
        let end_part = self
            .parts
            .iter()
            .position(|part| part.id == range.end.part_id)
            .with_context(|| {
                format!(
                    "code line '{}' has no inline part '{}'",
                    self.id.as_str(),
                    range.end.part_id.as_str()
                )
            })?;
        if start_part > end_part {
            bail!("logical range reverses inline part order");
        }
        let start_byte = resolve_offset(&self.parts[start_part], range.start.offset);
        let end_byte = resolve_offset(&self.parts[end_part], range.end.offset);
        let selects_text = if start_part == end_part {
            start_byte < end_byte
        } else {
            start_byte < self.parts[start_part].text().len()
                || self.parts[start_part + 1..end_part]
                    .iter()
                    .any(|part| !part.text().is_empty())
                || end_byte > 0
        };
        if !selects_text {
            bail!("logical range must select non-empty text");
        }
        Ok(ResolvedLogicalRange {
            start_part,
            start_byte,
            end_part,
            end_byte,
        })
    }
}

fn resolve_offset(part: &InlinePart, offset: LogicalOffset) -> usize {
    match offset {
        LogicalOffset::Start => 0,
        LogicalOffset::End => part.text().len(),
    }
}

pub struct CodeDocument {
    lines: HashMap<LineId, CodeLine>,
}

impl CodeDocument {
    pub fn new(lines: Vec<CodeLine>) -> Result<Self> {
        let mut by_id = HashMap::with_capacity(lines.len());
        for line in lines {
            let id = line.id.clone();
            if by_id.insert(id.clone(), line).is_some() {
                bail!("duplicate code line id '{}'", id.as_str());
            }
        }
        Ok(Self { lines: by_id })
    }

    pub(crate) fn line(&self, id: &LineId) -> Option<&CodeLine> {
        self.lines.get(id)
    }

    pub(crate) fn validate_snapshot(&self, snapshot: &CodeSnapshot) -> Result<()> {
        let mut seen = HashSet::with_capacity(snapshot.order.len());
        for id in &snapshot.order {
            if !self.lines.contains_key(id) {
                bail!("snapshot references unknown code line '{}'", id.as_str());
            }
            if !seen.insert(id) {
                bail!("snapshot repeats code line '{}'", id.as_str());
            }
        }
        Ok(())
    }
}

pub struct CodeSnapshot {
    order: Vec<LineId>,
}

impl CodeSnapshot {
    pub fn new<I>(order: impl IntoIterator<Item = I>) -> Self
    where
        I: Into<String>,
    {
        Self {
            order: order.into_iter().map(LineId::new).collect(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct CodeLayout {
    pub line_height: f32,
    pub entering_offset_x: f32,
}

struct LineTrack {
    line: CodeLine,
    from_row: Option<usize>,
    to_row: Option<usize>,
}

impl LineTrack {
    fn sample(&self, layout: CodeLayout, progress: TransitionProgress) -> PlacedLine<'_> {
        let (x, row, opacity) = match (self.from_row, self.to_row) {
            (Some(from), Some(to)) => (0.0, lerp(from as f32, to as f32, progress.layout), 1.0),
            (None, Some(to)) => (
                layout.entering_offset_x * (1.0 - progress.content),
                to as f32,
                progress.content.clamp(0.0, 1.0),
            ),
            (Some(from), None) => (
                -layout.entering_offset_x * progress.content,
                from as f32,
                (1.0 - progress.content).clamp(0.0, 1.0),
            ),
            (None, None) => unreachable!("line track must appear in at least one snapshot"),
        };
        PlacedLine {
            line: &self.line,
            x,
            y: row * layout.line_height,
            opacity,
            blur: (1.0 - opacity) * 4.0,
        }
    }
}

pub struct CodeTransition {
    tracks: Vec<LineTrack>,
    layout: CodeLayout,
}

#[derive(Clone, Copy)]
pub struct TransitionProgress {
    pub layout: f32,
    pub content: f32,
}

pub struct PlacedLine<'a> {
    pub line: &'a CodeLine,
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
    pub blur: f32,
}

impl CodeTransition {
    pub fn compile(
        document: &CodeDocument,
        before: &CodeSnapshot,
        after: &CodeSnapshot,
        layout: CodeLayout,
    ) -> Result<Self> {
        document.validate_snapshot(before)?;
        document.validate_snapshot(after)?;

        let before_rows: HashMap<_, _> = before
            .order
            .iter()
            .enumerate()
            .map(|(row, id)| (id.clone(), row))
            .collect();
        let after_rows: HashMap<_, _> = after
            .order
            .iter()
            .enumerate()
            .map(|(row, id)| (id.clone(), row))
            .collect();

        let mut manifest = after.order.clone();
        manifest.extend(
            before
                .order
                .iter()
                .filter(|id| !after_rows.contains_key(*id))
                .cloned(),
        );

        let tracks = manifest
            .into_iter()
            .map(|id| LineTrack {
                line: document.lines[&id].clone(),
                from_row: before_rows.get(&id).copied(),
                to_row: after_rows.get(&id).copied(),
            })
            .collect();

        Ok(Self { tracks, layout })
    }

    pub fn sample(&self, progress: TransitionProgress) -> Vec<PlacedLine<'_>> {
        self.tracks
            .iter()
            .map(|track| track.sample(self.layout, progress))
            .collect()
    }

    pub(crate) fn sample_line(
        &self,
        id: &LineId,
        progress: TransitionProgress,
    ) -> Option<PlacedLine<'_>> {
        self.tracks
            .iter()
            .find(|track| &track.line.id == id)
            .map(|track| track.sample(self.layout, progress))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodeDocument, CodeLayout, CodeLine, CodeSnapshot, CodeTransition, InlinePart, LogicalRange,
        SemanticRange, StyledSpan, SyntaxStyle, TransitionProgress,
    };

    fn line(id: &str) -> CodeLine {
        CodeLine::new(id, vec![StyledSpan::new(id, SyntaxStyle::Plain)])
    }

    #[test]
    fn inserted_lines_move_in_without_replacing_stable_lines() {
        let document = CodeDocument::new(vec![line("a"), line("insert"), line("b")]).unwrap();
        let transition = CodeTransition::compile(
            &document,
            &CodeSnapshot::new(["a", "b"]),
            &CodeSnapshot::new(["a", "insert", "b"]),
            CodeLayout {
                line_height: 44.0,
                entering_offset_x: 100.0,
            },
        )
        .unwrap();

        let before = transition.sample(TransitionProgress {
            layout: 0.0,
            content: 0.0,
        });
        let after = transition.sample(TransitionProgress {
            layout: 1.0,
            content: 1.0,
        });
        let before_b = before
            .iter()
            .find(|line| line.line.id.as_str() == "b")
            .unwrap();
        let after_b = after
            .iter()
            .find(|line| line.line.id.as_str() == "b")
            .unwrap();
        let inserted_before = before
            .iter()
            .find(|line| line.line.id.as_str() == "insert")
            .unwrap();

        assert_eq!(before_b.y, 44.0);
        assert_eq!(after_b.y, 88.0);
        assert_eq!(inserted_before.x, 100.0);
        assert_eq!(inserted_before.opacity, 0.0);
    }

    #[test]
    fn stable_inline_parts_own_flattened_span_order() {
        let line = CodeLine::with_parts(
            "definition",
            vec![
                InlinePart::new(
                    "prefix",
                    vec![StyledSpan::new("const value: ", SyntaxStyle::Plain)],
                ),
                InlinePart::new(
                    "promise",
                    vec![StyledSpan::new("Promise<A>", SyntaxStyle::Type)],
                ),
                InlinePart::new(
                    "effect",
                    vec![StyledSpan::new("Effect<A>", SyntaxStyle::Type)],
                ),
                InlinePart::new(
                    "suffix",
                    vec![StyledSpan::new(" = run()", SyntaxStyle::Plain)],
                ),
            ],
            [SemanticRange::new(
                "types",
                LogicalRange::spanning("promise", "effect"),
            )],
        )
        .unwrap();

        assert_eq!(line.parts().len(), 4);
        assert_eq!(
            line.spans()
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "const value: Promise<A>Effect<A> = run()"
        );
        assert_eq!(
            line.semantic_span_range(&super::RangeId::new("types"))
                .unwrap(),
            1..3
        );
    }

    #[test]
    fn logical_ranges_validate_part_identity_and_selected_text() {
        let duplicate = CodeLine::with_parts(
            "line",
            vec![
                InlinePart::new("value", vec![StyledSpan::new("a", SyntaxStyle::Plain)]),
                InlinePart::new("value", vec![StyledSpan::new("b", SyntaxStyle::Plain)]),
            ],
            [],
        )
        .err()
        .unwrap();
        assert!(
            duplicate
                .to_string()
                .contains("repeats inline part 'value'")
        );

        let empty_across_parts = CodeLine::with_parts(
            "line",
            vec![
                InlinePart::new("left", vec![StyledSpan::new("a", SyntaxStyle::Plain)]),
                InlinePart::new("right", vec![StyledSpan::new("b", SyntaxStyle::Plain)]),
            ],
            [SemanticRange::new(
                "empty",
                super::LogicalRange::new(
                    super::LogicalPosition::end("left"),
                    super::LogicalPosition::start("right"),
                ),
            )],
        )
        .err()
        .unwrap();
        assert!(
            empty_across_parts
                .to_string()
                .contains("must select non-empty text")
        );
    }
}
