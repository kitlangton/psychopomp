//! An opaque connected 3D grid with depth-tested labels. This is intentionally a
//! concrete diagram pass, not a mesh/material/scene-graph abstraction.
use super::*;
use psychopomp::grid::{GridAlignment, GridFillPlan, GridRules, GridStylePlan};

mod edges;
mod palette;
pub use palette::GridLinePalette;

const SAMPLES: u32 = 4;
const TILE: [u32; 2] = [256, 128];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 12] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4];

/// The ink aperture follows sampled geometry, not a timer.
pub struct GridTextDisclosure {
    pub opacity: f32,
    pub clip: GridTextClip,
    pub feather_pixels: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridTextClip {
    None,
    Cell,
    /// Signed bounds relative to this heading's center, along its catalog axis.
    Heading {
        axis: usize,
        start: f32,
        end: f32,
        cell: f32,
    },
}

pub struct GridItemFrame<'a> {
    pub label: &'a str,
    pub detail: &'a str,
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub presence: f32,
    pub reveal: [f32; 3],
    pub trim: [f32; 3],
    pub fill: [f32; 3],
    pub label_opacity: f32,
    pub text_disclosure: Option<GridTextDisclosure>,
    pub emphasis: f32,
    pub group: bool,
    pub heading: bool,
    pub label_style: Option<GridLabelStyle>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLabelStyle {
    pub font_size: f32,
    pub padding: f32,
    pub alignment: GridAlignment,
}

pub struct GridFrame<'a> {
    pub items: &'a [GridItemFrame<'a>],
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    center: [f32; 4],
    size: [f32; 4],
    color: [f32; 4],
    atlas: [f32; 4],
    label: [f32; 4],
    reveal: [f32; 4],
    trim: [f32; 4],
    fill: [f32; 4],
    text_disclosure: [f32; 4],
    text_clip_a: [f32; 4],
    text_clip_b: [f32; 4],
    label_layout: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Camera {
    viewport: [f32; 4],
    orbit: [f32; 4],
    depth: [f32; 4],
    background: [f32; 4],
    ink: [f32; 4],
}

pub(super) struct GridRenderer {
    pipeline: wgpu::RenderPipeline,
    heading_pipeline: wgpu::RenderPipeline,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    binding: wgpu::BindGroup,
    heading_binding: wgpu::BindGroup,
    labels: Vec<(String, String, bool)>,
    label_styles: Vec<Option<(GridLabelStyle, f32)>>,
    tile: [u32; 2],
    advances: Vec<f32>,
    atlas_rows: u32,
    atlas_columns: u32,
    edges: edges::Edges,
}

impl HeadlessRenderer {
    pub fn set_grid_line_palette(&mut self, palette: Option<GridLinePalette>) {
        self.grid_line_palette = palette;
    }

    pub fn render_grid(&mut self, frame: GridFrame<'_>) -> Result<Vec<u8>> {
        self.render_grid_styled(frame, None)
    }

    pub fn render_grid_styled(
        &mut self,
        frame: GridFrame<'_>,
        style: Option<&GridStylePlan>,
    ) -> Result<Vec<u8>> {
        // The recipe's immutable catalog warms all label resources before the
        // window opens, including cells that start hidden.
        if self.grid_renderer.as_ref().is_none_or(|grid| {
            !grid
                .labels
                .iter()
                .map(|(label, detail, heading)| (label.as_str(), detail.as_str(), *heading))
                .eq(frame
                    .items
                    .iter()
                    .map(|item| (item.label, item.detail, item.heading || item.group)))
                || !grid.label_styles.iter().copied().eq(frame
                    .items
                    .iter()
                    .map(|item| item.label_style.map(|s| (s, item.size[0]))))
        }) {
            self.grid_renderer = Some(GridRenderer::new(
                &self.device,
                &self.queue,
                &self.spec,
                &mut self.font_system,
                &mut self.swash_cache,
                frame.items,
            )?);
        }
        let grid = self.grid_renderer.as_ref().expect("prepared grid renderer");
        let mut instances = Vec::with_capacity(frame.items.len());
        for (index, item) in frame.items.iter().enumerate() {
            let presence = if item.heading || item.group {
                item.text_disclosure
                    .as_ref()
                    .map_or(item.presence, |pose| pose.opacity)
            } else {
                item.presence
            };
            if presence <= 0.00001 || item.reveal.iter().any(|v| *v <= 0.00001) {
                continue;
            }
            let label_scale = if item.label_style.is_some() {
                1.
            } else {
                ((item.size[0] - 16.) / grid.advances[index].max(1.)).min(1.)
            };
            let [text_clip_a, text_clip_b] =
                text_clip_planes(item, &frame, grid.advances[index] * label_scale);
            let color = if item.heading || item.group {
                item.color
            } else {
                self.grid_line_palette
                    .map_or(item.color, GridLinePalette::color)
            };
            let color = if !item.heading
                && !item.group
                && self.theme != Theme::Original
                && self.grid_line_palette.is_none()
            {
                theme::linear(self.theme.palette().accent)
            } else {
                color
            };
            let fill = if self.theme == Theme::Original {
                item.fill
            } else {
                let p = self.theme.palette();
                if item.fill == [0.009, 0.012, 0.020] {
                    theme::linear(p.background)
                } else if item.fill == [0.018, 0.024, 0.036] {
                    theme::linear(p.surface)
                } else if style.is_none_or(|s| {
                    matches!(
                        s.fill,
                        GridFillPlan::Checkerboard | GridFillPlan::Banded { .. }
                    )
                }) {
                    theme::linear(p.raised)
                } else {
                    item.fill
                }
            };
            instances.push(Instance {
                center: [
                    item.center[0],
                    item.center[1],
                    item.center[2],
                    item.presence,
                ],
                size: [
                    item.size[0],
                    item.size[1],
                    item.size[2],
                    if item.group {
                        1.
                    } else if item.heading {
                        if item.label_style.is_some() { 3. } else { 2. }
                    } else {
                        0.
                    },
                ],
                color: [color[0], color[1], color[2], item.emphasis * item.presence],
                atlas: [
                    (index as u32 % grid.atlas_columns) as f32 / grid.atlas_columns as f32,
                    (index as u32 / grid.atlas_columns) as f32 / grid.atlas_rows as f32,
                    1. / grid.atlas_columns as f32,
                    1. / grid.atlas_rows as f32,
                ],
                label: [
                    grid.tile[0] as f32 * label_scale,
                    grid.tile[1] as f32 * label_scale,
                    0.,
                    item.label_opacity,
                ],
                reveal: [
                    item.reveal[0],
                    item.reveal[1],
                    item.reveal[2],
                    style.map_or(1., |s| s.line_opacity),
                ],
                trim: [item.trim[0], item.trim[1], item.trim[2], 0.],
                fill: [fill[0], fill[1], fill[2], item.presence],
                text_disclosure: item
                    .text_disclosure
                    .as_ref()
                    .map_or([0.; 4], |pose| [pose.opacity, pose.feather_pixels, 1., 0.]),
                text_clip_a,
                text_clip_b,
                label_layout: item.label_style.map_or([0.; 4], |s| {
                    let half = (item.size[0] / 2. - s.padding).max(0.);
                    let x = match s.alignment {
                        GridAlignment::Left => -half + grid.advances[index] / 2.,
                        GridAlignment::Center => 0.,
                        GridAlignment::Right => half - grid.advances[index] / 2.,
                    };
                    [x, 0., half, 1.]
                }),
            });
        }
        assert!(instances.len() <= grid.capacity);
        self.queue
            .write_buffer(&grid.instances, 0, bytemuck::cast_slice(&instances));
        let bounds = visible_bounds(&frame).unwrap_or([0.; 6]);
        let center = if style.is_some_and(|s| s.table.is_some()) {
            [0.; 2]
        } else {
            [(bounds[0] + bounds[2]) / 2., (bounds[1] + bounds[3]) / 2.]
        };
        self.queue.write_buffer(
            &grid.uniform,
            0,
            bytemuck::bytes_of(&Camera {
                viewport: [
                    self.spec.width as f32,
                    self.spec.height as f32,
                    0.5 - center[0] * frame.scale / self.spec.width as f32,
                    0.5 + center[1] * frame.scale / self.spec.height as f32,
                ],
                orbit: [
                    frame.yaw,
                    frame.pitch,
                    frame.scale,
                    style.map_or(edges::WIDTH, |s| s.line_width),
                ],
                // Fit depth to sampled geometry too, rather than spending most
                // of the depth buffer's precision on empty space.
                depth: [
                    (bounds[4] + bounds[5]) / 2.,
                    1. / (bounds[5] - bounds[4] + 2.),
                    (grid.capacity as f32 + 1.) * 0.001,
                    style.map_or(0., |s| {
                        let rules = match s.rules {
                            GridRules::Grid => 0.,
                            GridRules::Rows => 1.,
                            GridRules::None => 2.,
                        };
                        rules
                            + if matches!(s.fill, GridFillPlan::None) || s.table.is_some() {
                                4.
                            } else {
                                0.
                            }
                    }),
                ],
                background: {
                    let c = if self.theme == Theme::Original {
                        [0.009, 0.012, 0.020]
                    } else {
                        theme::linear(self.theme.palette().background)
                    };
                    [c[0], c[1], c[2], 1.]
                },
                ink: {
                    let c = if self.theme == Theme::Original {
                        [0.7, 0.74, 0.82]
                    } else {
                        theme::linear(self.theme.palette().text)
                    };
                    [c[0], c[1], c[2], 1.]
                },
            }),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("keyed grid sample"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth-tested grid"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &grid.color,
                    depth_slice: None,
                    resolve_target: Some(&grid.edges.base),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear({
                            let c = if self.theme == Theme::Original {
                                [0.009, 0.012, 0.020]
                            } else {
                                theme::linear(self.theme.palette().background)
                            };
                            wgpu::Color {
                                r: c[0] as f64,
                                g: c[1] as f64,
                                b: c[2] as f64,
                                a: 1.,
                            }
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &grid.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&grid.pipeline);
            pass.set_bind_group(0, &grid.binding, &[]);
            pass.set_vertex_buffer(0, grid.instances.slice(..));
            pass.draw(0..36, 0..instances.len() as u32);
        }
        grid.edges.render(
            &mut encoder,
            &grid.instances,
            instances.len() as u32,
            &grid.depth,
            &self.view,
        );
        {
            // Upright headings are foreground ink, not 3D surfaces. Blend them
            // after strokes without a depth attachment so fading glyphs cannot
            // punch holes in either the material or the grid lines.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("grid heading overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&grid.heading_pipeline);
            pass.set_bind_group(0, &grid.heading_binding, &[]);
            pass.set_vertex_buffer(0, grid.instances.slice(..));
            pass.draw(0..6, 0..instances.len() as u32);
        }
        self.read_frame(encoder)
    }
}

impl GridRenderer {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        spec: &RenderSpec,
        fonts: &mut FontSystem,
        cache: &mut SwashCache,
        items: &[GridItemFrame<'_>],
    ) -> Result<Self> {
        let shader = device.create_shader_module(wgpu::include_wgsl!("grid.wgsl"));
        let pipeline = |heading| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if heading {
                    "grid heading ink"
                } else {
                    "instanced keyed grid"
                }),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &INSTANCE_ATTRIBUTES,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: (!heading).then_some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: if heading { 1 } else { SAMPLES },
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if heading {
                        "fragment_heading"
                    } else {
                        "fragment_main"
                    }),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: heading.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let heading_pipeline = pipeline(true);
        let pipeline = pipeline(false);
        let attachment = |format, label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: spec.width,
                        height: spec.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = attachment(FORMAT, "grid multisample color");
        let depth = attachment(wgpu::TextureFormat::Depth32Float, "grid depth");
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid orthographic orbit"),
            size: std::mem::size_of::<Camera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let capacity = items.len().max(1);
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tile = if items.iter().any(|i| i.label_style.is_some()) {
            [
                items
                    .iter()
                    .map(|i| i.size[0].ceil() as u32)
                    .max()
                    .unwrap_or(256)
                    .max(256)
                    .next_power_of_two(),
                items
                    .iter()
                    .filter_map(|i| i.label_style)
                    .map(|s| (s.font_size * 3.).ceil() as u32)
                    .max()
                    .unwrap_or(128)
                    .max(128)
                    .next_power_of_two(),
            ]
        } else {
            TILE
        };
        let atlas_columns = ((items.len() as f32 * tile[1] as f32 / tile[0] as f32)
            .sqrt()
            .ceil() as u32)
            .clamp(1, device.limits().max_texture_dimension_2d / tile[0]);
        let atlas_rows = (items.len() as u32).div_ceil(atlas_columns).max(1);
        let atlas_width = atlas_columns * tile[0];
        let atlas_height = atlas_rows * tile[1];
        if atlas_height > device.limits().max_texture_dimension_2d {
            bail!(
                "grid label atlas exceeds this adapter's texture limit; reduce table font size or catalog size"
            );
        }
        // Glyph color comes from the theme uniform; only coverage belongs in
        // the atlas. Keep the same 8-bit alpha precision without unused RGB.
        let mut pixels = vec![0_u8; (atlas_width * atlas_height) as usize];
        let mut advances = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let sprite = if let Some(style) = item.label_style {
                table_label_sprite(
                    fonts,
                    cache,
                    item.label,
                    item.detail,
                    style,
                    item.size[0],
                    tile,
                )
            } else {
                label_sprite(
                    fonts,
                    cache,
                    item.label,
                    item.detail,
                    item.heading || item.group,
                )
            };
            advances.push(sprite.advance);
            let base_x = index as u32 % atlas_columns * tile[0];
            let base_y = index as u32 / atlas_columns * tile[1];
            let offset_x = (tile[0] - sprite.width) / 2;
            let offset_y = (tile[1] - sprite.height) / 2;
            for y in 0..sprite.height {
                for x in 0..sprite.width {
                    let source = ((y * sprite.width + x) * 4) as usize;
                    let target =
                        ((base_y + offset_y + y) * atlas_width + base_x + offset_x + x) as usize;
                    pixels[target] = sprite.pixels[source + 3];
                }
            }
        }
        let view = upload_r8(
            device,
            queue,
            "grid face label atlas",
            [atlas_width, atlas_height],
            &pixels,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let binding = |pipeline: &wgpu::RenderPipeline| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("grid camera and labels"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        let heading_binding = binding(&heading_pipeline);
        let binding = binding(&pipeline);
        let edges = edges::Edges::new(device, spec, &uniform);
        Ok(Self {
            pipeline,
            heading_pipeline,
            color,
            depth,
            uniform,
            instances,
            capacity,
            binding,
            heading_binding,
            labels: items
                .iter()
                .map(|item| {
                    (
                        item.label.to_owned(),
                        item.detail.to_owned(),
                        item.heading || item.group,
                    )
                })
                .collect(),
            advances,
            tile,
            label_styles: items
                .iter()
                .map(|i| i.label_style.map(|s| (s, i.size[0])))
                .collect(),
            atlas_rows,
            atlas_columns,
            edges,
        })
    }
}

fn table_label_sprite(
    fonts: &mut FontSystem,
    cache: &mut SwashCache,
    primary: &str,
    detail: &str,
    style: GridLabelStyle,
    width: f32,
    tile: [u32; 2],
) -> TextSprite {
    let available = (width - 2. * style.padding)
        .min(tile[0] as f32 - 8.)
        .max(1.);
    let mut lines = Vec::new();
    for (text, size) in [(primary, style.font_size), (detail, style.font_size * 0.55)] {
        if text.is_empty() {
            continue;
        }
        let attrs = Attrs::new()
            .family(fonts::SANS)
            .color(Color::rgb(255, 255, 255));
        let height = (size * 1.4).ceil() as u32;
        lines.push(make_sprite(
            fonts,
            cache,
            vec![(text, attrs.clone())],
            attrs,
            Metrics::new(size, height as f32),
            tile[0],
            height,
        ));
    }
    let advance = lines
        .iter()
        .map(|s| s.advance.min(available))
        .fold(0., f32::max);
    let total_height =
        lines.iter().map(|s| s.height).sum::<u32>() + 4 * lines.len().saturating_sub(1) as u32;
    let mut top = tile[1].saturating_sub(total_height) / 2;
    let mut pixels = vec![0; (tile[0] * tile[1] * 4) as usize];
    for line in lines {
        let ink_width = line.advance.min(available);
        let offset = match style.alignment {
            GridAlignment::Left => 0.,
            GridAlignment::Center => (advance - ink_width) / 2.,
            GridAlignment::Right => advance - ink_width,
        };
        let left = ((tile[0] as f32 - advance) / 2. + offset).floor().max(0.) as u32;
        for y in 0..line.height.min(tile[1] - top) {
            for x in 0..(ink_width.ceil() as u32).min(tile[0] - left) {
                let from = ((y * line.width + x) * 4) as usize;
                let to = (((top + y) * tile[0] + left + x) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&line.pixels[from..from + 4]);
                pixels[to + 3] = (f32::from(pixels[to + 3]) * (available - x as f32).clamp(0., 1.))
                    .round() as u8;
            }
        }
        top = (top + line.height + 4).min(tile[1]);
    }
    TextSprite {
        width: tile[0],
        height: tile[1],
        advance,
        pixels,
    }
}

fn label_sprite(
    fonts: &mut FontSystem,
    cache: &mut SwashCache,
    primary: &str,
    detail: &str,
    heading: bool,
) -> TextSprite {
    let mut pixels = vec![0; (TILE[0] * TILE[1] * 4) as usize];
    let mut advance = 0_f32;
    for (text, size, height, top) in if heading {
        [(primary, 24., 32, 48), ("", 18., 28, 96)]
    } else {
        [(primary, 78., 96, 0), (detail, 18., 28, 96)]
    } {
        if text.is_empty() {
            continue;
        }
        let mut rasterize = |size| {
            let attrs = Attrs::new()
                .family(fonts::mono())
                .color(Color::rgb(255, 255, 255));
            make_sprite(
                fonts,
                cache,
                vec![(text, attrs.clone())],
                attrs,
                Metrics::new(size, height as f32),
                TILE[0],
                height,
            )
        };
        let mut line = rasterize(size);
        let available = (TILE[0] - 8) as f32;
        if line.advance > available {
            line = rasterize(size * available / line.advance);
        }
        advance = advance.max(line.advance);
        let left = (TILE[0] as f32 - line.advance).max(0.) as u32 / 2;
        for y in 0..height {
            for x in 0..TILE[0] - left {
                let from = ((y * TILE[0] + x) * 4) as usize;
                let to = (((y + top) * TILE[0] + x + left) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&line.pixels[from..from + 4]);
            }
        }
    }
    // Lines are already centered inside this atlas tile.
    TextSprite {
        width: TILE[0],
        height: TILE[1],
        advance,
        pixels,
    }
}

fn project(point: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let [x, y, z] = point;
    let turned = [cy * x + sy * z, y, -sy * x + cy * z];
    [
        turned[0],
        cp * turned[1] - sp * turned[2],
        sp * turned[1] + cp * turned[2],
    ]
}

/// Two local half-planes, positive on visible ink. Cells use their actual cut
/// edges; upright headings extend the same cell interval along its projected
/// growth direction. Adjacent headings therefore meet the edge at different times.
fn text_clip_planes(item: &GridItemFrame<'_>, frame: &GridFrame<'_>, width: f32) -> [[f32; 4]; 2] {
    let Some(pose) = &item.text_disclosure else {
        return [[0., 0., 1., 0.]; 2];
    };
    match pose.clip {
        GridTextClip::None => [[0., 0., 1., 0.]; 2],
        GridTextClip::Cell => [
            [-1., 0., (item.reveal[0] - 0.5) * item.size[0], 0.],
            [0., 1., (item.reveal[1] - 0.5) * item.size[1], 0.],
        ],
        GridTextClip::Heading {
            axis,
            start,
            end,
            cell,
        } => {
            if item.label_style.is_some() {
                // Conventional column headers belong to the table plane, not
                // upright billboards. Clip in that same local coordinate space.
                return [[1., 0., -start, 0.], [-1., 0., end, 0.]];
            }
            let mut direction = [0.; 3];
            direction[axis] = if axis == 0 { 1. } else { -1. };
            let projected = project(direction, frame.yaw, frame.pitch);
            let length = projected[0].hypot(projected[1]);
            let normal = [
                projected[0] / length.max(0.0001),
                projected[1] / length.max(0.0001),
            ];
            // Billboards do not foreshorten. When a projected cell is narrower
            // than its ink, fit that one aperture to the glyph footprint. This
            // continuous end-on fallback preserves fully revealed resting labels
            // without selecting a new wipe direction on reversal or camera moves.
            let ink_span = normal[0].abs() * width + normal[1].abs() * item.size[1];
            let span = (cell * length).max(ink_span + 2. * pose.feather_pixels / frame.scale);
            [
                [normal[0], normal[1], -start * span / cell, 0.],
                [-normal[0], -normal[1], end * span / cell, 0.],
            ]
        }
    }
}

/// Bounds of sampled *cell geometry*, not the final catalog or side headings.
/// This uses the same fractional clipping and projection as the vertex shader.
fn visible_bounds(frame: &GridFrame<'_>) -> Option<[f32; 6]> {
    let mut bounds: Option<[f32; 6]> = None;
    for item in frame.items.iter().filter(|item| {
        !item.group
            && !item.heading
            && item.presence > 0.00001
            && item.reveal.iter().all(|v| *v > 0.00001)
    }) {
        for corner in 0..8 {
            let p = std::array::from_fn(|axis| {
                let fraction = item.trim[axis]
                    + if corner & (1 << axis) == 0 {
                        0.
                    } else {
                        item.reveal[axis]
                    };
                item.center[axis]
                    + item.size[axis]
                        * if axis == 0 {
                            fraction - 0.5
                        } else {
                            0.5 - fraction
                        }
            });
            let [x, y, z] = project(p, frame.yaw, frame.pitch);
            bounds = Some(match bounds {
                None => [x, y, x, y, z, z],
                Some([l, b, r, t, near, far]) => [
                    l.min(x),
                    b.min(y),
                    r.max(x),
                    t.max(y),
                    near.min(z),
                    far.max(z),
                ],
            });
        }
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::theme::srgb_to_linear;

    #[test]
    #[ignore = "requires a headless GPU; fading headings must blend over cells and strokes, never paint dark glyphs"]
    fn heading_opacity_blends_with_the_existing_frame() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "heading-alpha-proof".into(),
        }))
        .unwrap();
        let render = |renderer: &mut HeadlessRenderer, opacity, group| {
            let items = [
                GridItemFrame {
                    label: "",
                    detail: "",
                    center: [0.; 3],
                    size: [260., 180., 150.],
                    color: [0.22, 0.28, 0.36],
                    fill: [0.08, 0.10, 0.14],
                    presence: 1.,
                    reveal: [1.; 3],
                    trim: [0.; 3],
                    label_opacity: 0.,
                    text_disclosure: None,
                    emphasis: 1.,
                    group: false,
                    heading: false,
                    label_style: None,
                },
                GridItemFrame {
                    label: "White · I",
                    detail: "",
                    center: [0., 0., 75.],
                    size: [240., 40., 0.],
                    color: [0.55, 0.6, 0.68],
                    fill: [0.; 3],
                    presence: 1.,
                    reveal: [1.; 3],
                    trim: [0.; 3],
                    label_opacity: 1.,
                    text_disclosure: Some(GridTextDisclosure {
                        opacity,
                        clip: GridTextClip::None,
                        feather_pixels: 8.,
                    }),
                    emphasis: 1.,
                    group,
                    heading: true,
                    label_style: None,
                },
            ];
            renderer
                .render_grid(GridFrame {
                    items: &items,
                    yaw: 0.62,
                    pitch: 0.58,
                    scale: 1.,
                })
                .unwrap()
        };
        for group in [false, true] {
            let blank = render(&mut renderer, 0., group);
            let full = render(&mut renderer, 1., group);
            assert_ne!(blank, full);
            for opacity in [0.5, 0.1, 0.01, 0.001, 0., 0.25, 0.75, 1.] {
                let pixels = render(&mut renderer, opacity, group);
                let dark = pixels
                    .iter()
                    .zip(&blank)
                    .filter(|(pixel, base)| pixel < base)
                    .count();
                assert_eq!(
                    dark, 0,
                    "fading heading darkened {dark} components: group={group}, opacity={opacity}"
                );
                let error = pixels
                    .iter()
                    .zip(&blank)
                    .zip(&full)
                    .map(|((&pixel, &base), &full)| {
                        (srgb_to_linear(pixel)
                            - (srgb_to_linear(base) * (1. - opacity)
                                + srgb_to_linear(full) * opacity))
                            .abs()
                    })
                    .fold(0., f32::max);
                assert!(
                    error < 0.008,
                    "heading must alpha-blend over existing pixels: group={group}, opacity={opacity}, error={error}"
                );
            }
            assert_eq!(
                blank,
                render(&mut renderer, 0., group),
                "no frame-history residue"
            );
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; follows silhouette edges as they become two-face creases"]
    fn silhouette_and_crease_strokes_keep_the_same_width() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "crease-width-proof".into(),
        }))
        .unwrap();
        let mut widths = Vec::new();
        for (point, direction, yaw, pitch) in [
            ([0., 75., 75.], [1., 0., 0.], 0., 0.),
            ([0., 75., 75.], [1., 0., 0.], 0., 0.03),
            ([0., 75., 75.], [1., 0., 0.], 0.62, 0.58),
            ([0., 75., -75.], [1., 0., 0.], 0.62, 0.58),
            ([-75., 0., 75.], [0., 1., 0.], 0., 0.),
            ([-75., 0., 75.], [0., 1., 0.], 0.03, 0.),
            ([-75., 0., 75.], [0., 1., 0.], 0.62, 0.58),
        ] {
            let render = |renderer: &mut HeadlessRenderer, color| {
                renderer
                    .render_grid(GridFrame {
                        items: &[GridItemFrame {
                            label: "",
                            detail: "",
                            center: [0.; 3],
                            size: [150.; 3],
                            color,
                            fill: [0.; 3],
                            presence: 1.,
                            reveal: [1.; 3],
                            trim: [0.; 3],
                            label_opacity: 0.,
                            text_disclosure: None,
                            emphasis: 1.,
                            group: false,
                            heading: false,
                            label_style: None,
                        }],
                        yaw,
                        pitch,
                        scale: 1.,
                    })
                    .unwrap()
            };
            let pixels = render(&mut renderer, [1.; 3]);
            let blank = render(&mut renderer, [0.; 3]);
            let edge = project(point, yaw, pitch);
            let axis = project(direction, yaw, pitch);
            let length = axis[0].hypot(axis[1]);
            let tangent = [axis[0] / length, -axis[1] / length];
            let normal = [-tangent[1], tangent[0]];
            let center = [960. + edge[0], 540. - edge[1]];
            let half_length = 40. * length;
            let mut coverage = 0.;
            for y in
                (center[1] - half_length - 8.) as usize..(center[1] + half_length + 8.) as usize
            {
                for x in
                    (center[0] - half_length - 8.) as usize..(center[0] + half_length + 8.) as usize
                {
                    let offset = [x as f32 + 0.5 - center[0], y as f32 + 0.5 - center[1]];
                    let along = offset[0] * tangent[0] + offset[1] * tangent[1];
                    let across = offset[0] * normal[0] + offset[1] * normal[1];
                    if along.abs() < half_length && across.abs() < 2. {
                        let depth = edge[2] + along / length * axis[2];
                        coverage += (srgb_to_linear(pixels[(y * 1920 + x) * 4])
                            - srgb_to_linear(blank[(y * 1920 + x) * 4]))
                            / (0.7 + depth / 1600.).clamp(0.32, 0.95);
                    }
                }
            }
            let width = coverage / (2. * half_length);
            eprintln!("edge={point:?} yaw={yaw} pitch={pitch}: {width:.3}px");
            widths.push(width);
        }
        let min = widths.iter().copied().fold(f32::INFINITY, f32::min);
        let max = widths.iter().copied().fold(0., f32::max);
        assert!(
            max - min < 0.12,
            "silhouette/crease width changes: {widths:?}"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; measures stroke coverage perpendicular to the same shared edge"]
    fn grid_line_width_is_constant_through_rotation_and_zoom() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "line-width-proof".into(),
        }))
        .unwrap();
        let mut widths = Vec::new();
        for (yaw, pitch, scale) in [
            (0., 0., 1.),
            (0.3, 0.3, 1.),
            (0.62, 0.58, 1.),
            (0.9, 0.8, 1.),
            (0.62, 0.58, 0.72),
            (0.62, 0.58, 1.25),
        ] {
            let items = [75., -75.].map(|y| GridItemFrame {
                label: "",
                detail: "",
                center: [0., y, 0.],
                size: [150.; 3],
                color: [1.; 3],
                fill: [0.; 3],
                presence: 1.,
                reveal: [1.; 3],
                trim: [0.; 3],
                label_opacity: 0.,
                text_disclosure: None,
                emphasis: 1.,
                group: false,
                heading: false,
                label_style: None,
            });
            let pixels = renderer
                .render_grid(GridFrame {
                    items: &items,
                    yaw,
                    pitch,
                    scale,
                })
                .unwrap();
            let edge = project([0., 0., 75.], yaw, pitch);
            let axis = project([1., 0., 0.], yaw, pitch);
            let length = axis[0].hypot(axis[1]);
            let tangent = [axis[0] / length, -axis[1] / length];
            let normal = [-tangent[1], tangent[0]];
            let center = [960. + edge[0] * scale, 540. - edge[1] * scale];
            let half_length = 40. * length * scale;
            let mut coverage = 0.;
            for y in
                (center[1] - half_length - 8.) as usize..(center[1] + half_length + 8.) as usize
            {
                for x in
                    (center[0] - half_length - 8.) as usize..(center[0] + half_length + 8.) as usize
                {
                    let offset = [x as f32 + 0.5 - center[0], y as f32 + 0.5 - center[1]];
                    let along = offset[0] * tangent[0] + offset[1] * tangent[1];
                    let across = offset[0] * normal[0] + offset[1] * normal[1];
                    if along.abs() < half_length && across.abs() < 6. {
                        let depth = (edge[2] + along / (length * scale) * axis[2]) * scale;
                        coverage += srgb_to_linear(pixels[(y * 1920 + x) * 4])
                            / (0.7 + depth / 1600.).clamp(0.32, 0.95);
                    }
                }
            }
            let width = coverage / (2. * half_length);
            eprintln!("yaw={yaw} pitch={pitch} scale={scale}: {width:.3} pixel stroke");
            widths.push(width);
        }
        let min = widths.iter().copied().fold(f32::INFINITY, f32::min);
        let max = widths.iter().copied().fold(0., f32::max);
        assert!(max - min < 0.12, "line width changes with view: {widths:?}");
    }

    #[test]
    fn long_tuple_labels_fit_before_rasterization() {
        let mut fonts = crate::render::fonts::font_system();
        let mut cache = SwashCache::new();
        let sprite = label_sprite(
            &mut fonts,
            &mut cache,
            "longname,longname,longname",
            "",
            false,
        );
        assert!(sprite.advance <= (TILE[0] - 7) as f32);
        assert!(sprite.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    }

    #[test]
    #[ignore = "requires a headless GPU; proves directional linear coverage in output pixels"]
    fn heading_feather_follows_x_y_and_projected_depth() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "feather-proof".into(),
        }))
        .unwrap();
        let render = |renderer: &mut HeadlessRenderer, opacity, end, axis, yaw, pitch, scale| {
            let item = GridItemFrame {
                label: "ABCDEF",
                detail: "",
                center: [0.; 3],
                size: [150., 40., 0.],
                color: [0.9, 0.3, 0.05],
                fill: [0.03, 0.04, 0.05],
                presence: 1.,
                reveal: [1.; 3],
                trim: [0.; 3],
                label_opacity: 1.,
                emphasis: 1.,
                group: false,
                heading: true,
                label_style: None,
                text_disclosure: Some(GridTextDisclosure {
                    opacity,
                    clip: GridTextClip::Heading {
                        axis,
                        start: -150.,
                        end,
                        cell: 300.,
                    },
                    feather_pixels: 8.,
                }),
            };
            renderer
                .render_grid(GridFrame {
                    items: &[item],
                    yaw,
                    pitch,
                    scale,
                })
                .unwrap()
        };
        for (axis, yaw, pitch, scale) in [
            (0, 0., 0., 1.),
            (1, 0., 0., 1.),
            (2, 0.62, 0.58, 1.),
            (2, 0.62, 0.58, 0.65),
        ] {
            let full = render(&mut renderer, 1., 150., axis, yaw, pitch, scale);
            let blank = render(&mut renderer, 0., 150., axis, yaw, pitch, scale);
            let mut direction = [0.; 3];
            direction[axis] = if axis == 0 { 1. } else { -1. };
            let projected = project(direction, yaw, pitch);
            let length = projected[0].hypot(projected[1]);
            let normal = [projected[0] / length, projected[1] / length];
            let half = render(
                &mut renderer,
                1.,
                4. / (length * scale),
                axis,
                yaw,
                pitch,
                scale,
            );
            assert_ne!(full, half, "axis {axis}");
            let mut hidden = 0;
            let mut shown = 0;
            for (i, ((full, blank), half)) in full
                .as_chunks::<4>()
                .0
                .iter()
                .zip(blank.as_chunks::<4>().0)
                .zip(half.as_chunks::<4>().0)
                .enumerate()
            {
                if full == blank {
                    assert_eq!(full, half, "feather cannot alter material or background");
                } else {
                    let along = normal[0] * (i % 1920) as f32 - normal[0] * 959.5
                        + normal[1] * (539.5 - (i / 1920) as f32)
                        - 4.;
                    if along > 1. {
                        assert_eq!(blank, half, "ink ahead of edge on axis {axis}");
                        hidden += 1;
                    }
                    if along < -9. {
                        assert_eq!(full, half, "ink behind edge on axis {axis}");
                        shown += 1;
                    }
                }
            }
            assert!(
                hidden > 10 && shown > 10,
                "must reveal different parts, not a uniform fade: {axis} {hidden} {shown}"
            );
            let pixel = (520..560)
                .flat_map(|y| (905..1015).map(move |x| (y * 1920 + x) * 4))
                .max_by_key(|&i| full[i].abs_diff(blank[i]))
                .unwrap();
            assert!(
                full[pixel].abs_diff(blank[pixel]) > 100,
                "test must sample a visible text stroke"
            );
            let local = [
                ((pixel / 4 % 1920) as f32 + 0.5 - 960.) / scale,
                (540. - (pixel / 4 / 1920) as f32 - 0.5) / scale,
            ];
            for coverage in [0.25, 0.5, 0.75] {
                let end =
                    (normal[0] * local[0] + normal[1] * local[1] + coverage * 8. / scale) / length;
                let pixels = render(&mut renderer, 1., end, axis, yaw, pitch, scale);
                let measured = (srgb_to_linear(pixels[pixel]) - srgb_to_linear(blank[pixel]))
                    / (srgb_to_linear(full[pixel]) - srgb_to_linear(blank[pixel]));
                assert!(
                    (measured - coverage).abs() < 0.02,
                    "linear coverage: expected {coverage}, got {measured}"
                );
            }
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; the growing face masks ink, not material or borders"]
    fn cell_feather_follows_the_clipped_face_without_changing_geometry() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "cell-edge-proof".into(),
        }))
        .unwrap();
        let render = |renderer: &mut HeadlessRenderer, reveal, opacity, clip| {
            renderer
                .render_grid(GridFrame {
                    items: &[GridItemFrame {
                        label: "♖",
                        detail: "White · I",
                        center: [0.; 3],
                        size: [150.; 3],
                        color: [0.9, 0.3, 0.05],
                        fill: [0.03, 0.04, 0.05],
                        presence: 1.,
                        reveal,
                        trim: [0.; 3],
                        label_opacity: 1.,
                        emphasis: 1.,
                        group: false,
                        heading: false,
                        label_style: None,
                        text_disclosure: Some(GridTextDisclosure {
                            opacity,
                            clip,
                            feather_pixels: 8.,
                        }),
                    }],
                    yaw: 0.,
                    pitch: 0.,
                    scale: 1.,
                })
                .unwrap()
        };
        for reveal in [[0.68, 1., 1.], [1., 0.88, 1.], [0.74, 0.83, 1.]] {
            let sharp = render(&mut renderer, reveal, 1., GridTextClip::None);
            let blank = render(&mut renderer, reveal, 0., GridTextClip::None);
            let feather = render(&mut renderer, reveal, 1., GridTextClip::Cell);
            assert_ne!(sharp, feather, "edge must cross actual ink: {reveal:?}");
            for ((sharp, blank), feather) in sharp
                .as_chunks::<4>()
                .0
                .iter()
                .zip(blank.as_chunks::<4>().0)
                .zip(feather.as_chunks::<4>().0)
            {
                if sharp == blank {
                    assert_eq!(sharp, feather, "only ink may change");
                }
            }
        }
        assert_eq!(
            render(&mut renderer, [1.; 3], 1., GridTextClip::None),
            render(&mut renderer, [1.; 3], 1., GridTextClip::Cell),
            "settled ink must be identical"
        );
    }

    #[test]
    fn heading_apertures_keep_retained_ink_visible_through_foreshortening() {
        let mut item = GridItemFrame {
            label: "Bishop",
            detail: "",
            center: [0.; 3],
            size: [120., 40., 0.],
            color: [0.; 3],
            fill: [0.; 3],
            presence: 1.,
            reveal: [1.; 3],
            trim: [0.; 3],
            label_opacity: 1.,
            emphasis: 1.,
            group: false,
            heading: true,
            label_style: None,
            text_disclosure: None,
        };
        for axis in 0..3 {
            for angle in [0., 0.00001, 0.01, 0.3, 0.62] {
                item.text_disclosure = Some(GridTextDisclosure {
                    opacity: 1.,
                    feather_pixels: 8.,
                    clip: GridTextClip::Heading {
                        axis,
                        start: -75.,
                        end: 75.,
                        cell: 150.,
                    },
                });
                let frame = GridFrame {
                    items: &[],
                    yaw: angle,
                    pitch: angle,
                    scale: 0.65,
                };
                let planes = text_clip_planes(&item, &frame, 86.4);
                for x in [-43.2, 43.2] {
                    for y in [-20., 20.] {
                        for plane in planes {
                            let distance = plane[0] * x + plane[1] * y + plane[2];
                            assert!(
                                distance >= 8. / frame.scale - 0.0001,
                                "retained heading faded at {axis}/{angle}: {distance}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; measures the actual visible silhouette through fractional growth and rotation"]
    fn growing_grid_pixels_stay_centered_on_both_axes() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "grid-center".into(),
        }))
        .unwrap();
        let blank = renderer
            .render_grid(GridFrame {
                items: &[],
                yaw: 0.,
                pitch: 0.,
                scale: 1.,
            })
            .unwrap();
        for (extent, yaw, pitch, cut) in [
            ([1., 1., 1.], 0., 0., 0.),
            ([2.37, 1., 1.], 0., 0., 0.),
            ([3., 1.46, 1.], 0.23, 0.18, 0.),
            ([3., 2., 2.71], 0.62, 0.58, 0.),
            ([3., 2., 4.], 0.62, 0.58, 0.),
            ([3., 2., 2.], 0.4, 0.3, 0.65),
        ] {
            let mut items = Vec::new();
            for c in 0..4 {
                for b in 0..2 {
                    for a in 0..3 {
                        let trim = [0., 0., (cut - c as f32).clamp(0., 1.)];
                        let reveal = [
                            (extent[0] - a as f32).clamp(0., 1.),
                            (extent[1] - b as f32).clamp(0., 1.),
                            ((extent[2] - c as f32).clamp(0., 1.) - trim[2]).max(0.),
                        ];
                        items.push(GridItemFrame {
                            label: "",
                            detail: "",
                            center: [
                                a as f32 * 150. - 150.,
                                75. - b as f32 * 150.,
                                225. - c as f32 * 150.,
                            ],
                            size: [150.; 3],
                            color: [0.9, 0.3, 0.05],
                            fill: [0.03, 0.04, 0.05],
                            presence: 1.,
                            reveal,
                            trim,
                            label_opacity: 0.,
                            text_disclosure: None,
                            emphasis: 1.,
                            group: false,
                            heading: false,
                            label_style: None,
                        });
                    }
                }
            }
            let frame = GridFrame {
                items: &items,
                yaw,
                pitch,
                scale: 0.72,
            };
            let pixels = renderer.render_grid(frame).unwrap();
            let mut bounds = [1920, 1080, 0, 0];
            for (index, (pixel, background)) in pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(blank.as_chunks::<4>().0)
                .enumerate()
            {
                if pixel == background {
                    continue;
                }
                let x = index as u32 % 1920;
                let y = index as u32 / 1920;
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
            }
            assert!(
                ((bounds[0] + bounds[2] + 1) as f32 / 2. - 960.).abs() <= 0.75,
                "x {extent:?}: {bounds:?}"
            );
            assert!(
                ((bounds[1] + bounds[3] + 1) as f32 / 2. - 540.).abs() <= 0.75,
                "y {extent:?}: {bounds:?}"
            );
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; verifies real depth occlusion and fractional geometry"]
    fn grid_depth_is_not_draw_order_and_fractional_motion_changes_coverage() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "grid-depth".into(),
        }))
        .unwrap();
        let item = |label, color, z| GridItemFrame {
            label,
            detail: "",
            color,
            center: [0., 0., z],
            size: [200., 160., 80.],
            presence: 1.,
            reveal: [1.; 3],
            trim: [0.; 3],
            fill: [0.03, 0.04, 0.05],
            label_opacity: 1.,
            text_disclosure: None,
            emphasis: 1.,
            group: false,
            heading: false,
            label_style: None,
        };
        let render = |renderer: &mut HeadlessRenderer, items: &[GridItemFrame<'_>]| {
            renderer
                .render_grid(GridFrame {
                    items,
                    yaw: 0.,
                    pitch: 0.,
                    scale: 1.,
                })
                .unwrap()
        };
        let front = render(&mut renderer, &[item("", [0.7, 0.2, 0.1], 200.)]);
        let back = render(&mut renderer, &[item("", [0.1, 0.2, 0.7], -200.)]);
        assert_ne!(front, back);
        for reversed in [false, true] {
            let mut items = vec![
                item("", [0.7, 0.2, 0.1], 200.),
                item("", [0.1, 0.2, 0.7], -200.),
            ];
            if reversed {
                items.reverse();
            }
            assert_eq!(
                front,
                render(&mut renderer, &items),
                "near coincident strokes must win regardless of submission order"
            );
        }
        let blank = render(&mut renderer, &[]);
        let center = (540 * 1920 + 960) * 4;
        assert_ne!(
            &front[center..center + 4],
            &blank[center..center + 4],
            "cell faces must be opaque, not an X-ray of the rear grid"
        );
        let mut moved = item("", [0.7, 0.2, 0.1], 200.);
        moved.reveal[0] = 0.999;
        assert_ne!(
            front,
            render(&mut renderer, &[moved]),
            "fractional growth must survive centering and final window scaling"
        );
        assert_eq!(
            front,
            render(&mut renderer, &[item("", [0.7, 0.2, 0.1], 200.)])
        );
    }
}
