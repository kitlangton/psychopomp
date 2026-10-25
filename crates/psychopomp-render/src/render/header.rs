//! One shaped line, optionally revealed word-by-word through a fixed edge.
//! Reflection mirrors that same sampled ink, never a separately timed copy.
use super::*;
use psychopomp::component_prototype::{HeaderPlan, HeaderSplit};
use std::ops::Range;

pub(crate) fn header_words(text: &str, split: HeaderSplit) -> Vec<Range<usize>> {
    if split == HeaderSplit::Line {
        return std::iter::once(0..text.len()).collect();
    }
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(start) = start.take() {
                ranges.push(start..index);
            }
        } else {
            start.get_or_insert(index);
        }
    }
    if let Some(start) = start {
        ranges.push(start..text.len());
    }
    ranges
}

pub(crate) struct HeaderGlyphs {
    sprite: TextSprite,
    ranges: Vec<[f32; 2]>,
    ink_bottom: f32,
    colored: theme::ThemedCache<(TextSprite, TextSprite)>,
}

impl HeadlessRenderer {
    pub(crate) fn prepare_header(&mut self, plan: &HeaderPlan) -> Result<HeaderGlyphs> {
        let height = (plan.font_size * 1.4).ceil() as u32;
        let width = plan.width.ceil() as u32;
        let attrs = Attrs::new()
            .family(fonts::SANS)
            .weight(Weight::BOLD)
            .color(Color::rgb(235, 233, 227));
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(plan.font_size, height as f32),
        );
        buffer.set_size(Some(plan.width), Some(height as f32));
        buffer.set_wrap(Wrap::None);
        buffer.set_rich_text(
            vec![(plan.text.as_str(), attrs.clone())],
            &attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        let run = buffer
            .layout_runs()
            .next()
            .context("header must shape to one line")?;
        anyhow::ensure!(
            run.line_w <= plan.width,
            "header is wider than its bound; reduce font size or shorten it"
        );
        let advance = run.line_w;
        let bounds = header_words(&plan.text, plan.split)
            .into_iter()
            .map(|word| {
                let glyphs = run
                    .glyphs
                    .iter()
                    .filter(|g| g.start < word.end && word.start < g.end)
                    .collect::<Vec<_>>();
                anyhow::ensure!(!glyphs.is_empty(), "header word has no shaped glyphs");
                Ok([
                    glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min),
                    glyphs
                        .iter()
                        .map(|g| g.x + g.w)
                        .fold(f32::NEG_INFINITY, f32::max),
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        anyhow::ensure!(
            bounds
                .windows(2)
                .all(|pair| pair[0][1] <= pair[1][0] + 0.01),
            "word staggering currently requires left-to-right word placement; use line mode for this text"
        );
        // Split in the whitespace between words, not at glyph ink edges. The
        // full line is shaped once, so stagger never changes kerning or placement.
        let ranges = bounds
            .iter()
            .enumerate()
            .map(|(i, b)| {
                [
                    if i == 0 {
                        0.
                    } else {
                        (bounds[i - 1][1] + b[0]) / 2.
                    },
                    if i + 1 == bounds.len() {
                        width as f32
                    } else {
                        (b[1] + bounds[i + 1][0]) / 2.
                    },
                ]
            })
            .collect();
        let mut pixels = vec![0; (width * height * 4) as usize];
        let lift = ink_lift(buffer.metrics().font_size);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            Color::rgb(235, 233, 227),
            |x, y, w, h, c| paint_rect(&mut pixels, width, height, x, y, w, h, glyph_ink(c, lift)),
        );
        let ink_bottom = pixels
            .chunks_exact(width as usize * 4)
            .rposition(|row| row.as_chunks::<4>().0.iter().any(|p| p[3] > 0))
            .map_or(height, |y| y as u32 + 1) as f32;
        Ok(HeaderGlyphs {
            sprite: TextSprite {
                width,
                height,
                advance,
                pixels,
            },
            ranges,
            ink_bottom,
            colored: Default::default(),
        })
    }

    pub(crate) fn composite_header(
        &self,
        pixels: &mut [u8],
        plan: &HeaderPlan,
        glyphs: &HeaderGlyphs,
        sample: impl Fn(&str, f32) -> f32,
    ) {
        let colored = glyphs.colored.get(self.theme, || {
            let mut normal = glyphs.sprite.clone();
            self.theme.sprite(&mut normal);
            let mut reflected = normal.clone();
            let row = normal.width as usize * 4;
            for y in 0..normal.height as usize {
                reflected.pixels[y * row..(y + 1) * row].copy_from_slice(
                    &normal.pixels[(normal.height as usize - 1 - y) * row
                        ..(normal.height as usize - y) * row],
                );
            }
            (normal, reflected)
        });
        let (normal, reflected) = &*colored;
        let origin = [sample("x", plan.origin[0]), sample("y", plan.origin[1])];
        let edge = origin[1] + glyphs.ink_bottom + 6.;
        let opacity = sample("opacity", 1.).clamp(0., 1.);
        for (index, [start, end]) in glyphs.ranges.iter().copied().enumerate() {
            let progress =
                sample(&format!("__header.{index}.reveal"), f32::from(plan.visible)).clamp(0., 1.);
            let y = origin[1] + (1. - progress) * (normal.height as f32 + 10.);
            let alpha = progress * opacity;
            let blur = (1. - progress) * 4.;
            let window = VerticalMask {
                top: origin[1] - 16.,
                bottom: edge,
                fade: 8.,
            };
            composite_text(
                pixels,
                [self.spec.width, self.spec.height],
                TextDraw {
                    source_left: start,
                    clip_width: end - start,
                    filter: TextFilter::Blur(blur),
                    opacity: alpha,
                    mask: Some(window),
                    ..TextDraw::new(normal, [origin[0] + start, y])
                },
            );
            if let Some(reflection) = plan.reflection {
                let top = edge + reflection.gap;
                // The top half of this triangular mask is outside clip_y. What
                // remains is a stationary, one-sided linear fade away from the edge.
                let mask = VerticalMask {
                    top: top - reflection.depth,
                    bottom: top + reflection.depth,
                    fade: reflection.depth,
                };
                composite_text(
                    pixels,
                    [self.spec.width, self.spec.height],
                    TextDraw {
                        source_left: start,
                        clip_width: end - start,
                        filter: TextFilter::Blur(blur + 1.),
                        opacity: alpha * reflection.opacity,
                        clip_y: Some([top, top + reflection.depth]),
                        mask: Some(mask),
                        ..TextDraw::new(
                            reflected,
                            [
                                origin[0] + start,
                                2. * edge - y - normal.height as f32 + reflection.gap,
                            ],
                        )
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_ranges_preserve_utf8_and_punctuation() {
        let text = "  Every naïve word, rises. ";
        assert_eq!(
            header_words(text, HeaderSplit::Words)
                .into_iter()
                .map(|r| &text[r])
                .collect::<Vec<_>>(),
            vec!["Every", "naïve", "word,", "rises."]
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; reflection is confined below the edge and follows the same glyph pose"]
    fn reflection_adds_only_fading_mirrored_ink_below_the_stationary_edge() {
        use psychopomp::component_prototype::HeaderReflection;
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "reflection-proof".into(),
        }))
        .unwrap();
        let plan = HeaderPlan {
            origin: [240.25, 330.5],
            text: "Through the edge.".into(),
            font_size: 62.,
            width: 1200.,
            split: HeaderSplit::Words,
            stagger_millis: 50,
            duration_seconds: 0.4,
            visible: false,
            events: vec![],
            reflection: Some(HeaderReflection {
                opacity: 0.3,
                depth: 60.,
                gap: 4.,
            }),
        };
        let glyphs = renderer.prepare_header(&plan).unwrap();
        let mut plain = plan.clone();
        plain.reflection = None;
        let top = plan.origin[1] + glyphs.ink_bottom + 6. + 4.;
        let draw = |p: &HeaderPlan, progress: f32| {
            let mut pixels = [1, 2, 4, 255].repeat(1920 * 1080);
            renderer.composite_header(&mut pixels, p, &glyphs, |name, d| {
                if name.starts_with("__header.") {
                    progress
                } else {
                    d
                }
            });
            pixels
        };
        for progress in [0., 0.25, 0.7, 1., 0.4, 0.] {
            let reflected = draw(&plan, progress);
            let normal = draw(&plain, progress);
            assert_eq!(
                &reflected[..top.floor() as usize * 1920 * 4],
                &normal[..top.floor() as usize * 1920 * 4]
            );
            assert_eq!(
                &reflected[(top + 60.).ceil() as usize * 1920 * 4..],
                &normal[(top + 60.).ceil() as usize * 1920 * 4..]
            );
            if progress == 1. {
                assert!(reflected != normal, "held reflection must be visible");
            }
            if progress == 0. {
                assert!(
                    reflected == normal,
                    "hidden words cannot leave a reflection"
                );
            }
            draw(&plan, 0.9);
            assert!(reflected == draw(&plan, progress));
        }
    }
}
