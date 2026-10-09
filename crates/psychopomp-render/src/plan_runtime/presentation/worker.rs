use std::{
    sync::{
        Arc,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use anyhow::{Context, Result};
use psychopomp::timeline::Timeline;
use winit::event_loop::EventLoopProxy;

use super::super::{PreparedPlan, VisualSampleKey};
use super::debug::DebugState;
use super::effective_grid_palette;
use super::scheduler::RequestStamp;
use crate::render::{GridLinePalette, HeadlessRenderer, Theme};

pub(super) enum RenderEvent {
    Frame {
        stamp: RequestStamp,
        pixels: Arc<Vec<u8>>,
        requested_at: Instant,
        render_time: std::time::Duration,
    },
    Failed(String),
}

pub(super) struct RenderWorker {
    requests: Option<SyncSender<RenderRequest>>,
    thread: Option<JoinHandle<()>>,
}

struct RenderRequest {
    stamp: RequestStamp,
    debug: Option<DebugState>,
    timeline: Arc<Timeline>,
    requested_at: Instant,
}

#[derive(Debug, PartialEq)]
struct FrameKey {
    slide_index: usize,
    visual: VisualSampleKey,
    palette: GridLinePalette,
    theme: Theme,
}

#[derive(Default)]
struct FrameCache {
    frame: Option<(FrameKey, Arc<Vec<u8>>)>,
    count: u32,
    total: std::time::Duration,
}

impl FrameCache {
    fn render(
        &mut self,
        renderer: &mut HeadlessRenderer,
        prepared: &PreparedPlan,
        request: &RenderRequest,
    ) -> Result<Arc<Vec<u8>>> {
        let clean = self.render_clean(renderer, prepared, request)?;
        if let Some(debug) = request.debug {
            let mut pixels = clean.as_ref().clone();
            debug.paint(
                prepared,
                renderer,
                &mut pixels,
                request.stamp.sample,
                &request.timeline,
            );
            Ok(Arc::new(pixels))
        } else {
            Ok(clean)
        }
    }

    fn render_clean(
        &mut self,
        renderer: &mut HeadlessRenderer,
        prepared: &PreparedPlan,
        request: &RenderRequest,
    ) -> Result<Arc<Vec<u8>>> {
        let stamp = request.stamp;
        let time = stamp.sample.at_nanos as f64 / 1_000_000_000.;
        let key = FrameKey {
            slide_index: stamp.slide_index,
            visual: prepared.visual_sample_key_using(time, &request.timeline)?,
            palette: stamp.palette,
            theme: stamp.theme,
        };
        if let Some((previous, pixels)) = &self.frame
            && *previous == key
        {
            return Ok(pixels.clone());
        }
        renderer.set_file_name(prepared.file_name());
        renderer.set_theme(stamp.theme);
        renderer.set_grid_line_palette(effective_grid_palette(stamp.theme, stamp.palette));
        let start = Instant::now();
        let pixels = Arc::new(prepared.render_sample_using(renderer, time, &request.timeline)?);
        self.total += start.elapsed();
        self.count += 1;
        self.frame = Some((key, pixels.clone()));
        Ok(pixels)
    }
}

impl RenderWorker {
    pub(super) fn spawn(
        slides: Vec<PreparedPlan>,
        mut renderer: HeadlessRenderer,
        proxy: EventLoopProxy<RenderEvent>,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<RenderRequest>(1);
        let thread = thread::Builder::new()
            .name("psychopomp-render".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let mut cached = FrameCache::default();
                    while let Ok(request) = receiver.recv() {
                        let render_started = Instant::now();
                        let prepared = slides
                            .get(request.stamp.slide_index)
                            .context("unknown requested slide")?;
                        let pixels = cached.render(&mut renderer, prepared, &request)?;
                        if proxy
                            .send_event(RenderEvent::Frame {
                                stamp: request.stamp,
                                pixels,
                                requested_at: request.requested_at,
                                render_time: render_started.elapsed(),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    if cached.count > 0 {
                        eprintln!(
                            "Native preview: {} fresh samples, {:.1} ms/sample mean",
                            cached.count,
                            cached.total.as_secs_f64() * 1000.0 / f64::from(cached.count)
                        );
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = proxy.send_event(RenderEvent::Failed(format!("{error:#}")));
                }
            })
            .context("start render worker")?;
        Ok(Self {
            requests: Some(sender),
            thread: Some(thread),
        })
    }

    pub(super) fn request(
        &self,
        stamp: RequestStamp,
        timeline: Arc<Timeline>,
        debug: Option<DebugState>,
    ) -> Result<()> {
        anyhow::ensure!(
            stamp.debug == debug.is_some(),
            "request stamp must describe the diagnostic payload"
        );
        self.requests
            .as_ref()
            .context("render worker has stopped")?
            .try_send(RenderRequest {
                stamp,
                debug,
                timeline,
                requested_at: Instant::now(),
            })
            .context("send render request")
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::new_renderer;
    use super::*;
    use psychopomp::playback::PlaybackSample;
    use std::path::Path;

    #[test]
    fn request_packet_preserves_provenance_and_rejects_bad_or_unavailable_sends() {
        use psychopomp::playback::Playback;
        let mut plan = psychopomp::author::PlanBuilder::new("packet", 1_000_000_000);
        plan.presentation_step("start", "Start", 0, 0);
        let plan = plan.finish().unwrap();
        let timeline = Arc::new(Timeline::compile_events([], [], 0.).unwrap());
        let mut playback = Playback::new(&plan, &timeline, false).unwrap();
        let sample = playback.sample(std::time::Duration::ZERO);
        let stamp = RequestStamp {
            slide_index: 0,
            sample,
            palette: GridLinePalette::Orange,
            theme: Theme::Original,
            debug: true,
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = RenderWorker {
            requests: Some(sender),
            thread: None,
        };
        assert!(worker.request(stamp, timeline.clone(), None).is_err());
        assert!(receiver.try_recv().is_err());
        let before = Instant::now();
        worker
            .request(
                stamp,
                timeline.clone(),
                Some(DebugState::capture(&playback, sample)),
            )
            .unwrap();
        let packet = receiver.recv().unwrap();
        assert_eq!(packet.stamp, stamp);
        assert!(packet.debug.is_some());
        assert!(Arc::ptr_eq(&packet.timeline, &timeline));
        assert!(packet.requested_at >= before && packet.requested_at <= Instant::now());
        let stamp = RequestStamp {
            debug: false,
            ..stamp
        };
        worker.request(stamp, timeline.clone(), None).unwrap();
        assert!(worker.request(stamp, timeline.clone(), None).is_err());
        drop(receiver);
        assert!(worker.request(stamp, timeline, None).is_err());
    }

    #[test]
    #[ignore = "requires headless GPU; slow inspection uses the same pixels and never contaminates the clean cache"]
    fn slow_debug_frames_expose_stagger_without_retiming_or_dirtying_cached_pixels() {
        use psychopomp::playback::{PlaybackCommand, PlaybackSpeed};
        use std::time::Duration;
        let plan = psychopomp_component_prototypes::build_slideshow_deck()
            .unwrap()
            .slides
            .pop()
            .unwrap()
            .plan;
        let mut renderer = pollster::block_on(new_renderer("slow-inspection-proof")).unwrap();
        renderer.set_interactive_preview(true);
        let prepared = PreparedPlan::prepare(plan, Path::new("."), &mut renderer).unwrap();
        let mut playback = prepared.playback(false).unwrap();
        playback.set_speed(PlaybackSpeed::Quarter, Duration::ZERO);
        playback.command(PlaybackCommand::Next, Duration::ZERO);
        let sample = playback.sample(Duration::from_millis(320));
        assert_eq!(sample.at_nanos, 80_000_000);
        let pending = playback.pending_starts(Duration::from_nanos(sample.at_nanos));
        assert_eq!(pending.count, 3);
        assert_eq!(pending.until_next, Some(Duration::from_millis(40)));
        let mut request = RenderRequest {
            stamp: RequestStamp {
                slide_index: 0,
                sample,
                palette: GridLinePalette::Orange,
                theme: Theme::Original,
                debug: false,
            },
            debug: None,
            timeline: playback.timeline(),
            requested_at: Instant::now(),
        };
        let mut cache = FrameCache::default();
        let clean = cache.render(&mut renderer, &prepared, &request).unwrap();
        assert!(clean.as_ref() == &prepared.render_sample(&mut renderer, 3.08).unwrap());
        request.debug = Some(DebugState::capture(&playback, sample));
        request.stamp.debug = true;
        let debug = cache.render(&mut renderer, &prepared, &request).unwrap();
        assert_ne!(clean, debug);
        assert_eq!(cache.count, 1);
        assert_eq!(
            &clean[210 * 1920 * 4..],
            &debug[210 * 1920 * 4..],
            "HUD must not alter the animation underneath"
        );
        request.debug = None;
        request.stamp.debug = false;
        assert!(
            Arc::ptr_eq(
                &clean,
                &cache.render(&mut renderer, &prepared, &request).unwrap()
            ),
            "hiding debug restores the original clean cache object"
        );
        if let Some(path) = std::env::var_os("PSYCHOPOMP_DEBUG_ARTIFACTS") {
            let path = std::path::PathBuf::from(path);
            std::fs::create_dir_all(&path).unwrap();
            for millis in [0, 240, 480, 720, 960, 1280, 2000] {
                let sample = PlaybackSample {
                    at_nanos: millis * 1_000_000 / 4,
                    ..sample
                };
                request.stamp.sample = sample;
                request.debug = Some(DebugState::capture(&playback, sample));
                request.stamp.debug = true;
                let pixels = cache.render(&mut renderer, &prepared, &request).unwrap();
                crate::plan_runtime::delivery::write_png(
                    &path.join(format!("quarter-{millis:04}.png")),
                    &pixels,
                )
                .unwrap();
            }
            if std::env::var_os("PSYCHOPOMP_DEBUG_VIDEO").is_some() {
                for (speed, frames, name) in [
                    (PlaybackSpeed::Normal, 84, "normal"),
                    (PlaybackSpeed::Quarter, 336, "quarter"),
                ] {
                    let mut clock = prepared.playback(false).unwrap();
                    clock.set_speed(speed, Duration::ZERO);
                    clock.command(PlaybackCommand::Next, Duration::ZERO);
                    let mut video = crate::encode::FfmpegEncoder::start_with_media(
                        &path.join(format!("{name}.mp4")),
                        crate::encode::VideoSpec {
                            width: 1920,
                            height: 1080,
                            fps: 60,
                        },
                        &[],
                    )
                    .unwrap();
                    request.timeline = clock.timeline();
                    for frame in 0..frames {
                        let sample = clock.sample(Duration::from_secs_f64(f64::from(frame) / 60.));
                        request.stamp.sample = sample;
                        request.debug = Some(DebugState::capture(&clock, sample));
                        request.stamp.debug = true;
                        video
                            .write_frame(&cache.render(&mut renderer, &prepared, &request).unwrap())
                            .unwrap();
                    }
                    video.finish().unwrap();
                }
            }
        }
        playback.command(PlaybackCommand::Previous, Duration::from_millis(320));
        assert_eq!(playback.pending_starts(Duration::from_millis(80)).count, 0);
    }

    #[test]
    #[ignore = "requires headless GPU; themes invalidate held caches without changing clocks or geometry"]
    fn themes_repaint_every_native_recipe_and_restore_original_pixels() {
        let mut plans = psychopomp_component_prototypes::build_slideshow_deck()
            .unwrap()
            .slides
            .into_iter()
            .map(|s| s.plan)
            .collect::<Vec<_>>();
        plans.extend(
            psychopomp_interactive_showcase::build_deck()
                .unwrap()
                .slides
                .into_iter()
                .map(|s| s.plan),
        );
        plans.push(
            psychopomp_data_modeling::build_deck().unwrap().slides[0]
                .plan
                .clone(),
        );
        let mut renderer = pollster::block_on(new_renderer("theme-proof")).unwrap();
        renderer.set_interactive_preview(true);
        for plan in plans {
            renderer.set_theme(Theme::Original);
            let prepared = PreparedPlan::prepare(plan, Path::new("."), &mut renderer).unwrap();
            let mut playback = prepared.playback(true).unwrap();
            playback.command(
                psychopomp::playback::PlaybackCommand::Last,
                std::time::Duration::ZERO,
            );
            let mut request = RenderRequest {
                stamp: RequestStamp {
                    slide_index: 0,
                    sample: playback.sample(std::time::Duration::ZERO),
                    theme: Theme::Original,
                    palette: GridLinePalette::Orange,
                    debug: false,
                },
                debug: None,
                timeline: playback.timeline(),
                requested_at: Instant::now(),
            };
            let visual = prepared
                .visual_sample_key_using(
                    request.stamp.sample.at_nanos as f64 / 1e9,
                    &request.timeline,
                )
                .unwrap();
            let mut cache = FrameCache::default();
            let original = cache.render(&mut renderer, &prepared, &request).unwrap();
            for theme in Theme::ALL.into_iter().skip(1) {
                request.stamp.theme = theme;
                let pixels = cache.render(&mut renderer, &prepared, &request).unwrap();
                assert_ne!(pixels, original, "{} {theme:?}", prepared.plan.id);
                assert!(Arc::ptr_eq(
                    &pixels,
                    &cache.render(&mut renderer, &prepared, &request).unwrap()
                ));
                assert_eq!(
                    playback.sample(std::time::Duration::ZERO),
                    request.stamp.sample
                );
                assert_eq!(
                    visual,
                    prepared
                        .visual_sample_key_using(
                            request.stamp.sample.at_nanos as f64 / 1e9,
                            &request.timeline
                        )
                        .unwrap()
                );
                if theme == Theme::Black {
                    assert_eq!(&pixels[..4], &[0, 0, 0, 255], "{}", prepared.plan.id);
                }
            }
            request.stamp.theme = Theme::Original;
            assert_eq!(
                original,
                cache.render(&mut renderer, &prepared, &request).unwrap(),
                "theme round-trip {}",
                prepared.plan.id
            );
        }
    }

    #[test]
    #[ignore = "requires headless GPU; a theme file repaints native recipes in its palette and restores the original"]
    fn theme_files_paint_their_background_and_restore_original_pixels() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes/light.json");
        let light = Theme::parse(path.to_str().unwrap()).unwrap();
        let background = light.palette().background;
        let plans = psychopomp_component_prototypes::build_slideshow_deck()
            .unwrap()
            .slides
            .into_iter()
            .map(|s| s.plan);
        let mut renderer = pollster::block_on(new_renderer("theme-file-proof")).unwrap();
        renderer.set_interactive_preview(true);
        for plan in plans {
            renderer.set_theme(Theme::Original);
            let prepared = PreparedPlan::prepare(plan, Path::new("."), &mut renderer).unwrap();
            let mut playback = prepared.playback(true).unwrap();
            playback.command(
                psychopomp::playback::PlaybackCommand::Last,
                std::time::Duration::ZERO,
            );
            let mut request = RenderRequest {
                stamp: RequestStamp {
                    slide_index: 0,
                    sample: playback.sample(std::time::Duration::ZERO),
                    theme: Theme::Original,
                    palette: GridLinePalette::Orange,
                    debug: false,
                },
                debug: None,
                timeline: playback.timeline(),
                requested_at: Instant::now(),
            };
            let mut cache = FrameCache::default();
            let original = cache.render(&mut renderer, &prepared, &request).unwrap();
            request.stamp.theme = light;
            let pixels = cache.render(&mut renderer, &prepared, &request).unwrap();
            assert_ne!(pixels, original, "{}", prepared.plan.id);
            assert_eq!(
                &pixels[..4],
                &[background[0], background[1], background[2], 255],
                "{} paints the theme file's background",
                prepared.plan.id
            );
            request.stamp.theme = Theme::Original;
            assert_eq!(
                original,
                cache.render(&mut renderer, &prepared, &request).unwrap(),
                "theme file round-trip {}",
                prepared.plan.id
            );
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; exercises the worker cache at an unchanged held sample"]
    fn grid_palette_changes_invalidate_held_pixels_not_motion() {
        let plan = psychopomp_keyed_grid::build_deck().unwrap().slides[1]
            .plan
            .clone();
        let mut renderer = pollster::block_on(new_renderer(&plan.id)).unwrap();
        let prepared = PreparedPlan::prepare(plan, Path::new("."), &mut renderer).unwrap();
        let mut playback = prepared.playback(true).unwrap();
        let mut request = RenderRequest {
            stamp: RequestStamp {
                slide_index: 0,
                sample: playback.sample(std::time::Duration::ZERO),
                palette: GridLinePalette::Orange,
                theme: Theme::Original,
                debug: false,
            },
            debug: None,
            timeline: playback.timeline(),
            requested_at: Instant::now(),
        };
        let mut cache = FrameCache::default();
        let default_pixels = prepared
            .render_sample_using(&mut renderer, 0., &request.timeline)
            .unwrap();
        let original = cache.render(&mut renderer, &prepared, &request).unwrap();
        assert_eq!(
            original.as_ref(),
            &default_pixels,
            "Orange must match ordinary export pixels"
        );
        let visual = prepared
            .visual_sample_key_using(0., &request.timeline)
            .unwrap();
        for (index, palette) in GridLinePalette::ALL.into_iter().enumerate() {
            request.stamp.palette = palette;
            let pixels = cache.render(&mut renderer, &prepared, &request).unwrap();
            assert!(
                Arc::ptr_eq(
                    &pixels,
                    &cache.render(&mut renderer, &prepared, &request).unwrap()
                ),
                "same palette and hold should reuse pixels"
            );
            assert_eq!(
                cache.count,
                index as u32 + 1,
                "palette changes must bypass the held-frame cache"
            );
            if index > 0 {
                assert_ne!(original, pixels);
            }
            assert_eq!(
                visual,
                prepared
                    .visual_sample_key_using(0., &request.timeline)
                    .unwrap()
            );
            // Header and captions are outside the grid. A color audition cannot
            // recolor text, seek the scene, or repaint the surrounding slide UI.
            assert_eq!(&original[..280 * 1920 * 4], &pixels[..280 * 1920 * 4]);
            assert_eq!(&original[820 * 1920 * 4..], &pixels[820 * 1920 * 4..]);
            if let Some(path) = std::env::var_os("PSYCHOPOMP_GRID_STYLE_ARTIFACTS") {
                let path = std::path::PathBuf::from(path);
                std::fs::create_dir_all(&path).unwrap();
                crate::plan_runtime::delivery::write_png(
                    &path.join(format!("{index}-{palette:?}.png")),
                    &pixels,
                )
                .unwrap();
            }
        }
        request.stamp.palette = GridLinePalette::Orange;
        assert_eq!(
            original,
            cache.render(&mut renderer, &prepared, &request).unwrap(),
            "cycling back restores exact pixels"
        );
    }

    #[test]
    #[ignore = "requires a headless GPU and fonts; measures the actual live sample path"]
    fn live_sampling_cost_and_out_of_order_pixels() {
        let plan = psychopomp_effect_succeed_slides::build_plan().unwrap();
        let mut renderer = pollster::block_on(new_renderer(&plan.id)).unwrap();
        let prepared = PreparedPlan::prepare(plan, Path::new("."), &mut renderer).unwrap();
        renderer.set_file_name(prepared.file_name());
        for preview in [false, true] {
            renderer.set_interactive_preview(preview);
            let initial = prepared.render_sample(&mut renderer, 0.0).unwrap();
            let final_frame = prepared.render_sample(&mut renderer, 12.5).unwrap();
            let mut elapsed = Vec::new();
            for index in 0..24 {
                let time = 1.0 + f64::from(index) / 60.0;
                let start = Instant::now();
                prepared.render_sample(&mut renderer, time).unwrap();
                elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            assert_eq!(initial, prepared.render_sample(&mut renderer, 0.0).unwrap());
            assert_eq!(
                final_frame,
                prepared.render_sample(&mut renderer, 12.5).unwrap()
            );
            elapsed.sort_by(f64::total_cmp);
            eprintln!(
                "1080p preview={preview}: median {:.2}ms, p95 {:.2}ms",
                elapsed[12], elapsed[22]
            );
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; proves live retarget rendering, not just scalar values"]
    fn interrupted_reversal_preserves_the_rendered_frame() {
        use psychopomp::playback::{Playback, PlaybackCommand};
        use std::time::Duration;
        let plan = psychopomp_effect_succeed_slides::build_plan().unwrap();
        let mut renderer = pollster::block_on(new_renderer(&plan.id)).unwrap();
        let prepared = PreparedPlan::prepare(plan.clone(), Path::new("."), &mut renderer).unwrap();
        renderer.set_file_name(prepared.file_name());
        renderer.set_interactive_preview(true);
        let mut playback = Playback::new(&plan, &prepared.timeline, false).unwrap();
        let initial = prepared
            .render_sample_using(&mut renderer, 0., &playback.timeline())
            .unwrap();
        playback.command(PlaybackCommand::Next, Duration::ZERO);
        let before = prepared
            .render_sample_using(&mut renderer, 0.15, &playback.timeline())
            .unwrap();
        assert_ne!(initial, before);
        playback.command(PlaybackCommand::Previous, Duration::from_millis(150));
        let after = prepared
            .render_sample_using(&mut renderer, 0.15, &playback.timeline())
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            initial,
            prepared
                .render_sample_using(&mut renderer, 2., &playback.timeline())
                .unwrap()
        );
        assert_eq!(
            after,
            prepared
                .render_sample_using(&mut renderer, 0.15, &playback.timeline())
                .unwrap()
        );
    }
}
