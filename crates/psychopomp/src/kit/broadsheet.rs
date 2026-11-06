//! The broadsheet kit: the claim is the picture. Huge condensed Anton on
//! flat fields of vermilion, cobalt, and paper, with Instrument Sans for the
//! line under it and code set on paper in the day [`SyntaxTheme`]. Beats
//! change room by flooding a new [`field`](super::field) rather than by
//! fading on the same black.
use crate::{
    canvas::CanvasPlan,
    paint::{Paint, PalettePlan},
    stage::StagePost,
    syntax::SyntaxTheme,
    typography::TypePlan,
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

pub const THEME: SyntaxTheme = SyntaxTheme::day();
pub const PAPER: &str = "#F2EDE4";
pub const INK: &str = "#141414";
pub const VERMILION: &str = "#E8401C";
pub const COBALT: &str = "#1F3FD1";
/// Signal yellow, for one word on cobalt or ink.
pub const SIGNAL: &str = "#FFC400";

pub const PALETTE: PalettePlan = PalettePlan {
    background: [242, 237, 228],
    surface: [232, 224, 210],
    raised: [220, 210, 194],
    text: [20, 20, 20],
    muted: [104, 98, 90],
    accent: [232, 64, 28],
    request: [31, 63, 209],
    success: [46, 111, 64],
    warning: [255, 196, 0],
    error: [196, 43, 35],
};

/// 1920x1080 on paper: Anton over Instrument Sans.
pub fn kit() -> StyleKit {
    StyleKit {
        name: "broadsheet",
        canvas: CanvasPlan::landscape_1080p(),
        palette: Some(PALETTE),
        post: StagePost {
            bloom: 0.0,
            grain: 0.022,
            vignette: 0.06,
            backdrop: 0.0,
        },
        ground: Some(Paint::hex(PAPER)),
        display: Voice::family(DISPLAY).tracking(-0.005).line_height(0.92),
        text: Voice::family(editorial::SANS)
            .tracking(0.0)
            .line_height(1.25),
        fonts: FONTS,
    }
}

/// A display line set in capitals, as Anton wants to be.
pub fn shout(text: &str, size: f32, paint: Paint) -> TypePlan {
    kit().headline(text.to_uppercase(), size, paint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shout_is_capital_anton() {
        let spec = shout("the catalog", 200.0, Paint::hex(INK));
        assert_eq!(spec.text, "THE CATALOG");
        assert_eq!(spec.family.as_deref(), Some(DISPLAY));
    }
}
