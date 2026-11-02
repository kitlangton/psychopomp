//! Editorial reel: a warm-paper magazine spread that sets itself, on the
//! editorial kit. A masthead rule draws across, a serif headline rises line
//! by line with an italic vermilion turn and a swash under it, a line
//! drawing of hills and water draws on stroke by stroke, and a sun rises from
//! behind its horizon while the page holds.
use anyhow::Result;
use psychopomp::{
    art::{SvgArt, TrimMode},
    author::{SECOND, seconds},
    caption::CaptionAlign,
    kit::editorial::{self, INK, VERMILION},
    math::easing::Ease,
    paint::Paint,
    plan::SpringPlan,
    rig::ClipMask,
    stage::{StageActor, StageElement, StagePlan},
    typography::{SplitUnit, TypePlan},
};

const MUTED: &str = "#6E6961";
const LEFT: f32 = 140.0;
const RIGHT: f32 = 1780.0;
/// The drawing's center and its view box.
const FIGURE: [f32; 2] = [1390.0, 560.0];
const VIEW: [f32; 2] = [700.0, 520.0];
const HORIZON: f32 = 360.0;
/// The hills' ridge, left to right, in view-box units.
const RIDGE: &str = "C110 300 200 268 290 322 C350 292 430 236 520 292 C580 326 640 318 680 334";
/// The sky above the ridge, inside the frame: the ridge walked back right to left.
const SKY: &str = "M4 4 H696 V334 H680 C640 318 580 326 520 292 C430 236 350 292 290 322 C200 268 110 300 20 360 H4 Z";
/// Where the sun sits, hidden behind the right hill, before it rises.
const SUN: [f32; 2] = [560.0, 430.0];

fn left(spec: TypePlan) -> TypePlan {
    spec.align(CaptionAlign::Left)
}

fn right(spec: TypePlan) -> TypePlan {
    spec.align(CaptionAlign::Right)
}

fn landscape() -> SvgArt {
    let [w, h] = VIEW;
    SvgArt::new(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" fill="none" stroke="{INK}">
<rect x="2" y="2" width="{iw}" height="{ih}"/>
<path d="M20 {HORIZON} H680"/>
<path d="M20 {HORIZON} {RIDGE}"/>
<path d="M60 420 C100 405 140 435 180 420 S260 405 300 420"/>
<path d="M240 462 C280 447 320 477 360 462 S440 447 480 462"/>
<path d="M410 412 C450 397 490 427 530 412 S610 397 650 412"/>
</svg>"#,
        iw = w - 4.0,
        ih = h - 4.0,
    ))
}

fn sun() -> SvgArt {
    SvgArt::new(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 180 180"><circle cx="90" cy="90" r="78" fill="{VERMILION}" stroke="{INK}"/></svg>"#
    ))
}

fn stage() -> StagePlan {
    let kit = editorial::kit();
    let ink = Paint::hex(INK);
    let muted = Paint::hex(MUTED);
    let vermilion = Paint::hex(VERMILION);
    let [fx, fy] = FIGURE;
    kit.stage([
        // Masthead.
        StageElement::kinetic_type(
            "kicker",
            [LEFT, 112.0, 0.0],
            left(editorial::kicker("Field Notes · No. 7", 22.0, ink)),
        ),
        StageElement::kinetic_type(
            "dateline",
            [RIGHT, 112.0, 0.0],
            right(editorial::kicker("November 2026", 22.0, muted)),
        ),
        editorial::rule("masthead", [960.0, 160.0, 0.0], RIGHT - LEFT, ink, 1.5),
        // Headline column.
        StageElement::kinetic_type(
            "headline",
            [LEFT, 360.0, 0.0],
            left(editorial::kit().headline("The quiet\ncraft of", 150.0, ink))
                .split(SplitUnit::Line)
                .masked(true),
        ),
        StageElement::kinetic_type(
            "turn",
            [LEFT, 585.0, 0.0],
            left(kit.display.italic().spec("motion.", 150.0, vermilion))
                .split(SplitUnit::Line)
                .masked(true),
        ),
        editorial::flourish("swash", [LEFT + 230.0, 667.0, 0.0], 440.0, vermilion, 4.0),
        StageElement::kinetic_type(
            "standfirst",
            [LEFT, 800.0, 0.0],
            left(kit.caption(
                "Drawing with time, where every frame\nis a pure function of the clock.",
                32.0,
                muted,
            ))
            .split(SplitUnit::Line)
            .masked(true),
        ),
        // The figure: a sun that shows only in the sky above the hills, the
        // line drawing over it.
        StageElement::group("sky", [fx - VIEW[0] * 0.5, fy - VIEW[1] * 0.5, 0.0])
            .clip(ClipMask::path(SKY))
            .child(StageElement::art("sun", [SUN[0], SUN[1], 0.0], sun())),
        StageElement::art("drawing", [fx, fy, 0.0], landscape())
            .stroke_paint(ink, 2.5)
            .trim_mode(TrimMode::Sequence),
        StageElement::kinetic_type(
            "figure",
            [fx - VIEW[0] * 0.5, fy + VIEW[1] * 0.5 + 40.0, 0.0],
            left(kit.caption("Fig. 1  Line work drawn on with trim paths.", 22.0, muted)),
        ),
        // Colophon.
        editorial::rule("foot", [960.0, 968.0, 0.0], RIGHT - LEFT, ink, 1.0),
        StageElement::kinetic_type(
            "colophon",
            [LEFT, 1004.0, 0.0],
            left(kit.caption("Set in Instrument Serif and Instrument Sans", 22.0, muted)),
        ),
        StageElement::kinetic_type(
            "imprint",
            [RIGHT, 1004.0, 0.0],
            right(editorial::kicker("Psychopomp", 20.0, ink)),
        ),
    ])
}

fn main() -> Result<()> {
    let plan = stage();
    let kit = editorial::kit();
    let mut scene = kit.begin("editorial", 14 * SECOND);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let rise = SpringPlan::ENTER;

    // The kicker and dateline are set on the first frame; the masthead rule
    // draws across under them.
    s.trim_in(sc, "masthead", 0, 1.4);

    // The headline rises a line at a time, turns italic, and is underlined.
    s.rise_type(sc, "headline", seconds(0.6), seconds(0.22), 170.0, rise);
    s.rise_type(sc, "turn", seconds(1.15), 0, 170.0, rise);
    s.channel(sc, "swash.trim-end", 0.0);
    s.trim_in(sc, "swash", seconds(1.9), 0.9);
    s.rise_type(sc, "standfirst", seconds(2.5), seconds(0.16), 46.0, rise);

    // The drawing draws on one stroke after another, then the sun rises from
    // behind the horizon and keeps climbing while the page holds.
    s.channel(sc, "drawing.trim-end", 0.0);
    s.trim_in(sc, "drawing", seconds(3.2), 3.4);
    s.channel(sc, "figure.opacity", 0.0);
    s.ease(sc, "figure.opacity", seconds(6.0), 1.0, 0.8, Ease::GLIDE);
    let sun_y = s.channel(sc, "sun.y", 0.0);
    sc.spring_with(&sun_y, seconds(6.4), -150.0, SpringPlan::MOVE);
    s.ease(sc, "sun.y", seconds(7.6), -205.0, 6.4, Ease::GLIDE);

    // The colophon closes the page.
    s.channel(sc, "foot.trim-end", 0.0);
    s.trim_in(sc, "foot", seconds(7.8), 1.4);
    for (id, at) in [("colophon", 8.4), ("imprint", 8.6)] {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
        s.ease(
            sc,
            &format!("{id}.opacity"),
            seconds(at),
            1.0,
            0.7,
            Ease::GLIDE,
        );
    }

    scene.finish()?.write_or_print(Some(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/editorial.json".into()),
    ))?;
    Ok(())
}
