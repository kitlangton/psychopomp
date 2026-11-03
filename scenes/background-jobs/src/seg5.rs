use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    plan::ScenePlan,
    rune_surface::{RuneAction, RuneSurfaceActor, RuneSurfacePlan},
    sfx,
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 5: Live Seer card rendered on `wgpu` via `RuneSurfaceActor` (`RuneKind::Note`),
/// showing `facts`, `compare`, and verified `{built}` `steps` for `#60367` + `#60375`.
pub fn seer_card(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1600);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("05-seer-card", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "stack ready",
        "Honest spinners & verified PR stack",
    )?;
    chip(&mut scene, "chip", Tone::Success, "21/21 CI · 260× stress")?;

    let ci_at = spoken.at_any(&["Six oh three", "six oh three", "twenty-one"]);
    let stack_at = spoken.at_any(&["two hundred and sixty", "stress-test", "sits right"]);

    let initial_md = "\
Sessions stay busy until both foreground turns and background jobs settle.

```facts
PR #60367 (t901): {verified} +867 / −334 · 21/21 GitHub Actions green
PR #60375 (t900): {verified} 30s yield_after · 260/260 randomized runs
Agent controls: {verified} job_list · job_output · job_wait · job_stop
```

```compare
Before on dev: 2m SIGTERM kill, web-only steer, and idle spinners while jobs run.
After (#60367 + #60375): Server steer + 30s yield_after + global onSettle + /jobs SSE.
```";

    let updated_md = format!(
        "{initial_md}\n\n```steps\n{{built}} #60367 · Server steer-to-background + delete app glue\n{{built}} #60375 · 30s yield_after on shell, execute & subagent\n{{built}} All 5 independent review findings fixed with regression tests\n```"
    );

    let mut note = RuneSurfaceActor::declare(
        &mut scene,
        "seer-summary",
        RuneSurfacePlan::note(initial_md)
            .titled("t900 & t901 · Background Jobs Stack on dev")
            .width(650.0)
            .scale_factor(1.56)
            .at([960.0, 520.0]),
    )?;
    note.settle_in(&mut scene, millis(80));
    note.commit(
        &mut scene,
        ci_at,
        "reveal verified PR stack",
        RuneAction::SetMarkdown {
            markdown: updated_md,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("GET /api/experimental/session/:id/jobs · ", Tone::Muted),
            sp("#60367", Tone::Accent),
            sp(" + ", Tone::Muted),
            sp("#60375", Tone::Success),
            sp(" targeting anomalyco/opencode dev", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, ci_at);

    sfx::MARK.play(&mut scene, "ci-mark", ci_at, -16.0);
    sfx::CONFIRM.play(&mut scene, "stack-reveal", stack_at, -15.0);

    scene.cue("seer-card", 0, duration);
    Ok(scene.finish()?)
}
