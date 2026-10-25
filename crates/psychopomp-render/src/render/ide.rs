//! IDE annotation pixels on the flat editor surface, before the card
//! projection: selection highlights under the code, Inlay Hint chips behind
//! their ghost text, then diagnostic waves and gutter icons, carets, and
//! Hover Cards over it. Geometry comes from `psychopomp::ide` (the wave, the
//! hover layout, the blink); this module measures text and paints.
use psychopomp::{
    code::StyledSpan,
    ide::{
        AMPLITUDE, HOVER_CODE_LINE, HOVER_CODE_SIZE, HOVER_RISE, HOVER_TEXT_LINE, HOVER_TEXT_SIZE,
        HoverPlan, HoverSectionPlan, Severity, WAVELENGTH, hover_layout, wave,
    },
    math::{Vec2, lerp, shapes::Box2, smoothstep, vec2},
};

use super::{
    EditorFrame, HeadlessRenderer, LINE_HEIGHT, PlainTextSpec, TextDraw, composite_text,
    make_spans_sprite_at_size, refresh_sprite, spans_fingerprint,
    text::blend_pixel,
    theme::mix,
    ui::{
        Bounds,
        card::{Fill, UiCanvas, UiColor},
    },
};

/// The editor's annotations at one sample, in code coordinates (x from the
/// code column, y from the first row's top).
#[derive(Clone, Copy, Default)]
pub struct EditorAnnotations<'a> {
    pub inlays: &'a [InlayFrame<'a>],
    pub diagnostics: &'a [DiagnosticFrame],
    pub selections: &'a [SelectionFrame],
    pub carets: &'a [CaretFrame],
    pub hovers: &'a [HoverFrame<'a>],
}

/// The span range of an Inlay Hint's Inline Reveal: drawn as ghost text.
#[derive(Clone, Copy)]
pub struct InlayFrame<'a> {
    pub line_id: &'a str,
    pub start_span: usize,
    pub end_span: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct DiagnosticFrame {
    pub x: f32,
    pub width: f32,
    pub line_y: f32,
    pub severity: Severity,
    pub gutter: bool,
    pub draw: f32,
    pub wave: f32,
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SelectionFrame {
    pub x: f32,
    pub width: f32,
    pub line_y: f32,
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct CaretFrame {
    pub x: f32,
    pub line_y: f32,
    pub opacity: f32,
}

#[derive(Clone, Copy)]
pub struct HoverFrame<'a> {
    pub plan: &'a HoverPlan,
    pub x: f32,
    pub width: f32,
    pub line_y: f32,
    pub presence: f32,
    /// The target line's opacity.
    pub opacity: f32,
}

/// Where the wave's center line sits below a row's top: under descenders.
const WAVE_Y: f32 = 39.5;
const WAVE_WIDTH: f32 = 1.6;
const GUTTER_RADIUS: f32 = 8.5;
const CARET: [f32; 2] = [2.6, 34.0];
const SELECTION_INSET: f32 = 4.0;
const CHIP_INSET: [f32; 2] = [1.0, 7.0];
const HOVER_PADDING: Vec2 = Vec2::new(16.0, 11.0);
const HOVER_RULE_GAP: f32 = 9.0;
const HOVER_RADIUS: f32 = 8.0;

/// The flat surface's code origin and visible code rows.
#[derive(Clone, Copy)]
pub(super) struct CodeArea {
    pub origin: Vec2,
    pub clip_y: [f32; 2],
}

impl CodeArea {
    fn row(self, x: f32, line_y: f32) -> Vec2 {
        self.origin + vec2(x, line_y)
    }

    fn visible(self, top: f32, bottom: f32) -> bool {
        bottom > self.clip_y[0] && top < self.clip_y[1]
    }
}

fn color([r, g, b]: [u8; 3]) -> UiColor {
    UiColor::srgb8(r, g, b, 255)
}

impl HeadlessRenderer {
    fn code_clip(&self, area: CodeArea) -> super::ui::card::Clip {
        super::ui::card::Clip::rounded(
            Bounds {
                origin: [0.0, area.clip_y[0]],
                size: [self.spec.width as f32, area.clip_y[1] - area.clip_y[0]],
            },
            0.0,
        )
    }

    /// Selection highlights, under the code.
    pub(super) fn composite_selections(
        &self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
        area: CodeArea,
    ) {
        let selections = frame.annotations.selections;
        if selections.is_empty() {
            return;
        }
        let tint = color(self.theme.tone(psychopomp::tone::Tone::Request));
        let clip = self.code_clip(area);
        let mut canvas = UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
        let _ = canvas.clipped(clip, |canvas| {
            for selection in selections {
                let top = area.row(selection.x, selection.line_y) + vec2(-1.0, SELECTION_INSET);
                canvas.fill(
                    Bounds {
                        origin: top.to_array(),
                        size: [selection.width + 2.0, LINE_HEIGHT - SELECTION_INSET * 2.0],
                    },
                    5.0,
                    Fill::Solid(tint),
                    selection.opacity * 0.26,
                );
            }
            Ok(())
        });
    }

    /// The faint chip behind an Inlay Hint's ghost text, as wide as its room.
    pub(super) fn composite_inlay_chip(
        &self,
        pixels: &mut [u8],
        origin: Vec2,
        width: f32,
        opacity: f32,
        clip_y: [f32; 2],
    ) {
        let width = width - CHIP_INSET[0] * 2.0;
        if width <= 0.5 || opacity <= 0.001 {
            return;
        }
        let palette = self.theme.palette();
        let fill = mix(palette.raised, palette.muted, 0.18);
        let mut canvas = UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
        let clip = super::ui::card::Clip::rounded(
            Bounds {
                origin: [0.0, clip_y[0]],
                size: [self.spec.width as f32, clip_y[1] - clip_y[0]],
            },
            0.0,
        );
        let _ = canvas.clipped(clip, |canvas| {
            canvas.fill(
                Bounds {
                    origin: (origin + vec2(CHIP_INSET[0], CHIP_INSET[1])).to_array(),
                    size: [width, LINE_HEIGHT - CHIP_INSET[1] * 2.0],
                },
                6.0,
                Fill::Solid(color(fill)),
                opacity * 0.75,
            );
            Ok(())
        });
    }

    /// Diagnostic waves and their gutter icons, over the code.
    pub(super) fn composite_diagnostics(
        &self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
        area: CodeArea,
    ) {
        let sign_x = self.spec.width as f32 * 0.145 - 30.0;
        let surface = self.theme.surface([13, 18, 27]);
        for diagnostic in frame.annotations.diagnostics {
            let top = area.row(diagnostic.x, diagnostic.line_y);
            if !area.visible(top.y, top.y + LINE_HEIGHT) {
                continue;
            }
            let tone = self.theme.tone(diagnostic.severity.tone());
            let path = wave(
                top + vec2(0.0, WAVE_Y),
                diagnostic.width,
                AMPLITUDE * diagnostic.wave.max(0.0),
                WAVELENGTH,
            )
            .slice(0.0, diagnostic.draw);
            let points = path
                .points()
                .iter()
                .map(|point| point.to_array())
                .collect::<Vec<_>>();
            let clip = self.code_clip(area);
            let mut canvas = UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
            let _ = canvas.clipped(clip, |canvas| {
                canvas.polyline(&points, WAVE_WIDTH, color(tone), diagnostic.opacity);
                Ok(())
            });
            if !diagnostic.gutter {
                continue;
            }
            // The icon pops in as the wave starts and leaves with its opacity.
            let pop = smoothstep(diagnostic.draw / 0.3);
            let alpha = diagnostic.opacity * pop;
            if alpha <= 0.001 {
                continue;
            }
            let center = vec2(sign_x, top.y + LINE_HEIGHT * 0.5);
            let radius = GUTTER_RADIUS * lerp(0.9, 1.0, pop);
            let _ = canvas.clipped(clip, |canvas| {
                canvas.fill(
                    Bounds::from_center(center.to_array(), [radius * 2.0; 2]),
                    radius,
                    Fill::Solid(color(tone)),
                    alpha,
                );
                Ok(())
            });
            let glyph = radius / GUTTER_RADIUS;
            let ink = color(surface);
            let strokes: Vec<Vec<[f32; 2]>> = match diagnostic.severity {
                Severity::Error => {
                    let d = 3.3 * glyph;
                    vec![
                        vec![
                            (center + vec2(-d, -d)).to_array(),
                            (center + vec2(d, d)).to_array(),
                        ],
                        vec![
                            (center + vec2(-d, d)).to_array(),
                            (center + vec2(d, -d)).to_array(),
                        ],
                    ]
                }
                Severity::Warning => vec![
                    vec![
                        (center + vec2(0.0, -4.6 * glyph)).to_array(),
                        (center + vec2(0.0, 1.0 * glyph)).to_array(),
                    ],
                    vec![
                        (center + vec2(0.0, 4.0 * glyph)).to_array(),
                        (center + vec2(0.0, 4.1 * glyph)).to_array(),
                    ],
                ],
                Severity::Info => vec![
                    vec![
                        (center + vec2(0.0, -4.4 * glyph)).to_array(),
                        (center + vec2(0.0, -4.3 * glyph)).to_array(),
                    ],
                    vec![
                        (center + vec2(0.0, -glyph)).to_array(),
                        (center + vec2(0.0, 4.6 * glyph)).to_array(),
                    ],
                ],
            };
            for stroke in strokes {
                canvas.polyline(&stroke, 2.0 * glyph, ink, alpha);
            }
        }
    }

    /// Carets, over the code.
    pub(super) fn composite_carets(
        &self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
        area: CodeArea,
    ) {
        let ink = color(self.theme.palette().accent);
        let clip = self.code_clip(area);
        let mut canvas = UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
        for caret in frame.annotations.carets {
            if caret.opacity <= 0.001 {
                continue;
            }
            let top = area.row(caret.x, caret.line_y)
                + vec2(-CARET[0] * 0.5, (LINE_HEIGHT - CARET[1]) * 0.5);
            let _ = canvas.clipped(clip, |canvas| {
                canvas.fill(
                    Bounds {
                        origin: top.to_array(),
                        size: CARET,
                    },
                    1.0,
                    Fill::Solid(ink),
                    caret.opacity,
                );
                Ok(())
            });
        }
    }

    /// Waves, gutter icons, and carets over the code, then Hover Cards inside
    /// the code body of the panel whose top is `panel_top`.
    pub(super) fn composite_annotations(
        &mut self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
        area: CodeArea,
        panel_top: f32,
    ) {
        self.composite_diagnostics(pixels, frame, area);
        self.composite_carets(pixels, frame, area);
        if !frame.annotations.hovers.is_empty() {
            let [width, height] = [self.spec.width as f32, self.spec.height as f32];
            let bounds = Box2 {
                min: vec2(width * 0.11 + 24.0, panel_top + 68.0),
                max: vec2(width * 0.89 - 24.0, panel_top + height * 0.70 - 28.0),
            };
            self.composite_hovers(pixels, frame, area, bounds);
        }
    }

    /// Hover Cards, over everything but the pointer, inside `bounds`.
    pub(super) fn composite_hovers(
        &mut self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
        area: CodeArea,
        bounds: Box2,
    ) {
        for hover in frame.annotations.hovers {
            self.composite_hover(pixels, hover, area, bounds);
        }
    }

    fn hover_code_sprite(&mut self, spans: &[StyledSpan]) -> String {
        let fingerprint = spans_fingerprint(spans);
        let key = format!("hover:{fingerprint:x}");
        refresh_sprite(&mut self.part_sprites, &*key, fingerprint, || {
            let mut sprite = make_spans_sprite_at_size(
                &mut self.font_system,
                &mut self.swash_cache,
                spans,
                HOVER_CODE_SIZE,
                HOVER_CODE_LINE,
            );
            self.theme.sprite(&mut sprite);
            sprite
        });
        key
    }

    fn hover_text_spec(&self) -> PlainTextSpec {
        PlainTextSpec {
            font_size: HOVER_TEXT_SIZE,
            color: [0, 0, 0],
            size: [1400, HOVER_TEXT_LINE as u32],
            semibold: false,
            crop_to_advance: true,
        }
    }

    /// The width of every line of `plan` and the card's content height.
    fn measure_hover(&mut self, plan: &HoverPlan) -> (Vec<Vec<f32>>, Vec2) {
        let spec = self.hover_text_spec();
        let mut widths = Vec::new();
        let mut size = Vec2::ZERO;
        for (index, section) in plan.sections.iter().enumerate() {
            if index > 0 {
                size.y += HOVER_RULE_GAP * 2.0 + 1.0;
            }
            match section {
                HoverSectionPlan::Code(lines) => {
                    for line in lines {
                        let key = self.hover_code_sprite(line);
                        let advance = self.part_sprites[&key].1.advance;
                        widths.push(vec![advance]);
                        size.x = size.x.max(advance);
                        size.y += HOVER_CODE_LINE;
                    }
                }
                HoverSectionPlan::Text(lines) => {
                    for line in lines {
                        let spans = line
                            .iter()
                            .map(|span| {
                                let color = self.theme.tone(span.tone);
                                if span.text.is_empty() {
                                    0.0
                                } else {
                                    self.plain_text_sprite(
                                        &span.text,
                                        PlainTextSpec { color, ..spec },
                                    )
                                    .advance
                                }
                            })
                            .collect::<Vec<_>>();
                        size.x = size.x.max(spans.iter().sum());
                        widths.push(spans);
                        size.y += HOVER_TEXT_LINE;
                    }
                }
            }
        }
        (widths, size)
    }

    fn composite_hover(
        &mut self,
        pixels: &mut [u8],
        hover: &HoverFrame<'_>,
        area: CodeArea,
        bounds: Box2,
    ) {
        let presence = hover.presence.max(0.0);
        let alpha = presence.min(1.0) * hover.opacity.clamp(0.0, 1.0);
        if alpha <= 0.001 {
            return;
        }
        let (widths, content) = self.measure_hover(hover.plan);
        let size = content + HOVER_PADDING * 2.0;
        let top = area.row(hover.x, hover.line_y);
        let range = Box2 {
            min: top + vec2(0.0, 2.0),
            max: top + vec2(hover.width, LINE_HEIGHT - 2.0),
        };
        let placed = hover_layout(range, size, hover.plan.side, 4.0, bounds);
        // It rises into place away from its range.
        let away = if placed.below { 1.0 } else { -1.0 };
        let shift = vec2(0.0, away * (presence - 1.0) * HOVER_RISE);
        let card = Box2 {
            min: placed.card.min + shift,
            max: placed.card.max + shift,
        };
        let tip = placed.tip + shift;
        let base = if placed.below { card.min.y } else { card.max.y };

        let palette = self.theme.palette();
        let material = mix(palette.surface, palette.raised, 0.8);
        let fill = color(material);
        let border = color(mix(palette.raised, palette.muted, 0.5));
        let canvas_size = [self.spec.width, self.spec.height];
        let card_bounds = Bounds {
            origin: card.min.to_array(),
            size: (card.max - card.min).to_array(),
        };
        {
            let mut canvas = UiCanvas::new(pixels, canvas_size);
            // A soft shadow lifts the card off the code.
            for (spread, strength) in [(14.0, 0.07), (8.0, 0.1), (3.0, 0.14)] {
                canvas.fill(
                    card_bounds.translate([0.0, 6.0]).expand(spread),
                    HOVER_RADIUS + spread,
                    Fill::Solid(UiColor::srgb8(0, 0, 0, 255)),
                    alpha * strength,
                );
            }
            canvas.fill(card_bounds, HOVER_RADIUS, Fill::Solid(fill), alpha);
            canvas.stroke(card_bounds, HOVER_RADIUS, 1.0, border, alpha);
        }
        // The pointer: the card's own material, its two edges bordered, joined
        // to the card without the border across its base.
        let half = psychopomp::ide::HOVER_TIP.x;
        let triangle = [
            vec2(placed.base_x - half, base),
            tip,
            vec2(placed.base_x + half, base),
        ];
        fill_triangle(
            pixels,
            canvas_size,
            [
                triangle[0] + vec2(0.0, -away * 1.5),
                triangle[1],
                triangle[2] + vec2(0.0, -away * 1.5),
            ],
            [material[0], material[1], material[2], 255],
            alpha,
        );
        UiCanvas::new(pixels, canvas_size).polyline(
            &triangle.map(|point| point.to_array()),
            1.0,
            border,
            alpha,
        );

        // Content: code in the editor's syntax colors, prose in tones, rules
        // between sections.
        let spec = self.hover_text_spec();
        let mut cursor = card.min + HOVER_PADDING;
        let mut widths = widths.into_iter();
        for (index, section) in hover.plan.sections.iter().enumerate() {
            if index > 0 {
                cursor.y += HOVER_RULE_GAP;
                UiCanvas::new(pixels, canvas_size).fill(
                    Bounds {
                        origin: [card.min.x + 1.0, cursor.y],
                        size: [card.max.x - card.min.x - 2.0, 1.0],
                    },
                    0.0,
                    Fill::Solid(border),
                    alpha * 0.8,
                );
                cursor.y += 1.0 + HOVER_RULE_GAP;
            }
            match section {
                HoverSectionPlan::Code(lines) => {
                    for line in lines {
                        let width = widths.next().map_or(0.0, |w| w[0]);
                        let key = self.hover_code_sprite(line);
                        let sprite = &self.part_sprites[&key].1;
                        composite_text(
                            pixels,
                            canvas_size,
                            TextDraw {
                                clip_width: width.ceil() + 1.0,
                                opacity: alpha,
                                ..TextDraw::new(sprite, cursor.to_array())
                            },
                        );
                        cursor.y += HOVER_CODE_LINE;
                    }
                }
                HoverSectionPlan::Text(lines) => {
                    for line in lines {
                        let line_widths = widths.next().unwrap_or_default();
                        let mut x = cursor.x;
                        for (span, width) in line.iter().zip(line_widths) {
                            if span.text.is_empty() {
                                continue;
                            }
                            let color = self.theme.tone(span.tone);
                            let sprite =
                                self.plain_text_sprite(&span.text, PlainTextSpec { color, ..spec });
                            composite_text(
                                pixels,
                                canvas_size,
                                TextDraw {
                                    clip_width: width.ceil() + 1.0,
                                    opacity: alpha,
                                    ..TextDraw::new(sprite, [x, cursor.y])
                                },
                            );
                            x += width;
                        }
                        cursor.y += HOVER_TEXT_LINE;
                    }
                }
            }
        }
    }
}

/// An antialiased filled triangle: coverage from the signed distance to its
/// nearest edge, half a pixel either side.
fn fill_triangle(
    pixels: &mut [u8],
    size: [u32; 2],
    points: [Vec2; 3],
    rgba: [u8; 4],
    opacity: f32,
) {
    let min = points[0].min(points[1]).min(points[2]).floor() - 1.0;
    let max = points[0].max(points[1]).max(points[2]).ceil() + 1.0;
    let area = (points[1] - points[0]).perp_dot(points[2] - points[0]);
    if area.abs() < 1e-3 {
        return;
    }
    let sign = area.signum();
    for y in (min.y.max(0.0) as u32)..(max.y.min(size[1] as f32).max(0.0) as u32) {
        for x in (min.x.max(0.0) as u32)..(max.x.min(size[0] as f32).max(0.0) as u32) {
            let p = vec2(x as f32 + 0.5, y as f32 + 0.5);
            let distance = (0..3)
                .map(|i| {
                    let a = points[i];
                    let b = points[(i + 1) % 3];
                    let edge = b - a;
                    sign * edge.perp_dot(p - a) / edge.length()
                })
                .fold(f32::INFINITY, f32::min);
            let coverage = (distance + 0.5).clamp(0.0, 1.0);
            if coverage > 0.0 {
                let index = (y as usize * size[0] as usize + x as usize) * 4;
                blend_pixel(&mut pixels[index..index + 4], rgba, opacity * coverage);
            }
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use psychopomp::{
        caption::CaptionSpanPlan,
        ide::{HoverPlan, Severity},
        math::{shapes::Box2, vec2},
        tone::Tone,
    };

    use super::{CodeArea, DiagnosticFrame, EditorAnnotations, HoverFrame};
    use crate::render::{EditorFrame, HeadlessRenderer, PointerFrame, RenderSpec, TokenHighlight};

    fn frame<'a>(annotations: EditorAnnotations<'a>) -> EditorFrame<'a> {
        EditorFrame {
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            panel_opacity: 1.0,
            line_marks: &[],
            panel_rotation: 0.0,
            panel_tilt_x: 0.0,
            panel_tilt_y: 0.0,
            panel_scale: 1.0,
            panel_near_blur: 0.0,
            focus_intensity: 0.0,
            focus_line_y: 0.0,
            focus_height: 44.0,
            token_highlight: TokenHighlight {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                opacity: 0.0,
            },
            pointer: PointerFrame {
                x: 0.0,
                y: 0.0,
                opacity: 0.0,
                rotation: 0.0,
                scale: 1.0,
                blur: 0.0,
            },
            inline_reveals: &[],
            lines: &[],
            annotations,
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; waves draw on from the left and hovers ink only while present"]
    fn waves_draw_on_and_hovers_leave_no_residue() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "ide-proof".into(),
        }))
        .unwrap();
        let area = CodeArea {
            origin: vec2(278.4, 287.6),
            clip_y: [287.6, 939.6],
        };
        let background = renderer.render_title_card("", None, 0.0);
        let inked = |pixels: &[u8], x: f32, y: f32| {
            let (x, y) = (x as usize, y as usize);
            (y.saturating_sub(4)..y + 4).any(|y| {
                let i = (y * 1920 + x) * 4;
                pixels[i..i + 3] != background[i..i + 3]
            })
        };
        let wave = |draw| DiagnosticFrame {
            x: 100.0,
            width: 300.0,
            line_y: 88.0,
            severity: Severity::Error,
            gutter: true,
            draw,
            wave: 1.0,
            opacity: 1.0,
        };
        let y = area.origin.y + 88.0 + super::WAVE_Y;
        let half = [wave(0.5)];
        let mut pixels = background.clone();
        renderer.composite_diagnostics(
            &mut pixels,
            &frame(EditorAnnotations {
                diagnostics: &half,
                ..Default::default()
            }),
            area,
        );
        let x = |offset: f32| area.origin.x + 100.0 + offset;
        assert!(inked(&pixels, x(20.0), y), "the wave starts at the range");
        assert!(!inked(&pixels, x(280.0), y), "and has not reached its end");
        assert!(
            inked(&pixels, 1920.0 * 0.145 - 30.0, y - super::WAVE_Y + 22.0),
            "gutter icon"
        );

        let plan = HoverPlan::new("run").text(vec![CaptionSpanPlan::new("why", Tone::Plain)]);
        let bounds = Box2 {
            min: vec2(235.0, 251.0),
            max: vec2(1685.0, 911.0),
        };
        let hover = |presence| HoverFrame {
            plan: &plan,
            x: 100.0,
            width: 120.0,
            line_y: 88.0,
            presence,
            opacity: 1.0,
        };
        let hidden = [hover(0.0)];
        let mut pixels = background.clone();
        renderer.composite_hovers(
            &mut pixels,
            &frame(EditorAnnotations {
                hovers: &hidden,
                ..Default::default()
            }),
            area,
            bounds,
        );
        assert!(pixels == background, "a hidden hover inks nothing");
        let shown = [hover(1.0)];
        renderer.composite_hovers(
            &mut pixels,
            &frame(EditorAnnotations {
                hovers: &shown,
                ..Default::default()
            }),
            area,
            bounds,
        );
        assert!(
            inked(&pixels, x(30.0), area.origin.y + 88.0 - 30.0),
            "the card sits above"
        );
        for (index, (a, b)) in pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(background.as_chunks::<4>().0)
            .enumerate()
        {
            if a != b {
                let (px, py) = ((index % 1920) as f32, (index / 1920) as f32);
                assert!(
                    px >= bounds.min.x - 20.0
                        && px <= bounds.max.x + 20.0
                        && py >= bounds.min.y - 20.0,
                    "ink stays near its bounds"
                );
            }
        }
    }
}
