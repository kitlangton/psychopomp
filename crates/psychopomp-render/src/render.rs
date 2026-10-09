use std::{
    collections::{HashMap, VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::mpsc,
};

use anyhow::{Context, Result, bail};
use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
use wgpu::util::DeviceExt;

use psychopomp::code::{CodeLine, LineId, PlacedLine, StyledSpan, SyntaxStyle};

mod callout;
mod caption;
mod changed_files;
mod chart;
mod chat;
mod component_prototype;
mod debug;
mod fonts;
mod footage;
mod grid;
mod header;
mod ide;
mod image;
mod lanes;
mod lens;
mod line_marks;
mod lower_third;
mod plot;
mod rich_text;
mod rolling;
mod sequence;
mod stage;
mod task;
mod terminal;
mod text;
mod theme;
mod transition;
mod tree;
mod ui;
mod value;
mod venn;
mod viz;
mod window;
mod wipe;
use text::{PlainTextSpec, TextSprite, blend_pixel, blend_pixel_at, make_sprite, paint_rect};

pub(crate) use callout::CalloutPose;
pub(crate) use changed_files::ChangedFilesLayout;
pub(crate) use chat::ChatGlyphs;
pub(crate) use component_prototype::PrototypeGlyphs;
pub(crate) use footage::{FootageLayer, FootagePose};
pub use grid::{
    GridFrame, GridItemFrame, GridLabelStyle, GridLinePalette, GridTextClip, GridTextDisclosure,
};
pub(crate) use header::{HeaderGlyphs, header_words};
pub use ide::{
    CaretFrame, DiagnosticFrame, EditorAnnotations, HoverFrame, InlayFrame, SelectionFrame,
};
pub(crate) use image::decode_image;
pub(crate) use lens::composite_lens;
pub(crate) use lower_third::LowerThirdGlyphs;
pub(crate) use rich_text::{RichTextGlyphs, RichTextSource, parse as parse_rich_text};
#[cfg(test)]
pub(crate) use stage::{POST_SHADER, PRIMITIVE_SHADER, SHADER_STRUCTS};
pub(crate) use stage::{StageGpu, icon_svg, stage_anchor};
pub use task::{BubblePose, ContentPose, TaskContentFrame, TaskVisualFrame};
pub(crate) use terminal::TerminalNames;
pub use theme::Theme;
pub(crate) use tree::TreeNames;
pub(crate) use venn::validate as validate_venn;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const BYTES_PER_PIXEL: u32 = 4;
const COPY_ROW_ALIGNMENT: u32 = 256;
const LINE_HEIGHT: f32 = 44.0;

#[derive(Clone)]
pub struct RenderSpec {
    pub width: u32,
    pub height: u32,
    pub file_name: String,
}

pub struct EditorFrame<'a> {
    pub panel_offset_x: f32,
    pub panel_offset_y: f32,
    /// Whole-card opacity over the editor background.
    pub panel_opacity: f32,
    pub line_marks: &'a [LineMarkFrame<'a>],
    pub panel_rotation: f32,
    pub panel_tilt_x: f32,
    pub panel_tilt_y: f32,
    pub panel_scale: f32,
    pub panel_near_blur: f32,
    pub focus_intensity: f32,
    pub focus_line_y: f32,
    pub focus_height: f32,
    pub token_highlight: TokenHighlight,
    pub pointer: PointerFrame,
    pub inline_reveals: &'a [InlineRevealFrame<'a>],
    pub lines: &'a [PlacedLine<'a>],
    /// Diagnostics, Hover Cards, Inlay Hints, and Cursors.
    pub annotations: EditorAnnotations<'a>,
}

impl EditorFrame<'_> {
    fn panel_offset(&self) -> [f32; 2] {
        [self.panel_offset_x, self.panel_offset_y]
    }

    fn panel_projection(&self) -> ui::card::CardProjection {
        ui::card::CardProjection {
            scale: self.panel_scale,
            rotation_z: self.panel_rotation,
            tilt_x: self.panel_tilt_x,
            tilt_y: self.panel_tilt_y,
            surface_blur: 0.0,
            near_edge_blur: self.panel_near_blur,
        }
    }
}

/// The editor panel's pose: what `panel-*` channels do to the projected card.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EditorPanel {
    pub offset: [f32; 2],
    pub scale: f32,
    pub rotation: f32,
    pub tilt: [f32; 2],
}

/// Where the flat editor surface is cut out and where its card lands.
struct EditorCard {
    size: [u32; 2],
    source_origin: [u32; 2],
}

impl EditorCard {
    fn new([width, height]: [u32; 2]) -> Self {
        let size = [
            (width as f32 * 0.78).round() as u32,
            (height as f32 * 0.70).round() as u32,
        ];
        Self {
            size,
            source_origin: [(width - size[0]) / 2, (height as f32 * 0.17).round() as u32],
        }
    }

    fn center([width, height]: [u32; 2], offset: [f32; 2]) -> [f32; 2] {
        [
            width as f32 * 0.5 + offset[0],
            height as f32 * 0.52 + offset[1],
        ]
    }
}

/// The canvas position of a point in editor code coordinates (x from the code
/// column, y from the first row's top, as Semantic Targets measure them),
/// carried through the same card projection the full editor path composites.
pub(crate) fn editor_canvas_point(
    canvas: [u32; 2],
    panel: EditorPanel,
    point: [f32; 2],
) -> [f32; 2] {
    let card = EditorCard::new(canvas);
    let flat = [
        canvas[0] as f32 * 0.145 + point[0],
        canvas[1] as f32 * 0.17 + 104.0 + point[1],
    ];
    let local = [
        flat[0] - card.source_origin[0] as f32 - card.size[0] as f32 * 0.5,
        flat[1] - card.source_origin[1] as f32 - card.size[1] as f32 * 0.5,
    ];
    let projected = ui::card::CardProjection {
        scale: panel.scale,
        rotation_z: panel.rotation,
        tilt_x: panel.tilt[0],
        tilt_y: panel.tilt[1],
        ..Default::default()
    }
    .project(local);
    let center = EditorCard::center(canvas, panel.offset);
    [center[0] + projected[0], center[1] + projected[1]]
}

/// A diff-marked line: a tinted row and gutter sign under the code.
#[derive(Clone, Copy)]
pub struct LineMarkFrame<'a> {
    pub line_id: &'a str,
    pub mark: psychopomp::editor::LineMarkPlan,
    pub presence: f32,
    /// Row spacing, so consecutive marked rows join into one band.
    pub row_height: f32,
}

#[derive(Clone, Copy)]
pub struct TokenHighlight {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub opacity: f32,
}

#[derive(Clone, Copy)]
pub struct PointerFrame {
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub scale: f32,
    pub blur: f32,
}

#[derive(Clone, Copy)]
pub struct InlineRevealFrame<'a> {
    pub line_id: &'a str,
    pub start_span: usize,
    pub end_span: usize,
    pub progress: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TextRangeBounds {
    pub x: f32,
    pub width: f32,
}

/// One partition of a line, shaped exactly as the inline compositor shapes it.
/// Selection bounds are local to this partition; hidden partitions occupy zero width.
pub(super) struct InlineRangeMetrics {
    pub spans: std::ops::Range<usize>,
    pub advance: f32,
    pub selection: Option<[f32; 2]>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    resolution: [f32; 4],
    focus: [f32; 4],
    token_highlight: [f32; 4],
    surface: [f32; 4],
    accent: [f32; 4],
}

/// Stationary canvas-space aperture. Text moves through it; the fade does not
/// move with the glyphs or paint over the already-composited background.
#[derive(Clone, Copy, Debug)]
pub struct VerticalMask {
    pub top: f32,
    pub bottom: f32,
    pub fade: f32,
}

impl VerticalMask {
    pub fn is_valid(self) -> bool {
        let height = self.bottom - self.top;
        self.top.is_finite()
            && self.bottom.is_finite()
            && height.is_finite()
            && height > 0.
            && self.fade.is_finite()
            && self.fade >= 0.
            && self.fade <= height * 0.5
    }

    fn coverage(self, start: f32, end: f32) -> f32 {
        let start = start.max(self.top);
        let end = end.min(self.bottom);
        if start >= end {
            return 0.;
        }
        if self.fade == 0. {
            return (end - start).clamp(0., 1.);
        }
        // Integrate the linear mask over this pixel's covered row interval.
        // Fractional mask edges cannot expose an opaque row at once.
        let fade = f64::from(self.fade);
        let ramp = |y: f64| {
            if y <= 0. {
                0.
            } else if y < fade {
                y * y / (2. * fade)
            } else {
                y - fade * 0.5
            }
        };
        let integral = |y: f32| {
            ramp(f64::from(y) - f64::from(self.top))
                - ramp(f64::from(y) - (f64::from(self.bottom) - fade))
        };
        (integral(end) - integral(start)).clamp(0., 1.) as f32
    }
}

pub struct HeadlessRenderer {
    spec: RenderSpec,
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
    scene_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    font_system: FontSystem,
    swash_cache: SwashCache,
    title_sprite: TextSprite,
    pointer_sprite: TextSprite,
    line_sprites: HashMap<LineId, (u64, TextSprite)>,
    part_sprites: HashMap<String, (u64, TextSprite)>,
    plain_text_sprites: text::PlainTextCache,
    editor_background_pixels: Vec<u8>,
    ui_card_pixels: Vec<u8>,
    ui_overlay_pixels: Vec<u8>,
    interactive_preview: bool,
    preview_editor_backgrounds: VecDeque<(String, Vec<u8>)>,
    grid_renderer: Option<grid::GridRenderer>,
    grid_line_palette: Option<GridLinePalette>,
    theme: Theme,
}

impl HeadlessRenderer {
    /// The delivered frame size in pixels.
    pub(crate) fn size(&self) -> [u32; 2] {
        [self.spec.width, self.spec.height]
    }

    pub async fn new(spec: RenderSpec) -> Result<Self> {
        if spec.width == 0 || spec.height == 0 {
            bail!("render dimensions must be non-zero");
        }

        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .context("request a headless GPU adapter")?;
        let info = adapter.get_info();
        eprintln!("GPU: {} ({:?})", info.name, info.backend);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("psychopomp prototype device"),
                ..Default::default()
            })
            .await
            .context("request a wgpu device")?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output frame"),
            size: wgpu::Extent3d {
                width: spec.width,
                height: spec.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let unpadded_bytes_per_row = spec.width * BYTES_PER_PIXEL;
        let padded_bytes_per_row =
            unpadded_bytes_per_row.div_ceil(COPY_ROW_ALIGNMENT) * COPY_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(spec.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let initial_uniforms = SceneUniforms {
            resolution: [spec.width as f32, spec.height as f32, 0.0, 0.0],
            focus: [0.0, 0.0, LINE_HEIGHT, 0.0],
            token_highlight: [0.0; 4],
            surface: [0.; 4],
            accent: [0.; 4],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene uniforms"),
            contents: bytemuck::bytes_of(&initial_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &scene_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let mut font_system = fonts::font_system();
        let mut swash_cache = SwashCache::new();
        let title_sprite = make_title_sprite(&mut font_system, &mut swash_cache, &spec.file_name);
        let pointer_sprite = make_pointer_sprite()?;
        Ok(Self {
            spec,
            device,
            queue,
            texture,
            view,
            readback,
            padded_bytes_per_row,
            scene_pipeline,
            uniform_buffer,
            scene_bind_group,
            font_system,
            swash_cache,
            title_sprite,
            pointer_sprite,
            line_sprites: HashMap::new(),
            part_sprites: HashMap::new(),
            plain_text_sprites: text::PlainTextCache::default(),
            editor_background_pixels: Vec::new(),
            ui_card_pixels: Vec::new(),
            ui_overlay_pixels: Vec::new(),
            interactive_preview: false,
            preview_editor_backgrounds: VecDeque::new(),
            grid_renderer: None,
            grid_line_palette: None,
            theme: Theme::default(),
        })
    }

    /// Live flat-editor preview skips the final optical resampling of glyphs.
    /// Export never enables this profile; unsupported poses/effects use the full path.
    pub fn set_interactive_preview(&mut self, enabled: bool) {
        self.interactive_preview = enabled;
    }

    pub(crate) fn theme(&self) -> Theme {
        self.theme
    }

    pub fn set_theme(&mut self, theme: Theme) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        self.line_sprites.clear();
        self.part_sprites.clear();
        self.plain_text_sprites.clear();
        self.preview_editor_backgrounds.clear();
        self.editor_background_pixels.clear();
        self.title_sprite = make_title_sprite(
            &mut self.font_system,
            &mut self.swash_cache,
            &self.spec.file_name,
        );
        theme.sprite(&mut self.title_sprite);
    }

    pub fn composite_ui<R>(
        &mut self,
        pixels: &mut [u8],
        draw: impl FnOnce(&mut ui::card::FrameUi<'_>) -> Result<R>,
    ) -> Result<R> {
        self.composite_ui_in(pixels, [self.spec.width, self.spec.height], draw)
    }

    /// `composite_ui` into `pixels` of `size`, such as one overlay's layer.
    pub(crate) fn composite_ui_in<R>(
        &mut self,
        pixels: &mut [u8],
        size: [u32; 2],
        draw: impl FnOnce(&mut ui::card::FrameUi<'_>) -> Result<R>,
    ) -> Result<R> {
        let mut card_pixels = std::mem::take(&mut self.ui_card_pixels);
        let mut overlay_pixels = std::mem::take(&mut self.ui_overlay_pixels);
        let result = {
            let mut frame =
                ui::card::FrameUi::new(pixels, size, &mut card_pixels, &mut overlay_pixels)?;
            draw(&mut frame)
        };
        self.ui_card_pixels = card_pixels;
        self.ui_overlay_pixels = overlay_pixels;
        result
    }

    pub fn set_file_name(&mut self, file_name: &str) {
        if self.spec.file_name == file_name {
            return;
        }
        self.spec.file_name = file_name.to_owned();
        self.title_sprite = make_title_sprite(
            &mut self.font_system,
            &mut self.swash_cache,
            &self.spec.file_name,
        );
        self.theme.sprite(&mut self.title_sprite);
    }

    pub fn render_title_card(
        &mut self,
        title: &str,
        subtitle: Option<&str>,
        opacity: f32,
    ) -> Vec<u8> {
        let mut pixels = vec![0_u8; self.spec.width as usize * self.spec.height as usize * 4];
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let [r, g, b] = self.theme.background([1, 2, 4]);
            pixel.copy_from_slice(&[r, g, b, 255]);
        }
        let center_x = self.spec.width as f32 * 0.5;
        let center_y = self.spec.height as f32 * 0.5;
        self.composite_centered_text(
            &mut pixels,
            title,
            [center_x, center_y - 20.0],
            64.0,
            [238, 240, 244],
            opacity,
        );
        if let Some(subtitle) = subtitle {
            self.composite_centered_text(
                &mut pixels,
                subtitle,
                [center_x, center_y + 64.0],
                26.0,
                [135, 145, 160],
                opacity * 0.9,
            );
        }
        pixels
    }

    pub fn composite_centered_text(
        &mut self,
        pixels: &mut [u8],
        text: &str,
        center: [f32; 2],
        font_size: f32,
        color: [u8; 3],
        opacity: f32,
    ) {
        self.composite_centered_text_masked(pixels, text, center, font_size, color, opacity, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_centered_text_masked(
        &mut self,
        pixels: &mut [u8],
        text: &str,
        center: [f32; 2],
        font_size: f32,
        color: [u8; 3],
        opacity: f32,
        mask: Option<VerticalMask>,
    ) {
        let canvas_size = [self.spec.width, self.spec.height];
        let color = self.theme.ink(color);
        let height = (font_size * 1.5).ceil() as u32;
        let sprite = self.plain_text_sprite(
            text,
            PlainTextSpec {
                font_size,
                color,
                size: [canvas_size[0].saturating_sub(160), height],
                semibold: false,
                crop_to_advance: false,
            },
        );
        composite_text(
            pixels,
            canvas_size,
            TextDraw {
                opacity,
                mask,
                ..TextDraw::new(
                    sprite,
                    [
                        center[0] - sprite.advance * 0.5,
                        center[1] - sprite.height as f32 * 0.5,
                    ],
                )
            },
        );
    }

    pub fn render_shapes(&mut self, frame: &EditorFrame<'_>) -> Result<Vec<u8>> {
        self.render_shapes_pass(frame, true)
    }

    fn render_shapes_pass(&mut self, frame: &EditorFrame<'_>, chrome: bool) -> Result<Vec<u8>> {
        let panel_center_y = self.spec.height as f32 * 0.52;
        let panel_top = self.spec.height as f32 * 0.17;
        let code_top = panel_top + 104.0;
        let focus_offset_y = code_top + frame.focus_line_y + LINE_HEIGHT * 0.5 - panel_center_y;
        let uniforms = SceneUniforms {
            resolution: [self.spec.width as f32, self.spec.height as f32, 0.0, 0.0],
            focus: [
                frame.focus_intensity,
                focus_offset_y,
                frame.focus_height,
                if chrome { 1.0 } else { 0.0 },
            ],
            token_highlight: [
                frame.token_highlight.x,
                frame.token_highlight.y,
                frame.token_highlight.width,
                frame.token_highlight.opacity,
            ],
            surface: {
                let c = theme::linear(self.theme.palette().raised);
                [c[0], c[1], c[2], f32::from(self.theme != Theme::Original)]
            },
            accent: {
                let c = theme::linear(self.theme.palette().accent);
                [c[0], c[1], c[2], 1.]
            },
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render scene geometry"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene geometry pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.scene_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.read_frame(encoder)
    }

    fn read_frame(&self, mut encoder: wgpu::CommandEncoder) -> Result<Vec<u8>> {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.spec.height),
                },
            },
            wgpu::Extent3d {
                width: self.spec.width,
                height: self.spec.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = self.readback.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("wait for GPU frame")?;
        receiver
            .recv()
            .context("receive GPU readback result")?
            .context("map GPU readback buffer")?;

        let unpadded_bytes_per_row = self.spec.width as usize * BYTES_PER_PIXEL as usize;
        let bytes = slice.get_mapped_range().context("read mapped GPU frame")?;
        let mut frame_bytes =
            Vec::with_capacity(unpadded_bytes_per_row * self.spec.height as usize);
        for row in bytes
            .chunks_exact(self.padded_bytes_per_row as usize)
            .take(self.spec.height as usize)
        {
            frame_bytes.extend_from_slice(&row[..unpadded_bytes_per_row]);
        }
        drop(bytes);
        self.readback.unmap();
        Ok(frame_bytes)
    }

    pub fn render_editor(&mut self, frame: &EditorFrame<'_>) -> Result<Vec<u8>> {
        if self.interactive_preview
            && can_preview_editor(frame, [self.spec.width, self.spec.height])
        {
            if let Some(index) = self
                .preview_editor_backgrounds
                .iter()
                .position(|(name, _)| name == &self.spec.file_name)
            {
                let cached = self
                    .preview_editor_backgrounds
                    .remove(index)
                    .expect("located cached chrome");
                self.preview_editor_backgrounds.push_back(cached);
            } else {
                let background_frame = EditorFrame {
                    lines: &[],
                    annotations: EditorAnnotations::default(),
                    focus_intensity: 0.,
                    token_highlight: TokenHighlight {
                        opacity: 0.,
                        ..frame.token_highlight
                    },
                    ..*frame
                };
                let pixels = self.render_editor_full(&background_frame)?;
                // Two editor slides are the demonstrated reuse; bound retained
                // RGBA memory instead of keeping one full canvas for every file.
                if self.preview_editor_backgrounds.len() == 2 {
                    self.preview_editor_backgrounds.pop_front();
                }
                self.preview_editor_backgrounds
                    .push_back((self.spec.file_name.clone(), pixels));
            }
            let mut pixels = self
                .preview_editor_backgrounds
                .back()
                .expect("background prepared")
                .1
                .clone();
            if frame.focus_intensity > 0. || frame.token_highlight.opacity > 0. {
                // The same WGSL recipe draws dynamic overlays, without sending
                // unchanged chrome through the expensive optical card compositor.
                let overlay = self.render_shapes_pass(frame, false)?;
                for (pixel, source) in pixels
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(overlay.as_chunks::<4>().0)
                {
                    if source[3] > 0 {
                        blend_pixel(pixel, [source[0], source[1], source[2], source[3]], 1.);
                    }
                }
            }
            self.composite_text_untransformed(&mut pixels, frame)?;
            return Ok(pixels);
        }
        self.render_editor_full(frame)
    }

    fn render_editor_full(&mut self, frame: &EditorFrame<'_>) -> Result<Vec<u8>> {
        let flat_frame = EditorFrame {
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            panel_opacity: 1.0,
            line_marks: frame.line_marks,
            panel_rotation: 0.0,
            panel_tilt_x: 0.0,
            panel_tilt_y: 0.0,
            panel_scale: 1.0,
            panel_near_blur: 0.0,
            focus_intensity: frame.focus_intensity,
            focus_line_y: frame.focus_line_y,
            focus_height: frame.focus_height,
            token_highlight: frame.token_highlight,
            pointer: frame.pointer,
            inline_reveals: frame.inline_reveals,
            lines: frame.lines,
            annotations: frame.annotations,
        };
        let mut flat_pixels = self.render_shapes(&flat_frame)?;
        self.composite_editor_title(&mut flat_pixels, flat_frame.panel_offset_y);
        self.composite_text_untransformed(&mut flat_pixels, &flat_frame)?;

        let EditorCard {
            size: card_size,
            source_origin,
        } = EditorCard::new([self.spec.width, self.spec.height]);
        let source = ui::card::RgbaSource::strided_region(
            &flat_pixels,
            [self.spec.width, self.spec.height],
            self.spec.width as usize * BYTES_PER_PIXEL as usize,
            source_origin,
            card_size,
        )?;
        if self.editor_background_pixels.is_empty() {
            let theme = self.theme;
            let mut background =
                vec![0_u8; self.spec.width as usize * self.spec.height as usize * 4];
            self.composite_ui(&mut background, |ui| {
                ui.paint(|canvas| {
                    let bounds = canvas.bounds();
                    canvas.fill(
                        bounds,
                        0.0,
                        ui::card::Fill::Solid({
                            let [r, g, b] = theme.background([4, 4, 5]);
                            ui::card::UiColor::srgb8(r, g, b, 255)
                        }),
                        1.0,
                    );
                    canvas.fill(
                        bounds,
                        0.0,
                        ui::card::Fill::Radial {
                            center: [bounds.size[0] * 0.5, bounds.size[1] * 0.46],
                            radius: bounds.size[0] * 0.62,
                            inner: ui::card::UiColor::srgb8(18, 18, 21, 150),
                            outer: ui::card::UiColor::srgb8(0, 0, 0, 0),
                        },
                        if theme == Theme::Original { 1.0 } else { 0. },
                    );
                    Ok(())
                })
            })?;
            self.editor_background_pixels = background;
        }
        let mut pixels = self.editor_background_pixels.clone();
        let destination_size = [card_size[0] as f32, card_size[1] as f32];
        let destination_center =
            EditorCard::center([self.spec.width, self.spec.height], frame.panel_offset());
        let mut card_style = ui::card::CardStyle::standard();
        if self.theme != Theme::Original {
            let [r, g, b] = self.theme.palette().surface;
            card_style.material = ui::card::Fill::Solid(ui::card::UiColor::srgb8(r, g, b, 255));
        }
        card_style.border_width = 0.75;
        card_style.border_color = ui::card::UiColor::srgb8(255, 255, 255, 10);
        self.composite_ui(&mut pixels, |ui| {
            ui.card(
                ui::card::CardFrame {
                    bounds: ui::Bounds::from_center(destination_center, destination_size),
                    style: card_style,
                    projection: frame.panel_projection(),
                    opacity: frame.panel_opacity.clamp(0.0, 1.0),
                },
                |card| {
                    card.content(|canvas| {
                        let bounds = canvas.bounds();
                        canvas.clipped(ui::card::Clip::rounded(bounds, 26.0), |canvas| {
                            canvas.rgba(bounds, source, ui::card::ContentFit::Contain, 1.0);
                            Ok(())
                        })
                    })?;
                    card.overlay(|canvas| {
                        let bounds = canvas.bounds().inset(ui::Edges::all(2.5));
                        canvas.stroke_fill(
                            bounds,
                            25.5,
                            1.0,
                            ui::card::Fill::Linear {
                                from: [0.0, bounds.origin[1]],
                                to: [0.0, bounds.bottom()],
                                start: ui::card::UiColor::srgb8(255, 255, 255, 38),
                                end: ui::card::UiColor::srgb8(255, 255, 255, 5),
                            },
                            1.0,
                        );
                        Ok(())
                    })
                },
            )
        })?;
        Ok(pixels)
    }

    /// Diff rows sit under the code: a tint across the card, an accent bar, and
    /// a vector +/- sign in the gutter. Consecutive rows join into one band.
    fn composite_line_marks(
        &self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
        [top, bottom]: [f32; 2],
    ) {
        let width = self.spec.width as f32;
        let (left, right) = (width * 0.11 + 12.0, width * 0.89 - 12.0);
        let sign_x = width * 0.145 - 30.0;
        let bands = frame
            .line_marks
            .iter()
            .filter_map(|mark| {
                let placed = frame
                    .lines
                    .iter()
                    .find(|line| line.line.id.as_str() == mark.line_id)?;
                let center = top + placed.y + LINE_HEIGHT * 0.5;
                let band = line_marks::Band {
                    top: (center - mark.row_height * 0.5).max(top),
                    bottom: (center + mark.row_height * 0.5).min(bottom),
                    opacity: mark.presence.clamp(0.0, 1.0) * placed.opacity.clamp(0.0, 1.0),
                    kind: usize::from(mark.mark == psychopomp::editor::LineMarkPlan::Removed),
                };
                (band.bottom > band.top && band.opacity > 0.001).then_some(band)
            })
            .collect::<Vec<_>>();
        let rows = line_marks::row_coverage(&bands, self.spec.height);
        let mut canvas = ui::card::UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
        for (y, row) in rows.iter().enumerate() {
            for (kind, opacity) in row.iter().enumerate() {
                if *opacity <= 0.001 {
                    continue;
                }
                let [r, g, b] = self.theme.tone(line_marks::TONES[kind]);
                let fill = ui::card::Fill::Solid(ui::card::UiColor::srgb8(r, g, b, 255));
                canvas.fill(
                    ui::Bounds {
                        origin: [left, y as f32],
                        size: [right - left, 1.0],
                    },
                    0.0,
                    fill,
                    opacity * 0.12,
                );
                canvas.fill(
                    ui::Bounds {
                        origin: [left, y as f32],
                        size: [3.0, 1.0],
                    },
                    0.0,
                    fill,
                    opacity * 0.85,
                );
            }
        }
        for mark in frame.line_marks {
            let Some(placed) = frame
                .lines
                .iter()
                .find(|line| line.line.id.as_str() == mark.line_id)
            else {
                continue;
            };
            let alpha = mark.presence.clamp(0.0, 1.0) * placed.opacity.clamp(0.0, 1.0);
            if alpha <= 0.001 {
                continue;
            }
            let center = top + placed.y + LINE_HEIGHT * 0.5;
            let band_top = (center - mark.row_height * 0.5).max(top);
            let band_bottom = (center + mark.row_height * 0.5).min(bottom);
            if band_bottom <= band_top {
                continue;
            }
            let color = self.theme.tone(match mark.mark {
                psychopomp::editor::LineMarkPlan::Added => psychopomp::tone::Tone::Success,
                psychopomp::editor::LineMarkPlan::Removed => psychopomp::tone::Tone::Error,
            });
            let half = 7.0;
            self.composite_prototype_path(
                pixels,
                &[[sign_x - half, center], [sign_x + half, center]],
                2.4,
                color,
                alpha,
            );
            if mark.mark == psychopomp::editor::LineMarkPlan::Added {
                self.composite_prototype_path(
                    pixels,
                    &[[sign_x, center - half], [sign_x, center + half]],
                    2.4,
                    color,
                    alpha,
                );
            }
        }
    }

    fn composite_editor_title(&self, pixels: &mut [u8], panel_offset_y: f32) {
        let panel_top = self.spec.height as f32 * 0.17 + panel_offset_y;
        composite_sprite(
            pixels,
            self.spec.width,
            self.spec.height,
            &self.title_sprite,
            (self.spec.width as f32 * 0.135).round() as i32,
            (panel_top + 16.0).round() as i32,
            1.0,
        );
    }

    fn composite_text_untransformed(
        &mut self,
        pixels: &mut [u8],
        frame: &EditorFrame<'_>,
    ) -> Result<()> {
        for line in frame.lines.iter().map(|placed| placed.line) {
            refresh_sprite(
                &mut self.line_sprites,
                &line.id,
                line_fingerprint(line),
                || {
                    let mut sprite =
                        make_line_sprite(&mut self.font_system, &mut self.swash_cache, line);
                    self.theme.sprite(&mut sprite);
                    sprite
                },
            );
        }

        let panel_top = self.spec.height as f32 * 0.17 + frame.panel_offset_y;
        let code_top = panel_top + 104.0;
        let code_bottom = panel_top + self.spec.height as f32 * 0.70;
        let code_right = self.spec.width as f32 * 0.89 - 32.0;
        self.composite_line_marks(pixels, frame, [code_top, code_bottom]);
        let area = ide::CodeArea {
            origin: psychopomp::math::vec2(self.spec.width as f32 * 0.145, code_top),
            clip_y: [code_top, code_bottom],
        };
        self.composite_selections(pixels, frame, area);
        for placed in frame.lines {
            if placed.opacity <= 0.001 {
                continue;
            }
            let line_x = self.spec.width as f32 * 0.145 + placed.x;
            let line_y = code_top + placed.y;
            if line_y + LINE_HEIGHT <= code_top || line_y >= code_bottom {
                continue;
            }
            if frame
                .inline_reveals
                .iter()
                .any(|reveal| placed.line.id.as_str() == reveal.line_id)
            {
                let reveals = frame
                    .inline_reveals
                    .iter()
                    .filter(|reveal| placed.line.id.as_str() == reveal.line_id)
                    .copied()
                    .collect::<Vec<_>>();
                self.composite_inline_reveals(
                    pixels,
                    placed,
                    &reveals,
                    frame.annotations.inlays,
                    line_x,
                    line_y,
                    code_right,
                    [code_top, code_bottom],
                )?;
                continue;
            }
            let sprite = self
                .line_sprites
                .get(&placed.line.id)
                .map(|(_, sprite)| sprite)
                .expect("line sprite was populated above");
            let line_blur = placed.blur;
            let clip_width = sprite.advance.min((code_right - line_x).max(0.0));
            composite_text(
                pixels,
                [self.spec.width, self.spec.height],
                TextDraw {
                    clip_width,
                    filter: TextFilter::Blur(line_blur),
                    opacity: placed.opacity,
                    clip_y: Some([code_top, code_bottom]),
                    ..TextDraw::new(sprite, [line_x, line_y])
                },
            );
        }
        self.composite_annotations(pixels, frame, area, panel_top);
        composite_sprite_rotated(
            pixels,
            self.spec.width,
            self.spec.height,
            &self.pointer_sprite,
            36.0 * frame.pointer.scale,
            36.0 * frame.pointer.scale,
            self.spec.width as f32 * 0.145 + frame.pointer.x,
            code_top + frame.pointer.y,
            frame.pointer.rotation,
            frame.pointer.blur,
            frame.pointer.opacity,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn composite_inline_reveals(
        &mut self,
        pixels: &mut [u8],
        placed: &PlacedLine<'_>,
        reveals: &[InlineRevealFrame],
        inlays: &[InlayFrame],
        x: f32,
        y: f32,
        right: f32,
        clip_y: [f32; 2],
    ) -> Result<()> {
        let segments = inline_reveal_segments(placed.line.spans().len(), reveals)?;

        for (start, end, _) in &segments {
            let spans = &placed.line.spans()[*start..*end];
            let key = format!("{}:{start}:{end}", placed.line.id.as_str());
            refresh_sprite(
                &mut self.part_sprites,
                &*key,
                spans_fingerprint(spans),
                || {
                    let mut sprite =
                        make_spans_sprite(&mut self.font_system, &mut self.swash_cache, spans);
                    self.theme.sprite(&mut sprite);
                    sprite
                },
            );
        }

        let mut cursor_x = x;
        let line_blur = placed.blur;
        for (start, end, progress) in segments {
            let key = format!("{}:{start}:{end}", placed.line.id.as_str());
            let sprite = &self.part_sprites[&key].1;
            let available = (right - cursor_x).max(0.0);
            let progress = progress.unwrap_or(1.0).clamp(0.0, 1.0);
            let width = sprite.advance * progress;
            if inlays.iter().any(|inlay| {
                inlay.line_id == placed.line.id.as_str()
                    && (inlay.start_span, inlay.end_span) == (start, end)
            }) {
                self.composite_inlay_chip(
                    pixels,
                    psychopomp::math::vec2(cursor_x, y),
                    width.min(available),
                    placed.opacity * progress,
                    clip_y,
                );
            }
            composite_text(
                pixels,
                [self.spec.width, self.spec.height],
                TextDraw {
                    clip_width: width.min(available),
                    filter: TextFilter::Blur(((1.0 - progress) * 4.0).max(line_blur)),
                    opacity: placed.opacity * progress,
                    clip_y: Some(clip_y),
                    ..TextDraw::new(sprite, [cursor_x, y])
                },
            );
            cursor_x += width;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn measure_text_range(&mut self, line: &CodeLine, text: &str) -> Result<TextRangeBounds> {
        let full_text: String = line.spans().iter().map(|span| span.text.as_str()).collect();
        let start = full_text
            .find(text)
            .with_context(|| format!("line '{}' does not contain '{text}'", line.id.as_str()))?;
        let end = start + text.len();
        self.measure_text_byte_range(line, start, end)
    }

    pub(super) fn measure_inline_target(
        &mut self,
        line: &CodeLine,
        reveals: &[InlineRevealFrame<'_>],
        selected: std::ops::Range<usize>,
    ) -> Result<Vec<InlineRangeMetrics>> {
        inline_reveal_segments(line.spans().len(), reveals)?
            .into_iter()
            .map(|(start, end, _)| {
                let spans = &line.spans()[start..end];
                let from = start.max(selected.start);
                let to = end.min(selected.end);
                let selection = if from < to {
                    let start_byte = line.spans()[start..from]
                        .iter()
                        .map(|span| span.text.len())
                        .sum();
                    let end_byte = line.spans()[start..to]
                        .iter()
                        .map(|span| span.text.len())
                        .sum();
                    Some(start_byte..end_byte)
                } else {
                    None
                };
                let (advance, selection) =
                    measure_code_spans(&mut self.font_system, spans, selection)?;
                Ok(InlineRangeMetrics {
                    spans: start..end,
                    advance,
                    selection: selection.map(|bounds| [bounds.x, bounds.x + bounds.width]),
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub fn measure_text_byte_range(
        &mut self,
        line: &CodeLine,
        start: usize,
        end: usize,
    ) -> Result<TextRangeBounds> {
        let (_, bounds) =
            measure_code_spans(&mut self.font_system, line.spans(), Some(start..end))?;
        Ok(bounds.expect("requested a code text range"))
    }
}

/// Code geometry only: one shaped partition supplies both its advance and the
/// optional glyph-cluster bounds. Range validation must precede shaping.
fn measure_code_spans(
    font_system: &mut FontSystem,
    spans: &[StyledSpan],
    selection: Option<std::ops::Range<usize>>,
) -> Result<(f32, Option<TextRangeBounds>)> {
    if let Some(range) = &selection {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        if range.start >= range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            bail!("text byte range is outside the code line");
        }
    }
    let base = Attrs::new().family(fonts::mono());
    let spans: Vec<_> = spans
        .iter()
        .map(|span| (span.text.as_str(), attributes(base.clone(), span.style)))
        .collect();
    let mut buffer = Buffer::new(font_system, Metrics::new(28.0, LINE_HEIGHT));
    buffer.set_size(Some(1320.0), Some(LINE_HEIGHT));
    buffer.set_wrap(Wrap::None);
    buffer.set_rich_text(spans, &base, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    let run = buffer.layout_runs().next();
    let advance = run.as_ref().map_or(0.0, |run| run.line_w);
    let Some(range) = selection else {
        return Ok((advance, None));
    };
    let run = run.context("shaped code line has no layout run")?;
    let mut selected = run
        .glyphs
        .iter()
        .filter(|glyph| glyph.end > range.start && glyph.start < range.end);
    let first = selected.next().context("text range has no shaped glyphs")?;
    let mut left = first.x;
    let mut right = first.x + first.w;
    for glyph in selected {
        left = left.min(glyph.x);
        right = right.max(glyph.x + glyph.w);
    }
    Ok((
        advance,
        Some(TextRangeBounds {
            x: left,
            width: right - left,
        }),
    ))
}

type InlineSegment = (usize, usize, Option<f32>);

fn inline_reveal_segments(
    span_count: usize,
    reveals: &[InlineRevealFrame],
) -> Result<Vec<InlineSegment>> {
    let mut reveals = reveals.to_vec();
    reveals.sort_by_key(|reveal| reveal.start_span);
    let mut segments = Vec::with_capacity(reveals.len() * 2 + 1);
    let mut cursor = 0;
    for reveal in reveals {
        if reveal.start_span >= reveal.end_span || reveal.end_span > span_count {
            bail!("inline reveal span range is outside the code line");
        }
        if reveal.start_span < cursor {
            bail!("inline reveal span ranges overlap on the same code line");
        }
        if cursor < reveal.start_span {
            segments.push((cursor, reveal.start_span, None));
        }
        segments.push((reveal.start_span, reveal.end_span, Some(reveal.progress)));
        cursor = reveal.end_span;
    }
    if cursor < span_count {
        segments.push((cursor, span_count, None));
    }
    Ok(segments)
}

fn make_pointer_sprite() -> Result<TextSprite> {
    const SIZE: u32 = 36 * 4;
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 256 256">
      <path fill="#f4f7f5" d="M224,104v50.93c0,46.2-36.85,84.55-83,85.06A83.71,83.71,0,0,1,80.6,215.4C58.79,192.33,34.15,136,34.15,136a16,16,0,0,1,6.53-22.23c7.66-4,17.1-.84,21.4,6.62l21,36.44a6.09,6.09,0,0,0,6,3.09l.12,0A8.19,8.19,0,0,0,96,151.74V32a16,16,0,0,1,16.77-16c8.61.4,15.23,7.82,15.23,16.43V104a8,8,0,0,0,8.53,8,8.17,8.17,0,0,0,7.47-8.25V88a16,16,0,0,1,16.77-16c8.61.4,15.23,7.82,15.23,16.43V112a8,8,0,0,0,8.53,8,8.17,8.17,0,0,0,7.47-8.25v-7.28c0-8.61,6.62-16,15.23-16.43A16,16,0,0,1,224,104Z"/>
    </svg>"##;

    rasterize_svg(SVG, SIZE, SIZE).context("rasterize Phosphor hand pointer")
}

/// Uploads single-channel glyph coverage as a sampled texture.
fn upload_r8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    [width, height]: [u32; 2],
    pixels: &[u8],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    texture.create_view(&Default::default())
}

fn rasterize_svg(svg: &str, width: u32, height: u32) -> Result<TextSprite> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .context("parse SVG sprite")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).context("allocate SVG sprite")?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / tree.size().width(),
        height as f32 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut pixels = pixmap.data().to_vec();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha > 0 {
            for channel in &mut pixel[..3] {
                *channel = u32::from(*channel)
                    .saturating_mul(255)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
    }
    Ok(TextSprite {
        width,
        height,
        advance: width as f32,
        pixels,
    })
}

fn make_title_sprite(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    file_name: &str,
) -> TextSprite {
    let attrs = Attrs::new()
        .family(fonts::mono())
        .weight(Weight::NORMAL)
        .color(Color::rgb(161, 161, 170));
    make_sprite(
        font_system,
        swash_cache,
        vec![(file_name, attrs.clone())],
        attrs,
        Metrics::new(20.0, 28.0),
        320,
        40,
    )
}

/// The cached sprite for `key`, rebuilt by `make` only when `fingerprint` changed.
fn refresh_sprite<'a, K, Q>(
    cache: &'a mut HashMap<K, (u64, TextSprite)>,
    key: &Q,
    fingerprint: u64,
    make: impl FnOnce() -> TextSprite,
) -> &'a TextSprite
where
    K: std::borrow::Borrow<Q> + Eq + Hash,
    Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
{
    if cache
        .get(key)
        .is_none_or(|(cached, _)| *cached != fingerprint)
    {
        cache.insert(key.to_owned(), (fingerprint, make()));
    }
    &cache[key].1
}

fn line_fingerprint(line: &CodeLine) -> u64 {
    spans_fingerprint(line.spans())
}

fn spans_fingerprint(spans: &[StyledSpan]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for span in spans {
        span.text.hash(&mut hasher);
        span.style.hash(&mut hasher);
    }
    hasher.finish()
}

fn can_preview_editor(frame: &EditorFrame<'_>, [width, height]: [u32; 2]) -> bool {
    let width = width as f32;
    let height = height as f32;
    if frame.panel_offset_y != 0.0
        || frame.panel_offset_x != 0.0
        || frame.panel_opacity < 1.0
        || frame.panel_rotation != 0.0
        || frame.panel_tilt_x != 0.0
        || frame.panel_tilt_y != 0.0
        || frame.panel_scale != 1.0
        || frame.panel_near_blur != 0.0
        || frame.pointer.opacity > 0.001
        || frame.lines.iter().any(|line| line.x < 0.0)
    {
        return false;
    }
    // Overlays outside the flat code body need the full rounded-card clip and
    // chrome draw order. Keep the cheap pass only where the two cannot overlap.
    let body_top = height * 0.17 + 64.0;
    let body_bottom = height * 0.87 - 28.0;
    if frame.focus_intensity > 0.0 {
        let top = height * 0.17 + 104.0 + frame.focus_line_y + LINE_HEIGHT * 0.5
            - frame.focus_height * 0.5
            - 2.0;
        if top < body_top || top + frame.focus_height + 4.0 > body_bottom {
            return false;
        }
    }
    if frame.token_highlight.opacity > 0.0 {
        let highlight = frame.token_highlight;
        let left = width * 0.145 + highlight.x - 15.0;
        let top = height * 0.17 + 104.0 + highlight.y - 5.0;
        if left < width * 0.11 + 28.0
            || left + highlight.width + 30.0 > width * 0.89 - 28.0
            || top < body_top
            || top + 54.0 > body_bottom
        {
            return false;
        }
    }
    true
}

fn make_line_sprite(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    line: &CodeLine,
) -> TextSprite {
    make_spans_sprite(font_system, swash_cache, line.spans())
}

fn make_spans_sprite(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    line_spans: &[StyledSpan],
) -> TextSprite {
    make_spans_sprite_at_size(font_system, swash_cache, line_spans, 28.0, LINE_HEIGHT)
}

fn make_spans_sprite_at_size(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    line_spans: &[StyledSpan],
    font_size: f32,
    line_height: f32,
) -> TextSprite {
    let base = Attrs::new().family(fonts::mono());
    let spans: Vec<_> = if line_spans.is_empty() {
        vec![(" ", attributes(base.clone(), SyntaxStyle::Plain))]
    } else {
        line_spans
            .iter()
            .map(|span| (span.text.as_str(), attributes(base.clone(), span.style)))
            .collect()
    };
    make_sprite(
        font_system,
        swash_cache,
        spans,
        base,
        Metrics::new(font_size, line_height),
        1320,
        line_height.ceil() as u32,
    )
}

fn composite_sprite(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    sprite: &TextSprite,
    x: i32,
    y: i32,
    opacity: f32,
) {
    if opacity <= 0.001 || sprite.width == 0 {
        return;
    }
    // Preserve the former full-width float conversion, including rounding and
    // saturation for large u32 widths rather than a wrapping integer cast.
    let visible_width = sprite.width as f32 as i32;
    for sprite_y in 0..sprite.height as i32 {
        let target_y = y + sprite_y;
        if !(0..canvas_height as i32).contains(&target_y) {
            continue;
        }
        for sprite_x in 0..visible_width {
            let target_x = x + sprite_x;
            if !(0..canvas_width as i32).contains(&target_x) {
                continue;
            }
            let source_index = (sprite_y as usize * sprite.width as usize + sprite_x as usize) * 4;
            if sprite.pixels[source_index + 3] == 0 {
                continue;
            }
            let target_index = (target_y as usize * canvas_width as usize + target_x as usize) * 4;
            blend_pixel(
                &mut canvas[target_index..target_index + 4],
                sprite.pixels[source_index..source_index + 4]
                    .try_into()
                    .expect("RGBA pixel has four channels"),
                opacity,
            );
        }
    }
}

/// One text sprite drawn onto a canvas. `TextDraw::new` draws the whole sprite
/// sharply and opaquely; override fields to reveal, fade, blur, or clip it.
#[derive(Clone, Copy)]
struct TextDraw<'a> {
    sprite: &'a TextSprite,
    /// Where sprite column `source_left` and the sprite's top land.
    origin: [f32; 2],
    source_left: f32,
    /// Width of the drawn sprite window, from `source_left`.
    clip_width: f32,
    filter: TextFilter,
    opacity: f32,
    /// Canvas rows that may receive ink; the whole canvas when `None`.
    clip_y: Option<[f32; 2]>,
    mask: Option<VerticalMask>,
}

impl<'a> TextDraw<'a> {
    fn new(sprite: &'a TextSprite, origin: [f32; 2]) -> Self {
        Self {
            sprite,
            origin,
            source_left: 0.0,
            clip_width: sprite.width as f32,
            filter: TextFilter::Blur(0.0),
            opacity: 1.0,
            clip_y: None,
            mask: None,
        }
    }
}

/// How a text sprite is resampled: a soft radial blur, or a vertical smear
/// that crossfades a sharp copy into a Gaussian streak (a Rolling Number's
/// fast wheel).
#[derive(Clone, Copy)]
enum TextFilter {
    Blur(f32),
    Smear { sigma: f32, amount: f32 },
}

impl TextFilter {
    /// Vertical taps out to three sigma, at most a pixel apart so thin
    /// strokes streak rather than repeat; the sharp copy keeps `1 - amount`.
    fn smear_taps(sigma: f32, amount: f32) -> Vec<(f32, f32)> {
        let steps = (3.0 * sigma).ceil().max(1.0) as i32;
        let gaussian = (-steps..=steps)
            .map(|step| {
                let offset = step as f32 * 3.0 * sigma / steps as f32;
                (offset, (-0.5 * (offset / sigma).powi(2)).exp())
            })
            .collect::<Vec<_>>();
        let total: f32 = gaussian.iter().map(|(_, weight)| weight).sum();
        gaussian
            .into_iter()
            .map(|(offset, weight)| {
                let sharp = if offset == 0.0 { 1.0 - amount } else { 0.0 };
                (offset, amount * weight / total + sharp)
            })
            .collect()
    }

    /// Extra source support, in pixels, along x and y.
    fn reach(self) -> [f32; 2] {
        match self {
            Self::Blur(blur) => [blur, blur],
            Self::Smear { sigma, .. } => [0.0, 3.0 * sigma],
        }
    }
}

fn composite_text(canvas: &mut [u8], [canvas_width, canvas_height]: [u32; 2], draw: TextDraw) {
    let TextDraw {
        sprite,
        origin: [x, y],
        source_left,
        clip_width,
        filter,
        opacity,
        clip_y,
        mask,
    } = draw;
    let clip_y = clip_y.unwrap_or([0.0, canvas_height as f32]);
    if opacity <= 0.0 || clip_width <= 0.0 {
        return;
    }
    let [reach_x, reach_y] = filter.reach();
    let smear = match filter {
        TextFilter::Smear { sigma, amount } if sigma > 0.0 && amount > 0.0 => {
            TextFilter::smear_taps(sigma, amount)
        }
        _ => Vec::new(),
    };
    let source_clip = [
        source_left,
        (source_left + clip_width).min(sprite.width as f32),
    ];
    let clip_y = mask.map_or(clip_y, |mask| {
        [clip_y[0].max(mask.top), clip_y[1].min(mask.bottom)]
    });
    // The source mask is filtered with the glyph. Include its complete support;
    // otherwise changing floor/ceil bounds would discard nonzero filtered texels.
    let left = x - source_left + source_clip[0].floor() - reach_x - 1.0;
    let right = x - source_left + source_clip[1].ceil() + reach_x + 1.0;
    let top = (y - reach_y - 1.0).max(clip_y[0]);
    let bottom = (y + sprite.height as f32 + reach_y + 1.0).min(clip_y[1]);
    for target_y in (top.floor() as i32).max(0)..(bottom.ceil() as i32).min(canvas_height as i32) {
        let row_start = (target_y as f32).max(clip_y[0]);
        let row_end = (target_y as f32 + 1.).min(clip_y[1]);
        let coverage_y = mask.map_or_else(
            || (row_end - row_start).clamp(0., 1.),
            |mask| mask.coverage(row_start, row_end),
        );
        if coverage_y <= 0. {
            continue;
        }
        for target_x in (left.floor() as i32).max(0)..(right.ceil() as i32).min(canvas_width as i32)
        {
            let source_x = source_left + target_x as f32 - x;
            let source_y = target_y as f32 - y;
            let mut color = [0.0; 4];
            match filter {
                TextFilter::Blur(blur) if blur > 0.0 => {
                    // Bilinear sample locations vary continuously with blur; no
                    // rounded taps that suddenly turn a sharp glyph into a 3x3 copy.
                    for (dy, wy) in [(-1.0, 0.25), (0.0, 0.5), (1.0, 0.25)] {
                        for (dx, wx) in [(-1.0, 0.25), (0.0, 0.5), (1.0, 0.25)] {
                            let sample = sample_text_sprite(
                                sprite,
                                source_x + dx * blur,
                                source_y + dy * blur,
                                source_clip,
                            );
                            for channel in 0..4 {
                                color[channel] += sample[channel] * wx * wy;
                            }
                        }
                    }
                }
                TextFilter::Smear { .. } if !smear.is_empty() => {
                    for &(offset, weight) in &smear {
                        let sample =
                            sample_text_sprite(sprite, source_x, source_y + offset, source_clip);
                        for channel in 0..4 {
                            color[channel] += sample[channel] * weight;
                        }
                    }
                }
                _ => color = sample_text_sprite(sprite, source_x, source_y, source_clip),
            }
            if color[3] <= 0.0 {
                continue;
            }
            let source = [
                (color[0] / color[3]).round() as u8,
                (color[1] / color[3]).round() as u8,
                (color[2] / color[3]).round() as u8,
                (color[3] * coverage_y).round() as u8,
            ];
            let target_index = (target_y as usize * canvas_width as usize + target_x as usize) * 4;
            blend_pixel(&mut canvas[target_index..target_index + 4], source, opacity);
        }
    }
}

/// Premultiplied RGBA interpolation avoids dark fringes around transparent glyphs.
fn sample_text_sprite(sprite: &TextSprite, x: f32, y: f32, clip: [f32; 2]) -> [f32; 4] {
    let left = x.floor() as i32;
    let top = y.floor() as i32;
    let dx = x - left as f32;
    let dy = y - top as f32;
    let mut color = [0.0; 4];
    for (row, wy) in [(top, 1.0 - dy), (top + 1, dy)] {
        for (column, wx) in [(left, 1.0 - dx), (left + 1, dx)] {
            let weight = wx * wy;
            if weight == 0.0
                || column < 0
                || row < 0
                || column >= sprite.width as i32
                || row >= sprite.height as i32
            {
                continue;
            }
            let offset = (row as usize * sprite.width as usize + column as usize) * 4;
            // Apply intentional source-range clipping once, before filtering.
            // Transparent texture borders already provide raster-edge coverage.
            let coverage =
                ((column as f32 + 1.0).min(clip[1]) - (column as f32).max(clip[0])).clamp(0.0, 1.0);
            let alpha = f32::from(sprite.pixels[offset + 3]) * weight * coverage;
            for (channel, value) in color[..3].iter_mut().enumerate() {
                *value += f32::from(sprite.pixels[offset + channel]) * alpha;
            }
            color[3] += alpha;
        }
    }
    color
}

#[allow(clippy::too_many_arguments)]
fn composite_sprite_rotated(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    sprite: &TextSprite,
    display_width: f32,
    display_height: f32,
    center_x: f32,
    center_y: f32,
    rotation: f32,
    blur: f32,
    opacity: f32,
) {
    composite_sprite_rotated_with_coverage(
        canvas,
        canvas_width,
        canvas_height,
        sprite,
        display_width,
        display_height,
        center_x,
        center_y,
        rotation,
        blur,
        opacity,
        |_, _| 1.0,
    );
}

#[allow(clippy::too_many_arguments)]
fn composite_sprite_rotated_with_coverage(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    sprite: &TextSprite,
    display_width: f32,
    display_height: f32,
    center_x: f32,
    center_y: f32,
    rotation: f32,
    blur: f32,
    opacity: f32,
    coverage_at: impl Fn(f32, f32) -> f32,
) {
    if opacity <= 0.001 {
        return;
    }
    if display_width <= 0.0 || display_height <= 0.0 {
        return;
    }
    let source_scale = [
        sprite.width as f32 / display_width,
        sprite.height as f32 / display_height,
    ];
    let filter_radius = [
        blur * source_scale[0] + (source_scale[0] - 1.).max(0.) * 0.5,
        blur * source_scale[1] + (source_scale[1] - 1.).max(0.) * 0.5,
    ];
    let radius = (display_width * display_width + display_height * display_height)
        .sqrt()
        .mul_add(
            0.5,
            blur + (1.0 / source_scale[0]).max(1.0 / source_scale[1]) + 1.0,
        )
        .ceil() as i32;
    let sine = rotation.sin();
    let cosine = rotation.cos();
    let center_pixel_x = center_x.round() as i32;
    let center_pixel_y = center_y.round() as i32;

    for target_y in center_pixel_y.saturating_sub(radius).max(0)
        ..=center_pixel_y
            .saturating_add(radius)
            .min(canvas_height as i32 - 1)
    {
        for target_x in center_pixel_x.saturating_sub(radius).max(0)
            ..=center_pixel_x
                .saturating_add(radius)
                .min(canvas_width as i32 - 1)
        {
            let coverage = coverage_at(target_x as f32 + 0.5, target_y as f32 + 0.5);
            if coverage <= 0.0 {
                continue;
            }
            let dx = target_x as f32 + 0.5 - center_x;
            let dy = target_y as f32 + 0.5 - center_y;
            let source_x =
                ((dx * cosine + dy * sine) / display_width + 0.5) * sprite.width as f32 - 0.5;
            let source_y =
                ((-dx * sine + dy * cosine) / display_height + 0.5) * sprite.height as f32 - 0.5;
            let mut color = [0.; 4];
            if filter_radius == [0., 0.] {
                color = sample_text_sprite(sprite, source_x, source_y, [0., sprite.width as f32]);
            } else {
                // Denser binomial taps soften transformed content instead of
                // showing three displaced copies of a blurred letter/border.
                // Fixed support and continuously varying offsets avoid kernel
                // changes at integer blur radii.
                const TAPS: [(f32, f32); 5] = [
                    (-1., 0.0625),
                    (-0.5, 0.25),
                    (0., 0.375),
                    (0.5, 0.25),
                    (1., 0.0625),
                ];
                for (y, wy) in TAPS {
                    for (x, wx) in TAPS {
                        let sample = sample_text_sprite(
                            sprite,
                            source_x + x * filter_radius[0],
                            source_y + y * filter_radius[1],
                            [0., sprite.width as f32],
                        );
                        for channel in 0..4 {
                            color[channel] += sample[channel] * wx * wy;
                        }
                    }
                }
            }
            if color[3] <= 0.0 {
                continue;
            }
            let source = [
                (color[0] / color[3]).round() as u8,
                (color[1] / color[3]).round() as u8,
                (color[2] / color[3]).round() as u8,
                color[3].round() as u8,
            ];
            let target_index = (target_y as usize * canvas_width as usize + target_x as usize) * 4;
            blend_pixel(
                &mut canvas[target_index..target_index + 4],
                source,
                opacity * coverage,
            );
        }
    }
}

fn attributes(base: Attrs<'static>, style: SyntaxStyle) -> Attrs<'static> {
    match style {
        SyntaxStyle::Plain => base.color(Color::rgb(228, 228, 231)),
        SyntaxStyle::Keyword => base.color(Color::rgb(196, 181, 253)),
        SyntaxStyle::Type => base.color(Color::rgb(125, 211, 252)),
        SyntaxStyle::String => base.color(Color::rgb(190, 242, 100)),
        SyntaxStyle::Accent => base.color(Color::rgb(110, 231, 183)),
        SyntaxStyle::Rgb(red, green, blue) => base.color(Color::rgb(red, green, blue)),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn editor_canvas_points_follow_the_panel_projection() {
        use super::{EditorPanel, editor_canvas_point};
        let canvas = [1920, 1080];
        let rest = EditorPanel {
            offset: [0.0; 2],
            scale: 1.0,
            rotation: 0.0,
            tilt: [0.0; 2],
        };
        // At rest the card shows the flat editor: code x starts at 14.5% of
        // the width and rows 104 px below the panel top. The compositor's
        // whole-pixel source cut lifts the content 0.4 px; the point follows.
        let point = editor_canvas_point(canvas, rest, [100.0, 22.0]);
        assert!((point[0] - (278.4 + 100.0)).abs() < 1e-3);
        assert!((point[1] - (183.6 + 104.0 + 22.0 - 0.4)).abs() < 1e-3);
        // Scale grows about the card center; offsets translate it.
        let center = [960.0, 1080.0 * 0.52];
        let zoomed = editor_canvas_point(
            canvas,
            EditorPanel {
                offset: [-90.0, 10.0],
                scale: 2.0,
                ..rest
            },
            [100.0, 22.0],
        );
        for axis in 0..2 {
            let expected = center[axis] + [-90.0, 10.0][axis] + (point[axis] - center[axis]) * 2.0;
            assert!((zoomed[axis] - expected).abs() < 1e-2, "{zoomed:?}");
        }
    }

    mod code_measurement {
        use super::super::*;
        use std::ops::Range;

        // The former byte-range measurement, independent of measure_code_spans.
        // Keep its fixed code policy and first-run cluster selection as an oracle.
        fn reference_bounds(
            fonts: &mut FontSystem,
            spans: &[StyledSpan],
            range: Range<usize>,
        ) -> Result<TextRangeBounds> {
            let text: String = spans.iter().map(|span| span.text.as_str()).collect();
            if range.start >= range.end
                || range.end > text.len()
                || !text.is_char_boundary(range.start)
                || !text.is_char_boundary(range.end)
            {
                bail!("text byte range is outside the code line");
            }
            let base = Attrs::new().family(fonts::mono());
            let spans = spans
                .iter()
                .map(|span| (span.text.as_str(), attributes(base.clone(), span.style)))
                .collect::<Vec<_>>();
            let mut buffer = Buffer::new(fonts, Metrics::new(28.0, 44.0));
            buffer.set_size(Some(1320.0), Some(44.0));
            buffer.set_wrap(Wrap::None);
            buffer.set_rich_text(spans, &base, Shaping::Advanced, None);
            buffer.shape_until_scroll(fonts, false);
            let run = buffer
                .layout_runs()
                .next()
                .context("shaped code line has no layout run")?;
            let mut glyphs = run
                .glyphs
                .iter()
                .filter(|glyph| glyph.end > range.start && glyph.start < range.end);
            let first = glyphs.next().context("text range has no shaped glyphs")?;
            let mut left = first.x;
            let mut right = first.x + first.w;
            for glyph in glyphs {
                left = left.min(glyph.x);
                right = right.max(glyph.x + glyph.w);
            }
            Ok(TextRangeBounds {
                x: left,
                width: right - left,
            })
        }

        fn bounds_bits(bounds: TextRangeBounds) -> [u32; 2] {
            [bounds.x.to_bits(), bounds.width.to_bits()]
        }

        #[test]
        fn shaping_matches_raster_advance_and_independent_range_reference() {
            let mut fonts = fonts::font_system();
            let mut swash = SwashCache::new();
            let mut cases = [
                "",
                " \t  ",
                "office ffi fi",
                "e\u{301} + café",
                "naïve 🦀 東京",
                "first\nsecond",
                "\n",
                "first\r\nsecond",
            ]
            .map(|text| vec![StyledSpan::new(text, SyntaxStyle::Plain)])
            .to_vec();
            cases.push(vec![StyledSpan::new(
                "long code ".repeat(200),
                SyntaxStyle::Plain,
            )]);
            cases.push(vec![
                StyledSpan::new("const ", SyntaxStyle::Keyword),
                StyledSpan::new("", SyntaxStyle::Accent),
                StyledSpan::new("café", SyntaxStyle::Plain),
                StyledSpan::new(": Effect", SyntaxStyle::Type),
                StyledSpan::new(" = ", SyntaxStyle::Rgb(250, 125, 64)),
                StyledSpan::new("\"e\u{301}\"", SyntaxStyle::String),
            ]);
            for spans in cases {
                let raster = make_spans_sprite(&mut fonts, &mut swash, &spans);
                let (advance, unselected) = measure_code_spans(&mut fonts, &spans, None).unwrap();
                assert_eq!(advance.to_bits(), raster.advance.to_bits());
                assert!(unselected.is_none());
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                if text.is_empty() {
                    assert_eq!(advance, 0., "an empty-text partition occupies no width");
                }
                let boundaries = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain(std::iter::once(text.len()))
                    .collect::<Vec<_>>();
                let ranges = boundaries
                    .windows(2)
                    .take(24)
                    .map(|pair| pair[0]..pair[1])
                    .chain([
                        0..text.len(),
                        boundaries[boundaries.len() / 2]..text.len(),
                        boundaries[boundaries.len().saturating_sub(2)]..text.len(),
                    ]);
                for range in ranges {
                    let expected = reference_bounds(&mut fonts, &spans, range.clone())
                        .map(bounds_bits)
                        .map_err(|e| e.to_string());
                    let actual = measure_code_spans(&mut fonts, &spans, Some(range.clone()))
                        .map(|(advance, bounds)| {
                            assert_eq!(advance.to_bits(), raster.advance.to_bits());
                            bounds_bits(bounds.unwrap())
                        })
                        .map_err(|e| e.to_string());
                    assert_eq!(actual, expected, "range {range:?} of {text:?}");
                }
            }
            let combined = [StyledSpan::new("e\u{301}", SyntaxStyle::Plain)];
            let base = measure_code_spans(&mut fonts, &combined, Some(0..1)).unwrap();
            let mark = measure_code_spans(&mut fonts, &combined, Some(1..3)).unwrap();
            assert_eq!(bounds_bits(base.1.unwrap()), bounds_bits(mark.1.unwrap()));
        }

        #[test]
        fn invalid_byte_ranges_keep_the_validation_error() {
            let mut fonts = fonts::font_system();
            for (text, start, end) in [
                ("", 0, 0),
                ("", 0, 1),
                ("x", 1, 0),
                ("x", 1, 1),
                ("x", 0, 2),
                ("x", usize::MAX, usize::MAX),
                ("é", 0, 1),
                ("é", 1, 2),
                ("e\u{301}", 0, 2),
            ] {
                let spans = [StyledSpan::new(text, SyntaxStyle::Plain)];
                let actual = measure_code_spans(&mut fonts, &spans, Some(start..end))
                    .expect_err("invalid range must fail before shaping");
                assert_eq!(
                    actual.to_string(),
                    "text byte range is outside the code line"
                );
                assert_eq!(
                    actual.to_string(),
                    reference_bounds(&mut fonts, &spans, start..end)
                        .err()
                        .unwrap()
                        .to_string()
                );
            }
            assert!(measure_code_spans(&mut fonts, &[], Some(0..1)).is_err());
        }

        #[test]
        #[ignore = "requires a headless GPU; exercises public measurement and inline partition dispatch"]
        fn public_measurement_preserves_partition_metrics_and_errors() {
            let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
                width: 1920,
                height: 1080,
                file_name: "code-measurement-proof".into(),
            }))
            .unwrap();
            let line = CodeLine::new(
                "line",
                ["const ", "", "x", " = ", "e\u{301}", " + ", "雪"]
                    .map(|text| StyledSpan::new(text, SyntaxStyle::Plain))
                    .to_vec(),
            );
            let reveals = [(4, 6), (1, 2)].map(|(start_span, end_span)| InlineRevealFrame {
                line_id: "line",
                start_span,
                end_span,
                progress: 0.37,
            });
            let metrics = renderer
                .measure_inline_target(&line, &reveals, 2..5)
                .unwrap();
            let expected = [
                (0..1, None),
                (1..2, None),
                (2..4, Some(0..4)),
                (4..6, Some(0..3)),
                (6..7, None),
            ];
            assert_eq!(metrics.len(), expected.len());
            for (metric, (range, selected)) in metrics.iter().zip(expected) {
                let spans = &line.spans()[range.clone()];
                assert_eq!(metric.spans, range);
                let raster =
                    make_spans_sprite(&mut renderer.font_system, &mut renderer.swash_cache, spans);
                assert_eq!(metric.advance.to_bits(), raster.advance.to_bits());
                let bounds = selected.map(|range| {
                    let b = reference_bounds(&mut renderer.font_system, spans, range).unwrap();
                    [b.x.to_bits(), (b.x + b.width).to_bits()]
                });
                assert_eq!(metric.selection.map(|b| b.map(f32::to_bits)), bounds);
            }
            assert_eq!(metrics[1].advance, 0.);
            let error = renderer
                .measure_inline_target(&line, &reveals, 1..2)
                .err()
                .unwrap();
            assert_eq!(
                error.to_string(),
                "text byte range is outside the code line"
            );
            for range in [0..1, 0..18, 10..12, 10..11, 1..1, 0..100] {
                let expected =
                    reference_bounds(&mut renderer.font_system, line.spans(), range.clone())
                        .map(bounds_bits)
                        .map_err(|e| e.to_string());
                let actual = renderer
                    .measure_text_byte_range(&line, range.start, range.end)
                    .map(bounds_bits)
                    .map_err(|e| e.to_string());
                assert_eq!(actual, expected);
            }
            assert_eq!(
                renderer
                    .measure_text_range(&line, "missing")
                    .err()
                    .unwrap()
                    .to_string(),
                "line 'line' does not contain 'missing'"
            );
        }
    }

    #[test]
    fn integer_sprite_matches_the_former_full_width_clipping_path() {
        fn reference(
            canvas: &mut [u8],
            size: [u32; 2],
            sprite: &super::TextSprite,
            origin: [i32; 2],
            opacity: f32,
        ) {
            let clip_width = sprite.width as f32;
            if opacity <= 0.001 || clip_width <= 0.0 {
                return;
            }
            let visible_width = clip_width.ceil().min(sprite.width as f32) as i32;
            for sy in 0..sprite.height as i32 {
                let y = origin[1] + sy;
                if !(0..size[1] as i32).contains(&y) {
                    continue;
                }
                for sx in 0..visible_width {
                    let x = origin[0] + sx;
                    if !(0..size[0] as i32).contains(&x) {
                        continue;
                    }
                    let source = (sy as usize * sprite.width as usize + sx as usize) * 4;
                    if sprite.pixels[source + 3] == 0 {
                        continue;
                    }
                    let target = (y as usize * size[0] as usize + x as usize) * 4;
                    super::blend_pixel(
                        &mut canvas[target..target + 4],
                        sprite.pixels[source..source + 4].try_into().unwrap(),
                        opacity,
                    );
                }
            }
        }
        for sprite_size in [[4, 3], [1, 1], [0, 3], [3, 0], [u32::MAX, 0]] {
            let sprite = super::TextSprite {
                width: sprite_size[0],
                height: sprite_size[1],
                advance: 2.25,
                pixels: (0..sprite_size[0] as usize * sprite_size[1] as usize)
                    .flat_map(|i| [240, 17, 93, [0, 1, 127, 255][i % 4]])
                    .collect(),
            };
            for size in [[8, 6], [1, 1], [0, 6], [8, 0]] {
                for origin in [[-10, -10], [-1, -1], [0, 0], [1, 2], [7, 5], [8, 6]] {
                    for opacity in [-1., 0., 0.001, 0.001001, 0.5, 1., 2., f32::NAN] {
                        let mut actual = [13, 71, 29, 127].repeat((size[0] * size[1]) as usize);
                        let mut expected = actual.clone();
                        reference(&mut expected, size, &sprite, origin, opacity);
                        super::composite_sprite(
                            &mut actual,
                            size[0],
                            size[1],
                            &sprite,
                            origin[0],
                            origin[1],
                            opacity,
                        );
                        assert_eq!(
                            actual, expected,
                            "{sprite_size:?}/{size:?}/{origin:?}/{opacity}"
                        );
                    }
                }
            }
        }
        // The old cast rounded large widths through f32 and saturated at i32::MAX.
        // A direct u32 -> i32 cast would silently wrap instead.
        for width in [
            0,
            1,
            1320,
            (1 << 24) - 1,
            (1 << 24) + 1,
            i32::MAX as u32,
            u32::MAX,
        ] {
            assert_eq!(
                (width as f32).ceil().min(width as f32) as i32,
                width as f32 as i32
            );
        }
    }

    #[test]
    fn vertical_mask_integrates_linear_fades_and_fractional_edges() {
        let mask = super::VerticalMask {
            top: 2.,
            bottom: 8.,
            fade: 2.,
        };
        assert!(mask.is_valid());
        for (y, expected) in [0., 0., 0.25, 0.75, 1., 1., 0.75, 0.25, 0.]
            .into_iter()
            .enumerate()
        {
            assert_eq!(mask.coverage(y as f32, y as f32 + 1.), expected);
        }
        let fractional = super::VerticalMask {
            top: 2.5,
            bottom: 8.5,
            fade: 2.,
        };
        assert_eq!(fractional.coverage(2., 3.), 0.0625);
        assert_eq!(fractional.coverage(8., 9.), 0.0625);
        let a = super::VerticalMask {
            top: 2.49,
            ..fractional
        };
        let b = super::VerticalMask {
            top: 2.51,
            ..fractional
        };
        assert!((a.coverage(2., 3.) - b.coverage(2., 3.)).abs() < 0.006);
        assert_eq!(
            super::VerticalMask {
                fade: 0.,
                ..fractional
            }
            .coverage(2., 3.),
            0.5
        );
    }

    #[test]
    fn stationary_mask_fades_glyph_rows_not_the_existing_background() {
        let sprite = opaque_test_sprite();
        let mask = super::VerticalMask {
            top: 4.,
            bottom: 12.,
            fade: 2.,
        };
        let mut pixels = vec![0; 16 * 16 * 4];
        super::composite_text(
            &mut pixels,
            [16, 16],
            TextDraw {
                clip_width: 4.,
                mask: Some(mask),
                ..TextDraw::new(&sprite, [4., 3.])
            },
        );
        let alpha = |y: usize| pixels[(y * 16 + 5) * 4 + 3];
        assert_eq!([alpha(3), alpha(4), alpha(5), alpha(6)], [0, 64, 191, 255]);
        for pixel in pixels.as_chunks::<4>().0.iter().filter(|p| p[3] != 0) {
            assert_eq!(&pixel[..3], &[255; 3]);
        }

        let background = [17, 33, 49, 255].repeat(16 * 16);
        let mut pixels = background.clone();
        super::composite_text(
            &mut pixels,
            [16, 16],
            TextDraw {
                clip_width: 4.,
                mask: Some(mask),
                ..TextDraw::new(&sprite, [4., 3.])
            },
        );
        for y in 0..16 {
            for x in 0..16 {
                if x == 0 || !(4..12).contains(&y) {
                    let offset = (y * 16 + x) * 4;
                    assert_eq!(&pixels[offset..offset + 4], &background[offset..offset + 4]);
                }
            }
        }
    }

    #[test]
    fn filtered_text_footprint_has_no_integer_boundary_pops() {
        let sprite = super::TextSprite {
            width: 12,
            height: 12,
            advance: 12.,
            pixels: vec![255; 12 * 12 * 4],
        };
        let draw = |origin, width, blur| {
            let mut pixels = vec![0; 40 * 24 * 4];
            super::composite_text(
                &mut pixels,
                [40, 24],
                TextDraw {
                    clip_width: width,
                    filter: TextFilter::Blur(blur),
                    ..TextDraw::new(&sprite, origin)
                },
            );
            pixels
        };
        for (a, b) in [
            (draw([10.5, 4.], 4.5, 0.), draw([10.5, 4.], 4.51, 0.)),
            (draw([9.99, 4.], 12., 0.5), draw([10., 4.], 12., 0.5)),
            (draw([10., 3.99], 12., 0.5), draw([10., 4.], 12., 0.5)),
        ] {
            assert!(
                a.as_chunks::<4>()
                    .0
                    .iter()
                    .zip(b.as_chunks::<4>().0)
                    .map(|(a, b)| a[3].abs_diff(b[3]))
                    .max()
                    .unwrap()
                    <= 4
            );
        }
    }

    #[test]
    fn rotated_content_uses_the_fractional_parent_center() {
        let sprite = opaque_test_sprite();
        let draw = |x| {
            let mut pixels = vec![0; 32 * 32 * 4];
            super::composite_sprite_rotated(
                &mut pixels,
                32,
                32,
                &sprite,
                5.5,
                5.5,
                x,
                12.25,
                0.12,
                0.2,
                1.,
            );
            pixels
        };
        let centroid = |pixels: &[u8]| {
            let total = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| f64::from(p[3]))
                .sum::<f64>();
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .map(|(index, p)| (index % 32) as f64 * f64::from(p[3]))
                .sum::<f64>()
                / total
        };
        let delta = centroid(&draw(12.51)) - centroid(&draw(12.49));
        assert!((delta - 0.02).abs() < 0.01, "centroid delta={delta}");
    }

    #[test]
    fn transformed_content_blur_remains_continuous_at_zero_and_fractional_radii() {
        let sprite = opaque_test_sprite();
        let draw = |blur| {
            let mut pixels = vec![0; 32 * 32 * 4];
            super::composite_sprite_rotated(
                &mut pixels,
                32,
                32,
                &sprite,
                4.,
                4.,
                16.25,
                16.5,
                0.12,
                blur,
                1.,
            );
            pixels
        };
        for (from, to) in [(0., 0.001), (0.49, 0.51), (3.49, 3.51), (5.99, 6.01)] {
            assert!(
                draw(from)
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(draw(to).as_chunks::<4>().0)
                    .all(|(a, b)| a[3].abs_diff(b[3]) <= 3),
                "blur {from} -> {to}"
            );
        }
    }

    #[test]
    fn fractional_raster_edges_conserve_alpha_and_color() {
        let sprite = super::TextSprite {
            width: 1,
            height: 1,
            advance: 1.,
            pixels: vec![255, 32, 0, 255],
        };
        for origin in [[2., 2.], [2.5, 2.], [2.5, 2.5], [2.25, 2.75]] {
            let mut pixels = vec![0; 8 * 8 * 4];
            super::composite_text(
                &mut pixels,
                [8, 8],
                TextDraw {
                    clip_width: 1.,
                    ..TextDraw::new(&sprite, origin)
                },
            );
            let alpha = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| u32::from(pixel[3]))
                .sum::<u32>();
            assert!(alpha.abs_diff(255) <= 1, "{origin:?}: alpha={alpha}");
            for pixel in pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
            {
                assert_eq!(&pixel[..3], &[255, 32, 0]);
            }
        }
    }

    #[test]
    fn spotlight_region_registers_with_fractional_base_text_and_viewport_clip() {
        let mut sprite = super::TextSprite {
            width: 12,
            height: 12,
            advance: 12.,
            pixels: vec![0; 12 * 12 * 4],
        };
        for row in 0..12 {
            sprite.pixels[(row * 12 + 5) * 4..(row * 12 + 5) * 4 + 4].copy_from_slice(&[255; 4]);
        }
        for y in [4.25, 4.49, 4.51] {
            for blur in [0., 0.49, 0.51] {
                let mut base = vec![0; 40 * 24 * 4];
                let mut bright = base.clone();
                super::composite_text(
                    &mut base,
                    [40, 24],
                    TextDraw {
                        clip_width: 12.,
                        filter: TextFilter::Blur(blur),
                        clip_y: Some([5.5, 14.5]),
                        ..TextDraw::new(&sprite, [10.25, y])
                    },
                );
                super::composite_text(
                    &mut bright,
                    [40, 24],
                    TextDraw {
                        source_left: 2.5,
                        clip_width: 6.,
                        filter: TextFilter::Blur(blur),
                        clip_y: Some([5.5, 14.5]),
                        ..TextDraw::new(&sprite, [12.75, y])
                    },
                );
                assert_eq!(base, bright, "origin y={y}, blur={blur}");
            }
        }
    }

    #[test]
    fn text_positions_clip_edges_and_blur_preserve_fractional_motion() {
        let mut sprite = super::TextSprite {
            width: 12,
            height: 12,
            advance: 12.,
            pixels: vec![0; 12 * 12 * 4],
        };
        sprite.pixels[(5 * 12 + 5) * 4..(5 * 12 + 5) * 4 + 4].copy_from_slice(&[255; 4]);
        let draw = |sprite: &super::TextSprite, x, width, blur| {
            let mut pixels = vec![0; 40 * 24 * 4];
            super::composite_text(
                &mut pixels,
                [40, 24],
                TextDraw {
                    clip_width: width,
                    filter: TextFilter::Blur(blur),
                    ..TextDraw::new(sprite, [x, 4.25])
                },
            );
            pixels
        };
        let centroid = |pixels: &[u8]| {
            let total = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| f64::from(p[3]))
                .sum::<f64>();
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .map(|(i, p)| (i % 40) as f64 * f64::from(p[3]))
                .sum::<f64>()
                / total
        };
        let a = draw(&sprite, 10.49, 12., 0.);
        let b = draw(&sprite, 10.51, 12., 0.);
        assert_ne!(a, b);
        assert!((centroid(&b) - centroid(&a) - 0.02).abs() < 0.01);
        let a = draw(&sprite, 10., 12., 0.49);
        let b = draw(&sprite, 10., 12., 0.51);
        assert!(a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap() < 10);
        sprite.pixels.fill(255);
        let a = draw(&sprite, 10., 4., 0.);
        let b = draw(&sprite, 10., 4.01, 0.);
        let edge = (8 * 40 + 14) * 4 + 3;
        assert_eq!(a[edge], 0);
        assert!((1..=3).contains(&b[edge]));
        assert_eq!(draw(&sprite, 10., 4.01, 0.), b);
    }

    use super::{
        InlineRevealFrame, TextDraw, TextFilter, TextSprite, composite_sprite_rotated,
        composite_sprite_rotated_with_coverage, inline_reveal_segments,
    };

    fn opaque_test_sprite() -> TextSprite {
        TextSprite {
            width: 4,
            height: 4,
            advance: 4.0,
            pixels: vec![255; 4 * 4 * 4],
        }
    }

    #[test]
    fn preview_keeps_dynamic_overlays_separate_and_falls_back_for_optical_motion() {
        use super::{EditorFrame, PointerFrame, TokenHighlight, can_preview_editor};
        let mut frame = EditorFrame {
            panel_offset_x: 0.0,
            panel_offset_y: 0.,
            panel_opacity: 1.0,
            line_marks: &[],
            panel_rotation: 0.,
            panel_tilt_x: 0.,
            panel_tilt_y: 0.,
            panel_scale: 1.,
            panel_near_blur: 0.,
            focus_intensity: 0.,
            focus_line_y: 0.,
            focus_height: 44.,
            token_highlight: TokenHighlight {
                x: 0.,
                y: 0.,
                width: 0.,
                opacity: 0.,
            },
            pointer: PointerFrame {
                x: 0.,
                y: 0.,
                opacity: 0.,
                rotation: 0.,
                scale: 1.,
                blur: 0.,
            },
            inline_reveals: &[],
            lines: &[],
            annotations: Default::default(),
        };
        let hidden = can_preview_editor(&frame, [1920, 1080]);
        frame.focus_line_y = 100.;
        assert_eq!(hidden, can_preview_editor(&frame, [1920, 1080]));
        frame.focus_intensity = 1.;
        assert_eq!(hidden, can_preview_editor(&frame, [1920, 1080]));
        frame.panel_rotation = 0.01;
        assert!(!can_preview_editor(&frame, [1920, 1080]));
        frame.panel_rotation = 0.;
        frame.pointer.opacity = 1.;
        assert!(!can_preview_editor(&frame, [1920, 1080]));
        frame.pointer.opacity = 0.;
        frame.token_highlight.opacity = 1.;
        frame.token_highlight.x = -150.;
        frame.token_highlight.width = 100.;
        assert!(!can_preview_editor(&frame, [1920, 1080]));
        frame.token_highlight.x = 20.;
        frame.token_highlight.y = -90.;
        assert!(!can_preview_editor(&frame, [1920, 1080]));
    }

    #[test]
    fn unmasked_rotated_sprite_matches_constant_coverage() {
        let sprite = opaque_test_sprite();
        let mut wrapped = vec![0_u8; 20 * 20 * 4];
        let mut generic = wrapped.clone();

        composite_sprite_rotated(
            &mut wrapped,
            20,
            20,
            &sprite,
            8.0,
            8.0,
            10.0,
            10.0,
            0.2,
            1.0,
            0.8,
        );
        composite_sprite_rotated_with_coverage(
            &mut generic,
            20,
            20,
            &sprite,
            8.0,
            8.0,
            10.0,
            10.0,
            0.2,
            1.0,
            0.8,
            |_, _| 1.0,
        );

        assert_eq!(wrapped, generic);
    }

    #[test]
    fn rotated_sprite_coverage_clips_after_blur_sampling() {
        let sprite = opaque_test_sprite();
        let mut pixels = vec![0_u8; 20 * 20 * 4];
        let coverage = |x: f32, y: f32| {
            if (8.0..12.0).contains(&x) && (8.0..12.0).contains(&y) {
                1.0
            } else {
                0.0
            }
        };

        composite_sprite_rotated_with_coverage(
            &mut pixels,
            20,
            20,
            &sprite,
            12.0,
            12.0,
            10.0,
            10.0,
            0.35,
            2.0,
            1.0,
            coverage,
        );

        let mut painted = 0;
        for y in 0..20 {
            for x in 0..20 {
                let alpha = pixels[(y * 20 + x) * 4 + 3];
                if alpha > 0 {
                    painted += 1;
                    assert!(coverage(x as f32 + 0.5, y as f32 + 0.5) > 0.0);
                }
            }
        }
        assert!(painted > 0);
    }

    #[test]
    fn multiple_inline_reveals_partition_one_stable_line() {
        let segments = inline_reveal_segments(
            8,
            &[
                InlineRevealFrame {
                    line_id: "line",
                    start_span: 5,
                    end_span: 7,
                    progress: 0.25,
                },
                InlineRevealFrame {
                    line_id: "line",
                    start_span: 1,
                    end_span: 3,
                    progress: 0.75,
                },
            ],
        )
        .unwrap();

        assert_eq!(
            segments,
            vec![
                (0, 1, None),
                (1, 3, Some(0.75)),
                (3, 5, None),
                (5, 7, Some(0.25)),
                (7, 8, None),
            ]
        );
    }

    #[test]
    fn overlapping_inline_reveals_are_rejected() {
        let error = inline_reveal_segments(
            5,
            &[
                InlineRevealFrame {
                    line_id: "line",
                    start_span: 1,
                    end_span: 3,
                    progress: 1.0,
                },
                InlineRevealFrame {
                    line_id: "line",
                    start_span: 2,
                    end_span: 4,
                    progress: 1.0,
                },
            ],
        )
        .unwrap_err();

        assert!(error.to_string().contains("overlap"));
    }
}
