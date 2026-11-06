//! The IDE kit: code is the picture. Full-bleed highlighted code on
//! near-black in the night [`SyntaxTheme`], the focused line bright and the
//! rest dimmed, the camera dollying into the line the voice names, and
//! Archivo Black headlines when a claim needs saying. Edges and packets take
//! the palette's amber accent.
use crate::{
    canvas::CanvasPlan,
    face::Face,
    paint::{Paint, PalettePlan},
    stage::StagePost,
    syntax::SyntaxTheme,
};

use super::{KitFont, StyleKit, Voice};

pub const DISPLAY: &str = "Archivo Black";

pub const FONTS: &[KitFont] = &[KitFont {
    family: DISPLAY,
    path: "assets/fonts/ArchivoBlack-Regular.ttf",
}];

pub const THEME: SyntaxTheme = SyntaxTheme::night();
/// Near-black ground with a cool lift.
pub const GROUND: &str = "#0B0D12";
pub const INK: &str = "#E8EAF0";
pub const MUTED: &str = "#707A8C";
/// The one accent: amber, for edges, packets, and the word that matters.
pub const AMBER: &str = "#FFB020";

pub const PALETTE: PalettePlan = PalettePlan {
    background: [11, 13, 18],
    surface: [20, 24, 32],
    raised: [30, 36, 50],
    text: [232, 234, 240],
    muted: [112, 122, 140],
    accent: [255, 176, 32],
    request: [92, 212, 255],
    success: [110, 231, 150],
    warning: [255, 159, 67],
    error: [255, 107, 107],
};

/// 1920x1080 on near-black: Archivo Black over CommitMono.
pub fn kit() -> StyleKit {
    StyleKit {
        name: "ide",
        canvas: CanvasPlan::landscape_1080p(),
        palette: Some(PALETTE),
        post: StagePost {
            bloom: 0.12,
            grain: 0.016,
            vignette: 0.22,
            backdrop: 0.0,
        },
        ground: Some(Paint::radial_hex([0.42, 0.38], 1.1, "#121620", GROUND)),
        display: Voice::family(DISPLAY).tracking(-0.02).line_height(0.98),
        text: Voice::face(Face::Mono).line_height(1.3),
        fonts: FONTS,
    }
}
