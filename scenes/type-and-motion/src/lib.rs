//! Type and motion showroom: one frame of Stage labels and CPU captions at
//! video sizes, text drifting and scaling slowly enough to expose shimmer, and
//! the built-in entrances and named springs side by side. It uses only the
//! engine's own helpers, so the same program renders an engine's before and
//! after.
use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, SECOND, millis, seconds},
    caption::{CaptionActor, CaptionAlign, CaptionPlan, CaptionSpanPlan},
    math::easing::Ease,
    plan::{ScenePlan, SpringPlan},
    rolling::{RollingNumberActor, RollingNumberPlan},
    stage::{Arrow, Figure, Fill, OrbEntrance, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

const DURATION: f64 = 4.0;
const LEFT: f32 = 120.0;
const RIGHT: f32 = 1060.0;
const SAMPLE: &str = "Compaction · 0.002 s";

fn left(id: &str, at: [f32; 2], size: f32, spans: &[(&str, Tone)]) -> StageElement {
    StageElement::label(id, [at[0], at[1], 0.0], size, spans).align(CaptionAlign::Left)
}

fn bar(id: &str, y: f32) -> StageElement {
    StageElement::Shape {
        id: id.into(),
        at: [RIGHT + 330.0, y, 0.0],
        shape: Figure::Rect([44.0, 44.0]),
        corner: 10.0,
        fill: Some(Fill::Tone(Tone::Accent)),
        fill_opacity: 0.5,
        stroke: Some(Tone::Accent),
        width: 1.6,
        dash: None,
        arrow: Arrow::None,
    }
}

/// The springs compared on the right: label, feel.
const FEELS: [(&str, SpringPlan); 4] = [
    ("PANEL", SpringPlan::PANEL),
    ("POP", SpringPlan::POP),
    ("LIVELY", SpringPlan::LIVELY),
    ("MOVE", SpringPlan::MOVE),
];

fn feel_y(index: usize) -> f32 {
    560.0 + 84.0 * index as f32
}

fn stage_plan() -> StagePlan {
    let mut elements = vec![
        left("type", [LEFT, 90.0], 30.0, &[("TYPE", Tone::Muted)]),
        left("motion", [RIGHT, 90.0], 30.0, &[("MOTION", Tone::Muted)]),
    ];
    let mut y = 160.0;
    for (index, size) in [24.0, 32.0, 40.0, 52.0, 72.0].into_iter().enumerate() {
        y += size * 0.75 + 18.0;
        elements.push(left(
            &format!("size-{index}"),
            [LEFT, y],
            size,
            &[(SAMPLE, Tone::Plain)],
        ));
        y += size * 0.75;
    }
    elements.push(left(
        "tones",
        [LEFT, 640.0],
        40.0,
        &[
            ("muted ", Tone::Muted),
            ("accent ", Tone::Accent),
            ("success ", Tone::Success),
            ("error", Tone::Error),
        ],
    ));
    elements.push(left(
        "drift",
        [LEFT, 730.0],
        40.0,
        &[("drifting ", Tone::Plain), ("0.25 px a frame", Tone::Muted)],
    ));
    elements.push(left(
        "grow",
        [LEFT, 810.0],
        40.0,
        &[("scaling ", Tone::Plain), ("1.00 → 1.05", Tone::Muted)],
    ));

    elements.push(StageElement::card(
        "card",
        [RIGHT + 200.0, 230.0, 0.0],
        [300.0, 120.0],
        "settle_in",
    ));
    let mut orb = StageElement::orb("orb", [RIGHT + 560.0, 230.0, 0.0], 54.0);
    orb = orb.tone(Tone::Accent);
    elements.push(orb);
    elements.push(left(
        "orb-label",
        [RIGHT + 640.0, 230.0],
        28.0,
        &[("orb_in", Tone::Muted)],
    ));
    elements.push(bar("flash", 400.0));
    elements.push(left(
        "flash-label",
        [RIGHT, 400.0],
        28.0,
        &[("hit", Tone::Muted)],
    ));
    for (index, (name, _)) in FEELS.iter().enumerate() {
        elements.push(left(
            &format!("feel-{index}-label"),
            [RIGHT, feel_y(index)],
            28.0,
            &[(name, Tone::Muted)],
        ));
        elements.push(bar(&format!("feel-{index}"), feel_y(index)));
    }
    StagePlan {
        post: StagePost {
            bloom: 0.0,
            grain: 0.0,
            vignette: 0.0,
            backdrop: 0.0,
        },
        elements,
    }
}

pub fn build_plan() -> Result<ScenePlan> {
    let end = seconds(DURATION);
    let mut scene = PlanBuilder::new("type-and-motion", end);
    let plan = stage_plan();
    let mut stage = StageActor::declare(&mut scene, "stage", &plan)?;
    let (s, sc) = (&mut stage, &mut scene);

    // Text that moves too slowly to blur exposes shimmer and weight pulsing.
    s.ease(
        sc,
        "drift.x",
        0,
        0.25 * 60.0 * DURATION as f32,
        DURATION as f32,
        Ease::Linear,
    );
    s.ease(sc, "grow.scale", 0, 1.05, DURATION as f32, Ease::Linear);

    let go = millis(400);
    s.settle_in(sc, "card", go);
    s.orb_in(sc, "orb", go, OrbEntrance::HERO);
    s.hit(sc, "flash.flash", millis(900), 1.0, 0.0);
    for (index, (_, feel)) in FEELS.iter().enumerate() {
        let x = format!("feel-{index}.x");
        s.channel(sc, &x, 0.0);
        s.spring(sc, &x, go, 360.0, *feel);
        s.spring(sc, &x, millis(2300), 0.0, *feel);
    }

    let mut caption = CaptionActor::declare(
        sc,
        "caption",
        &CaptionPlan::line(
            [LEFT, 900.0],
            40.0,
            vec![
                CaptionSpanPlan::new("caption ", Tone::Plain),
                CaptionSpanPlan::new("drifting 0.25 px a frame", Tone::Muted),
            ],
        ),
    )?;
    let x = caption.channel(sc, "x", 0.0);
    sc.ease(
        &x,
        0,
        0.25 * 60.0 * DURATION as f32,
        DURATION as f32,
        Ease::Linear,
    );
    let mut subtitle = CaptionActor::declare(
        sc,
        "caption-show",
        &CaptionPlan::line(
            [LEFT, 980.0],
            40.0,
            vec![
                CaptionSpanPlan::new("caption ", Tone::Plain),
                CaptionSpanPlan::new("show / hide", Tone::Muted),
            ],
        ),
    )?;
    subtitle.show(sc, go);
    subtitle.hide(sc, millis(2600));

    let mut counter = RollingNumberActor::declare(
        sc,
        "counter",
        RollingNumberPlan::new([RIGHT + 640.0, 400.0], 72.0, "55")
            .aligned(CaptionAlign::Left)
            .tone(Tone::Error)
            .suffix(vec![CaptionSpanPlan::new(" s", Tone::Muted)]),
    )?;
    counter.roll(sc, millis(900), "0.002")?;
    counter.roll(sc, millis(2600), "55")?;

    scene.cue("gallery", 0, DURATION as u64 * SECOND);
    Ok(scene.finish()?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_showroom_plan_is_valid() {
        super::build_plan().unwrap();
    }
}
