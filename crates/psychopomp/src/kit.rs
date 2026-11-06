//! Style Kits: a look picked from the brief. Each kit is one place that fixes
//! a film's canvas, palette, frame post, ground, and two type voices (display
//! and text), plus a few primitives that suit it. A Scene Program starts from
//! the kit the brief calls for, then authors ordinary Stage elements:
//!
//! | Brief | Kit |
//! |---|---|
//! | PR, bug, or architecture explainer on the dark editor theme | [`explainer`] |
//! | essay, story, or quiet announcement on warm paper | [`editorial`] |
//! | social poster, title card, or quote in portrait | [`kinetic_poster`] |
//! | bright product launch or feature reveal | [`product_showcase`] |
//! | code walkthrough where the code is the picture | [`ide`] |
//! | pitch or argument told in huge claims on colour fields | [`broadsheet`] |
//!
//! Every kit shares the [`scale`] (type sizes and the legibility floor),
//! [`code`] (highlighted code blocks the camera can frame), [`diagram`]
//! (nodes, drawn-on edges, packets), and [`field`] (colour fields). A film
//! may move between kits' rooms beat to beat: one visual idea per beat, and
//! the next beat changes scale, colour, or composition.
//!
//! Kits are values, not a framework: change any field (`.canvas(..)`,
//! `.palette`, a voice) when the brief asks for it.
pub mod broadsheet;
pub mod code;
pub mod diagram;
pub mod editorial;
pub mod explainer;
pub mod field;
pub mod ide;
pub mod kinetic_poster;
pub mod product_showcase;
pub mod scale;

use crate::{
    author::PlanBuilder,
    canvas::CanvasPlan,
    caption::CaptionAlign,
    face::Face,
    paint::{Paint, PalettePlan},
    rig::ClipMask,
    stage::{StageElement, StagePlan, StagePost},
    typography::{SplitUnit, TypePlan},
};

/// A bundled font file, its path relative to the workspace root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KitFont {
    pub family: &'static str,
    pub path: &'static str,
}

/// One typographic voice: a face (or a custom family) with its weight,
/// tracking, and leading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    pub face: Face,
    pub family: Option<&'static str>,
    pub weight: u16,
    pub italic: bool,
    /// Tracking in `em`.
    pub tracking: f32,
    pub line_height: f32,
}

impl Voice {
    pub const fn face(face: Face) -> Self {
        Self {
            face,
            family: None,
            weight: 400,
            italic: false,
            tracking: 0.0,
            line_height: 1.15,
        }
    }

    pub const fn family(family: &'static str) -> Self {
        Self {
            family: Some(family),
            ..Self::face(Face::Sans)
        }
    }

    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub const fn tracking(mut self, em: f32) -> Self {
        self.tracking = em;
        self
    }

    pub const fn line_height(mut self, ratio: f32) -> Self {
        self.line_height = ratio;
        self
    }

    /// Kinetic type in this voice, centered, in `paint`.
    pub fn spec(&self, text: impl Into<String>, size: f32, paint: Paint) -> TypePlan {
        let mut spec = TypePlan::new(text, self.face, size)
            .weight(self.weight)
            .tracking(self.tracking)
            .line_height(self.line_height)
            .align(CaptionAlign::Center)
            .paint(paint);
        if let Some(family) = self.family {
            spec = spec.family(family);
        }
        if self.italic {
            spec = spec.italic();
        }
        spec
    }

    /// A type matte in this voice: children show only inside its glyphs.
    pub fn matte(&self, text: impl Into<String>, size: f32) -> ClipMask {
        ClipMask::Type {
            text: text.into(),
            face: self.face,
            family: self.family.map(Into::into),
            size,
            weight: self.weight,
            tracking: self.tracking,
            invert: false,
        }
    }
}

/// A film's look, chosen from the brief.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleKit {
    pub name: &'static str,
    pub canvas: CanvasPlan,
    /// `None` keeps the render theme's colors (`--theme`).
    pub palette: Option<PalettePlan>,
    pub post: StagePost,
    /// A full-frame paint behind every Stage element, if any.
    pub ground: Option<Paint>,
    pub display: Voice,
    pub text: Voice,
    pub fonts: &'static [KitFont],
}

impl StyleKit {
    /// The same kit on another canvas (aspect, resolution, or frame rate).
    pub fn canvas(mut self, canvas: CanvasPlan) -> Self {
        self.canvas = canvas;
        self
    }

    /// A new plan with this kit's canvas, palette, and fonts.
    pub fn begin(&self, id: impl Into<String>, duration_nanos: u64) -> PlanBuilder {
        let mut scene = PlanBuilder::new(id, duration_nanos);
        self.apply(&mut scene);
        scene
    }

    /// Set this kit's canvas, palette, and fonts on `scene`. The explainer
    /// kit sets nothing, so existing plans stay byte-identical.
    pub fn apply(&self, scene: &mut PlanBuilder) {
        if !self.canvas.is_default() {
            scene.set_canvas(self.canvas);
        }
        if let Some(palette) = self.palette {
            scene.set_palette(palette);
        }
        for font in self.fonts {
            scene.add_font(font.family, font.path);
        }
    }

    /// A Stage on this kit's post, its ground (if any) painted first.
    pub fn stage(&self, elements: impl IntoIterator<Item = StageElement>) -> StagePlan {
        let ground = self
            .ground
            .map(|paint| StageElement::backdrop("ground", paint));
        StagePlan {
            post: self.post,
            elements: ground.into_iter().chain(elements).collect(),
        }
    }

    /// Display type: headlines, numbers, and posters.
    pub fn headline(&self, text: impl Into<String>, size: f32, paint: Paint) -> TypePlan {
        self.display.spec(text, size, paint)
    }

    /// Text type: kickers, captions, and credits.
    pub fn caption(&self, text: impl Into<String>, size: f32, paint: Paint) -> TypePlan {
        self.text.spec(text, size, paint)
    }

    /// A display-type matte for footage, gradients, or shaders.
    pub fn matte(&self, text: impl Into<String>, size: f32) -> ClipMask {
        self.display.matte(text, size)
    }

    /// The canvas center, where a kit's compositions are balanced.
    pub fn center(&self) -> [f32; 2] {
        self.canvas.center()
    }
}

/// Display type split into `unit`s that rise through their own line slits.
pub fn masked(spec: TypePlan, unit: SplitUnit) -> TypePlan {
    spec.split(unit).masked(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::author::SECOND;

    fn kits() -> [StyleKit; 6] {
        [
            explainer::kit(),
            editorial::kit(),
            kinetic_poster::kit(),
            product_showcase::kit(),
            ide::kit(),
            broadsheet::kit(),
        ]
    }

    #[test]
    fn the_explainer_kit_leaves_a_plan_untouched() {
        let plain = PlanBuilder::new("a", SECOND).finish().unwrap();
        let kitted = explainer::kit().begin("a", SECOND).finish().unwrap();
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::to_value(&kitted).unwrap()
        );
    }

    #[test]
    fn every_other_kit_sets_its_palette_and_fonts() {
        for kit in &kits()[1..] {
            let plan = kit.begin(kit.name, SECOND).finish().unwrap();
            assert_eq!(plan.palette, kit.palette, "{}", kit.name);
            assert_eq!(plan.canvas, kit.canvas, "{}", kit.name);
            let families = plan
                .fonts
                .iter()
                .map(|font| font.family.as_str())
                .collect::<Vec<_>>();
            for voice in [kit.display, kit.text] {
                if let Some(family) = voice.family {
                    assert!(families.contains(&family), "{}: {family}", kit.name);
                }
            }
        }
    }

    #[test]
    fn the_poster_is_portrait() {
        assert_eq!(kinetic_poster::kit().canvas.size(), [1080, 1920]);
    }

    #[test]
    fn kit_fonts_ship_in_the_workspace() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for kit in kits() {
            for font in kit.fonts {
                assert!(root.join(font.path).is_file(), "{}", font.path);
            }
        }
    }

    #[test]
    fn a_kit_stage_paints_its_ground_first() {
        let kit = editorial::kit();
        let stage = kit.stage([StageElement::rect("box", [0.0; 3], [10.0, 10.0])]);
        assert_eq!(stage.elements[0].id(), "ground");
        assert_eq!(stage.post, kit.post);
        stage.validate().unwrap();
        assert_eq!(explainer::kit().stage([]).elements.len(), 0);
    }

    #[test]
    fn a_voice_carries_its_family_into_type_and_mattes() {
        let kit = product_showcase::kit();
        let spec = kit.headline("GO", 200.0, Paint::hex("#111"));
        assert_eq!(spec.family.as_deref(), kit.display.family);
        assert_eq!(spec.tracking, kit.display.tracking);
        let ClipMask::Type {
            family, tracking, ..
        } = kit.matte("GO", 200.0)
        else {
            panic!("a type matte");
        };
        assert_eq!(family.as_deref(), kit.display.family);
        assert_eq!(tracking, kit.display.tracking);
    }
}
