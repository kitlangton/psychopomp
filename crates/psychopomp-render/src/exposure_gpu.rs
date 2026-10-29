//! Shared GPU linear-light shutter accumulator (`Rgba16Float`) for Rune surfaces
//! and composited GPU overlays. Each shutter sample renders into an sRGB8 target
//! (`OFFSCREEN_FORMAT`), decodes in hardware to linear premultiplied `f32`, adds
//! `sample * weight` into an `Rgba16Float` accumulator, and resolves or blends
//! over a destination frame once per exposed output frame.

#![allow(dead_code)]

use bytemuck::{Pod, Zeroable};

const SAMPLE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const ACCUM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct ShutterUniform {
    pub weight: f32,
    pub opacity: f32,
    pub _pad: [f32; 2],
}

pub(crate) struct GpuShutter {
    size: [u32; 2],
    sample_tex: wgpu::Texture,
    pub sample_view: wgpu::TextureView,
    accum_tex: wgpu::Texture,
    pub accum_view: wgpu::TextureView,
    resolved_tex: wgpu::Texture,
    pub resolved_view: wgpu::TextureView,
    add_uniform_buf: wgpu::Buffer,
    resolve_uniform_buf: wgpu::Buffer,
    bind_layout: wgpu::BindGroupLayout,
    sample_bind: wgpu::BindGroup,
    accum_bind: wgpu::BindGroup,
    add_pipe: wgpu::RenderPipeline,
    resolve_opaque_pipe: wgpu::RenderPipeline,
    resolve_alpha_pipe: wgpu::RenderPipeline,
    composite_over_pipe: wgpu::RenderPipeline,
}

impl GpuShutter {
    pub(crate) fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let size = [size[0].max(1), size[1].max(1)];
        let shader = device.create_shader_module(wgpu::include_wgsl!("exposure_gpu.wgsl"));

        let uniform = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<ShutterUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let add_uniform_buf = uniform("gpu shutter add uniform");
        let resolve_uniform_buf = uniform("gpu shutter resolve uniform");

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu shutter bind layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu shutter pipeline layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });

        let make_pipe = |label: &str,
                         entry: &str,
                         format: wgpu::TextureFormat,
                         blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipe_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
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

        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let add_pipe = make_pipe(
            "gpu shutter add",
            "fs_add",
            ACCUM_FORMAT,
            Some(wgpu::BlendState {
                color: additive,
                alpha: additive,
            }),
        );
        let resolve_opaque_pipe = make_pipe(
            "gpu shutter resolve opaque",
            "fs_resolve_opaque",
            SAMPLE_FORMAT,
            None,
        );
        let resolve_alpha_pipe = make_pipe(
            "gpu shutter resolve alpha",
            "fs_resolve_alpha",
            SAMPLE_FORMAT,
            None,
        );
        let composite_over_pipe = make_pipe(
            "gpu shutter composite over",
            "fs_composite_over",
            SAMPLE_FORMAT,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );

        let (
            sample_tex,
            sample_view,
            sample_bind,
            accum_tex,
            accum_view,
            accum_bind,
            resolved_tex,
            resolved_view,
        ) = Self::targets(
            device,
            &bind_layout,
            &add_uniform_buf,
            &resolve_uniform_buf,
            size,
        );

        Self {
            size,
            sample_tex,
            sample_view,
            accum_tex,
            accum_view,
            resolved_tex,
            resolved_view,
            add_uniform_buf,
            resolve_uniform_buf,
            bind_layout,
            sample_bind,
            accum_bind,
            add_pipe,
            resolve_opaque_pipe,
            resolve_alpha_pipe,
            composite_over_pipe,
        }
    }

    fn targets(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        add_uniform_buf: &wgpu::Buffer,
        resolve_uniform_buf: &wgpu::Buffer,
        size: [u32; 2],
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::BindGroup,
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::BindGroup,
        wgpu::Texture,
        wgpu::TextureView,
    ) {
        let extent = wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        };
        let tex = |label: &str, format: wgpu::TextureFormat, extra: wgpu::TextureUsages| {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | extra,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            (t, v)
        };
        let (sample_tex, sample_view) = tex(
            "shutter sample",
            SAMPLE_FORMAT,
            wgpu::TextureUsages::COPY_SRC,
        );
        let (accum_tex, accum_view) =
            tex("shutter accum", ACCUM_FORMAT, wgpu::TextureUsages::empty());
        let (resolved_tex, resolved_view) = tex(
            "shutter resolved",
            SAMPLE_FORMAT,
            wgpu::TextureUsages::COPY_SRC,
        );
        let bind = |label: &str, view: &wgpu::TextureView, buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buf.as_entire_binding(),
                    },
                ],
            })
        };
        let sample_bind = bind("shutter sample bind", &sample_view, add_uniform_buf);
        let accum_bind = bind("shutter accum bind", &accum_view, resolve_uniform_buf);
        (
            sample_tex,
            sample_view,
            sample_bind,
            accum_tex,
            accum_view,
            accum_bind,
            resolved_tex,
            resolved_view,
        )
    }

    pub(crate) fn resolved_texture(&self) -> &wgpu::Texture {
        &self.resolved_tex
    }

    pub(crate) fn ensure_size(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        let size = [size[0].max(1), size[1].max(1)];
        if self.size == size {
            return;
        }
        self.size = size;
        (
            self.sample_tex,
            self.sample_view,
            self.sample_bind,
            self.accum_tex,
            self.accum_view,
            self.accum_bind,
            self.resolved_tex,
            self.resolved_view,
        ) = Self::targets(
            device,
            &self.bind_layout,
            &self.add_uniform_buf,
            &self.resolve_uniform_buf,
            size,
        );
    }

    /// Add `sample_view * weight` into `accum_view`. When `first` is true,
    /// `accum_view` clears to transparent before adding the weighted sample.
    pub(crate) fn add_sample(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        weight: f32,
        first: bool,
    ) {
        queue.write_buffer(
            &self.add_uniform_buf,
            0,
            bytemuck::bytes_of(&ShutterUniform {
                weight,
                opacity: 1.0,
                _pad: [0.0; 2],
            }),
        );
        let load = if first {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gpu shutter add"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.accum_view,
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
        pass.set_pipeline(&self.add_pipe);
        pass.set_bind_group(0, &self.sample_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Resolve `accum_view` into `target` (`Rgba8UnormSrgb`). With `straight_alpha`,
    /// premultiplied linear colors unpremultiply on the GPU for PNG / card export.
    pub(crate) fn resolve(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        opacity: f32,
        straight_alpha: bool,
    ) {
        queue.write_buffer(
            &self.resolve_uniform_buf,
            0,
            bytemuck::bytes_of(&ShutterUniform {
                weight: 1.0,
                opacity: opacity.clamp(0.0, 1.0),
                _pad: [0.0; 2],
            }),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gpu shutter resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(if straight_alpha {
            &self.resolve_alpha_pipe
        } else {
            &self.resolve_opaque_pipe
        });
        pass.set_bind_group(0, &self.accum_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Blend premultiplied `accum_view * opacity` in linear light directly over `target`.
    pub(crate) fn composite_over(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        opacity: f32,
    ) {
        queue.write_buffer(
            &self.resolve_uniform_buf,
            0,
            bytemuck::bytes_of(&ShutterUniform {
                weight: 1.0,
                opacity: opacity.clamp(0.0, 1.0),
                _pad: [0.0; 2],
            }),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gpu shutter composite over"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&self.composite_over_pipe);
        pass.set_bind_group(0, &self.accum_bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
