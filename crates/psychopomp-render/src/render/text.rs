//! Immutable plain CommitMono text, separate from authored inline identities,
//! SVG/bubble resources and bounded debug slots. Callers choose paint policy.
use super::{HeadlessRenderer, fonts};
use cosmic_text::{Attrs, Color, FontSystem, Metrics, SwashCache, Weight};
use psychopomp::face::Face;
use std::collections::HashMap;
mod raster;
pub(super) use raster::{TextSprite, blend_pixel, blend_pixel_at, make_sprite, paint_rect};

#[derive(Clone, Copy)]
pub(super) struct PlainTextSpec {
    pub font_size: f32,
    pub color: [u8; 3],
    pub size: [u32; 2],
    pub semibold: bool,
    pub crop_to_advance: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RasterKey {
    font_size_bits: u32,
    color: [u8; 3],
    size: [u32; 2],
    semibold: bool,
    crop_to_advance: bool,
}
impl From<PlainTextSpec> for RasterKey {
    fn from(s: PlainTextSpec) -> Self {
        Self {
            font_size_bits: s.font_size.to_bits(),
            color: s.color,
            size: s.size,
            semibold: s.semibold,
            crop_to_advance: s.crop_to_advance,
        }
    }
}

#[derive(Default)]
pub(super) struct PlainTextCache {
    sprites: HashMap<(Face, RasterKey), HashMap<String, TextSprite>>,
}
impl PlainTextCache {
    pub(super) fn clear(&mut self) {
        self.sprites.clear();
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.sprites.values().map(HashMap::len).sum()
    }
    fn get(
        &mut self,
        fonts: &mut FontSystem,
        swash: &mut SwashCache,
        face: Face,
        text: &str,
        spec: PlainTextSpec,
    ) -> &TextSprite {
        let sprites = self.sprites.entry((face, spec.into())).or_default();
        if !sprites.contains_key(text) {
            let attrs = match face {
                Face::Mono => Attrs::new().family(fonts::mono()).weight(if spec.semibold {
                    Weight::SEMIBOLD
                } else {
                    Weight::NORMAL
                }),
                face => fonts::attrs(face),
            }
            .color(Color::rgb(spec.color[0], spec.color[1], spec.color[2]));
            let mut sprite = make_sprite(
                fonts,
                swash,
                vec![(text, attrs.clone())],
                attrs,
                Metrics::new(spec.font_size, spec.size[1] as f32),
                spec.size[0],
                spec.size[1],
            );
            if spec.crop_to_advance {
                crop_to_advance(&mut sprite);
            }
            sprites.insert(text.into(), sprite);
        }
        &sprites[text]
    }
}

impl HeadlessRenderer {
    pub(super) fn plain_text_sprite(&mut self, text: &str, spec: PlainTextSpec) -> &TextSprite {
        self.plain_text_sprite_in(Face::Mono, text, spec)
    }

    /// `text` set in `face` rather than CommitMono.
    pub(super) fn plain_text_sprite_in(
        &mut self,
        face: Face,
        text: &str,
        spec: PlainTextSpec,
    ) -> &TextSprite {
        self.plain_text_sprites.get(
            &mut self.font_system,
            &mut self.swash_cache,
            face,
            text,
            spec,
        )
    }
}

fn crop_to_advance(sprite: &mut TextSprite) {
    let width = sprite.advance.ceil().max(1.0) as u32;
    if width < sprite.width {
        let mut cropped = vec![0_u8; width as usize * sprite.height as usize * 4];
        for y in 0..sprite.height as usize {
            let source = y * sprite.width as usize * 4;
            let target = y * width as usize * 4;
            let bytes = width as usize * 4;
            cropped[target..target + bytes].copy_from_slice(&sprite.pixels[source..source + bytes]);
        }
        sprite.width = width;
        sprite.pixels = cropped;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plain_text_policy_matches_direct_raster_bytes_and_crop_rules() {
        let mut fonts = crate::render::fonts::font_system();
        let mut swash = SwashCache::new();
        let mut cache = PlainTextCache::default();
        let long = "An immutable value ".repeat(25);
        for text in ["", " ", "fi", "e\u{301}", "♞", long.as_str()] {
            for (width, multiplier, crop, semibold) in [
                (1760, 1.5, false, false),
                (720, 1.5, true, false),
                (900, 1.5, false, false),
                (760, 1.55, true, true),
            ] {
                let size = 24.25_f32;
                let height = (size * multiplier).ceil() as u32;
                let color = [228, 231, 235];
                let spec = PlainTextSpec {
                    font_size: size,
                    color,
                    size: [width, height],
                    semibold,
                    crop_to_advance: crop,
                };
                let actual = cache
                    .get(&mut fonts, &mut swash, Face::Mono, text, spec)
                    .clone();
                let attrs = Attrs::new()
                    .family(fonts::mono())
                    .weight(if semibold {
                        Weight::SEMIBOLD
                    } else {
                        Weight::NORMAL
                    })
                    .color(Color::rgb(color[0], color[1], color[2]));
                let raw = make_sprite(
                    &mut fonts,
                    &mut swash,
                    vec![(text, attrs.clone())],
                    attrs,
                    Metrics::new(size, height as f32),
                    width,
                    height,
                );
                let expected_width = if crop {
                    raw.advance.ceil().clamp(1., raw.width as f32) as u32
                } else {
                    raw.width
                };
                let expected = raw
                    .pixels
                    .chunks_exact(raw.width as usize * 4)
                    .flat_map(|row| row[..expected_width as usize * 4].iter().copied())
                    .collect::<Vec<_>>();
                assert_eq!(
                    (actual.width, actual.height, actual.advance.to_bits()),
                    (expected_width, height, raw.advance.to_bits())
                );
                assert_eq!(actual.pixels, expected);
            }
        }
        for width in [720, 760] {
            for advance in [
                -1_f32,
                0.,
                0.25,
                1.25,
                719.25,
                720.,
                759.25,
                760.,
                900.,
                f32::MAX,
            ] {
                let task = advance.ceil().max(1.) as u32;
                let deployment = advance.ceil().clamp(1., width as f32) as u32;
                assert_eq!(task.min(width), deployment);
            }
        }
    }

    #[test]
    fn crop_keeps_fractional_advance_and_original_rows() {
        let mut sprite = TextSprite {
            width: 7,
            height: 2,
            advance: 2.25,
            pixels: (0..56).collect(),
        };
        let old = sprite.pixels.clone();
        crop_to_advance(&mut sprite);
        assert_eq!((sprite.width, sprite.advance), (3, 2.25));
        assert_eq!(sprite.pixels, [&old[..12], &old[28..40]].concat());
    }
    #[test]
    fn keys_include_all_raster_inputs_and_cache_hits_reuse_pixels() {
        let mut fonts = crate::render::fonts::font_system();
        let mut swash = SwashCache::new();
        let mut cache = PlainTextCache::default();
        let base = PlainTextSpec {
            font_size: 24.25,
            color: [228, 231, 235],
            size: [720, 37],
            semibold: false,
            crop_to_advance: false,
        };
        for other in [
            PlainTextSpec {
                font_size: 24.250002,
                ..base
            },
            PlainTextSpec {
                color: [228, 231, 236],
                ..base
            },
            PlainTextSpec {
                size: [900, 37],
                ..base
            },
            PlainTextSpec {
                size: [720, 38],
                ..base
            },
            PlainTextSpec {
                semibold: true,
                ..base
            },
            PlainTextSpec {
                crop_to_advance: true,
                ..base
            },
        ] {
            assert_ne!(RasterKey::from(base), RasterKey::from(other));
        }
        for text in ["", " ", "fi", "e\u{301}", "♞", "debug:row.0"] {
            let first = cache
                .get(&mut fonts, &mut swash, Face::Mono, text, base)
                .clone();
            let pointer = cache
                .get(&mut fonts, &mut swash, Face::Mono, text, base)
                .pixels
                .as_ptr();
            let hit = cache.get(&mut fonts, &mut swash, Face::Mono, text, base);
            assert_eq!(hit.pixels, first.pixels);
            assert_eq!(hit.advance.to_bits(), first.advance.to_bits());
            assert_eq!(hit.pixels.as_ptr(), pointer);
        }
        assert_eq!(cache.len(), 6);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}
