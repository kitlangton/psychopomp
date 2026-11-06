//! Product launch reel: a bright launch on the product showcase kit. Footage
//! plays inside heavy display type, the word lifts to make room for three
//! feature cards, then returns for the closing line and an "out now" pill.
use anyhow::Result;
use psychopomp::{
    author::{SECOND, seconds},
    caption::CaptionAlign,
    footage::{self, Clip},
    kit::product_showcase::{self, BLUE, INK, MUTED, footage, type_window},
    math::easing::Ease,
    paint::Paint,
    plan::SpringPlan,
    stage::{StageActor, StageElement, StagePlan},
    typography::SplitUnit,
};

const CENTER: [f32; 2] = [960.0, 520.0];
const WORD: f32 = 400.0;
const CARD: [f32; 2] = [440.0, 230.0];
const CARDS: [(&str, &str, &str); 3] = [
    ("linear", "Linear HDR", "Rgba16Float, end to end"),
    ("blur", "24× motion blur", "A real shutter, every frame"),
    ("mattes", "Masks and mattes", "Footage inside any glyph"),
];

fn card(id: &str, x: f32, title: &str, line: &str) -> StageElement {
    let kit = product_showcase::kit();
    let inset = -CARD[0] * 0.5 + 48.0;
    StageElement::group(id, [x, 760.0, 0.0]).children([
        StageElement::rect(&format!("{id}-shadow"), [0.0, 16.0, 0.0], CARD)
            .corner(32.0)
            .stroke(None)
            .paint(Paint::rgba(30, 50, 90, 34)),
        StageElement::rect(&format!("{id}-face"), [0.0; 3], CARD)
            .corner(32.0)
            .stroke(None)
            .paint(Paint::hex("#FFFFFF")),
        StageElement::rect(
            &format!("{id}-dot"),
            [inset + 8.0, -62.0, 0.0],
            [16.0, 16.0],
        )
        .corner(8.0)
        .stroke(None)
        .paint(Paint::hex(BLUE)),
        StageElement::kinetic_type(
            &format!("{id}-title"),
            [inset, 4.0, 0.0],
            kit.caption(title, 40.0, Paint::hex(INK))
                .align(CaptionAlign::Left),
        ),
        StageElement::kinetic_type(
            &format!("{id}-line"),
            [inset, 58.0, 0.0],
            kit.caption(line, 24.0, Paint::hex(MUTED))
                .align(CaptionAlign::Left),
        ),
    ])
}

fn stage() -> StagePlan {
    let kit = product_showcase::kit();
    let [cx, cy] = CENTER;
    let fill = footage(
        "word-fill",
        [0.0; 3],
        [2000.0, 1000.0],
        Clip::new("gradient-media").bouncing(),
    );
    let mut elements = vec![
        StageElement::kinetic_type(
            "kicker",
            [cx, 200.0, 0.0],
            kit.text
                .tracking(0.3)
                .spec("INTRODUCING", 26.0, Paint::hex(MUTED)),
        ),
        type_window("word", [cx, cy, 0.0], "VIVID", WORD, fill),
        StageElement::kinetic_type(
            "tagline",
            [cx, 830.0, 0.0],
            kit.caption("Footage, inside the letters.", 48.0, Paint::hex(INK))
                .split(SplitUnit::Line)
                .masked(true),
        ),
        StageElement::kinetic_type(
            "closer",
            [cx, 830.0, 0.0],
            kit.caption("Psychopomp. Out now.", 48.0, Paint::hex(INK))
                .split(SplitUnit::Line)
                .masked(true),
        ),
        StageElement::group("pill", [cx, 930.0, 0.0]).children([
            StageElement::rect("pill-face", [0.0; 3], [220.0, 64.0])
                .corner(32.0)
                .stroke(None)
                .paint(Paint::hex(BLUE)),
            StageElement::kinetic_type(
                "pill-text",
                [0.0, 0.0, 0.0],
                kit.text
                    .tracking(0.18)
                    .spec("NOV 2", 26.0, Paint::hex("#FFFFFF")),
            ),
        ]),
    ];
    for (k, (id, title, line)) in CARDS.into_iter().enumerate() {
        elements.push(card(
            id,
            cx + (k as f32 - 1.0) * (CARD[0] + 40.0),
            title,
            line,
        ));
    }
    kit.stage(elements)
}

fn main() -> Result<()> {
    let plan = stage();
    let kit = product_showcase::kit();
    let mut scene = kit.begin("product-launch", 14 * SECOND);
    let media = footage::media(
        "gradient-media",
        "../assets/footage/gradient.mp4",
        0,
        scene.duration_nanos(),
    );
    scene.media(media);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let rise = SpringPlan::ENTER;

    // The kicker is set on the first frame and the word arrives under it;
    // footage drifts under the glyphs for the whole film.
    s.channel(sc, "word.opacity", 0.0);
    s.ease(sc, "word.opacity", seconds(0.3), 1.0, 0.3, Ease::GLIDE);
    let scale = s.channel(sc, "word.scale", 0.86);
    sc.spring_with(&scale, seconds(0.3), 1.0, SpringPlan::PANEL);
    s.channel(sc, "word-fill.x", -200.0);
    s.ease(sc, "word-fill.x", seconds(0.3), 200.0, 13.0, Ease::GLIDE);
    s.rise_type(sc, "tagline", seconds(1.3), 0, 64.0, rise);

    // The word lifts and shrinks to make room; three feature cards rise in.
    s.ease(sc, "tagline.opacity", seconds(4.3), 0.0, 0.35, Ease::GLIDE);
    s.ease(sc, "kicker.opacity", seconds(4.3), 0.0, 0.35, Ease::GLIDE);
    let y = s.channel(sc, "word.y", 0.0);
    sc.spring_with(&y, seconds(4.5), -140.0, SpringPlan::MOVE);
    sc.spring_with(&scale, seconds(4.5), 0.62, SpringPlan::MOVE);
    for (k, (id, ..)) in CARDS.into_iter().enumerate() {
        let at = seconds(4.8 + 0.12 * k as f64);
        s.channel(sc, &format!("{id}.opacity"), 0.0);
        s.ease(sc, &format!("{id}.opacity"), at, 1.0, 0.3, Ease::GLIDE);
        s.channel(sc, &format!("{id}-shadow.blur"), 22.0);
        let y = s.channel(sc, &format!("{id}.y"), 90.0);
        sc.spring_with(&y, at, 0.0, rise);
    }

    // The cards leave, the word returns, and the closing line and pill land.
    for (k, (id, ..)) in CARDS.into_iter().enumerate() {
        let at = seconds(9.4 + 0.08 * k as f64);
        s.ease(sc, &format!("{id}.opacity"), at, 0.0, 0.3, Ease::GLIDE);
        s.ease(sc, &format!("{id}.y"), at, -40.0, 0.4, Ease::GLIDE);
    }
    sc.spring_with(&y, seconds(9.8), 0.0, SpringPlan::MOVE);
    sc.spring_with(&scale, seconds(9.8), 1.0, SpringPlan::MOVE);
    s.rise_type(sc, "closer", seconds(10.4), 0, 64.0, rise);
    s.channel(sc, "pill.opacity", 0.0);
    s.ease(sc, "pill.opacity", seconds(10.8), 1.0, 0.25, Ease::GLIDE);
    let pill = s.channel(sc, "pill.scale", 0.7);
    sc.spring_with(&pill, seconds(10.8), 1.0, SpringPlan::PANEL);

    scene.finish()?.write_or_print(Some(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/product-launch.json".into()),
    ))?;
    Ok(())
}
