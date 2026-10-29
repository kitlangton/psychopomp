//! The explainer visualization overlays: checklists, meters, benchmark bars,
//! subtitles, and confetti. Each recipe decodes and checks its channels in
//! its own module; this one only holds them, so the shared preflight and
//! preparation code take them as one group. Checklists, meters, and bars
//! draw with the charts; confetti and subtitles draw above every overlay.
mod bars;
mod checklist;
mod confetti;
mod meter;
mod subtitles;

use anyhow::Result;
use psychopomp::{
    bars::BARS_RECIPE,
    checklist::CHECKLIST_RECIPE,
    confetti::CONFETTI_RECIPE,
    meter::METER_RECIPE,
    plan::{ActorPlan, ContinuousChannelPlan},
    subtitles::SUBTITLES_RECIPE,
};

use crate::render::HeadlessRenderer;

/// Whether `recipe` is one of these overlays.
pub(super) fn accepts(recipe: &str) -> bool {
    matches!(
        recipe,
        CHECKLIST_RECIPE | METER_RECIPE | BARS_RECIPE | SUBTITLES_RECIPE | CONFETTI_RECIPE
    )
}

#[derive(Default)]
pub(super) struct VizInputs {
    checklists: Vec<checklist::PreparedChecklist>,
    meters: Vec<meter::PreparedMeter>,
    bars: Vec<bars::PreparedBars>,
    subtitles: Vec<subtitles::SubtitlesInput>,
    confetti: Vec<confetti::PreparedConfetti>,
}

impl VizInputs {
    /// Decode an actor whose recipe [`accepts`] names.
    pub(super) fn parse(
        &mut self,
        actor: &ActorPlan,
        channels: &[ContinuousChannelPlan],
    ) -> Result<()> {
        match actor.recipe.as_str() {
            CHECKLIST_RECIPE => self
                .checklists
                .push(checklist::PreparedChecklist::new(actor, channels)?),
            METER_RECIPE => self
                .meters
                .push(meter::PreparedMeter::new(actor, channels)?),
            BARS_RECIPE => self.bars.push(bars::PreparedBars::new(actor, channels)?),
            SUBTITLES_RECIPE => self
                .subtitles
                .push(subtitles::SubtitlesInput::new(actor, channels)?),
            CONFETTI_RECIPE => self
                .confetti
                .push(confetti::PreparedConfetti::new(actor, channels)?),
            recipe => anyhow::bail!("'{recipe}' is not a visualization overlay"),
        }
        Ok(())
    }

    /// Subtitles follow the authored clock, as Rolling Numbers do; every
    /// other overlay here is channel-driven.
    pub(super) fn native(&self) -> bool {
        self.subtitles.is_empty()
    }

    pub(super) fn prepare(self, renderer: &mut HeadlessRenderer) -> PreparedViz {
        PreparedViz {
            checklists: self.checklists,
            meters: self.meters,
            bars: self.bars,
            subtitles: self
                .subtitles
                .into_iter()
                .map(|input| input.prepare(renderer))
                .collect(),
            confetti: self.confetti,
        }
    }
}

pub(super) struct PreparedViz {
    checklists: Vec<checklist::PreparedChecklist>,
    meters: Vec<meter::PreparedMeter>,
    bars: Vec<bars::PreparedBars>,
    subtitles: Vec<subtitles::PreparedSubtitles>,
    confetti: Vec<confetti::PreparedConfetti>,
}

impl PreparedViz {
    /// Subtitle transitions change pixels with no channel moving.
    pub(super) fn moving(&self, time: f64) -> bool {
        self.subtitles
            .iter()
            .any(|subtitles| subtitles.moving(time))
    }

    pub(super) fn has_confetti(&self, actor: &str) -> bool {
        self.confetti.iter().any(|confetti| confetti.id() == actor)
    }

    pub(super) fn confetti_bounds(
        &self,
        sample: impl Fn(&str, &str, f32) -> f32 + Copy,
    ) -> Vec<(Option<[u32; 4]>, Vec<psychopomp::math::shapes::Box2>)> {
        self.confetti
            .iter()
            .map(|confetti| confetti.bounds(sample))
            .collect()
    }

    /// Checklists, meters, and bars, in the chart layer. `velocity` reads a
    /// channel's rate of change, which smears fast Readout wheels.
    pub(super) fn render_diagrams(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        sample: impl Fn(&str, &str, f32) -> f32 + Copy,
        velocity: impl Fn(&str, &str) -> f32 + Copy,
    ) {
        for checklist in &self.checklists {
            checklist.render(pixels, renderer, sample);
        }
        for meter in &self.meters {
            meter.render(pixels, renderer, sample, velocity);
        }
        for bars in &self.bars {
            bars.render(pixels, renderer, sample, velocity);
        }
    }

    /// Confetti, then subtitles: burned-in captions stay on top.
    pub(super) fn render_foreground(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        time: f64,
        sample: impl Fn(&str, &str, f32) -> f32 + Copy,
    ) {
        for confetti in &self.confetti {
            confetti.render(pixels, renderer, sample);
        }
        for subtitles in &self.subtitles {
            subtitles.render(pixels, renderer, time, sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use psychopomp::{
        author::PlanBuilder,
        axis::AxisPlan,
        bars::{BarSeriesPlan, BarsActor, BarsPlan},
        checklist::{ChecklistActor, ChecklistItemPlan, ChecklistPlan},
        confetti::{ConfettiActor, ConfettiPlan},
        meter::{MeterActor, MeterPlan},
        subtitles::{SubtitlesActor, SubtitlesPlan},
        tone::Tone,
    };

    use super::super::{preflight, validate_renderer_plan};

    /// One plan with every overlay, plus an optional stray channel on `actor`.
    fn build(stray: Option<(&str, &str)>, subtitles: bool) -> psychopomp::plan::ScenePlan {
        let mut scene = PlanBuilder::new("viz", 4_000_000_000);
        let mut checks = ChecklistActor::declare(
            &mut scene,
            "checks",
            &ChecklistPlan::new([200.0, 200.0], 600.0).item(ChecklistItemPlan::new("a", "lint")),
        )
        .unwrap();
        checks.start(&mut scene, "a", 0).unwrap();
        let mut meter = MeterActor::declare(
            &mut scene,
            "meter",
            &MeterPlan::countdown([1400.0, 400.0], 120.0, 3.0),
            3.0,
        )
        .unwrap();
        meter.countdown(&mut scene, 0, 3.0);
        let mut bars = BarsActor::declare(
            &mut scene,
            "bars",
            &BarsPlan::new([600.0, 700.0], 600.0, AxisPlan::new([0.0, 10.0]))
                .series(BarSeriesPlan::new("s", "", Tone::Accent))
                .row("r", "row"),
        )
        .unwrap();
        bars.set(&mut scene, "r", "s", 0, 5.0).unwrap();
        let mut confetti =
            ConfettiActor::declare(&mut scene, "pop", &ConfettiPlan::new([960.0, 540.0])).unwrap();
        confetti.burst(&mut scene, 1_000_000_000);
        if subtitles {
            SubtitlesActor::declare(
                &mut scene,
                "subs",
                &SubtitlesPlan::new([960.0, 980.0], 1200.0).word("hello", 0, 300_000_000),
            )
            .unwrap();
        }
        match stray {
            Some(("checks", property)) => _ = checks.channel(&mut scene, property, 0.0),
            Some(("meter", property)) => _ = meter.channel(&mut scene, property, 0.0),
            Some(("bars", property)) => _ = bars.channel(&mut scene, property, 0.0),
            Some((_, property)) => _ = confetti.channel(&mut scene, property, 0.0),
            None => {}
        }
        scene.finish().unwrap()
    }

    #[test]
    fn overlays_preflight_strict_channels_and_stay_native_without_subtitles() {
        validate_renderer_plan(&build(None, true)).unwrap();
        for (actor, typo) in [
            ("checks", "item.a.spin"),
            ("checks", "item.b.mark"),
            ("meter", "progress"),
            ("bars", "bar.r.other"),
            ("pop", "age"),
        ] {
            let error = validate_renderer_plan(&build(Some((actor, typo)), false)).unwrap_err();
            assert!(
                format!("{error:#}").contains("unknown property"),
                "{actor}.{typo}: {error:#}"
            );
        }
        assert!(preflight::Plan::new(build(None, false)).unwrap().native());
        assert!(!preflight::Plan::new(build(None, true)).unwrap().native());
    }
}
