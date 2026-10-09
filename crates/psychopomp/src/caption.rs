//! Captions: short lines of styled CommitMono text in the terminal voice of an
//! explainer, with an optional typing reveal and block caret. Spans carry
//! semantic tones, so one keyword can take the accent while the rest stays plain.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    anchor::{self, AnchorPlan},
    author::{ActorHandle, ContinuousHandle, PlanBuilder},
    tone::Tone,
};

pub const CAPTION_RECIPE: &str = "caption";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptionPlan {
    /// Anchor point: `align` chooses which edge of each line sits on x, and y is
    /// the vertical center of the first line.
    pub origin: [f32; 2],
    #[serde(default, skip_serializing_if = "CaptionAlign::is_default")]
    pub align: CaptionAlign,
    pub size: f32,
    pub lines: Vec<Vec<CaptionSpanPlan>>,
    /// A rounded surface behind the text, as for a status chip.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chip: bool,
    /// Places the caption can pin to; while it has any, the blended anchor
    /// (plus that anchor's offset) replaces `origin`. The first is where it
    /// starts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<AnchorPlan>,
    /// Make the chip liquid glass: a frosted pane that refracts the scene
    /// behind the text instead of covering it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub glass: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptionSpanPlan {
    pub text: String,
    #[serde(default, skip_serializing_if = "Tone::is_default")]
    pub tone: Tone,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptionAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl CaptionAlign {
    pub fn is_default(&self) -> bool {
        *self == Self::Left
    }
}

impl CaptionSpanPlan {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

impl CaptionPlan {
    /// One line of spans.
    pub fn line(origin: [f32; 2], size: f32, spans: Vec<CaptionSpanPlan>) -> Self {
        Self {
            origin,
            align: CaptionAlign::Left,
            size,
            lines: vec![spans],
            chip: false,
            anchors: Vec::new(),
            glass: false,
        }
    }

    pub fn aligned(mut self, align: CaptionAlign) -> Self {
        self.align = align;
        self
    }

    pub fn chip(mut self) -> Self {
        self.chip = true;
        self
    }

    /// Pin the caption's origin to `anchor`; the first anchor is where it starts.
    pub fn anchor(mut self, anchor: AnchorPlan) -> Self {
        self.anchors.push(anchor);
        self
    }

    /// A chip of liquid glass.
    pub fn glass(mut self) -> Self {
        self.chip = true;
        self.glass = true;
        self
    }

    pub fn line_height(&self) -> f32 {
        self.size * 1.45
    }

    /// True when `property` names one of this caption's channels.
    pub fn accepts(&self, property: &str) -> bool {
        matches!(property, "opacity" | "x" | "y" | "typed" | "caret")
            || anchor::accepts(property, &self.anchors)
    }

    /// Characters revealed by the `typed` channel, across all lines in order.
    pub fn char_count(&self) -> usize {
        self.lines
            .iter()
            .flatten()
            .map(|span| span.text.chars().count())
            .sum()
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.origin.iter().all(|v| v.is_finite()),
            "caption origin must be finite"
        );
        ensure!(
            (10.0..=120.0).contains(&self.size),
            "caption size must be between 10 and 120"
        );
        ensure!(
            (1..=6).contains(&self.lines.len()),
            "a caption has one to six lines"
        );
        for line in &self.lines {
            ensure!(
                line.iter().any(|span| !span.text.is_empty()),
                "caption lines cannot be empty"
            );
            for span in line {
                ensure!(
                    !span.text.contains('\n'),
                    "caption spans are single-line; use another line instead"
                );
            }
        }
        ensure!(
            self.char_count() <= 320,
            "captions are limited to 320 characters"
        );
        anchor::validate("caption", &self.anchors)
    }
}

/// Authoring handle for one caption actor. Channels are declared once, with the
/// recipe's defaults as initial values.
#[derive(Clone, Debug)]
pub struct CaptionActor {
    actor: ActorHandle,
    chars: usize,
    anchors: Vec<String>,
}

impl CaptionActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &CaptionPlan,
    ) -> Result<Self> {
        plan.validate()?;
        let actor = scene.actor(id, CAPTION_RECIPE, plan)?;
        Ok(Self {
            actor,
            chars: plan.char_count(),
            anchors: anchor::ids(&plan.anchors),
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    pub fn char_count(&self) -> usize {
        self.chars
    }

    pub fn channel(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, property, initial)
    }

    /// Fade and rise in. A caption with a `show` starts hidden.
    pub fn show(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        show(scene, &self.actor, at_nanos);
    }

    /// Fade out in place.
    pub fn hide(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        hide(scene, &self.actor, at_nanos);
    }

    /// Glide to the anchor `to`, carrying velocity through interruptions.
    pub fn move_to(&mut self, scene: &mut PlanBuilder, to: &str, at_nanos: u64) -> Result<()> {
        anchor::move_to(scene, &self.actor, &self.anchors, to, at_nanos)
    }

    /// Type the caption in at `chars_per_second`, showing the block caret while
    /// typing and for `caret_hold_seconds` afterward. The caption becomes
    /// visible when typing starts. Returns when typing finishes.
    pub fn type_in(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        chars_per_second: f32,
        caret_hold_seconds: f32,
    ) -> u64 {
        self.type_in_at(scene, at_nanos, chars_per_second, caret_hold_seconds)
    }

    pub(crate) fn type_in_at(
        &self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        chars_per_second: f32,
        caret_hold_seconds: f32,
    ) -> u64 {
        let opacity = scene.channel(&self.actor, "opacity", 0.0);
        let typed = scene.channel(&self.actor, "typed", 0.0);
        let caret = scene.channel(&self.actor, "caret", 0.0);
        scene.set(&opacity, at_nanos, 1.0);
        scene.set(&caret, at_nanos, 1.0);
        let done = type_steps(scene, &typed, at_nanos, self.chars, chars_per_second);
        let caret_off = done + (f64::from(caret_hold_seconds.max(0.0)) * 1e9) as u64;
        scene.spring(&caret, caret_off, 0.0, 0.2, 0.0);
        done
    }
}

/// Fade and rise in, as captions and Rolling Numbers do; the first `show`
/// declares the actor hidden and 10 px low.
pub(crate) fn show(scene: &mut PlanBuilder, actor: &ActorHandle, at_nanos: u64) {
    let opacity = scene.channel(actor, "opacity", 0.0);
    let y = scene.channel(actor, "y", 10.0);
    scene.spring(&opacity, at_nanos, 1.0, 0.35, 0.0);
    scene.spring_with(&y, at_nanos, 0.0, crate::plan::SpringPlan::ENTER);
}

/// Fade out in place. If `opacity` was not already declared by `show` or
/// `type_in`, it starts at the recipe's visible resting value (`1.0`).
pub(crate) fn hide(scene: &mut PlanBuilder, actor: &ActorHandle, at_nanos: u64) {
    let opacity = scene.channel(actor, "opacity", 1.0);
    scene.spring_with(&opacity, at_nanos, 0.0, crate::plan::SpringPlan::EXIT);
}

/// Reveal `chars` characters on a 0..1 `typed` channel at `chars_per_second`,
/// one exact step per character. Returns when the last one appears.
pub(crate) fn type_steps(
    scene: &mut PlanBuilder,
    typed: &ContinuousHandle,
    at_nanos: u64,
    chars: usize,
    chars_per_second: f32,
) -> u64 {
    let per_char = (1e9 / f64::from(chars_per_second.max(1.0))) as u64;
    for index in 1..=chars {
        scene.set(
            typed,
            at_nanos + per_char * index as u64,
            index as f32 / chars as f32,
        );
    }
    at_nanos + per_char * chars as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> CaptionPlan {
        CaptionPlan::line(
            [140.0, 960.0],
            30.0,
            vec![
                CaptionSpanPlan::new("one server", Tone::Accent),
                CaptionSpanPlan::new(". every client.", Tone::Plain),
            ],
        )
    }

    #[test]
    fn captions_round_trip_with_compact_defaults() {
        let plan = plan();
        plan.validate().unwrap();
        assert_eq!(plan.char_count(), 25);
        let json = serde_json::to_value(&plan).unwrap();
        assert!(json.get("align").is_none() && json.get("chip").is_none());
        assert!(json.get("glass").is_none());
        assert_eq!(json["lines"][0][0]["tone"], "accent");
        assert!(json["lines"][0][1].get("tone").is_none());
        assert_eq!(serde_json::from_value::<CaptionPlan>(json).unwrap(), plan);
    }

    #[test]
    fn invalid_captions_are_rejected() {
        let mut empty = plan();
        empty.lines = vec![vec![CaptionSpanPlan::new("", Tone::Plain)]];
        assert!(empty.validate().is_err());
        let mut newline = plan();
        newline.lines[0][0].text = "a\nb".into();
        assert!(newline.validate().is_err());
        let mut huge = plan();
        huge.size = 400.0;
        assert!(huge.validate().is_err());
    }

    #[test]
    fn typing_writes_one_exact_step_per_character() {
        let mut scene = PlanBuilder::new("caption-demo", 10_000_000_000);
        let mut caption = CaptionActor::declare(&mut scene, "line", &plan()).unwrap();
        let done = caption.type_in(&mut scene, 1_000_000_000, 50.0, 0.5);
        assert_eq!(done, 1_000_000_000 + 25 * 20_000_000);
        let plan = scene.finish().unwrap();
        let typed = plan
            .continuous_channels
            .iter()
            .find(|channel| channel.property == "typed")
            .unwrap();
        assert_eq!(typed.events.len(), 25);
    }
}
