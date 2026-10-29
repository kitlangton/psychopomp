//! Prepared captions: decoded and validated once; channel names are strict.
use anyhow::Result;
use psychopomp::{
    anchor::AnchorPlan,
    caption::CaptionPlan,
    math::{Vec2, shapes::Box2},
    plan::{ActorPlan, ContinuousChannelPlan},
};

use super::preflight::{decode, strict_channels};
use crate::render::HeadlessRenderer;

pub(super) struct PreparedCaption {
    id: String,
    plan: CaptionPlan,
}

impl PreparedCaption {
    pub(super) fn new(actor: &ActorPlan, channels: &[ContinuousChannelPlan]) -> Result<Self> {
        let plan = decode(actor, "caption", CaptionPlan::validate)?;
        strict_channels(&actor.id, channels, "caption", |property| {
            plan.accepts(property)
        })?;
        Ok(Self {
            id: actor.id.clone(),
            plan,
        })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn anchors(&self) -> &[AnchorPlan] {
        &self.plan.anchors
    }

    /// The literal origin, used while the caption has no anchors.
    pub(super) fn origin(&self) -> Vec2 {
        Vec2::from(self.plan.origin)
    }

    pub(super) fn render(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        origin: Vec2,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) {
        renderer.composite_caption_at(
            pixels,
            &self.plan,
            origin.to_array(),
            |property, default| sample(&self.id, property, default),
        );
    }

    pub(super) fn bounds(
        &self,
        renderer: &mut HeadlessRenderer,
        origin: Vec2,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) -> Option<Box2> {
        renderer.caption_bounds(&self.plan, origin.to_array(), |property, default| {
            sample(&self.id, property, default)
        })
    }

    pub(super) fn text(&self) -> String {
        self.plan
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }
}
