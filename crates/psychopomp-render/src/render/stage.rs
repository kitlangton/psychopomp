//! Stage pixels. The CPU samples channels, projects every element through the
//! camera, and emits depth-sorted signed-distance primitives; the GPU draws them
//! into an HDR target, blooms the bright light, and composites with highlight
//! rolloff, chroma, vignette, and grain (stage.wgsl, stage_post.wgsl).
use std::cell::Cell;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::*;
use crate::footage::StageFootageFrame;
use psychopomp::{
    callout::CalloutSide,
    caption::{CaptionAlign, CaptionSpanPlan},
    effects::combustion::{self, Burst},
    effects::lightning::{self, Discharge},
    effects::shake,
    effects::spinner::{self, Mark},
    effects::surface,
    effects::{dissolve, shield},
    face::Face,
    footage::{Fit, Mask, focus_window},
    math::{
        Quat, Vec2, Vec3,
        curve::{CubicBezier, Polyline},
        easing::{cubic_out, quad_out},
        lerp,
        random::hash,
        remap_clamp,
        shapes::{Box2, Polygon, Port, Shape, connect, fit_between_ports, sphere_ring},
        smoothstep, stops, vec2, vec3,
    },
    stage::{
        Arrow, BoltEnd, Camera, Curve, Figure, Fill, Material, OrbPoint, StageElement, StagePlan,
        StatusText, TRACK, Waypoint, form_points, morph_point, orb_points, packet, shatter_offset,
    },
    tone::Tone,
};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const MAX_PRIMS: usize = 24_000;
const MAX_POINTS: usize = 48_000;
const BLOOM_LEVELS: usize = 5;
/// Text rasterizes at twice its size so camera push-ins stay crisp.
const TEXT_RASTER: f32 = 2.0;
const BEAM_SAMPLES: usize = 48;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct Prim {
    bbox: [f32; 4],
    a: [f32; 4],
    b: [f32; 4],
    fill: [f32; 4],
    stroke: [f32; 4],
    glow: [f32; 4],
    uv: [f32; 4],
    /// Rounded rects: a reflection on the edge (x, y, radius, strength) and a
    /// pool of light in the glass, in the same form.
    light: [f32; 4],
    light_color: [f32; 4],
    pool: [f32; 4],
    pool_color: [f32; 4],
    /// A dissolve over any primitive: age (0 for none), the card's center
    /// x and y, and its scale; its half size in world pixels and field seed;
    /// and the rim's tone.
    mask: [f32; 4],
    mask_shape: [f32; 4],
    mask_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    viewport: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    texel: [f32; 4],
    params: [f32; 4],
    look: [f32; 4],
    shock: [f32; 4],
    rewind: [f32; 4],
    /// Radial zoom streak toward the frame center and white flash (0 for none).
    motion: [f32; 4],
}

struct AtlasText {
    /// x, y, width, height in atlas texels.
    rect: [f32; 4],
}

#[derive(Clone, Copy, PartialEq)]
enum PassKind {
    Accumulate,
    Prefilter,
    Down,
    Up,
    Composite,
}

struct PostPass {
    kind: PassKind,
    bind: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    texel: [f32; 2],
    /// Index into the bloom chain, or `None` for the final target.
    target: Option<usize>,
}

pub(crate) struct StageGpu {
    primitives: wgpu::RenderPipeline,
    primitive_binding: wgpu::BindGroup,
    prims: wgpu::Buffer,
    points: wgpu::Buffer,
    hdr: wgpu::TextureView,
    /// The weighted sum of a frame's shutter samples, in linear HDR light.
    exposure: wgpu::TextureView,
    accumulate: wgpu::RenderPipeline,
    accumulation: PostPass,
    bloom: Vec<wgpu::TextureView>,
    prefilter: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    passes: Vec<PostPass>,
    texts: HashMap<String, AtlasText>,
    orbs: HashMap<String, Vec<OrbPoint>>,
    forms: HashMap<String, FormGeometry>,
    /// Every footage element's frames share one sRGB atlas; each sample
    /// uploads only the slots whose frame changed.
    footage: wgpu::Texture,
    slots: HashMap<String, FootageSlot>,
}

/// Where one footage element's frame lives in the footage atlas, and which
/// frame it holds.
pub(crate) struct FootageSlot {
    origin: [u32; 2],
    size: [u32; 2],
    shown: Cell<Option<u64>>,
}

/// Shelf-pack each footage element's frame into one atlas: its size and
/// every slot, by element.
fn footage_slots(
    sizes: &HashMap<String, [u32; 2]>,
    limit: u32,
) -> Result<([u32; 2], HashMap<String, FootageSlot>)> {
    let mut order = sizes.iter().collect::<Vec<_>>();
    // Tallest first packs shelves tightly; IDs break ties deterministically.
    order.sort_by(|a, b| b.1[1].cmp(&a.1[1]).then(a.0.cmp(b.0)));
    let width = order
        .iter()
        .map(|(_, size)| size[0] + 2)
        .max()
        .unwrap_or(1)
        .max(2048)
        .min(limit);
    let (mut x, mut y, mut row) = (0_u32, 0_u32, 0_u32);
    let mut slots = HashMap::new();
    for (id, size) in order {
        if size[0] > width {
            bail!(
                "footage '{id}' decodes {} pixels wide; the Stage atlas holds {width}",
                size[0]
            );
        }
        if x + size[0] > width {
            x = 0;
            y += row + 2;
            row = 0;
        }
        slots.insert(
            id.clone(),
            FootageSlot {
                origin: [x, y],
                size: *size,
                shown: Cell::new(None),
            },
        );
        x += size[0] + 2;
        row = row.max(size[1]);
    }
    let height = (y + row).max(1);
    if height > limit {
        bail!(
            "stage footage needs a {width}x{height} atlas, past the device's {limit}; lower the clips' resolution"
        );
    }
    Ok(([width, height], slots))
}

/// A form's shapes as matched point sets, prepared once: point `i` of every
/// shape is the same particle.
pub(crate) struct FormGeometry {
    shapes: Vec<Vec<Vec3>>,
    seeds: Vec<Vec3>,
    radii: Vec<f32>,
    tilt: f32,
}

impl FormGeometry {
    fn new(shapes: &[psychopomp::stage::FormShape], points: u32, tilt: f32) -> Self {
        Self {
            shapes: form_points(shapes, points),
            seeds: (0..points)
                .map(|i| vec3(hash(i, 3), hash(i, 7), hash(i, 11)))
                .collect(),
            radii: shapes.iter().map(|shape| shape.radius()).collect(),
            tilt,
        }
    }
}

/// The SVG source of a bundled icon (`psychopomp::stage::ICONS`).
fn bundled_icon(name: &str) -> Option<&'static str> {
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            match name {
                $($name => Some(include_str!(concat!("../../../../assets/icons/", $name, ".svg"))),)*
                _ => None,
            }
        };
    }
    icons!(
        "arrows-clockwise",
        "bell",
        "brain",
        "broadcast",
        "chart-line-up",
        "check-circle",
        "clock",
        "cloud",
        "code",
        "cpu",
        "cube",
        "cursor",
        "cursor-text",
        "database",
        "desktop",
        "device-mobile",
        "envelope",
        "file",
        "fingerprint",
        "folder",
        "gear",
        "git-branch",
        "globe",
        "hand-pointing",
        "hard-drives",
        "hourglass",
        "key",
        "lightning",
        "lock",
        "lock-open",
        "magnifying-glass",
        "package",
        "plug",
        "queue",
        "robot",
        "shield-check",
        "sparkle",
        "stack",
        "terminal",
        "user",
        "users",
        "warning",
        "x-circle",
    )
}

/// An icon element's SVG document: a bundled icon, or its path data filled
/// in a `view`-unit square.
pub(crate) fn icon_svg(icon: &str, path: &str, view: f32) -> Option<String> {
    if icon.is_empty() {
        Some(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {view} {view}"><path d="{path}"/></svg>"#
        ))
    } else {
        bundled_icon(icon).map(str::to_owned)
    }
}

/// Rasterize an icon's coverage at `pixels` square, for the atlas.
fn icon_sprite(svg: &str, pixels: u32) -> Result<TextSprite> {
    rasterize_svg(svg, pixels, pixels)
}

/// A shader source: the file under `PSYCHOPOMP_SHADER_DIR` when set (live
/// editing without recompiling), otherwise the compiled-in copy.
fn shader(file: &str, builtin: &'static str) -> std::borrow::Cow<'static, str> {
    if let Some(dir) = std::env::var_os("PSYCHOPOMP_SHADER_DIR")
        && let Ok(source) = std::fs::read_to_string(std::path::Path::new(&dir).join(file))
    {
        return source.into();
    }
    builtin.into()
}

/// The modules joined, in order, into the Stage primitive shader: binding-free
/// effect modules first, then the shader that calls them.
pub(crate) const PRIMITIVE_SHADER: [(&str, &str); 7] = [
    ("effects/noise.wgsl", include_str!("effects/noise.wgsl")),
    (
        "effects/combustion.wgsl",
        include_str!("effects/combustion.wgsl"),
    ),
    (
        "effects/lightning.wgsl",
        include_str!("effects/lightning.wgsl"),
    ),
    (
        "effects/dissolve.wgsl",
        include_str!("effects/dissolve.wgsl"),
    ),
    ("effects/shield.wgsl", include_str!("effects/shield.wgsl")),
    ("effects/scan.wgsl", include_str!("effects/scan.wgsl")),
    ("stage.wgsl", include_str!("stage.wgsl")),
];

/// The modules joined, in order, into the Stage post-processing shader.
pub(crate) const POST_SHADER: [(&str, &str); 4] = [
    ("effects/noise.wgsl", include_str!("effects/noise.wgsl")),
    (
        "effects/pressure.wgsl",
        include_str!("effects/pressure.wgsl"),
    ),
    ("effects/rewind.wgsl", include_str!("effects/rewind.wgsl")),
    ("stage_post.wgsl", include_str!("stage_post.wgsl")),
];

/// Byte sizes of the Rust structs the Stage shaders read, by WGSL name.
#[cfg(test)]
pub(crate) const SHADER_STRUCTS: [(&str, usize); 3] = [
    ("Prim", std::mem::size_of::<Prim>()),
    ("Globals", std::mem::size_of::<Globals>()),
    ("Post", std::mem::size_of::<PostUniform>()),
];

/// One shader from its modules, each read live under `PSYCHOPOMP_SHADER_DIR`.
fn compose(modules: &[(&str, &'static str)]) -> String {
    modules
        .iter()
        .map(|(file, builtin)| shader(file, builtin))
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_key(element: &str, part: &str) -> String {
    format!("{element}#{part}")
}

impl HeadlessRenderer {
    #[cfg(test)]
    pub(crate) fn prepare_stage(&mut self, plan: &StagePlan) -> Result<StageGpu> {
        self.prepare_stage_with(plan, &HashMap::new())
    }

    /// Prepare a Stage whose footage elements decode at `footage` sizes.
    pub(crate) fn prepare_stage_with(
        &mut self,
        plan: &StagePlan,
        footage: &HashMap<String, [u32; 2]>,
    ) -> Result<StageGpu> {
        let texts = self.stage_atlas(plan)?;
        let (atlas_view, atlas_size, rects) = texts;
        let device = &self.device;
        let (width, height) = (self.spec.width, self.spec.height);
        let texture = |label: &str, size: [u32; 2], format: wgpu::TextureFormat| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0].max(1),
                        height: size[1].max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let hdr = texture("stage HDR", [width, height], HDR_FORMAT);
        let exposure = texture("stage exposure", [width, height], HDR_FORMAT);
        let sizes = (0..BLOOM_LEVELS)
            .map(|level| [width >> (level + 1), height >> (level + 1)])
            .collect::<Vec<_>>();
        let bloom = sizes
            .iter()
            .map(|size| texture("stage bloom level", *size, HDR_FORMAT))
            .collect::<Vec<_>>();
        let limit = device.limits().max_texture_dimension_2d;
        let (atlas_extent, slots) = footage_slots(footage, limit)?;
        let footage = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stage footage atlas"),
            size: wgpu::Extent3d {
                width: atlas_extent[0],
                height: atlas_extent[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let footage_view = footage.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stage linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let buffer = |label: &str, size: usize, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals = buffer(
            "stage globals",
            std::mem::size_of::<Globals>(),
            wgpu::BufferUsages::UNIFORM,
        );
        let prims = buffer(
            "stage primitives",
            MAX_PRIMS * std::mem::size_of::<Prim>(),
            wgpu::BufferUsages::STORAGE,
        );
        let points = buffer(
            "stage polyline points",
            MAX_POINTS * 16,
            wgpu::BufferUsages::STORAGE,
        );
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let uniform_ty = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let storage_ty = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let texture_ty = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let sampler_ty = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let primitive_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage primitives"),
            entries: &[
                entry(0, both, uniform_ty),
                entry(1, both, storage_ty),
                entry(2, fragment, storage_ty),
                entry(3, fragment, texture_ty),
                entry(4, fragment, sampler_ty),
                entry(5, fragment, texture_ty),
            ],
        });
        let primitive_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage primitives"),
            layout: &primitive_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: prims.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: points.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&footage_view),
                },
            ],
        });
        let primitive_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stage primitives"),
            source: wgpu::ShaderSource::Wgsl(compose(&PRIMITIVE_SHADER).into()),
        });
        let premultiplied = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let primitives = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stage primitives"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&primitive_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &primitive_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &primitive_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(premultiplied),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage post"),
            entries: &[
                entry(0, fragment, uniform_ty),
                entry(1, fragment, texture_ty),
                entry(2, fragment, sampler_ty),
                entry(3, fragment, texture_ty),
            ],
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stage post"),
            source: wgpu::ShaderSource::Wgsl(compose(&POST_SHADER).into()),
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&post_layout)],
            immediate_size: 0,
        });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let post = |entry_point: &str, format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &post_shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let accumulate = post("accumulate", HDR_FORMAT, Some(additive));
        let prefilter = post("prefilter", HDR_FORMAT, None);
        let down = post("down", HDR_FORMAT, None);
        let up = post("up", HDR_FORMAT, Some(additive));
        let composite = post("composite", FORMAT, None);

        // Passes: prefilter HDR → level 0, down 0→1…, up …→0, composite.
        let pass = |kind,
                    source: &wgpu::TextureView,
                    second: &wgpu::TextureView,
                    texel: [f32; 2],
                    target| {
            let uniform = buffer(
                "stage post pass",
                std::mem::size_of::<PostUniform>(),
                wgpu::BufferUsages::UNIFORM,
            );
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("stage post pass"),
                layout: &post_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(second),
                    },
                ],
            });
            PostPass {
                kind,
                bind,
                uniform,
                texel,
                target,
            }
        };
        let texel = |size: [u32; 2]| [1.0 / size[0].max(1) as f32, 1.0 / size[1].max(1) as f32];
        // The second texture is only read by the composite; other passes bind the
        // atlas there so no pass reads the texture it writes.
        // Each shutter sample adds the HDR target into the exposure; bloom and
        // the composite then read the finished exposure once.
        let accumulation = pass(
            PassKind::Accumulate,
            &hdr,
            &atlas_view,
            texel([width, height]),
            None,
        );
        let mut passes = vec![pass(
            PassKind::Prefilter,
            &exposure,
            &atlas_view,
            texel([width, height]),
            Some(0),
        )];
        for level in 1..BLOOM_LEVELS {
            passes.push(pass(
                PassKind::Down,
                &bloom[level - 1],
                &atlas_view,
                texel(sizes[level - 1]),
                Some(level),
            ));
        }
        for level in (1..BLOOM_LEVELS).rev() {
            passes.push(pass(
                PassKind::Up,
                &bloom[level],
                &atlas_view,
                texel(sizes[level]),
                Some(level - 1),
            ));
        }
        passes.push(pass(
            PassKind::Composite,
            &exposure,
            &bloom[0],
            texel([width, height]),
            None,
        ));

        self.queue.write_buffer(
            &globals,
            0,
            bytemuck::bytes_of(&Globals {
                viewport: [width as f32, height as f32, atlas_size[0], atlas_size[1]],
            }),
        );
        let orbs = plan
            .elements
            .iter()
            .filter_map(|element| match element {
                StageElement::Orb { id, points, .. } => Some((id.clone(), orb_points(*points))),
                _ => None,
            })
            .collect();
        let forms = plan
            .elements
            .iter()
            .filter_map(|element| match element {
                StageElement::Form {
                    id,
                    shapes,
                    points,
                    tilt,
                    ..
                } => Some((id.clone(), FormGeometry::new(shapes, *points, *tilt))),
                _ => None,
            })
            .collect();
        Ok(StageGpu {
            primitives,
            primitive_binding,
            prims,
            points,
            hdr,
            exposure,
            accumulate,
            accumulation,
            bloom,
            prefilter,
            down,
            up,
            composite,
            passes,
            texts: rects,
            orbs,
            forms,
            footage,
            slots,
        })
    }

    /// Rasterize every string the stage can show into one R8 coverage atlas.
    fn stage_atlas(
        &mut self,
        plan: &StagePlan,
    ) -> Result<(wgpu::TextureView, [f32; 2], HashMap<String, AtlasText>)> {
        let mut strings: Vec<(String, String, f32, Face)> = Vec::new();
        for element in &plan.elements {
            match element {
                StageElement::Card {
                    id, title, status, ..
                } => {
                    strings.push((text_key(id, "title"), title.clone(), 26.0, Face::Mono));
                    for (index, entry) in status.iter().enumerate() {
                        strings.push((
                            text_key(id, &format!("status{index}")),
                            entry.text.clone(),
                            18.0,
                            Face::Mono,
                        ));
                    }
                }
                StageElement::Packet { id, label, .. } if !label.is_empty() => {
                    strings.push((text_key(id, "label"), label.clone(), 19.0, Face::Mono));
                }
                StageElement::Label {
                    id,
                    size,
                    spans,
                    face,
                    ..
                } => {
                    for (index, span) in spans.iter().enumerate() {
                        if !span.text.is_empty() {
                            strings.push((
                                text_key(id, &format!("span{index}")),
                                span.text.clone(),
                                *size,
                                *face,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        let mut cache: HashMap<(&str, u32, Face), TextSprite> = HashMap::new();
        let mut sprites = strings
            .iter()
            .map(|(_, text, size, face)| {
                cache
                    .entry((text.as_str(), size.to_bits(), *face))
                    .or_insert_with(|| {
                        let raster = size * TEXT_RASTER;
                        let line = (raster * 1.35).ceil();
                        let attrs = fonts::attrs(*face).color(Color::rgb(255, 255, 255));
                        let width =
                            ((text.chars().count() as f32 * raster * 0.7) as u32 + 64).min(4096);
                        make_sprite(
                            &mut self.font_system,
                            &mut self.swash_cache,
                            vec![(text.as_str(), attrs.clone())],
                            attrs,
                            Metrics::new(raster, line),
                            width,
                            line as u32,
                        )
                    })
                    .clone()
            })
            .collect::<Vec<_>>();
        // Icons follow the text, rasterized at the same density.
        let mut keys = strings.into_iter().map(|(key, ..)| key).collect::<Vec<_>>();
        for element in &plan.elements {
            if let StageElement::Icon {
                id,
                size,
                icon,
                path,
                view,
                ..
            } = element
            {
                let svg = icon_svg(icon, path, *view)
                    .with_context(|| format!("icon '{id}' names no bundled icon"))?;
                let pixels = ((size * TEXT_RASTER).ceil() as u32).clamp(4, 2048);
                sprites
                    .push(icon_sprite(&svg, pixels).with_context(|| format!("icon '{id}' SVG"))?);
                keys.push(text_key(id, "icon"));
            }
        }
        // Shelf packing, rows of the tallest sprite.
        let atlas_width = 4096_u32;
        let mut placements = Vec::with_capacity(sprites.len());
        let (mut x, mut y, mut row) = (1_u32, 1_u32, 0_u32);
        for sprite in &sprites {
            let w = (sprite.advance.ceil() as u32 + 4).min(sprite.width);
            if x + w + 1 > atlas_width {
                x = 1;
                y += row + 2;
                row = 0;
            }
            placements.push((x, y, w, sprite.height));
            x += w + 2;
            row = row.max(sprite.height);
        }
        let atlas_height = (y + row + 2).max(4).next_power_of_two();
        if atlas_height > self.device.limits().max_texture_dimension_2d {
            bail!("stage text atlas exceeds device limits");
        }
        let mut pixels = vec![0_u8; (atlas_width * atlas_height) as usize];
        for (sprite, &(px, py, w, h)) in sprites.iter().zip(&placements) {
            for row in 0..h {
                for column in 0..w {
                    pixels[((py + row) * atlas_width + px + column) as usize] =
                        sprite.pixels[((row * sprite.width + column) * 4 + 3) as usize];
                }
            }
        }
        let atlas = upload_r8(
            &self.device,
            &self.queue,
            "stage text atlas",
            [atlas_width, atlas_height],
            &pixels,
        );
        let rects = keys
            .into_iter()
            .zip(placements)
            .map(|(key, (x, y, w, h))| {
                (
                    key,
                    AtlasText {
                        rect: [x as f32, y as f32, w as f32, h as f32],
                    },
                )
            })
            .collect();
        Ok((atlas, [atlas_width as f32, atlas_height as f32], rects))
    }

    #[cfg(test)]
    pub(crate) fn render_stage(
        &mut self,
        plan: &StagePlan,
        gpu: &StageGpu,
        time: f64,
        value: impl Fn(&str, f32) -> f32,
    ) -> Result<Vec<u8>> {
        self.render_stage_exposure(
            plan,
            gpu,
            &[(time, 1.0)],
            |_, property, default| value(property, default),
            |_| Ok(Vec::new()),
        )
    }

    /// Upload each footage frame whose slot holds another.
    fn upload_footage(&mut self, gpu: &StageGpu, frames: &[StageFootageFrame]) {
        for frame in frames {
            let Some(slot) = gpu.slots.get(&frame.element) else {
                continue;
            };
            if slot.shown.get() == Some(frame.identity) || frame.size != slot.size {
                continue;
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu.footage,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: slot.origin[0],
                        y: slot.origin[1],
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(slot.size[0] * 4),
                    rows_per_image: Some(slot.size[1]),
                },
                wgpu::Extent3d {
                    width: slot.size[0],
                    height: slot.size[1],
                    depth_or_array_layers: 1,
                },
            );
            slot.shown.set(Some(frame.identity));
        }
    }

    /// One frame exposed through weighted shutter samples. Each sample's
    /// light adds into a linear HDR exposure on the GPU, so a bright streak
    /// keeps its energy; bloom, rolloff, and grain then develop the frame
    /// once, with the post settings of its central sample.
    pub(crate) fn render_stage_exposure(
        &mut self,
        plan: &StagePlan,
        gpu: &StageGpu,
        exposure: &[(f64, f32)],
        value: impl Fn(f64, &str, f32) -> f32,
        footage: impl Fn(f64) -> Result<Vec<StageFootageFrame>>,
    ) -> Result<Vec<u8>> {
        let look = Look::new(self.theme);
        let size = vec2(self.spec.width as f32, self.spec.height as f32);
        let mean = exposure
            .iter()
            .map(|(time, weight)| time * f64::from(*weight))
            .sum::<f64>()
            / exposure
                .iter()
                .map(|(_, weight)| f64::from(*weight))
                .sum::<f64>()
                .max(1e-9);
        let central = exposure
            .iter()
            .map(|(time, _)| *time)
            .min_by(|a, b| (a - mean).abs().total_cmp(&(b - mean).abs()))
            .context("an exposure needs at least one sample")?;
        let mut post = [[0.0; 4]; 5];
        for (index, &(time, weight)) in exposure.iter().enumerate() {
            let value = |property: &str, default: f32| value(time, property, default);
            // Writes land before the next submission, so each sample's
            // frames are in the atlas when its primitives draw.
            if !gpu.slots.is_empty() {
                let frames = footage(time)?;
                self.upload_footage(gpu, &frames);
            }
            let scene = Scene::sample_with(plan, &value, time as f32, size, Some(&gpu.forms));
            let mut painter = Painter {
                scene: &scene,
                look,
                orbs: &gpu.orbs,
                footage: &gpu.slots,
                frame: StageFrame::new(&gpu.texts, look.background),
            };
            painter.backdrop(plan.post.backdrop);
            for (order, element) in plan.elements.iter().enumerate() {
                painter.element(order, element);
            }
            let (prims, points) = painter.frame.finish()?;
            if time == central {
                post = post_settings(plan, &scene, &value, look, time);
            }
            let exposure = [weight, scene.camera.roll, scene.camera.cover()];
            self.draw_sample(gpu, &prims, &points, look.background, exposure, index == 0);
        }
        self.develop(gpu, post)
    }

    /// Draw one sample's primitives into the HDR target and add it, weighted,
    /// into the exposure (cleared by the first sample). `exposure` is the
    /// weight, then the camera roll and its covering magnification, which turn
    /// this sample's image as it is added so a rolling camera blurs too.
    fn draw_sample(
        &mut self,
        gpu: &StageGpu,
        prims: &[Prim],
        points: &[[f32; 4]],
        clear: Vec3,
        [weight, roll, cover]: [f32; 3],
        first: bool,
    ) {
        self.queue
            .write_buffer(&gpu.prims, 0, bytemuck::cast_slice(prims));
        if !points.is_empty() {
            self.queue
                .write_buffer(&gpu.points, 0, bytemuck::cast_slice(points));
        }
        self.queue.write_buffer(
            &gpu.accumulation.uniform,
            0,
            bytemuck::bytes_of(&PostUniform {
                texel: [0.0; 4],
                params: [weight, roll, cover, 0.0],
                look: [0.0; 4],
                shock: [0.0; 4],
                rewind: [0.0; 4],
                motion: [0.0; 4],
            }),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage sample"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stage primitives"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &gpu.hdr,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(clear.x),
                            g: f64::from(clear.y),
                            b: f64::from(clear.z),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&gpu.primitives);
            pass.set_bind_group(0, &gpu.primitive_binding, &[]);
            pass.draw(0..4, 0..prims.len() as u32);
        }
        fullscreen(
            &mut encoder,
            &gpu.exposure,
            &gpu.accumulate,
            &gpu.accumulation.bind,
            if first {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            } else {
                wgpu::LoadOp::Load
            },
        );
        // Buffer writes land before their submission runs, so each sample
        // submits before the next overwrites its primitives.
        self.queue.submit([encoder.finish()]);
    }

    /// Bloom and composite the finished exposure into the frame. `post` is
    /// the bloom parameters and the composite look.
    fn develop(&mut self, gpu: &StageGpu, post: [[f32; 4]; 5]) -> Result<Vec<u8>> {
        for pass in &gpu.passes {
            self.queue.write_buffer(
                &pass.uniform,
                0,
                bytemuck::bytes_of(&PostUniform {
                    texel: [pass.texel[0], pass.texel[1], 0.0, 0.0],
                    params: post[0],
                    look: post[1],
                    shock: post[2],
                    rewind: post[3],
                    motion: post[4],
                }),
            );
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage develop"),
            });
        for post in &gpu.passes {
            let target = post.target.map_or(&self.view, |level| &gpu.bloom[level]);
            let pipeline = match post.kind {
                PassKind::Accumulate => &gpu.accumulate,
                PassKind::Prefilter => &gpu.prefilter,
                PassKind::Down => &gpu.down,
                PassKind::Up => &gpu.up,
                PassKind::Composite => &gpu.composite,
            };
            let load = if post.kind == PassKind::Up {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            };
            fullscreen(&mut encoder, target, pipeline, &post.bind, load);
        }
        self.read_frame(encoder)
    }
}

/// One fullscreen triangle into `target`.
fn fullscreen(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
    load: wgpu::LoadOp<wgpu::Color>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("stage post"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.draw(0..3, 0..1);
}

/// Where `edge` of a positioned element lands on the delivered frame at
/// `time`: its outline as beams see it, the camera's authored roll, then the
/// develop pass's shake roll and punch-in about the frame center, so an
/// overlay pinned there moves with the element through camera moves, jolts,
/// and settles. The develop pass uses its
/// exposure's central sample; an overlay uses its own sample, which differs by
/// far less than a pixel within one shutter.
pub(crate) fn stage_anchor(
    plan: &StagePlan,
    value: &dyn Fn(&str, f32) -> f32,
    time: f64,
    size: Vec2,
    element: &str,
    edge: CalloutSide,
) -> Option<Vec2> {
    let element = plan.element(element)?;
    let scene = Scene::camera(plan, value, time as f32, size);
    // The authored roll turns each sample as it is exposed.
    let point = scene.camera.rolled(edge.on(scene.place(element)?.outline));
    let roll = shake::rumble(time as f32, trauma(plan, value)).roll;
    // Mirrors `composite` in stage_post.wgsl, which samples the inverse.
    let zoom = 1.0 + read(plan, value, "camera.punch").max(0.0) + roll.abs() * 0.6;
    let center = size * 0.5;
    Some(center + Vec2::from_angle(roll).rotate(point - center) * zoom)
}

/// Camera trauma: a jolt's decaying `shake` plus a scene's sustained `quake`.
fn trauma(plan: &StagePlan, value: &dyn Fn(&str, f32) -> f32) -> f32 {
    read(plan, value, "camera.shake").max(0.0) + read(plan, value, "camera.quake").max(0.0)
}

/// A Stage channel's value, or its default from the one Stage channel
/// defaults table when nothing writes it, so authoring and pixels agree.
fn read(plan: &StagePlan, value: &dyn Fn(&str, f32) -> f32, property: &str) -> f32 {
    let default = plan.channel_default(property);
    debug_assert!(
        default.is_some(),
        "the Stage reads '{property}' without a channel default"
    );
    value(property, default.unwrap_or(0.0))
}

/// Bloom, look, pressure wave, rewind, zoom-streak, and flash settings at one sample.
fn post_settings(
    plan: &StagePlan,
    scene: &Scene,
    value: &dyn Fn(&str, f32) -> f32,
    look: Look,
    time: f64,
) -> [[f32; 4]; 5] {
    let params = [
        read(plan, value, "post.bloom").max(0.0),
        0.95,
        0.25,
        read(plan, value, "post.exposure").max(0.0),
    ];
    let grade = [
        read(plan, value, "post.chroma").max(0.0),
        read(plan, value, "post.vignette").clamp(0.0, 1.0),
        plan.post.grain,
        ((time * 60.0).floor() % 997.0) as f32,
    ];
    let shock = plan
        .elements
        .iter()
        .find_map(|element| {
            let (StageElement::Orb { id, .. } | StageElement::Form { id, .. }) = element else {
                return None;
            };
            let age = scene.v(id, "burst");
            let place = scene.placements.get(id.as_str())?;
            let center = scene.camera.rolled(place.center);
            (0.0..2.4)
                .contains(&age)
                .then_some([center.x, center.y, age, place.scale])
        })
        .unwrap_or([0.0, 0.0, -1.0, 0.0]);
    // Roll and the punch-in transform the whole developed frame.
    let rewind = [
        read(plan, value, "post.rewind"),
        look.background.dot(Vec3::new(0.2126, 0.7152, 0.0722)),
        shake::rumble(time as f32, trauma(plan, value)).roll,
        read(plan, value, "camera.punch").max(0.0),
    ];
    let motion = [
        read(plan, value, "post.zoom").clamp(0.0, 0.5),
        read(plan, value, "post.flash").clamp(0.0, 1.0),
        0.0,
        0.0,
    ];
    [params, grade, shock, rewind, motion]
}

/// The theme's colors in linear light.
#[derive(Clone, Copy)]
struct Look {
    theme: Theme,
    background: Vec3,
    surface: Vec3,
    raised: Vec3,
    text: Vec3,
    muted: Vec3,
}

impl Look {
    fn new(theme: Theme) -> Self {
        let palette = theme.palette();
        Self {
            theme,
            background: linear3(palette.background),
            surface: linear3(palette.surface),
            raised: linear3(palette.raised),
            text: linear3(palette.text),
            muted: linear3(palette.muted),
        }
    }

    fn tone(&self, tone: Tone) -> Vec3 {
        linear3(self.theme.tone(tone))
    }
}

fn linear3(rgb: [u8; 3]) -> Vec3 {
    Vec3::from(super::theme::linear(rgb))
}

/// Where a positioned element sits this sample.
#[derive(Clone, Copy)]
struct Placement {
    world: Vec3,
    /// Depth for draw order and depth of field (`Camera::depth`): its world z
    /// unless the camera is turned.
    depth: f32,
    /// Projected center on screen.
    center: Vec2,
    /// Perspective times the element's own scale (and an orb's breath).
    scale: f32,
    /// The on-screen outline beams attach to.
    outline: Shape,
}

/// A beam's path on screen and what sits at its ends.
struct Link {
    path: Polyline,
    /// Full visible body boundaries, not an orb's submerged wire endpoints.
    label_ports: [Port; 2],
    /// Depth (`Camera::depth`) and on-screen scale at the `from` and `to` ends.
    depth: [f32; 2],
    scale: [f32; 2],
    /// Ends that plug into a card side and show a socket there.
    socket: [bool; 2],
    /// Open ends, where an arriving packet's landing ring shows and its label
    /// holds until arrival: card sides, shapes, icons, and a path's free ends.
    /// A body's submerged end absorbs arrivals out of sight.
    landing: [bool; 2],
    /// Fractions of the path where its visible line meets each end's outline:
    /// arrowheads sit there, not at a body's submerged endpoint.
    tips: [f32; 2],
    /// The source card's frame, for the light that sweeps it before drawing.
    source: Option<Box2>,
}

impl Link {
    fn far(&self) -> f32 {
        self.depth[0].max(self.depth[1])
    }

    fn scale_at(&self, fraction: f32) -> f32 {
        lerp(self.scale[0], self.scale[1], fraction)
    }

    /// The link as a packet travels it: from its `to` end when `reverse`.
    fn toward(&self, reverse: bool) -> Link {
        fn ends<T: Copy>([a, b]: [T; 2], reverse: bool) -> [T; 2] {
            if reverse { [b, a] } else { [a, b] }
        }
        Link {
            path: if reverse {
                self.path.reversed()
            } else {
                self.path.clone()
            },
            depth: ends(self.depth, reverse),
            scale: ends(self.scale, reverse),
            socket: ends(self.socket, reverse),
            landing: ends(self.landing, reverse),
            tips: if reverse {
                [1.0 - self.tips[1], 1.0 - self.tips[0]]
            } else {
                self.tips
            },
            label_ports: ends(self.label_ports, reverse),
            source: if reverse { None } else { self.source },
        }
    }
}

/// Where a bolt's ends are this sample.
#[derive(Clone, Copy)]
struct BoltEnds {
    from: Port,
    to: Port,
    scale: [f32; 2],
    depth: [f32; 2],
}

impl BoltEnds {
    fn scale(&self) -> f32 {
        (self.scale[0] + self.scale[1]) * 0.5
    }

    fn near(&self) -> f32 {
        self.depth[0].min(self.depth[1])
    }
}

/// Something touching a shield's surface: where, its direction on the
/// sphere (toward the viewer is -z), seconds since contact, and strength.
#[derive(Clone, Copy)]
struct Contact {
    point: Vec2,
    direction: Vec3,
    age: f32,
    strength: f32,
}

/// A closed card or orb outline, sampled by fraction of its perimeter.
enum Perimeter {
    Path(Polyline),
    Round(psychopomp::math::shapes::Circle),
}

impl Perimeter {
    fn of(outline: Shape, scale: f32) -> Option<Self> {
        match outline {
            Shape::Box(frame) => Some(Self::Path(
                frame.perimeter_from(vec2(frame.center().x, frame.min.y), 14.0 * scale),
            )),
            Shape::Circle(circle) => Some(Self::Round(circle)),
            Shape::Polygon(polygon) => {
                let vertices = polygon.vertices();
                let closed = vertices.iter().chain(vertices.first()).copied().collect();
                Some(Self::Path(Polyline::new(closed)))
            }
            Shape::Point(_) => None,
        }
    }

    fn at(&self, fraction: f32) -> Vec2 {
        match self {
            Self::Path(path) => path.at(fraction.rem_euclid(1.0)),
            Self::Round(circle) => lightning::on_circle(circle.center, circle.radius, fraction),
        }
    }

    fn length(&self) -> f32 {
        match self {
            Self::Path(path) => path.length(),
            Self::Round(circle) => TAU * circle.radius,
        }
    }
}

/// A stable 32-bit salt for an element ID (FNV-1a).
fn salt(id: &str) -> u32 {
    id.bytes().fold(0x811C_9DC5_u32, |h, byte| {
        (h ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// Electricity and scans on a plain card take the request tone.
fn spark_tone(tone: Tone) -> Tone {
    if tone == Tone::Plain {
        Tone::Request
    } else {
        tone
    }
}

/// A sampled particle form: every point's offset from the center (turned
/// and scaled, before shatter or collapse) and the silhouette wires attach to.
struct SampledForm {
    /// Offset in world pixels, nearness to the camera (0..1), and seeds.
    dots: Vec<(Vec3, f32, Vec3)>,
    /// Bounding radius of the current shape, in world pixels.
    radius: f32,
    /// The projected silhouette at rest.
    hull: Polygon,
}

/// Light cast by something that moves. A reflection lights only the edges it
/// nears (a packet gathering, flying, and landing, or a drawing beam's bead); a
/// pool also enters a card's glass (the ember left at a port, the flood where a
/// packet arrives, a beam's surge on contact).
#[derive(Clone, Copy)]
struct Light {
    at: Vec2,
    tone: Tone,
    strength: f32,
    /// Falloff radius in pixels at unit scale.
    radius: f32,
    pool: bool,
    scale: f32,
}

impl Light {
    /// How strongly it lights a point `distance` pixels away.
    fn falloff(&self, distance: f32) -> f32 {
        let r = distance / (self.radius * self.scale).max(1.0);
        if self.pool {
            0.5 * (-2.0 * r * r).exp()
        } else {
            stops(r, &REFLECTION)
        }
    }

    /// The light in the form a primitive carries it.
    fn uniform(&self, look: &Look, opacity: f32) -> ([f32; 4], [f32; 4]) {
        (
            [
                self.at.x,
                self.at.y,
                self.radius * self.scale,
                self.strength * opacity,
            ],
            rgba(look.tone(self.tone), 1.0),
        )
    }
}

/// The diagrams' reflection: full at the light, 0.65 at 0.3 of its radius,
/// 0.16 at 0.7, gone at the radius. Mirrored in `stage.wgsl`.
const REFLECTION: [(f32, f32); 4] = [(0.0, 1.0), (0.3, 0.65), (0.7, 0.16), (1.0, 0.0)];
/// Diagram pixels to stage pixels: the diagrams sit about this much smaller
/// than a 1080p frame.
const DIAGRAM_SCALE: f32 = 1.4;
const REFLECTION_RADIUS: f32 = 80.0 * DIAGRAM_SCALE;

/// Opacity of a bead that travels a path: born as it leaves one end, gone as
/// it reaches the other.
fn bead(progress: f32) -> f32 {
    smoothstep(progress / 0.08) * (1.0 - smoothstep((progress - 0.92) / 0.08))
}

/// Channel values, camera, placements, and beam paths of one sample.
struct Scene<'a> {
    plan: &'a StagePlan,
    value: &'a dyn Fn(&str, f32) -> f32,
    time: f32,
    camera: Camera,
    focus: f32,
    dof: f32,
    placements: HashMap<&'a str, Placement>,
    links: HashMap<&'a str, Link>,
    /// Each path's legs, split at its stops.
    routes: HashMap<&'a str, Vec<Link>>,
    forms: HashMap<&'a str, SampledForm>,
    lights: Vec<Light>,
}

impl<'a> Scene<'a> {
    /// The sample's channels and camera, before anything is placed.
    fn camera(
        plan: &'a StagePlan,
        value: &'a dyn Fn(&str, f32) -> f32,
        time: f32,
        size: Vec2,
    ) -> Self {
        // A held camera sways slowly; a jolt's spring-loaded shove and trauma
        // rumble add on top. All are sampled per shutter sample, so they blur.
        let sway = shake::handheld(time, read(plan, value, "camera.handheld"));
        let authored = Camera {
            position: vec3(
                read(plan, value, "camera.x"),
                read(plan, value, "camera.y"),
                read(plan, value, "camera.z"),
            ),
            yaw: read(plan, value, "camera.yaw") + sway.yaw,
            pitch: (read(plan, value, "camera.pitch") + sway.pitch).clamp(-1.45, 1.45),
            roll: read(plan, value, "camera.roll"),
            zoom: read(plan, value, "camera.zoom").max(0.05),
            pivot: read(plan, value, "camera.pivot"),
            size,
        };
        let mut scene = Self {
            plan,
            value,
            time,
            camera: authored,
            focus: read(plan, value, "camera.focus"),
            dof: read(plan, value, "camera.dof").max(0.0),
            placements: HashMap::new(),
            links: HashMap::new(),
            routes: HashMap::new(),
            forms: HashMap::new(),
            lights: Vec::new(),
        };
        let pan = scene.tracked();
        let rumble = shake::rumble(time, trauma(plan, value));
        scene.camera.position = vec3(
            pan.x + read(plan, value, "camera.kick-x") + rumble.offset.x,
            pan.y + read(plan, value, "camera.kick-y") + rumble.offset.y,
            authored.position.z,
        );
        if sway.offset != Vec2::ZERO {
            scene.camera.position += sway.offset.extend(0.0);
        }
        scene
    }

    /// The authored pan, blended toward the pans that center each followed
    /// element or packet by its `camera.track.<id>` weight. Weights summing
    /// past 1 share the frame. A packet is found where this very sample draws
    /// it, so following is exact: a few passes settle the parallax between
    /// ends at different depths.
    fn tracked(&self) -> Vec2 {
        let base = self.camera.position.truncate();
        let tracks = self
            .plan
            .elements
            .iter()
            .filter(|element| element.followable())
            .filter_map(|element| {
                let weight = read(self.plan, self.value, &format!("{TRACK}{}", element.id()));
                (weight > 0.0).then_some((element, weight))
            })
            .collect::<Vec<_>>();
        if tracks.is_empty() {
            return base;
        }
        let total = tracks.iter().map(|(_, weight)| weight).sum::<f32>();
        let share = total.max(1.0);
        let mut pan = base;
        for _ in 0..3 {
            let mut probe = Scene {
                camera: Camera {
                    position: pan.extend(self.camera.position.z),
                    ..self.camera
                },
                placements: HashMap::new(),
                links: HashMap::new(),
                routes: HashMap::new(),
                forms: HashMap::new(),
                lights: Vec::new(),
                ..*self
            };
            probe.placements = self
                .plan
                .elements
                .iter()
                .filter_map(|element| Some((element.id(), probe.place(element)?)))
                .collect();
            let aims = tracks.iter().map(|(element, weight)| {
                let aim = probe
                    .followed(element)
                    .map_or(pan, |point| probe.camera.aim(point));
                aim * (weight / share)
            });
            pan = base * (1.0 - total / share) + aims.sum::<Vec2>();
        }
        pan
    }

    /// The world point the camera follows for `element`: a packet's head
    /// where it is drawn along its beam, or a positioned element's center.
    fn followed(&self, element: &StageElement) -> Option<Vec3> {
        match element {
            StageElement::Packet {
                id, beam, reverse, ..
            } => {
                let StageElement::Beam { from, to, bend, .. } = self.plan.element(beam)? else {
                    return None;
                };
                let link = self.link(beam, from, to, *bend)?.toward(*reverse);
                let travel = packet::travel(self.v(id, "age"), self.v(id, "flight").max(0.05));
                let depth = lerp(link.depth[0], link.depth[1], travel);
                Some(self.camera.unproject(link.path.at(travel), depth))
            }
            element => self.placements.get(element.id()).map(|place| place.world),
        }
    }

    #[cfg(test)]
    fn sample(
        plan: &'a StagePlan,
        value: &'a dyn Fn(&str, f32) -> f32,
        time: f32,
        size: Vec2,
    ) -> Self {
        Self::sample_with(plan, value, time, size, None)
    }

    /// Everything placed for one sample. Forms need their prepared geometry.
    fn sample_with(
        plan: &'a StagePlan,
        value: &'a dyn Fn(&str, f32) -> f32,
        time: f32,
        size: Vec2,
        geometry: Option<&HashMap<String, FormGeometry>>,
    ) -> Self {
        let mut scene = Self::camera(plan, value, time, size);
        scene.placements = plan
            .elements
            .iter()
            .filter_map(|element| Some((element.id(), scene.place(element)?)))
            .collect();
        // A form attaches to its sampled, turned silhouette.
        for (id, geometry) in geometry.into_iter().flatten() {
            let Some(place) = scene.placements.get(id.as_str()).copied() else {
                continue;
            };
            let Some(element) = plan.element(id) else {
                continue;
            };
            let form = scene.form(element.id(), geometry, place);
            if let Some(placement) = scene.placements.get_mut(element.id()) {
                placement.outline = Shape::Polygon(form.hull);
            }
            scene.forms.insert(element.id(), form);
        }
        scene.routes = plan
            .elements
            .iter()
            .filter_map(|element| match element {
                StageElement::Path {
                    id,
                    through,
                    curve,
                    corner,
                    bend,
                    ..
                } => Some((id.as_str(), scene.route(through, *curve, *corner, *bend)?)),
                _ => None,
            })
            .collect();
        // Shields wrap what they surround, so they are placed after it.
        for element in &plan.elements {
            if let StageElement::Shield { id, around, .. } = element
                && let Some(place) = scene.shield_place(element, around)
            {
                scene.placements.insert(id.as_str(), place);
            }
        }
        scene.links = plan
            .elements
            .iter()
            .filter_map(|element| match element {
                StageElement::Beam {
                    id, from, to, bend, ..
                } => Some((id.as_str(), scene.link(id, from, to, *bend)?)),
                _ => None,
            })
            .collect();
        let mut lights = Vec::new();
        for element in &plan.elements {
            scene.lights_of(element, &mut lights);
        }
        scene.lights = lights;
        scene
    }

    /// An element's channel, or its Stage channel default when nothing writes it.
    fn v(&self, id: &str, property: &str) -> f32 {
        read(self.plan, self.value, &format!("{id}.{property}"))
    }

    /// A channel clamped to 0..1: opacities, progress, and amounts.
    fn unit(&self, id: &str, property: &str) -> f32 {
        self.v(id, property).clamp(0.0, 1.0)
    }

    /// An orb's spin: ambient drift plus its authored `rotation` offset.
    fn orb_rotation(&self, id: &str) -> Quat {
        Quat::from_rotation_x(0.42)
            * Quat::from_rotation_y(self.time * 0.14 * self.v(id, "spin") + self.v(id, "rotation"))
    }

    /// An orb's world radius before any collapse.
    fn orb_radius(&self, id: &str, radius: f32) -> f32 {
        radius * self.v(id, "scale").max(0.01) * self.breath()
    }

    /// Depth-of-field blur, in pixels, of something at depth `z`.
    fn blur_at(&self, z: f32) -> f32 {
        (self.dof * (z - self.focus).abs() / 100.0).min(24.0)
    }

    /// The shell barely breathes. A pulse is light, never a discontinuous scale
    /// change: beam ports must stay attached when an arrival strikes.
    fn breath(&self) -> f32 {
        1.0 + 0.006 * (self.time * 1.4).sin()
    }

    fn place(&self, element: &StageElement) -> Option<Placement> {
        let id = element.id();
        let anchor = Vec3::from(element.anchor()?);
        let offset = vec3(self.v(id, "x"), self.v(id, "y"), self.v(id, "z"));
        let world = anchor + offset;
        let (center, perspective) = self.camera.project(world)?;
        let breath = match element {
            StageElement::Orb { .. } | StageElement::Form { .. } => self.breath(),
            _ => 1.0,
        };
        let scale = perspective * self.v(id, "scale").max(0.01) * breath;
        let sized = match element {
            StageElement::Shape {
                shape: Figure::Rect(size),
                ..
            } => Some(Figure::Rect([
                (size[0] + self.v(id, "width")).max(1.0),
                (size[1] + self.v(id, "height")).max(1.0),
            ])),
            _ => None,
        };
        let outline = match (element, &sized) {
            // A turned figure attaches to its turned corners.
            (
                StageElement::Shape {
                    shape: shape @ (Figure::Rect(_) | Figure::Polygon(_)),
                    ..
                },
                sized,
            ) if self.v(id, "rotation") != 0.0 => Shape::Polygon(Polygon::hull(
                center,
                figure_outline(sized.as_ref().unwrap_or(shape), 0.0, self.v(id, "rotation"))
                    .into_iter()
                    .map(|point| center + point * scale),
            )),
            (_, Some(rect)) => rect.outline(center, scale),
            _ => element.outline(center, scale),
        };
        Some(Placement {
            world,
            depth: self.camera.depth(world),
            center,
            scale,
            outline,
        })
    }

    /// A shield is centered on what it surrounds, at its own scale.
    fn shield_place(&self, element: &StageElement, around: &str) -> Option<Placement> {
        let host = self.placements.get(around)?;
        let (_, perspective) = self.camera.project(host.world)?;
        let scale = perspective * self.v(element.id(), "scale").max(0.01);
        Some(Placement {
            world: host.world,
            depth: host.depth,
            center: host.center,
            scale,
            outline: element.outline(host.center, scale),
        })
    }

    /// One end of a bolt: an element's outline, or a projected world point,
    /// with its scale and depth.
    fn bolt_end(&self, end: &BoltEnd) -> Option<(Shape, f32, f32)> {
        match end {
            BoltEnd::Element(id) => {
                let place = self.placements.get(id.as_str())?;
                Some((place.outline, place.scale, place.depth))
            }
            BoltEnd::Point(at) => {
                let world = Vec3::from(*at);
                let (center, scale) = self.camera.project(world)?;
                Some((Shape::Point(center), scale, self.camera.depth(world)))
            }
        }
    }

    /// Where a bolt leaves and strikes: each outline where the straight line
    /// between them crosses it.
    fn bolt_ends(&self, from: &BoltEnd, to: &BoltEnd) -> Option<BoltEnds> {
        let (a, scale_a, depth_a) = self.bolt_end(from)?;
        let (b, scale_b, depth_b) = self.bolt_end(to)?;
        let start = a.boundary_toward(b.center());
        Some(BoltEnds {
            from: start,
            to: b.boundary_toward(start.point),
            scale: [scale_a, scale_b],
            depth: [depth_a, depth_b],
        })
    }

    /// A bolt's discharge seed: its latest zap's `seed`, mixed with its ID so
    /// bolts zapped at the same moment still differ.
    fn bolt_seed(&self, id: &str) -> u32 {
        (self.v(id, "seed").max(0.0).round() as u32) ^ salt(id)
    }

    /// A form's points this sample: morphed between its shapes, turned by
    /// its tilt, spin, pitch, and roll, and scaled; and the silhouette.
    fn form(&self, id: &str, geometry: &FormGeometry, place: Placement) -> SampledForm {
        let last = geometry.shapes.len() - 1;
        let morph = self.v(id, "morph").clamp(0.0, last as f32);
        let from = (morph.floor() as usize).min(last.saturating_sub(1));
        let radius = if last == 0 {
            geometry.radii[0]
        } else {
            lerp(
                geometry.radii[from],
                geometry.radii[from + 1],
                morph - from as f32,
            )
        };
        let turn = Quat::from_rotation_x(geometry.tilt)
            * Quat::from_rotation_y(self.time * 0.14 * self.v(id, "spin") + self.v(id, "rotation"))
            * Quat::from_rotation_x(self.v(id, "pitch"))
            * Quat::from_rotation_z(self.v(id, "roll"));
        let grow = self.v(id, "scale").max(0.01) * self.breath();
        let dots = geometry
            .seeds
            .iter()
            .enumerate()
            .map(|(index, seed)| {
                let point = turn * morph_point(&geometry.shapes, index, seed.x, morph);
                let near = (0.5 * (1.0 - point.z / radius.max(1.0))).clamp(0.0, 1.0);
                (point * grow, near, *seed)
            })
            .collect::<Vec<_>>();
        let hull = Polygon::hull(
            place.center,
            dots.iter()
                .filter_map(|(offset, ..)| self.camera.project(place.world + *offset))
                .map(|(point, _)| point),
        );
        SampledForm { dots, radius, hull }
    }

    fn link(&self, id: &str, from: &str, to: &str, bend: f32) -> Option<Link> {
        let (a, b) = (self.placements.get(from)?, self.placements.get(to)?);
        // A twang sags the curve downward, as if it had weight, then vibrates
        // back to rest.
        let downward = if (b.center - a.center).perp().y >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let bend = bend + 10.0 * downward * self.v(id, "twang");
        let curve = connect(
            self.submerged(from, a.outline),
            self.submerged(to, b.outline),
            bend * (a.scale + b.scale) * 0.5,
        );
        let card = |id: &str| matches!(self.plan.element(id), Some(StageElement::Card { .. }));
        let end = b.outline.port_toward(curve.start);
        Some(Link {
            path: curve.flatten(BEAM_SAMPLES),
            label_ports: [a.outline.port_toward(end.point), end],
            depth: [a.depth, b.depth],
            scale: [a.scale, b.scale],
            socket: [card(from), card(to)],
            landing: [self.open(from), self.open(to)],
            tips: [0.0, 1.0],
            source: match a.outline {
                Shape::Box(frame) if card(from) => Some(frame),
                _ => None,
            },
        })
    }

    /// The outline a wire aims for. Circular and form ends continue beneath
    /// the shell: occlusion hides the cap, and the visible wire meets the
    /// silhouette rather than a floating socket. Flat shapes and icons have
    /// no shell, so wires meet their outline.
    fn submerged(&self, id: &str, outline: Shape) -> Shape {
        match (self.plan.element(id), outline) {
            (Some(StageElement::Shape { .. } | StageElement::Icon { .. }), outline) => outline,
            (_, Shape::Circle(mut circle)) => {
                circle.radius *= 0.68;
                Shape::Circle(circle)
            }
            (_, Shape::Polygon(polygon)) => Shape::Polygon(polygon.scaled(0.68)),
            (_, outline) => outline,
        }
    }

    /// Whether an arrival at `id` shows its landing (see `Link::landing`).
    fn open(&self, id: &str) -> bool {
        matches!(
            self.plan.element(id),
            Some(
                StageElement::Card { .. } | StageElement::Shape { .. } | StageElement::Icon { .. }
            )
        )
    }

    /// A path's legs on screen, split at its stops. Hops that leave or enter
    /// an element connect like beams; runs between points are straight with
    /// rounded corners, a Catmull-Rom curve, or an authored Bézier chain.
    fn route(
        &self,
        through: &[Waypoint],
        curve: Curve,
        corner: f32,
        bend: f32,
    ) -> Option<Vec<Link>> {
        // Each waypoint on screen: its outline (a point for a world point),
        // depth from the camera, and perspective scale.
        let stops = through
            .iter()
            .map(|waypoint| match waypoint {
                Waypoint::Element(id) => {
                    let place = self.placements.get(id.as_str())?;
                    Some((Some(id.as_str()), place.outline, place.depth, place.scale))
                }
                Waypoint::Point(at) => {
                    let world = Vec3::from(*at);
                    let (point, scale) = self.camera.project(world)?;
                    Some((None, Shape::Point(point), self.camera.depth(world), scale))
                }
            })
            .collect::<Option<Vec<_>>>()?;
        let last = stops.len() - 1;
        let breaks = (0..=last)
            .filter(|&index| index == 0 || index == last || stops[index].0.is_some())
            .collect::<Vec<_>>();
        let card = |id: Option<&str>| {
            id.is_some_and(|id| matches!(self.plan.element(id), Some(StageElement::Card { .. })))
        };
        let legs = breaks
            .windows(2)
            .map(|pair| {
                let leg = &stops[pair[0]..=pair[1]];
                let scale = (leg[0].3 + leg[leg.len() - 1].3) * 0.5;
                let path = match curve {
                    Curve::Smooth => {
                        catmull_rom(&leg.iter().map(|s| s.1.center()).collect::<Vec<_>>())
                    }
                    Curve::Bezier => {
                        bezier_chain(&leg.iter().map(|s| s.1.center()).collect::<Vec<_>>())
                    }
                    Curve::Straight => {
                        let pieces = leg
                            .windows(2)
                            .map(|hop| match (hop[0].0, hop[1].0) {
                                (None, None) => {
                                    Polyline::new(vec![hop[0].1.center(), hop[1].1.center()])
                                }
                                (a, b) => {
                                    let outline = |id: Option<&str>, shape| match id {
                                        Some(id) => self.submerged(id, shape),
                                        None => shape,
                                    };
                                    connect(
                                        outline(a, hop[0].1),
                                        outline(b, hop[1].1),
                                        bend * scale,
                                    )
                                    .flatten(BEAM_SAMPLES)
                                }
                            })
                            .collect::<Vec<_>>();
                        join_rounded(&pieces, corner * scale)
                    }
                };
                let (start, end) = (leg[0], leg[leg.len() - 1]);
                let first = path.at(0.0);
                let toward = |shape: Shape, other: Vec2| match shape {
                    Shape::Point(point) => Port {
                        point,
                        normal: (other - point).normalize_or(Vec2::X),
                    },
                    shape => shape.port_toward(other),
                };
                let end_port = toward(end.1, first);
                // Where the visible line leaves or meets a body's silhouette.
                let tip = |shape: Shape, id: Option<&str>, from_end: bool| {
                    if !id
                        .is_some_and(|id| self.plan.element(id).is_some_and(StageElement::is_body))
                    {
                        return if from_end { 1.0 } else { 0.0 };
                    }
                    let (mut inside, mut outside) = if from_end { (1.0, 0.0) } else { (0.0, 1.0) };
                    for _ in 0..20 {
                        let middle = (inside + outside) * 0.5;
                        if shape.distance(path.at(middle)) < 0.0 {
                            inside = middle;
                        } else {
                            outside = middle;
                        }
                    }
                    outside
                };
                Link {
                    label_ports: [toward(start.1, end_port.point), end_port],
                    depth: [start.2, end.2],
                    scale: [start.3, end.3],
                    socket: [card(start.0), card(end.0)],
                    landing: [
                        start.0.is_none_or(|id| self.open(id)),
                        end.0.is_none_or(|id| self.open(id)),
                    ],
                    tips: [tip(start.1, start.0, false), tip(end.1, end.0, true)],
                    source: None,
                    path,
                }
            })
            .collect();
        Some(legs)
    }
}

/// Pieces of a path joined end to end, each corner between them rounded by a
/// curve cut back `radius` pixels along both sides (at most half of each).
fn join_rounded(pieces: &[Polyline], radius: f32) -> Polyline {
    let mut pieces = pieces.to_vec();
    let mut points: Vec<Vec2> = Vec::new();
    for index in 0..pieces.len() {
        if index + 1 < pieces.len() {
            let (before, after) = (&pieces[index], &pieces[index + 1]);
            let r = radius.min(before.length() * 0.5).min(after.length() * 0.5);
            if r > 0.5 {
                let corner = after.at(0.0);
                let before = before.slice(0.0, 1.0 - r / before.length());
                let after = after.slice(r / after.length(), 1.0);
                let (a, b) = (before.at(1.0), after.at(0.0));
                // A quadratic through the corner, as a cubic: tangent to both sides.
                let round = CubicBezier {
                    start: a,
                    control_a: a + (corner - a) * (2.0 / 3.0),
                    control_b: b + (corner - b) * (2.0 / 3.0),
                    end: b,
                }
                .flatten(10);
                points.extend(before.points());
                points.extend(&round.points()[1..round.points().len() - 1]);
                pieces[index + 1] = after;
                continue;
            }
        }
        points.extend(pieces[index].points());
    }
    points.dedup_by(|a, b| a.distance(*b) < 1e-3);
    Polyline::new(points)
}

/// A Catmull-Rom curve through `points`, as cubic Béziers.
fn catmull_rom(points: &[Vec2]) -> Polyline {
    let at = |index: isize| points[index.clamp(0, points.len() as isize - 1) as usize];
    let mut out = vec![points[0]];
    for index in 0..points.len() as isize - 1 {
        let curve = CubicBezier {
            start: at(index),
            control_a: at(index) + (at(index + 1) - at(index - 1)) / 6.0,
            control_b: at(index + 1) - (at(index + 2) - at(index)) / 6.0,
            end: at(index + 1),
        };
        out.extend(&curve.flatten(24).points()[1..]);
    }
    Polyline::new(out)
}

/// A chain of cubic Béziers: start, then two controls and an end per curve.
fn bezier_chain(points: &[Vec2]) -> Polyline {
    let mut out = vec![points[0]];
    for curve in points.windows(4).step_by(3) {
        let curve = CubicBezier {
            start: curve[0],
            control_a: curve[1],
            control_b: curve[2],
            end: curve[3],
        };
        out.extend(&curve.flatten(32).points()[1..]);
    }
    Polyline::new(out)
}

impl Scene<'_> {
    /// Packet contact begins at the visible shell, before the submerged endpoint.
    fn orb_contacts(&self, id: &str, place: Placement) -> Vec<(Vec3, f32, Tone, f32)> {
        let Shape::Circle(circle) = place.outline else {
            return Vec::new();
        };
        self.plan
            .elements
            .iter()
            .filter_map(|element| {
                let StageElement::Packet {
                    id: packet_id,
                    beam,
                    reverse,
                    tone,
                    ..
                } = element
                else {
                    return None;
                };
                let StageElement::Beam { from, to, .. } = self.plan.element(beam)? else {
                    return None;
                };
                if (if *reverse { from } else { to }) != id {
                    return None;
                }
                let path = self.links.get(beam.as_str())?.toward(*reverse).path;
                let fraction = circle.entry_fraction(&path)?;
                let since = packet::since_crossing(
                    self.v(packet_id, "age"),
                    self.v(packet_id, "flight").max(0.05),
                    fraction,
                )?;
                if since >= 1.4 {
                    return None;
                }
                let normal = (path.at(fraction) - place.center).normalize_or(Vec2::X);
                // Toward the lens from the screen-space contact, in the world.
                let direction = self
                    .camera
                    .world_dir(vec3(normal.x, normal.y, -0.34).normalize());
                Some((direction, since, *tone, self.unit(packet_id, "opacity")))
            })
            .chain(
                self.strikes_on(id)
                    .into_iter()
                    .map(|(point, since, tone, strength)| {
                        let normal = (point - place.center).normalize_or(Vec2::X);
                        (
                            vec3(normal.x, normal.y, -0.34).normalize(),
                            since,
                            tone,
                            strength,
                        )
                    }),
            )
            .collect()
    }

    /// Return strokes of bolts striking `target` within a surface ripple's
    /// life: contact point, seconds since the stroke, tone, and strength.
    fn strikes_on(&self, target: &str) -> Vec<(Vec2, f32, Tone, f32)> {
        let mut strikes = Vec::new();
        for element in &self.plan.elements {
            let StageElement::Bolt {
                id,
                from,
                to: to @ BoltEnd::Element(end),
                strikes: count,
                tone,
                ..
            } = element
            else {
                continue;
            };
            let age = self.v(id, "age");
            if end != target || age < 0.0 {
                continue;
            }
            let Some(ends) = self.bolt_ends(from, to) else {
                continue;
            };
            let discharge = Discharge::new(*count, self.bolt_seed(id));
            let opacity = self.unit(id, "opacity");
            for k in 0..discharge.strikes {
                let since = age - discharge.strike_time(k);
                if (0.0..shield::RIPPLE).contains(&since) {
                    // Re-strikes ripple more softly than the first contact.
                    let strength = 0.65 * discharge.strength(k).powi(2) * opacity;
                    strikes.push((ends.to.point, since, *tone, strength));
                }
            }
        }
        strikes
    }

    /// What touches a shield: packets crossing into it on their way to what
    /// it surrounds (softly), and bolts striking it. The four most recent.
    fn shield_contacts(&self, id: &str, place: Placement) -> Vec<Contact> {
        let (Shape::Circle(circle), Some(StageElement::Shield { around, .. })) =
            (place.outline, self.plan.element(id))
        else {
            return Vec::new();
        };
        let toward = |point: Vec2| {
            let normal = (point - circle.center) / circle.radius.max(1.0);
            let z = -(1.0 - normal.length_squared()).max(0.0).sqrt();
            vec3(normal.x, normal.y, z.min(-0.2)).normalize()
        };
        let mut contacts = self
            .strikes_on(id)
            .into_iter()
            .map(|(point, age, _, strength)| Contact {
                point,
                direction: toward(point),
                age,
                strength,
            })
            .collect::<Vec<_>>();
        for element in &self.plan.elements {
            let StageElement::Packet {
                id: packet,
                beam,
                reverse,
                ..
            } = element
            else {
                continue;
            };
            let Some(StageElement::Beam { from, to, .. }) = self.plan.element(beam) else {
                continue;
            };
            if (if *reverse { from } else { to }) != around {
                continue;
            }
            let Some(link) = self.links.get(beam.as_str()) else {
                continue;
            };
            let path = link.toward(*reverse).path;
            let Some(fraction) = circle.entry_fraction(&path).filter(|f| *f > 0.0) else {
                continue;
            };
            let since = packet::since_crossing(
                self.v(packet, "age"),
                self.v(packet, "flight").max(0.05),
                fraction,
            );
            if let Some(age) = since.filter(|age| *age < shield::RIPPLE) {
                let point = path.at(fraction);
                contacts.push(Contact {
                    point,
                    direction: toward(point),
                    age,
                    strength: 0.55 * self.unit(packet, "opacity"),
                });
            }
        }
        contacts.sort_by(|a, b| a.age.total_cmp(&b.age));
        contacts.truncate(4);
        contacts
    }

    /// A charged outline lights its own rim where its brightest arcs crawl.
    fn charge_lights(&self, id: &str, tone: Tone, lights: &mut Vec<Light>) {
        let charge = self.v(id, "charge");
        if charge <= 0.0 {
            return;
        }
        let Some(place) = self.placements.get(id) else {
            return;
        };
        let Some(perimeter) = Perimeter::of(place.outline, place.scale) else {
            return;
        };
        let opacity = self.unit(id, "opacity");
        let mut creeps = lightning::crackle(self.time, charge, salt(id));
        creeps.sort_by(|a, b| b.energy.total_cmp(&a.energy));
        for creep in creeps.iter().take(3) {
            lights.push(Light {
                at: perimeter.at(creep.center),
                tone: spark_tone(tone),
                strength: 1.4 * creep.energy * opacity,
                radius: REFLECTION_RADIUS * 0.8,
                pool: false,
                scale: place.scale,
            });
        }
    }

    /// The lights a beam, packet, or combusting orb casts this sample.
    fn lights_of(&self, element: &StageElement, lights: &mut Vec<Light>) {
        match element {
            StageElement::Card { id, tone, .. } => self.charge_lights(id, *tone, lights),
            StageElement::Orb {
                id, radius, tone, ..
            } => {
                self.charge_lights(id, *tone, lights);
                let age = self.v(id, "burst");
                let burst = Burst::sample(age);
                if burst.rim_strength == 0.0 {
                    return;
                }
                let Some(place) = self.placements.get(id.as_str()) else {
                    return;
                };
                let strength = burst.rim_strength * self.unit(id, "opacity");
                lights.push(Light {
                    at: place.center,
                    tone: Tone::Accent,
                    strength,
                    radius: radius * 4.4,
                    pool: false,
                    scale: place.scale,
                });
            }
            StageElement::Beam { id, tone, .. } => {
                let Some(link) = self.links.get(id.as_str()) else {
                    return;
                };
                let opacity = self.unit(id, "opacity");
                let draw = self.unit(id, "draw");
                // A plain draw-on emits no travelling bead or arrival light.
                // An explicitly authored surge may still light the receiver.
                let strength = if draw >= 0.999 {
                    self.unit(id, "surge") * 0.5
                } else {
                    0.0
                };
                if strength * opacity > 0.01 {
                    lights.push(Light {
                        at: link.path.at(1.0),
                        tone: *tone,
                        strength: strength * opacity,
                        radius: 150.0,
                        pool: true,
                        scale: link.scale_at(1.0),
                    });
                }
            }
            StageElement::Form { id, .. } => {
                let age = self.v(id, "burst");
                let burst = Burst::sample(age);
                if burst.rim_strength == 0.0 {
                    return;
                }
                let (Some(place), Some(form)) = (
                    self.placements.get(id.as_str()),
                    self.forms.get(id.as_str()),
                ) else {
                    return;
                };
                lights.push(Light {
                    at: place.center,
                    tone: Tone::Accent,
                    strength: burst.rim_strength * self.unit(id, "opacity"),
                    radius: form.radius * 4.4,
                    pool: false,
                    scale: place.scale,
                });
            }
            StageElement::Packet {
                id,
                beam,
                reverse,
                tone,
                ..
            } => {
                for (link, age, flight) in self.packet_legs(id, beam, *reverse) {
                    self.packet_lights(id, *tone, &link, age, flight, lights);
                }
            }
            StageElement::Bolt {
                id,
                from,
                to,
                strikes,
                tone,
                ..
            } => {
                let Some(ends) = self.bolt_ends(from, to) else {
                    return;
                };
                let opacity = self.unit(id, "opacity");
                let seed = self.bolt_seed(id);
                let discharge = Discharge::new(*strikes, seed);
                let age = self.v(id, "age");
                let hum = lightning::hum(self.time, self.v(id, "hum"), seed)
                    .map_or(0.0, |(_, energy)| 0.6 * energy);
                let mut cast = |at: Vec2, strength: f32, radius: f32, pool: bool, scale: f32| {
                    if strength * opacity > 0.01 {
                        lights.push(Light {
                            at,
                            tone: *tone,
                            strength: strength * opacity,
                            radius,
                            pool,
                            scale,
                        });
                    }
                };
                // The leader's tip lights what it nears; a stroke lights both
                // rims it touches and floods into the one it struck.
                if let Some(progress) = discharge.leader(age) {
                    let tip = ends.from.point.lerp(ends.to.point, progress);
                    cast(
                        tip,
                        discharge.light(age),
                        REFLECTION_RADIUS,
                        false,
                        ends.scale(),
                    );
                }
                let strength = if discharge.leader(age).is_some() {
                    hum
                } else {
                    discharge.light(age).max(hum)
                };
                // A bolt outshines a packet: its rims blaze, then cool with it.
                cast(
                    ends.to.point,
                    1.8 * strength,
                    REFLECTION_RADIUS * 1.5,
                    false,
                    ends.scale[1],
                );
                cast(
                    ends.from.point,
                    1.4 * strength,
                    REFLECTION_RADIUS * 1.3,
                    false,
                    ends.scale[0],
                );
                let flood = discharge
                    .glowing(age)
                    .map(|(k, since)| discharge.core(k, since))
                    .fold(0.5 * hum, f32::max);
                cast(ends.to.point, 0.8 * flood, 150.0, true, ends.scale[1]);
            }
            StageElement::Shield { id, tone, .. } => {
                let Some(place) = self.placements.get(id.as_str()) else {
                    return;
                };
                let opacity = self.unit(id, "opacity");
                for contact in self.shield_contacts(id, *place) {
                    let strength = 0.9 * shield::flare(contact.age) * contact.strength * opacity;
                    if strength > 0.01 {
                        lights.push(Light {
                            at: contact.point,
                            tone: *tone,
                            strength,
                            radius: REFLECTION_RADIUS,
                            pool: false,
                            scale: place.scale,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// The legs a packet flies this sample, each oriented the way it travels,
    /// with its own clock and flight time. A beam is one leg on the packet's
    /// clock; a path's legs start one after another (`packet::leg_start`).
    fn packet_legs(&self, id: &str, wire: &str, reverse: bool) -> Vec<(Link, f32, f32)> {
        let age = self.v(id, "age");
        let flight = self.v(id, "flight").max(0.05);
        if let Some(link) = self.links.get(wire) {
            return vec![(link.toward(reverse), age, flight)];
        }
        let Some(legs) = self.routes.get(wire) else {
            return Vec::new();
        };
        let count = legs.len();
        let ordered: Vec<&Link> = if reverse {
            legs.iter().rev().collect()
        } else {
            legs.iter().collect()
        };
        ordered
            .into_iter()
            .enumerate()
            .map(|(leg, link)| {
                (
                    link.toward(reverse),
                    age - packet::leg_start(leg, count, flight),
                    flight / count as f32,
                )
            })
            .collect()
    }

    /// The light one packet leg casts: its reflection gathering, flying, and
    /// landing, the ember where it left, and the flood where it arrived.
    fn packet_lights(
        &self,
        id: &str,
        tone: Tone,
        link: &Link,
        age: f32,
        flight: f32,
        lights: &mut Vec<Light>,
    ) {
        if !(0.0..packet::LIFETIME).contains(&age) {
            return;
        }
        let opacity = self.unit(id, "opacity");
        let mut cast = |fraction: f32, strength: f32, radius: f32, pool: bool| {
            if strength * opacity > 0.01 {
                lights.push(Light {
                    at: link.path.at(fraction),
                    tone,
                    strength: strength * opacity,
                    radius,
                    pool,
                    scale: link.scale_at(fraction),
                });
            }
        };
        // The reflection rides the packet: it gathers at the port, flies,
        // and fades as the packet is absorbed.
        if let Some(g) = packet::gather(age) {
            cast(0.0, cubic_out(g).powf(1.5), REFLECTION_RADIUS, false);
        }
        if packet::flight(age, flight).is_some() {
            cast(packet::travel(age, flight), 1.0, REFLECTION_RADIUS, false);
        }
        if let Some(q) = packet::landing(age, flight) {
            cast(1.0, (1.0 - q).powi(2), REFLECTION_RADIUS, false);
        }
        // An ember glows where it left, seeping outward as it cools.
        let t = age / packet::EMBER;
        if t < 1.0 {
            let spread = (0.1 + 0.7 * t.sqrt()) * 210.0 * DIAGRAM_SCALE;
            let core = 0.6 * (t / 0.04).min(1.0) * (1.0 - t).powf(0.9);
            cast(0.0, core, spread, true);
        }
        // Light floods into whatever it reached, spreading and fading.
        if let Some(since) = packet::since_arrival(age, flight) {
            let t = since / packet::FLOOD;
            if t < 1.0 {
                let travel = 1.0 - (1.0 - t).powi(4);
                let width = 0.07 + 0.6 * travel.sqrt();
                let fade = (-0.9 * t).exp() * (1.0 - smoothstep((t - 0.65) / 0.35));
                let strength = 1.4 * smoothstep(t / 0.07) * (0.12 / (0.12 + width)).sqrt() * fade;
                cast(1.0, strength, width * 300.0 * DIAGRAM_SCALE, true);
            }
        }
    }

    /// The lights that can reach `outline`.
    fn lights_on(&self, outline: Shape) -> impl Iterator<Item = &Light> + '_ {
        self.lights
            .iter()
            .filter(move |light| outline.distance(light.at) < light.radius * light.scale)
    }
}

/// Polyline styles: dash, gap, phase, and fade toward the start.
const SOLID: [f32; 4] = [0.0; 4];
const COMET: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// How a card's ink is painted this sample: where its text sits, and how
/// deletion cools and reddens every ink.
#[derive(Clone, Copy)]
struct CardInk {
    center: Vec2,
    scale: f32,
    /// Text ink opacity, and the card's own.
    alpha: f32,
    opacity: f32,
    /// Text blur, and the frame's.
    blur: f32,
    edge_blur: f32,
    cool: f32,
    damage: f32,
    gray: Vec3,
    red: Vec3,
}

impl CardInk {
    /// Inks cool toward the frame gray, then snap to red at once.
    fn condemn(&self, ink: Vec3) -> Vec3 {
        ink.lerp(self.gray, 0.6 * self.cool)
            .lerp(self.red, self.damage)
    }
}

/// Draws each element of one sample into its frame.
struct Painter<'a> {
    scene: &'a Scene<'a>,
    look: Look,
    orbs: &'a HashMap<String, Vec<OrbPoint>>,
    footage: &'a HashMap<String, FootageSlot>,
    frame: StageFrame<'a>,
}

impl<'a> Painter<'a> {
    /// A faint neutral light behind the scene. Warmth comes only from the bloom
    /// of what is actually lit.
    fn backdrop(&mut self, amount: f32) {
        let size = self.scene.camera.size;
        let look = self.look;
        self.frame.prims.push(Prim {
            bbox: [0.0, 0.0, size.x, size.y],
            a: [5.0, size.x * 0.5, size.y * 0.44, size.x * 0.72],
            fill: rgba(look.background.lerp(look.raised, amount), 1.0),
            stroke: rgba(look.background, 1.0),
            ..Default::default()
        });
        self.frame.close(f32::INFINITY, 0);
    }

    fn element(&mut self, order: usize, element: &StageElement) {
        let scene = self.scene;
        let id = element.id();
        let place = scene.placements.get(id).copied();
        match (element, place) {
            (
                StageElement::Card {
                    size,
                    status,
                    tone,
                    mark,
                    ..
                },
                Some(place),
            ) => self.card(order, id, Vec2::from(*size), status, *tone, *mark, place),
            (StageElement::Orb { radius, tone, .. }, Some(place)) => {
                self.orb(order, id, *radius, *tone, place)
            }
            (StageElement::Label { align, spans, .. }, Some(place)) => {
                self.label(order, id, *align, spans, place)
            }
            (
                StageElement::Ring {
                    radius,
                    thickness,
                    tone,
                    ..
                },
                Some(place),
            ) => self.ring(order, id, [*radius, *thickness], *tone, place),
            (StageElement::Beam { tone, .. }, _) => {
                if let Some(link) = scene.links.get(id) {
                    self.beam(order, id, *tone, link);
                }
            }
            (
                StageElement::Packet {
                    beam,
                    reverse,
                    tone,
                    ..
                },
                _,
            ) => {
                for (link, age, flight) in scene.packet_legs(id, beam, *reverse) {
                    self.packet(order, id, *tone, age, flight, &link);
                }
            }
            (
                StageElement::Bolt {
                    from,
                    to,
                    strikes,
                    branching,
                    tone,
                    ..
                },
                _,
            ) => {
                if let Some(ends) = scene.bolt_ends(from, to) {
                    self.bolt(order, id, ends, *strikes, *branching, *tone);
                }
            }
            (StageElement::Shield { tone, .. }, Some(place)) => {
                self.shield(order, id, *tone, place)
            }
            (StageElement::Form { tone, .. }, Some(place)) => self.form(order, id, *tone, place),
            (
                StageElement::Shape {
                    shape,
                    corner,
                    fill,
                    fill_opacity,
                    stroke,
                    width,
                    dash,
                    arrow,
                    ..
                },
                Some(place),
            ) => self.shape(
                order,
                id,
                FigureStyle {
                    figure: shape,
                    corner: *corner,
                    fill: *fill,
                    fill_opacity: *fill_opacity,
                    stroke: *stroke,
                    width: *width,
                    dash: *dash,
                    arrow: *arrow,
                },
                place,
            ),
            (
                StageElement::Path {
                    tone,
                    width,
                    dash,
                    arrow,
                    ..
                },
                _,
            ) => {
                if let Some(legs) = scene.routes.get(id) {
                    self.path(order, id, *tone, *width, *dash, *arrow, legs);
                }
            }
            (
                StageElement::Icon {
                    icon,
                    size,
                    tone,
                    ink,
                    ..
                },
                Some(place),
            ) => self.icon(order, id, icon, *size, *tone, *ink, place),
            (
                StageElement::Footage {
                    size,
                    fit,
                    mask,
                    framed,
                    tint,
                    ..
                },
                Some(place),
            ) => self.footage(
                order,
                id,
                Vec2::from(*size),
                *fit,
                mask,
                *framed,
                *tint,
                place,
            ),
            // A positioned element behind the camera.
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn card(
        &mut self,
        order: usize,
        id: &str,
        size: Vec2,
        status: &[StatusText],
        tone: Tone,
        mark: Mark,
        place: Placement,
    ) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        let scale = place.scale;
        let half = size * 0.5 * scale;
        let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * scale;
        let red = look.tone(Tone::Error);
        // The afterimage: the slot a deleted card leaves, drawn beneath it.
        let ghost = scene.unit(id, "ghost");
        if ghost > 0.001 {
            for (inset, strength) in [(0.0, 0.55), (3.0 * scale, 0.33)] {
                self.frame.rounded_rect(
                    place.center,
                    half - inset,
                    [14.0 * scale - inset, 1.0],
                    blur,
                    Paint {
                        stroke: rgba(red, strength * ghost),
                        ..Default::default()
                    },
                );
            }
        }
        if opacity <= 0.001 {
            self.frame.close(place.depth, order);
            return;
        }
        let first = self.frame.prims.len();
        let glow = scene.v(id, "glow").clamp(0.0, 1.5);
        let flash = scene.v(id, "flash").clamp(0.0, 1.5);
        let alarm = scene.v(id, "alarm").clamp(0.0, 1.5);
        let dim = scene.unit(id, "dim");
        // Deletion: inks cool toward the frame gray, then snap to red at once.
        let content = scene.unit(id, "content");
        let pen = CardInk {
            center: place.center + vec2(0.0, 7.0 * (1.0 - content) * scale),
            scale,
            alpha: opacity * content * (1.0 - 0.55 * dim),
            opacity,
            blur: blur + 1.5 * (1.0 - content) * scale,
            edge_blur: blur,
            cool: scene.unit(id, "cool"),
            damage: scene.unit(id, "damage"),
            gray: look.raised.lerp(look.muted, 0.22),
            red,
        };
        let own = look.tone(tone);
        // Ink and rim respond; the substrate stays dark. In linear light even
        // a modest full-card tint overwhelms the directional socket reflection.
        // The rim reddens by the alarm's share of all its light, continuously.
        let lit = (flash + alarm).min(1.5);
        let hue = own.lerp(red, alarm / (glow + flash + alarm).max(1e-3));
        let edge = pen.condemn(pen.gray.lerp(hue, (glow * 0.32 + lit * 0.28).min(0.65)));
        // The strongest reflection on the edge, and the strongest pool in the glass.
        let strongest = |pool: bool| {
            scene
                .lights_on(place.outline)
                .filter(|light| light.pool == pool)
                .max_by(|a, b| a.strength.total_cmp(&b.strength))
                .map_or(([0.0; 4], [0.0; 4]), |light| light.uniform(&look, opacity))
        };
        let (reflection, reflection_color) = strongest(false);
        let (pool, pool_color) = strongest(true);
        self.frame.rounded_rect(
            place.center,
            half,
            [14.0 * scale, 1.0 * scale.max(0.5)],
            blur,
            Paint {
                fill: rgba(look.surface, 0.97 * opacity * (1.0 - 0.45 * dim)),
                stroke: rgba(edge, opacity * (1.0 - 0.5 * dim)),
                glow: glow4(own * (glow * 0.018 * opacity), 7.0 * scale),
                light: reflection,
                light_color: reflection_color,
                pool,
                pool_color,
            },
        );
        // The diagrams' double frame: a quiet rule 3 px inside the edge.
        self.frame.rounded_rect(
            place.center,
            half - 3.0 * scale,
            [11.0 * scale, 1.0],
            blur,
            Paint {
                stroke: rgba(
                    pen.condemn(look.raised.lerp(look.muted, 0.06)),
                    opacity * (1.0 - 0.6 * dim),
                ),
                ..Default::default()
            },
        );
        let title_lift = if status.is_empty() { 0.0 } else { 13.0 * scale };
        self.frame.text(
            &text_key(id, "title"),
            pen.center - vec2(0.0, title_lift),
            scale,
            CaptionAlign::Center,
            rgba(
                pen.condemn(look.text.lerp(Vec3::ONE, (flash * 0.35).min(1.0))),
                pen.alpha,
            ),
            f32::MAX,
            pen.blur,
        );
        if !status.is_empty() {
            self.card_status(id, status, mark, &pen);
        }
        self.glitch(first, id, place, half);
        self.cut(first, id, place, half, &pen);
        self.dissolve(first, id, tone, size, place, opacity);
        self.crackle(id, place.outline, tone, place.scale, opacity, blur);
        self.scan(id, tone, half, place, opacity, blur);
        self.frame.close(place.depth, order);
    }

    /// Burn the card's primitives away along its dissolve field, glowing at
    /// the front, and shed ash from wherever it has passed.
    fn dissolve(
        &mut self,
        first: usize,
        id: &str,
        tone: Tone,
        size: Vec2,
        place: Placement,
        opacity: f32,
    ) {
        let age = self.scene.v(id, "dissolve");
        if age <= 0.0 {
            return;
        }
        let seed = salt(id) & 0xFFFF;
        let rim = self.look.tone(spark_tone(tone));
        let half = size * 0.5;
        for prim in &mut self.frame.prims[first..] {
            prim.mask = [age, place.center.x, place.center.y, place.scale];
            prim.mask_shape = [half.x, half.y, seed as f32, 0.0];
            prim.mask_color = rgba(rim, 1.0);
        }
        let hot = vec3(2.4, 2.1, 1.8);
        for index in 0..dissolve::ASH {
            let Some(ash) = dissolve::ash(index, seed, half, age) else {
                continue;
            };
            let alpha = ash.opacity * opacity;
            let color = rim.lerp(hot, ash.heat.powi(4)) * (0.15 + 0.65 * ash.heat);
            self.frame.circle(
                place.center + ash.local * place.scale,
                [ash.radius * place.scale, 0.0],
                0.0,
                Paint {
                    fill: rgba(color, alpha),
                    glow: glow4(color * (0.06 * alpha * ash.heat), 2.5 * place.scale),
                    ..Default::default()
                },
            );
        }
    }

    /// Charge: short arcs crawl the outline, bowing off it, and strobe.
    fn crackle(
        &mut self,
        id: &str,
        outline: Shape,
        tone: Tone,
        scale: f32,
        opacity: f32,
        blur: f32,
    ) {
        let charge = self.scene.v(id, "charge");
        if charge <= 0.0 || opacity <= 0.001 {
            return;
        }
        let Some(perimeter) = Perimeter::of(outline, scale) else {
            return;
        };
        let own = self.look.tone(spark_tone(tone));
        let white = Vec3::ONE.lerp(own, 0.15);
        let center = outline.center();
        let length = perimeter.length();
        for creep in lightning::crackle(self.scene.time, charge, salt(id)) {
            let base = |t: f32| perimeter.at(creep.center + creep.span * (t - 0.5));
            let reach = creep.span * length;
            let points = lightning::jag(base, 4, creep.roll, 0.42)
                .into_iter()
                .enumerate()
                .map(|(index, point)| {
                    let bow = (PI * index as f32 / 16.0).sin() * reach * 0.18;
                    point + (point - center).normalize_or_zero() * bow
                })
                .collect::<Vec<_>>();
            let energy = creep.energy * opacity;
            let arc = Plasma {
                half: 0.45 * scale,
                taper: 0.0,
                blur,
                core: white * (2.2 * energy),
                corona: own * (0.9 * energy),
                radius: 3.0 * scale,
            };
            self.frame.plasma(&points, 1.0, |_| 1.0, arc);
            // Half the arcs throw a twig off the surface.
            let fork = hash(creep.roll.strike, 1);
            if fork < 0.5 {
                let at = 4 + (fork * 16.0) as usize;
                let out = (points[at] - center).normalize_or(Vec2::NEG_Y);
                let turn = (hash(creep.roll.strike, 2) - 0.5) * 1.6;
                let twig = lightning::twig(
                    points[at],
                    Vec2::from_angle(turn).rotate(out),
                    reach * (0.25 + 0.3 * hash(creep.roll.strike, 3)),
                    3,
                    creep.roll,
                );
                self.frame
                    .plasma(&twig, 1.0, |_| 0.7, Plasma { taper: 0.8, ..arc });
            }
        }
    }

    /// A scan line sweeps down the card, fading in and out at its ends,
    /// with a wake behind it and light where it crosses the rim.
    fn scan(
        &mut self,
        id: &str,
        tone: Tone,
        half: Vec2,
        place: Placement,
        opacity: f32,
        blur: f32,
    ) {
        let scan = self.scene.v(id, "scan");
        if scan <= 0.0 || scan >= 1.0 || opacity <= 0.001 {
            return;
        }
        let strength = smoothstep(scan / 0.1) * (1.0 - smoothstep((scan - 0.9) / 0.1)) * opacity;
        let scale = place.scale;
        self.frame.prims.push(Prim {
            bbox: around(place.center, half + Vec2::splat(30.0 * scale + blur)),
            a: [12.0, place.center.x, place.center.y, 14.0 * scale],
            b: [
                half.x,
                half.y,
                (scan * 2.0 - 1.0) * (half.y + 4.0 * scale),
                1.0,
            ],
            fill: rgba(self.look.tone(spark_tone(tone)), strength),
            stroke: [half.y * 0.9, 1.0 * scale, blur, 0.0],
            ..Default::default()
        });
    }

    /// The status line: statuses cross-fade by the fractional `status`
    /// channel, led by the card's spinner while it waits.
    fn card_status(&mut self, id: &str, status: &[StatusText], mark: Mark, pen: &CardInk) {
        let (scene, look, scale) = (self.scene, self.look, pen.scale);
        let last = (status.len() - 1) as f32;
        let index = scene.v(id, "status").clamp(0.0, last);
        // A swap cross-fades straight from `status-from` to `status`; otherwise
        // the fractional `status` cross-fades between neighbouring entries.
        let from = scene.v(id, "status-from");
        let (low, high, fraction) = if from >= 0.0 {
            let entry = |value: f32| value.round().clamp(0.0, last) as usize;
            (entry(from), entry(index), scene.unit(id, "swap"))
        } else {
            let low = index.floor() as usize;
            (low, (low + 1).min(status.len() - 1), index.fract())
        };
        let entries = [(low, 1.0 - fraction), (high, fraction)];
        let tone_of = |entry: usize| match status[entry].tone {
            Tone::Plain => look.muted,
            tone => look.tone(tone),
        };
        // The icon clears the incoming line before its ink is readable.
        let widths = entries.map(|(entry, _)| {
            self.frame
                .width(&text_key(id, &format!("status{entry}")), scale)
        });
        let width = lerp(widths[0], widths[1], smoothstep(entries[1].1 / 0.45));
        let color = entries.iter().fold(Vec3::ZERO, |sum, (entry, weight)| {
            sum + tone_of(*entry) * *weight
        });
        // A status spinner leads the line; the pair stays centered.
        let spinner = spinner::sample(
            scene.v(id, "spinner"),
            scene.v(id, "release"),
            scene.v(id, "mark"),
            mark,
        );
        let unit = 18.0 / 16.0 * scale;
        let gap = 8.0 * scale;
        let lead = (16.0 * unit + gap) * spinner.opacity;
        let line_y = pen.center.y + 19.0 * scale;
        let icon = vec2(pen.center.x - (width + lead) * 0.5 + 8.0 * unit, line_y);
        let ink_color = pen.condemn(
            color
                .lerp(look.text, 0.3)
                .lerp(Vec3::ONE, spinner.flash * 0.8),
        );
        for stroke in &spinner.strokes {
            let points = stroke
                .iter()
                .map(|(point, weight)| (icon + (*point - vec2(8.0, 8.0)) * unit, *weight))
                .collect::<Vec<_>>();
            self.frame.trail(
                &points,
                [1.5 * unit, pen.blur],
                Paint {
                    stroke: rgba(ink_color, pen.alpha * spinner.opacity),
                    glow: glow4(ink_color * (0.03 * spinner.flash * pen.alpha), 3.0 * unit),
                    ..Default::default()
                },
            );
        }
        // While the motor waits, a sheen sweeps the status: the blog's
        // ShimmerText, phased by the same clock so it never resets.
        let shimmer = if scene.v(id, "mark") < 0.0 {
            [scene.v(id, "spinner") / 1.6, spinner.opacity]
        } else {
            [0.0; 2]
        };
        for (entry, weight) in entries {
            if weight <= 0.001 {
                continue;
            }
            // Separate the outgoing and incoming ink instead of showing
            // two readable words on top of each other at mid-transition.
            // Each word fades in only past the midpoint: never two at once.
            let visibility = smoothstep((weight - 0.45) / 0.55);
            let drift = if entry == low { -1.0 } else { 1.0 };
            if let Some(glyphs) = self.frame.text(
                &text_key(id, &format!("status{entry}")),
                vec2(
                    pen.center.x + lead * 0.5,
                    line_y + drift * 6.0 * (1.0 - weight) * scale,
                ),
                scale,
                CaptionAlign::Center,
                rgba(pen.condemn(tone_of(entry)), pen.alpha * visibility),
                f32::MAX,
                pen.blur + (1.0 - weight) * 1.5,
            ) {
                glyphs.light = [shimmer[0], shimmer[1], 0.0, 0.0];
            }
        }
    }

    /// A few frames of horizontal band displacement, one layout per integer
    /// `glitch` seed; zero is off. Seeds step, then hold still: no noise loop.
    fn glitch(&mut self, first: usize, id: &str, place: Placement, half: Vec2) {
        let seed = self.scene.v(id, "glitch").round();
        if seed < 1.0 {
            return;
        }
        let (seed, scale) = (seed as u32, place.scale);
        let weights: [f32; 7] = std::array::from_fn(|band| 0.35 + hash(seed, band as u32));
        let total: f32 = weights.iter().sum();
        let original = self.frame.prims.drain(first..).collect::<Vec<_>>();
        let mut y = place.center.y - half.y - 16.0 * scale;
        for (band, weight) in weights.iter().enumerate() {
            let next = y + (2.0 * half.y + 32.0 * scale) * weight / total;
            let roll = hash(seed.wrapping_mul(31), band as u32 + 11);
            let shift = if roll < 0.35 {
                0.0
            } else {
                (roll - 0.675) * 16.0 * scale
            };
            // The outer bands run on, so the card's halo is never clipped.
            let top = if band == 0 { f32::MIN } else { y };
            let bottom = if band == weights.len() - 1 {
                f32::MAX
            } else {
                next
            };
            self.frame.echo(
                &original,
                [f32::MIN, top, f32::MAX, bottom],
                vec2(shift, 0.0),
                1.0,
            );
            y = next;
        }
    }

    /// Deletion by a red hairline drawn left to right across the card, then
    /// the halves part 3 px in total and fade. `cut` runs 0..1.
    fn cut(&mut self, first: usize, id: &str, place: Placement, half: Vec2, pen: &CardInk) {
        let cut = self.scene.unit(id, "cut");
        if cut <= 0.0 {
            return;
        }
        let (center, scale) = (place.center, place.scale);
        let draw = cubic_out((cut / 0.4).min(1.0));
        let part = smoothstep((cut - 0.4) / 0.6);
        // Between the title and status lines, so the hairline crosses no ink.
        let seam = center.y + 4.5 * scale;
        if part > 0.0 {
            let original = self.frame.prims.drain(first..).collect::<Vec<_>>();
            if part >= 1.0 {
                return;
            }
            let gap = 1.5 * part * scale;
            for (clip, offset) in [(f32::MIN, seam, -gap), (seam, f32::MAX, gap)]
                .map(|(top, bottom, dy)| ([f32::MIN, top, f32::MAX, bottom], vec2(0.0, dy)))
            {
                self.frame.echo(&original, clip, offset, 1.0 - part);
            }
        }
        let reach = half.x + 6.0 * scale;
        let alpha = (1.0 - part) * pen.opacity;
        self.frame.polyline(
            &Polyline::new(vec![
                vec2(center.x - reach, seam),
                vec2(center.x + reach, seam),
            ]),
            draw,
            [1.0 * scale.max(0.75), pen.edge_blur],
            Paint {
                stroke: rgba(pen.red, alpha),
                glow: glow4(pen.red * (0.04 * alpha), 3.0 * scale),
                ..Default::default()
            },
            SOLID,
        );
    }

    fn orb(&mut self, order: usize, id: &str, radius: f32, tone: Tone, place: Placement) {
        let age = self.scene.v(id, "burst");
        self.orb_shell(order, id, radius, tone, place, age);
        if age >= combustion::COLLAPSE {
            let opacity = self.scene.unit(id, "opacity");
            if opacity > 0.001 {
                self.burst(order, id, radius, place, age, opacity);
            }
        }
        if age < 0.0 && self.scene.v(id, "charge") > 0.0 {
            let scene = self.scene;
            let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * place.scale;
            let opacity = scene.unit(id, "opacity");
            self.crackle(id, place.outline, tone, place.scale, opacity, blur);
            self.frame.close(place.depth - 0.1, order);
        }
    }

    /// Lightning from its discharge clock: a dim stepped leader, then
    /// white-hot return strokes that re-roll the path a few frames apart,
    /// each flashing both contacts and throwing sparks from the receiver,
    /// then a cooling channel. A humming bolt re-strikes continuously.
    /// Its light on nearby rims comes from `Scene::lights_of`.
    fn bolt(
        &mut self,
        order: usize,
        id: &str,
        ends: BoltEnds,
        strikes: u32,
        branching: f32,
        tone: Tone,
    ) {
        let scene = self.scene;
        let opacity = scene.unit(id, "opacity");
        if opacity <= 0.001 {
            return;
        }
        let seed = scene.bolt_seed(id);
        let discharge = Discharge::new(strikes, seed);
        let age = scene.v(id, "age");
        let own = self.look.tone(tone);
        let white = Vec3::ONE.lerp(own, 0.1);
        let scale = ends.scale();
        let blur = scene.blur_at(ends.near());
        let (a, b) = (ends.from.point, ends.to.point);
        if let Some(progress) = discharge.leader(age) {
            // The leader feels its way out along the first stroke's channel:
            // dim, branching, brightest at its tip.
            let bolt = lightning::bolt(a, b, discharge.roll(0), branching);
            let glow = 0.5 * opacity;
            let leader = Plasma {
                half: 0.7 * scale,
                taper: 0.0,
                blur,
                core: (white * 0.6 + own * 0.6) * glow,
                corona: own * (0.7 * glow),
                radius: 4.0 * scale,
            };
            self.frame.plasma(
                &bolt.main,
                progress,
                |f| 0.35 + 0.65 * (f / progress).clamp(0.0, 1.0).powi(6),
                leader,
            );
            for branch in bolt.branches.iter().filter(|branch| branch.fork < progress) {
                let grown = ((progress - branch.fork) / 0.3).min(1.0);
                self.frame.plasma(
                    &branch.points,
                    grown,
                    |_| 0.5 * branch.energy,
                    Plasma {
                        half: 0.5 * scale,
                        taper: 0.6,
                        ..leader
                    },
                );
            }
            let tip = Polyline::new(bolt.main.clone()).at(progress);
            self.flare(
                tip,
                2.5 * scale,
                white * (2.5 * glow),
                own * (0.5 * glow),
                18.0 * scale,
            );
        }
        for (k, since) in discharge.glowing(age) {
            let core = discharge.core(k, since) * opacity;
            let glow = discharge.afterglow(k, since) * opacity;
            let bolt = lightning::bolt(a, b, discharge.roll(k), branching);
            self.frame.plasma(
                &bolt.main,
                1.0,
                |_| 1.0,
                Plasma {
                    half: 0.85 * scale,
                    taper: 0.25,
                    blur,
                    core: white * (3.4 * core) + own * (1.2 * glow),
                    corona: own * (1.3 * core + 0.6 * glow),
                    radius: 4.5 * scale,
                },
            );
            // Branches carry less current and cool first.
            for branch in &bolt.branches {
                let share = branch.energy * if branch.depth > 1 { 0.75 } else { 1.0 };
                self.frame.plasma(
                    &branch.points,
                    1.0,
                    |_| 1.0,
                    Plasma {
                        half: 0.65 * scale * share.sqrt(),
                        taper: 0.85,
                        blur,
                        core: white * (3.4 * core * share) + own * (0.4 * glow * share),
                        corona: own * (1.2 * core * share + 0.15 * glow * share),
                        radius: 3.5 * scale,
                    },
                );
            }
            // The contact flashes; the source flickers with it.
            self.flare(
                b,
                2.6 * scale,
                white * (3.5 * core),
                own * (1.0 * core + 0.25 * glow),
                30.0 * scale,
            );
            self.flare(
                a,
                1.8 * scale,
                white * (2.0 * core),
                own * (0.5 * core),
                18.0 * scale,
            );
            for index in 0..lightning::spark_count(k) {
                if let Some(spark) = lightning::spark(seed, k, index, ends.to.normal, since) {
                    let color =
                        own.lerp(vec3(4.0, 3.6, 3.0), spark.heat) * (0.25 + 0.75 * spark.heat);
                    let alpha = spark.opacity * opacity;
                    let (head, tail) = (
                        b + spark.offset * ends.scale[1],
                        b + spark.tail * ends.scale[1],
                    );
                    self.frame.polyline(
                        &Polyline::new(vec![tail, head]),
                        1.0,
                        [1.2 * ends.scale[1], 0.0],
                        Paint {
                            stroke: rgba(color, alpha),
                            glow: glow4(color * (0.12 * alpha), 3.0 * ends.scale[1]),
                            ..Default::default()
                        },
                        COMET,
                    );
                }
            }
        }
        if let Some((roll, energy)) = lightning::hum(scene.time, scene.v(id, "hum"), seed) {
            let energy = energy * opacity;
            let bolt = lightning::bolt(a, b, roll, branching * 0.4);
            let arc = Plasma {
                half: 0.9 * scale,
                taper: 0.0,
                blur,
                core: white * (3.5 * energy),
                corona: own * (1.5 * energy),
                radius: 4.5 * scale,
            };
            self.frame.plasma(&bolt.main, 1.0, |_| 1.0, arc);
            for branch in &bolt.branches {
                self.frame.plasma(
                    &branch.points,
                    1.0,
                    |_| branch.energy,
                    Plasma {
                        half: 0.6 * scale,
                        taper: 0.85,
                        ..arc
                    },
                );
            }
            for end in [a, b] {
                self.flare(
                    end,
                    2.2 * scale,
                    white * (2.0 * energy),
                    own * (0.6 * energy),
                    16.0 * scale,
                );
            }
        }
        self.frame.close(ends.near() - 1.0, order);
    }

    /// A point of light: a hot disc of `radius` and a soft tone-colored bloom
    /// of `reach`, both pure emission.
    fn flare(&mut self, at: Vec2, radius: f32, hot: Vec3, tone: Vec3, reach: f32) {
        if hot.max_element() + tone.max_element() <= 1e-4 {
            return;
        }
        self.frame.circle(
            at,
            [radius, 0.0],
            0.0,
            Paint {
                glow: glow4(hot, radius),
                ..Default::default()
            },
        );
        self.frame.circle(
            at,
            [0.0, 0.0],
            0.0,
            Paint {
                glow: glow4(tone, reach * 0.3),
                ..Default::default()
            },
        );
    }

    /// A forcefield bubble: faint cells, a Fresnel rim, and ripples from what
    /// crosses or strikes it.
    fn shield(&mut self, order: usize, id: &str, tone: Tone, place: Placement) {
        let scene = self.scene;
        let opacity = scene.unit(id, "opacity");
        let up = scene.unit(id, "up");
        let contacts = scene.shield_contacts(id, place);
        let Shape::Circle(circle) = place.outline else {
            return;
        };
        if opacity <= 0.001 || (up <= 0.001 && contacts.is_empty()) {
            return;
        }
        // Each contact is its direction on the sphere scaled by its strength,
        // and its age; a negative age is empty.
        let mut slots = [[0.0, 0.0, 0.0, -1.0]; 4];
        for (slot, contact) in slots.iter_mut().zip(&contacts) {
            let d = contact.direction * contact.strength.max(1e-3);
            *slot = [d.x, d.y, d.z, contact.age];
        }
        self.frame.prims.push(Prim {
            bbox: around(circle.center, Vec2::splat(circle.radius + 14.0)),
            a: [11.0, circle.center.x, circle.center.y, circle.radius],
            b: [up, scene.time, opacity, scene.blur_at(place.depth)],
            fill: rgba(self.look.tone(tone), 1.0),
            light: slots[0],
            pool: slots[1],
            uv: slots[2],
            light_color: slots[3],
            ..Default::default()
        });
        self.frame.close(place.depth - 2.0, order);
    }

    fn orb_shell(
        &mut self,
        order: usize,
        id: &str,
        radius: f32,
        tone: Tone,
        mut place: Placement,
        age: f32,
    ) {
        let (scene, look) = (self.scene, self.look);
        let bursting = age >= 0.0;
        let burst = Burst::sample(age);
        let opacity = scene.unit(id, "opacity") * burst.shell_opacity;
        if opacity <= 0.001 {
            return;
        }
        let collapse = burst.shell_scale;
        place.scale *= collapse;
        let shatter = if bursting {
            0.0
        } else {
            scene.unit(id, "shatter")
        };
        let pulse = scene.v(id, "pulse");
        let hurt = scene.unit(id, "hurt");
        let world_radius = scene.orb_radius(id, radius) * collapse;
        let own = look.tone(tone);
        let red = look.tone(Tone::Error);
        let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * place.scale;
        // A dark, softly feathered body occludes connections behind the shell.
        self.frame.circle(
            place.center,
            [radius * place.scale, 0.0],
            5.0 * place.scale + blur,
            Paint {
                fill: rgba(look.background, opacity * (1.0 - shatter)),
                ..Default::default()
            },
        );
        // Core light, fading as the orb breaks apart.
        let core = (0.025 + 0.075 * pulse.max(0.0)) * (1.0 - shatter) * opacity;
        self.frame.circle(
            place.center,
            [0.0, 0.0],
            0.0,
            Paint {
                glow: glow4(own.lerp(red, hurt) * core, radius * place.scale * 0.42),
                ..Default::default()
            },
        );
        let rotation = scene.orb_rotation(id);
        let contacts = scene.orb_contacts(id, place);
        let dots = self.orbs[id]
            .iter()
            .map(|point| {
                let unit = rotation * point.unit;
                let (displacement, emission) = contacts.iter().fold(
                    (0.0, Vec3::ZERO),
                    |(offset, light), (direction, age, tone, strength)| {
                        let response =
                            surface::impact(*age, unit.dot(*direction).clamp(-1.0, 1.0).acos());
                        (
                            offset + response.displacement * strength,
                            light
                                + look.tone(*tone).lerp(Vec3::ONE, 0.35)
                                    * response.light
                                    * strength,
                        )
                    },
                );
                let offset = shatter_offset(
                    OrbPoint {
                        unit,
                        seed: point.seed,
                    },
                    world_radius,
                    shatter,
                ) + unit * (displacement * world_radius / 150.0);
                (
                    place.world + offset,
                    (1.0 - scene.camera.view_dir(unit).z) * 0.5,
                    point.seed.z,
                    emission,
                )
            })
            .collect::<Vec<_>>();
        self.particles(
            dots,
            place.outline,
            Shell {
                own,
                red,
                opacity,
                shatter,
                hurt,
                pulse,
                blur,
            },
        );
        for (direction, age, tone, strength) in contacts {
            let (angle, emission) = surface::wavefront(age);
            if angle >= PI || emission * strength < 0.01 {
                continue;
            }
            let points = sphere_ring(direction, angle, 72)
                .into_iter()
                .filter_map(|unit| {
                    scene
                        .camera
                        .project(place.world + unit * world_radius)
                        .map(|(point, _)| {
                            (point, smoothstep(-scene.camera.view_dir(unit).z / 0.20))
                        })
                })
                .collect::<Vec<_>>();
            let ink = look.tone(tone).lerp(Vec3::ONE, 0.4);
            let energy = emission * strength * opacity;
            self.frame.trail(
                &points,
                [1.15 * place.scale, 0.0],
                Paint {
                    stroke: rgba(ink * 1.1, energy * 0.55),
                    glow: glow4(ink * energy * 0.055, 4.0 * place.scale),
                    ..Default::default()
                },
            );
        }
        self.frame.close(place.depth, order);
    }

    /// A particle body's points, far to near: each a soft dot whose size and
    /// light grow toward the camera, lit by nearby packets, reddened by
    /// shatter and hurt. A dot is world position, nearness, seed, and its own
    /// emission.
    fn particles(&mut self, mut dots: Vec<(Vec3, f32, f32, Vec3)>, outline: Shape, shell: Shell) {
        let (scene, look) = (self.scene, self.look);
        let Shell {
            own,
            red,
            opacity,
            shatter,
            hurt,
            pulse,
            blur,
        } = shell;
        dots.sort_by(|a, b| scene.camera.depth(b.0).total_cmp(&scene.camera.depth(a.0)));
        let fade = (1.0 - shatter).powf(0.7);
        let lights = scene.lights_on(outline).collect::<Vec<_>>();
        for (point, near, seed, emission) in dots {
            let Some((center, scale)) = scene.camera.project(point) else {
                continue;
            };
            let alpha = (0.1 + 0.75 * near) * opacity * fade;
            if alpha < 0.01 {
                continue;
            }
            let lit = lights.iter().fold(Vec3::ZERO, |sum, light| {
                sum + look.tone(light.tone)
                    * (light.strength * light.falloff(center.distance(light.at)))
            });
            let color = own
                .lerp(Vec3::ONE, 0.16 * near)
                .lerp(red, (shatter * 2.4 + hurt * 0.8).min(1.0))
                + lit * 0.85
                + emission;
            self.frame.circle(
                center,
                [
                    (0.85 + 1.15 * near) * scale * (1.0 + 0.25 * shatter * seed),
                    0.0,
                ],
                blur,
                Paint {
                    fill: rgba(color * (0.6 + 0.3 * near + 0.15 * pulse), alpha),
                    glow: glow4(
                        color * (0.035 * alpha) + emission * (0.10 * alpha),
                        2.5 * scale,
                    ),
                    ..Default::default()
                },
            );
        }
    }

    /// A form: the orb's material on any shape. A dark silhouette occludes
    /// what passes behind it, a faint core lights it, and its points turn,
    /// morph, shatter, and burst. The surface ripple is the orb's alone.
    fn form(&mut self, order: usize, id: &str, tone: Tone, place: Placement) {
        let scene = self.scene;
        let Some(form) = scene.forms.get(id) else {
            return;
        };
        let age = scene.v(id, "burst");
        let burst = Burst::sample(age);
        let opacity = scene.unit(id, "opacity") * burst.shell_opacity;
        if opacity > 0.001 {
            let look = self.look;
            let collapse = burst.shell_scale;
            let shatter = if age >= 0.0 {
                0.0
            } else {
                scene.unit(id, "shatter")
            };
            let pulse = scene.v(id, "pulse");
            let hurt = scene.unit(id, "hurt");
            let own = look.tone(tone);
            let red = look.tone(Tone::Error);
            let scale = place.scale * collapse;
            let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * scale;
            // A hollow form (`solid` 0), such as a ring of dots, hides nothing.
            let solid = scene.unit(id, "solid");
            self.frame.polygon(
                form.hull.scaled(collapse).vertices(),
                0.0,
                5.0 * scale + blur,
                Paint {
                    fill: rgba(look.background, opacity * (1.0 - shatter) * solid),
                    ..Default::default()
                },
            );
            let core = (0.025 + 0.075 * pulse.max(0.0)) * (1.0 - shatter) * opacity;
            self.frame.circle(
                place.center,
                [0.0, 0.0],
                0.0,
                Paint {
                    glow: glow4(own.lerp(red, hurt) * core, form.radius * scale * 0.42),
                    ..Default::default()
                },
            );
            let dots = form
                .dots
                .iter()
                .map(|(offset, near, seed)| {
                    let offset = *offset * collapse;
                    let burst = 1.0 + shatter * (0.6 + 2.4 * seed.x);
                    let fall = shatter * shatter * (160.0 + 460.0 * seed.y);
                    let drift = shatter * (seed.z - 0.5) * 120.0;
                    (
                        place.world + offset * burst + vec3(drift, fall, 0.0),
                        *near,
                        seed.z,
                        Vec3::ZERO,
                    )
                })
                .collect();
            self.particles(
                dots,
                place.outline,
                Shell {
                    own,
                    red,
                    opacity,
                    shatter,
                    hurt,
                    pulse,
                    blur,
                },
            );
            self.frame.close(place.depth, order);
        }
        let opacity = scene.unit(id, "opacity");
        if age >= combustion::COLLAPSE && opacity > 0.001 && age < combustion::DURATION {
            let radius_px = form.radius * place.scale;
            let points = form
                .dots
                .iter()
                .enumerate()
                .map(|(index, (offset, _, seed))| {
                    (
                        index,
                        offset.normalize_or(Vec3::Y),
                        place.world + *offset * burst.shell_scale,
                        *seed,
                    )
                })
                .collect::<Vec<_>>();
            self.embers(order, place, radius_px, age, opacity, points);
        }
    }

    /// One deterministic impact clock owns collapse, combustion, smoke, and
    /// ballistic embers. Reverse this clock to reassemble the same performance.
    fn burst(
        &mut self,
        order: usize,
        id: &str,
        radius: f32,
        place: Placement,
        age: f32,
        opacity: f32,
    ) {
        if age >= combustion::DURATION {
            return;
        }
        let radius_px = radius * place.scale;
        let burst = Burst::sample(age);
        let rotation = self.scene.orb_rotation(id);
        let world_radius = self.scene.orb_radius(id, radius);
        let points = self.orbs[id]
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let unit = rotation * point.unit;
                (
                    index,
                    unit,
                    place.world + unit * (world_radius * burst.shell_scale),
                    point.seed,
                )
            })
            .collect();
        self.embers(order, place, radius_px, age, opacity, points);
    }

    /// The burst's procedural fire volume and its ballistic embers, each
    /// leaving its `anchor` on the compressed shell along unit `direction`.
    fn embers(
        &mut self,
        order: usize,
        place: Placement,
        radius_px: f32,
        age: f32,
        opacity: f32,
        points: Vec<(usize, Vec3, Vec3, Vec3)>,
    ) {
        self.frame.prims.push(Prim {
            bbox: around(place.center, Vec2::splat(radius_px * 4.4)),
            a: [6.0, place.center.x, place.center.y, radius_px],
            b: [age, opacity, 0.0, 0.0],
            ..Default::default()
        });
        let burst = Burst::sample(age);
        for (index, unit, anchor, seed) in points {
            let ember = burst.ember(unit, seed);
            let position = anchor + ember.offset;
            let Some((center, perspective)) = self.scene.camera.project(position) else {
                continue;
            };
            let ignition = burst.ignition;
            let fade = ember.opacity * opacity;
            if fade <= 0.001 {
                continue;
            }
            let hot = vec3(4.5, 1.6, 0.35).lerp(vec3(0.8, 0.025, 0.006), 1.0 - ember.heat);
            let color = self.look.tone(Tone::Accent).lerp(hot, ignition);
            if index % 4 == 0 && ignition > 0.0 {
                let tail = anchor + ember.tail_offset;
                if let Some((tail, _)) = self.scene.camera.project(tail) {
                    self.frame.polyline(
                        &Polyline::new(vec![tail, center]),
                        1.0,
                        [1.3 * perspective, 0.0],
                        Paint {
                            stroke: rgba(color, fade * 0.8),
                            glow: glow4(color * fade * 0.12, 3.0 * perspective),
                            ..Default::default()
                        },
                        COMET,
                    );
                }
            }
            self.frame.circle(
                center,
                [ember.radius * perspective, 0.0],
                0.0,
                Paint {
                    fill: rgba(color, fade),
                    glow: glow4(color * fade * ignition * 0.15, 3.0 * perspective),
                    ..Default::default()
                },
            );
        }
        self.frame.close(place.depth, order);
    }

    fn beam(&mut self, order: usize, id: &str, tone: Tone, link: &Link) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        if opacity <= 0.001 {
            return;
        }
        let draw = scene.unit(id, "draw");
        let broken = scene.unit(id, "break");
        let flow = scene.v(id, "flow").clamp(0.0, 1.5);
        let emphasis = scene.unit(id, "emphasis");
        let surge = scene.v(id, "surge").clamp(0.0, 1.5);
        let own = look.tone(tone);
        let scale = link.scale_at(0.5);
        let blur = scene.blur_at(link.far());
        // Idle wires are matte. Only a contact surge briefly emits light.
        let color = (look.muted * 0.7)
            .lerp(own, 0.12 + 0.45 * emphasis)
            .lerp(own, 0.35 * surge.min(1.0))
            .lerp(look.tone(Tone::Error), (broken * 3.0).min(1.0));
        let line = Paint {
            stroke: rgba(
                color,
                0.75 * opacity * (1.0 - 0.65 * broken) * (1.0 + 0.3 * surge),
            ),
            glow: glow4(color * (0.05 * surge * opacity), 4.0 * scale),
            ..Default::default()
        };
        let width = 1.4 * scale * (1.0 + 0.25 * surge);
        if broken <= 0.001 {
            self.frame
                .polyline(&link.path, draw, [width, blur], line, SOLID);
        } else {
            // Snapped in the middle, both halves recoil all the way to their ends.
            let keep = 0.5 * (1.0 - broken).powf(1.5);
            for half in [link.path.slice(0.0, keep), link.path.slice(1.0 - keep, 1.0)] {
                self.frame.polyline(&half, 1.0, [width, blur], line, SOLID);
            }
        }
        if flow > 0.001 && broken <= 0.001 && draw > 0.98 {
            // Small beads of light travel toward the `to` end.
            let bead = Paint {
                stroke: rgba(own, (flow * opacity * 0.65).min(1.0)),
                ..Default::default()
            };
            let beads = [7.0 * scale, 190.0 * scale, -scene.time * 90.0 * scale, 0.0];
            self.frame
                .polyline(&link.path, 1.0, [1.8 * scale, blur], bead, beads);
        }
        // Behind both ends, so a beam never crosses the cards it connects.
        self.frame.close(link.far() + 1.0, order);
        // Before drawing, light runs once around the source card's frame, from
        // its port back to it.
        let sweep = scene.v(id, "sweep");
        if sweep > 0.001
            && sweep < 0.999
            && let Some(frame) = link.source
        {
            let scale = link.scale[0];
            let trace = frame.perimeter_from(link.path.at(0.0), 14.0 * scale);
            let dash = 36.0 * DIAGRAM_SCALE * scale / trace.length().max(1.0);
            let light = bead(sweep) * opacity;
            self.frame.polyline(
                &trace.slice(sweep - dash, sweep),
                1.0,
                [1.5 * DIAGRAM_SCALE * scale, blur],
                Paint {
                    stroke: rgba(own.lerp(Vec3::ONE, 0.6) * 1.3, light),
                    glow: glow4(own * (0.3 * light), 6.0 * scale),
                    ..Default::default()
                },
                COMET,
            );
            self.frame.close(link.depth[0] - 0.3, order);
        }
        // Fixed-size sockets resolve softly; connecting alone does not strike
        // or overshoot the receiver.
        let source = if draw > 0.001 {
            1.0
        } else {
            scene.unit(id, "port")
        };
        for end in [0, 1] {
            let shown = if end == 0 {
                source
            } else {
                smoothstep(remap_clamp(draw, [0.92, 1.0], [0.0, 1.0]))
            };
            if !link.socket[end] || shown <= 0.001 {
                continue;
            }
            let scale = link.scale[end];
            self.frame.circle(
                link.path.at(end as f32),
                [4.4 * scale, 1.3 * scale],
                blur,
                Paint {
                    fill: rgba(color, opacity * shown),
                    stroke: rgba(look.background, opacity * shown),
                    ..Default::default()
                },
            );
            self.frame.close(link.depth[end] - 0.25, order);
        }
    }

    /// A packet's whole life from its clock: light gathers at the port, a solid
    /// dot flies with a cooling trail, then it opens into a small ring as it is
    /// absorbed. Its light on nearby edges comes from `Scene::lights_of`.
    /// One leg of a packet, on its own clock (`age`, `flight`), along `link`
    /// oriented the way it travels.
    fn packet(&mut self, order: usize, id: &str, tone: Tone, age: f32, flight: f32, link: &Link) {
        let scene = self.scene;
        let opacity = scene.unit(id, "opacity");
        if !(0.0..packet::LIFETIME).contains(&age) || opacity <= 0.001 {
            return;
        }
        let (path, scale_at) = (&link.path, |fraction| link.scale_at(fraction));
        let own = self.look.tone(tone);
        // The dot is nearly white; its tone lives in the trail and its reflections.
        let ink = own.lerp(Vec3::ONE, 0.55) * 1.3;
        let dot = 4.0 * DIAGRAM_SCALE;
        if let Some(g) = packet::gather(age) {
            // A soft disc closes in on the port while the dot grows in.
            let g = cubic_out(g);
            let (port, scale) = (path.at(0.0), scale_at(0.0));
            self.frame.circle(
                port,
                [(4.0 + 14.0 * (1.0 - g)) * DIAGRAM_SCALE * scale, 0.0],
                6.0 * scale,
                Paint {
                    fill: rgba(ink * 0.7, 0.5 * (PI * g).sin() * opacity),
                    ..Default::default()
                },
            );
            self.frame.circle(
                port,
                [dot * g * scale, 0.0],
                0.0,
                Paint {
                    fill: rgba(ink, g.powf(1.5) * opacity),
                    ..Default::default()
                },
            );
        }
        self.trail(path, age, flight, own * opacity, scale_at(0.5));
        let label_alpha = if packet::flight(age, flight).is_some() {
            let travel = packet::travel(age, flight);
            let (head, scale) = (path.at(travel), scale_at(travel));
            self.frame.circle(
                head,
                [dot * scale, 0.0],
                0.0,
                Paint {
                    fill: rgba(ink, opacity),
                    glow: glow4(own * (0.3 * opacity), 6.0 * scale),
                    ..Default::default()
                },
            );
            let label = remap_clamp(travel, [0.0, 0.1], [0.0, 1.0]);
            if link.landing[1] {
                label
            } else {
                label * (1.0 - smoothstep((travel - 0.65) / 0.25))
            }
        } else {
            if link.landing[1] {
                packet::landing(age, flight).map_or(0.0, |q| 1.0 - smoothstep(q / 0.4))
            } else {
                0.0
            }
        };
        if label_alpha > 0.001 {
            let travel = packet::travel(age, flight);
            let scale = scale_at(travel);
            let key = text_key(id, "label");
            let at = path.at(travel) - vec2(0.0, 26.0 * scale);
            if let Some(at) = self
                .frame
                .packet_label_at(&key, at, scale, link.label_ports)
            {
                self.frame.text(
                    &key,
                    at,
                    scale,
                    CaptionAlign::Center,
                    rgba(own, opacity * label_alpha),
                    f32::MAX,
                    0.0,
                );
            }
        }
        if let Some(q) = packet::landing(age, flight)
            && link.landing[1]
        {
            // The dot is the ring: a 2 px ring with a 4 px stroke looks like the
            // dot, then opens, grows a little, and fades out.
            let opening = smoothstep(q / 0.24);
            let scale = scale_at(1.0);
            self.frame.arc(
                path.at(1.0),
                [
                    (2.0 + 2.0 * opening + 9.1 * quad_out(q)) * DIAGRAM_SCALE * scale,
                    (4.0 - 2.5 * opening) * DIAGRAM_SCALE * scale,
                ],
                1.0,
                0.0,
                Paint {
                    stroke: rgba(
                        ink,
                        (1.0 - 0.734 * opening) * (1.0 - q).powf(2.52) * opacity,
                    ),
                    ..Default::default()
                },
            );
        }
        self.frame.close(link.far() + 0.5, order);
    }

    /// Every point the packet crossed within the cooling time, dimming with
    /// the time since it crossed: long and bright mid-flight, short near the
    /// ends, and still cooling after it lands.
    fn trail(&mut self, path: &Polyline, age: f32, flight: f32, color: Vec3, scale: f32) {
        let head = packet::travel(age, flight);
        let tail = packet::travel(age - packet::COOLING, flight);
        if head - tail <= 1e-4 {
            return;
        }
        let stretch = path.slice(tail, head);
        let length = path.length().max(1.0);
        let points = stretch
            .points()
            .iter()
            .zip(stretch.lengths())
            .map(|(point, along)| {
                let fraction = (tail * length + along) / length;
                let since = packet::since_crossing(age, flight, fraction);
                (*point, since.map_or(0.0, packet::heat))
            })
            .collect::<Vec<_>>();
        self.frame.trail(
            &points,
            [1.6 * DIAGRAM_SCALE * scale, 0.0],
            Paint {
                stroke: rgba(color * 1.25, 1.0),
                glow: glow4(color * 0.2, 5.0 * scale),
                ..Default::default()
            },
        );
    }

    fn label(
        &mut self,
        order: usize,
        id: &str,
        align: CaptionAlign,
        spans: &[CaptionSpanPlan],
        place: Placement,
    ) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        if opacity <= 0.001 {
            return;
        }
        let typed = scene.unit(id, "typed");
        let blur = scene.blur_at(place.depth);
        let parts = spans
            .iter()
            .enumerate()
            .filter(|(_, span)| !span.text.is_empty())
            .map(|(index, span)| (text_key(id, &format!("span{index}")), span))
            .collect::<Vec<_>>();
        let widths = parts
            .iter()
            .map(|(key, _)| self.frame.width(key, place.scale))
            .collect::<Vec<_>>();
        let total: f32 = widths.iter().sum();
        let chars: usize = parts
            .iter()
            .map(|(_, span)| span.text.chars().count())
            .sum();
        let mut remaining = if typed >= 1.0 {
            usize::MAX
        } else {
            (typed * chars as f32 + 1e-3).floor() as usize
        };
        let mut x = place.center.x
            - match align {
                CaptionAlign::Left => 0.0,
                CaptionAlign::Center => total * 0.5,
                CaptionAlign::Right => total,
            };
        for ((key, span), width) in parts.iter().zip(&widths) {
            let count = span.text.chars().count();
            let shown = remaining.min(count);
            if shown > 0 {
                let reveal = if shown == count {
                    f32::MAX
                } else {
                    width * shown as f32 / count as f32
                };
                self.frame.text(
                    key,
                    vec2(x, place.center.y),
                    place.scale,
                    CaptionAlign::Left,
                    rgba(look.tone(span.tone), opacity),
                    reveal,
                    blur,
                );
            }
            remaining -= shown;
            x += width;
        }
        self.frame.close(place.depth - 0.5, order);
    }

    /// `size` is the radius and thickness.
    fn ring(&mut self, order: usize, id: &str, size: [f32; 2], tone: Tone, place: Placement) {
        let scene = self.scene;
        let opacity = scene.unit(id, "opacity");
        let expand = scene.unit(id, "expand");
        let alpha = opacity * (1.0 - expand);
        if alpha <= 0.001 {
            return;
        }
        let own = self.look.tone(tone);
        self.frame.arc(
            place.center,
            [
                size[0] * (1.0 + 1.3 * expand) * place.scale,
                size[1] * place.scale,
            ],
            scene.unit(id, "sweep"),
            scene.blur_at(place.depth),
            Paint {
                stroke: rgba(own, alpha),
                glow: glow4(own * (0.035 * alpha), 5.0 * place.scale),
                ..Default::default()
            },
        );
        self.frame.close(place.depth, order);
    }
}

/// How a particle body's points are lit this sample.
#[derive(Clone, Copy)]
struct Shell {
    own: Vec3,
    red: Vec3,
    opacity: f32,
    shatter: f32,
    hurt: f32,
    pulse: f32,
    blur: f32,
}

/// A shape element's figure and paint, from its payload.
#[derive(Clone, Copy)]
struct FigureStyle<'a> {
    figure: &'a Figure,
    corner: f32,
    fill: Option<Fill>,
    fill_opacity: f32,
    stroke: Option<Tone>,
    width: f32,
    dash: Option<[f32; 2]>,
    arrow: Arrow,
}

impl Painter<'_> {
    /// A flat figure: an optional fill, then a stroke that draws on along its
    /// outline from twelve o'clock, clockwise. It turns about its center. A
    /// flash lifts the stroke, never the whole fill.
    fn shape(&mut self, order: usize, id: &str, style: FigureStyle, place: Placement) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        if opacity <= 0.001 {
            return;
        }
        let scale = place.scale;
        let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * scale;
        let draw = scene.unit(id, "draw");
        let emphasis = scene.unit(id, "emphasis");
        let flash = scene.v(id, "flash").clamp(0.0, 1.5);
        let sized_rect = match style.figure {
            Figure::Rect(size) => Some(Figure::Rect([
                (size[0] + scene.v(id, "width")).max(1.0),
                (size[1] + scene.v(id, "height")).max(1.0),
            ])),
            _ => None,
        };
        let figure = sized_rect.as_ref().unwrap_or(style.figure);
        let outline = figure_outline(figure, style.corner, scene.v(id, "rotation"))
            .into_iter()
            .map(|point| place.center + point * scale)
            .collect::<Vec<_>>();
        let closed = !matches!(figure, Figure::Arc { .. });
        // Like a card: the strongest reflection rides the outline, and the
        // strongest pool floods the fill from where an arrival entered.
        let strongest = |pool: bool| {
            scene
                .lights_on(place.outline)
                .filter(|light| light.pool == pool)
                .max_by(|a, b| a.strength.total_cmp(&b.strength))
                .map_or(([0.0; 4], [0.0; 4]), |light| light.uniform(&look, opacity))
        };
        let (light, light_color) = strongest(false);
        let (pool, pool_color) = strongest(true);
        if let (
            Figure::Rect(size),
            Some(Fill::Material(material @ (Material::Raised | Material::Well))),
        ) = (figure, style.fill)
            && scene.v(id, "rotation") == 0.0
            && style.dash.is_none()
            && style.arrow == Arrow::None
        {
            let half = Vec2::from(*size) * 0.5 * scale;
            let corner = style.corner * scale;
            let amount = scene.unit(id, "fill") * style.fill_opacity * opacity;
            let own = look.tone(style.stroke.unwrap_or(Tone::Muted));
            let gray = look.raised.lerp(
                look.muted,
                if material == Material::Raised {
                    0.38
                } else {
                    0.22
                },
            );
            let edge = gray.lerp(own, (0.55 * emphasis + 0.65 * flash).min(1.0));
            let settled_border = draw >= 0.999 && style.stroke.is_some();
            let border = if settled_border {
                style.width.max(1.0) * scale
            } else {
                0.0
            };
            if material == Material::Raised && amount > 0.001 {
                self.frame.rounded_rect(
                    place.center + vec2(0.0, 5.0 * scale),
                    half + Vec2::splat(2.0 * scale),
                    [corner + 2.0 * scale, 0.0],
                    blur + 7.0 * scale,
                    Paint {
                        fill: rgba(look.background, 0.68 * amount),
                        ..Default::default()
                    },
                );
            }
            let surface = match material {
                Material::Raised => look.surface.lerp(look.raised, 0.44 + 0.16 * emphasis),
                Material::Well => look.background.lerp(look.surface, 0.48),
                _ => look.surface,
            };
            self.frame.rounded_rect(
                place.center,
                half,
                [corner, border],
                blur,
                Paint {
                    fill: rgba(surface, amount),
                    stroke: rgba(edge, opacity * (0.9 + 0.1 * emphasis)),
                    glow: glow4(
                        own * ((0.018 * emphasis + 0.05 * flash) * opacity),
                        6.0 * scale,
                    ),
                    light,
                    light_color,
                    pool,
                    pool_color,
                },
            );
            if settled_border {
                if material == Material::Raised && size[0].min(size[1]) >= 48.0 {
                    self.frame.rounded_rect(
                        place.center,
                        half - 3.0 * scale,
                        [(style.corner - 3.0).max(2.0) * scale, 1.0],
                        blur,
                        Paint {
                            stroke: rgba(look.raised.lerp(look.muted, 0.08), opacity * 0.85),
                            ..Default::default()
                        },
                    );
                } else if material == Material::Well {
                    self.frame.rounded_rect(
                        place.center,
                        half - 2.0 * scale,
                        [(style.corner - 2.0).max(2.0) * scale, 1.5 * scale],
                        blur,
                        Paint {
                            stroke: rgba(look.background, 0.65 * amount),
                            ..Default::default()
                        },
                    );
                }
                self.frame.close(place.depth, order);
                return;
            }
        } else if let Some(fill) = style.fill
            && closed
        {
            let color = match fill {
                Fill::Tone(tone) => look.tone(tone),
                Fill::Material(Material::Surface | Material::Raised) => look.surface,
                Fill::Material(Material::Background | Material::Well) => look.background,
            };
            let amount = scene.unit(id, "fill") * style.fill_opacity * opacity;
            self.frame.polygon(
                &outline,
                0.0,
                blur,
                Paint {
                    fill: rgba(color, amount),
                    pool,
                    pool_color,
                    ..Default::default()
                },
            );
        }
        if let Some(tone) = style.stroke {
            let own = look.tone(tone);
            let color = own.lerp(Vec3::ONE, (0.18 * emphasis + 0.35 * flash).min(1.0));
            let alpha = opacity * (0.85 + 0.15 * emphasis);
            let paint = Paint {
                stroke: rgba(color, alpha),
                glow: glow4(own * ((0.04 * emphasis + 0.1 * flash) * alpha), 4.0 * scale),
                light,
                light_color,
                ..Default::default()
            };
            let mut line = outline.clone();
            if closed {
                line.push(outline[0]);
            }
            let line = Polyline::new(line);
            let width = style.width * scale;
            let length = line.length().max(1e-3);
            let drawn = draw * length;
            let heads = [style.arrow.at_start(), style.arrow.at_end()];
            let cut = self.arrowheads(&line, [0.0, drawn], heads, width, blur, paint);
            let line = line.slice(cut[0] / length, (drawn - cut[1]) / length);
            let dash = style
                .dash
                .map_or(SOLID, |[on, off]| [on * scale, off * scale, cut[0], 0.0]);
            if draw > 0.0 {
                self.frame.polyline(&line, 1.0, [width, blur], paint, dash);
            }
        }
        self.frame.close(place.depth, order);
    }

    /// Arrowheads on `line` at the ends of its visible `span` (lengths along
    /// it) that carry `heads`: the start's points back along the line, the
    /// end's forward. A head shrinks when the visible line is shorter than
    /// it, so a drawing arrow grows from nothing. Returns how far each head
    /// cuts the line back.
    fn arrowheads(
        &mut self,
        line: &Polyline,
        span: [f32; 2],
        heads: [bool; 2],
        width: f32,
        blur: f32,
        paint: Paint,
    ) -> [f32; 2] {
        let mut cut = [0.0; 2];
        let full = 7.0 + 4.2 * width;
        let visible = (span[1] - span[0]).max(0.0);
        for (end, tip) in span.into_iter().enumerate() {
            if !heads[end] {
                continue;
            }
            let size = full.min(visible * if heads[1 - end] { 0.5 } else { 1.0 });
            if size <= 0.5 {
                continue;
            }
            let back = if end == 0 { tip + size } else { tip - size };
            let (point, base) = (line.at_length(tip), line.at_length(back));
            let along = (point - base).normalize_or(Vec2::X);
            let side = along.perp() * (size * 0.48);
            let base = point - along * size;
            self.frame.polygon(
                &[
                    point,
                    base - side,
                    point - along * (size * 0.74),
                    base + side,
                ],
                0.0,
                blur,
                Paint {
                    fill: paint.stroke,
                    glow: paint.glow,
                    ..Default::default()
                },
            );
            cut[end] = size * 0.7;
        }
        cut
    }

    /// A path's legs: each the visible stretch between `trim` and `draw` of
    /// the whole route, with arrowheads riding the drawn tip, sockets where
    /// it plugs into cards, and flow when traffic is live.
    #[allow(clippy::too_many_arguments)]
    fn path(
        &mut self,
        order: usize,
        id: &str,
        tone: Tone,
        width: f32,
        dash: Option<[f32; 2]>,
        arrow: Arrow,
        legs: &[Link],
    ) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        if opacity <= 0.001 || legs.is_empty() {
            return;
        }
        let draw = scene.unit(id, "draw");
        let trim = scene.unit(id, "trim");
        let flow = scene.v(id, "flow").clamp(0.0, 1.5);
        let emphasis = scene.unit(id, "emphasis");
        let surge = scene.v(id, "surge").clamp(0.0, 1.5);
        let own = look.tone(tone);
        // Matte like a beam; emphasis and surge bring up its tone.
        let color = (look.muted * 0.7)
            .lerp(own, 0.12 + 0.45 * emphasis)
            .lerp(own, 0.35 * surge.min(1.0));
        let far = legs.iter().map(Link::far).fold(f32::MIN, f32::max);
        let blur = scene.blur_at(far);
        let lengths = legs.iter().map(|leg| leg.path.length()).collect::<Vec<_>>();
        let total = lengths.iter().sum::<f32>().max(1e-3);
        let (shown_from, shown_to) = (trim * total, draw * total);
        let mut start = 0.0;
        for (leg, length) in legs.iter().zip(&lengths) {
            let scale = leg.scale_at(0.5);
            let paint = Paint {
                stroke: rgba(color, 0.75 * opacity * (1.0 + 0.3 * surge)),
                glow: glow4(color * (0.05 * surge * opacity), 4.0 * scale),
                ..Default::default()
            };
            let line_width = width * scale * (1.0 + 0.25 * surge);
            // The visible stretch of this leg, inside its tips.
            let low = (shown_from - start).max(leg.tips[0] * length);
            let high = (shown_to - start).min(leg.tips[1] * length);
            if high > low {
                let heads = [arrow.at_start(), arrow.at_end()];
                let cut = self.arrowheads(&leg.path, [low, high], heads, line_width, blur, paint);
                let line = leg
                    .path
                    .slice((low + cut[0]) / length, (high - cut[1]) / length);
                let style = dash.map_or(SOLID, |[on, off]| {
                    [on * scale, off * scale, start + low + cut[0], 0.0]
                });
                self.frame
                    .polyline(&line, 1.0, [line_width, blur], paint, style);
                if flow > 0.001 && draw > 0.98 {
                    let bead = Paint {
                        stroke: rgba(own, (flow * opacity * 0.65).min(1.0)),
                        ..Default::default()
                    };
                    let beads = [7.0 * scale, 190.0 * scale, -scene.time * 90.0 * scale, 0.0];
                    self.frame
                        .polyline(&line, 1.0, [1.8 * scale, blur], bead, beads);
                }
            }
            // Sockets resolve where the line plugs into a card.
            for end in [0, 1] {
                if !leg.socket[end] {
                    continue;
                }
                let at = start + end as f32 * length;
                let shown = if end == 0 {
                    smoothstep(remap_clamp(shown_to - at, [0.0, 8.0], [0.0, 1.0]))
                } else {
                    smoothstep(remap_clamp(shown_to - at, [-8.0, 0.0], [0.0, 1.0]))
                } * smoothstep(remap_clamp(at - shown_from, [-8.0, 0.0], [0.0, 1.0]));
                if shown <= 0.001 {
                    continue;
                }
                let scale = leg.scale[end];
                self.frame.circle(
                    leg.path.at(end as f32),
                    [4.4 * scale, 1.3 * scale],
                    blur,
                    Paint {
                        fill: rgba(color, opacity * shown),
                        stroke: rgba(look.background, opacity * shown),
                        ..Default::default()
                    },
                );
            }
            start += length;
        }
        // Behind every element it connects.
        self.frame.close(far + 1.0, order);
    }

    /// An icon: its atlas coverage tinted by its tone, `size` world pixels
    /// square through the camera. A flash lifts its ink toward white.
    #[allow(clippy::too_many_arguments)]
    fn icon(
        &mut self,
        order: usize,
        id: &str,
        icon: &str,
        size: f32,
        tone: Tone,
        ink: Option<[u8; 3]>,
        place: Placement,
    ) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        let key = text_key(id, "icon");
        let Some(rect) = self.frame.texts.get(&key).map(|text| text.rect) else {
            return;
        };
        if opacity <= 0.001 {
            return;
        }
        let flash = scene.v(id, "flash").clamp(0.0, 1.5);
        let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * place.scale;
        let own = ink.map(linear3).unwrap_or_else(|| match tone {
            Tone::Plain => look.text,
            tone => look.tone(tone),
        });
        let color = own.lerp(Vec3::ONE, (0.4 * flash).min(1.0));
        let side = size * place.scale;
        let top_left = place.center - Vec2::splat(side * 0.5);
        let uv = [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]];
        if matches!(icon, "cursor" | "hand-pointing" | "cursor-text") {
            let shadow = top_left + vec2(2.0, 4.5) * place.scale;
            self.frame.prims.push(Prim {
                bbox: around(
                    place.center + vec2(2.0, 4.5) * place.scale,
                    Vec2::splat(side * 0.5 + 8.0 * place.scale),
                ),
                a: [4.0, shadow.x, shadow.y, blur * 0.5 + 3.2 * place.scale],
                b: [side, side, f32::MAX, 0.0],
                fill: rgba(look.background, 0.78 * opacity),
                uv,
                ..Default::default()
            });
            for offset in [
                vec2(-1.8, 0.0),
                vec2(1.8, 0.0),
                vec2(0.0, -1.8),
                vec2(0.0, 1.8),
                vec2(-1.3, -1.3),
                vec2(1.3, -1.3),
                vec2(-1.3, 1.3),
                vec2(1.3, 1.3),
            ] {
                let rim = top_left + offset * place.scale;
                self.frame.prims.push(Prim {
                    bbox: around(place.center, Vec2::splat(side * 0.5 + 4.0 * place.scale)),
                    a: [4.0, rim.x, rim.y, blur * 0.5 + 0.35 * place.scale],
                    b: [side, side, f32::MAX, 0.0],
                    fill: rgba(look.background, 0.94 * opacity),
                    uv,
                    ..Default::default()
                });
            }
        }
        self.frame.prims.push(Prim {
            bbox: around(place.center, Vec2::splat(side * 0.5 + 2.0)),
            a: [4.0, top_left.x, top_left.y, blur * 0.5],
            b: [side, side, f32::MAX, 0.0],
            fill: rgba(color, opacity),
            uv,
            ..Default::default()
        });
        if flash > 0.001 {
            // Light from the struck icon itself, under the bloom threshold.
            self.frame.circle(
                place.center,
                [0.0, 0.0],
                0.0,
                Paint {
                    glow: glow4(own * (0.05 * flash * opacity), side * 0.35),
                    ..Default::default()
                },
            );
        }
        self.frame.close(place.depth - 0.5, order);
    }

    /// Footage on a camera-facing quad: the current frame's fit and focus
    /// window, cut to its mask, defocused with depth, color-treated, and
    /// optionally on a card's mat with a rim.
    #[allow(clippy::too_many_arguments)]
    fn footage(
        &mut self,
        order: usize,
        id: &str,
        size: Vec2,
        fit: Fit,
        mask: &Mask,
        framed: bool,
        tint: Tone,
        place: Placement,
    ) {
        let (scene, look) = (self.scene, self.look);
        let opacity = scene.unit(id, "opacity");
        let Some(slot) = self.footage.get(id) else {
            return;
        };
        if opacity <= 0.001 || slot.shown.get().is_none() {
            self.frame.close(place.depth, order);
            return;
        }
        let scale = place.scale;
        let blur = scene.blur_at(place.depth) + scene.v(id, "blur").max(0.0) * scale;
        let turn = Vec2::from_angle(scene.v(id, "rotation"));
        let source = [slot.size[0] as f32, slot.size[1] as f32];
        let (window, content) = fit.frame(source, size.to_array());
        let [wx, wy, ww, wh] = focus_window(
            window,
            [scene.v(id, "focus-x"), scene.v(id, "focus-y")],
            scene.v(id, "focus-size"),
        );
        // A contained source letterboxes: the quad is just its content.
        let offset = vec2(
            content[0] + content[2] * 0.5 - size.x * 0.5,
            content[1] + content[3] * 0.5 - size.y * 0.5,
        );
        let center = place.center + turn.rotate(offset * scale);
        let box_size = vec2(content[2], content[3]);
        let half = box_size * 0.5 * scale;
        let corner = match mask {
            Mask::Rect { radius } => radius * scale,
            _ => 0.0,
        };
        let (mask_kind, first, count) = match mask {
            Mask::Rect { .. } => (0.0, 0.0, 0.0),
            Mask::Circle => (1.0, 0.0, 0.0),
            Mask::Polygon { .. } => {
                let first = self.frame.points.len();
                self.frame.points.extend(
                    mask.corners(box_size.to_array())
                        .into_iter()
                        .map(|corner| center + turn.rotate(Vec2::from(corner) * scale))
                        .map(|point| [point.x, point.y, 0.0, 0.0]),
                );
                (2.0, first as f32, (self.frame.points.len() - first) as f32)
            }
        };
        let rim = look.raised.lerp(look.muted, 0.22);
        if framed {
            // A card's mat behind the footage, its rim catching the overhead key.
            let mat = 7.0 * scale;
            let backing = match mask {
                Mask::Circle => half.min_element() + mat,
                _ => 0.0,
            };
            if backing > 0.0 {
                self.frame.circle(
                    center,
                    [backing, 1.0 * scale.max(0.5)],
                    blur,
                    Paint {
                        fill: rgba(look.surface, 0.97 * opacity),
                        stroke: rgba(rim, opacity),
                        ..Default::default()
                    },
                );
            } else if !matches!(mask, Mask::Polygon { .. }) {
                self.frame.rounded_rect(
                    center,
                    half + mat,
                    [corner + mat, 1.0 * scale.max(0.5)],
                    blur,
                    Paint {
                        fill: rgba(look.surface, 0.97 * opacity),
                        stroke: rgba(rim, opacity),
                        ..Default::default()
                    },
                );
            }
        }
        let tone = look.tone(tint);
        let reach = half.length() + blur + 2.0;
        self.frame.prims.push(Prim {
            bbox: around(center, Vec2::splat(reach)),
            a: [13.0, center.x, center.y, corner],
            b: [half.x, half.y, blur, turn.to_angle()],
            fill: [
                opacity,
                scene.v(id, "saturation").max(0.0),
                scene.unit(id, "dim"),
                scene.unit(id, "tint"),
            ],
            stroke: rgba(rim, if framed { 0.9 } else { 0.0 }),
            glow: [
                tone.x,
                tone.y,
                tone.z,
                if framed { scale.max(0.5) } else { 0.0 },
            ],
            uv: [
                slot.origin[0] as f32 + wx,
                slot.origin[1] as f32 + wy,
                slot.origin[0] as f32 + wx + ww,
                slot.origin[1] as f32 + wy + wh,
            ],
            light: [
                slot.origin[0] as f32,
                slot.origin[1] as f32,
                (slot.origin[0] + slot.size[0]) as f32,
                (slot.origin[1] + slot.size[1]) as f32,
            ],
            light_color: [mask_kind, first, count, 0.0],
            ..Default::default()
        });
        self.frame.close(place.depth, order);
    }
}

/// A figure's outline in world pixels about its center, turned `rotation`
/// radians: a closed figure from twelve o'clock (a rectangle from the middle
/// of its top) clockwise on screen, without repeating its first point; an arc
/// from its start to its end. Corners round at `corner`.
fn figure_outline(figure: &Figure, corner: f32, rotation: f32) -> Vec<Vec2> {
    let turn = Vec2::from_angle(rotation);
    let circle = |radius: f32, start: f32, sweep: f32, closed: bool| {
        let segments = ((96.0 * sweep).ceil() as usize).max(8);
        let count = if closed { segments } else { segments + 1 };
        (0..count)
            .map(|k| {
                let angle = -FRAC_PI_2 + TAU * (start + sweep * k as f32 / segments as f32);
                Vec2::from_angle(angle) * radius
            })
            .collect::<Vec<_>>()
    };
    let points = match figure {
        Figure::Rect(size) => {
            let half = Vec2::from(*size) * 0.5;
            let corners = [
                vec2(half.x, -half.y),
                vec2(half.x, half.y),
                vec2(-half.x, half.y),
                vec2(-half.x, -half.y),
            ];
            let mut points = vec![vec2(0.0, -half.y)];
            points.extend(round_corners(&corners, corner.min(half.min_element())));
            points
        }
        Figure::Circle(radius) => circle(*radius, 0.0, 1.0, true),
        Figure::Arc {
            radius,
            start,
            sweep,
        } => circle(*radius, *start, *sweep, false),
        Figure::Polygon(points) => round_corners(
            &points.iter().map(|p| Vec2::from(*p)).collect::<Vec<_>>(),
            corner,
        ),
    };
    points.into_iter().map(|point| turn.rotate(point)).collect()
}

/// A closed polygon's corners, each rounded by a curve cut back `radius`
/// along both of its sides (at most half of each side).
fn round_corners(corners: &[Vec2], radius: f32) -> Vec<Vec2> {
    if radius <= 0.0 {
        return corners.to_vec();
    }
    let n = corners.len();
    let mut points = Vec::with_capacity(n * 9);
    for (index, corner) in corners.iter().enumerate() {
        let (previous, next) = (corners[(index + n - 1) % n], corners[(index + 1) % n]);
        let r = radius
            .min(corner.distance(previous) * 0.5)
            .min(corner.distance(next) * 0.5);
        let a = *corner + (previous - *corner).normalize_or_zero() * r;
        let b = *corner + (next - *corner).normalize_or_zero() * r;
        // A circular corner is a quarter turn; this cubic matches it to 0.03%
        // for right angles and stays tangent to both sides for any angle.
        let k = 0.552_284_8;
        points.extend(
            CubicBezier {
                start: a,
                control_a: a + (*corner - a) * k,
                control_b: b + (*corner - b) * k,
                end: b,
            }
            .flatten(8)
            .points(),
        );
    }
    points
}

/// Colors of one primitive: straight linear RGBA fill and stroke, and glow as
/// linear RGB intensity with its radius in pixels.
#[derive(Clone, Copy, Default)]
struct Paint {
    fill: [f32; 4],
    stroke: [f32; 4],
    glow: [f32; 4],
    /// Rounded rects only: see `Prim::light` and `Prim::pool`.
    light: [f32; 4],
    light_color: [f32; 4],
    pool: [f32; 4],
    pool_color: [f32; 4],
}

/// How a lightning channel shines: core half width and its taper toward the
/// end, defocus, white-hot core and corona light, and the corona's radius.
#[derive(Clone, Copy)]
struct Plasma {
    half: f32,
    taper: f32,
    blur: f32,
    core: Vec3,
    corona: Vec3,
    radius: f32,
}

/// The primitives of one sample, grouped into depth-sorted layers.
struct StageFrame<'a> {
    texts: &'a HashMap<String, AtlasText>,
    prims: Vec<Prim>,
    points: Vec<[f32; 4]>,
    /// Depth, declaration order, and primitive range of each layer.
    layers: Vec<(f32, usize, usize, usize)>,
    /// First primitive of the layer being drawn.
    open: usize,
    /// Background color for the soft backing behind text.
    shade: Vec3,
}

impl<'a> StageFrame<'a> {
    fn new(texts: &'a HashMap<String, AtlasText>, shade: Vec3) -> Self {
        Self {
            texts,
            prims: Vec::new(),
            points: Vec::new(),
            layers: Vec::new(),
            open: 0,
            shade,
        }
    }

    /// End the layer drawn since the last close, at world depth `depth`.
    fn close(&mut self, depth: f32, order: usize) {
        self.layers
            .push((depth, order, self.open, self.prims.len()));
        self.open = self.prims.len();
    }

    /// Primitives far to near, keeping declaration order among equal depths.
    fn finish(mut self) -> Result<(Vec<Prim>, Vec<[f32; 4]>)> {
        self.layers
            .sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let prims = self
            .layers
            .iter()
            .flat_map(|&(_, _, first, last)| self.prims[first..last].iter().copied())
            .collect::<Vec<_>>();
        if prims.len() > MAX_PRIMS || self.points.len() > MAX_POINTS {
            bail!(
                "stage frame needs {} primitives and {} points; limits are {MAX_PRIMS} and {MAX_POINTS}",
                prims.len(),
                self.points.len()
            );
        }
        Ok((prims, self.points))
    }

    /// `shape` is the corner radius and border width.
    fn rounded_rect(&mut self, center: Vec2, half: Vec2, shape: [f32; 2], blur: f32, paint: Paint) {
        self.prims.push(Prim {
            bbox: around(center, half + Vec2::splat(paint.glow[3] * 4.0 + blur + 2.0)),
            a: [0.0, center.x, center.y, shape[0]],
            b: [half.x, half.y, shape[1], blur],
            fill: paint.fill,
            stroke: paint.stroke,
            glow: paint.glow,
            light: paint.light,
            light_color: paint.light_color,
            pool: paint.pool,
            pool_color: paint.pool_color,
            ..Default::default()
        });
    }

    /// `shape` is the radius and border width.
    fn circle(&mut self, center: Vec2, shape: [f32; 2], blur: f32, paint: Paint) {
        self.prims.push(Prim {
            bbox: around(
                center,
                Vec2::splat(shape[0] + paint.glow[3] * 4.0 + blur + 1.0),
            ),
            a: [1.0, center.x, center.y, shape[0]],
            b: [shape[1], 0.0, 0.0, blur],
            fill: paint.fill,
            stroke: paint.stroke,
            glow: paint.glow,
            ..Default::default()
        });
    }

    /// Any simple polygon through `points` (in order, not repeating the
    /// first), filled, with an optional `border` of that many pixels.
    fn polygon(&mut self, points: &[Vec2], border: f32, blur: f32, paint: Paint) {
        let visible = paint.fill[3] > 0.001
            || (border > 0.0 && paint.stroke[3] > 0.001)
            || paint.glow[..3].iter().any(|v| *v > 0.0);
        if points.len() < 3 || !visible {
            return;
        }
        let first = self.points.len();
        self.points
            .extend(points.iter().map(|point| [point.x, point.y, 0.0, 0.0]));
        let (low, high) = points
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(low, high), point| {
                (low.min(*point), high.max(*point))
            });
        let pad = Vec2::splat(paint.glow[3] * 4.0 + blur + 2.0);
        let (low, high) = (low - pad, high + pad);
        self.prims.push(Prim {
            bbox: [low.x, low.y, high.x, high.y],
            a: [7.0, border, 0.0, blur],
            fill: paint.fill,
            stroke: paint.stroke,
            glow: paint.glow,
            uv: [first as f32, points.len() as f32, 0.0, 0.0],
            pool: paint.pool,
            pool_color: paint.pool_color,
            ..Default::default()
        });
    }

    /// A ring, or an arc clockwise from twelve o'clock over `sweep` of a turn.
    /// `shape` is the radius and thickness.
    fn arc(&mut self, center: Vec2, shape: [f32; 2], sweep: f32, blur: f32, paint: Paint) {
        self.prims.push(Prim {
            bbox: around(
                center,
                Vec2::splat(shape[0] + shape[1] + paint.glow[3] * 4.0 + blur + 2.0),
            ),
            a: [2.0, center.x, center.y, shape[0]],
            b: [shape[1], -FRAC_PI_2, TAU * sweep, blur],
            stroke: paint.stroke,
            glow: paint.glow,
            ..Default::default()
        });
    }

    /// `path` drawn to `fraction` of its length. `stroke` is the width and blur;
    /// `style` is dash, gap, phase, and fade toward the start.
    fn polyline(
        &mut self,
        path: &Polyline,
        fraction: f32,
        stroke: [f32; 2],
        paint: Paint,
        style: [f32; 4],
    ) {
        self.path(
            path,
            &vec![1.0; path.points().len()],
            fraction,
            stroke,
            paint,
            style,
        );
    }

    /// A path whose points each carry a heat that scales its light, such as a
    /// trail cooling behind a packet.
    fn trail(&mut self, points: &[(Vec2, f32)], stroke: [f32; 2], paint: Paint) {
        let path = Polyline::new(points.iter().map(|(point, _)| *point).collect());
        let heat = points.iter().map(|(_, heat)| *heat).collect::<Vec<_>>();
        self.path(&path, &heat, 1.0, stroke, paint, SOLID);
    }

    fn path(
        &mut self,
        path: &Polyline,
        heat: &[f32],
        fraction: f32,
        stroke: [f32; 2],
        paint: Paint,
        style: [f32; 4],
    ) {
        let points = path.points();
        if points.len() < 2 || fraction <= 0.0 || paint.stroke[3] <= 0.001 {
            return;
        }
        let first = self.points.len();
        self.points.extend(
            points
                .iter()
                .zip(path.lengths())
                .zip(heat)
                .map(|((point, along), heat)| [point.x, point.y, *along, *heat]),
        );
        let (low, high) = points
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(low, high), point| {
                (low.min(*point), high.max(*point))
            });
        let pad = Vec2::splat(stroke[0] + paint.glow[3] * 4.0 + stroke[1] + 2.0);
        let (low, high) = (low - pad, high + pad);
        self.prims.push(Prim {
            bbox: [low.x, low.y, high.x, high.y],
            a: [
                3.0,
                stroke[0],
                path.length() * fraction.clamp(0.0, 1.0),
                stroke[1],
            ],
            b: style,
            stroke: paint.stroke,
            glow: paint.glow,
            uv: [first as f32, points.len() as f32, 0.0, 0.0],
            light: paint.light,
            light_color: paint.light_color,
            ..Default::default()
        });
    }

    /// A lightning channel through `points`, drawn to `drawn` of its length,
    /// whose points carry `energy` (of their fraction along it).
    fn plasma(&mut self, points: &[Vec2], drawn: f32, energy: impl Fn(f32) -> f32, style: Plasma) {
        if points.len() < 2
            || drawn <= 0.0
            || style.core.max_element() + style.corona.max_element() <= 1e-4
        {
            return;
        }
        let path = Polyline::new(points.to_vec());
        let total = path.length().max(1e-3);
        let first = self.points.len();
        self.points.extend(
            points
                .iter()
                .zip(path.lengths())
                .map(|(point, along)| [point.x, point.y, *along, energy(*along / total)]),
        );
        let reach = style.radius * 5.0 + style.half + style.blur + 2.0;
        let (low, high) = points
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(low, high), point| {
                (low.min(*point), high.max(*point))
            });
        let p0 = points[0];
        let pn = points[points.len() - 1];
        let perp = (pn - p0).normalize_or(Vec2::X).perp();
        let (min_perp, max_perp) =
            points
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &p| {
                    let d = (p - p0).dot(perp);
                    (lo.min(d), hi.max(d))
                });
        let strip_pad = reach + 2.0;
        self.prims.push(Prim {
            bbox: [low.x - reach, low.y - reach, high.x + reach, high.y + reach],
            a: [10.0, style.half, total * drawn.min(1.0), style.taper],
            b: [style.blur, style.radius, reach, 0.0],
            fill: style.core.extend(0.0).to_array(),
            stroke: style.corona.extend(0.0).to_array(),
            uv: [first as f32, points.len() as f32, 0.0, 0.0],
            light: [p0.x, p0.y, perp.x, perp.y],
            light_color: [min_perp - strip_pad, max_perp + strip_pad, 0.0, 0.0],
            ..Default::default()
        });
    }

    /// Push copies of `prims` clipped to `clip`, then moved by `offset` with
    /// their opacity scaled by `alpha`. Clipping is the quad itself: every
    /// primitive rasterizes only inside its bounding box.
    fn echo(&mut self, prims: &[Prim], clip: [f32; 4], offset: Vec2, alpha: f32) {
        for prim in prims {
            let mut copy = *prim;
            copy.bbox = [
                prim.bbox[0].max(clip[0]) + offset.x,
                prim.bbox[1].max(clip[1]) + offset.y,
                prim.bbox[2].min(clip[2]) + offset.x,
                prim.bbox[3].min(clip[3]) + offset.y,
            ];
            if copy.bbox[0] >= copy.bbox[2] || copy.bbox[1] >= copy.bbox[3] {
                continue;
            }
            if matches!(prim.a[0] as u32, 3 | 7) {
                let first = prim.uv[0] as usize;
                let count = prim.uv[1] as usize;
                copy.uv[0] = self.points.len() as f32;
                for index in first..first + count {
                    let [x, y, along, heat] = self.points[index];
                    self.points.push([x + offset.x, y + offset.y, along, heat]);
                }
            } else {
                copy.a[1] += offset.x;
                copy.a[2] += offset.y;
            }
            for light in [&mut copy.light, &mut copy.pool] {
                light[0] += offset.x;
                light[1] += offset.y;
                light[3] *= alpha;
            }
            copy.fill[3] *= alpha;
            copy.stroke[3] *= alpha;
            for channel in &mut copy.glow[..3] {
                *channel *= alpha;
            }
            self.prims.push(copy);
        }
    }

    /// On-screen width of an atlas string's ink at `scale`.
    fn width(&self, key: &str, scale: f32) -> f32 {
        self.texts.get(key).map_or(0.0, |text| {
            (text.rect[2] - 4.0).max(0.0) * scale / TEXT_RASTER
        })
    }

    /// Let the packet finish travelling, but hold its complete measured label
    /// in the open corridor. Depth sorting still keeps the packet behind bodies.
    fn packet_label_at(&self, key: &str, at: Vec2, scale: f32, ports: [Port; 2]) -> Option<Vec2> {
        let text = self.texts.get(key)?;
        let size = vec2(text.rect[2], text.rect[3]) * (scale / TEXT_RASTER);
        // Center alignment uses ink width; the atlas also contains transparent
        // raster padding. Include it, plus clearance for the soft backing.
        let offset = vec2((size.x - self.width(key, scale)) * 0.5, 0.0);
        let bounds = Box2::from_center_size(at + offset, size);
        let fitted = fit_between_ports(bounds, ports, 8.0 * scale)?;
        Some(at + fitted.center() - bounds.center())
    }

    /// One atlas string at `scale`, anchored on its vertical center. `reveal`
    /// clips it to a width, for typing.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        key: &str,
        at: Vec2,
        scale: f32,
        align: CaptionAlign,
        fill: [f32; 4],
        reveal: f32,
        blur: f32,
    ) -> Option<&mut Prim> {
        let text = self.texts.get(key)?;
        if fill[3] <= 0.001 {
            return None;
        }
        let size = vec2(text.rect[2], text.rect[3]) * (scale / TEXT_RASTER);
        let ink = self.width(key, scale);
        let left = at.x
            - match align {
                CaptionAlign::Left => 0.0,
                CaptionAlign::Center => ink * 0.5,
                CaptionAlign::Right => ink,
            };
        let top = at.y - size.y * 0.5;
        let center = vec2(left, top) + size * 0.5;
        let uv = [
            text.rect[0],
            text.rect[1],
            text.rect[0] + text.rect[2],
            text.rect[1] + text.rect[3],
        ];
        let reveal = reveal.min(size.x + 4.0);
        // A soft dark backing keeps text legible where it crosses beams and glow.
        let backing = Prim {
            bbox: around(center, size * 0.5 + 8.0),
            a: [4.0, left, top, 5.0 + blur * 0.5],
            b: [size.x, size.y, reveal, 0.0],
            fill: rgba(self.shade, fill[3] * 0.85),
            uv,
            ..Default::default()
        };
        self.prims.push(backing);
        self.prims.push(Prim {
            bbox: around(center, size * 0.5 + 2.0),
            a: [4.0, left, top, blur * 0.5],
            fill,
            ..backing
        });
        self.prims.last_mut()
    }
}

/// The screen rectangle `pad` around `center`.
fn around(center: Vec2, pad: Vec2) -> [f32; 4] {
    [
        center.x - pad.x,
        center.y - pad.y,
        center.x + pad.x,
        center.y + pad.y,
    ]
}

fn rgba(color: Vec3, alpha: f32) -> [f32; 4] {
    color.extend(alpha.clamp(0.0, 1.0)).to_array()
}

fn glow4(color: Vec3, radius: f32) -> [f32; 4] {
    color.extend(radius).to_array()
}

#[cfg(test)]
mod tests {
    use super::Scene;
    use psychopomp::math::vec2;
    use psychopomp::stage::StagePlan;

    use crate::render::HeadlessRenderer;

    #[test]
    fn packet_labels_hold_their_full_atlas_bounds_outside_both_cards() {
        let texts = std::collections::HashMap::from([(
            "request#label".into(),
            super::AtlasText {
                rect: [0.0, 0.0, 220.0, 48.0],
            },
        )]);
        let mut frame = super::StageFrame::new(&texts, super::Vec3::ZERO);
        let ports = [
            super::Port {
                point: vec2(720.0, 560.0),
                normal: super::Vec2::X,
            },
            super::Port {
                point: vec2(1200.0, 560.0),
                normal: -super::Vec2::X,
            },
        ];
        for scale in [0.5, 1.0, 1.5] {
            for x in [720.0, 850.0, 960.0, 1180.0, 1200.0] {
                let at = frame
                    .packet_label_at("request#label", vec2(x, 534.0), scale, ports)
                    .unwrap();
                let text = frame
                    .text(
                        "request#label",
                        at,
                        scale,
                        super::CaptionAlign::Center,
                        [1.0; 4],
                        f32::MAX,
                        0.0,
                    )
                    .unwrap();
                assert!(text.bbox[0] > 720.0 && text.bbox[2] < 1200.0);
            }
        }
        let short = [
            ports[0],
            super::Port {
                point: vec2(780.0, 560.0),
                ..ports[1]
            },
        ];
        assert!(
            frame
                .packet_label_at("request#label", vec2(760.0, 534.0), 1.0, short)
                .is_none()
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; a completed connection cannot wake or pulse afterward"]
    fn a_completed_connection_holds_identical_pixels_after_contact() {
        use psychopomp::{
            author::PlanBuilder,
            plan::{ScalarPlan, compile_channels},
            stage::StageActor,
            timeline::PropertyId,
        };
        let recipe: StagePlan = serde_json::from_value(serde_json::json!({
            "post": { "bloom": 0, "grain": 0, "vignette": 0, "backdrop": 0 },
            "elements": [
                { "kind": "card", "id": "client", "at": [560, 560, 0], "size": [320, 120], "title": "client" },
                { "kind": "card", "id": "api", "at": [1360, 560, 0], "size": [320, 120], "title": "api" },
                { "kind": "beam", "id": "link", "from": "client", "to": "api" }
            ]
        })).unwrap();
        let mut builder = PlanBuilder::new("quiet-connection", 4_000_000_000);
        let mut actor = StageActor::declare(&mut builder, "stage", &recipe).unwrap();
        let contact = actor.connect(&mut builder, "link", 0, 0.5) as f64 / 1e9;
        let plan = builder.finish().unwrap();
        let timeline = compile_channels(
            plan.continuous_channels
                .iter()
                .map(|c| (c, PropertyId::new(&c.property))),
            plan.duration_nanos,
            |scalar| match scalar {
                ScalarPlan::Literal(value) => Ok(*value),
                _ => unreachable!(),
            },
        )
        .unwrap();
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "quiet-connection".into(),
        }))
        .unwrap();
        let gpu = renderer.prepare_stage(&recipe).unwrap();
        let render = |renderer: &mut HeadlessRenderer, time| {
            renderer
                .render_stage(&recipe, &gpu, time, |property, default| {
                    timeline
                        .sample_at(&PropertyId::new(property), time)
                        .map_or(default, |state| state.position)
                })
                .unwrap()
        };
        let settled = render(&mut renderer, contact);
        for time in [
            contact + 0.02,
            contact + 0.2,
            contact + 0.5,
            contact + 1.0,
            contact,
        ] {
            assert!(
                render(&mut renderer, time) == settled,
                "connection changed at {time}"
            );
        }
        assert!(
            render(&mut renderer, 0.5) != settled,
            "the draw still animates"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; measured packet labels clear moving bodies in either direction"]
    fn measured_packet_labels_clear_endpoints_through_camera_motion_and_reversal() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "packet-label-proof".into(),
        }))
        .unwrap();
        for destination in [
            serde_json::json!({
                "kind": "card", "id": "api", "at": [1360, 560, 0], "size": [320, 120], "title": "api"
            }),
            serde_json::json!({
                "kind": "orb", "id": "api", "at": [1360, 560, 0], "radius": 100
            }),
        ] {
            let plan: StagePlan = serde_json::from_value(serde_json::json!({
                "elements": [
                    { "kind": "card", "id": "client", "at": [560, 560, 0], "size": [320, 120], "title": "client" },
                    destination,
                    { "kind": "beam", "id": "link", "from": "client", "to": "api", "bend": 40 },
                    { "kind": "packet", "id": "request", "beam": "link", "label": "GET /user" }
                ]
            })).unwrap();
            let gpu = renderer.prepare_stage(&plan).unwrap();
            let frame = super::StageFrame::new(&gpu.texts, super::Vec3::ZERO);
            let key = "request#label";
            for camera in [0.0, 340.0, 100.0, 0.0] {
                let value = |property: &str, default| {
                    if property == "camera.z" {
                        camera
                    } else {
                        default
                    }
                };
                let scene = Scene::sample(&plan, &value, 1.0, vec2(1920.0, 1080.0));
                for reverse in [false, true] {
                    let link = scene.links["link"].toward(reverse);
                    for travel in [0.0, 0.1, 0.5, 0.9, 1.0, 0.5] {
                        let scale = link.scale_at(travel);
                        let desired = link.path.at(travel) - vec2(0.0, 26.0 * scale);
                        let at = frame
                            .packet_label_at(key, desired, scale, link.label_ports)
                            .unwrap();
                        let text = &gpu.texts[key];
                        let size = vec2(text.rect[2], text.rect[3]) * (scale / super::TEXT_RASTER);
                        let bounds = super::Box2::from_center_size(
                            at + vec2((size.x - frame.width(key, scale)) * 0.5, 0.0),
                            size,
                        );
                        for port in link.label_ports {
                            let clearance = (bounds.center() - port.point).dot(port.normal)
                                - bounds.extents().dot(port.normal.abs());
                            assert!(clearance >= 8.0 * scale - 0.001);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_bundled_icon_rasterizes_to_visible_coverage() {
        for name in psychopomp::stage::ICONS {
            let svg = super::icon_svg(name, "", 256.0).unwrap_or_else(|| panic!("{name}"));
            let sprite = super::icon_sprite(&svg, 48).unwrap();
            let covered = sprite.pixels.chunks(4).filter(|p| p[3] > 128).count();
            assert!(covered > 60, "{name} covers {covered} pixels");
        }
        assert!(super::icon_svg("nope", "", 256.0).is_none());
        let square = super::icon_svg("", "M0 0 H24 V24 H0 Z", 24.0).unwrap();
        let sprite = super::icon_sprite(&square, 16).unwrap();
        assert!(
            sprite.pixels.chunks(4).all(|p| p[3] == 255),
            "path data fills its view"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; explicit art pigment survives neutral themes and holes stay transparent"]
    fn flat_art_keeps_pigment_holes_and_identical_rest_frames() {
        let recipe: StagePlan = serde_json::from_value(serde_json::json!({
            "post": psychopomp::stage::StagePost::FLAT,
            "elements": [{ "kind": "icon", "id": "art", "at": [160, 90, 0],
                "size": 100, "view": 300, "ink": [0, 75, 147],
                "path": "M210 60H90V240H210V60ZM270 300H30V0H270V300Z" }]
        }))
        .unwrap();
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 320,
            height: 180,
            file_name: "flat-art-proof".into(),
        }))
        .unwrap();
        renderer.set_theme(super::Theme::Neutral);
        let gpu = renderer.prepare_stage(&recipe).unwrap();
        let frame = renderer
            .render_stage(&recipe, &gpu, 0.0, |_, rest| rest)
            .unwrap();
        let pixel = |x: usize, y: usize| &frame[(y * 320 + x) * 4..(y * 320 + x) * 4 + 3];
        assert_eq!(
            pixel(160, 90),
            pixel(10, 10),
            "the counter is the actual background"
        );
        let ink = pixel(130, 90);
        assert!(
            ink[2] > ink[1] && ink[1] > ink[0],
            "blue artwork is not neutralized: {ink:?}"
        );
        for time in [2.0, 0.5, 0.0] {
            assert_eq!(
                frame,
                renderer
                    .render_stage(&recipe, &gpu, time, |_, rest| rest)
                    .unwrap()
            );
        }
    }

    #[test]
    fn wires_attach_to_a_turning_form_silhouette_continuously() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "form", "id": "store", "at": [1500, 470, 0], "points": 720,
                  "shapes": [{ "shape": "plane", "size": [340, 184] }] },
                { "kind": "card", "id": "gateway", "at": [900, 500, 0], "size": [232, 132], "title": "g" },
                { "kind": "path", "id": "write", "through": ["gateway", "store"], "arrow": "end" },
                { "kind": "beam", "id": "link", "from": "gateway", "to": "store" }
            ]
        }))
        .unwrap();
        let geometry = std::collections::HashMap::from([(
            "store".to_string(),
            super::FormGeometry::new(
                &[psychopomp::stage::FormShape::Plane {
                    size: [340.0, 184.0],
                }],
                720,
                0.42,
            ),
        )]);
        let mut last: Option<(super::Vec2, super::Vec2)> = None;
        for step in 0..200 {
            let time = 4.0 + step as f32 * 0.004;
            let value = |property: &str, default| match property {
                "store.pitch" => time * 0.8,
                _ => default,
            };
            let scene =
                Scene::sample_with(&plan, &value, time, vec2(1920.0, 1080.0), Some(&geometry));
            let super::Shape::Polygon(hull) = scene.placements["store"].outline else {
                panic!("a form attaches to its sampled silhouette");
            };
            assert_eq!(
                hull.vertices().len(),
                4,
                "a plane's silhouette is its corners"
            );
            let leg = &scene.routes["write"][0];
            let ends = (leg.path.at(1.0), scene.links["link"].path.at(1.0));
            assert!(
                hull.distance(leg.path.at(leg.tips[1])).abs() < 1.0,
                "the arrow tip sits on the silhouette"
            );
            if let Some((path, beam)) = last {
                assert!(
                    path.distance(ends.0) < 2.0 && beam.distance(ends.1) < 2.0,
                    "{time}"
                );
            }
            last = Some(ends);
        }
    }

    #[test]
    fn stage_anchors_follow_the_camera_and_the_developed_punch() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "api", "at": [1160, 540, 0], "size": [300, 100], "title": "api" }
            ]
        }))
        .unwrap();
        let size = vec2(1920.0, 1080.0);
        let anchor = |value: &dyn Fn(&str, f32) -> f32| {
            super::stage_anchor(&plan, value, 1.0, size, "api", super::CalloutSide::Top).unwrap()
        };
        assert_eq!(anchor(&|_, default| default), vec2(1160.0, 490.0));
        // Dollying in 700 px doubles the z = 0 plane about the frame center.
        let dolly = anchor(&|property, default| match property {
            "camera.z" => 700.0,
            _ => default,
        });
        assert!(dolly.abs_diff_eq(vec2(1360.0, 440.0), 1e-3), "{dolly}");
        // The develop pass's punch-in scales the frame about its center too.
        let punch = anchor(&|property, default| match property {
            "camera.punch" => 0.1,
            _ => default,
        });
        assert!(punch.abs_diff_eq(vec2(1180.0, 485.0), 1e-3), "{punch}");
        let missing = super::stage_anchor(
            &plan,
            &|_, default| default,
            1.0,
            size,
            "ghost",
            super::CalloutSide::Top,
        );
        assert!(missing.is_none());
    }

    #[test]
    fn stage_anchors_turn_with_the_authored_roll() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "api", "at": [1160, 540, 0], "size": [300, 100], "title": "api" }
            ]
        }))
        .unwrap();
        let size = vec2(1920.0, 1080.0);
        let rolled = super::stage_anchor(
            &plan,
            &|property, default| {
                if property == "camera.roll" {
                    0.1
                } else {
                    default
                }
            },
            1.0,
            size,
            "api",
            super::CalloutSide::Center,
        )
        .unwrap();
        let camera = psychopomp::stage::Camera {
            roll: 0.1,
            ..psychopomp::stage::Camera::new(size)
        };
        assert_eq!(rolled, camera.rolled(vec2(1160.0, 540.0)));
        assert!(rolled.y > 560.0, "a clockwise roll lowers the right side");
    }

    /// A card-to-card wire with a packet in flight; ends at different depths
    /// so following has parallax to resolve.
    fn followed_plan() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "client", "at": [420, 560, 0], "size": [300, 110], "title": "client" },
                { "kind": "card", "id": "api", "at": [1700, 420, 260], "size": [300, 110], "title": "api" },
                { "kind": "beam", "id": "link", "from": "client", "to": "api", "bend": 40 },
                { "kind": "packet", "id": "request", "beam": "link" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_followed_packet_stays_centered_where_it_is_drawn() {
        let plan = followed_plan();
        let size = vec2(1920.0, 1080.0);
        for (yaw, age) in [(0.0, 0.2), (0.0, 0.7), (0.0, 1.1), (0.3, 0.9), (0.0, 3.0)] {
            let value = |property: &str, default| match property {
                "camera.track.request" => 1.0,
                "camera.yaw" => yaw,
                "camera.z" => 120.0,
                "request.age" => age,
                "request.flight" => 1.2,
                _ => default,
            };
            let scene = Scene::sample(&plan, &value, 1.0, size);
            let link = &scene.links["link"];
            let head = link.path.at(psychopomp::stage::packet::travel(age, 1.2));
            assert!(
                head.distance(size * 0.5) < 0.5,
                "yaw {yaw}, age {age}: the packet is drawn at {head}"
            );
        }
    }

    #[test]
    fn catching_a_followed_packet_is_continuous() {
        let plan = followed_plan();
        let size = vec2(1920.0, 1080.0);
        // The weight eases in while the packet flies; sample at 240 Hz.
        let pan = |time: f32| {
            let value = |property: &str, default| match property {
                "camera.track.request" => psychopomp::math::smoothstep(time / 0.6),
                "camera.x" => -200.0,
                "request.age" => time,
                "request.flight" => 1.2,
                _ => default,
            };
            Scene::camera(&plan, &value, time, size)
                .camera
                .position
                .truncate()
        };
        assert_eq!(pan(0.0), vec2(-200.0, 0.0), "no weight, no follow");
        let mut previous = pan(0.0);
        for step in 1..480 {
            let next = pan(step as f32 / 240.0);
            assert!(
                next.distance(previous) < 12.0,
                "the camera jumped {} px at step {step}",
                next.distance(previous)
            );
            previous = next;
        }
    }

    #[test]
    fn a_turned_camera_sorts_and_focuses_by_view_depth() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "left", "at": [560, 540, 400], "size": [200, 100], "title": "left" },
                { "kind": "card", "id": "right", "at": [1360, 540, 0], "size": [200, 100], "title": "right" }
            ]
        }))
        .unwrap();
        let size = vec2(1920.0, 1080.0);
        let depth = |yaw: f32| {
            let value = |property: &str, default| {
                if property == "camera.yaw" {
                    yaw
                } else {
                    default
                }
            };
            let scene = Scene::sample(&plan, &value, 1.0, size);
            (
                scene.placements["left"].depth,
                scene.placements["right"].depth,
            )
        };
        assert_eq!(depth(0.0), (400.0, 0.0), "unturned depth is world z");
        let (left, right) = depth(-1.2);
        assert!(
            left < right,
            "swung far left, the deep left card is nearer: {left} {right}"
        );
    }

    #[test]
    fn an_orb_pulse_does_not_displace_attached_ports() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "orb", "id": "service", "at": [960, 480, 0], "radius": 150 },
                { "kind": "card", "id": "client", "at": [420, 300, -40], "size": [300, 110], "title": "client" },
                { "kind": "beam", "id": "link", "from": "client", "to": "service" }
            ]
        }))
        .unwrap();
        let quiet = |_: &str, default| default;
        let pulse = |property: &str, default| {
            if property == "service.pulse" {
                1.0
            } else {
                default
            }
        };
        let before = Scene::sample(&plan, &quiet, 1.0, vec2(1920.0, 1080.0));
        let impact = Scene::sample(&plan, &pulse, 1.0, vec2(1920.0, 1080.0));
        assert_eq!(
            before.placements["service"].scale,
            impact.placements["service"].scale
        );
        assert_eq!(
            before.links["link"].path.points(),
            impact.links["link"].path.points()
        );
        let link = &before.links["link"];
        let orb = before.placements["service"];
        assert!(
            link.path.at(1.0).distance(orb.center) < 150.0 * orb.scale * 0.8,
            "the cap is submerged beneath the orb's occluding body"
        );
    }

    #[test]
    fn combustion_light_is_local_and_cools_with_the_burst_clock() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [{ "kind": "orb", "id": "service", "at": [960, 480, 0], "radius": 150 }]
        }))
        .unwrap();
        let mut strengths = Vec::new();
        for age in [-1.0, 0.0, 0.3, 1.0, 3.0] {
            let values = |property: &str, default| {
                if property == "service.burst" {
                    age
                } else {
                    default
                }
            };
            let scene = Scene::sample(&plan, &values, 2.0, vec2(1920.0, 1080.0));
            strengths.push(scene.lights.iter().map(|light| light.strength).sum::<f32>());
            for light in &scene.lights {
                assert!(light.falloff(400.0) > 0.0);
                assert_eq!(light.falloff(900.0), 0.0, "far rims remain dark");
                assert!(
                    !light.pool,
                    "combustion lights rims without washing the card"
                );
            }
        }
        assert_eq!([strengths[0], strengths[1], strengths[4]], [0.0; 3]);
        assert!(strengths[2] > strengths[3] && strengths[3] > 0.0);
    }

    fn effects_plan() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "build", "at": [420, 540, 0], "size": [300, 110], "title": "build" },
                { "kind": "orb", "id": "api", "at": [1400, 540, 0], "radius": 110 },
                { "kind": "shield", "id": "guard", "around": "api", "radius": 190 },
                { "kind": "card", "id": "client", "at": [1400, 940, 0], "size": [240, 90], "title": "client" },
                { "kind": "beam", "id": "link", "from": "client", "to": "api" },
                { "kind": "packet", "id": "req", "beam": "link" },
                { "kind": "bolt", "id": "zap", "from": "build", "to": "guard" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn bolt_light_is_local_strobes_with_strokes_and_reaches_rest() {
        use psychopomp::effects::lightning::{Discharge, LEADER};
        let plan = effects_plan();
        let size = vec2(1920.0, 1080.0);
        let at = |age: f32| {
            move |property: &str, default: f32| match property {
                "zap.age" => age,
                _ => default,
            }
        };
        let strength = |age: f32| {
            let value = at(age);
            let scene = Scene::sample(&plan, &value, 2.0, size);
            scene
                .lights
                .iter()
                .map(|light| light.strength)
                .fold(0.0, f32::max)
        };
        let discharge = Discharge::new(3, super::salt("zap"));
        assert_eq!(strength(-1.0), 0.0, "quiet before the zap");
        assert!(
            strength(LEADER) > strength(LEADER - 0.02),
            "contact outshines the leader"
        );
        assert!(
            strength(LEADER) > strength(LEADER + 0.04),
            "a stroke flashes, then dims"
        );
        let restrike = discharge.strike_time(1);
        assert!(
            strength(restrike) > strength(restrike - 0.005),
            "a re-strike flashes again"
        );
        let last = discharge.strike_time(discharge.strikes - 1);
        let ripple = last + psychopomp::effects::shield::RIPPLE;
        assert!(
            strength(discharge.lifetime()) > 0.0,
            "the struck shield still flares"
        );
        assert_eq!(
            strength(ripple),
            0.0,
            "dark once the channel and ripple cool"
        );
        let value = at(LEADER);
        let scene = Scene::sample(&plan, &value, 2.0, size);
        for light in &scene.lights {
            assert!(light.falloff(400.0) < 1e-3, "bolt light stays local");
        }
        // The struck shield ripples from its contact; the packet's crossing too.
        let guard = scene.placements["guard"];
        assert_eq!(scene.shield_contacts("guard", guard).len(), 1);
        let crossing = |property: &str, default| match property {
            "req.age" => 1.1,
            "req.flight" => 0.8,
            _ => default,
        };
        let packet = Scene::sample(&plan, &crossing, 2.0, size);
        let contacts = packet.shield_contacts("guard", packet.placements["guard"]);
        assert_eq!(contacts.len(), 1);
        assert!(contacts[0].strength < 1.0, "packets ripple softly");
        assert!(
            (contacts[0].point.distance(guard.center) - 190.0).abs() < 1.0,
            "on the bubble"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; effect frames are pure functions of their clocks"]
    fn effect_frames_are_deterministic_quiet_at_rest_and_order_free() {
        let plan = effects_plan();
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "effects-proof".into(),
        }))
        .unwrap();
        let gpu = renderer.prepare_stage(&plan).unwrap();
        let quiet = |property: &str, default| match property {
            "guard.up" => 0.0,
            _ => default,
        };
        let without: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "build", "at": [420, 540, 0], "size": [300, 110], "title": "build" },
                { "kind": "orb", "id": "api", "at": [1400, 540, 0], "radius": 110 },
                { "kind": "card", "id": "client", "at": [1400, 940, 0], "size": [240, 90], "title": "client" },
                { "kind": "beam", "id": "link", "from": "client", "to": "api" },
                { "kind": "packet", "id": "req", "beam": "link" }
            ]
        }))
        .unwrap();
        let bare = renderer.prepare_stage(&without).unwrap();
        let rest = renderer.render_stage(&plan, &gpu, 2.0, quiet).unwrap();
        assert!(
            renderer.render_stage(&without, &bare, 2.0, quiet).unwrap() == rest,
            "an idle bolt and a lowered shield draw nothing"
        );
        let draw = |renderer: &mut HeadlessRenderer, values: &[(&str, f32)]| {
            renderer
                .render_stage(&plan, &gpu, 2.0, |property, default| {
                    values
                        .iter()
                        .find(|(name, _)| *name == property)
                        .map_or(quiet(property, default), |(_, value)| *value)
                })
                .unwrap()
        };
        assert!(
            draw(&mut renderer, &[("build.dissolve", 0.0)]) == rest,
            "dissolve starts intact"
        );
        assert!(draw(&mut renderer, &[("build.charge", 0.0)]) == rest);
        let strike = draw(&mut renderer, &[("zap.age", 0.09)]);
        let burning = draw(&mut renderer, &[("build.dissolve", 0.5)]);
        let charged = draw(&mut renderer, &[("build.charge", 1.0)]);
        let scanning = draw(&mut renderer, &[("build.scan", 0.4)]);
        for frame in [&strike, &burning, &charged, &scanning] {
            assert!(*frame != rest, "every effect reaches pixels");
        }
        // Any order, same pixels.
        assert!(draw(&mut renderer, &[("build.charge", 1.0)]) == charged);
        assert!(draw(&mut renderer, &[("zap.age", 0.09)]) == strike);
        assert!(draw(&mut renderer, &[("build.dissolve", 0.5)]) == burning);
        assert!(
            draw(&mut renderer, &[("zap.age", 3.0)]) == rest,
            "the strike leaves no trace"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; stage frames are pure functions of time and channels"]
    fn stage_frames_are_deterministic_and_respond_to_channels() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "stage-proof".into(),
        }))
        .unwrap();
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "orb", "id": "service", "at": [960, 480, 0], "radius": 150, "points": 400 },
                { "kind": "card", "id": "client", "at": [420, 300, -40], "size": [300, 110], "title": "client" },
                { "kind": "beam", "id": "link", "from": "client", "to": "service" },
                { "kind": "packet", "id": "probe", "beam": "link", "label": "GET" }
            ]
        }))
        .unwrap();
        let gpu = renderer.prepare_stage(&plan).unwrap();
        let draw = |renderer: &mut HeadlessRenderer, time: f64, shatter: f32, dolly: f32| {
            renderer
                .render_stage(&plan, &gpu, time, |property, default| match property {
                    "service.shatter" => shatter,
                    "camera.z" => dolly,
                    "probe.age" => 0.74,
                    "probe.flight" => 0.8,
                    _ => default,
                })
                .unwrap()
        };
        let first = draw(&mut renderer, 1.0, 0.0, 0.0);
        draw(&mut renderer, 3.0, 1.0, 200.0);
        assert!(
            draw(&mut renderer, 1.0, 0.0, 0.0) == first,
            "sampling order cannot change a frame"
        );
        assert!(
            draw(&mut renderer, 1.0, 0.6, 0.0) != first,
            "shattering reaches pixels"
        );
        assert!(
            draw(&mut renderer, 1.0, 0.0, 120.0) != first,
            "the camera dolly reaches pixels"
        );
        assert!(
            draw(&mut renderer, 1.2, 0.0, 0.0) != first,
            "the orb spins with time"
        );
        let burst = |renderer: &mut HeadlessRenderer, age: f32| {
            renderer
                .render_stage(&plan, &gpu, 2.0, |property, default| match property {
                    "service.burst" => age,
                    _ => default,
                })
                .unwrap()
        };
        let fire = burst(&mut renderer, 0.45);
        let intact = burst(&mut renderer, -1.0);
        let rewind = |renderer: &mut HeadlessRenderer, age: f32| {
            renderer
                .render_stage(&plan, &gpu, 2.0, |property, default| {
                    if property == "post.rewind" {
                        age
                    } else {
                        default
                    }
                })
                .unwrap()
        };
        let post = |renderer: &mut HeadlessRenderer, name: &str, amount: f32| {
            renderer
                .render_stage(&plan, &gpu, 2.0, |property, default| {
                    if property == name { amount } else { default }
                })
                .unwrap()
        };
        let mean =
            |frame: &[u8]| frame.iter().map(|&b| f64::from(b)).sum::<f64>() / frame.len() as f64;
        assert!(
            post(&mut renderer, "post.zoom", 0.0) == intact,
            "no streak is the identity"
        );
        let streak = post(&mut renderer, "post.zoom", 0.3);
        assert!(streak != intact, "the zoom streak reaches pixels");
        assert!(
            post(&mut renderer, "post.zoom", 0.3) == streak,
            "the streak is deterministic"
        );
        assert!(
            post(&mut renderer, "post.flash", 0.0) == intact,
            "no flash is the identity"
        );
        assert!(
            mean(&post(&mut renderer, "post.flash", 1.0)) > 200.0,
            "a full flash washes the frame toward white"
        );
        assert!(
            post(&mut renderer, "camera.quake", 0.0) == intact,
            "no quake is the identity"
        );
        assert!(
            post(&mut renderer, "camera.quake", 2.0) != intact,
            "a quake moves the camera"
        );
        for (channel, rest, moved) in [
            ("camera.yaw", 0.0, 0.3),
            ("camera.pitch", 0.0, -0.2),
            ("camera.roll", 0.0, 0.05),
            ("camera.zoom", 1.0, 1.3),
            ("camera.handheld", 0.0, 1.0),
        ] {
            assert!(
                post(&mut renderer, channel, rest) == intact,
                "{channel} at rest is the identity"
            );
            let turned = post(&mut renderer, channel, moved);
            assert!(turned != intact, "{channel} reaches pixels");
            assert!(
                post(&mut renderer, channel, moved) == turned,
                "{channel} is deterministic"
            );
        }
        assert!(
            rewind(&mut renderer, 0.0) == intact,
            "rewind starts without a cut"
        );
        let scanned = rewind(&mut renderer, 0.4);
        assert!(scanned != intact, "reverse scan reaches pixels");
        assert!(
            rewind(&mut renderer, 1.4) == intact,
            "rewind settles exactly"
        );
        assert!(
            rewind(&mut renderer, 0.4) == scanned,
            "rewind is independent of sampling order"
        );
        assert!(
            burst(&mut renderer, 0.0) == intact,
            "entering Burst preserves the intact pixels"
        );
        let early = burst(&mut renderer, 0.0001);
        let error = intact
            .iter()
            .zip(&early)
            .map(|(a, b)| a.abs_diff(*b) as f64)
            .sum::<f64>()
            / intact.len() as f64;
        assert!(
            error < 0.01,
            "compression begins continuously, mean byte error {error}"
        );
        let smoke = burst(&mut renderer, 2.8);
        assert!(fire != smoke, "combustion cools into smoke");
        assert!(
            burst(&mut renderer, 0.45) == fire,
            "reverse sampling reconstructs the fire"
        );
        assert!(
            burst(&mut renderer, -1.0) != smoke,
            "negative age restores the intact orb"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU; Stage footage draws its frame, cut and treated"]
    fn stage_footage_draws_its_frame_masked_defocused_and_treated() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(crate::render::RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "stage-footage-proof".into(),
        }))
        .unwrap();
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "post": { "bloom": 0.0, "grain": 0.0, "vignette": 0.0, "backdrop": 0.0 },
            "elements": [
                { "kind": "footage", "id": "tv", "at": [960, 540, 0], "size": [400, 400],
                  "clip": { "media": "clip" }, "mask": { "shape": "circle" } }
            ]
        }))
        .unwrap();
        let sizes = super::HashMap::from([("tv".to_owned(), [8, 8])]);
        let gpu = renderer.prepare_stage_with(&plan, &sizes).unwrap();
        // Left half red, right half blue.
        let pixels: crate::footage::Frame = (0..64)
            .flat_map(|i| {
                if i % 8 < 4 {
                    [230, 20, 20, 255]
                } else {
                    [20, 20, 230, 255]
                }
            })
            .collect::<Vec<u8>>()
            .into();
        let frame = |identity| {
            Ok(vec![super::StageFootageFrame {
                element: "tv".into(),
                identity,
                pixels: pixels.clone(),
                size: [8, 8],
            }])
        };
        let draw = |renderer: &mut HeadlessRenderer, overrides: &[(&str, f32)]| {
            renderer
                .render_stage_exposure(
                    &plan,
                    &gpu,
                    &[(1.0, 1.0)],
                    |_, property, default| {
                        overrides
                            .iter()
                            .find(|(name, _)| *name == property)
                            .map_or(default, |(_, value)| *value)
                    },
                    |_| frame(7),
                )
                .unwrap()
        };
        let pixel = |pixels: &[u8], x: usize, y: usize| {
            let i = (y * 1920 + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        let plain = draw(&mut renderer, &[]);
        let left = pixel(&plain, 860, 540);
        let right = pixel(&plain, 1060, 540);
        assert!(
            left[0] > 180 && left[2] < 80,
            "the left half is red: {left:?}"
        );
        assert!(
            right[2] > 180 && right[0] < 80,
            "the right half is blue: {right:?}"
        );
        let corner = pixel(&plain, 960 - 190, 540 - 190);
        assert!(
            corner.iter().all(|v| *v < 40),
            "the circle leaves the corner dark: {corner:?}"
        );
        let gray = draw(&mut renderer, &[("tv.saturation", 0.0)]);
        let [r, g, b] = pixel(&gray, 860, 540);
        assert!(
            r.abs_diff(g) <= 2 && g.abs_diff(b) <= 2,
            "desaturated: {r} {g} {b}"
        );
        let soft = draw(&mut renderer, &[("tv.blur", 40.0)]);
        let middle = pixel(&soft, 960, 540);
        assert!(
            middle[0] > 60 && middle[2] > 60,
            "defocus mixes the halves: {middle:?}"
        );
        let hidden = draw(&mut renderer, &[("tv.opacity", 0.0)]);
        assert!(pixel(&hidden, 860, 540).iter().all(|v| *v < 40));
        assert!(
            draw(&mut renderer, &[]) == plain,
            "the same frame draws the same pixels"
        );
    }
}
