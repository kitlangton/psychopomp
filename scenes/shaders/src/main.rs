//! Shaders showroom: a custom WGSL layer. A drifting, domain-warped gradient
//! field with film grain fills a rounded panel and, through a type matte,
//! the word beside it. Its warp, speed, and colors are ordinary channels, so
//! the field's hot color shifts to pink mid-film and Studio tunes every parameter.
use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, SECOND, seconds},
    math::easing::Ease,
    plan::SpringPlan,
    stage::{ShaderParam, StageActor, StageElement, StagePlan},
};

const GRAIN_FIELD: &str = include_str!("grain_field.wgsl");

fn field(id: &str, at: [f32; 3], size: [f32; 2], glow: [u8; 3]) -> StageElement {
    StageElement::shader(
        id,
        at,
        size,
        GRAIN_FIELD,
        vec![
            ShaderParam::scalar("speed", 0.2, [0.0, 2.0]),
            ShaderParam::scalar("grain", 0.12, [0.0, 0.6]),
            ShaderParam::scalar("warp", 0.8, [0.0, 3.0]),
            ShaderParam::color("deep", [16, 24, 64]),
            ShaderParam::color("glow", glow),
            ShaderParam::color("haze", [120, 210, 255]),
        ],
    )
}

fn stage() -> Result<StagePlan> {
    let mut plan: StagePlan = serde_json::from_value(serde_json::json!({
        "post": { "bloom": 0.22, "grain": 0.01, "vignette": 0.25, "backdrop": 0.1 },
        "elements": [
            { "kind": "group", "id": "word", "at": [1420, 540, 0],
              "clip": { "kind": "type", "text": "FIELD", "face": "sans-bold", "size": 210,
                        "weight": 900, "tracking": 0.02 },
              "children": [] }
        ]
    }))?;
    plan.elements.insert(
        0,
        field("panel", [600.0, 540.0, 0.0], [820.0, 620.0], [255, 120, 60]).corner(32.0),
    );
    if let Some(StageElement::Group { children, .. }) = plan.elements.last_mut() {
        children.push(field(
            "word-field",
            [0.0; 3],
            [820.0, 260.0],
            [255, 90, 170],
        ));
    }
    Ok(plan)
}

fn main() -> Result<()> {
    let plan = stage()?;
    let mut scene = PlanBuilder::new("shaders", 8 * SECOND);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    // The panel settles in, then its field warps harder and its glow
    // turns from orange to hot pink.
    s.channel(sc, "panel.opacity", 0.0);
    s.ease(sc, "panel.opacity", seconds(0.2), 1.0, 0.4, Ease::GLIDE);
    let scale = s.channel(sc, "panel.scale", 0.94);
    sc.spring_with(&scale, seconds(0.2), 1.0, SpringPlan::PANEL);
    s.channel(sc, "panel.warp", 0.5);
    s.ease(sc, "panel.warp", seconds(1.0), 1.4, 5.0, Ease::Smootherstep);
    for (component, to) in [("r", 1.0), ("g", 0.3), ("b", 0.52)] {
        let property = format!("panel.glow.{component}");
        s.ease(sc, &property, seconds(4.0), to, 1.6, Ease::Smootherstep);
    }

    // The word's field slides under its letters from the left.
    s.channel(sc, "word.opacity", 0.0);
    s.ease(sc, "word.opacity", seconds(0.9), 1.0, 0.4, Ease::GLIDE);
    let x = s.channel(sc, "word-field.x", -260.0);
    sc.spring_with(&x, seconds(0.9), 0.0, SpringPlan::MOVE);
    s.channel(sc, "word-field.speed", 0.6);

    scene.finish()?.write_or_print(Some(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/shaders.json".into()),
    ))?;
    Ok(())
}
