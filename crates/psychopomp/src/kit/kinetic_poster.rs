//! The kinetic poster kit: portrait social posters, title cards, and quotes.
//! 1080x1920, a chalk Swiss ground, condensed Anton stacked tight over
//! Instrument Sans credits, one signal orange, and lines that rise through
//! their own slits instead of fading.
use crate::{
    canvas::CanvasPlan,
    paint::{Paint, PalettePlan},
    rig::ClipMask,
    stage::{StageElement, StagePost},
    typography::{SplitUnit, TypePlan},
};

use super::{KitFont, StyleKit, Voice, editorial};

pub const DISPLAY: &str = "Anton";

pub const FONTS: &[KitFont] = &[
    KitFont {
        family: DISPLAY,
        path: "assets/fonts/Anton-Regular.ttf",
    },
    KitFont {
        family: editorial::SANS,
        path: "assets/fonts/InstrumentSans.ttf",
    },
];

pub const INK: &str = "#111110";
pub const SIGNAL: &str = "#FF3B1D";
pub const CHALK: &str = "#F3F0E8";

/// 1080x1920 portrait, condensed display over a plain sans.
pub fn kit() -> StyleKit {
    StyleKit {
        name: "kinetic-poster",
        canvas: CanvasPlan::portrait_1080p(),
        palette: Some(PalettePlan::swiss_poster()),
        post: StagePost {
            grain: 0.012,
            ..StagePost::FLAT
        },
        ground: Some(Paint::hex(CHALK)),
        display: Voice::family(DISPLAY).tracking(-0.005).line_height(0.9),
        text: Voice::family(editorial::SANS).tracking(0.04),
        fonts: FONTS,
    }
}

/// Poster lines, one per `\n`, each rising through its own slit.
pub fn stack(text: &str, size: f32, paint: Paint) -> TypePlan {
    super::masked(kit().headline(text, size, paint), SplitUnit::Line)
}

/// A flat block of color with no stroke.
pub fn block(id: &str, at: [f32; 3], size: [f32; 2], paint: Paint) -> StageElement {
    StageElement::rect(id, at, size).stroke(None).paint(paint)
}

/// A group whose children show only inside a `size` window: they slide in
/// and out past its invisible edges.
pub fn slot(id: &str, at: [f32; 3], size: [f32; 2]) -> StageElement {
    StageElement::group(id, at).clip(ClipMask::rect(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_rises_line_by_line_through_slits() {
        let spec = stack("MAKE\nTHINGS\nMOVE", 300.0, Paint::hex(INK));
        assert!(spec.masked);
        assert_eq!(spec.split, SplitUnit::Line);
        assert_eq!(spec.unit_count(), 3);
        assert_eq!(spec.family.as_deref(), Some(DISPLAY));
    }

    #[test]
    fn blocks_and_slots_validate_on_a_stage() {
        let kit = kit();
        let stage = kit.stage([
            block(
                "bar",
                [540.0, 960.0, 0.0],
                [900.0, 40.0],
                Paint::hex(SIGNAL),
            ),
            slot("slot", [540.0, 960.0, 0.0], [900.0, 300.0]).child(StageElement::kinetic_type(
                "line",
                [0.0; 3],
                stack("GO", 200.0, Paint::hex(INK)),
            )),
        ]);
        stage.validate().unwrap();
    }
}
