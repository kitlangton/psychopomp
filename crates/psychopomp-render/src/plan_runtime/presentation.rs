//! Native desktop playback. Winit owns the window and input; the existing Rust
//! renderer samples the scene on a worker. No video export, browser, or decoder
//! is involved unless the authored scene itself contains recorded video.
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use psychopomp::{
    plan::SlidePlan,
    playback::{Playback, PlaybackCommand, PlaybackPhase, PlaybackSample, PlaybackSpeed},
    timeline::PropertyId,
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Fullscreen, Window, WindowId, WindowLevel},
};

use super::{PreparedPlan, new_renderer};
use crate::{
    exposure::{HEIGHT, WIDTH},
    render::{GridLinePalette, Theme},
};
mod benchmark;
mod debug;
mod gpu;
mod preferences;
mod scheduler;
mod worker;
use benchmark::Benchmark;
use scheduler::{Event as ScheduleEvent, FrameScheduler, RequestStamp};
use worker::{RenderEvent, RenderWorker};

#[derive(Default)]
pub(super) struct Options {
    reduced_motion: bool,
    full_quality: bool,
    benchmark: bool,
    benchmark_gpu: bool,
    fps: Option<u32>,
    theme: Option<Theme>,
    speed: PlaybackSpeed,
    debug: bool,
}

impl Options {
    pub(super) fn parse(flags: &[String]) -> Result<Self> {
        let mut options = Self::default();
        let mut flags = flags.iter();
        while let Some(flag) = flags.next() {
            match flag.as_str() {
                "--reduced-motion" => options.reduced_motion = true,
                "--full-quality" => options.full_quality = true,
                "--benchmark" => options.benchmark = true,
                "--benchmark-gpu" => {
                    options.benchmark = true;
                    options.benchmark_gpu = true;
                }
                "--fps" => {
                    let fps = flags
                        .next()
                        .context("--fps requires an integer from 1 to 1000")?
                        .parse::<u32>()
                        .context("--fps requires an integer from 1 to 1000")?;
                    if !(1..=1000).contains(&fps) {
                        bail!("--fps must be from 1 to 1000");
                    }
                    options.fps = Some(fps);
                }
                "--theme" => {
                    options.theme = Some(Theme::parse(
                        flags
                            .next()
                            .context("--theme requires a name or theme file")?,
                    )?)
                }
                "--speed" => {
                    options.speed = PlaybackSpeed::parse(
                        flags
                            .next()
                            .context("--speed requires 1, 0.5, 0.25, or 0.1")?,
                    )?
                }
                "--debug" => options.debug = true,
                _ => bail!("unknown presentation option '{flag}'"),
            }
        }
        if options.benchmark && (options.speed != PlaybackSpeed::Normal || options.debug) {
            bail!("benchmarks require normal speed and no debug overlay");
        }
        Ok(options)
    }
}

fn preflight_slides(slides: Vec<SlidePlan>) -> Result<Vec<(String, super::preflight::Plan)>> {
    if slides.is_empty() {
        bail!("presentation needs at least one slide");
    }
    slides
        .into_iter()
        .map(|slide| {
            let input = super::preflight::Plan::new(slide.plan)?;
            input.require_native()?;
            Ok((slide.title, input))
        })
        .collect()
}

pub(super) fn run(slides: Vec<SlidePlan>, base: PathBuf, options: Options) -> Result<()> {
    let slides = preflight_slides(slides)?;
    let mut renderer = pollster::block_on(new_renderer(&slides[0].1.plan.id))?;
    let preferences = preferences::path();
    let saved = preferences
        .as_ref()
        .and_then(|path| match preferences::load(path) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("Could not load theme: {e:#}; using Original");
                None
            }
        })
        .unwrap_or_default();
    let theme = options.theme.unwrap_or(saved);
    renderer.set_theme(theme);
    renderer.set_interactive_preview(!options.full_quality);
    let mut prepared = Vec::new();
    let mut playbacks = Vec::new();
    for (title, input) in slides {
        let plan = PreparedPlan::prepare_preflight(input, &base, &mut renderer)?;
        let mut playback = plan.playback(options.reduced_motion)?;
        playback.set_speed(options.speed, Duration::ZERO);
        renderer.set_file_name(plan.file_name());
        // Warm immutable glyph/chrome resources before opening the window, not
        // at the first interactive visit to each slide.
        plan.render_sample_using(&mut renderer, 0.0, &playback.timeline())?;
        playbacks.push(SlidePlayback {
            grid: matches!(&plan.root, super::PreparedRoot::Grid(_)),
            title,
            playback,
            running: plan.running_properties(),
            resume_on_enter: false,
        });
        prepared.push(plan);
    }
    let event_loop = EventLoop::<RenderEvent>::with_user_event().build()?;
    let worker = RenderWorker::spawn(prepared, renderer, event_loop.create_proxy())?;
    let mut player = Player {
        slides: playbacks,
        slide_index: 0,
        worker,
        epoch: Instant::now(),
        window: None,
        front: None,
        scheduler: FrameScheduler::new(),
        title: String::new(),
        failure: None,
        filter: Filter::Smooth,
        benchmark: options.benchmark.then(|| Benchmark::new(&options)),
        front_timing: None,
        frame_interval: frame_interval(options.fps, None),
        refresh_millihertz: None,
        modifiers: ModifiersState::empty(),
        grid_palette: GridLinePalette::default(),
        theme,
        preferences,
        options,
    };
    eprintln!(
        "Native Psychopomp player: ⌘←/⌘→ or Shift+' / ' previous/next slide, ←/→ previous/next step, 1–9 choose slide, T/Shift+T next/previous theme (saved), S/Shift+S speed, ,/. previous/next frame (pauses), D debug overlay, R replay, Shift+R replay paused, Space/P pause or resume, Home/End first/last step, C/Shift+C next/previous grid line color, M reduced motion, X smooth/pixelated, F full screen, Esc close."
    );
    eprintln!(
        "Live single-sample preview. Export retains shutter sampling and audio; this player is silent."
    );
    event_loop.run_app(&mut player)?;
    match player.failure {
        Some(error) => Err(anyhow!(error)),
        None => Ok(()),
    }
}

struct WindowState {
    presenter: gpu::Presenter,
    window: Arc<Window>,
}

struct Player {
    slides: Vec<SlidePlayback>,
    slide_index: usize,
    worker: RenderWorker,
    epoch: Instant,
    window: Option<WindowState>,
    front: Option<Arc<Vec<u8>>>,
    scheduler: FrameScheduler,
    title: String,
    failure: Option<String>,
    filter: Filter,
    benchmark: Option<Benchmark>,
    front_timing: Option<(Instant, Duration)>,
    frame_interval: Duration,
    refresh_millihertz: Option<u32>,
    modifiers: ModifiersState,
    grid_palette: GridLinePalette,
    theme: Theme,
    preferences: Option<PathBuf>,
    options: Options,
}

struct SlidePlayback {
    grid: bool,
    title: String,
    playback: Playback,
    running: Vec<PropertyId>,
    resume_on_enter: bool,
}

impl SlidePlayback {
    fn sample(&mut self, now: Duration) -> PlaybackSample {
        let sample = self.playback.sample(now);
        let timeline = self.playback.timeline();
        let running = !self.playback.reduced_motion()
            && self.running.iter().any(|property| {
                timeline
                    .sample_at(property, sample.at_nanos as f64 / 1e9)
                    .is_some_and(|state| state.position > 0.001)
            });
        self.playback.keep_running(running, now);
        self.playback.sample(now)
    }

    fn leave(&mut self, now: Duration) {
        self.resume_on_enter = self.sample(now).phase == PlaybackPhase::Playing;
        self.playback.pause(now);
    }

    fn enter(&mut self, now: Duration) {
        if self.resume_on_enter && self.playback.sample(now).phase == PlaybackPhase::Paused {
            self.playback.command(PlaybackCommand::TogglePause, now);
        }
    }
}

impl Player {
    fn sample(&mut self) -> PlaybackSample {
        self.slides[self.slide_index].sample(self.epoch.elapsed())
    }

    fn select_slide(&mut self, index: usize) {
        if index >= self.slides.len() || index == self.slide_index {
            return;
        }
        let now = self.epoch.elapsed();
        self.slides[self.slide_index].leave(now);
        self.slide_index = index;
        self.slides[index].enter(now);
        self.scheduler.event(ScheduleEvent::SlideChanged);
        self.update_title();
        self.redraw();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.failure = Some(error.to_string());
        event_loop.exit();
    }

    fn redraw(&self) {
        if let Some(state) = &self.window {
            state.window.request_redraw();
        }
    }

    fn update_refresh_rate(&mut self) {
        let refresh = self
            .window
            .as_ref()
            .and_then(|state| state.window.current_monitor())
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .filter(|rate| *rate > 0);
        let interval = frame_interval(self.options.fps, refresh);
        if self.refresh_millihertz != refresh || self.frame_interval != interval {
            if self.benchmark.as_ref().is_some_and(Benchmark::measuring) {
                self.failure =
                    Some("display refresh changed during benchmark; rerun on one display".into());
            }
            self.refresh_millihertz = refresh;
            self.frame_interval = interval;
            eprintln!(
                "Native pacing: {:.3} fps sampling, {} display, FIFO synchronized presentation",
                1.0 / interval.as_secs_f64(),
                refresh.map_or_else(
                    || "unknown refresh (60 Hz fallback)".into(),
                    |rate| format!("{:.3} Hz", f64::from(rate) / 1000.)
                )
            );
            if let (Some(fps), Some(refresh)) = (self.options.fps, refresh)
                && fps * 1000 > refresh
            {
                eprintln!(
                    "Requested sampling exceeds display refresh; this is not a claim of {fps} visible frames/sec."
                );
            }
        }
    }

    fn update_title(&mut self) {
        let sample = self.sample();
        let slide = &self.slides[self.slide_index];
        let step = &slide.playback.steps()[sample.step_index];
        let mode = if slide.playback.reduced_motion() {
            "Reduced motion"
        } else {
            match sample.phase {
                PlaybackPhase::Held => "Holding",
                PlaybackPhase::Playing => "Playing",
                PlaybackPhase::Paused => "Paused",
            }
        };
        let title = format!(
            "Psychopomp · Slide {}/{}: {} · Step {}/{} · {} · {mode} · {} [S]{} · {:?} · Theme: {} [T]{}   ['  ← →  R  Space  X  F]",
            self.slide_index + 1,
            self.slides.len(),
            slide.title,
            sample.step_index + 1,
            slide.playback.steps().len(),
            step.title,
            self.options.speed.label(),
            if self.options.debug {
                " · Debug [D]"
            } else {
                ""
            },
            self.filter,
            self.theme.name(),
            if slide.grid {
                format!(
                    " · Lines: {} [C]",
                    effective_grid_palette(self.theme, self.grid_palette)
                        .map_or("Theme", GridLinePalette::name)
                )
            } else {
                String::new()
            }
        );
        if title != self.title {
            if let Some(state) = &self.window {
                state.window.set_title(&title);
            }
            self.title = title;
        }
    }

    fn command(&mut self, command: PlaybackCommand) {
        if self.slides[self.slide_index]
            .playback
            .command(command, self.epoch.elapsed())
        {
            self.update_title();
            eprintln!("{}", self.title);
            self.redraw();
        }
    }

    fn inspect(&mut self, action: InspectionAction) {
        let now = self.epoch.elapsed();
        let scheduling = match action {
            InspectionAction::Speed(reverse) => {
                self.options.speed = self.options.speed.cycle(reverse);
                for slide in &mut self.slides {
                    slide.playback.set_speed(self.options.speed, now);
                }
                ScheduleEvent::InspectionChanged
            }
            InspectionAction::Frame(backward) => {
                if !self.slides[self.slide_index]
                    .playback
                    .step_frame(backward, now)
                {
                    eprintln!("Disable reduced motion [M] to inspect transition frames");
                    return;
                }
                ScheduleEvent::InspectionChanged
            }
            InspectionAction::Overlay => {
                self.options.debug = !self.options.debug;
                ScheduleEvent::AppearanceChanged
            }
            InspectionAction::ReplayPaused => {
                let playback = &mut self.slides[self.slide_index].playback;
                playback.command(PlaybackCommand::Replay, now);
                playback.pause(now);
                ScheduleEvent::InspectionChanged
            }
        };
        self.scheduler.event(scheduling);
        self.update_title();
        eprintln!("{}", self.title);
        self.redraw();
    }

    fn request_frame(&mut self) -> Result<()> {
        if !self.scheduler.can_sample() {
            return Ok(());
        }
        let sample = self.sample();
        let stamp = RequestStamp {
            slide_index: self.slide_index,
            sample,
            palette: self.grid_palette,
            theme: self.theme,
            debug: self.options.debug,
        };
        let playback = &self.slides[self.slide_index].playback;
        let worker = &self.worker;
        self.scheduler
            .request(sample, self.frame_interval, Instant::now, || {
                worker.request(
                    stamp,
                    playback.timeline(),
                    stamp
                        .debug
                        .then(|| debug::DebugState::capture(playback, sample)),
                )
            })?;
        Ok(())
    }

    fn paint(&mut self) -> Result<()> {
        let Some(state) = &mut self.window else {
            return Ok(());
        };
        let size = state.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if self.scheduler.needs_paint() {
            match state.presenter.paint(
                self.front.as_ref(),
                self.filter,
                self.options.benchmark_gpu,
            )? {
                gpu::Paint::Presented(timing) => {
                    if let Some(benchmark) = &mut self.benchmark
                        && let Some((requested_at, render_time)) = self.front_timing
                    {
                        benchmark.frame(
                            Instant::now(),
                            self.slide_index,
                            [size.width, size.height],
                            timing,
                            render_time,
                            requested_at.elapsed(),
                        );
                    }
                    self.scheduler.event(ScheduleEvent::Presented);
                }
                gpu::Paint::Retry => {
                    self.scheduler.event(ScheduleEvent::Retry {
                        now: Instant::now(),
                        interval: self.frame_interval,
                    });
                }
                gpu::Paint::Occluded => {
                    if self.benchmark.as_ref().is_some_and(Benchmark::measuring) {
                        bail!("benchmark surface was occluded; rerun with it visible");
                    }
                    self.scheduler.event(ScheduleEvent::VisibilityChanged(true));
                }
            }
        }
        self.request_frame()
    }
}

impl ApplicationHandler<RenderEvent> for Player {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let created = (|| -> Result<WindowState> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Psychopomp · preparing scene")
                        .with_window_level(if self.benchmark.is_some() {
                            WindowLevel::AlwaysOnTop
                        } else {
                            WindowLevel::Normal
                        })
                        .with_inner_size(LogicalSize::new(1280., 720.))
                        .with_min_inner_size(LogicalSize::new(480., 270.)),
                )?,
            );
            let presenter = pollster::block_on(gpu::Presenter::new(window.clone()))?;
            Ok(WindowState { presenter, window })
        })();
        match created {
            Ok(state) => {
                self.window = Some(state);
                self.update_refresh_rate();
                self.scheduler.event(ScheduleEvent::Repaint);
                self.update_title();
                self.redraw();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: RenderEvent) {
        match event {
            RenderEvent::Frame {
                stamp,
                pixels,
                requested_at,
                render_time,
            } => {
                let slide_index = self.slide_index;
                let palette = self.grid_palette;
                let theme = self.theme;
                let debug = self.options.debug;
                let epoch = self.epoch;
                let slide = &mut self.slides[slide_index];
                if self.scheduler.complete(stamp, || RequestStamp {
                    slide_index,
                    sample: slide.sample(epoch.elapsed()),
                    palette,
                    theme,
                    debug,
                }) {
                    self.front = Some(pixels);
                    self.front_timing = Some((requested_at, render_time));
                }
                // Even discarded frames release the worker so the newest requested state can render.
                self.update_title();
                self.redraw();
            }
            RenderEvent::Failed(error) => self.fail(event_loop, error),
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if self.failure.is_some() {
            event_loop.exit();
            return;
        }
        if self.benchmark.as_ref().is_some_and(Benchmark::finished) {
            let benchmark = self.benchmark.take().expect("benchmark is running");
            if let Err(error) = benchmark.report(self.frame_interval, self.refresh_millihertz) {
                self.fail(event_loop, error);
            } else {
                event_loop.exit();
            }
            return;
        }
        if let Some(benchmark) = &self.benchmark {
            self.select_slide(benchmark.slide_index(self.slides.len()));
        }
        if let Some(benchmark) = &mut self.benchmark
            && let Some(command) = benchmark.command()
        {
            self.command(command);
        }
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let sample = self.sample();
        let drawable = self.window.as_ref().is_some_and(|state| {
            let size = state.window.inner_size();
            size.width > 0 && size.height > 0
        });
        let frame_deadline = self.scheduler.wake_at(sample, drawable, Instant::now);
        let deadline = frame_deadline
            .into_iter()
            .chain(self.benchmark.as_ref().map(Benchmark::deadline))
            .min();
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_none_or(|state| state.window.id() != window_id)
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.benchmark.is_some() {
                    self.fail(event_loop, "benchmark cancelled");
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.paint() {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.update_refresh_rate();
                self.scheduler.event(ScheduleEvent::Repaint);
                self.redraw();
            }
            WindowEvent::Moved(_) | WindowEvent::Focused(true) => self.update_refresh_rate(),
            WindowEvent::Focused(false) => {
                self.modifiers = ModifiersState::empty();
                if self.benchmark.is_none() {
                    self.slides[self.slide_index]
                        .playback
                        .pause(self.epoch.elapsed());
                }
                self.update_title();
                self.redraw();
            }
            WindowEvent::Occluded(occluded) => {
                if occluded && self.benchmark.as_ref().is_some_and(Benchmark::measuring) {
                    self.fail(
                        event_loop,
                        "benchmark window was occluded; rerun with it visible",
                    );
                    return;
                }
                self.scheduler
                    .event(ScheduleEvent::VisibilityChanged(occluded));
                if occluded {
                    self.slides[self.slide_index]
                        .playback
                        .pause(self.epoch.elapsed());
                } else {
                    self.redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if self.benchmark.is_some() {
                    if event.logical_key == Key::Named(NamedKey::Escape) {
                        self.fail(event_loop, "benchmark cancelled");
                    }
                    return;
                }
                if let Some(navigation) = arrow_navigation(&event.logical_key, self.modifiers) {
                    match navigation {
                        ArrowNavigation::Step(command) => self.command(command),
                        ArrowNavigation::Slide(offset) => self.select_slide(
                            (self.slide_index as isize + offset)
                                .rem_euclid(self.slides.len() as isize)
                                as usize,
                        ),
                    }
                    return;
                }
                if let Some(action) = inspection_action(&event.logical_key, self.modifiers) {
                    self.inspect(action);
                    return;
                }
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::Home) => self.command(PlaybackCommand::First),
                    Key::Named(NamedKey::End) => self.command(PlaybackCommand::Last),
                    Key::Named(NamedKey::Space) => {
                        let command = if self.sample().phase == PlaybackPhase::Held {
                            PlaybackCommand::Next
                        } else {
                            PlaybackCommand::TogglePause
                        };
                        self.command(command);
                    }
                    Key::Character(key) => match key.to_lowercase().as_str() {
                        "t" if !self.modifiers.intersects(
                            ModifiersState::SUPER | ModifiersState::CONTROL | ModifiersState::ALT,
                        ) =>
                        {
                            self.theme = self.theme.cycle(self.modifiers.shift_key());
                            self.grid_palette = GridLinePalette::default();
                            if let Some(path) = &self.preferences {
                                if let Err(e) = preferences::save(path, self.theme) {
                                    eprintln!("Theme changed, but could not save it: {e:#}");
                                }
                            } else {
                                eprintln!(
                                    "Theme changed, but no configuration directory is available to save it"
                                );
                            }
                            self.scheduler.event(ScheduleEvent::AppearanceChanged);
                            self.update_title();
                            eprintln!("{}", self.title);
                            self.redraw();
                        }
                        "c" if self.slides[self.slide_index].grid
                            && !self.modifiers.intersects(
                                ModifiersState::SUPER
                                    | ModifiersState::CONTROL
                                    | ModifiersState::ALT,
                            ) =>
                        {
                            self.grid_palette = self.grid_palette.cycle(self.modifiers.shift_key());
                            // Force a fresh held/paused sample without touching the
                            // Playback clock, step, or current motion trajectories.
                            self.scheduler.event(ScheduleEvent::AppearanceChanged);
                            self.update_title();
                            eprintln!("{}", self.title);
                            self.redraw();
                        }
                        "'" => self.select_slide((self.slide_index + 1) % self.slides.len()),
                        "\"" => self.select_slide(
                            (self.slide_index + self.slides.len() - 1) % self.slides.len(),
                        ),
                        digit
                            if digit.len() == 1
                                && digit.as_bytes()[0].is_ascii_digit()
                                && digit != "0" =>
                        {
                            self.select_slide((digit.as_bytes()[0] - b'1') as usize)
                        }
                        "r" => self.command(PlaybackCommand::Replay),
                        "p" => self.command(PlaybackCommand::TogglePause),
                        "x" => {
                            self.filter = match self.filter {
                                Filter::Smooth => Filter::Pixelated,
                                Filter::Pixelated => Filter::Smooth,
                            };
                            self.scheduler.event(ScheduleEvent::Repaint);
                            self.update_title();
                            self.redraw();
                        }
                        "m" => {
                            let enabled = !self.slides[self.slide_index].playback.reduced_motion();
                            self.slides[self.slide_index]
                                .playback
                                .set_reduced_motion(enabled, self.epoch.elapsed());
                            self.update_title();
                            self.redraw();
                        }
                        "f" => {
                            let window = &self.window.as_ref().expect("event window exists").window;
                            window.set_fullscreen(if window.fullscreen().is_some() {
                                None
                            } else {
                                Some(Fullscreen::Borderless(None))
                            });
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Native pacing is independent of the authored video FPS. Unknown/zero refresh
/// falls back to 60 Hz; millihertz preserves fractional rates such as 59.94 Hz.
/// The grid line palette that overrides the recipe's colors, if any: other
/// themes keep their own accent until the audition leaves Orange.
fn effective_grid_palette(theme: Theme, palette: GridLinePalette) -> Option<GridLinePalette> {
    (theme == Theme::Original || palette != GridLinePalette::Orange).then_some(palette)
}

fn frame_interval(fps: Option<u32>, refresh_millihertz: Option<u32>) -> Duration {
    let rate = fps
        .filter(|fps| *fps > 0)
        .map(|fps| u64::from(fps) * 1000)
        .or_else(|| refresh_millihertz.filter(|rate| *rate > 0).map(u64::from))
        .unwrap_or(60_000);
    Duration::from_nanos(1_000_000_000_000 / rate)
}

#[derive(Debug, PartialEq)]
enum ArrowNavigation {
    Step(PlaybackCommand),
    Slide(isize),
}

#[derive(Debug, PartialEq)]
enum InspectionAction {
    Speed(bool),
    Frame(bool),
    Overlay,
    ReplayPaused,
}

fn inspection_action(key: &Key, modifiers: ModifiersState) -> Option<InspectionAction> {
    if modifiers.intersects(ModifiersState::SUPER | ModifiersState::CONTROL | ModifiersState::ALT) {
        return None;
    }
    let Key::Character(key) = key else {
        return None;
    };
    match key.to_lowercase().as_str() {
        "s" => Some(InspectionAction::Speed(modifiers.shift_key())),
        "d" => Some(InspectionAction::Overlay),
        "," => Some(InspectionAction::Frame(true)),
        "." => Some(InspectionAction::Frame(false)),
        "r" if modifiers.shift_key() => Some(InspectionAction::ReplayPaused),
        _ => None,
    }
}

fn arrow_navigation(key: &Key, modifiers: ModifiersState) -> Option<ArrowNavigation> {
    let (direction, command) = match key {
        Key::Named(NamedKey::ArrowRight) => (1, PlaybackCommand::Next),
        Key::Named(NamedKey::ArrowLeft) => (-1, PlaybackCommand::Previous),
        _ => return None,
    };
    Some(if modifiers.super_key() {
        ArrowNavigation::Slide(direction)
    } else {
        ArrowNavigation::Step(command)
    })
}

/// Letterbox an authored frame instead of reflowing its layout on resize.
fn viewport([width, height]: [u32; 2]) -> [u32; 4] {
    if width == 0 || height == 0 {
        return [0; 4];
    }
    let scale = (width as f64 / f64::from(WIDTH)).min(height as f64 / f64::from(HEIGHT));
    let w = (f64::from(WIDTH) * scale).round().max(1.0) as u32;
    let h = (f64::from(HEIGHT) * scale).round().max(1.0) as u32;
    [(width - w) / 2, (height - h) / 2, w, h]
}

#[derive(Clone, Copy, Debug)]
enum Filter {
    Smooth,
    Pixelated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_recipe_gate_accepts_a_grid_scene() {
        let mut slides = vec![SlidePlan {
            title: "Grid".into(),
            plan: psychopomp_keyed_grid::build_deck().unwrap().slides[0]
                .plan
                .clone(),
        }];
        preflight_slides(slides.clone()).unwrap();
        slides[0].plan.actors[0].recipe = "unsupported-recipe".into();
        assert!(preflight_slides(slides).is_err());
    }

    #[test]
    fn command_arrows_switch_slides_and_plain_arrows_keep_steps() {
        for (key, direction, command) in [
            (NamedKey::ArrowLeft, -1, PlaybackCommand::Previous),
            (NamedKey::ArrowRight, 1, PlaybackCommand::Next),
        ] {
            assert_eq!(
                arrow_navigation(&Key::Named(key), ModifiersState::SUPER),
                Some(ArrowNavigation::Slide(direction))
            );
            assert_eq!(
                arrow_navigation(&Key::Named(key), ModifiersState::empty()),
                Some(ArrowNavigation::Step(command))
            );
            assert_eq!(
                arrow_navigation(&Key::Named(key), ModifiersState::CONTROL),
                Some(ArrowNavigation::Step(command))
            );
        }
        assert_eq!(
            arrow_navigation(&Key::Named(NamedKey::Home), ModifiersState::SUPER),
            None
        );
    }

    #[test]
    fn inspection_keys_are_scoped() {
        let key = |s: &str| Key::Character(s.into());
        for (text, modifiers, expected) in [
            (
                "s",
                ModifiersState::empty(),
                Some(InspectionAction::Speed(false)),
            ),
            (
                "S",
                ModifiersState::SHIFT,
                Some(InspectionAction::Speed(true)),
            ),
            (
                "d",
                ModifiersState::empty(),
                Some(InspectionAction::Overlay),
            ),
            (
                ".",
                ModifiersState::empty(),
                Some(InspectionAction::Frame(false)),
            ),
            (
                ",",
                ModifiersState::empty(),
                Some(InspectionAction::Frame(true)),
            ),
            (
                "R",
                ModifiersState::SHIFT,
                Some(InspectionAction::ReplayPaused),
            ),
            ("r", ModifiersState::empty(), None),
            ("s", ModifiersState::SUPER, None),
            ("d", ModifiersState::CONTROL, None),
        ] {
            assert_eq!(inspection_action(&key(text), modifiers), expected);
        }
    }

    fn slide(ambient: bool) -> SlidePlayback {
        use psychopomp::{
            author::PlanBuilder,
            timeline::{SpringProfile, TimedEvent, Timeline},
        };
        let mut plan = PlanBuilder::new("clock", 4_000_000_000);
        let actor = plan.actor("node", "effect-task", ()).unwrap();
        let x = plan.continuous(&actor, "x", 0.);
        let activity = plan.continuous(&actor, "activity", if ambient { 1. } else { 0. });
        plan.spring(&x, 1_000_000_000, 1., 0.4, 0.);
        plan.presentation_step("initial", "Initial", 0, 0);
        plan.presentation_step("next", "Next", 1_000_000_000, 3_000_000_000);
        let x = PropertyId::new(x.id());
        let activity = PropertyId::new(activity.id());
        let timeline = Timeline::compile_events(
            [
                (x.clone(), 0.),
                (activity.clone(), if ambient { 1. } else { 0. }),
            ],
            [TimedEvent::spring(
                1.,
                x,
                1.,
                SpringProfile::from_visual_duration(0.4, 0., 0.001, 0.001),
            )],
            4.,
        )
        .unwrap();
        SlidePlayback {
            grid: false,
            title: "test".into(),
            playback: Playback::new(&plan.finish().unwrap(), &timeline, false).unwrap(),
            running: vec![activity],
            resume_on_enter: false,
        }
    }

    #[test]
    fn slide_switch_preserves_step_and_pauses_its_clock_until_return() {
        let mut slide = slide(false);
        slide
            .playback
            .command(PlaybackCommand::Next, Duration::ZERO);
        slide.leave(Duration::from_millis(150));
        let saved = slide.sample(Duration::from_secs(100));
        assert_eq!(saved.at_nanos, 150_000_000);
        assert_eq!(saved.step_index, 1);
        slide.enter(Duration::from_secs(100));
        assert_eq!(
            slide.sample(Duration::from_secs(100)).at_nanos,
            saved.at_nanos
        );
        assert_eq!(
            slide.sample(Duration::from_secs(100)).phase,
            PlaybackPhase::Playing
        );
        slide.playback.pause(Duration::from_millis(100_100));
        slide.leave(Duration::from_secs(101));
        slide.enter(Duration::from_secs(200));
        assert_eq!(
            slide.sample(Duration::from_secs(200)).phase,
            PlaybackPhase::Paused
        );
    }

    #[test]
    fn ambient_task_motion_runs_while_held_but_pause_and_reduced_motion_stop_it() {
        let mut slide = slide(true);
        assert_eq!(slide.sample(Duration::ZERO).phase, PlaybackPhase::Playing);
        assert_eq!(slide.sample(Duration::from_secs(2)).at_nanos, 2_000_000_000);
        slide.playback.pause(Duration::from_secs(2));
        assert_eq!(slide.sample(Duration::from_secs(9)).at_nanos, 2_000_000_000);
        slide
            .playback
            .set_reduced_motion(true, Duration::from_secs(9));
        let held = slide.sample(Duration::from_secs(9));
        assert_eq!(held.phase, PlaybackPhase::Held);
        assert_eq!(held, slide.sample(Duration::from_secs(20)));
    }

    #[test]
    fn pacing_follows_high_and_fractional_refresh_rates() {
        assert_eq!(
            frame_interval(None, Some(120_000)),
            Duration::from_nanos(8_333_333)
        );
        assert_eq!(
            frame_interval(None, Some(144_000)),
            Duration::from_nanos(6_944_444)
        );
        assert_eq!(
            frame_interval(None, Some(59_940)),
            Duration::from_nanos(16_683_350)
        );
        assert_eq!(frame_interval(None, Some(0)), frame_interval(None, None));
        assert_eq!(frame_interval(None, None), Duration::from_nanos(16_666_666));
        assert_eq!(
            frame_interval(Some(120), Some(60_000)),
            frame_interval(None, Some(120_000))
        );
    }

    #[test]
    fn presentation_options_validate_explicit_fps() {
        let parse = |flags: &[&str]| {
            Options::parse(
                &flags
                    .iter()
                    .map(|flag| flag.to_string())
                    .collect::<Vec<_>>(),
            )
        };
        for flags in [
            &["--fps"][..],
            &["--fps", "0"],
            &["--fps", "1001"],
            &["--fps", "NaN"],
            &["--fps", "60.5"],
            &["--other"],
            &["--speed"],
            &["--speed", "0"],
            &["--speed", "NaN"],
            &["--benchmark", "--speed", "0.25"],
            &["--benchmark", "--debug"],
        ] {
            assert!(parse(flags).is_err());
        }
        let options = parse(&[
            "--fps",
            "120",
            "--benchmark-gpu",
            "--full-quality",
            "--reduced-motion",
        ])
        .unwrap();
        assert_eq!(options.fps, Some(120));
        assert!(
            options.benchmark
                && options.benchmark_gpu
                && options.full_quality
                && options.reduced_motion
        );
        assert!(parse(&[]).unwrap().fps.is_none());
        assert!(!parse(&["--benchmark"]).unwrap().benchmark_gpu);
        assert_eq!(
            parse(&["--speed", "0.25", "--debug"]).unwrap().speed,
            PlaybackSpeed::Quarter
        );
        assert!(parse(&["--debug"]).unwrap().debug);
    }

    #[test]
    fn resizing_preserves_the_authored_aspect_ratio() {
        assert_eq!(viewport([1920, 1080]), [0, 0, 1920, 1080]);
        assert_eq!(viewport([1280, 800]), [0, 40, 1280, 720]);
        assert_eq!(viewport([2000, 900]), [200, 0, 1600, 900]);
        assert_eq!(viewport([0, 0]), [0, 0, 0, 0]);
    }
}
