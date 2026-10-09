//! Native visual trials: measured glyph runs and coverage-unioned Bezier ink.
//! No layout state, callbacks, or independent animation clock lives here.
use super::*;
use psychopomp::component_prototype::{Font, TextPart};

pub(crate) struct PrototypeGlyphs {
    sprite: TextSprite,
    themed: theme::ThemedCache<TextSprite>,
}

impl PrototypeGlyphs {
    pub(crate) fn width(&self) -> f32 {
        self.sprite.advance
    }
    pub(crate) fn height(&self) -> f32 {
        self.sprite.height as f32
    }
}

impl HeadlessRenderer {
    pub(crate) fn prepare_prototype_glyphs(
        &mut self,
        text: &str,
        font: Font,
        size: f32,
        color: [u8; 3],
    ) -> PrototypeGlyphs {
        self.prepare_prototype_spans(
            &[StyledSpan {
                text: text.into(),
                style: SyntaxStyle::Rgb(color[0], color[1], color[2]),
            }],
            font,
            size,
        )
    }

    pub(crate) fn prepare_prototype_part(
        &mut self,
        part: &TextPart,
        font: Font,
        size: f32,
    ) -> PrototypeGlyphs {
        if part.spans.is_empty() {
            self.prepare_prototype_glyphs(&part.text, font, size, part.color)
        } else {
            self.prepare_prototype_spans(&part.spans, font, size)
        }
    }

    fn prepare_prototype_spans(
        &mut self,
        spans: &[StyledSpan],
        font: Font,
        size: f32,
    ) -> PrototypeGlyphs {
        let family = match font {
            Font::Sans => fonts::SANS,
            Font::Mono => fonts::mono(),
        };
        let attrs = Attrs::new().family(family).weight(Weight::NORMAL);
        let height = (size * 1.4).ceil() as u32;
        PrototypeGlyphs {
            themed: Default::default(),
            sprite: make_sprite(
                &mut self.font_system,
                &mut self.swash_cache,
                spans
                    .iter()
                    .map(|s| (s.text.as_str(), attributes(attrs.clone(), s.style)))
                    .collect(),
                attrs,
                Metrics::new(size, height as f32),
                self.spec.width,
                height,
            ),
        }
    }

    pub(crate) fn composite_prototype_glyphs(
        &self,
        pixels: &mut [u8],
        glyphs: &PrototypeGlyphs,
        origin: [f32; 2],
        presence: f32,
        opacity: f32,
    ) {
        self.composite_prototype_glyphs_with_blur(
            pixels,
            glyphs,
            origin,
            presence,
            opacity,
            (1. - opacity.clamp(0., 1.)) * 4.,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn composite_prototype_glyphs_with_blur(
        &self,
        pixels: &mut [u8],
        glyphs: &PrototypeGlyphs,
        origin: [f32; 2],
        presence: f32,
        opacity: f32,
        blur: f32,
    ) {
        let themed;
        let sprite = if self.theme == Theme::Original {
            &glyphs.sprite
        } else {
            themed = glyphs.themed.get(self.theme, || {
                let mut sprite = glyphs.sprite.clone();
                self.theme.sprite(&mut sprite);
                sprite
            });
            &*themed
        };
        let visible = if presence >= 1. {
            glyphs.sprite.width as f32
        } else {
            glyphs.width() * presence.clamp(0., 1.)
        };
        // Optical treatment of the existing fade, not another motion window.
        // Fully present words stay sharp even while their positions are moving.
        composite_text(
            pixels,
            [self.spec.width, self.spec.height],
            TextDraw {
                clip_width: visible,
                filter: TextFilter::Blur(blur),
                opacity,
                ..TextDraw::new(sprite, origin)
            },
        );
    }

    pub(crate) fn composite_prototype_path(
        &self,
        pixels: &mut [u8],
        points: &[[f32; 2]],
        width: f32,
        color: [u8; 3],
        opacity: f32,
    ) {
        let [r, g, b] = self.theme.ink(color);
        ui::card::UiCanvas::new(pixels, [self.spec.width, self.spec.height]).polyline(
            points,
            width,
            ui::card::UiColor::srgb8(r, g, b, 255),
            opacity,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a headless GPU; syntax paint within a part must not split or reshape its animation"]
    fn styled_runs_keep_one_measured_part_with_distinct_token_colors() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "styled-part-proof".into(),
        }))
        .unwrap();
        let part = TextPart {
            id: "green".into(),
            text: " | \"green\"".into(),
            color: [235, 233, 227],
            spans: vec![
                StyledSpan {
                    text: " | ".into(),
                    style: SyntaxStyle::Plain,
                },
                StyledSpan {
                    text: "\"green\"".into(),
                    style: SyntaxStyle::String,
                },
            ],
        };
        let solid = renderer.prepare_prototype_glyphs(&part.text, Font::Mono, 54., part.color);
        let styled = renderer.prepare_prototype_part(&part, Font::Mono, 54.);
        assert_eq!(solid.width(), styled.width());
        assert!(
            solid
                .sprite
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(styled.sprite.pixels.as_chunks::<4>().0)
                .all(|(a, b)| a[3] == b[3]),
            "syntax colors must not change glyph geometry"
        );
        for color in [[228, 228, 231], [190, 242, 100]] {
            assert!(
                styled
                    .sprite
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[..3] == color && p[3] > 200),
                "missing syntax color {color:?}"
            );
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; fading words soften without blurring retained or settled ink"]
    fn word_fades_blur_but_retained_words_stay_pixel_identical() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "word-fade-blur-proof".into(),
        }))
        .unwrap();
        let glyphs = renderer.prepare_prototype_glyphs("more ", Font::Sans, 112., [216, 168, 120]);
        let background = renderer.render_title_card("", None, 0.);
        let sharp = |origin, presence: f32, opacity| {
            let mut pixels = background.clone();
            composite_text(
                &mut pixels,
                [1920, 1080],
                TextDraw {
                    clip_width: if presence >= 1. {
                        glyphs.sprite.width as f32
                    } else {
                        glyphs.width() * presence
                    },
                    opacity,
                    ..TextDraw::new(&glyphs.sprite, origin)
                },
            );
            pixels
        };
        // Out-of-order opacity visits exercise both entrance and exit with the
        // same optical pose. The last case covers an entire component fading.
        for (presence, opacity) in [
            (0., 0.),
            (0.25, 0.25),
            (0.75, 0.75),
            (1., 1.),
            (0.5, 0.5),
            (0., 0.),
            (1., 0.5),
        ] {
            let origin = [350.25, 375.5];
            let mut pixels = background.clone();
            renderer.composite_prototype_glyphs(&mut pixels, &glyphs, origin, presence, opacity);
            let old = sharp(origin, presence, opacity);
            if opacity == 0. || opacity == 1. {
                assert!(
                    pixels == old,
                    "transparent and sharp endpoints must not change"
                );
            } else {
                assert!(
                    pixels.iter().zip(old).filter(|(a, b)| **a != *b).count() > 100,
                    "partial-opacity ink must actually blur"
                );
            }
        }
        for x in [350., 350.1, 350.7, 675.35] {
            let origin = [x, 375.5];
            let mut pixels = background.clone();
            renderer.composite_prototype_glyphs(&mut pixels, &glyphs, origin, 1., 1.);
            assert!(
                pixels == sharp(origin, 1., 1.),
                "moving retained words must remain sharp"
            );
        }
    }
}
