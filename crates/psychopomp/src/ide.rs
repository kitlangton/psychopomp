//! IDE annotations over an editor's Semantic Targets, for code explainers:
//! a Diagnostic (a severity wave drawn on along a range, with a gutter icon),
//! a Hover Card (an IDE tooltip pinned to a range), a Cursor (a blinking caret
//! and its selection), and an Inlay Hint (ghost text revealed inline after a
//! range). Each is attached to the editor root by target ID, so the renderer
//! measures it at every sample and it follows line motion, Inline Reveals, and
//! the panel's projection. This module owns payloads, validation, the wave,
//! blink, and hover-card geometry, and the authoring handles; no GPU.
use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    author::{ActorHandle, ContinuousHandle, PlanBuilder},
    caption::CaptionSpanPlan,
    code::{StyledSpan, SyntaxStyle},
    editor::{
        EditorInlineRevealPlan, EditorLinePlan, EditorPartPlan, EditorRecipePlan,
        EditorSemanticRangePlan,
    },
    highlight,
    math::{Vec2, curve::Polyline, easing::Ease, shapes::Box2, smoothstep, vec2},
    plan::SpringPlan,
    stage::DRAW_CURVE,
    tone::Tone,
};

pub const DIAGNOSTIC_RECIPE: &str = "diagnostic";
pub const HOVER_RECIPE: &str = "hover-card";
pub const CURSOR_RECIPE: &str = "cursor";

/// Inline parts whose IDs start with this are Inlay Hints: ghost text drawn
/// dim on a faint chip. [`insert_inlay`] creates them.
pub const INLAY_PART_PREFIX: &str = "inlay:";

/// How serious a Diagnostic is: its wave and gutter icon take the tone.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    #[default]
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn tone(self) -> Tone {
        match self {
            Self::Error => Tone::Error,
            Self::Warning => Tone::Warning,
            Self::Info => Tone::Request,
        }
    }
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.chars().any(|c| c.is_whitespace() || c == '.')
}

/// A wave under one Semantic Target. Channels: `draw` (0..1 of its length,
/// left to right), `opacity`, and `wave` (amplitude, 1; 0 lies flat).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticPlan {
    pub target: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub severity: Severity,
    /// A severity icon in the gutter beside the line.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gutter: bool,
}

impl DiagnosticPlan {
    pub fn new(target: impl Into<String>, severity: Severity) -> Self {
        Self {
            target: target.into(),
            severity,
            gutter: true,
        }
    }

    pub fn error(target: impl Into<String>) -> Self {
        Self::new(target, Severity::Error)
    }

    pub fn warning(target: impl Into<String>) -> Self {
        Self::new(target, Severity::Warning)
    }

    pub fn info(target: impl Into<String>) -> Self {
        Self::new(target, Severity::Info)
    }

    pub fn without_gutter(mut self) -> Self {
        self.gutter = false;
        self
    }

    pub fn accepts(&self, property: &str) -> bool {
        matches!(property, "draw" | "opacity" | "wave")
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.target.is_empty(),
            "a diagnostic needs a semantic target"
        );
        Ok(())
    }
}

/// Where a Hover Card sits relative to its range.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HoverSide {
    #[default]
    Above,
    Below,
}

/// One block of a Hover Card; consecutive blocks are separated by a rule.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HoverSectionPlan {
    /// Monospaced, syntax-highlighted lines, such as a type signature.
    Code(Vec<Vec<StyledSpan>>),
    /// Prose lines of toned spans, such as a diagnostic message.
    Text(Vec<Vec<CaptionSpanPlan>>),
}

impl HoverSectionPlan {
    pub fn lines(&self) -> usize {
        match self {
            Self::Code(lines) => lines.len(),
            Self::Text(lines) => lines.len(),
        }
    }
}

/// An IDE tooltip pinned to a Semantic Target, with a small pointer toward it.
/// Channel: `presence` (0 hidden, 1 shown; the card fades in and rises
/// `RISE` pixels away from its range).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoverPlan {
    pub target: String,
    pub sections: Vec<HoverSectionPlan>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub side: HoverSide,
}

/// Hover Card text sizes, in editor pixels (code is 28).
pub const HOVER_CODE_SIZE: f32 = 22.0;
pub const HOVER_CODE_LINE: f32 = 32.0;
pub const HOVER_TEXT_SIZE: f32 = 20.0;
pub const HOVER_TEXT_LINE: f32 = 30.0;
/// How far a hover rises into place.
pub const HOVER_RISE: f32 = 8.0;

impl HoverPlan {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            sections: Vec::new(),
            side: HoverSide::Above,
        }
    }

    /// A highlighted TypeScript line, joining a preceding code block.
    pub fn code(mut self, line: &str) -> Self {
        let spans = highlight::typescript(line);
        match self.sections.last_mut() {
            Some(HoverSectionPlan::Code(lines)) => lines.push(spans),
            _ => self.sections.push(HoverSectionPlan::Code(vec![spans])),
        }
        self
    }

    /// A prose line, joining a preceding text block.
    pub fn text(mut self, spans: Vec<CaptionSpanPlan>) -> Self {
        match self.sections.last_mut() {
            Some(HoverSectionPlan::Text(lines)) => lines.push(spans),
            _ => self.sections.push(HoverSectionPlan::Text(vec![spans])),
        }
        self
    }

    /// Start a new block of the same kind after a rule.
    pub fn rule(mut self) -> Self {
        let empty = match self.sections.last() {
            Some(HoverSectionPlan::Code(_)) => HoverSectionPlan::Code(Vec::new()),
            _ => HoverSectionPlan::Text(Vec::new()),
        };
        self.sections.push(empty);
        self
    }

    pub fn below(mut self) -> Self {
        self.side = HoverSide::Below;
        self
    }

    pub fn accepts(&self, property: &str) -> bool {
        property == "presence"
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.target.is_empty(),
            "a hover card needs a semantic target"
        );
        ensure!(
            (1..=4).contains(&self.sections.len()),
            "a hover card has one to four sections"
        );
        let mut lines = 0;
        for section in &self.sections {
            ensure!(section.lines() > 0, "hover card sections cannot be empty");
            lines += section.lines();
            let texts: Vec<String> = match section {
                HoverSectionPlan::Code(code) => code
                    .iter()
                    .map(|line| line.iter().map(|span| span.text.as_str()).collect())
                    .collect(),
                HoverSectionPlan::Text(text) => text
                    .iter()
                    .map(|line| line.iter().map(|span| span.text.as_str()).collect())
                    .collect(),
            };
            for text in texts {
                ensure!(!text.is_empty(), "hover card lines cannot be empty");
                ensure!(
                    !text.contains(['\n', '\r']),
                    "hover card lines are single-line; add another line instead"
                );
                ensure!(
                    text.chars().count() <= 96,
                    "hover card lines are limited to 96 characters"
                );
            }
        }
        ensure!(lines <= 10, "a hover card has at most ten lines");
        Ok(())
    }
}

/// A Hover Card placed beside its range: the card's box and where its pointer
/// meets the card edge (`tip` is the pointer's point, at the range).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoverLayout {
    pub card: Box2,
    pub base_x: f32,
    pub tip: Vec2,
    pub below: bool,
}

/// The pointer's height and half its base width.
pub const HOVER_TIP: Vec2 = Vec2::new(8.0, 9.0);

/// Place a `size` card on `side` of `range`, `gap` pixels beyond it, with its
/// left edge near the range's start as an IDE aligns hovers. The card slides
/// to stay inside `bounds` rather than flipping sides, so a moving range moves
/// its card continuously; the pointer keeps aiming at the range's center.
pub fn hover_layout(
    range: Box2,
    size: Vec2,
    side: HoverSide,
    gap: f32,
    bounds: Box2,
) -> HoverLayout {
    let above_y = range.min.y - gap - HOVER_TIP.y - size.y;
    let below_y = range.max.y + gap + HOVER_TIP.y;
    let below = match side {
        HoverSide::Above
            if above_y < bounds.min.y
                && bounds.min.y + size.y + HOVER_TIP.y > range.min.y
                && below_y + size.y <= bounds.max.y =>
        {
            true
        }
        HoverSide::Below
            if below_y + size.y > bounds.max.y
                && (bounds.max.y - size.y).max(bounds.min.y) - HOVER_TIP.y < range.max.y
                && above_y >= bounds.min.y =>
        {
            false
        }
        HoverSide::Above => false,
        HoverSide::Below => true,
    };
    let x = (range.min.x - 14.0).clamp(bounds.min.x, (bounds.max.x - size.x).max(bounds.min.x));
    let y = if below { below_y } else { above_y };
    let y = y.clamp(bounds.min.y, (bounds.max.y - size.y).max(bounds.min.y));
    let card = Box2 {
        min: vec2(x, y),
        max: vec2(x, y) + size,
    };
    let margin = 10.0 + HOVER_TIP.x;
    let base_x = range.center().x.clamp(
        card.min.x + margin,
        (card.max.x - margin).max(card.min.x + margin),
    );
    let edge = if below { card.min.y } else { card.max.y };
    let tip = vec2(
        base_x,
        edge + if below { -HOVER_TIP.y } else { HOVER_TIP.y },
    );
    HoverLayout {
        card,
        base_x,
        tip,
        below,
    }
}

/// One place a Cursor can be: a Semantic Target, with the caret at `head`
/// (0 its start, 1 its end) and the selection between `tail` and `head`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CursorAnchorPlan {
    pub id: String,
    pub target: String,
}

/// A text caret and its selection. Channels: `opacity`, `head` and `tail`
/// (fractions of the weighted range; `tail` defaults to `head`, no
/// selection), `blink` (seconds since the caret last moved; -1 holds it
/// solid), and `anchor.<id>` weights (1 for the first anchor).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CursorPlan {
    pub anchors: Vec<CursorAnchorPlan>,
}

impl CursorPlan {
    pub fn new(id: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            anchors: vec![CursorAnchorPlan {
                id: id.into(),
                target: target.into(),
            }],
        }
    }

    pub fn anchor(mut self, id: impl Into<String>, target: impl Into<String>) -> Self {
        self.anchors.push(CursorAnchorPlan {
            id: id.into(),
            target: target.into(),
        });
        self
    }

    pub fn weight_property(anchor: &str) -> String {
        format!("anchor.{anchor}")
    }

    pub fn accepts(&self, property: &str) -> bool {
        matches!(property, "opacity" | "head" | "tail" | "blink")
            || property
                .strip_prefix("anchor.")
                .is_some_and(|id| self.anchors.iter().any(|anchor| anchor.id == id))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=8).contains(&self.anchors.len()),
            "a cursor has one to eight anchors"
        );
        let mut ids = HashSet::new();
        for anchor in &self.anchors {
            ensure!(
                valid_id(&anchor.id),
                "cursor anchor ID '{}' must be non-empty, without whitespace or dots",
                anchor.id
            );
            ensure!(
                ids.insert(anchor.id.as_str()),
                "cursor anchor '{}' is declared twice",
                anchor.id
            );
            ensure!(
                !anchor.target.is_empty(),
                "cursor anchor '{}' needs a semantic target",
                anchor.id
            );
        }
        Ok(())
    }
}

/// The caret's opacity `elapsed` seconds after it last moved: solid for half
/// a second, then a one-second blink with short eased fades. A pure function
/// of the `blink` clock, so any sample renders alike; negative holds solid.
pub fn caret_blink(elapsed: f32) -> f32 {
    const SOLID: f32 = 0.5;
    const PERIOD: f32 = 1.0;
    const FADE: f32 = 0.09;
    if !elapsed.is_finite() || elapsed < SOLID {
        return 1.0;
    }
    let phase = (elapsed - SOLID).rem_euclid(PERIOD);
    let on = PERIOD * 0.5;
    if phase < on - FADE {
        1.0
    } else if phase < on {
        1.0 - smoothstep((phase - (on - FADE)) / FADE)
    } else if phase < PERIOD - FADE {
        0.0
    } else {
        smoothstep((phase - (PERIOD - FADE)) / FADE)
    }
}

/// The wavelength and amplitude of a diagnostic wave, in editor pixels.
pub const WAVELENGTH: f32 = 8.0;
pub const AMPLITUDE: f32 = 2.1;

/// A wavy underline from `start` (its left end, on the wave's center line)
/// along `length` pixels. Its phase is measured from `start`, so the wave
/// keeps its shape as its range moves, and it ends exactly at the range end.
pub fn wave(start: Vec2, length: f32, amplitude: f32, wavelength: f32) -> Polyline {
    let length = length.max(0.0);
    let step = wavelength / 8.0;
    let count = (length / step).ceil().max(1.0) as usize;
    Polyline::new(
        (0..=count)
            .map(|index| {
                let x = (index as f32 * step).min(length);
                let y = -amplitude * (x / wavelength * std::f32::consts::TAU).sin();
                start + vec2(x, y)
            })
            .collect(),
    )
}

/// The part ID of Inlay Hint `id`.
pub fn inlay_part(id: &str) -> String {
    format!("{INLAY_PART_PREFIX}{id}")
}

/// The editor channel that reveals Inlay Hint `id` (0 collapsed, 1 open).
pub fn inlay_channel(id: &str) -> String {
    format!("inlay.{id}")
}

/// Muted ghost text for an Inlay Hint. The neutral gray maps to every
/// theme's muted ink.
pub fn ghost(text: &str) -> Vec<StyledSpan> {
    vec![StyledSpan::new(text, SyntaxStyle::Rgb(140, 146, 158))]
}

/// Insert Inlay Hint `id` into `line` after the last part of semantic range
/// `after`, as its own inline part and semantic range, and return the Inline
/// Reveal that opens it on [`inlay_channel`]. The line keeps its identity;
/// only the hint's room and ink are revealed.
pub fn insert_inlay(
    line: &mut EditorLinePlan,
    id: &str,
    after: &str,
    spans: Vec<StyledSpan>,
) -> Result<EditorInlineRevealPlan> {
    ensure!(
        valid_id(id),
        "inlay ID '{id}' must be non-empty, without whitespace or dots"
    );
    ensure!(
        spans.iter().any(|span| !span.text.is_empty()),
        "inlay '{id}' needs text"
    );
    let range = line
        .semantic_ranges
        .iter()
        .find(|range| range.id == after)
        .with_context(|| format!("line '{}' has no semantic range '{after}'", line.id))?;
    let index = line
        .parts
        .iter()
        .position(|part| part.id == range.last_part_id)
        .with_context(|| format!("line '{}' has no part '{}'", line.id, range.last_part_id))?;
    let part = inlay_part(id);
    if line.parts.iter().any(|existing| existing.id == part) {
        bail!("line '{}' already has inlay '{id}'", line.id);
    }
    line.parts.insert(
        index + 1,
        EditorPartPlan {
            id: part.clone(),
            spans,
        },
    );
    line.semantic_ranges.push(EditorSemanticRangePlan {
        id: part.clone(),
        first_part_id: part.clone(),
        last_part_id: part.clone(),
    });
    Ok(EditorInlineRevealPlan {
        line_id: line.id.clone(),
        range_id: part,
        channel: Some(inlay_channel(id)),
        reversed: false,
    })
}

impl EditorRecipePlan {
    /// Insert Inlay Hint `id` after semantic range `after` on `line_id`; see
    /// [`insert_inlay`]. Reveal it with [`InlayHint`].
    pub fn insert_inlay(
        &mut self,
        id: &str,
        line_id: &str,
        after: &str,
        spans: Vec<StyledSpan>,
    ) -> Result<()> {
        let line = self
            .lines
            .iter_mut()
            .find(|line| line.id == line_id)
            .with_context(|| format!("editor has no line '{line_id}'"))?;
        let reveal = insert_inlay(line, id, after, spans)?;
        self.additional_inline_reveals.push(reveal);
        Ok(())
    }
}

/// Authoring handle for one Inlay Hint's reveal channel on its editor.
#[derive(Clone, Debug)]
pub struct InlayHint {
    channel: ContinuousHandle,
}

impl InlayHint {
    pub fn on(scene: &mut PlanBuilder, editor: &ActorHandle, id: &str) -> Self {
        Self {
            channel: scene.channel(editor, &inlay_channel(id), 0.0),
        }
    }

    pub fn channel(&self) -> &ContinuousHandle {
        &self.channel
    }

    /// The hint opens its room inline and its ghost text resolves.
    pub fn show(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        scene.spring(&self.channel, at_nanos, 1.0, 0.6, 0.0);
    }

    pub fn hide(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        scene.spring(&self.channel, at_nanos, 0.0, 0.4, 0.0);
    }
}

/// Authoring handle for one Diagnostic.
#[derive(Clone, Debug)]
pub struct DiagnosticActor {
    actor: ActorHandle,
}

const WAVE_DRAW_SECONDS: f32 = 0.55;

impl DiagnosticActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &DiagnosticPlan,
    ) -> Result<Self> {
        plan.validate()?;
        Ok(Self {
            actor: scene.actor(id, DIAGNOSTIC_RECIPE, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    pub fn channel(
        &self,
        scene: &mut PlanBuilder,
        property: &str,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, property, initial)
    }

    /// The gutter icon pops in and the wave draws on along the range. A
    /// diagnostic with a `show` starts hidden. Returns when the wave is drawn.
    pub fn show(&self, scene: &mut PlanBuilder, at_nanos: u64) -> u64 {
        let draw = self.channel(scene, "draw", 0.0);
        scene.ease(&draw, at_nanos, 1.0, WAVE_DRAW_SECONDS, DRAW_CURVE);
        at_nanos + crate::author::whole_millis(WAVE_DRAW_SECONDS)
    }

    /// Fixed: the wave relaxes flat as it fades, and the gutter icon goes.
    pub fn clear(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let wave = self.channel(scene, "wave", 1.0);
        let opacity = self.channel(scene, "opacity", 1.0);
        scene.spring(&wave, at_nanos, 0.0, 0.35, 0.0);
        scene.ease(&opacity, at_nanos + 120_000_000, 0.0, 0.32, Ease::CubicOut);
    }
}

/// Authoring handle for one Hover Card.
#[derive(Clone, Debug)]
pub struct HoverActor {
    actor: ActorHandle,
}

impl HoverActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &HoverPlan,
    ) -> Result<Self> {
        plan.validate()?;
        Ok(Self {
            actor: scene.actor(id, HOVER_RECIPE, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    /// The card pops in: it fades up and rises into place with a slight
    /// overshoot. A hover with a `show` starts hidden.
    pub fn show(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let presence = scene.channel(&self.actor, "presence", 0.0);
        scene.spring_with(&presence, at_nanos, 1.0, SpringPlan::POP);
    }

    pub fn hide(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let presence = scene.channel(&self.actor, "presence", 1.0);
        scene.spring(&presence, at_nanos, 0.0, 0.2, 0.0);
    }
}

/// Authoring handle for one Cursor. Channels are declared on first use.
#[derive(Clone, Debug)]
pub struct CursorActor {
    actor: ActorHandle,
    anchors: Vec<String>,
}

impl CursorActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &CursorPlan,
    ) -> Result<Self> {
        plan.validate()?;
        Ok(Self {
            actor: scene.actor(id, CURSOR_RECIPE, plan)?,
            anchors: plan
                .anchors
                .iter()
                .map(|anchor| anchor.id.clone())
                .collect(),
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    pub fn channel(
        &self,
        scene: &mut PlanBuilder,
        property: &str,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, property, initial)
    }

    /// The caret holds solid while it moves.
    fn hold(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let blink = self.channel(scene, "blink", -1.0);
        scene.set(&blink, at_nanos, -1.0);
    }

    /// Restart the blink: the caret holds solid, then blinks until it moves.
    fn rest(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let blink = self.channel(scene, "blink", -1.0);
        let seconds = (scene.duration_nanos().saturating_sub(at_nanos) / 1_000_000) as f32 / 1000.0;
        scene.set(&blink, at_nanos, 0.0);
        if seconds > 0.0 {
            scene.ease(&blink, at_nanos, seconds, seconds, Ease::Linear);
        }
    }

    /// The caret appears where it is and starts blinking.
    pub fn show(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let opacity = self.channel(scene, "opacity", 0.0);
        scene.spring(&opacity, at_nanos, 1.0, 0.16, 0.0);
        self.rest(scene, at_nanos);
    }

    /// The caret fades and stops blinking.
    pub fn hide(&self, scene: &mut PlanBuilder, at_nanos: u64) {
        let opacity = self.channel(scene, "opacity", 1.0);
        let blink = self.channel(scene, "blink", -1.0);
        scene.set(&blink, at_nanos, -1.0);
        scene.spring(&opacity, at_nanos, 0.0, 0.16, 0.0);
    }

    fn weights(&self, scene: &mut PlanBuilder, anchor: &str, at_nanos: u64) -> Result<()> {
        ensure!(
            self.anchors.iter().any(|id| id == anchor),
            "cursor '{}' has no anchor '{anchor}'",
            self.actor.id()
        );
        self.hold(scene, at_nanos);
        let spring = SpringPlan::visual(0.32, 0.0).with_thresholds(1e-5, 1e-5);
        for (index, id) in self.anchors.clone().iter().enumerate() {
            let initial = if index == 0 { 1.0 } else { 0.0 };
            let weight = self.channel(scene, &CursorPlan::weight_property(id), initial);
            scene.spring_with(&weight, at_nanos, f32::from(id == anchor), spring);
        }
        Ok(())
    }

    /// Glide the caret to the start (`head` 0) or end (1) of `anchor`,
    /// collapsing any selection.
    pub fn move_to(
        &self,
        scene: &mut PlanBuilder,
        anchor: &str,
        head: f32,
        at_nanos: u64,
    ) -> Result<()> {
        self.weights(scene, anchor, at_nanos)?;
        let head_channel = self.channel(scene, "head", 1.0);
        let tail = self.channel(scene, "tail", 1.0);
        scene.spring(&head_channel, at_nanos, head, 0.32, 0.0);
        scene.spring(&tail, at_nanos, head, 0.32, 0.0);
        self.rest(scene, at_nanos + 320_000_000);
        Ok(())
    }

    /// Select `anchor`'s whole range: the caret goes to its start, then sweeps
    /// to its end over `seconds` as the selection grows behind it. Returns
    /// when the selection is complete.
    pub fn select(
        &self,
        scene: &mut PlanBuilder,
        anchor: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> Result<u64> {
        self.weights(scene, anchor, at_nanos)?;
        let head = self.channel(scene, "head", 1.0);
        let tail = self.channel(scene, "tail", 1.0);
        scene.spring(&head, at_nanos, 0.0, 0.3, 0.0);
        scene.spring(&tail, at_nanos, 0.0, 0.3, 0.0);
        let sweep = at_nanos + 300_000_000;
        scene.ease(&head, sweep, 1.0, seconds, Ease::GLIDE);
        let done = sweep + crate::author::whole_millis(seconds);
        self.rest(scene, done);
        Ok(done)
    }

    /// The selection shrinks into the caret.
    pub fn collapse(&self, scene: &mut PlanBuilder, at_nanos: u64, head: f32) {
        let tail = self.channel(scene, "tail", 1.0);
        scene.spring(&tail, at_nanos, head, 0.25, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor::EditorTargetSelector, plan::ScalarPlan};

    fn line() -> EditorLinePlan {
        EditorLinePlan {
            id: "program".into(),
            parts: vec![
                EditorPartPlan {
                    id: "lead".into(),
                    spans: highlight::typescript("const "),
                },
                EditorPartPlan {
                    id: "name".into(),
                    spans: highlight::typescript("program"),
                },
                EditorPartPlan {
                    id: "rest".into(),
                    spans: highlight::typescript(" = Effect.succeed(1)"),
                },
            ],
            semantic_ranges: vec![
                EditorSemanticRangePlan {
                    id: "name".into(),
                    first_part_id: "name".into(),
                    last_part_id: "name".into(),
                },
                EditorSemanticRangePlan {
                    id: "rest".into(),
                    first_part_id: "rest".into(),
                    last_part_id: "rest".into(),
                },
            ],
            mark: None,
        }
    }

    fn recipe() -> EditorRecipePlan {
        EditorRecipePlan {
            file_name: "program.ts".into(),
            lines: vec![line()],
            initial_line_ids: vec!["program".into()],
            final_line_ids: vec!["program".into()],
            snapshots: Vec::new(),
            line_height: 44.0,
            entering_offset_x: 0.0,
            focus_line_id: "program".into(),
            focus_height: 44.0,
            inline_reveal: None,
            additional_inline_reveals: Vec::new(),
        }
    }

    #[test]
    fn an_inlay_opens_room_inside_a_stable_line() {
        let mut recipe = recipe();
        recipe
            .insert_inlay("type", "program", "name", ghost(": Effect<number>"))
            .unwrap();
        let line = &recipe.lines[0];
        assert_eq!(
            line.parts
                .iter()
                .map(|part| part.id.as_str())
                .collect::<Vec<_>>(),
            ["lead", "name", "inlay:type", "rest"],
            "the hint sits after its range and every authored part keeps its ID"
        );
        let editor = recipe.compile().unwrap();
        let reveal = &editor.inline_reveals()[0];
        assert_eq!(reveal.plan.channel(), "inlay.type");
        assert_eq!(reveal.parts, 2..3);
        // Collapsed, the hint is absent; open, it is present. Neither changes
        // the line's identity or any other part's presence.
        let id = crate::code::LineId::new("program");
        let presence = |open: f32| {
            editor
                .part_presence(&id, |channel, default| {
                    if channel == "inlay.type" {
                        open
                    } else {
                        default
                    }
                })
                .unwrap()
        };
        assert_eq!(presence(0.0), vec![1.0, 1.0, 0.0, 1.0]);
        assert_eq!(presence(1.0), vec![1.0; 4]);
        assert_eq!(editor.lines().len(), 1);
        // A target after the hint still resolves on the same stable line.
        assert!(
            editor
                .line("program")
                .unwrap()
                .semantic_span_range(&crate::code::RangeId::new("rest"))
                .is_ok()
        );
        let mut twice = recipe.clone();
        assert!(
            twice
                .insert_inlay("type", "program", "name", ghost(": number"))
                .is_err()
        );
        assert!(
            recipe
                .clone()
                .insert_inlay("other", "program", "missing", ghost(": x"))
                .is_err()
        );
        let _ = EditorTargetSelector {
            line_id: "program".into(),
            range_id: inlay_part("type"),
        };
    }

    #[test]
    fn waves_are_deterministic_and_keep_their_shape_as_they_move() {
        let a = wave(vec2(100.0, 40.0), 75.0, AMPLITUDE, WAVELENGTH);
        let b = wave(vec2(100.0, 40.0), 75.0, AMPLITUDE, WAVELENGTH);
        assert_eq!(a.points(), b.points(), "a pure function of its inputs");
        let moved = wave(vec2(137.25, 82.5), 75.0, AMPLITUDE, WAVELENGTH);
        for (a, b) in a.points().iter().zip(moved.points()) {
            assert!((*b - *a - vec2(37.25, 42.5)).length() < 1e-4);
        }
        assert_eq!(a.points()[0], vec2(100.0, 40.0));
        assert!(
            (a.points().last().unwrap().x - 175.0).abs() < 1e-4,
            "ends at the range end"
        );
        assert!(
            a.points()
                .iter()
                .all(|p| (p.y - 40.0).abs() <= AMPLITUDE + 1e-4)
        );
        assert!(
            a.points()
                .iter()
                .any(|p| (p.y - 40.0).abs() > AMPLITUDE * 0.9),
            "it waves"
        );
        let flat = wave(vec2(0.0, 10.0), 50.0, 0.0, WAVELENGTH);
        assert!(flat.points().iter().all(|p| p.y == 10.0));
        let half = a.slice(0.0, 0.5);
        assert!(
            (half.length() - a.length() * 0.5).abs() < 1e-3,
            "drawn by length"
        );
    }

    #[test]
    fn the_caret_holds_after_moving_then_blinks_continuously() {
        for t in [-1.0, 0.0, 0.25, 0.49] {
            assert_eq!(caret_blink(t), 1.0);
        }
        assert_eq!(caret_blink(0.5 + 0.2), 1.0);
        assert_eq!(caret_blink(0.5 + 0.7), 0.0);
        assert_eq!(caret_blink(0.5 + 1.2), 1.0, "periodic");
        let mut previous = caret_blink(0.0);
        for step in 1..4000 {
            let value = caret_blink(step as f32 * 0.001);
            assert!((value - previous).abs() < 0.03, "no pops at {step} ms");
            previous = value;
        }
    }

    #[test]
    fn hover_cards_stay_in_bounds_and_aim_at_their_range() {
        let bounds = Box2 {
            min: vec2(0.0, 0.0),
            max: vec2(1000.0, 600.0),
        };
        let range = Box2 {
            min: vec2(300.0, 300.0),
            max: vec2(400.0, 344.0),
        };
        let size = vec2(420.0, 120.0);
        let above = hover_layout(range, size, HoverSide::Above, 4.0, bounds);
        assert!(!above.below);
        assert_eq!(above.card.max.y, 300.0 - 4.0 - HOVER_TIP.y);
        assert_eq!(above.tip, vec2(350.0, 300.0 - 4.0));
        let below = hover_layout(range, size, HoverSide::Below, 4.0, bounds);
        assert_eq!(below.card.min.y, 344.0 + 4.0 + HOVER_TIP.y);
        // Near the right edge, the card slides in and the pointer still aims.
        let edge = Box2 {
            min: vec2(900.0, 300.0),
            max: vec2(990.0, 344.0),
        };
        let slid = hover_layout(edge, size, HoverSide::Above, 4.0, bounds);
        assert_eq!(slid.card.max.x, 1000.0);
        assert_eq!(slid.base_x, 945.0);
        let corner = Box2 {
            min: vec2(990.0, 300.0),
            max: vec2(1000.0, 344.0),
        };
        let pinned = hover_layout(corner, size, HoverSide::Above, 4.0, bounds);
        assert_eq!(pinned.base_x, 1000.0 - 10.0 - HOVER_TIP.x);
        // Following a range: the card translates with it, continuously.
        let moved = hover_layout(
            Box2 {
                min: range.min + vec2(0.0, 44.0),
                max: range.max + vec2(0.0, 44.0),
            },
            size,
            HoverSide::Above,
            4.0,
            bounds,
        );
        assert_eq!(moved.card.min - above.card.min, vec2(0.0, 44.0));
        // When Above would hit the top bound and overlap its own target, flip Below.
        let top_range = Box2 {
            min: vec2(300.0, 20.0),
            max: vec2(400.0, 64.0),
        };
        let flipped = hover_layout(top_range, size, HoverSide::Above, 4.0, bounds);
        assert!(flipped.below);
        assert!(flipped.card.min.y >= top_range.max.y);
    }

    #[test]
    fn plans_validate_and_handles_write_strict_channels() {
        let hover = HoverPlan::new("arg")
            .code("const program: Effect<string, NotFound, Database>")
            .text(vec![CaptionSpanPlan::new(
                "Type 'Database' is not assignable",
                Tone::Plain,
            )]);
        hover.validate().unwrap();
        assert_eq!(hover.sections.len(), 2);
        assert!(HoverPlan::new("arg").validate().is_err());
        let json = serde_json::to_value(&hover).unwrap();
        assert!(json.get("side").is_none());
        assert!(json["sections"][0].get("code").is_some());
        assert!(DiagnosticPlan::error("arg").accepts("draw"));
        assert!(!DiagnosticPlan::error("arg").accepts("drew"));
        let diagnostic = serde_json::to_value(DiagnosticPlan::warning("arg")).unwrap();
        assert_eq!(
            diagnostic,
            serde_json::json!({ "target": "arg", "severity": "warning" })
        );
        let cursor = CursorPlan::new("call", "call").anchor("arg", "arg");
        assert!(cursor.accepts("anchor.arg") && !cursor.accepts("anchor.nowhere"));
        assert!(CursorPlan::new("a.b", "x").validate().is_err());

        let mut scene = PlanBuilder::new("ide", 4_000_000_000);
        let diagnostic =
            DiagnosticActor::declare(&mut scene, "error", &DiagnosticPlan::error("arg")).unwrap();
        assert_eq!(diagnostic.show(&mut scene, 500_000_000), 1_050_000_000);
        diagnostic.clear(&mut scene, 3_000_000_000);
        let caret = CursorActor::declare(&mut scene, "caret", &cursor).unwrap();
        caret.show(&mut scene, 0);
        caret.select(&mut scene, "arg", 1_000_000_000, 0.4).unwrap();
        assert!(
            caret
                .move_to(&mut scene, "nowhere", 1.0, 2_000_000_000)
                .is_err()
        );
        let plan = scene.finish().unwrap();
        let channel = |id: &str| {
            plan.continuous_channels
                .iter()
                .find(|channel| channel.id == id)
                .unwrap()
        };
        assert!(matches!(
            channel("error.draw").initial,
            ScalarPlan::Literal(0.0)
        ));
        assert!(matches!(
            channel("caret.anchor.call").initial,
            ScalarPlan::Literal(1.0)
        ));
        assert!(matches!(
            channel("caret.anchor.arg").initial,
            ScalarPlan::Literal(0.0)
        ));
        assert!(matches!(
            channel("caret.blink").initial,
            ScalarPlan::Literal(-1.0)
        ));
    }
}
