//! Footage overlay pixels: one decoded frame (a still, or a video's current
//! frame) sampled once straight through the shared projected card. The fit
//! and the focus window choose the source region, the mask cuts the card's
//! outline (its border and shadow follow it), and a color treatment
//! desaturates, tints, or dims the source before it meets the material. The
//! Video Card and image recipes draw through here too, as framed and bare
//! rectangles.
use anyhow::Result;
use psychopomp::{
    footage::{FootagePlan, Mask, Treatment, focus_window},
    math::Vec2,
    video::TITLE_BAR,
};

use super::{
    HeadlessRenderer, PlainTextSpec, TextDraw, blend_pixel, composite_text,
    ui::{
        Bounds,
        card::{
            CardFrame, CardProjection, CardShape, CardStyle, ContentFit, Fill, RgbaSource,
            SourceTreatment, UiCanvas, UiColor,
        },
    },
};

/// The title bar is drawn at twice its size, so it stays crisp on a card
/// scaled past one.
const TITLE_DENSITY: f32 = 2.0;

/// One sample of a footage overlay's channels: its center on the canvas
/// (literal or anchored, plus its offsets), card pose, focus window
/// (center and size as fractions of the fit window), and color treatment.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FootagePose {
    pub center: [f32; 2],
    pub scale: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub tilt: [f32; 2],
    pub blur: f32,
    pub defocus: f32,
    pub focus: ([f32; 2], f32),
    pub treatment: Treatment,
}

/// One overlay drawn alone over transparency: straight-alpha pixels of
/// `size` whose top-left lands at `origin` on the frame. Over is
/// associative, so blending it equals drawing the overlay directly (to
/// within 8-bit rounding), and a settled overlay draws once.
pub(crate) struct FootageLayer {
    pub origin: [u32; 2],
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
}

impl FootageLayer {
    /// Composite `layers`, in order, over `pixels` (a frame `frame` pixels
    /// in size): one pass in bands of rows across threads, each band taking
    /// every layer's rows inside it.
    pub(crate) fn blend_all(layers: &[&Self], pixels: &mut [u8], frame: [u32; 2]) {
        static WORKERS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        if layers.is_empty() {
            return;
        }
        let stride = frame[0] as usize * 4;
        let workers =
            *WORKERS.get_or_init(|| std::thread::available_parallelism().map_or(1, usize::from));
        let band = (frame[1] as usize).div_ceil(workers).max(1);
        std::thread::scope(|scope| {
            for (index, rows) in pixels.chunks_mut(band * stride).enumerate() {
                scope.spawn(move || {
                    for layer in layers {
                        layer.blend_band(rows, index * band, stride);
                    }
                });
            }
        });
    }

    /// Blend the layer's rows that fall in `rows`, which start at frame row
    /// `top`. Over an opaque row (every root draws one) "over" is
    /// branchless integer arithmetic, rounded exactly.
    fn blend_band(&self, rows: &mut [u8], top: usize, stride: usize) {
        let width = self.size[0] as usize * 4;
        let left = self.origin[0] as usize * 4;
        let count = rows.len() / stride;
        let first = (self.origin[1] as usize).max(top);
        let last = (self.origin[1] as usize + self.size[1] as usize).min(top + count);
        for y in first..last {
            let line = &self.pixels[(y - self.origin[1] as usize) * width..][..width];
            let target = &mut rows[(y - top) * stride + left..][..width];
            let (pixels, _) = target.as_chunks_mut::<4>();
            let (sources, _) = line.as_chunks::<4>();
            if pixels.iter().all(|pixel| pixel[3] == 255) {
                for (destination, source) in pixels.iter_mut().zip(sources) {
                    let alpha = u32::from(source[3]);
                    for channel in 0..3 {
                        let mixed = u32::from(source[channel]) * alpha
                            + u32::from(destination[channel]) * (255 - alpha);
                        // round(mixed / 255), exactly, for mixed ≤ 255².
                        destination[channel] = (((mixed + 128) * 257) >> 16) as u8;
                    }
                }
            } else {
                for (destination, source) in pixels.iter_mut().zip(sources) {
                    blend_pixel(destination, *source, 1.0);
                }
            }
        }
    }
}

impl HeadlessRenderer {
    /// Draw `frame` (straight-alpha RGBA of `size`) as `plan` poses it.
    pub(crate) fn composite_footage(
        &mut self,
        pixels: &mut [u8],
        plan: &FootagePlan,
        frame: &[u8],
        size: [u32; 2],
        pose: FootagePose,
    ) -> Result<()> {
        let canvas = self.size();
        self.draw_footage(pixels, canvas, [0.0, 0.0], plan, frame, size, pose)
    }

    /// The overlay as a layer over transparency, cropped to what it can
    /// ink on the frame; `None` when it inks nothing.
    pub(crate) fn footage_layer(
        &mut self,
        plan: &FootagePlan,
        frame: &[u8],
        size: [u32; 2],
        pose: FootagePose,
    ) -> Result<Option<FootageLayer>> {
        if pose.opacity <= 0.001 {
            return Ok(None);
        }
        let canvas = self.size();
        let half = plan.card_size().map(|v| v * 0.5);
        let projection = CardProjection {
            scale: pose.scale.max(0.01),
            rotation_z: pose.rotation,
            tilt_x: pose.tilt[0],
            tilt_y: pose.tilt[1],
            surface_blur: 0.0,
            near_edge_blur: 0.0,
        };
        // The compositor's own reach: projected corners plus the shadow.
        let reach = if plan.framed { 30.0 * 3.0 + 18.0 } else { 0.0 } + 2.0;
        let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
            .map(|[x, y]| projection.project([x * half[0], y * half[1]]));
        let low = corners
            .iter()
            .fold([f32::MAX; 2], |low, p| [low[0].min(p[0]), low[1].min(p[1])]);
        let high = corners.iter().fold([f32::MIN; 2], |high, p| {
            [high[0].max(p[0]), high[1].max(p[1])]
        });
        let x0 = (pose.center[0] + low[0] - reach).floor().max(0.0);
        let y0 = (pose.center[1] + low[1] - reach).floor().max(0.0);
        let x1 = (pose.center[0] + high[0] + reach)
            .ceil()
            .min(canvas[0] as f32);
        let y1 = (pose.center[1] + high[1] + reach)
            .ceil()
            .min(canvas[1] as f32);
        if x1 <= x0 || y1 <= y0 {
            return Ok(None);
        }
        let origin = [x0 as u32, y0 as u32];
        let layer_size = [(x1 - x0) as u32, (y1 - y0) as u32];
        let mut pixels = vec![0; layer_size[0] as usize * layer_size[1] as usize * 4];
        self.draw_footage(&mut pixels, layer_size, [x0, y0], plan, frame, size, pose)?;
        Ok(Some(FootageLayer {
            origin,
            size: layer_size,
            pixels,
        }))
    }

    /// Draw onto `pixels` of `canvas`, whose top-left is `offset` on the frame.
    #[allow(clippy::too_many_arguments)]
    fn draw_footage(
        &mut self,
        pixels: &mut [u8],
        canvas: [u32; 2],
        offset: [f32; 2],
        plan: &FootagePlan,
        frame: &[u8],
        size: [u32; 2],
        pose: FootagePose,
    ) -> Result<()> {
        if pose.opacity <= 0.001 {
            return Ok(());
        }
        let pose = FootagePose {
            center: [pose.center[0] - offset[0], pose.center[1] - offset[1]],
            ..pose
        };
        let palette = self.theme.palette();
        let card_size = plan.card_size();
        let bar = plan.title_bar();
        let radius = match plan.mask {
            Mask::Rect { radius } => radius,
            _ => 0.0,
        };
        let style = if plan.framed {
            let [r, g, b] = palette.surface;
            let [br, bg, bb] = palette.raised;
            CardStyle {
                material: Fill::Solid(UiColor::srgb8(r, g, b, 255)),
                corner_radius: radius,
                border_width: 1.25,
                border_color: UiColor::srgb8(br, bg, bb, 255),
                shadow_offset: [0.0, 18.0],
                shadow_blur: 30.0,
                shadow_opacity: self.theme.shadow(0.55),
            }
        } else {
            CardStyle {
                material: Fill::Solid(UiColor::srgb8(0, 0, 0, 0)),
                corner_radius: radius,
                border_width: 0.0,
                border_color: UiColor::srgb8(0, 0, 0, 0),
                shadow_offset: [0.0, 0.0],
                shadow_blur: 0.0,
                shadow_opacity: 0.0,
            }
        };
        let card = CardFrame {
            bounds: Bounds::from_center(pose.center, card_size),
            style,
            projection: CardProjection {
                scale: pose.scale.max(0.01),
                rotation_z: pose.rotation,
                tilt_x: pose.tilt[0],
                tilt_y: pose.tilt[1],
                surface_blur: pose.defocus.max(0.0),
                near_edge_blur: pose.blur.max(0.0),
            },
            opacity: pose.opacity.clamp(0.0, 1.0),
        };
        let body = [card_size[0], card_size[1] - bar];
        let (window, content) = plan.fit.frame([size[0] as f32, size[1] as f32], body);
        let [vx, vy, vw, vh] = focus_window(window, pose.focus.0, pose.focus.1);
        let fit = ContentFit::Region {
            source: Bounds {
                origin: [vx, vy],
                size: [vw, vh],
            },
            content: Bounds {
                origin: [content[0], bar + content[1]],
                size: [content[2], content[3]],
            },
        };
        let title = plan
            .title
            .as_deref()
            .map(|title| self.video_title_bar(title, card_size[0]));
        let corners = plan
            .mask
            .corners(card_size)
            .into_iter()
            .map(Vec2::from)
            .collect::<Vec<_>>();
        let shape = match plan.mask {
            Mask::Rect { .. } => CardShape::Rounded,
            Mask::Circle => CardShape::Circle,
            Mask::Polygon { .. } => CardShape::Polygon(&corners),
        };
        let treatment = (!pose.treatment.is_none()).then(|| SourceTreatment {
            treatment: pose.treatment,
            tint: self.theme.tone(plan.tint).map(|v| f32::from(v) / 255.0),
        });
        let source = RgbaSource::packed(frame, size)?;
        self.composite_ui_in(pixels, canvas, |ui| {
            match (shape, treatment) {
                (CardShape::Rounded, None) => ui.card_source(card, source, fit)?,
                (shape, treatment) => ui.card_source_shaped(card, shape, treatment, source, fit)?,
            }
            if let Some((strip, strip_size)) = &title {
                ui.card_layer(
                    card,
                    RgbaSource::packed(strip, *strip_size)?,
                    Bounds {
                        origin: [0.0, 0.0],
                        size: [card_size[0], TITLE_BAR],
                    },
                )?;
            }
            Ok(())
        })
    }

    /// A transparent title bar: three quiet window dots, the centered title,
    /// and a hairline above the footage.
    pub(super) fn video_title_bar(&mut self, title: &str, width: f32) -> (Vec<u8>, [u32; 2]) {
        let palette = self.theme.palette();
        let size = [
            (width * TITLE_DENSITY).ceil() as u32,
            (TITLE_BAR * TITLE_DENSITY).ceil() as u32,
        ];
        let mut pixels = vec![0_u8; size[0] as usize * size[1] as usize * 4];
        let d = TITLE_DENSITY;
        let middle = TITLE_BAR * 0.5 * d;
        {
            let mut canvas = UiCanvas::new(&mut pixels, size);
            let [mr, mg, mb] = palette.muted;
            for index in 0..3 {
                canvas.fill(
                    Bounds::from_center([(24.0 + index as f32 * 18.0) * d, middle], [10.0 * d; 2]),
                    5.0 * d,
                    Fill::Solid(UiColor::srgb8(mr, mg, mb, 255)),
                    0.4,
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
                    (size[0] as f32 - sprite.advance) * 0.5,
                    middle - sprite.height as f32 * 0.5,
                ],
            ),
        );
        (pixels, size)
    }
}

#[cfg(test)]
mod gpu_tests {
    use psychopomp::{
        footage::{Clip, FootagePlan, Treatment},
        video::VideoPlan,
    };

    use super::FootagePose;
    use crate::render::{HeadlessRenderer, RenderSpec};

    fn pose(center: [f32; 2]) -> FootagePose {
        FootagePose {
            center,
            scale: 1.0,
            opacity: 1.0,
            rotation: 0.0,
            tilt: [0.0, 0.0],
            blur: 0.0,
            defocus: 0.0,
            focus: ([0.5, 0.5], 1.0),
            treatment: Treatment::NONE,
        }
    }

    /// Left half black, right half white.
    fn halves(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                if i % width < width / 2 {
                    [0, 0, 0, 255]
                } else {
                    [255; 4]
                }
            })
            .collect()
    }

    #[test]
    #[ignore = "requires a headless GPU; the focus window crops footage inside the card"]
    fn focus_crops_into_the_footage_and_hidden_cards_leave_no_ink() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "video-proof".into(),
        }))
        .unwrap();
        let frame = halves(64, 32);
        let video = VideoPlan::new("clip", [64, 32], 30).at([960.0, 540.0], 1200.0);
        let plan = FootagePlan::new(Clip::new("clip"), [960.0, 540.0], [1200.0, 600.0])
            .fit(psychopomp::footage::Fit::Fill)
            .framed();
        assert_eq!(plan.card_size(), video.card_size());
        let background = renderer.render_title_card("", None, 0.0);
        let draw = |renderer: &mut HeadlessRenderer, pose: FootagePose| {
            let mut pixels = background.clone();
            renderer
                .composite_footage(&mut pixels, &plan, &frame, [64, 32], pose)
                .unwrap();
            pixels
        };
        let left = |pixels: &[u8]| pixels[(540 * 1920 + 600) * 4];
        let whole = draw(&mut renderer, pose([960.0, 540.0]));
        assert_eq!(left(&whole), 0, "the card's left shows the black half");
        let focused = draw(
            &mut renderer,
            FootagePose {
                focus: ([0.75, 0.5], 0.5),
                ..pose([960.0, 540.0])
            },
        );
        assert_eq!(
            left(&focused),
            255,
            "focusing right fills the card with white"
        );
        let hidden = draw(
            &mut renderer,
            FootagePose {
                opacity: 0.0,
                ..pose([960.0, 540.0])
            },
        );
        assert!(hidden == background);
    }

    #[test]
    #[ignore = "requires a headless GPU; masks cut the outline and treatments recolor"]
    fn circles_and_polygons_cut_footage_and_treatments_desaturate() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "footage-proof".into(),
        }))
        .unwrap();
        // Saturated red footage.
        let frame = [220, 30, 30, 255].repeat(32 * 32);
        let background = renderer.render_title_card("", None, 0.0);
        let pixel = |pixels: &[u8], x: usize, y: usize| {
            let i = (y * 1920 + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        let draw = |renderer: &mut HeadlessRenderer, plan: &FootagePlan, pose: FootagePose| {
            let mut pixels = background.clone();
            renderer
                .composite_footage(&mut pixels, plan, &frame, [32, 32], pose)
                .unwrap();
            pixels
        };
        let square = FootagePlan::new(Clip::new("clip"), [960.0, 540.0], [400.0, 400.0]);
        let corner = (960 - 190, 540 - 190);
        let drawn = draw(&mut renderer, &square, pose([960.0, 540.0]));
        assert_eq!(pixel(&drawn, corner.0, corner.1), [220, 30, 30]);
        let circle = draw(
            &mut renderer,
            &square.clone().circle(),
            pose([960.0, 540.0]),
        );
        assert_eq!(
            pixel(&circle, corner.0, corner.1),
            pixel(&background, corner.0, corner.1),
            "a circle leaves the corners"
        );
        assert_eq!(pixel(&circle, 960, 540), [220, 30, 30]);
        // A triangle pointing down: its top corners are in, its bottom corners out.
        let triangle = square.clone().polygon([[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
        let cut = draw(&mut renderer, &triangle, pose([960.0, 540.0]));
        assert_eq!(pixel(&cut, corner.0, corner.1), [220, 30, 30]);
        assert_eq!(
            pixel(&cut, corner.0, 540 + 190),
            pixel(&background, corner.0, 540 + 190)
        );
        let gray = draw(
            &mut renderer,
            &square,
            FootagePose {
                treatment: Treatment {
                    saturation: 0.0,
                    ..Treatment::NONE
                },
                ..pose([960.0, 540.0])
            },
        );
        let [r, g, b] = pixel(&gray, 960, 540);
        assert!(r == g && g == b, "desaturated: {r} {g} {b}");
    }
}
