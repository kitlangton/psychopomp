//! `psychopomp verify`: run every Scene Program in the manifest, load each
//! plan it writes (validating it), render its key frames on one renderer, and
//! store plans and pixels as a baseline or compare them against one.
mod diff;
mod manifest;

use std::{
    collections::HashMap,
    env, fs,
    hash::{DefaultHasher, Hasher},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    delivery::write_png,
    new_renderer,
    still::{self, Change, Loaded},
};
use crate::render::{HeadlessRenderer, Theme};
use manifest::{Manifest, Scene};

pub(crate) const USAGE: &str = "psychopomp verify baseline [LABEL] [--manifest FILE] [--out DIR] [--only NAME,..] | psychopomp verify compare [LABEL | DIR] [--manifest FILE] [--out DIR] [--only NAME,..] [--expect NAME,..]";

const DEFAULT_LABEL: &str = "baseline";
/// The profile Scene Programs are built with: the one `cargo run -p` uses,
/// so emitted plans match checked-in ones. Release and dev builds can differ
/// in a plan's last float digits (constant-folded trigonometry), so a
/// baseline and its comparison must share a profile.
const PROFILE: &str = "dev";
const SUMMARY: &str = "summary.json";
/// Absolute paths under the workspace are stored relative to this marker, so
/// worktrees can share one baseline.
const ROOT_MARKER: &str = "$ROOT";

pub(crate) fn command(arguments: &[String]) -> Result<()> {
    run(Options::parse(arguments)?)
}

#[derive(Debug, PartialEq)]
enum Mode {
    Baseline,
    /// Compare against a baseline label or directory.
    Compare(String),
}

#[derive(Debug, PartialEq)]
struct Options {
    mode: Mode,
    label: Option<String>,
    manifest: PathBuf,
    out: Option<PathBuf>,
    only: Vec<String>,
    expect: Vec<String>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self> {
        let Some((mode, arguments)) = arguments.split_first() else {
            bail!("usage: {USAGE}");
        };
        let mut options = Self {
            mode: Mode::Baseline,
            label: None,
            manifest: PathBuf::from("verify.json"),
            out: None,
            only: Vec::new(),
            expect: Vec::new(),
        };
        let mut positional = Vec::new();
        let mut iter = arguments.iter();
        while let Some(argument) = iter.next() {
            let mut value = || {
                iter.next()
                    .with_context(|| format!("{argument} requires a value; usage: {USAGE}"))
            };
            match argument.as_str() {
                "--manifest" => options.manifest = PathBuf::from(value()?),
                "--out" => options.out = Some(PathBuf::from(value()?)),
                "--only" => options.only.extend(names(value()?)),
                "--expect" => options.expect.extend(names(value()?)),
                flag if flag.starts_with("--") => bail!("unknown option '{flag}'; usage: {USAGE}"),
                _ => positional.push(argument.clone()),
            }
        }
        ensure!(positional.len() <= 1, "usage: {USAGE}");
        let positional = positional.pop();
        match mode.as_str() {
            "baseline" => {
                ensure!(
                    options.expect.is_empty(),
                    "--expect applies to verify compare"
                );
                options.label = positional;
            }
            "compare" => {
                options.mode =
                    Mode::Compare(positional.unwrap_or_else(|| DEFAULT_LABEL.to_owned()));
            }
            _ => bail!("usage: {USAGE}"),
        }
        Ok(options)
    }

    /// The output directory and, when comparing, the baseline directory.
    fn directories(&self, root: &Path) -> Result<(PathBuf, Option<PathBuf>)> {
        let labeled = |label: &str| root.join("target/verify").join(label);
        match &self.mode {
            Mode::Baseline => Ok((
                self.out
                    .clone()
                    .unwrap_or_else(|| labeled(self.label.as_deref().unwrap_or(DEFAULT_LABEL))),
                None,
            )),
            Mode::Compare(reference) => {
                let reference = if reference.contains(std::path::MAIN_SEPARATOR) {
                    PathBuf::from(reference)
                } else {
                    labeled(reference)
                };
                let out = self.out.clone().unwrap_or_else(|| labeled("current"));
                ensure!(
                    absolute(&out) != absolute(&reference),
                    "the comparison would overwrite its baseline {}; pass --out",
                    reference.display()
                );
                Ok((out, Some(reference)))
            }
        }
    }
}

fn names(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

/// What one run stored: `summary.json` beside `plans/` and `frames/`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    profile: String,
    commit: Option<String>,
    dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<String>,
    timings: Timings,
    scenes: Vec<SceneRecord>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Timings {
    build: f64,
    emit: f64,
    render: f64,
    total: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SceneRecord {
    name: String,
    package: String,
    source: String,
    theme: Theme,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<f64>,
    #[serde(default)]
    frames: Vec<FrameRecord>,
    seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Why no frame rendered: a required file is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skipped: Option<String>,
    /// When comparing: the plan's byte identity and structural changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<PlanComparison>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameRecord {
    time: f64,
    shutter: bool,
    /// Relative to the run's directory. A comparison stores only the frames
    /// that differ from or are missing in its baseline.
    file: String,
    hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    change: Option<FrameChange>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameChange {
    identical: bool,
    /// No baseline frame at this time.
    new: bool,
    changed_pixels: usize,
    max_delta: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounds: Option<[usize; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanComparison {
    identical: bool,
    new: bool,
    changes: Vec<String>,
}

impl SceneRecord {
    fn changed(&self) -> bool {
        self.plan.as_ref().is_some_and(|plan| !plan.identical)
            || self.frames.iter().any(|frame| {
                frame
                    .change
                    .as_ref()
                    .is_some_and(|change| !change.identical)
            })
    }
}

/// A baseline run being compared against.
struct Baseline {
    directory: PathBuf,
    summary: Summary,
}

impl Baseline {
    fn read(directory: PathBuf) -> Result<Self> {
        let path = directory.join(SUMMARY);
        let json = fs::read_to_string(&path).with_context(|| {
            format!(
                "read baseline {}; create it first with `psychopomp verify baseline`",
                path.display()
            )
        })?;
        let summary: Summary = serde_json::from_str(&json)
            .with_context(|| format!("parse baseline {}", path.display()))?;
        ensure!(
            summary.profile == PROFILE,
            "baseline {} emitted plans with the {} profile, not {PROFILE}; recreate it",
            directory.display(),
            summary.profile
        );
        Ok(Self { directory, summary })
    }

    fn scene(&self, name: &str) -> Option<&SceneRecord> {
        self.summary.scenes.iter().find(|scene| scene.name == name)
    }

    fn plan(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.directory.join("plans").join(format!("{name}.json"))).ok()
    }
}

fn run(options: Options) -> Result<()> {
    let started = Instant::now();
    let manifest_path = options.manifest.canonicalize().with_context(|| {
        format!(
            "find verify manifest {}; run from the repository root or pass --manifest",
            options.manifest.display()
        )
    })?;
    let root = manifest_path
        .parent()
        .context("manifest has no directory")?;
    let manifest = Manifest::read(&manifest_path)?;
    let scenes = manifest.select(&options.only)?;
    manifest.select(&options.expect)?;
    let (out, reference) = options.directories(root)?;
    let baseline = reference.map(Baseline::read).transpose()?;
    replace_output(&out)?;

    let mut timings = Timings::default();
    let clock = Instant::now();
    let binaries = build(root, &scenes)?;
    timings.build = clock.elapsed().as_secs_f64();

    let clock = Instant::now();
    let mut failed_runs = HashMap::new();
    for (package, args) in manifest::runs(&scenes) {
        if let Err(error) = emit(root, &binaries[package], args) {
            failed_runs.insert((package, args), format!("{package} failed: {error:#}"));
        }
    }
    timings.emit = clock.elapsed().as_secs_f64();

    let clock = Instant::now();
    let mut renderer = pollster::block_on(new_renderer("verify"))?;
    let mut records = Vec::new();
    for scene in &scenes {
        let clock = Instant::now();
        let mut record = SceneRecord {
            name: scene.name.clone(),
            package: scene.package.clone(),
            source: scene.plan.clone(),
            theme: scene.theme(&manifest),
            duration: None,
            frames: Vec::new(),
            seconds: 0.0,
            error: failed_runs
                .get(&(scene.package.as_str(), scene.args.as_slice()))
                .cloned(),
            plan: None,
            skipped: None,
        };
        if record.error.is_none()
            && let Err(error) = verify_scene(
                scene,
                &mut record,
                root,
                &out,
                baseline.as_ref(),
                &mut renderer,
            )
        {
            record.error = Some(format!("{error:#}"));
        }
        record.seconds = clock.elapsed().as_secs_f64();
        print_scene(&record, baseline.is_some());
        records.push(record);
    }
    timings.render = clock.elapsed().as_secs_f64();
    timings.total = started.elapsed().as_secs_f64();

    let summary = Summary {
        profile: PROFILE.to_owned(),
        commit: git(root, &["rev-parse", "--short", "HEAD"]),
        dirty: git(root, &["status", "--porcelain", "--untracked-files=no"])
            .is_some_and(|status| !status.is_empty()),
        baseline: baseline
            .as_ref()
            .map(|baseline| baseline.directory.display().to_string()),
        timings,
        scenes: records,
    };
    fs::write(
        out.join(SUMMARY),
        serde_json::to_string_pretty(&summary)? + "\n",
    )?;
    conclude(&summary, root, &out, baseline.as_ref(), &options)
}

/// Clear a previous run's directory; refuse to touch anything else.
fn replace_output(out: &Path) -> Result<()> {
    if out.exists() {
        let empty = fs::read_dir(out)?.next().is_none();
        ensure!(
            empty || out.join(SUMMARY).exists(),
            "refusing to replace {}: it is not a verify run",
            out.display()
        );
        fs::remove_dir_all(out).with_context(|| format!("clear {}", out.display()))?;
    }
    fs::create_dir_all(out.join("plans")).with_context(|| format!("create {}", out.display()))
}

/// Build every Scene Program in one Cargo invocation; their executables by
/// package name.
fn build(root: &Path, scenes: &[&Scene]) -> Result<HashMap<String, PathBuf>> {
    let mut packages = Vec::new();
    for scene in scenes {
        if !packages.contains(&scene.package.as_str()) {
            packages.push(scene.package.as_str());
        }
    }
    eprintln!(
        "building {} Scene Program{}",
        packages.len(),
        if packages.len() == 1 { "" } else { "s" }
    );
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.current_dir(root).args([
        "build",
        "--profile",
        PROFILE,
        "--quiet",
        "--message-format=json-render-diagnostics",
    ]);
    for package in &packages {
        command.args(["-p", package]);
    }
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .context("run cargo build")?;
    ensure!(output.status.success(), "cargo build failed");
    let mut binaries = HashMap::new();
    for line in output.stdout.split(|&byte| byte == b'\n') {
        let Ok(message) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if let (Some(name), Some(executable)) = (
            message.pointer("/target/name").and_then(Value::as_str),
            message.get("executable").and_then(Value::as_str),
        ) {
            binaries.insert(name.to_owned(), PathBuf::from(executable));
        }
    }
    for package in packages {
        ensure!(
            binaries.contains_key(package),
            "package '{package}' built no binary named '{package}'"
        );
    }
    Ok(binaries)
}

/// Run one Scene Program from the workspace root. Generated media is never
/// requested: `PSYCHOPOMP_MEDIA=plan` calls no provider and fails instead.
fn emit(root: &Path, binary: &Path, args: &[String]) -> Result<()> {
    let output = Command::new(binary)
        .args(args)
        .current_dir(root)
        .env("PSYCHOPOMP_MEDIA", "plan")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run {}", binary.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr.lines().rev().take(12).collect::<Vec<_>>();
        bail!(
            "{}\n{}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

fn verify_scene(
    scene: &Scene,
    record: &mut SceneRecord,
    root: &Path,
    out: &Path,
    baseline: Option<&Baseline>,
    renderer: &mut HeadlessRenderer,
) -> Result<()> {
    let source = root.join(&scene.plan);
    let json = fs::read_to_string(&source)
        .with_context(|| format!("read {} (does the program write it?)", source.display()))?;
    let json = normalize(&json, root);
    fs::write(
        out.join("plans").join(format!("{}.json", scene.name)),
        &json,
    )?;
    if let Some(baseline) = baseline {
        record.plan = Some(compare_plans(baseline.plan(&scene.name).as_deref(), &json));
    }
    if let Some(missing) = scene.missing(root) {
        record.skipped = Some(format!("frames need {missing}"));
        return Ok(());
    }

    let loaded = Loaded::load_on(&source, record.theme, renderer)?;
    let duration = loaded.duration_seconds();
    record.duration = Some(duration);
    let outside = scene
        .frames()
        .filter(|&(at, _)| at > duration)
        .map(|(at, _)| format!("{at}s"))
        .collect::<Vec<_>>();
    ensure!(
        outside.is_empty(),
        "{} past the plan's {duration:.3}s; choose times inside it",
        outside.join(", ")
    );
    let frames = Path::new("frames").join(&scene.name);
    fs::create_dir_all(out.join(&frames))?;
    let previous = baseline.and_then(|baseline| baseline.scene(&scene.name));
    for (at, shutter) in scene.frames() {
        let pixels = loaded.still(renderer, at, shutter)?;
        let hash = hash(&pixels);
        let file = frames.join(frame_file(at, shutter));
        let mut frame = FrameRecord {
            time: at,
            shutter,
            file: file.display().to_string(),
            hash,
            change: None,
        };
        let Some(baseline) = baseline else {
            write_png(&out.join(&file), &pixels)?;
            record.frames.push(frame);
            continue;
        };
        let before = previous.and_then(|scene| {
            scene
                .frames
                .iter()
                .find(|before| before.time == at && before.shutter == shutter)
        });
        let change = match before {
            Some(before) if before.hash == frame.hash => FrameChange::identical(),
            Some(before) => {
                let path = baseline.directory.join(&before.file);
                let change = Change::between(&still::read_png(&path)?, &pixels);
                FrameChange {
                    identical: change.pixels == 0,
                    new: false,
                    changed_pixels: change.pixels,
                    max_delta: change.max,
                    bounds: change.bounds,
                    baseline: Some(path.display().to_string()),
                }
            }
            None => FrameChange {
                identical: false,
                new: true,
                ..FrameChange::identical()
            },
        };
        if !change.identical {
            write_png(&out.join(&file), &pixels)?;
        }
        frame.change = Some(change);
        record.frames.push(frame);
    }
    Ok(())
}

impl FrameChange {
    fn identical() -> Self {
        Self {
            identical: true,
            new: false,
            changed_pixels: 0,
            max_delta: 0,
            bounds: None,
            baseline: None,
        }
    }
}

fn compare_plans(before: Option<&str>, after: &str) -> PlanComparison {
    let Some(before) = before else {
        return PlanComparison {
            identical: false,
            new: true,
            changes: Vec::new(),
        };
    };
    if before == after {
        return PlanComparison {
            identical: true,
            new: false,
            changes: Vec::new(),
        };
    }
    let changes = match (
        serde_json::from_str::<Value>(before),
        serde_json::from_str::<Value>(after),
    ) {
        (Ok(before), Ok(after)) => diff::summarize(&before, &after),
        _ => vec!["not JSON".to_owned()],
    };
    PlanComparison {
        identical: false,
        new: false,
        changes: if changes.is_empty() {
            vec!["bytes differ; same structure (formatting or key order)".to_owned()]
        } else {
            changes
        },
    }
}

fn normalize(json: &str, root: &Path) -> String {
    json.replace(&*root.to_string_lossy(), ROOT_MARKER)
}

fn frame_file(at: f64, shutter: bool) -> String {
    let name = still::frame_name(at);
    if shutter {
        name.replace(".png", ".shutter.png")
    } else {
        name
    }
}

/// A content hash that lets a comparison skip decoding identical baselines;
/// a mismatch is always confirmed pixel by pixel.
fn hash(pixels: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(pixels);
    format!("{:016x}", hasher.finish())
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// `path` relative to the workspace root when it lies inside it.
fn shown(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn print_scene(record: &SceneRecord, comparing: bool) {
    /// Plan changes printed per scene; `summary.json` holds them all.
    const SHOWN: usize = 10;
    let name = &record.name;
    let seconds = record.seconds;
    if let Some(error) = &record.error {
        println!("{name:<24} FAILED  {seconds:6.2}s");
        for line in error.lines() {
            println!("    {line}");
        }
        return;
    }
    let total = record.frames.len();
    let skipped = record
        .skipped
        .as_ref()
        .map(|reason| format!("  (no frames: {reason})"))
        .unwrap_or_default();
    if !comparing {
        println!("{name:<24} {total:>2} frames          {seconds:6.2}s{skipped}");
        return;
    }
    let plan = match &record.plan {
        Some(plan) if plan.new => "plan NEW",
        Some(plan) if !plan.identical => "plan CHANGED",
        _ => "plan same",
    };
    let differing = record
        .frames
        .iter()
        .filter(|frame| {
            frame
                .change
                .as_ref()
                .is_some_and(|change| !change.identical)
        })
        .count();
    let frames = if differing == 0 {
        format!("{total} frames same")
    } else {
        format!("{differing} of {total} frames CHANGED")
    };
    println!("{name:<24} {plan:<13} {frames:<22} {seconds:6.2}s{skipped}");
    let changes = record.plan.iter().flat_map(|plan| &plan.changes);
    for change in changes.clone().take(SHOWN) {
        println!("    {change}");
    }
    let hidden = changes.count().saturating_sub(SHOWN);
    if hidden > 0 {
        println!("    ... {hidden} more plan changes in summary.json");
    }
    for frame in &record.frames {
        let Some(change) = frame.change.as_ref().filter(|change| !change.identical) else {
            continue;
        };
        let at = format!(
            "{:.3}s{}",
            frame.time,
            if frame.shutter { " shutter" } else { "" }
        );
        if change.new {
            println!("    frame {at}: new, not in the baseline");
            continue;
        }
        let bounds = change
            .bounds
            .map(|[x0, y0, x1, y1]| format!(" in {x0},{y0}..{x1},{y1}"))
            .unwrap_or_default();
        println!(
            "    frame {at}: {} px differ, max delta {}{bounds}: {}",
            change.changed_pixels, change.max_delta, frame.file
        );
    }
}

fn conclude(
    summary: &Summary,
    root: &Path,
    out: &Path,
    baseline: Option<&Baseline>,
    options: &Options,
) -> Result<()> {
    let Timings {
        build,
        emit,
        render,
        total,
    } = summary.timings;
    let frames = summary
        .scenes
        .iter()
        .map(|scene| scene.frames.len())
        .sum::<usize>();
    println!(
        "{} scenes, {frames} frames: build {build:.1}s, emit {emit:.1}s, render {render:.1}s, total {total:.1}s",
        summary.scenes.len()
    );
    for scene in &summary.scenes {
        if let Some(reason) = &scene.skipped {
            println!("{}: plan only, {reason}", scene.name);
        }
    }
    let failed = summary
        .scenes
        .iter()
        .filter(|scene| scene.error.is_some())
        .map(|scene| scene.name.as_str())
        .collect::<Vec<_>>();
    let Some(baseline) = baseline else {
        println!("baseline written to {}", shown(out, root));
        ensure!(failed.is_empty(), "failed: {}", failed.join(", "));
        return Ok(());
    };
    let expect = &options.expect;
    let changed = |expected: bool| {
        summary
            .scenes
            .iter()
            .filter(|scene| scene.changed() && expect.contains(&scene.name) == expected)
            .map(|scene| scene.name.as_str())
            .collect::<Vec<_>>()
    };
    let (unexpected, expected) = (changed(false), changed(true));
    let commit = baseline
        .summary
        .commit
        .as_deref()
        .unwrap_or("unknown commit");
    println!(
        "compared {} with {} ({commit}{})",
        shown(out, root),
        shown(&baseline.directory, root),
        if baseline.summary.dirty {
            ", dirty"
        } else {
            ""
        }
    );
    if options.only.is_empty() {
        let missing = baseline
            .summary
            .scenes
            .iter()
            .filter(|before| !summary.scenes.iter().any(|scene| scene.name == before.name))
            .map(|before| before.name.as_str())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            println!(
                "in the baseline but not the manifest: {}",
                missing.join(", ")
            );
        }
    }
    if !expected.is_empty() {
        println!("changed as expected: {}", expected.join(", "));
    }
    let unchanged = expect
        .iter()
        .filter(|name| !expected.contains(&name.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if !unchanged.is_empty() {
        println!("expected to change but identical: {}", unchanged.join(", "));
    }
    ensure!(failed.is_empty(), "failed: {}", failed.join(", "));
    if !unexpected.is_empty() {
        bail!("changed unexpectedly: {}", unexpected.join(", "));
    }
    if expected.is_empty() {
        println!("identical");
    } else {
        println!("no unexpected changes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Mode, Options, compare_plans, frame_file, normalize};

    fn parse(arguments: &str) -> anyhow::Result<Options> {
        Options::parse(
            &arguments
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn baselines_default_to_a_label_under_target() {
        let root = Path::new("/repo");
        let options = parse("baseline").unwrap();
        assert_eq!(options.mode, Mode::Baseline);
        assert_eq!(options.manifest, PathBuf::from("verify.json"));
        assert_eq!(
            options.directories(root).unwrap(),
            (PathBuf::from("/repo/target/verify/baseline"), None)
        );
        let options = parse("baseline main --only hello,camera --only tree").unwrap();
        assert_eq!(options.only, ["hello", "camera", "tree"]);
        assert_eq!(
            options.directories(root).unwrap().0,
            PathBuf::from("/repo/target/verify/main")
        );
        let options = parse("baseline --out /tmp/shared --manifest other.json").unwrap();
        assert_eq!(options.manifest, PathBuf::from("other.json"));
        assert_eq!(
            options.directories(root).unwrap().0,
            PathBuf::from("/tmp/shared")
        );
    }

    #[test]
    fn comparisons_take_a_label_or_a_directory() {
        let root = Path::new("/repo");
        let (out, reference) = parse("compare").unwrap().directories(root).unwrap();
        assert_eq!(out, PathBuf::from("/repo/target/verify/current"));
        assert_eq!(
            reference,
            Some(PathBuf::from("/repo/target/verify/baseline"))
        );
        let options = parse("compare /tmp/shared --expect hello").unwrap();
        assert_eq!(options.expect, ["hello"]);
        assert_eq!(
            options.directories(root).unwrap().1,
            Some(PathBuf::from("/tmp/shared"))
        );
        let error = parse("compare current").unwrap().directories(root);
        assert!(error.unwrap_err().to_string().contains("pass --out"));
    }

    #[test]
    fn malformed_arguments_are_rejected() {
        for arguments in [
            "",
            "check",
            "baseline a b",
            "baseline --expect hello",
            "compare --only",
            "compare --theme neutral",
        ] {
            assert!(parse(arguments).is_err(), "{arguments}");
        }
    }

    #[test]
    fn plans_compare_bytes_then_structure() {
        let plan = r#"{ "id": "a", "durationNanos": 1000000000 }"#;
        assert!(compare_plans(Some(plan), plan).identical);
        assert!(compare_plans(None, plan).new);
        let reordered = r#"{ "durationNanos": 1000000000, "id": "a" }"#;
        let comparison = compare_plans(Some(plan), reordered);
        assert!(!comparison.identical);
        assert!(comparison.changes[0].contains("same structure"));
        let longer = r#"{ "id": "a", "durationNanos": 2000000000 }"#;
        assert_eq!(
            compare_plans(Some(plan), longer).changes,
            ["duration 1.000s → 2.000s"]
        );
    }

    #[test]
    fn workspace_paths_are_stored_relative_to_a_marker() {
        let json = r#"{ "path": "/repo/scenes/a/narration/intro.mp3" }"#;
        assert_eq!(
            normalize(json, Path::new("/repo")),
            r#"{ "path": "$ROOT/scenes/a/narration/intro.mp3" }"#
        );
    }

    #[test]
    fn frame_files_sort_by_time_and_mark_the_shutter() {
        assert_eq!(frame_file(1.5, false), "00001.500.png");
        assert_eq!(frame_file(12.0, true), "00012.000.shutter.png");
    }
}
