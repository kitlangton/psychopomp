use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    plan::ScenePlan,
    rune_surface::{
        RuneAction, RuneDiffAnnotation, RuneDiffFile, RuneDiffMode, RuneDiffSelection,
        RuneSurfaceActor, RuneSurfacePlan,
    },
    sfx,
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 4: `RuneSurfaceActor` (`RuneKind::Diff` — `rune_kit::DiffView`)
/// showing Myers word-diff on `shell.ts`, line selection, inline review card,
/// and live Unified → Split toggle.
pub fn rune_diff(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("04-rune-diff", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "shell.ts",
        "Explicit timeouts, onSettle & job_* tools",
    )?;
    chip(&mut scene, "chip", Tone::Success, "rune_kit::DiffView")?;

    let timeout_at = spoken.at_any(&["explicit timeout", "two-minute", "implicit"]);
    let settle_at = spoken.at_any(&["global Job", "settle", "config reloads"]);
    let tools_at = spoken.at_any(&["four pinned", "job list", "job stop"]);

    let before = "\
const job = yield* jobs.start({ id: info.id, run })
yield* notifyWhenDone(ctx.sessionID, info.id, settled)
const res = yield* jobs.block({ id: job.id, sessionID })
if (res?.type === \"backgrounded\") {
  return backgroundResult(info.id, info.file)
}
";

    let after = "\
const job = yield* jobs.start<Output>({
  id: info.id, run, output: shell.snapshot(info.id),
  onSettle: (s) => deliverShellNotice(sessions, sessionID, s),
})
const res = yield* jobs.block<Output>({
  id: job.id, sessionID, yield_after: input.yield_after ?? 30_000,
})
if (res?.type === \"backgrounded\") {
  if (input.timeout === undefined) yield* shell.timeout(info.id, 0)
  return backgroundResult(info.id, info.file, yield* snapshot)
}
";

    let mut diff = RuneSurfaceActor::declare(
        &mut scene,
        "shell-diff",
        RuneSurfacePlan::diff_files(vec![RuneDiffFile {
            path: "packages/core/src/tool/plugin/shell.ts".into(),
            before: before.into(),
            after: after.into(),
        }])
        .titled("packages/core/src/tool/plugin/shell.ts")
        .box_size(880.0, 415.0)
        .scale_factor(1.72)
        .at([960.0, 530.0]),
    )?;
    diff.settle_in(&mut scene, millis(80));

    diff.commit(
        &mut scene,
        timeout_at,
        "highlight explicit timeout guard",
        RuneAction::SelectDiffLines {
            selection: Some(RuneDiffSelection {
                file: 0,
                new_side: true,
                start_line: 8,
                end_line: 10,
            }),
        },
    )?;

    diff.commit(
        &mut scene,
        settle_at,
        "annotate global onSettle",
        RuneAction::SetDiffAnnotations {
            annotations: vec![RuneDiffAnnotation {
                file: 0,
                new_side: true,
                line: 3,
                author: "Review #1 & #3".into(),
                body: "onSettle runs in global Job.Service scope; explicit timeouts stay armed after yield_after."
                    .into(),
            }],
        },
    )?;

    diff.commit(
        &mut scene,
        tools_at,
        "toggle to split diff",
        RuneAction::SetDiffMode {
            mode: RuneDiffMode::Split,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("tools.opencode.{", Tone::Muted),
            sp("job_list, job_output, job_wait, job_stop", Tone::Accent),
            sp("} · stopSubtree cancels nested child jobs", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, tools_at.saturating_sub(millis(300)));

    sfx::MARK.play(&mut scene, "sel-lines", timeout_at, -16.0);
    sfx::TICK.play(&mut scene, "note-pop", settle_at, -16.0);
    sfx::WHOOSH.play(&mut scene, "split-mode", tools_at, -16.5);

    scene.cue("rune-diff", 0, duration);
    Ok(scene.finish()?)
}
