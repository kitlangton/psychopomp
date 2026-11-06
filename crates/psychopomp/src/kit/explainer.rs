//! The explainer kit: PR, bug, and architecture films on the dark editor
//! theme. `CommitMono` everywhere, restrained bloom, Stage cards and orbs, and
//! fixed frame captions: a header naming the film top left, status chips top
//! right, and a footer that sums up a beat bottom left. Each caption returns
//! its declared Caption, present until the film types it in, shows, or hides it.
//!
//! Pick it only when the brief is an explainer; other briefs start from
//! another kit.
use anyhow::Result;

use crate::{
    author::PlanBuilder,
    canvas::CanvasPlan,
    caption::{CaptionActor, CaptionAlign, CaptionPlan, CaptionSpanPlan},
    face::Face,
    stage::{StageElement, StagePost},
    tone::Tone,
};

use super::{StyleKit, Voice};

/// The left edge of the header and footer.
pub const LEFT: f32 = 140.0;
/// The right edge of the chips.
pub const RIGHT: f32 = 1780.0;
pub const HEADER_Y: f32 = 96.0;
pub const FOOTER_Y: f32 = 1004.0;

/// 1920x1080 on the render theme, `CommitMono`, restrained post.
pub fn kit() -> StyleKit {
    StyleKit {
        name: "explainer",
        canvas: CanvasPlan::landscape_1080p(),
        palette: None,
        post: StagePost::RESTRAINED,
        ground: None,
        display: Voice::face(Face::Mono).weight(700),
        text: Voice::face(Face::Mono),
        fonts: &[],
    }
}

/// A Stage card: a titled panel that takes statuses and a mark.
pub fn card(id: &str, at: [f32; 3], size: [f32; 2], title: &str) -> StageElement {
    StageElement::card(id, at, size, title)
}

/// A Stage orb: a point cloud that stands for a service or a process.
pub fn orb(id: &str, at: [f32; 3], radius: f32) -> StageElement {
    StageElement::orb(id, at, radius)
}

/// `#50784  keep the real startup error`: an accent label, then the title.
pub fn header(scene: &mut PlanBuilder, label: &str, title: &str) -> Result<CaptionActor> {
    let plan = CaptionPlan::line(
        [LEFT, HEADER_Y],
        30.0,
        vec![
            CaptionSpanPlan::new(label, Tone::Accent),
            CaptionSpanPlan::new("  ", Tone::Plain),
            CaptionSpanPlan::new(title, Tone::Plain),
        ],
    );
    CaptionActor::declare(scene, "header", &plan)
}

/// A status chip, right-aligned top right: `● before`.
pub fn chip(scene: &mut PlanBuilder, id: &str, dot: Tone, text: &str) -> Result<CaptionActor> {
    let plan = CaptionPlan::line(
        [RIGHT, HEADER_Y],
        22.0,
        vec![
            CaptionSpanPlan::new("● ", dot),
            CaptionSpanPlan::new(text, Tone::Plain),
        ],
    )
    .aligned(CaptionAlign::Right)
    .chip();
    CaptionActor::declare(scene, id, &plan)
}

/// The one caption that sums up a beat, bottom left.
pub fn footer(
    scene: &mut PlanBuilder,
    id: &str,
    spans: Vec<CaptionSpanPlan>,
) -> Result<CaptionActor> {
    CaptionActor::declare(scene, id, &CaptionPlan::line([LEFT, FOOTER_Y], 28.0, spans))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_captions_sit_in_their_corners() {
        let mut scene = PlanBuilder::new("chrome", 1_000_000_000);
        header(&mut scene, "#1", "a title").unwrap();
        chip(&mut scene, "chip", Tone::Success, "after").unwrap();
        footer(
            &mut scene,
            "footer",
            vec![CaptionSpanPlan::new("done", Tone::Plain)],
        )
        .unwrap();
        let plan = scene.finish().unwrap();
        let data = |id: &str| {
            let actor = plan.actors.iter().find(|actor| actor.id == id).unwrap();
            serde_json::from_value::<CaptionPlan>(actor.data.clone()).unwrap()
        };
        assert_eq!(data("header").origin, [LEFT, HEADER_Y]);
        assert_eq!(data("chip").align, CaptionAlign::Right);
        assert_eq!(data("footer").origin, [LEFT, FOOTER_Y]);
    }

    #[test]
    fn cards_and_orbs_are_the_stage_elements() {
        let at = [1.0, 2.0, 0.0];
        assert_eq!(
            card("c", at, [300.0, 100.0], "t"),
            StageElement::card("c", at, [300.0, 100.0], "t")
        );
        assert_eq!(orb("o", at, 40.0), StageElement::orb("o", at, 40.0));
    }
}
