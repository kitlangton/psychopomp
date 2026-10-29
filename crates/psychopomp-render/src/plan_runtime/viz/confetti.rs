//! Prepared confetti: decoded once; channel names are strict.
use anyhow::Result;
use psychopomp::{
    confetti::ConfettiPlan,
    plan::{ActorPlan, ContinuousChannelPlan},
};

use super::super::preflight::{decode, strict_channels};
use crate::render::HeadlessRenderer;

pub(super) struct PreparedConfetti {
    id: String,
    plan: ConfettiPlan,
}

impl PreparedConfetti {
    pub(super) fn new(actor: &ActorPlan, channels: &[ContinuousChannelPlan]) -> Result<Self> {
        let plan = decode(actor, "confetti", ConfettiPlan::validate)?;
        strict_channels(&actor.id, channels, "confetti", ConfettiPlan::accepts)?;
        Ok(Self {
            id: actor.id.clone(),
            plan,
        })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn bounds(
        &self,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) -> (Option<[u32; 4]>, Vec<psychopomp::math::shapes::Box2>) {
        crate::render::confetti_bounds(&self.plan, |property, default| {
            sample(&self.id, property, default)
        })
    }

    pub(super) fn render(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) {
        renderer.composite_confetti(pixels, &self.plan, |property, default| {
            sample(&self.id, property, default)
        });
    }
}
