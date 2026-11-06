//! The product showcase kit: bright launches and feature reveals. Daylight
//! glass palette on a white-to-sky ground, heavy Archivo Black display over
//! Instrument Sans, footage and gradients shown through display type and
//! rounded product windows.
use crate::{
    canvas::CanvasPlan,
    footage::{Clip, Fit, Mask},
    paint::{Paint, PalettePlan},
    rig::ClipMask,
    stage::{StageElement, StagePost},
    tone::Tone,
};

use super::{KitFont, StyleKit, Voice, editorial};

pub const DISPLAY: &str = "Archivo Black";

pub const FONTS: &[KitFont] = &[
    KitFont {
        family: DISPLAY,
        path: "assets/fonts/ArchivoBlack-Regular.ttf",
    },
    KitFont {
        family: editorial::SANS,
        path: "assets/fonts/InstrumentSans.ttf",
    },
];

pub const INK: &str = "#111318";
pub const MUTED: &str = "#5C6474";
pub const BLUE: &str = "#0066CC";

/// 1920x1080 daylight, heavy display over a clean sans.
pub fn kit() -> StyleKit {
    StyleKit {
        name: "product-showcase",
        canvas: CanvasPlan::landscape_1080p(),
        palette: Some(PalettePlan::light_glass()),
        post: StagePost {
            bloom: 0.06,
            grain: 0.006,
            vignette: 0.0,
            backdrop: 0.0,
        },
        ground: Some(Paint::linear_hex(90.0, "#FFFFFF", "#E4ECF7")),
        display: Voice::family(DISPLAY).tracking(-0.03).line_height(0.95),
        text: Voice::family(editorial::SANS).tracking(0.0),
        fonts: FONTS,
    }
}

/// Footage `size` pixels at `at`, covering its box.
pub fn footage(id: &str, at: [f32; 3], size: [f32; 2], clip: Clip) -> StageElement {
    StageElement::Footage {
        id: id.into(),
        at,
        size,
        clip,
        fit: Fit::Cover,
        mask: Mask::default(),
        framed: false,
        tint: Tone::Accent,
    }
}

/// `fill` (footage, a gradient, a shader) shown only inside `word` in the
/// display voice, centered on `at`.
pub fn type_window(
    id: &str,
    at: [f32; 3],
    word: &str,
    size: f32,
    fill: StageElement,
) -> StageElement {
    StageElement::group(id, at)
        .clip(kit().matte(word, size))
        .child(fill)
}

/// A rounded product window: children show inside a `size` rounded rectangle.
pub fn window(id: &str, at: [f32; 3], size: [f32; 2], corner: f32) -> StageElement {
    StageElement::group(id, at).clip(ClipMask::rect(size).corner(corner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footage_plays_inside_display_type() {
        let fill = footage(
            "clip",
            [0.0; 3],
            [1200.0, 600.0],
            Clip::new("media").looping(),
        );
        let StageElement::Group {
            clip: Some(ClipMask::Type { text, family, .. }),
            children,
            ..
        } = type_window("word", [960.0, 540.0, 0.0], "VIVID", 300.0, fill)
        else {
            panic!("a type-matted group");
        };
        assert_eq!(text, "VIVID");
        assert_eq!(family.as_deref(), Some(DISPLAY));
        assert_eq!(children[0].id(), "clip");
    }
}
