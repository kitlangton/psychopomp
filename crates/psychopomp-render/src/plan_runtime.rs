use std::{
    collections::HashMap,
    fs,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use psychopomp::{
    anchor::{AnchorPlan, AnchorTarget},
    composition::{Asset, Duration, MediaPlacement, MediaRole, Time, TimeRange},
    math::{Vec2, shapes::Box2},
    plan::{
        DeckPlan, MediaKindPlan, MediaRolePlan, ReadPlanError, ReelPlan, ScalarPlan, ScenePlan,
        TargetComponentPlan,
    },
    state::{StateTrack, TimedState},
    timeline::{PropertyId, Timeline},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    exposure::{HEIGHT, WIDTH},
    render::{HeadlessRenderer, RenderSpec, Theme},
};

mod anchor;
mod attachments;
mod callout;
mod caption;
mod changed_files;
mod chat;
mod component_prototype;
pub(crate) mod delivery;
mod editor;
mod footage;
mod generated;
mod grid;
mod header;
mod ide;
mod lanes;
mod lens;
mod lower_third;
mod plot;
mod preflight;
mod presentation;
#[cfg(test)]
mod proof;
mod reel;
mod render_queue;
mod rich_text;
mod rolling;
mod sequence;
#[cfg(test)]
mod stability_tests;
mod stage;
mod still;
mod task;
mod terminal;
mod tree;
mod value;
mod venn;
pub(crate) mod verify;
mod viz;

use editor::PreparedEditor;

/// Shutter samples per Stage frame: enough that a fast ember draws a
/// continuous streak rather than a row of copies.
const STAGE_TEMPORAL_SAMPLES: u32 = 24;
const BUILTIN_HERO_PLAN: &str = include_str!("../../../scenes/hero/hero.plan.json");

/// Measured placement of a semantic code target within the editor body.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TargetGeometry {
    pub x: f32,
    pub width: f32,
    pub line_y: f32,
}

impl TargetGeometry {
    pub fn center_x(self) -> f32 {
        self.x + self.width * 0.5
    }
}

pub(crate) async fn render_builtin_hero(output: &Path) -> Result<()> {
    let plan = ScenePlan::from_json(BUILTIN_HERO_PLAN)?;
    let window = plan_window(&plan, WindowSelection::Full)?;
    let (loaded, mut renderer) =
        still::Loaded::prepare(PlanFile::Plan(plan), Path::new("."), Theme::Original).await?;
    loaded.render_video(&mut renderer, output, window)
}

const FRAME_USAGE: &str = "psychopomp plan frame <plan-or-reel.json> <seconds> [output.png] [--shutter] [--theme NAME | --theme FILE.json]";
const SNAPSHOT_USAGE: &str = "psychopomp plan snapshot <plan-or-reel.json> <a,b,c | from:to:step> <dir> [--compare] [--shutter] [--theme NAME | --theme FILE.json]";
const RENDER_USAGE: &str = "psychopomp plan render <plan-or-reel.json> [output] [--cue ID | --range START..END] [--theme NAME | --theme FILE.json]";
const PRESENT_USAGE: &str = "psychopomp plan present <plan.json> [--theme NAME | --theme FILE.json] [--speed 1|0.5|0.25|0.1] [--debug] [--reduced-motion] [--full-quality] [--fps FPS] [--benchmark | --benchmark-gpu]";

fn usage() -> String {
    [
        "psychopomp plan serve",
        "psychopomp plan schema",
        "psychopomp plan validate <plan.json>",
        "psychopomp plan inspect <plan.json>",
        "psychopomp plan steps <plan.json>",
        "psychopomp plan diff <before.json> <after.json>",
        FRAME_USAGE,
        SNAPSHOT_USAGE,
        RENDER_USAGE,
        PRESENT_USAGE,
    ]
    .join(" | ")
}

pub(crate) fn command(arguments: &[String]) -> Result<()> {
    let Some((command, arguments)) = arguments.split_first() else {
        bail!("usage: {}", usage());
    };
    match (command.as_str(), arguments) {
        ("render", arguments) => {
            let (arguments, theme) = delivery_theme(arguments)?;
            render_command(&arguments, theme)
        }
        ("frame", arguments) => {
            let (arguments, theme) = delivery_theme(arguments)?;
            frame_command(&arguments, theme)
        }
        ("snapshot", arguments) => {
            let (arguments, theme) = delivery_theme(arguments)?;
            let (arguments, flags) = flags(&arguments, &["--compare", "--shutter"]);
            let [path, times, dir] = arguments.as_slice() else {
                bail!("usage: {SNAPSHOT_USAGE}");
            };
            still::snapshot(
                Path::new(path),
                &still::parse_times(times)?,
                Path::new(dir),
                flags[0],
                flags[1],
                theme,
            )
        }
        ("present", [path, flags @ ..]) => {
            let options = presentation::Options::parse(flags)?;
            let slides = match PlanFile::read(Path::new(path))? {
                PlanFile::Deck(deck) => deck.slides,
                PlanFile::Plan(plan) => vec![psychopomp::plan::SlidePlan {
                    title: plan.id.clone(),
                    plan,
                }],
                PlanFile::Reel(_) => bail!("plan present takes a scene plan or deck, not a reel"),
            };
            let base = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
            presentation::run(slides, base.to_owned(), options)
        }
        ("present", _) => bail!("usage: {PRESENT_USAGE}"),
        ("serve", []) => pollster::block_on(serve()),
        ("schema", []) => {
            println!("{}", serde_json::to_string_pretty(&ScenePlan::schema())?);
            Ok(())
        }
        ("validate", [path]) => {
            match PlanFile::read(Path::new(path))? {
                PlanFile::Plan(plan) => validate_renderer_plan(&plan)?,
                PlanFile::Reel(reel) => reel::validate(&reel)?,
                PlanFile::Deck(_) => bail!(DECK_UNSUPPORTED),
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "valid": true,
                    "path": path,
                }))?
            );
            Ok(())
        }
        ("inspect", [path]) => {
            let report = match PlanFile::read(Path::new(path))? {
                PlanFile::Plan(plan) => inspect_plan(&plan),
                PlanFile::Reel(reel) => reel::inspect(&reel),
                PlanFile::Deck(_) => bail!(DECK_UNSUPPORTED),
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        ("steps", [path]) => {
            let plan = read_plan(Path::new(path))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&psychopomp::editor::inspect_steps(&plan)?)?
            );
            Ok(())
        }
        ("diff", [before, after]) => {
            let before = read_plan(Path::new(before))?;
            let after = read_plan(Path::new(after))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({ "changes": before.diff(&after)? }))?
            );
            Ok(())
        }
        _ => bail!("usage: {}", usage()),
    }
}

fn delivery_theme(arguments: &[String]) -> Result<(Vec<String>, Theme)> {
    let mut args = Vec::new();
    let mut theme = None;
    let mut iter = arguments.iter();
    while let Some(arg) = iter.next() {
        if arg == "--theme" {
            anyhow::ensure!(theme.is_none(), "--theme may be specified once");
            theme = Some(Theme::parse(
                iter.next()
                    .context("--theme requires a name or theme file")?,
            )?);
        } else {
            args.push(arg.clone());
        }
    }
    Ok((args, theme.unwrap_or_default()))
}

/// Remove boolean `names` from `arguments`, reporting which were present.
fn flags<const N: usize>(arguments: &[String], names: &[&str; N]) -> (Vec<String>, [bool; N]) {
    let present = names.map(|name| arguments.iter().any(|argument| argument == name));
    let rest = arguments
        .iter()
        .filter(|argument| !names.contains(&argument.as_str()))
        .cloned()
        .collect();
    (rest, present)
}

fn frame_command(arguments: &[String], theme: Theme) -> Result<()> {
    let (arguments, [shutter]) = flags(arguments, &["--shutter"]);
    let [plan, seconds, rest @ ..] = arguments.as_slice() else {
        bail!("usage: {FRAME_USAGE}");
    };
    if rest.len() > 1 {
        bail!("usage: {FRAME_USAGE}");
    }
    let seconds = seconds
        .parse::<f64>()
        .context("parse frame time in seconds")?;
    Time::try_seconds(seconds)
        .context("frame time must be finite, non-negative, and representable")?;
    let output = rest
        .first()
        .map_or_else(|| PathBuf::from("output/scene-plan.png"), PathBuf::from);
    create_output_directory(&output)?;
    let (loaded, mut renderer) = pollster::block_on(still::Loaded::load(Path::new(plan), theme))?;
    delivery::write_png(&output, &loaded.still(&mut renderer, seconds, shutter)?)
}

fn render_command(arguments: &[String], theme: Theme) -> Result<()> {
    let Some(path) = arguments.first() else {
        bail!("plan render requires a plan path");
    };
    let mut cursor = 1;
    let output = if arguments
        .get(cursor)
        .is_some_and(|argument| !argument.starts_with("--"))
    {
        let output = PathBuf::from(&arguments[cursor]);
        cursor += 1;
        output
    } else {
        PathBuf::from("output/scene-plan.mp4")
    };
    let selection = match arguments.get(cursor).map(String::as_str) {
        None => WindowSelection::Full,
        Some("--cue") => {
            let cue = arguments
                .get(cursor + 1)
                .context("--cue requires a cue ID")?
                .clone();
            cursor += 2;
            WindowSelection::Cue(cue)
        }
        Some("--range") => {
            let range = arguments
                .get(cursor + 1)
                .context("--range requires START..END seconds")?;
            cursor += 2;
            WindowSelection::Range(parse_range(range)?)
        }
        Some(argument) => bail!("unknown plan render option '{argument}'"),
    };
    if cursor != arguments.len() {
        bail!("unexpected plan render arguments");
    }
    create_output_directory(&output)?;
    let path = Path::new(path);
    let file = PlanFile::read(path)?;
    let window = file.window(selection)?;
    let _slot = render_queue::wait()?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let (loaded, mut renderer) = pollster::block_on(still::Loaded::prepare(file, base, theme))?;
    loaded.render_video(&mut renderer, &output, window)
}

fn create_output_directory(output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }
    Ok(())
}

enum WindowSelection {
    Full,
    Cue(String),
    Range(TimeRange),
}

fn parse_range(value: &str) -> Result<TimeRange> {
    let (start, end) = value
        .split_once("..")
        .context("render range must use START..END seconds")?;
    let start = Time::try_seconds(start.parse::<f64>().context("parse render range start")?)
        .context("render range start must be finite, non-negative, and representable")?;
    let end = Time::try_seconds(end.parse::<f64>().context("parse render range end")?)
        .context("render range end must be finite, non-negative, and representable")?;
    if end <= start {
        bail!("render range must have positive duration");
    }
    Ok(TimeRange::new(start, end))
}

fn plan_window(plan: &ScenePlan, selection: WindowSelection) -> Result<TimeRange> {
    Ok(match selection {
        WindowSelection::Full => TimeRange::new(Time::ZERO, Time::from_nanos(plan.duration_nanos)),
        WindowSelection::Cue(id) => {
            let cue = plan
                .cues
                .iter()
                .find(|cue| cue.id == id)
                .with_context(|| format!("scene plan has no cue '{id}'"))?;
            TimeRange::new(
                Time::from_nanos(cue.start_nanos),
                Time::from_nanos(cue.end_nanos),
            )
        }
        WindowSelection::Range(range) => range,
    })
}

const DECK_UNSUPPORTED: &str =
    "decks are presented with `psychopomp plan present`; this command takes a scene plan or reel";

/// A plan file, recognized by its top-level key: reels have `segments`, decks
/// have `slides`.
enum PlanFile {
    Plan(ScenePlan),
    Reel(ReelPlan),
    Deck(DeckPlan),
}

impl PlanFile {
    fn read(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)
            .with_context(|| format!("read plan file {}", path.display()))?;
        let value: Value = serde_json::from_str(&json)
            .with_context(|| format!("parse JSON {}", path.display()))?;
        if value.get("segments").is_some() {
            let reel: ReelPlan = serde_json::from_str(&json)
                .with_context(|| format!("parse reel {}", path.display()))?;
            reel.validate()?;
            Ok(Self::Reel(reel))
        } else if value.get("slides").is_some() {
            let deck: DeckPlan = serde_json::from_value(value)?;
            deck.validate()?;
            Ok(Self::Deck(deck))
        } else {
            plan_from_json(&json).map(Self::Plan)
        }
    }

    /// `--cue` on a reel selects one whole segment by its scene ID.
    fn window(&self, selection: WindowSelection) -> Result<TimeRange> {
        match (self, selection) {
            (Self::Plan(plan), selection) => plan_window(plan, selection),
            (Self::Reel(reel), WindowSelection::Full) => Ok(TimeRange::new(
                Time::ZERO,
                Time::from_nanos(reel.duration_nanos()),
            )),
            (Self::Reel(reel), WindowSelection::Cue(id)) => reel::segment_window(reel, &id),
            (Self::Reel(_), WindowSelection::Range(range)) => Ok(range),
            (Self::Deck(_), _) => bail!(DECK_UNSUPPORTED),
        }
    }
}

fn read_plan(path: &Path) -> Result<ScenePlan> {
    let json =
        fs::read_to_string(path).with_context(|| format!("read scene plan {}", path.display()))?;
    plan_from_json(&json)
}

fn plan_from_json(json: &str) -> Result<ScenePlan> {
    match ScenePlan::from_json(json) {
        Ok(plan) => Ok(plan),
        Err(ReadPlanError::Validation(error)) => {
            eprintln!("{}", serde_json::to_string_pretty(error.diagnostics())?);
            Err(error.into())
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) async fn new_renderer(file_name: &str) -> Result<HeadlessRenderer> {
    HeadlessRenderer::new(RenderSpec {
        width: WIDTH,
        height: HEIGHT,
        file_name: file_name.to_owned(),
    })
    .await
}

/// Pure compiled data is also the test seam for scalar/media compilation. It is
/// not a partially initialized renderer and cannot render pixels.
struct CompiledPlan {
    plan: ScenePlan,
    timeline: Timeline,
    properties: HashMap<String, PropertyId>,
    state_tracks: HashMap<String, StateTrack<serde_json::Value>>,
    media: Vec<MediaPlacement>,
}

enum PreparedRoot {
    Blank,
    Title(preflight::Title),
    Editor {
        editor: Box<PreparedEditor>,
        pointer: Option<String>,
    },
    Grid(Box<grid::PreparedGrid>),
    Stage(Box<stage::PreparedStage>),
}

struct PreparedPlan {
    compiled: CompiledPlan,
    root: PreparedRoot,
    native: bool,
    texts: Vec<preflight::PlainText>,
    attachments: Vec<attachments::Attachment>,
    tasks: Vec<task::PreparedTask>,
    value_tokens: Vec<value::PreparedValueToken>,
    components: component_prototype::PreparedComponents,
    rich_text: Vec<rich_text::PreparedRichText>,
    venn: Vec<venn::PreparedVenn>,
    sequences: Vec<sequence::PreparedSequence>,
    captions: Vec<caption::PreparedCaption>,
    rolling: Vec<rolling::PreparedRollingNumber>,
    trees: Vec<tree::PreparedTree>,
    plots: Vec<plot::PreparedPlot>,
    lanes: Vec<lanes::PreparedLanes>,
    callouts: Vec<callout::PreparedCallout>,
    lenses: Vec<lens::PreparedLens>,
    headers: Vec<header::PreparedHeader>,
    /// Video Cards, images, footage overlays, and Stage footage, with the
    /// store their frames come from.
    footage: footage::Footage,
    terminals: Vec<terminal::PreparedTerminal>,
    chats: Vec<chat::PreparedChat>,
    changed_files: Vec<changed_files::PreparedChangedFiles>,
    lower_thirds: Vec<lower_third::PreparedLowerThird>,
    viz: viz::PreparedViz,
}

// A prepared scene exposes a read-only view of its compiled data. There is no
// DerefMut: changing a plan requires fresh preflight and resource preparation.
impl std::ops::Deref for PreparedPlan {
    type Target = CompiledPlan;
    fn deref(&self) -> &Self::Target {
        &self.compiled
    }
}

#[derive(Debug, PartialEq)]
struct VisualSampleKey {
    motion: Vec<[u32; 4]>,
    states: Vec<Value>,
    video_frames: Vec<u64>,
    ambient_time: Option<u64>,
    /// Stage-pinned callout and lens anchors and pinned overlay origins, which
    /// move with the camera.
    anchors: Vec<[u32; 2]>,
}

impl PreparedPlan {
    fn playback(&self, reduced_motion: bool) -> Result<psychopomp::playback::Playback> {
        if !self.native {
            bail!(preflight::NATIVE_UNSUPPORTED);
        }
        let defaults = self
            .attachments
            .iter()
            .map(|attachment| (attachment.weight.clone(), attachment.default_profile))
            .collect();
        let mut delays = HashMap::new();
        for header in &self.headers {
            header.delays(&mut delays);
        }
        psychopomp::playback::Playback::with_start_delays(
            &self.plan,
            &self.timeline,
            reduced_motion,
            &defaults,
            &delays,
        )
    }
    fn prepare(plan: ScenePlan, base: &Path, renderer: &mut HeadlessRenderer) -> Result<Self> {
        Self::prepare_preflight(preflight::Plan::new(plan)?, base, renderer)
    }

    fn prepare_preflight(
        input: preflight::Plan,
        base: &Path,
        renderer: &mut HeadlessRenderer,
    ) -> Result<Self> {
        let native = input.native();
        let preflight::Plan {
            mut plan,
            mut root,
            texts,
            tasks,
            components,
            rich_text,
            headers,
            value_tokens,
            venn,
            sequences,
            captions,
            rolling,
            trees,
            plots,
            lanes,
            footage,
            callouts,
            terminals,
            chats,
            changed_files,
            lower_thirds,
            viz,
            lenses,
        } = input;
        let components = component_prototype::PreparedComponents::prepare_inputs(
            &mut plan, components, renderer,
        )?;
        let tasks = tasks
            .into_iter()
            .map(|(id, recipe)| task::PreparedTask::from_recipe(id, recipe, renderer))
            .collect::<Vec<_>>();
        for task in &tasks {
            generated::extend(&mut plan, task.channels(), generated::Owner::Task)?;
        }
        let mut targets = HashMap::new();
        let mut scales = HashMap::new();
        if let preflight::RootPlan::Editor {
            editor, selectors, ..
        } = &mut root
        {
            for (id, selector) in selectors {
                targets.insert(
                    id.clone(),
                    editor.resolve_selection(renderer, id, selector)?,
                );
                scales.insert(
                    id.clone(),
                    editor.target_scale(id).expect("resolved target"),
                );
            }
        }
        let attachments = attachments::compile(&mut plan, &targets, &scales)?;
        plan.validate()?;
        let compiled = CompiledPlan::new(plan, base, &targets)?;
        let rich_text = rich_text
            .into_iter()
            .map(|(id, source)| rich_text::PreparedRichText::from_source(id, source, renderer))
            .collect::<Result<Vec<_>>>()?;
        let headers = headers
            .into_iter()
            .map(|(id, recipe)| header::PreparedHeader::from_recipe(id, recipe, renderer))
            .collect::<Result<Vec<_>>>()?;
        let mut sequences = sequences;
        for sequence in &mut sequences {
            sequence.measure(renderer);
        }
        let rolling = rolling
            .into_iter()
            .map(|input| input.prepare(renderer))
            .collect();
        let footage = footage.open(base)?;
        let chats = chats
            .into_iter()
            .map(|input| input.prepare(renderer))
            .collect();
        let changed_files = changed_files
            .into_iter()
            .map(|input| input.prepare(renderer))
            .collect();
        let viz = viz.prepare(renderer);
        let root = match root {
            preflight::RootPlan::Blank => PreparedRoot::Blank,
            preflight::RootPlan::Title(title) => PreparedRoot::Title(title),
            preflight::RootPlan::Editor {
                editor, pointer, ..
            } => PreparedRoot::Editor { editor, pointer },
            preflight::RootPlan::Grid(grid) => PreparedRoot::Grid(grid),
            preflight::RootPlan::Stage { id, recipe } => PreparedRoot::Stage(Box::new(
                stage::PreparedStage::from_recipe(id, *recipe, &footage.stage_sizes(), renderer)?,
            )),
        };
        Ok(Self {
            compiled,
            root,
            native,
            texts,
            attachments,
            tasks,
            value_tokens,
            components,
            rich_text,
            venn,
            sequences,
            captions,
            rolling,
            trees,
            plots,
            lanes,
            callouts,
            lenses,
            headers,
            footage,
            terminals,
            chats,
            changed_files,
            lower_thirds,
            viz,
        })
    }
}

impl CompiledPlan {
    #[cfg(test)]
    fn compile(plan: ScenePlan, base: &Path) -> Result<Self> {
        Self::new(plan, base, &HashMap::new())
    }

    fn new(
        plan: ScenePlan,
        base: &Path,
        targets: &HashMap<String, TargetGeometry>,
    ) -> Result<Self> {
        let mut properties = HashMap::new();
        let channels = plan.continuous_channels.iter().map(|channel| {
            let property = PropertyId::new(channel.id.clone());
            properties.insert(channel.id.clone(), property.clone());
            (channel, property)
        });
        let timeline =
            psychopomp::plan::compile_channels(channels, plan.duration_nanos, |value| {
                resolve_scalar(value, targets)
            })?;
        let state_tracks = plan
            .state_channels
            .iter()
            .map(|channel| {
                Ok((
                    channel.id.clone(),
                    StateTrack::compile(
                        channel.initial.clone(),
                        channel.events.iter().map(|event| {
                            TimedState::new(seconds_f64(event.at_nanos), event.value.clone())
                        }),
                        seconds_f64(plan.duration_nanos),
                    )?,
                ))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        // Preflight rejected media no recipe consumes; video belongs to its recipe.
        let media = plan
            .media
            .iter()
            .filter(|media| matches!(media.kind, MediaKindPlan::Audio))
            .map(|media| {
                let clip = Asset::audio(media.id.clone(), resolve_media_path(base, media))
                    .clip(TimeRange::new(
                        Time::from_nanos(media.source_start_nanos),
                        Time::from_nanos(media.source_end_nanos),
                    ))
                    .gain_db(media.gain_db);
                let role = match media.role {
                    MediaRolePlan::Script => MediaRole::Script,
                    MediaRolePlan::Layer => MediaRole::Layer,
                };
                MediaPlacement::new(clip, role, Time::from_nanos(media.timeline_start_nanos))
            })
            .collect();
        Ok(Self {
            plan,
            timeline,
            properties,
            state_tracks,
            media,
        })
    }

    fn duration(&self) -> Duration {
        Duration::from_nanos(self.plan.duration_nanos)
    }
}

impl PreparedPlan {
    fn editor(&self) -> Option<&PreparedEditor> {
        match &self.root {
            PreparedRoot::Editor { editor, .. } => Some(editor),
            _ => None,
        }
    }

    fn file_name(&self) -> &str {
        match &self.root {
            PreparedRoot::Editor { editor, .. } => editor.file_name(),
            _ => &self.plan.id,
        }
    }
}

impl CompiledPlan {
    #[cfg(test)]
    fn visual_sample_key(&self, time: f64) -> Result<VisualSampleKey> {
        self.visual_sample_key_using(time, &self.timeline)
    }

    fn visual_sample_key_using(&self, time: f64, timeline: &Timeline) -> Result<VisualSampleKey> {
        let previous_time = (time - 1.0 / 240.0).max(0.0);
        let motion = self
            .plan
            .continuous_channels
            .iter()
            .map(|channel| {
                let property = self
                    .properties
                    .get(&channel.id)
                    .with_context(|| format!("missing compiled property '{}'", channel.id))?;
                let current = timeline
                    .sample_at(property, time)
                    .with_context(|| format!("sample property '{}' at {time}", channel.id))?;
                let previous = timeline
                    .sample_at(property, previous_time)
                    .with_context(|| {
                        format!("sample property '{}' at {previous_time}", channel.id)
                    })?;
                Ok([
                    current.position.to_bits(),
                    current.velocity.to_bits(),
                    previous.position.to_bits(),
                    previous.velocity.to_bits(),
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        let states = self
            .plan
            .state_channels
            .iter()
            .map(|channel| {
                self.state_tracks
                    .get(&channel.id)
                    .with_context(|| format!("missing compiled state track '{}'", channel.id))
                    .map(|track| track.sample_at(time).current.clone())
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(VisualSampleKey {
            motion,
            states,
            video_frames: Vec::new(),
            ambient_time: None,
            anchors: Vec::new(),
        })
    }

    fn property(&self, actor: &str, name: &str) -> Option<&PropertyId> {
        let channel = self
            .plan
            .continuous_channels
            .iter()
            .find(|c| c.actor_id == actor && c.property == name)?;
        self.properties.get(&channel.id)
    }
    #[cfg(test)]
    fn playback(&self, reduced_motion: bool) -> Result<psychopomp::playback::Playback> {
        psychopomp::playback::Playback::new(&self.plan, &self.timeline, reduced_motion)
    }
}

impl PreparedPlan {
    fn visual_sample_key(&self, time: f64) -> Result<VisualSampleKey> {
        self.visual_sample_key_using(time, &self.timeline)
    }
    fn visual_sample_key_using(&self, time: f64, timeline: &Timeline) -> Result<VisualSampleKey> {
        let mut key = self.compiled.visual_sample_key_using(time, timeline)?;
        key.video_frames = self.video_frames(time, timeline);
        self.key_footage_playheads(&mut key);
        // A stage always moves (spin, flow, grain), so every temporal sample renders.
        let stage = matches!(&self.root, PreparedRoot::Stage(_));
        key.ambient_time = (stage
            || self.rolling_moves(time)
            || self.viz.moving(time)
            || self.running_properties().iter().any(|property| {
                timeline
                    .sample_at(property, time)
                    .is_some_and(|state| state.position > 0.001)
            }))
        .then_some(time.to_bits());
        self.key_caret_blinks(&mut key, time, timeline);
        Ok(key)
    }

    /// A caret's blink clock always runs, but its pixels change only while the
    /// blink fades: key the blink's opacity instead, so samples of a steady
    /// caret merge.
    fn key_caret_blinks(&self, key: &mut VisualSampleKey, time: f64, timeline: &Timeline) {
        let Some(editor) = self.editor() else {
            return;
        };
        for actor in editor.cursor_ids() {
            let Some(index) = self
                .plan
                .continuous_channels
                .iter()
                .position(|c| c.actor_id == actor && c.property == "blink")
            else {
                continue;
            };
            let clock = self
                .property(actor, "blink")
                .and_then(|property| timeline.sample_at(property, time))
                .map_or(-1.0, |state| state.position);
            key.motion[index] = [psychopomp::ide::caret_blink(clock).to_bits(), 0, 0, 0];
        }
    }

    fn running_properties(&self) -> Vec<PropertyId> {
        self.tasks
            .iter()
            .filter_map(task::PreparedTask::running_property)
            .map(PropertyId::new)
            .collect()
    }

    fn render_sample(&self, renderer: &mut HeadlessRenderer, time: f64) -> Result<Vec<u8>> {
        self.render_sample_using(renderer, time, &self.timeline)
    }

    /// Shutter samples for a frame centered at `center`.
    fn temporal_samples(&self, center: f64) -> u32 {
        match &self.root {
            // Samples accumulate on the GPU without a readback each.
            PreparedRoot::Stage(_) => STAGE_TEMPORAL_SAMPLES,
            _ => crate::exposure::plan_temporal_samples(center),
        }
    }

    /// One exposed frame from weighted shutter samples. A Stage accumulates
    /// its light on the GPU; overlays drawn over it are averaged only across
    /// samples where they differ, and only where they differ when that is
    /// just callouts.
    fn render_exposure(
        &self,
        renderer: &mut HeadlessRenderer,
        exposure: &[(f64, f32)],
    ) -> Result<Vec<u8>> {
        let PreparedRoot::Stage(stage) = &self.root else {
            if !self.lenses.is_empty() && self.texts.is_empty() && self.tasks.is_empty() {
                return self.render_lensed_exposure(renderer, exposure);
            }
            return crate::exposure::accumulate(renderer, exposure, |renderer, time| {
                self.render_sample(renderer, time)
            });
        };
        let timeline = &self.timeline;
        let base = stage.render_exposure(
            renderer,
            exposure,
            |actor, property, time, default| {
                self.property_value(timeline, actor, property, time, default)
            },
            |time| {
                self.footage.stage_frames(time, |property, default| {
                    self.property_value(timeline, stage.id(), property, time, default)
                })
            },
        )?;
        let size = renderer.size();
        let overlays = crate::exposure::merge_equal_samples(exposure.iter().copied(), |time| {
            self.overlay_key(time, stage.id(), size)
        })?;
        if !self.only_callouts_differ(&overlays, stage.id(), size)? {
            return crate::exposure::accumulate(renderer, &overlays, |renderer, time| {
                let mut pixels = base.clone();
                self.render_overlays(&mut pixels, renderer, time, timeline)?;
                Ok(pixels)
            });
        }
        // Only callouts differ: repaint just where moving ones ink, and draw a
        // still callout once unless a moving one overlaps it.
        let mut inked = Vec::new();
        for callout in &self.callouts {
            let poses = overlays
                .iter()
                .map(|&(time, _)| self.callout_pose(callout, time, timeline, size))
                .collect::<Vec<_>>();
            let moves = poses.windows(2).any(|pair| pair[0] != pair[1]);
            let bounds = poses
                .into_iter()
                .flatten()
                .filter_map(|pose| callout.bounds(renderer, pose))
                .collect::<Vec<_>>();
            inked.push((moves, bounds));
        }
        let moving = inked
            .iter()
            .filter(|(moves, _)| *moves)
            .flat_map(|(_, bounds)| bounds.iter().copied())
            .collect::<Vec<_>>();
        let overlaps = |a: &Box2, b: &Box2| {
            (a.min - 1.0).cmplt(b.max + 1.0).all() && (b.min - 1.0).cmplt(a.max + 1.0).all()
        };
        let redraw = inked
            .iter()
            .map(|(moves, bounds)| {
                *moves || bounds.iter().any(|a| moving.iter().any(|b| overlaps(a, b)))
            })
            .collect::<Vec<_>>();
        let region = crate::exposure::Region::covering(moving);
        let mut first = base.clone();
        self.render_overlays(&mut first, renderer, overlays[0].0, timeline)?;
        crate::exposure::accumulate_region(&overlays, &region, first, |frame, time| {
            region.copy(&base, frame);
            self.render_overlays_drawing(
                frame,
                renderer,
                time,
                timeline,
                |callout| redraw[callout],
                true,
            )
        })
    }

    /// Whether `samples` differ in nothing but their callouts. An overlay
    /// pinned to a Stage element differs while the camera moves it, and a
    /// visible lens refracts the frame around it, so its samples always
    /// compose whole.
    fn only_callouts_differ(
        &self,
        samples: &[(f64, f32)],
        stage: &str,
        size: [u32; 2],
    ) -> Result<bool> {
        if self.lenses.iter().any(|lens| {
            samples
                .iter()
                .any(|&(time, _)| self.lens_glass(lens, time, &self.timeline, size).is_some())
        }) {
            return Ok(false);
        }
        let mut keys = samples.iter().map(|&(time, _)| {
            let mut key = self.overlay_key_ignoring(time, |actor| {
                actor == stage || self.callouts.iter().any(|callout| callout.id() == actor)
            })?;
            key.anchors = self.stage_pins(time, size);
            Ok::<_, anyhow::Error>(key)
        });
        let Some(first) = keys.next().transpose()? else {
            return Ok(true);
        };
        for key in keys {
            if key? != first {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The visual state of everything but the root `stage` actor, plus where
    /// any callout pinned to it lands.
    fn overlay_key(&self, time: f64, stage: &str, size: [u32; 2]) -> Result<VisualSampleKey> {
        let mut key = self.overlay_key_ignoring(time, |actor| actor == stage)?;
        key.anchors = self
            .callouts
            .iter()
            .filter(|callout| callout.on_stage())
            .filter_map(|callout| self.callout_pose(callout, time, &self.timeline, size))
            .map(|pose| pose.anchor.to_array().map(f32::to_bits))
            .collect();
        key.anchors.extend(self.stage_pins(time, size));
        key.anchors.extend(
            self.lenses
                .iter()
                .filter(|lens| lens.on_stage())
                .filter_map(|lens| self.lens_glass(lens, time, &self.timeline, size))
                .map(|glass| glass.outline.center.to_array().map(f32::to_bits)),
        );
        Ok(key)
    }

    /// Every overlay that can pin to anchors, callouts aside.
    fn pinnable(&self) -> impl Iterator<Item = (&str, &[AnchorPlan])> {
        self.captions
            .iter()
            .map(|caption| (caption.id(), caption.anchors()))
            .chain(
                self.rolling
                    .iter()
                    .map(|number| (number.id(), number.anchors())),
            )
            .chain(
                self.texts
                    .iter()
                    .map(|text| (text.id.as_str(), text.anchors.as_slice())),
            )
            .chain(
                self.footage
                    .overlays()
                    .iter()
                    .map(|footage| (footage.id(), footage.anchors())),
            )
    }

    /// Where every overlay pinned to a Stage element lands at `time`: they
    /// move with the camera while no channel of theirs does.
    fn stage_pins(&self, time: f64, size: [u32; 2]) -> Vec<[u32; 2]> {
        self.pinnable()
            .filter(|(_, anchors)| anchor::on_stage(anchors))
            .map(|(actor, anchors)| {
                self.pin(actor, anchors, Vec2::ZERO, time, &self.timeline, size)
                    .map_or([u32::MAX; 2], |origin| origin.to_array().map(f32::to_bits))
            })
            .collect()
    }

    /// Where `target` is on the frame at `time`, from the prepared root.
    fn resolve_anchor(
        &self,
        target: AnchorTarget<'_>,
        time: f64,
        timeline: &Timeline,
        size: [u32; 2],
    ) -> Option<Vec2> {
        anchor::resolve(
            &self.root,
            &self.sequences,
            target,
            size,
            |actor, property, default| {
                self.property_value(timeline, actor, property, time, default)
            },
            |actor, property| self.raw_motion_value(timeline, actor, property, time),
            time,
        )
    }

    /// An overlay's origin at `time`: `literal` while it has no anchors, else
    /// its weighted anchors blended; `None` when none of them can be placed.
    fn pin(
        &self,
        actor: &str,
        anchors: &[AnchorPlan],
        literal: Vec2,
        time: f64,
        timeline: &Timeline,
        size: [u32; 2],
    ) -> Option<Vec2> {
        if anchors.is_empty() {
            return Some(literal);
        }
        anchor::pin(
            actor,
            anchors,
            |actor, property, default| {
                self.property_value(timeline, actor, property, time, default)
            },
            |target| self.resolve_anchor(target, time, timeline, size),
        )
    }

    /// The visual state of overlays, without the channels of `ignored` actors.
    fn overlay_key_ignoring(
        &self,
        time: f64,
        ignored: impl Fn(&str) -> bool,
    ) -> Result<VisualSampleKey> {
        let mut key = self
            .compiled
            .visual_sample_key_using(time, &self.timeline)?;
        for (motion, channel) in key
            .motion
            .iter_mut()
            .zip(&self.compiled.plan.continuous_channels)
        {
            if ignored(&channel.actor_id) {
                *motion = [0; 4];
            }
        }
        key.video_frames = self.video_frames(time, &self.timeline);
        self.key_footage_playheads(&mut key);
        key.ambient_time =
            (self.rolling_moves(time) || self.viz.moving(time)).then_some(time.to_bits());
        Ok(key)
    }

    /// The source frame each footage overlay shows: footage changes pixels
    /// without any channel moving.
    fn video_frames(&self, time: f64, timeline: &Timeline) -> Vec<u64> {
        self.footage
            .overlays()
            .iter()
            .map(|footage| {
                self.footage
                    .identity(footage, time, self.playhead(footage, time, timeline))
            })
            .collect()
    }

    /// With `PSYCHOPOMP_FOOTAGE_STATS` set, how the footage store fared:
    /// frames read from disk, shared hits, evictions, and peak memory.
    fn report_footage(&self) {
        if std::env::var_os("PSYCHOPOMP_FOOTAGE_STATS").is_none() {
            return;
        }
        let stats = self.footage.stats();
        eprintln!(
            "footage: {} frames read, {} shared hits, {} evicted, peak {:.1} MiB resident",
            stats.reads,
            stats.hits,
            stats.evictions,
            stats.peak_bytes as f64 / (1 << 20) as f64
        );
    }

    /// A footage overlay's written playhead, if its `time` channel exists.
    fn playhead(
        &self,
        footage: &footage::PreparedFootage,
        time: f64,
        timeline: &Timeline,
    ) -> Option<f32> {
        self.motion_value(timeline, footage.id(), "time", time)
            .map(|state| state.position)
    }

    /// A playing `time` channel moves every sample, but its pixels change
    /// only when the source frame does, which `video_frames` already keys:
    /// blank its motion, so samples within one source frame merge.
    fn key_footage_playheads(&self, key: &mut VisualSampleKey) {
        for footage in self.footage.overlays() {
            if let Some(index) = self
                .plan
                .continuous_channels
                .iter()
                .position(|c| c.actor_id == footage.id() && c.property == "time")
            {
                key.motion[index] = [0; 4];
            }
        }
    }

    fn callout_pose(
        &self,
        callout: &callout::PreparedCallout,
        time: f64,
        timeline: &Timeline,
        size: [u32; 2],
    ) -> Option<crate::render::CalloutPose> {
        let value = |actor: &str, property: &str, default: f32| {
            self.property_value(timeline, actor, property, time, default)
        };
        callout.pose(value, |target| {
            self.resolve_anchor(target, time, timeline, size)
        })
    }

    /// A CPU exposure with lenses on top: the page beneath the glass renders
    /// once per distinct state, and each sample refracts its own copy, so a
    /// lens gliding over still code costs a lens per sample, not a page.
    fn render_lensed_exposure(
        &self,
        renderer: &mut HeadlessRenderer,
        exposure: &[(f64, f32)],
    ) -> Result<Vec<u8>> {
        let timeline = &self.timeline;
        let mut pages: Vec<(VisualSampleKey, Vec<u8>)> = Vec::new();
        crate::exposure::accumulate(renderer, exposure, |renderer, time| {
            let key = self.overlay_key_ignoring(time, |actor| {
                self.lenses.iter().any(|lens| lens.id() == actor)
            })?;
            let index = match pages.iter().position(|(seen, _)| *seen == key) {
                Some(index) => index,
                None => {
                    let mut page = self.render_root(renderer, time, timeline)?;
                    self.render_overlays_drawing(
                        &mut page,
                        renderer,
                        time,
                        timeline,
                        |_| true,
                        false,
                    )?;
                    pages.push((key, page));
                    pages.len() - 1
                }
            };
            let mut pixels = pages[index].1.clone();
            self.render_lenses(&mut pixels, renderer.size(), time, timeline);
            Ok(pixels)
        })
    }

    fn render_lenses(&self, pixels: &mut [u8], size: [u32; 2], time: f64, timeline: &Timeline) {
        for lens in &self.lenses {
            if let Some(glass) = self.lens_glass(lens, time, timeline, size) {
                crate::render::composite_lens(pixels, size, &glass);
            }
        }
    }

    fn lens_glass(
        &self,
        lens: &lens::PreparedLens,
        time: f64,
        timeline: &Timeline,
        size: [u32; 2],
    ) -> Option<psychopomp::lens::Glass> {
        let value = |actor: &str, property: &str, default: f32| {
            self.property_value(timeline, actor, property, time, default)
        };
        lens.glass(value, |anchor| {
            self.resolve_anchor(anchor.target(), time, timeline, size)
        })
    }

    /// A settling Rolling Number changes every sample without a channel moving.
    fn rolling_moves(&self, time: f64) -> bool {
        self.rolling.iter().any(|number| number.moving(time))
            || self.changed_files.iter().any(|files| files.moving(time))
    }

    fn render_sample_using(
        &self,
        renderer: &mut HeadlessRenderer,
        time: f64,
        timeline: &Timeline,
    ) -> Result<Vec<u8>> {
        let mut pixels = self.render_root(renderer, time, timeline)?;
        self.render_overlays(&mut pixels, renderer, time, timeline)?;
        Ok(pixels)
    }

    fn render_root(
        &self,
        renderer: &mut HeadlessRenderer,
        time: f64,
        timeline: &Timeline,
    ) -> Result<Vec<u8>> {
        let value = |actor: &str, property: &str, default: f32| {
            self.property_value(timeline, actor, property, time, default)
        };
        Ok(match &self.root {
            PreparedRoot::Stage(stage) => stage.render(renderer, time, value, |time| {
                self.footage.stage_frames(time, |property, default| {
                    self.property_value(timeline, stage.id(), property, time, default)
                })
            })?,
            PreparedRoot::Editor { editor, pointer } => {
                editor.render(renderer, time, pointer.as_deref(), |actor, property, at| {
                    self.motion_value(timeline, actor, property, at)
                })?
            }
            PreparedRoot::Grid(grid) => grid.render(
                renderer,
                timeline,
                time,
                value(grid.actor_id(), "scale", 1.),
            )?,
            PreparedRoot::Title(title) => renderer.render_title_card(
                &title.title,
                title.subtitle.sample_at(time).current.as_deref(),
                value(&title.id, "opacity", 1.0).clamp(0.0, 1.0),
            ),
            PreparedRoot::Blank => renderer.render_title_card("", None, 0.0),
        })
    }

    /// Everything a plan draws over its root, in its fixed layer order.
    fn render_overlays(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        time: f64,
        timeline: &Timeline,
    ) -> Result<()> {
        self.render_overlays_drawing(pixels, renderer, time, timeline, |_| true, true)
    }

    /// `render_overlays`, drawing only the callouts whose index `callouts`
    /// accepts, and the lenses only when `lenses` is set.
    fn render_overlays_drawing(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        time: f64,
        timeline: &Timeline,
        callouts: impl Fn(usize) -> bool,
        lenses: bool,
    ) -> Result<()> {
        let value = |actor: &str, property: &str, default: f32| {
            self.property_value(timeline, actor, property, time, default)
        };
        // Video cards, images, and footage are the bottom media surface.
        // Value tiles are diagram surfaces; ordinary text is their foreground
        // annotation layer, regardless of declaration order.
        let size = renderer.size();
        self.footage.render(
            pixels,
            renderer,
            time,
            |footage| {
                let center = self.pin(
                    footage.id(),
                    footage.anchors(),
                    footage.center(),
                    time,
                    timeline,
                    size,
                )?;
                Some((center, self.playhead(footage, time, timeline)))
            },
            value,
        )?;
        // Text-surface windows sit just above recordings, beneath diagrams and text.
        for terminal in &self.terminals {
            terminal.render(pixels, renderer, value)?;
        }
        for chat in &self.chats {
            chat.render(pixels, renderer, value)?;
        }
        for files in &self.changed_files {
            files.render(pixels, renderer, time, value)?;
        }
        for diagram in &self.venn {
            diagram.render(pixels, renderer, value);
        }
        for token in &self.value_tokens {
            token.render(pixels, renderer, value);
        }
        for sequence in &self.sequences {
            sequence.render(pixels, renderer, value);
        }
        for plot in &self.plots {
            plot.render(pixels, renderer, value);
        }
        for lanes in &self.lanes {
            lanes.render(pixels, renderer, value);
        }
        self.viz
            .render_diagrams(pixels, renderer, value, |actor, property| {
                self.motion_value(timeline, actor, property, time)
                    .map_or(0.0, |state| state.velocity)
            });
        self.components.render(pixels, renderer, value)?;
        for header in &self.headers {
            header.render(pixels, renderer, value);
        }
        for text in &self.rich_text {
            text.render(pixels, renderer, value);
        }
        for tree in &self.trees {
            tree.render(pixels, renderer, value);
        }
        for caption in &self.captions {
            if let Some(origin) = self.pin(
                caption.id(),
                caption.anchors(),
                caption.origin(),
                time,
                timeline,
                size,
            ) {
                caption.render(pixels, renderer, origin, value);
            }
        }
        for number in &self.rolling {
            if let Some(origin) = self.pin(
                number.id(),
                number.anchors(),
                number.origin(),
                time,
                timeline,
                size,
            ) {
                number.render(pixels, renderer, time, origin, value);
            }
        }
        for third in &self.lower_thirds {
            third.render(pixels, renderer, value);
        }
        for (index, callout) in self.callouts.iter().enumerate() {
            if !callouts(index) {
                continue;
            }
            if let Some(pose) = self.callout_pose(callout, time, timeline, size) {
                callout.render(pixels, renderer, pose);
            }
        }
        // Lenses refract everything composed so far; plain text and Tasks
        // stay above the glass.
        if lenses {
            self.render_lenses(pixels, renderer.size(), time, timeline);
        }
        for text in &self.texts {
            let mut center = [
                value(&text.id, "x", text.center[0]),
                value(&text.id, "y", text.center[1]),
            ];
            if !text.anchors.is_empty() {
                // Pinned text moves from its anchor by the channels'
                // displacement from its declared center.
                let Some(pinned) =
                    self.pin(&text.id, &text.anchors, Vec2::ZERO, time, timeline, size)
                else {
                    continue;
                };
                center = [
                    pinned.x + center[0] - text.center[0],
                    pinned.y + center[1] - text.center[1],
                ];
            }
            renderer.composite_centered_text_masked(
                pixels,
                text.content.sample_at(time).current,
                center,
                text.font_size,
                text.color,
                value(&text.id, "opacity", 1.).clamp(0., 1.),
                text.mask,
            );
        }
        for task in &self.tasks {
            task.render(pixels, renderer, time, |actor, property| {
                self.motion_value(timeline, actor, property, time)
            })?;
        }
        self.viz.render_foreground(pixels, renderer, time, value);
        Ok(())
    }

    fn property_value(
        &self,
        timeline: &Timeline,
        actor_id: &str,
        property_name: &str,
        time: f64,
        default: f32,
    ) -> f32 {
        self.motion_value(timeline, actor_id, property_name, time)
            .map_or(default, |state| state.position)
    }

    fn motion_value(
        &self,
        timeline: &Timeline,
        actor_id: &str,
        property_name: &str,
        time: f64,
    ) -> Option<psychopomp::motion::MotionState> {
        let property = self.property(actor_id, property_name)?;
        let mut state = timeline.sample_at(property, time)?;
        for attachment in self
            .attachments
            .iter()
            .filter(|attachment| attachment.channel == property.as_str())
        {
            let weight = timeline.sample_at(self.properties.get(&attachment.weight)?, time)?;
            if weight.position == 0.0 && weight.velocity == 0.0 {
                continue;
            }
            let geometry = self
                .editor()?
                .target_motion(&attachment.target, |actor, property| {
                    self.raw_motion_value(timeline, actor, property, time)
                })?;
            let target = match attachment.component {
                TargetComponentPlan::X => geometry.x,
                TargetComponentPlan::Width => geometry.width,
                TargetComponentPlan::LineY => geometry.line_y,
                TargetComponentPlan::CenterX => psychopomp::motion::MotionState {
                    position: geometry.x.position + geometry.width.position * 0.5,
                    velocity: geometry.x.velocity + geometry.width.velocity * 0.5,
                },
            };
            let offset = target.position - attachment.base;
            state.position += weight.position * offset;
            state.velocity += weight.velocity * offset + weight.position * target.velocity;
        }
        Some(state)
    }

    fn raw_motion_value(
        &self,
        timeline: &Timeline,
        actor_id: &str,
        property_name: &str,
        time: f64,
    ) -> Option<psychopomp::motion::MotionState> {
        self.property(actor_id, property_name)
            .and_then(|property| timeline.sample_at(property, time))
    }
}

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
enum ServerRequest {
    Schema,
    Validate {
        plan: PathBuf,
    },
    Inspect {
        plan: PathBuf,
    },
    Steps {
        plan: PathBuf,
    },
    Diff {
        before: PathBuf,
        after: PathBuf,
    },
    Frame {
        plan: PathBuf,
        output: PathBuf,
        at_nanos: u64,
    },
    Render {
        plan: PathBuf,
        output: PathBuf,
        #[serde(default)]
        cue: Option<String>,
        #[serde(default)]
        start_nanos: Option<u64>,
        #[serde(default)]
        end_nanos: Option<u64>,
    },
    Shutdown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerResponse {
    id: Option<Value>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn serve() -> Result<()> {
    let mut renderer = new_renderer("scene-plan").await?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let envelope = serde_json::from_str::<Value>(&line);
        let id = envelope
            .as_ref()
            .ok()
            .and_then(|value| value.get("id"))
            .cloned();
        let request = envelope.and_then(serde_json::from_value::<ServerRequest>);
        let shutdown = matches!(&request, Ok(ServerRequest::Shutdown));
        let response = match request {
            Ok(request) => match handle_server_request(request, &mut renderer) {
                Ok(result) => ServerResponse {
                    id,
                    ok: true,
                    result: Some(result),
                    error: None,
                },
                Err(error) => ServerResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(format!("{error:#}")),
                },
            },
            Err(error) => ServerResponse {
                id,
                ok: false,
                result: None,
                error: Some(format!("parse request: {error}")),
            },
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
        if shutdown {
            break;
        }
    }
    Ok(())
}

fn handle_server_request(request: ServerRequest, renderer: &mut HeadlessRenderer) -> Result<Value> {
    match request {
        ServerRequest::Schema => Ok(ScenePlan::schema()),
        ServerRequest::Validate { plan } => {
            let plan = read_plan(&plan)?;
            validate_renderer_plan(&plan)?;
            Ok(json!({ "valid": true, "id": plan.id }))
        }
        ServerRequest::Inspect { plan } => {
            let plan = read_plan(&plan)?;
            Ok(inspect_plan(&plan))
        }
        ServerRequest::Steps { plan } => Ok(serde_json::to_value(
            psychopomp::editor::inspect_steps(&read_plan(&plan)?)?,
        )?),
        ServerRequest::Diff { before, after } => {
            let before = read_plan(&before)?;
            let after = read_plan(&after)?;
            Ok(json!({ "changes": before.diff(&after)? }))
        }
        ServerRequest::Frame {
            plan,
            output,
            at_nanos,
        } => {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let scene_plan = read_plan(&plan)?;
            if at_nanos > scene_plan.duration_nanos {
                bail!("frame time exceeds scene duration");
            }
            let base = plan.parent().unwrap_or_else(|| Path::new("."));
            let prepared = PreparedPlan::prepare(scene_plan, base, renderer)?;
            delivery::render_frame(&prepared, renderer, &output, Time::from_nanos(at_nanos))?;
            Ok(json!({ "output": output, "atNanos": at_nanos }))
        }
        ServerRequest::Render {
            plan,
            output,
            cue,
            start_nanos,
            end_nanos,
        } => {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let scene_plan = read_plan(&plan)?;
            let selection = match (cue, start_nanos, end_nanos) {
                (Some(id), None, None) => WindowSelection::Cue(id),
                (None, Some(start), Some(end)) if start < end => WindowSelection::Range(
                    TimeRange::new(Time::from_nanos(start), Time::from_nanos(end)),
                ),
                (None, None, None) => WindowSelection::Full,
                _ => bail!("provide either cue or both startNanos and endNanos"),
            };
            let window = plan_window(&scene_plan, selection)?;
            let base = plan.parent().unwrap_or_else(|| Path::new("."));
            let prepared = PreparedPlan::prepare(scene_plan, base, renderer)?;
            delivery::render_video(&prepared, renderer, &output, window)?;
            Ok(json!({
                "output": output,
                "startNanos": window.start().as_nanos(),
                "endNanos": window.end().as_nanos(),
            }))
        }
        ServerRequest::Shutdown => Ok(json!({ "shutdown": true })),
    }
}

fn resolve_media_path(base: &Path, media: &psychopomp::plan::MediaPlan) -> PathBuf {
    if media.path.is_absolute() {
        media.path.clone()
    } else {
        base.join(&media.path)
    }
}

fn inspect_plan(plan: &ScenePlan) -> Value {
    let mut cues = plan.cues.iter().collect::<Vec<_>>();
    cues.sort_by_key(|cue| (cue.start_nanos, cue.end_nanos, cue.id.as_str()));
    json!({
        "id": plan.id,
        "version": plan.version,
        "durationNanos": plan.duration_nanos,
        "presentationSteps": plan.presentation_steps,
        "actors": plan.actors.iter().map(|actor| &actor.id).collect::<Vec<_>>(),
        "semanticTargets": plan.semantic_targets.iter().map(|target| &target.id).collect::<Vec<_>>(),
        "continuousChannels": plan.continuous_channels.iter().map(|channel| &channel.id).collect::<Vec<_>>(),
        "stateChannels": plan.state_channels.iter().map(|channel| &channel.id).collect::<Vec<_>>(),
        "cues": cues,
        "media": plan.media.iter().map(|media| &media.id).collect::<Vec<_>>(),
    })
}

fn resolve_scalar(scalar: &ScalarPlan, targets: &HashMap<String, TargetGeometry>) -> Result<f32> {
    match scalar {
        ScalarPlan::Literal(value) => Ok(*value),
        ScalarPlan::Target(target) => {
            let geometry = targets.get(&target.target_id).with_context(|| {
                format!(
                    "semantic target '{}' was not resolved by its renderer recipe",
                    target.target_id
                )
            })?;
            let value = match target.component {
                TargetComponentPlan::X => geometry.x,
                TargetComponentPlan::Width => geometry.width,
                TargetComponentPlan::CenterX => geometry.center_x(),
                TargetComponentPlan::LineY => geometry.line_y,
            };
            Ok(value + target.offset)
        }
    }
}

fn validate_renderer_plan(plan: &ScenePlan) -> Result<()> {
    preflight::Plan::new(plan.clone()).map(|_| ())
}

fn seconds_f64(nanos: u64) -> f64 {
    nanos as f64 / 1_000_000_000.0
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use psychopomp::plan::{
        ActorPlan, ContinuousChannelPlan, MediaKindPlan, MediaPlan, MediaRolePlan, ScalarPlan,
        ScenePlan, SemanticTargetPlan, TargetComponentPlan, TargetScalarPlan, TrackEventPlan,
    };
    use serde_json::json;

    use super::{
        BUILTIN_HERO_PLAN, CompiledPlan, TargetGeometry, parse_range, validate_renderer_plan,
    };

    #[test]
    fn plan_channels_compile_through_the_shared_timeline() {
        let mut plan = ScenePlan::new("demo", 2_000_000_000);
        plan.actors.push(ActorPlan {
            id: "title".to_owned(),
            recipe: "title-card".to_owned(),
            data: json!({ "title": "Hello" }),
        });
        plan.continuous_channels.push(ContinuousChannelPlan {
            id: "title.opacity".to_owned(),
            actor_id: "title".to_owned(),
            property: "opacity".to_owned(),
            initial: 0.0.into(),
            events: vec![TrackEventPlan::Set {
                at_nanos: 1_000_000_000,
                value: 1.0.into(),
            }],
        });
        plan.validate().unwrap();

        let prepared = CompiledPlan::compile(plan, std::path::Path::new(".")).unwrap();
        let property = &prepared.properties["title.opacity"];
        assert_eq!(
            prepared.timeline.sample(property, 0.5).unwrap().position,
            0.0
        );
        assert_eq!(
            prepared.timeline.sample(property, 1.0).unwrap().position,
            1.0
        );
    }

    #[test]
    fn visual_keys_merge_settled_samples_but_preserve_motion() {
        let mut plan = ScenePlan::new("demo", 2_000_000_000);
        plan.actors.push(ActorPlan {
            id: "title".to_owned(),
            recipe: "title-card".to_owned(),
            data: json!({ "title": "Hello" }),
        });
        plan.continuous_channels.push(ContinuousChannelPlan {
            id: "title.opacity".to_owned(),
            actor_id: "title".to_owned(),
            property: "opacity".to_owned(),
            initial: 0.0.into(),
            events: vec![TrackEventPlan::Spring {
                at_nanos: 0,
                target: 1.0.into(),
                response_seconds: 0.4,
                damping_ratio: 1.0,
                position_threshold: 0.001,
                velocity_threshold: 0.001,
            }],
        });
        plan.validate().unwrap();
        let prepared = CompiledPlan::compile(plan, std::path::Path::new(".")).unwrap();

        assert_ne!(
            prepared.visual_sample_key(0.1).unwrap(),
            prepared.visual_sample_key(0.11).unwrap()
        );
        assert_eq!(
            prepared.visual_sample_key(1.5).unwrap(),
            prepared.visual_sample_key(1.51).unwrap()
        );
    }

    #[test]
    fn semantic_target_scalars_resolve_before_timeline_compilation() {
        let mut plan = ScenePlan::new("demo", 1_000_000_000);
        plan.actors.push(ActorPlan {
            id: "editor".to_owned(),
            recipe: "editor".to_owned(),
            data: json!({}),
        });
        plan.semantic_targets.push(SemanticTargetPlan {
            id: "token".to_owned(),
            actor_id: "editor".to_owned(),
            selector: json!({}),
        });
        plan.continuous_channels.push(ContinuousChannelPlan {
            id: "editor.x".to_owned(),
            actor_id: "editor".to_owned(),
            property: "x".to_owned(),
            initial: ScalarPlan::Target(TargetScalarPlan {
                target_id: "token".to_owned(),
                component: TargetComponentPlan::CenterX,
                offset: 40.0,
            }),
            events: vec![],
        });
        plan.validate().unwrap();
        let targets = HashMap::from([(
            "token".to_owned(),
            TargetGeometry {
                x: 100.0,
                width: 20.0,
                line_y: 200.0,
            },
        )]);

        let prepared = CompiledPlan::new(plan, std::path::Path::new("."), &targets).unwrap();
        assert_eq!(
            prepared
                .timeline
                .sample(&prepared.properties["editor.x"], 0.0)
                .unwrap()
                .position,
            150.0
        );
    }

    #[test]
    fn unsupported_timed_media_returns_an_error_instead_of_panicking() {
        let mut plan = ScenePlan::new("demo", 2_000_000_000);
        plan.media.push(MediaPlan {
            id: "image".to_owned(),
            path: "image.png".into(),
            kind: MediaKindPlan::Image,
            role: MediaRolePlan::Layer,
            source_start_nanos: 0,
            source_end_nanos: 1_000_000_000,
            timeline_start_nanos: 0,
            timeline_end_nanos: 1_000_000_000,
            gain_db: 0.0,
        });
        plan.validate().unwrap();

        let error = validate_renderer_plan(&plan).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no actor consuming Image media 'image'")
        );
    }

    #[test]
    fn invalid_cli_ranges_return_errors() {
        assert!(parse_range("NaN..1").is_err());
        assert!(parse_range("-1..1").is_err());
        assert!(parse_range("2..1").is_err());
    }

    #[test]
    fn renderer_default_tests_guard_the_canonical_hero_plan() {
        assert_eq!(
            psychopomp_hero::build_plan()
                .unwrap()
                .to_json_pretty()
                .unwrap(),
            BUILTIN_HERO_PLAN
        );
    }

    #[test]
    fn invalid_text_recipes_and_duplicate_roots_are_rejected_during_preparation() {
        let mut plan = ScenePlan::new("demo", 1_000_000_000);
        plan.actors.push(ActorPlan {
            id: "text".to_owned(),
            recipe: "text".to_owned(),
            data: json!({ "text": "Hello", "center": [960, 540], "fontSize": -1 }),
        });
        assert!(validate_renderer_plan(&plan).is_err());

        plan.actors = vec![
            ActorPlan {
                id: "first".to_owned(),
                recipe: "title-card".to_owned(),
                data: json!({ "title": "First" }),
            },
            ActorPlan {
                id: "second".to_owned(),
                recipe: "title-card".to_owned(),
                data: json!({ "title": "Second" }),
            },
        ];
        assert!(validate_renderer_plan(&plan).is_err());
    }

    #[test]
    fn concrete_validation_rejects_unpreparable_and_unknown_recipes() {
        let mut plan = ScenePlan::new("demo", 1_000_000_000);
        plan.actors.push(ActorPlan {
            id: "grid".to_owned(),
            recipe: "keyed-grid".to_owned(),
            data: json!({}),
        });
        assert!(validate_renderer_plan(&plan).is_err());

        plan.actors[0].recipe = "unknown".to_owned();
        assert!(validate_renderer_plan(&plan).is_err());
    }

    #[test]
    fn media_ranges_remain_exact_integer_nanoseconds_during_preparation() {
        let mut plan = ScenePlan::new("long", u64::MAX);
        plan.media.push(MediaPlan {
            id: "audio".to_owned(),
            path: "audio.wav".into(),
            kind: MediaKindPlan::Audio,
            role: MediaRolePlan::Layer,
            source_start_nanos: u64::MAX - 1,
            source_end_nanos: u64::MAX,
            timeline_start_nanos: 0,
            timeline_end_nanos: 1,
            gain_db: 0.0,
        });
        plan.validate().unwrap();

        let prepared = CompiledPlan::compile(plan, std::path::Path::new(".")).unwrap();
        let source = prepared.media[0].clip().source_range();
        assert_eq!(source.start().as_nanos(), u64::MAX - 1);
        assert_eq!(source.end().as_nanos(), u64::MAX);
    }
}
