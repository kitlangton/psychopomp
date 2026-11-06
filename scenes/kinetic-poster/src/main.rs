//! Kinetic poster reel: a 1080x1920 portrait Swiss poster on the kinetic
//! poster kit. Condensed lines rise through their own slits, leave the same
//! way they came, and give way to a signal-orange second stack; rules and a
//! credit block slide in from behind invisible edges.
use anyhow::Result;
use psychopomp::{
    author::{SECOND, seconds},
    caption::CaptionAlign,
    kit::kinetic_poster::{self, CHALK, INK, SIGNAL, block, slot, stack},
    math::easing::Ease,
    paint::Paint,
    plan::SpringPlan,
    stage::{StageActor, StageElement, StagePlan},
    typography::{SplitUnit, TypePlan},
};

const LEFT: f32 = 80.0;
const RIGHT: f32 = 1000.0;
const WIDTH: f32 = RIGHT - LEFT;
const MID: f32 = 540.0;
/// The first stack's size and its line pitch (the kit's 0.9 leading).
const FIRST: f32 = 340.0;
const SECOND_SIZE: f32 = 260.0;
/// Where the slide-in credit block rests.
const CREDITS_Y: f32 = 1660.0;
const CREDITS_H: f32 = 300.0;
/// How far a line sits below (or leaves above) its slit: the slit is the
/// line's raster box, taller than the line pitch.
const TRAVEL: f32 = 1.5;
/// Ease-in for exits: units leave their slits still accelerating.
const LEAVE: Ease = Ease::CubicBezier([0.55, 0.0, 0.8, 0.2]);

fn left(spec: TypePlan) -> TypePlan {
    spec.align(CaptionAlign::Left)
}

fn right(spec: TypePlan) -> TypePlan {
    spec.align(CaptionAlign::Right)
}

fn credit(text: &str, size: f32, paint: Paint) -> TypePlan {
    kinetic_poster::kit()
        .text
        .tracking(0.16)
        .spec(text.to_uppercase(), size, paint)
}

fn stage() -> StagePlan {
    let kit = kinetic_poster::kit();
    let ink = Paint::hex(INK);
    let signal = Paint::hex(SIGNAL);
    let chalk = Paint::hex(CHALK);
    kit.stage([
        StageElement::kinetic_type(
            "kicker",
            [LEFT, 120.0, 0.0],
            left(credit("Psychopomp / Motion poster", 24.0, ink)),
        ),
        StageElement::kinetic_type("issue", [RIGHT, 120.0, 0.0], right(credit("No. 01", 24.0, ink))),
        slot("top-rule", [MID, 168.0, 0.0], [WIDTH, 12.0]).child(block(
            "top-bar",
            [0.0; 3],
            [WIDTH, 6.0],
            ink,
        )),
        StageElement::kinetic_type(
            "first",
            [LEFT - 6.0, 760.0, 0.0],
            left(stack("MAKE\nTHINGS\nMOVE.", FIRST, ink)),
        ),
        StageElement::kinetic_type(
            "second",
            [LEFT - 6.0, 760.0, 0.0],
            left(stack("ONE\nFRAME\nAT A\nTIME.", SECOND_SIZE, signal)),
        ),
        slot("bar-slot", [MID, 1300.0, 0.0], [WIDTH, 40.0]).child(block(
            "bar",
            [0.0; 3],
            [WIDTH, 28.0],
            signal,
        )),
        StageElement::kinetic_type(
            "note",
            [LEFT, 1390.0, 0.0],
            left(
                kit.caption(
                    "Every frame is a pure function of time,\nsampled 24 times for its motion blur.",
                    34.0,
                    ink,
                )
                .split(SplitUnit::Line)
                .masked(true),
            ),
        ),
        slot("credits", [MID, CREDITS_Y, 0.0], [WIDTH, CREDITS_H]).children([
            block("credits-block", [0.0; 3], [WIDTH, CREDITS_H], ink),
            StageElement::kinetic_type(
                "credits-head",
                [-WIDTH * 0.5 + 48.0, -70.0, 0.0],
                left(kit.headline("RENDERED IN RUST", 88.0, chalk)),
            ),
            StageElement::kinetic_type(
                "credits-line",
                [-WIDTH * 0.5 + 48.0, 50.0, 0.0],
                left(credit("1080 × 1920 · 60 fps · 24 samples", 26.0, chalk)),
            ),
            StageElement::kinetic_type(
                "credits-mark",
                [WIDTH * 0.5 - 48.0, 50.0, 0.0],
                right(credit("2026", 26.0, Paint::hex(SIGNAL))),
            ),
        ]),
    ])
}

fn main() -> Result<()> {
    let plan = stage();
    let kit = kinetic_poster::kit();
    let mut scene = kit.begin("kinetic-poster", 12 * SECOND);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let rise = SpringPlan::ENTER;

    // Masthead: the credits are set on the first frame and the rule slides
    // in under them from behind the left edge.
    let x = s.channel(sc, "top-bar.x", -WIDTH);
    sc.spring_with(&x, 0, 0.0, SpringPlan::MOVE);

    // The first stack rises a line at a time and the orange bar shoots in under it.
    s.rise_type(
        sc,
        "first",
        seconds(0.3),
        seconds(0.14),
        FIRST * TRAVEL,
        rise,
    );
    let x = s.channel(sc, "bar.x", -WIDTH);
    sc.spring_with(&x, seconds(1.3), 0.0, SpringPlan::MOVE);
    s.rise_type(sc, "note", seconds(1.7), seconds(0.12), 50.0, rise);

    // The first stack leaves up through its slits; the orange stack rises in.
    for unit in 0..3 {
        s.ease(
            sc,
            &format!("first.u{unit}.y"),
            seconds(4.4 + 0.1 * unit as f64),
            -FIRST * TRAVEL,
            0.42,
            LEAVE,
        );
    }
    s.rise_type(
        sc,
        "second",
        seconds(4.85),
        seconds(0.14),
        SECOND_SIZE * TRAVEL,
        rise,
    );

    // The bar swaps sides, and the credit block slides up from behind its
    // bottom edge carrying its lines.
    s.ease(sc, "bar.x", seconds(4.4), WIDTH, 0.5, Ease::GLIDE);
    s.set(sc, "bar.x", seconds(4.95), -WIDTH);
    let x = s.channel(sc, "bar.x", -WIDTH);
    sc.spring_with(&x, seconds(5.0), 0.0, SpringPlan::MOVE);
    for (id, at) in [
        ("credits-block", 6.6),
        ("credits-head", 6.75),
        ("credits-line", 6.9),
        ("credits-mark", 6.9),
    ] {
        let y = s.channel(sc, &format!("{id}.y"), CREDITS_H);
        sc.spring_with(&y, seconds(at), 0.0, SpringPlan::MOVE);
    }

    // The orange stack opens its tracking a touch while the poster holds.
    s.sweep_tracking(sc, "second", seconds(7.6), 0.0, 0.012, 4.0);

    scene.finish()?.write_or_print(Some(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/kinetic-poster.json".into()),
    ))?;
    Ok(())
}
