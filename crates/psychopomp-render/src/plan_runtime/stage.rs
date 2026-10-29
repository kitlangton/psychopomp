//! Prepared Stage root: decoded and validated once, with strict channel names,
//! and GPU resources (text atlas, bloom chain, pipelines) built at preparation.
use std::collections::HashMap;

use anyhow::{Context, Result};
use psychopomp::{
    plan::{ActorPlan, ContinuousChannelPlan},
    stage::{StageElement, StagePlan},
};

use super::preflight::{decode, strict_channels};
use crate::{
    footage::StageFootageFrame,
    render::{HeadlessRenderer, StageBoundsItem, StageGpu, icon_svg, stage_bounds},
};

pub(super) fn validate_recipe(
    actor: &ActorPlan,
    channels: &[ContinuousChannelPlan],
) -> Result<StagePlan> {
    let plan = decode(actor, "stage", StagePlan::validate)?;
    strict_channels(&actor.id, channels, "stage", |property| {
        plan.accepts(property)
    })?;
    // Icon SVG must parse before any GPU work, so a typo in path data fails
    // validation rather than the first render.
    for element in &plan.elements {
        if let StageElement::Icon {
            id,
            icon,
            path,
            view,
            ..
        } = element
        {
            let svg = icon_svg(icon, path, *view)
                .with_context(|| format!("stage icon '{id}' names no bundled icon"))?;
            let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
                .with_context(|| format!("stage icon '{id}' SVG does not parse"))?;
            anyhow::ensure!(
                tree.root().has_children(),
                "stage icon '{id}' path data draws nothing"
            );
        }
    }
    Ok(plan)
}

pub(super) struct PreparedStage {
    id: String,
    plan: StagePlan,
    gpu: StageGpu,
}

impl PreparedStage {
    /// `footage` is the decoded size of each footage element's frames.
    pub(super) fn from_recipe(
        id: String,
        plan: StagePlan,
        footage: &HashMap<String, [u32; 2]>,
        renderer: &mut HeadlessRenderer,
    ) -> Result<Self> {
        let gpu = renderer.prepare_stage_with(&plan, footage)?;
        Ok(Self { id, plan, gpu })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn plan(&self) -> &StagePlan {
        &self.plan
    }

    /// One frame exposed through weighted shutter samples, accumulated on the
    /// GPU. `footage` gives each sample's footage frames.
    pub(super) fn render_exposure(
        &self,
        renderer: &mut HeadlessRenderer,
        exposure: &[(f64, f32)],
        value: impl Fn(&str, &str, f64, f32) -> f32,
        footage: impl Fn(f64) -> Result<Vec<StageFootageFrame>>,
    ) -> Result<Vec<u8>> {
        renderer.render_stage_exposure(
            &self.plan,
            &self.gpu,
            exposure,
            |time, property, default| value(&self.id, property, time, default),
            footage,
        )
    }

    pub(super) fn render(
        &self,
        renderer: &mut HeadlessRenderer,
        time: f64,
        value: impl Fn(&str, &str, f32) -> f32,
        footage: impl Fn(f64) -> Result<Vec<StageFootageFrame>>,
    ) -> Result<Vec<u8>> {
        renderer.render_stage_exposure(
            &self.plan,
            &self.gpu,
            &[(time, 1.0)],
            |_, property, default| value(&self.id, property, default),
            footage,
        )
    }

    pub(super) fn bounds(
        &self,
        time: f64,
        size: [u32; 2],
        value: impl Fn(&str, &str, f32) -> f32,
    ) -> Vec<StageBoundsItem> {
        stage_bounds(
            &self.plan,
            &self.gpu,
            &|property, default| value(&self.id, property, default),
            time,
            psychopomp::math::vec2(size[0] as f32, size[1] as f32),
        )
    }
}

#[cfg(test)]
mod tests {
    use psychopomp::{author::PlanBuilder, stage::StageActor};

    use super::super::validate_renderer_plan;

    fn plan(extra: Option<&str>) -> psychopomp::plan::ScenePlan {
        let recipe = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "orb", "id": "service", "at": [960, 460, 0], "radius": 150 },
                { "kind": "card", "id": "client", "at": [420, 300, 0], "size": [300, 110], "title": "client" },
                { "kind": "beam", "id": "link", "from": "client", "to": "service" },
                { "kind": "packet", "id": "probe", "beam": "link", "label": "GET" }
            ]
        }))
        .unwrap();
        let mut scene = PlanBuilder::new("stage-preflight", 3_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &recipe).unwrap();
        stage.send(&mut scene, "probe", 500_000_000, 0.6);
        stage.to(&mut scene, "camera.z", 0, 120.0, 1.0);
        if let Some(property) = extra {
            stage.to(&mut scene, property, 0, 1.0, 0.3);
        }
        scene.finish().unwrap()
    }

    #[test]
    fn icon_path_data_must_draw_something() {
        for (path, valid) in [("M32 32 H224 V224 Z", true), ("Q", false)] {
            let recipe = serde_json::from_value(serde_json::json!({
                "elements": [
                    { "kind": "icon", "id": "mark", "at": [960, 540, 0], "size": 64, "path": path }
                ]
            }))
            .unwrap();
            let mut scene = PlanBuilder::new("icon-preflight", 1_000_000_000);
            StageActor::declare(&mut scene, "stage", &recipe).unwrap();
            let result = validate_renderer_plan(&scene.finish().unwrap());
            assert_eq!(result.is_ok(), valid, "{path}: {result:?}");
        }
    }

    #[test]
    fn stage_preflight_accepts_element_channels_and_rejects_typos() {
        validate_renderer_plan(&plan(None)).unwrap();
        validate_renderer_plan(&plan(Some("service.shatter"))).unwrap();
        validate_renderer_plan(&plan(Some("camera.roll"))).unwrap();
        validate_renderer_plan(&plan(Some("camera.track.probe"))).unwrap();
        for typo in [
            "service.shater",
            "client.travel",
            "camera.spin",
            "camera.track.link",
            "ghost.opacity",
        ] {
            let error = validate_renderer_plan(&plan(Some(typo))).unwrap_err();
            assert!(
                format!("{error:#}").contains("unknown property"),
                "{typo}: {error:#}"
            );
        }
    }
}
