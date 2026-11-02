//! Art showroom: vector SVG on the Stage. An emblem read from a file draws on
//! as one pen stroke, takes a gradient fill, then draws off from its start; a
//! filled figure morphs circle → star → two circles → circle, matched curve for
//! curve; and a comet of stroke runs around a rounded track on `trim-offset`.
use anyhow::Result;
use psychopomp::{
    art::{SvgArt, TrimMode},
    author::{PlanBuilder, SECOND, seconds},
    math::easing::Ease,
    paint::Paint,
    score::Stage,
    stage::{StageElement, StagePlan, StagePost},
    tone::Tone,
};

fn svg(body: &str) -> SvgArt {
    SvgArt::new(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 300">{body}</svg>"#
    ))
}

fn star(points: usize, outer: f32, inner: f32) -> String {
    let corners = (0..points * 2)
        .map(|k| {
            let angle =
                -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / points as f32;
            let radius = if k % 2 == 0 { outer } else { inner };
            format!(
                "{:.2} {:.2}",
                150.0 + radius * angle.cos(),
                150.0 + radius * angle.sin()
            )
        })
        .collect::<Vec<_>>();
    format!("M{} Z", corners.join(" L"))
}

fn stage() -> StagePlan {
    let emblem = SvgArt::from_file(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/emblem.svg"));
    let circle = svg(
        r##"<circle cx="150" cy="150" r="110" fill="#FFB347" stroke="#fff" stroke-width="3"/>"##,
    );
    let star = svg(&format!(
        r##"<path d="{}" fill="#FF5E8A" stroke="#fff" stroke-width="3"/>"##,
        star(5, 140.0, 58.0)
    ));
    let pair = svg(
        r##"<path d="M32 150 A58 58 0 1 0 148 150 A58 58 0 1 0 32 150 Z M152 150 A58 58 0 1 0 268 150 A58 58 0 1 0 152 150 Z" fill="#5EB8FF" stroke="#fff" stroke-width="3"/>"##,
    );
    let track = SvgArt::new(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 120"><rect x="4" y="4" width="1192" height="112" rx="56" fill="none" stroke="#9EA7B3" stroke-width="2"/></svg>"##,
    );
    let label = |id: &str, x: f32, text: &str| {
        StageElement::label(id, [x, 130.0, 0.0], 28.0, &[(text, Tone::Muted)])
    };
    StagePlan {
        post: StagePost {
            bloom: 0.2,
            grain: 0.012,
            vignette: 0.25,
            backdrop: 0.12,
        },
        elements: vec![
            label("mark-label", 520.0, "trim-start · trim-end"),
            label("shape-label", 1400.0, "morph"),
            StageElement::art("mark", [520.0, 480.0, 0.0], emblem)
                .stroke_paint(Paint::hex("#D9381E"), 4.0)
                .paint(Paint::linear_hex(90.0, "#FFC45C", "#D9381E"))
                .trim_mode(TrimMode::Sequence),
            StageElement::art("shape", [1400.0, 480.0, 0.0], circle)
                .morph_to(star)
                .morph_to(pair),
            StageElement::art("track-rail", [960.0, 900.0, 0.0], track.clone()),
            StageElement::art("track", [960.0, 900.0, 0.0], track)
                .stroke_paint(Paint::hex("#FFC45C"), 6.0),
        ],
    }
}

fn main() -> Result<()> {
    let plan = stage();
    let mut scene = PlanBuilder::new("art", 6 * SECOND);
    let stage = Stage::declare(&mut scene, "stage", &plan)?;
    let mut score = scene.score();

    // The emblem draws on as one pen line, fills, then draws off from its start.
    score.at(0, stage.channel("mark.fill", 0.0));
    score.at(seconds(0.2), stage.trim_in("mark", 1.8));
    score.at(seconds(2.0), stage.ease("mark.fill", 1.0, 0.6, Ease::GLIDE));
    score.at(
        seconds(4.2),
        stage.ease("mark.trim-start", 1.0, 1.2, Ease::DRAW),
    );

    // The figure morphs through its targets and back to the circle.
    score.at(seconds(0.6), stage.morph("shape", 1, 0.9));
    score.at(seconds(2.2), stage.morph("shape", 2, 0.9));
    score.at(seconds(3.8), stage.morph("shape", 0, 1.0));

    // A quarter of the track's stroke circles it, wrapping across the seam.
    score.at(0, stage.channel("track-rail.opacity", 0.25));
    score.at(0, stage.channel("track.trim-end", 0.22));
    score.at(0, stage.channel("track.trim-offset", 0.0));
    score.at(0, stage.ease("track.trim-offset", 2.0, 6.0, Ease::Linear));

    scene.finish()?.write_or_print(Some(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/art.json".into()),
    ))?;
    Ok(())
}
