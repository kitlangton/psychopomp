//! Deterministic random-access timeline wrapper (`RuneTimeline<S>`) around
//! `rune::Runtime<S>`, paired with shared-device GPU linear-light shutter
//! exposure (`GpuShutter`).
//!
//! `rune::Runtime::frame(dt)` is a forward-stepping state machine for live
//! windows (discrete tree commits prune exited nodes, `.growth()` integrates on
//! `1/240 s` ticks). Wrapping `Runtime<S>` in an ordered commit schedule makes
//! any Rune view a pure function `f(t)` with `0` differing pixels between a
//! cold jump, sequential `60 Hz` playback, `1,440 Hz` shutter substeps, and
//! backward rewinds.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use rune::motion_gate::{self, Allow, Finding, Recording};
use rune::render::{Instance, OFFSCREEN_FORMAT, Renderer};
use rune::text::TextSystem;
use rune::{Cx, El, ElementInfo, Env, Runtime, Tx};

use crate::exposure_gpu::GpuShutter;
use crate::render::{BYTES_PER_PIXEL, HeadlessRenderer};

static NEXT_TIMELINE_ID: AtomicU64 = AtomicU64::new(1);

const SUBSTEP_HZ: f64 = 240.0;
const TIME_EPS: f64 = 1.0e-12;

type CommitCallback<S> = Box<dyn Fn(&mut S, &mut Tx)>;

struct ScheduledCommit<S> {
    at_nanos: u64,
    order: usize,
    label: String,
    apply: CommitCallback<S>,
}

/// Deterministic random-access schedule over a `rune::Runtime<S>`.
pub(crate) struct RuneTimeline<S: Clone + 'static> {
    id: u64,
    initial: S,
    view: fn(&S, &mut Cx<S>) -> El,
    pixel_size: [u32; 2],
    scale: f32,
    dark: bool,
    growth_grid: bool,
    commits: Vec<ScheduledCommit<S>>,
    rt: Option<Runtime<S>>,
    cur_t: f64,
    next_commit: usize,
    atlas_generation: u64,
}

impl<S: Clone + 'static> RuneTimeline<S> {
    pub(crate) fn new(
        initial: S,
        view: fn(&S, &mut Cx<S>) -> El,
        pixel_size: [u32; 2],
        scale: f32,
        dark: bool,
    ) -> Self {
        let scale = scale.max(0.25);
        Self {
            id: NEXT_TIMELINE_ID.fetch_add(1, Ordering::Relaxed),
            initial,
            view,
            pixel_size: [pixel_size[0].max(1), pixel_size[1].max(1)],
            scale,
            dark,
            growth_grid: false,
            commits: Vec::new(),
            rt: None,
            cur_t: 0.0,
            next_commit: 0,
            atlas_generation: 0,
        }
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn pixel_size(&self) -> [u32; 2] {
        self.pixel_size
    }

    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    pub(crate) fn logical_size(&self) -> (f32, f32) {
        (
            self.pixel_size[0] as f32 / self.scale,
            self.pixel_size[1] as f32 / self.scale,
        )
    }

    /// Quantize internal stepping onto Rune's `1/240 s` `.growth()` tick grid.
    #[allow(dead_code)]
    pub(crate) fn with_growth_grid(mut self, enabled: bool) -> Self {
        self.growth_grid = enabled;
        self
    }

    /// Schedule a discrete state commit at `at_nanos` on the global scene clock.
    /// Equal-timestamp commits preserve insertion order.
    pub(crate) fn commit(
        &mut self,
        at_nanos: u64,
        label: impl Into<String>,
        apply: impl Fn(&mut S, &mut Tx) + 'static,
    ) {
        let order = self.commits.len();
        self.commits.push(ScheduledCommit {
            at_nanos,
            order,
            label: label.into(),
            apply: Box::new(apply),
        });
        self.commits
            .sort_by_key(|commit| (commit.at_nanos, commit.order));
        // Invalidate cached cursor so newly inserted commits take effect.
        self.rewind_keeping_text();
    }

    fn rewind_keeping_text(&mut self) {
        let text = self
            .rt
            .take()
            .map(|rt| rt.text)
            .unwrap_or_else(opencode_text_system);
        let mut rt = Runtime::with_text(
            self.initial.clone(),
            self.view,
            self.logical_size(),
            Env::new(self.dark, self.scale),
            text,
        );
        rt.frame(0.0);
        self.rt = Some(rt);
        self.cur_t = 0.0;
        self.next_commit = 0;
        self.atlas_generation = self.atlas_generation.wrapping_add(1);
    }

    fn step_span(rt: &mut Runtime<S>, from_t: f64, to_t: f64, growth_grid: bool) {
        let total = (to_t - from_t).max(0.0);
        if total == 0.0 {
            return;
        }
        if !growth_grid {
            rt.frame(total);
            return;
        }
        let mut cur = from_t;
        let first_tick = (from_t * SUBSTEP_HZ).floor() as u64 + 1;
        let last_tick = (to_t * SUBSTEP_HZ).floor() as u64;
        for tick in first_tick..=last_tick {
            let tick_t = tick as f64 / SUBSTEP_HZ;
            let dt = (tick_t - cur).max(0.0);
            if dt > 0.0 {
                rt.frame(dt);
                cur = tick_t;
            }
        }
        let rem = (to_t - cur).max(0.0);
        if rem > 0.0 {
            rt.frame(rem);
        }
    }

    /// Evaluate the Rune runtime at `target_t` seconds (pure function of `target_t`).
    pub(crate) fn sample_at(&mut self, target_t: f64) -> &mut Runtime<S> {
        let target_t = target_t.max(0.0);
        if self.rt.is_none() || target_t + TIME_EPS < self.cur_t {
            self.rewind_keeping_text();
        }
        let growth_grid = self.growth_grid;
        {
            let rt = self.rt.as_mut().expect("runtime initialized");
            while self.next_commit < self.commits.len() {
                let commit_t = self.commits[self.next_commit].at_nanos as f64 * 1.0e-9;
                if commit_t > target_t + TIME_EPS {
                    break;
                }
                if commit_t > self.cur_t {
                    Self::step_span(rt, self.cur_t, commit_t, growth_grid);
                    self.cur_t = commit_t;
                }
                let apply = &self.commits[self.next_commit].apply;
                rt.update(|state, tx| apply(state, tx));
                rt.frame(0.0);
                self.next_commit += 1;
            }
            if target_t > self.cur_t {
                Self::step_span(rt, self.cur_t, target_t, growth_grid);
                self.cur_t = target_t;
            }
        }
        self.rt.as_mut().expect("runtime initialized")
    }

    /// Presented bounding box `[x, y, w, h]` in output pixels and effective opacity
    /// of element `key` (supports both leaf keys like `"dialog"` and slash paths
    /// like `"rows/console"` or `"seg/segment1"`).
    pub(crate) fn presented_px(&mut self, target_t: f64, key: &str) -> Option<([f32; 4], f32)> {
        let scale = self.scale;
        let rt = self.sample_at(target_t);
        if !key.contains('/')
            && let Some((rect, opacity)) = rt.presented(key)
        {
            return Some((rect.map(|v| v * scale), opacity));
        }
        let suffix = format!("/{key}");
        let elements: Vec<ElementInfo> = rt.elements();
        elements
            .into_iter()
            .find(|el| {
                if key.contains('/') {
                    el.path == key || el.path.ends_with(&suffix)
                } else {
                    el.key.as_deref() == Some(key)
                }
            })
            .map(|el| (el.rect.map(|v| v * scale), el.opacity))
    }

    /// Run `rune::motion_gate` across `[0.0 .. duration_secs]` at `hz`, recording
    /// every scheduled commit as an action input so layout/fade/streak findings
    /// are caught headlessly without rendering video.
    #[allow(dead_code)]
    pub(crate) fn motion_audit(
        &self,
        duration_secs: f64,
        hz: f64,
        allow: &[Allow],
    ) -> Vec<Finding> {
        let hz = hz.max(60.0);
        let dt = 1.0 / hz;
        let mut rt = Runtime::with_text(
            self.initial.clone(),
            self.view,
            self.logical_size(),
            Env::new(self.dark, self.scale),
            opencode_text_system(),
        );
        rt.frame(0.0);
        let mut rec = Recording::new(rt.viewport, hz);

        // Frame 0 settled baseline before any positive-time commits.
        let mut commit_idx = 0usize;
        while commit_idx < self.commits.len() && self.commits[commit_idx].at_nanos == 0 {
            let apply = &self.commits[commit_idx].apply;
            rt.update(|state, tx| apply(state, tx));
            rt.frame(0.0);
            commit_idx += 1;
        }
        rec.awake.push(false);
        rec.sample(&rt.elements());

        let total_steps = (duration_secs.max(dt) / dt).ceil() as usize;
        let mut cur_t = 0.0;
        for step in 1..=total_steps {
            let next_t = step as f64 * dt;
            while commit_idx < self.commits.len() {
                let ct = self.commits[commit_idx].at_nanos as f64 * 1.0e-9;
                if ct > next_t + TIME_EPS {
                    break;
                }
                let pre = (ct - cur_t).max(0.0);
                if pre > 0.0 {
                    rt.frame(pre);
                    cur_t = ct;
                }
                rec.input(&self.commits[commit_idx].label);
                let apply = &self.commits[commit_idx].apply;
                rt.update(|state, tx| apply(state, tx));
                rt.frame(0.0);
                commit_idx += 1;
            }
            let rem = (next_t - cur_t).max(0.0);
            let awake = rt.frame(rem);
            cur_t = next_t;
            rec.awake.push(awake);
            rec.sample(&rt.elements());
        }

        motion_gate::check(&rec)
            .into_iter()
            .filter(|finding| motion_gate::allowed(allow, finding, "rune-timeline").is_none())
            .collect()
    }
}

pub(crate) fn opencode_text_system() -> TextSystem {
    TextSystem::new(rune_theme_opencode::FONTS, rune_theme_opencode::FAMILIES)
}

/// Shared Rune GPU state hosted on `HeadlessRenderer`'s `wgpu::Device`.
pub(crate) struct RuneGpu {
    pub renderer: Renderer,
    pub shutter: GpuShutter,
    last_timeline: Option<(u64, u64)>,
    prev_instances: Vec<u8>,
}

// SAFETY: `wgpu` handles and `GpuShutter` are `Send`. `Renderer` is `!Send` only
// because its optional macOS `CVMetalTextureCache` (`SurfaceRenderer`) is `!Send`,
// and `RuneGpu` never instantiates `SurfaceRenderer` (`prepare_surfaces` is never
// called on non-empty native surfaces). Matches `rune::render::prewarm::Prewarmed`.
unsafe impl Send for RuneGpu {}

impl RuneGpu {
    pub(crate) fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        Self {
            renderer: Renderer::new(device, OFFSCREEN_FORMAT),
            shutter: GpuShutter::new(device, size),
            last_timeline: None,
            prev_instances: Vec::new(),
        }
    }

    /// Render and accumulate `exposure` (`[(time_secs, weight)]`) into `shutter.accum_view`
    /// (`Rgba16Float`), coalescing consecutive identical instance frames so still windows
    /// execute a single uber-quad pass while moving windows run all 24 HDR shutter samples.
    pub(crate) fn accumulate_exposure<S: Clone + 'static>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        timeline: &mut RuneTimeline<S>,
        exposure: &[(f64, f32)],
    ) {
        let size = timeline.pixel_size();
        self.shutter.ensure_size(device, size);
        let viewport = (size[0], size[1]);

        let mut pending_weight = 0.0_f32;
        let mut first_flush = true;
        let mut prev_clear = [0.0_f32; 4];
        let mut have_sample = false;

        let tl_id = timeline.id();
        for &(sample_t, weight) in exposure {
            if weight <= 0.0 {
                continue;
            }
            timeline.sample_at(sample_t);
            let key = (tl_id, timeline.atlas_generation);
            let rt = timeline.rt.as_mut().expect("runtime initialized");
            if self.last_timeline != Some(key) {
                rt.text.atlas.invalidate();
                rt.images.invalidate();
                self.last_timeline = Some(key);
                have_sample = false;
            }

            let clear = rt.clear_color();
            let inst_bytes: &[u8] = bytemuck::cast_slice::<Instance, u8>(&rt.instances);
            let unchanged = have_sample
                && clear == prev_clear
                && rt.text.atlas.dirty.is_none()
                && inst_bytes == self.prev_instances.as_slice();

            if unchanged {
                pending_weight += weight;
                continue;
            }

            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rune shutter sample"),
            });
            if have_sample && pending_weight > 0.0 {
                self.shutter
                    .add_sample(queue, &mut enc, pending_weight, first_flush);
                first_flush = false;
            }

            self.renderer.upload_images(device, queue, &mut rt.images);
            self.renderer
                .prepare(device, queue, &mut rt.text.atlas, &rt.instances, viewport);
            self.renderer
                .draw(&mut enc, &self.shutter.sample_view, clear);
            self.renderer.submit(queue, enc);

            self.prev_instances.clear();
            self.prev_instances.extend_from_slice(inst_bytes);
            prev_clear = clear;
            pending_weight = weight;
            have_sample = true;
        }

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rune shutter final flush"),
        });
        if have_sample && pending_weight > 0.0 {
            self.shutter
                .add_sample(queue, &mut enc, pending_weight, first_flush);
        } else {
            let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rune shutter empty clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.shutter.accum_view,
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
        }
        queue.submit([enc.finish()]);
    }
}

impl HeadlessRenderer {
    pub(crate) fn ensure_rune_gpu(&mut self) -> &mut RuneGpu {
        let size = [self.spec.width, self.spec.height];
        let device = &self.device;
        self.rune_gpu
            .get_or_insert_with(|| RuneGpu::new(device, size))
    }

    /// Render a `RuneTimeline<S>` across `exposure` on the shared `wgpu::Device`
    /// and read back a single exposed `Rgba8UnormSrgb` frame.
    #[allow(dead_code)]
    pub(crate) fn render_rune_exposure<S: Clone + 'static>(
        &mut self,
        timeline: &mut RuneTimeline<S>,
        exposure: &[(f64, f32)],
        opacity: f32,
        straight_alpha: bool,
    ) -> Result<Vec<u8>> {
        self.ensure_rune_gpu();
        let rune_gpu = self.rune_gpu.as_mut().expect("rune_gpu initialized");
        rune_gpu.accumulate_exposure(&self.device, &self.queue, timeline, exposure);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rune exposure resolve"),
            });
        rune_gpu.shutter.resolve(
            &self.queue,
            &mut encoder,
            &self.view,
            opacity,
            straight_alpha,
        );
        self.read_frame(encoder)
    }

    /// Accumulate a `RuneTimeline<S>` across `exposure` in linear `Rgba16Float` on
    /// the GPU and composite it over `base_pixels` (`WIDTH × HEIGHT × 4`) in a
    /// single upload + linear blend + readback pass.
    #[allow(dead_code)]
    pub(crate) fn composite_rune_exposure<S: Clone + 'static>(
        &mut self,
        base_pixels: &mut [u8],
        timeline: &mut RuneTimeline<S>,
        exposure: &[(f64, f32)],
        opacity: f32,
    ) -> Result<()> {
        if opacity <= 0.001 {
            return Ok(());
        }
        self.ensure_rune_gpu();
        let rune_gpu = self.rune_gpu.as_mut().expect("rune_gpu initialized");
        rune_gpu.accumulate_exposure(&self.device, &self.queue, timeline, exposure);

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            base_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.spec.width * BYTES_PER_PIXEL),
                rows_per_image: Some(self.spec.height),
            },
            wgpu::Extent3d {
                width: self.spec.width,
                height: self.spec.height,
                depth_or_array_layers: 1,
            },
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rune composite over base"),
            });
        rune_gpu
            .shutter
            .composite_over(&self.queue, &mut encoder, &self.view, opacity);
        let out = self.read_frame(encoder)?;
        base_pixels.copy_from_slice(&out);
        Ok(())
    }

    /// Accumulate and resolve a `RuneTimeline<S>` into `shutter.resolved_view` on
    /// the GPU with zero CPU readback (for 2.5D `StageElement::RuneCard` and live
    /// Studio preview).
    #[allow(dead_code)]
    pub(crate) fn render_rune_texture_exposure<S: Clone + 'static>(
        &mut self,
        timeline: &mut RuneTimeline<S>,
        exposure: &[(f64, f32)],
        opacity: f32,
        straight_alpha: bool,
    ) -> [u32; 2] {
        self.ensure_rune_gpu();
        let rune_gpu = self.rune_gpu.as_mut().expect("rune_gpu initialized");
        rune_gpu.accumulate_exposure(&self.device, &self.queue, timeline, exposure);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rune texture resolve"),
            });
        rune_gpu.shutter.resolve(
            &self.queue,
            &mut encoder,
            &rune_gpu.shutter.resolved_view,
            opacity,
            straight_alpha,
        );
        self.queue.submit([encoder.finish()]);
        timeline.pixel_size()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rune::{
        Color, Cx, El, Shadow, Spring, TextEdit, TextStyle, Transition, col, row, spacer, text,
    };
    use rune_kit::{Badge, SegmentedControl, TextInput};

    use super::RuneTimeline;
    use crate::exposure::{HEIGHT, SHUTTER_SECONDS, WIDTH, exposure};
    use crate::render::{HeadlessRenderer, RenderSpec};

    #[derive(Clone)]
    struct DemoState {
        tab: usize,
        show_go: bool,
        renamed: bool,
        search: TextEdit,
    }

    impl Default for DemoState {
        fn default() -> Self {
            Self {
                tab: 0,
                show_go: true,
                renamed: false,
                search: TextEdit::new(""),
            }
        }
    }

    fn connect_view(s: &DemoState, _cx: &mut Cx<DemoState>) -> El {
        let k = rune_theme_opencode::kit(true);
        let c = &k.c;
        let tabs = ["Cost", "Tokens", "Web Search"];
        let rows: &[(&str, &str, &str, bool)] = &[
            ("go", "OpenCode Go", "API key only", true),
            (
                "console",
                if s.renamed {
                    "OpenCode"
                } else {
                    "OpenCode Console"
                },
                if s.renamed {
                    "sign in or API key · Go included"
                } else {
                    "sign in or API key"
                },
                false,
            ),
            ("anthropic", "Anthropic", "Claude Max or API key", false),
            ("openai", "OpenAI", "ChatGPT Plus/Pro or API key", false),
        ];

        let mut list = col().key("rows").w_full().gap(4.0);
        for &(id, title, hint, warn) in rows {
            if id == "go" && !s.show_go {
                continue;
            }
            let selected = id == "console";
            let item = row()
                .key(id)
                .w_full()
                .h(38.0)
                .px(12.0)
                .radius(6.0)
                .center_y()
                .fill(if selected {
                    c.layer_02
                } else {
                    Color::TRANSPARENT
                })
                .motion(Spring::new(0.38, 0.0))
                .transition(Transition::opacity())
                .child(
                    text(title)
                        .key("name")
                        .font(TextStyle::new(13.5, 520.0))
                        .color(if warn { c.danger.fg } else { c.text_base }),
                )
                .child(spacer())
                .child(text(hint).key("hint").ride_trailing().font(k.ty.ui).color(
                    if selected && s.renamed {
                        c.success.fg
                    } else {
                        c.text_muted
                    },
                ));
            list = list.child(item);
        }

        let dialog = col()
            .key("dialog")
            .w(430.0)
            .padding(18.0)
            .gap(14.0)
            .radius(12.0)
            .fill(c.layer_01)
            .box_shadow(Shadow {
                y: 16.0,
                blur: 36.0,
                spread: 0.5,
                color: c.border_base,
                ..Default::default()
            })
            .child(
                row()
                    .key("hdr")
                    .w_full()
                    .center_y()
                    .child(
                        text("Connect an integration")
                            .font(TextStyle::new(15.0, 600.0))
                            .color(c.text_base),
                    )
                    .child(spacer())
                    .child(Badge::new("esc").build(k)),
            )
            .child(
                TextInput::new(&s.search)
                    .placeholder("Search 230 providers…")
                    .build(k),
            )
            .child(
                row()
                    .key("seg-row")
                    .w_full()
                    .center_y()
                    .child(text("Usage view").font(k.ty.ui).color(c.text_muted))
                    .child(spacer())
                    .child(
                        SegmentedControl::new(&tabs, Some(s.tab))
                            .width(Some(258.0))
                            .build(k)
                            .key("seg"),
                    ),
            )
            .child(
                text("Popular")
                    .font(TextStyle::new(11.5, 560.0))
                    .color(c.text_muted),
            )
            .child(list);

        col()
            .key("canvas")
            .w_full()
            .h_full()
            .fill(c.bg_deep)
            .center()
            .child(dialog)
    }

    fn scheduled_timeline() -> RuneTimeline<DemoState> {
        let mut tl = RuneTimeline::new(
            DemoState::default(),
            connect_view,
            [WIDTH, HEIGHT],
            2.0,
            true,
        );
        // Intentionally register out of order to verify stable timestamp sorting.
        tl.commit(920_000_000, "select web search", |s, _| s.tab = 2);
        tl.commit(400_000_000, "select tokens", |s, _| s.tab = 1);
        tl.commit(800_000_000, "merge go into console", |s, _| {
            s.show_go = false;
            s.renamed = true;
        });
        tl
    }

    fn geometry_signature(instances: &[rune::render::Instance]) -> Vec<u8> {
        let mut normalized = instances.to_vec();
        for inst in &mut normalized {
            // Internal glyph atlas shelf UVs depend on which subpixel phases cached first;
            // GPU readback tests below verify exact rendered pixel equality.
            inst.uv = [0.0; 4];
            inst.params[0] = 0.0;
        }
        bytemuck::cast_slice(&normalized).to_vec()
    }

    #[test]
    fn cpu_instances_and_presented_anchors_match_out_of_order_and_on_rewind() {
        let probe_t = 0.96;

        let mut direct = scheduled_timeline();
        let anchor_direct = direct
            .presented_px(probe_t, "rows/console")
            .expect("rows/console present");
        let sig_direct = geometry_signature(&direct.sample_at(probe_t).instances);

        // Step forward at 60 Hz, overshoot to 1.35s, then rewind backward to 0.96s!
        let mut scrubbed = scheduled_timeline();
        for f in 1..=81 {
            scrubbed.sample_at(f as f64 / 60.0);
        }
        let anchor_rewound = scrubbed
            .presented_px(probe_t, "rows/console")
            .expect("rows/console present after rewind");
        let sig_rewound = geometry_signature(&scrubbed.sample_at(probe_t).instances);

        assert_eq!(anchor_direct, anchor_rewound);
        assert_eq!(sig_direct, sig_rewound);
    }

    fn diff_pixels(a: &[u8], b: &[u8]) -> (usize, u8) {
        assert_eq!(a.len(), b.len());
        let mut changed = 0usize;
        let mut max_delta = 0u8;
        for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
            if pa != pb {
                changed += 1;
                for c in 0..4 {
                    max_delta = max_delta.max(pa[c].abs_diff(pb[c]));
                }
            }
        }
        (changed, max_delta)
    }

    #[test]
    fn gpu_shutter_and_random_access_have_zero_pixel_diff() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: WIDTH,
            height: HEIGHT,
            file_name: "rune-timeline-proof".into(),
        }))
        .expect("headless GPU");

        let probe_t = 0.96;
        let single = [(probe_t, 1.0_f32)];

        // 1. Direct cold jump (0 -> 0.40 -> 0.80 -> 0.92 -> 0.96)
        let mut tl_jump = scheduled_timeline();
        let px_jump = renderer
            .render_rune_exposure(&mut tl_jump, &single, 1.0, false)
            .unwrap();

        // 2. Sequential 60 Hz playback
        let mut tl_60 = scheduled_timeline();
        for f in 1..=57 {
            tl_60.sample_at(f as f64 / 60.0);
        }
        let px_60 = renderer
            .render_rune_exposure(&mut tl_60, &single, 1.0, false)
            .unwrap();

        // 3. 1,440 Hz shutter substepping
        let mut tl_1440 = scheduled_timeline();
        for s in 1..=1382 {
            tl_1440.sample_at(s as f64 / 1440.0);
        }
        let px_1440 = renderer
            .render_rune_exposure(&mut tl_1440, &single, 1.0, false)
            .unwrap();

        // 4. Forward to 1.4s then backward rewind to 0.96s on the same timeline
        tl_jump.sample_at(1.40);
        let px_rewind = renderer
            .render_rune_exposure(&mut tl_jump, &single, 1.0, false)
            .unwrap();

        // 5. Fresh independent HeadlessRenderer
        let mut renderer2 = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: WIDTH,
            height: HEIGHT,
            file_name: "rune-timeline-proof-2".into(),
        }))
        .expect("second headless GPU");
        let mut tl_fresh = scheduled_timeline();
        let px_fresh = renderer2
            .render_rune_exposure(&mut tl_fresh, &single, 1.0, false)
            .unwrap();

        assert_eq!(diff_pixels(&px_jump, &px_60), (0, 0), "jump vs 60 Hz");
        assert_eq!(diff_pixels(&px_jump, &px_1440), (0, 0), "jump vs 1440 Hz");
        assert_eq!(diff_pixels(&px_jump, &px_rewind), (0, 0), "jump vs rewind");
        assert_eq!(
            diff_pixels(&px_jump, &px_fresh),
            (0, 0),
            "jump vs fresh renderer"
        );

        // 6. 24-sample 180-deg GPU Rgba16Float shutter exposure mid-motion (t = 0.845s)
        let samples_24 = exposure(0.845, SHUTTER_SECONDS * 2.0, 24);
        let mut warm_ms = Vec::new();
        for run in 0..8 {
            let mut tl = scheduled_timeline();
            tl.sample_at(samples_24[0].0);
            let t0 = Instant::now();
            let px = renderer
                .render_rune_exposure(&mut tl, &samples_24, 1.0, false)
                .unwrap();
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(px.len(), (WIDTH * HEIGHT * 4) as usize);
            if run > 0 {
                warm_ms.push(ms);
            }
        }
        warm_ms.sort_by(f64::total_cmp);
        let median_ms = warm_ms[warm_ms.len() / 2];
        eprintln!(
            "RUNE_SHUTTER_24_MEDIAN_MS: {:.2} {:?}",
            median_ms,
            warm_ms
                .iter()
                .map(|x| (x * 100.0).round() / 100.0)
                .collect::<Vec<_>>()
        );
    }
}
