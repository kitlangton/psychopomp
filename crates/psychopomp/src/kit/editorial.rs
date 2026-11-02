//! The editorial kit: essays, stories, and quiet announcements on warm paper.
//! Instrument Serif headlines over Instrument Sans text, carbon ink with a
//! vermilion accent, a flat frame with a little paper tooth, and line work
//! that draws on with `trim_in`.
use crate::{
    art::SvgArt,
    canvas::CanvasPlan,
    paint::{Paint, PalettePlan},
    stage::{StageElement, StagePost},
    typography::TypePlan,
};

use super::{KitFont, StyleKit, Voice};

pub const SERIF: &str = "Instrument Serif";
pub const SANS: &str = "Instrument Sans";

pub const FONTS: &[KitFont] = &[
    KitFont {
        family: SERIF,
        path: "assets/fonts/InstrumentSerif-Regular.ttf",
    },
    KitFont {
        family: SERIF,
        path: "assets/fonts/InstrumentSerif-Italic.ttf",
    },
    KitFont {
        family: SANS,
        path: "assets/fonts/InstrumentSans.ttf",
    },
];

/// Carbon ink on the paper palette.
pub const INK: &str = "#1B1A18";
/// The paper palette's vermilion.
pub const VERMILION: &str = "#D9381E";

/// 1920x1080 on warm paper, serif over sans.
pub fn kit() -> StyleKit {
    StyleKit {
        name: "editorial",
        canvas: CanvasPlan::landscape_1080p(),
        palette: Some(PalettePlan::paper()),
        post: StagePost {
            bloom: 0.0,
            grain: 0.018,
            vignette: 0.1,
            backdrop: 0.0,
        },
        ground: Some(Paint::radial_hex([0.5, 0.42], 0.95, "#F8F3EA", "#E8DFCE")),
        display: Voice::family(SERIF).tracking(-0.015).line_height(1.0),
        text: Voice::family(SANS).tracking(0.01).line_height(1.35),
        fonts: FONTS,
    }
}

/// A small tracked uppercase line above a headline.
pub fn kicker(text: &str, size: f32, paint: Paint) -> TypePlan {
    kit()
        .text
        .tracking(0.22)
        .spec(text.to_uppercase(), size, paint)
}

/// A straight hairline `length` pixels long, centered on `at`; it starts
/// drawn off, so `trim_in` draws it on.
pub fn rule(id: &str, at: [f32; 3], length: f32, paint: Paint, width: f32) -> StageElement {
    let art = SvgArt::new(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {length} 8"><path d="M0 4 H{length}" fill="none" stroke="{INK}"/></svg>"#
    ));
    StageElement::art(id, at, art).stroke_paint(paint, width)
}

/// A hand-drawn swash underline `length` pixels wide, centered on `at`.
pub fn flourish(id: &str, at: [f32; 3], length: f32, paint: Paint, width: f32) -> StageElement {
    let (w, h) = (length, length * 0.08);
    let art = SvgArt::new(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><path d="M0 {y0} C{c1x} {c1y} {c2x} {c2y} {mx} {my} S{ex1} {ey1} {w} {y1}" fill="none" stroke="{INK}"/></svg>"#,
        y0 = h * 0.7,
        c1x = w * 0.18,
        c1y = h * 0.15,
        c2x = w * 0.32,
        c2y = h * 0.95,
        mx = w * 0.52,
        my = h * 0.55,
        ex1 = w * 0.82,
        ey1 = h * 0.05,
        y1 = h * 0.35,
    ));
    StageElement::art(id, at, art).stroke_paint(paint, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_work_is_vector_art_that_validates() {
        let ink = Paint::hex(INK);
        for element in [
            rule("rule", [0.0; 3], 600.0, ink, 2.0),
            flourish("swash", [0.0; 3], 500.0, ink, 3.0),
        ] {
            let StageElement::Art { art, stroke, .. } = &element else {
                panic!("art");
            };
            assert_eq!(*stroke, Some(ink));
            art.validate().unwrap();
        }
    }

    #[test]
    fn a_kicker_is_tracked_uppercase_sans() {
        let spec = kicker("an essay", 22.0, Paint::hex(INK));
        assert_eq!(spec.text, "AN ESSAY");
        assert_eq!(spec.family.as_deref(), Some(SANS));
        assert!(spec.tracking > 0.1);
    }
}
