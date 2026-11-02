//! Caption pixels: styled CommitMono lines, an optional chip surface (solid, or
//! a liquid-glass pane that refracts what is behind it), a typing
//! reveal by character, and the accent block caret. Alignment uses each line's
//! full width, so typed text never slides while it appears.
use psychopomp::{
    caption::{CaptionAlign, CaptionPlan},
    lens::Glass,
    math::{
        shapes::{Box2, RoundedBox},
        vec2,
    },
};

use super::{
    HeadlessRenderer, PlainTextSpec, TextDraw, composite_lens, composite_text,
    ui::{
        Bounds,
        card::{Fill, SurfaceStyle, UiCanvas, UiColor},
    },
};

impl HeadlessRenderer {
    pub(crate) fn composite_caption(
        &mut self,
        pixels: &mut [u8],
        plan: &CaptionPlan,
        sample: impl Fn(&str, f32) -> f32,
    ) {
        self.composite_caption_at(pixels, plan, plan.origin, sample);
    }

    /// Screen-space bounding box of a visible caption at `origin`.
    pub(crate) fn caption_bounds(
        &mut self,
        plan: &CaptionPlan,
        origin: [f32; 2],
        sample: impl Fn(&str, f32) -> f32,
    ) -> Option<Box2> {
        let opacity = sample("opacity", 1.0).clamp(0.0, 1.0);
        if opacity <= 0.02 || plan.lines.is_empty() {
            return None;
        }
        let origin = [origin[0] + sample("x", 0.0), origin[1] + sample("y", 0.0)];
        let spec = PlainTextSpec {
            font_size: plan.size,
            color: [0, 0, 0],
            size: [1600, (plan.size * 1.5).ceil() as u32],
            semibold: false,
            crop_to_advance: true,
        };
        let line_height = plan.line_height();
        let widths = plan
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| {
                        if span.text.is_empty() {
                            0.0
                        } else {
                            let color =
                                span.color.unwrap_or_else(|| self.effective_tone(span.tone));
                            self.plain_text_sprite(&span.text, PlainTextSpec { color, ..spec })
                                .advance
                        }
                    })
                    .sum::<f32>()
            })
            .collect::<Vec<_>>();
        let lefts = widths
            .iter()
            .map(|&width| match plan.align {
                CaptionAlign::Left => origin[0],
                CaptionAlign::Center => origin[0] - width * 0.5,
                CaptionAlign::Right => origin[0] - width,
            })
            .collect::<Vec<_>>();
        let left = lefts.iter().copied().fold(f32::INFINITY, f32::min);
        let right = widths
            .iter()
            .zip(&lefts)
            .map(|(w, l)| l + w)
            .fold(f32::NEG_INFINITY, f32::max);
        let top = origin[1] - line_height * 0.5 - if plan.chip { 4.0 } else { 0.0 };
        let bottom = origin[1]
            + line_height * (plan.lines.len() as f32 - 0.5)
            + if plan.chip { 4.0 } else { 0.0 };
        let pad_x = if plan.chip { plan.size * 0.7 } else { 0.0 };
        Some(Box2 {
            min: vec2(left - pad_x, top),
            max: vec2(right + pad_x, bottom),
        })
    }

    /// `composite_caption` with its origin at `origin` (as when anchored)
    /// rather than the plan's.
    pub(crate) fn composite_caption_at(
        &mut self,
        pixels: &mut [u8],
        plan: &CaptionPlan,
        origin: [f32; 2],
        sample: impl Fn(&str, f32) -> f32,
    ) {
        let opacity = sample("opacity", 1.0).clamp(0.0, 1.0);
        if opacity <= 0.001 {
            return;
        }
        let origin = [origin[0] + sample("x", 0.0), origin[1] + sample("y", 0.0)];
        let typed = sample("typed", 1.0).clamp(0.0, 1.0);
        let caret = sample("caret", 0.0).clamp(0.0, 1.0);
        let canvas = [self.spec.width, self.spec.height];
        let spec = PlainTextSpec {
            font_size: plan.size,
            color: [0, 0, 0],
            size: [1600, (plan.size * 1.5).ceil() as u32],
            semibold: false,
            crop_to_advance: true,
        };
        let line_height = plan.line_height();
        // Measure every span once; widths drive alignment and the chip.
        let widths = plan
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| {
                        let color = span.color.unwrap_or_else(|| self.effective_tone(span.tone));
                        if span.text.is_empty() {
                            0.0
                        } else {
                            self.plain_text_sprite(&span.text, PlainTextSpec { color, ..spec })
                                .advance
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let lefts = widths
            .iter()
            .map(|line| {
                let width: f32 = line.iter().sum();
                match plan.align {
                    CaptionAlign::Left => origin[0],
                    CaptionAlign::Center => origin[0] - width * 0.5,
                    CaptionAlign::Right => origin[0] - width,
                }
            })
            .collect::<Vec<_>>();
        if plan.chip {
            let palette = self.effective_palette();
            let left = lefts.iter().copied().fold(f32::INFINITY, f32::min);
            let right = widths
                .iter()
                .zip(&lefts)
                .map(|(line, left)| left + line.iter().sum::<f32>())
                .fold(f32::NEG_INFINITY, f32::max);
            let top = origin[1] - line_height * 0.5 - 4.0;
            let bottom = origin[1] + line_height * (plan.lines.len() as f32 - 0.5) + 4.0;
            let bounds = Bounds {
                origin: [left - plan.size * 0.7, top],
                size: [right - left + plan.size * 1.4, bottom - top],
            };
            if plan.glass {
                let [x, y] = bounds.origin;
                let [width, height] = bounds.size;
                let outline = RoundedBox::new(
                    vec2(x + width * 0.5, y + height * 0.5),
                    vec2(width, height) * 0.5,
                    height * 0.5,
                );
                if let Some(pane) = Glass::pane(outline, opacity) {
                    composite_lens(pixels, canvas, &pane);
                }
            } else {
                let [r, g, b] = palette.surface;
                let [br, bg, bb] = palette.raised;
                UiCanvas::new(pixels, canvas).surface(
                    bounds,
                    SurfaceStyle::new(
                        Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                        (bottom - top) * 0.5,
                    )
                    .border(1.2, UiColor::srgb8(br, bg, bb, 255), 1.0),
                    opacity,
                );
            }
        }
        let total = plan.char_count();
        let mut remaining = if typed >= 1.0 {
            usize::MAX
        } else {
            ((typed * total as f32) + 1e-3).floor() as usize
        };
        let mut caret_at = None;
        'lines: for (index, line) in plan.lines.iter().enumerate() {
            let y = origin[1] + line_height * index as f32;
            let mut x = lefts[index];
            caret_at = Some([x, y]);
            for (span, width) in line.iter().zip(&widths[index]) {
                let chars = span.text.chars().count();
                if chars == 0 {
                    continue;
                }
                let shown = remaining.min(chars);
                if shown == 0 {
                    break 'lines;
                }
                let clip = width * shown as f32 / chars as f32;
                let color = span.color.unwrap_or_else(|| self.effective_tone(span.tone));
                let sprite = self.plain_text_sprite(&span.text, PlainTextSpec { color, ..spec });
                composite_text(
                    pixels,
                    canvas,
                    TextDraw {
                        clip_width: if shown == chars {
                            clip.ceil() + 1.0
                        } else {
                            clip
                        },
                        opacity,
                        ..TextDraw::new(sprite, [x, y - spec.size[1] as f32 * 0.5])
                    },
                );
                x += clip;
                caret_at = Some([x, y]);
                remaining -= shown;
                if shown < chars {
                    break 'lines;
                }
            }
        }
        if caret > 0.001
            && let Some([x, y]) = caret_at
        {
            let [r, g, b] = self.theme.palette().accent;
            UiCanvas::new(pixels, canvas).fill(
                Bounds::from_center(
                    [x + plan.size * 0.36, y],
                    [plan.size * 0.56, plan.size * 1.08],
                ),
                1.5,
                Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                opacity * caret,
            );
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use psychopomp::{
        caption::{CaptionAlign, CaptionPlan, CaptionSpanPlan},
        tone::Tone,
    };

    use crate::render::HeadlessRenderer;

    #[test]
    #[ignore = "requires a headless GPU; typing reveals by character and aligned text never slides"]
    fn captions_type_by_character_without_residue() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "caption-proof".into(),
        }))
        .unwrap();
        let plan = CaptionPlan::line(
            [960.0, 540.0],
            32.0,
            vec![
                CaptionSpanPlan::new("one server", Tone::Accent),
                CaptionSpanPlan::new(". every client.", Tone::Plain),
            ],
        )
        .aligned(CaptionAlign::Center)
        .chip();
        let background = renderer.render_title_card("", None, 0.);
        let draw = |renderer: &mut HeadlessRenderer, opacity: f32, typed: f32, caret: f32| {
            let mut pixels = background.clone();
            renderer.composite_caption(&mut pixels, &plan, |property, default| match property {
                "opacity" => opacity,
                "typed" => typed,
                "caret" => caret,
                _ => default,
            });
            pixels
        };
        assert!(
            draw(&mut renderer, 0.0, 1.0, 1.0) == background,
            "hidden captions leave no ink"
        );
        let partial = draw(&mut renderer, 1.0, 0.4, 0.0);
        let full = draw(&mut renderer, 1.0, 1.0, 0.0);
        assert!(partial != full, "typing reveals characters");
        assert!(
            draw(&mut renderer, 1.0, 0.4, 1.0) != partial,
            "the caret reaches pixels"
        );
        // Centered alignment uses the full width: the first glyphs stay put while typing.
        let row = 540 * 1920 * 4;
        let left_edge = |pixels: &[u8]| {
            (0..1920).find(|x| {
                let i = row + x * 4;
                pixels[i..i + 3] != background[i..i + 3]
            })
        };
        assert_eq!(left_edge(&partial), left_edge(&full));
    }
}
