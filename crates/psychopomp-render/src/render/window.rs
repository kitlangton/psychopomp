//! The floating window shared by Terminal, Chat Thread, and Changed Files:
//! the theme's shell through the projected card compositor (shadow, border,
//! material), an optional title bar that scales with the body, and the
//! sampled pose of the body and its content. Content is drawn by each recipe
//! directly onto the frame, unscaled, once the body has nearly settled.
use std::cell::RefCell;

use anyhow::Result;
use psychopomp::{
    math::{Vec2, smoothstep, vec2},
    window::TITLE_BAR,
};

use super::{
    HeadlessRenderer, PlainTextSpec, TextDraw, Theme, VerticalMask, blend_pixel, composite_text,
    theme::mix,
    ui::{
        Bounds,
        card::{
            CardFrame, CardProjection, CardStyle, ContentFit, Fill, RgbaSource, UiCanvas, UiColor,
        },
    },
};

/// The title bar is drawn at twice its size, so it stays crisp while scaled.
const BAR_DENSITY: f32 = 2.0;
/// Content settles up from this far below as it fades in.
const CONTENT_RISE: f32 = 6.0;

/// One sample of a window's channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowPose {
    pub offset: [f32; 2],
    pub scale: f32,
    pub opacity: f32,
    pub content: f32,
}

impl WindowPose {
    pub fn sample(sample: impl Fn(&str, f32) -> f32) -> Self {
        Self {
            offset: [sample("x", 0.0), sample("y", 0.0)],
            scale: sample("scale", 1.0).max(0.05),
            opacity: sample("opacity", 1.0).clamp(0.0, 1.0),
            content: sample("content", 1.0).clamp(0.0, 1.0),
        }
    }

    pub fn visible(&self) -> bool {
        self.opacity > 0.001
    }

    /// Opacity of everything inside the window.
    pub fn ink(&self) -> f32 {
        self.opacity * smoothstep(self.content)
    }

    /// Where an authored content point lands this sample.
    pub fn place(&self, point: [f32; 2]) -> [f32; 2] {
        [
            point[0] + self.offset[0],
            point[1] + self.offset[1] + (1.0 - self.content) * CONTENT_RISE,
        ]
    }
}

/// How a window's shell is dressed.
#[derive(Clone, Copy)]
pub(crate) struct WindowChrome<'a> {
    /// Show the title bar with window controls and this optional title.
    pub bar: bool,
    pub title: Option<&'a str>,
    pub fill: [u8; 3],
    pub corner_radius: f32,
}

/// The last composed shell of one window: its shadow, material, border, and
/// title bar at full opacity over transparency, cropped to their bounds. A
/// window's pose holds still while its content moves, so most samples blend
/// this layer instead of projecting the card again.
#[derive(Default)]
pub(crate) struct ShellCache(RefCell<Option<Shell>>);

struct Shell {
    key: ShellKey,
    origin: [usize; 2],
    size: [usize; 2],
    pixels: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
struct ShellKey {
    theme: Theme,
    canvas: [u32; 2],
    bounds: [u32; 4],
    scale: u32,
    bar: bool,
    title: Option<String>,
    fill: [u8; 3],
    corner_radius: u32,
}

impl HeadlessRenderer {
    /// Paint a window's shell and title bar at `bounds` (authored, before the
    /// pose's offset); its content is the caller's.
    pub(crate) fn composite_window(
        &mut self,
        pixels: &mut [u8],
        bounds: Bounds,
        pose: WindowPose,
        chrome: WindowChrome<'_>,
        cache: &ShellCache,
    ) -> Result<()> {
        let placed = bounds.translate(pose.offset);
        let key = ShellKey {
            theme: self.theme,
            canvas: [self.spec.width, self.spec.height],
            bounds: [
                placed.origin[0],
                placed.origin[1],
                placed.size[0],
                placed.size[1],
            ]
            .map(f32::to_bits),
            scale: pose.scale.to_bits(),
            bar: chrome.bar,
            title: chrome.title.map(str::to_owned),
            fill: chrome.fill,
            corner_radius: chrome.corner_radius.to_bits(),
        };
        let mut cached = cache.0.borrow_mut();
        if cached.as_ref().is_none_or(|shell| shell.key != key) {
            *cached = Some(self.compose_shell(bounds, pose, chrome, key)?);
        }
        let shell = cached.as_ref().expect("composed above");
        let width = self.spec.width as usize;
        for row in 0..shell.size[1] {
            let source = row * shell.size[0] * 4;
            let target = ((shell.origin[1] + row) * width + shell.origin[0]) * 4;
            let source = &shell.pixels[source..source + shell.size[0] * 4];
            let target = &mut pixels[target..target + shell.size[0] * 4];
            for (pixel, layer) in target
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(source.as_chunks::<4>().0)
            {
                if layer[3] == 255 && pose.opacity >= 1.0 {
                    *pixel = *layer;
                } else if layer[3] != 0 {
                    blend_pixel(pixel, *layer, pose.opacity);
                }
            }
        }
        Ok(())
    }

    fn compose_shell(
        &mut self,
        bounds: Bounds,
        pose: WindowPose,
        chrome: WindowChrome<'_>,
        key: ShellKey,
    ) -> Result<Shell> {
        let palette = self.theme.palette();
        let [r, g, b] = chrome.fill;
        let [br, bg, bb] = palette.raised;
        let card = CardFrame {
            bounds: bounds.translate(pose.offset),
            style: CardStyle {
                material: Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                corner_radius: chrome.corner_radius,
                border_width: 1.25,
                border_color: UiColor::srgb8(br, bg, bb, 255),
                shadow_offset: [0.0, 22.0],
                shadow_blur: 34.0,
                shadow_opacity: self.theme.shadow(0.5),
            },
            projection: CardProjection {
                scale: pose.scale,
                ..CardProjection::default()
            },
            opacity: 1.0,
        };
        let bar = chrome
            .bar
            .then(|| self.window_bar(chrome.title, bounds.size[0], chrome.fill));
        let empty = [0_u8; 4];
        let [width, height] = [self.spec.width as usize, self.spec.height as usize];
        let mut layer = vec![0_u8; width * height * 4];
        self.composite_ui(&mut layer, |ui| {
            // No content: the shell alone, with its material.
            ui.card_source(
                card,
                RgbaSource::packed(&empty, [1, 1])?,
                ContentFit::Region {
                    source: Bounds {
                        origin: [0.0, 0.0],
                        size: [1.0, 1.0],
                    },
                    content: Bounds {
                        origin: [0.0, 0.0],
                        size: [0.0, 0.0],
                    },
                },
            )?;
            if let Some((bar, size)) = &bar {
                ui.card_layer(
                    card,
                    RgbaSource::packed(bar, *size)?,
                    Bounds {
                        origin: [0.0, 0.0],
                        size: [bounds.size[0], TITLE_BAR],
                    },
                )?;
            }
            Ok(())
        })?;
        // Crop to the inked rows and columns.
        let inked = |x: usize, y: usize| layer[(y * width + x) * 4 + 3] != 0;
        let rows = (0..height)
            .filter(|&y| (0..width).any(|x| inked(x, y)))
            .collect::<Vec<_>>();
        let (Some(&top), Some(&bottom)) = (rows.first(), rows.last()) else {
            return Ok(Shell {
                key,
                origin: [0, 0],
                size: [0, 0],
                pixels: Vec::new(),
            });
        };
        let columns = (0..width)
            .filter(|&x| (top..=bottom).any(|y| inked(x, y)))
            .collect::<Vec<_>>();
        let (left, right) = (
            columns[0],
            *columns.last().expect("an inked row has a column"),
        );
        let size = [right - left + 1, bottom - top + 1];
        let mut pixels = Vec::with_capacity(size[0] * size[1] * 4);
        for y in top..=bottom {
            let start = (y * width + left) * 4;
            pixels.extend_from_slice(&layer[start..start + size[0] * 4]);
        }
        Ok(Shell {
            key,
            origin: [left, top],
            size,
            pixels,
        })
    }

    /// A title bar: a slightly lifted strip with three window controls, the
    /// centered title, and a hairline above the body.
    fn window_bar(
        &mut self,
        title: Option<&str>,
        width: f32,
        fill: [u8; 3],
    ) -> (Vec<u8>, [u32; 2]) {
        let palette = self.theme.palette();
        let d = BAR_DENSITY;
        let size = [(width * d).ceil() as u32, (TITLE_BAR * d).ceil() as u32];
        let mut pixels = vec![0_u8; size[0] as usize * size[1] as usize * 4];
        let middle = TITLE_BAR * 0.5 * d;
        {
            let mut canvas = UiCanvas::new(&mut pixels, size);
            let [r, g, b] = mix(fill, palette.raised, 0.32);
            canvas.fill(
                Bounds {
                    origin: [0.0, 0.0],
                    size: [size[0] as f32, size[1] as f32],
                },
                0.0,
                Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                1.0,
            );
            use psychopomp::tone::Tone;
            for (index, tone) in [Tone::Error, Tone::Warning, Tone::Success]
                .into_iter()
                .enumerate()
            {
                let [r, g, b] = mix(self.theme.tone(tone), palette.muted, 0.18);
                canvas.fill(
                    Bounds::from_center([(22.0 + index as f32 * 20.0) * d, middle], [12.0 * d; 2]),
                    6.0 * d,
                    Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                    0.85,
                );
            }
            let [r, g, b] = palette.raised;
            canvas.fill(
                Bounds {
                    origin: [0.0, (TITLE_BAR - 1.0) * d],
                    size: [size[0] as f32, d],
                },
                0.0,
                Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                1.0,
            );
        }
        if let Some(title) = title.filter(|title| !title.is_empty()) {
            let spec = PlainTextSpec {
                font_size: 15.0 * d,
                color: palette.muted,
                size: [size[0], (15.0 * d * 1.5).ceil() as u32],
                semibold: false,
                crop_to_advance: true,
            };
            let sprite = self.plain_text_sprite(title, spec);
            composite_text(
                &mut pixels,
                size,
                TextDraw::new(
                    sprite,
                    [
                        ((size[0] as f32 - sprite.advance) * 0.5).round(),
                        middle - sprite.height as f32 * 0.5,
                    ],
                ),
            );
        }
        (pixels, size)
    }

    /// One line of CommitMono spans from `x` (left edge) centered on `y`.
    /// `shown` limits the characters drawn; returns the pen's end.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mono_spans<'a>(
        &mut self,
        pixels: &mut [u8],
        spans: impl IntoIterator<Item = (&'a str, [u8; 3])>,
        size: f32,
        [x, y]: [f32; 2],
        alpha: f32,
        mut shown: usize,
        mask: Option<VerticalMask>,
    ) -> f32 {
        let canvas = [self.spec.width, self.spec.height];
        let mut pen = x;
        for (text, color) in spans {
            let chars = text.chars().count();
            if chars == 0 {
                continue;
            }
            let spec = mono_spec(size, color);
            let sprite = self.plain_text_sprite(text, spec);
            let take = shown.min(chars);
            let width = sprite.advance * take as f32 / chars as f32;
            if take > 0 && alpha > 0.001 {
                composite_text(
                    pixels,
                    canvas,
                    TextDraw {
                        clip_width: if take == chars {
                            sprite.width as f32
                        } else {
                            width
                        },
                        opacity: alpha,
                        mask,
                        ..TextDraw::new(sprite, [pen, y - sprite.height as f32 * 0.5])
                    },
                );
            }
            pen += width;
            shown -= take;
            if take < chars {
                break;
            }
        }
        pen
    }

    /// The advance of one CommitMono column at `size`.
    pub(crate) fn mono_column(&mut self, size: f32) -> f32 {
        self.plain_text_sprite("0", mono_spec(size, [0; 3])).advance
    }

    /// A round stroke through weighted points (each point's ink 0..1), as
    /// the spinner's fading wake needs. Coverage takes the nearest segment.
    pub(crate) fn weighted_stroke(
        &mut self,
        pixels: &mut [u8],
        points: &[(Vec2, f32)],
        width: f32,
        color: [u8; 3],
        alpha: f32,
        mask: Option<VerticalMask>,
    ) {
        if points.len() < 2 || alpha <= 0.001 {
            return;
        }
        let canvas = [self.spec.width as i32, self.spec.height as i32];
        let reach = width * 0.5 + 1.0;
        let (min, max) = points.iter().fold(
            (
                vec2(f32::INFINITY, f32::INFINITY),
                vec2(f32::NEG_INFINITY, f32::NEG_INFINITY),
            ),
            |(min, max), (p, _)| (min.min(*p), max.max(*p)),
        );
        for y in
            ((min.y - reach).floor() as i32).max(0)..((max.y + reach).ceil() as i32).min(canvas[1])
        {
            let row = mask.map_or(1.0, |mask| mask.coverage(y as f32, y as f32 + 1.0));
            if row <= 0.0 {
                continue;
            }
            for x in ((min.x - reach).floor() as i32).max(0)
                ..((max.x + reach).ceil() as i32).min(canvas[0])
            {
                let p = vec2(x as f32 + 0.5, y as f32 + 0.5);
                let mut best = (f32::INFINITY, 0.0);
                for pair in points.windows(2) {
                    let ((a, wa), (b, wb)) = (pair[0], pair[1]);
                    let d = b - a;
                    let t = if d.length_squared() > 0.0 {
                        ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let distance = (p - (a + d * t)).length();
                    if distance < best.0 {
                        best = (distance, wa + (wb - wa) * t);
                    }
                }
                let coverage = (width * 0.5 + 0.5 - best.0).clamp(0.0, 1.0) * best.1 * row;
                if coverage <= 0.0 {
                    continue;
                }
                let index = (y as usize * canvas[0] as usize + x as usize) * 4;
                let [r, g, b] = color;
                blend_pixel(
                    &mut pixels[index..index + 4],
                    [r, g, b, 255],
                    alpha * coverage,
                );
            }
        }
    }
}

pub(crate) fn mono_spec(size: f32, color: [u8; 3]) -> PlainTextSpec {
    PlainTextSpec {
        font_size: size,
        color,
        size: [2400, (size * 1.5).ceil() as u32],
        semibold: false,
        crop_to_advance: true,
    }
}

/// Run `draw` on a canvas clipped to a rectangle (hard edges).
pub(crate) fn clipped(
    pixels: &mut [u8],
    size: [u32; 2],
    clip: Bounds,
    corner_radius: f32,
    draw: impl FnOnce(&mut UiCanvas<'_>),
) {
    let mut canvas = UiCanvas::new(pixels, size);
    canvas
        .clipped(
            super::ui::card::Clip::rounded(clip, corner_radius),
            |canvas| {
                draw(canvas);
                Ok(())
            },
        )
        .expect("drawing cannot fail");
}

/// A color as a solid UI fill.
pub(crate) fn solid([r, g, b]: [u8; 3]) -> Fill {
    Fill::Solid(UiColor::srgb8(r, g, b, 255))
}
