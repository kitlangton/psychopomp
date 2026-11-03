use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    editor::diff::{Diff, add, keep, remove},
    ide::{CursorActor, CursorPlan, DiagnosticActor, DiagnosticPlan, HoverActor, HoverPlan},
    plan::ScenePlan,
    sfx,
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 3: Real IDE Stepped `Diff` + `CursorActor` + `DiagnosticActor` +
/// `HoverActor` + `InlayHint` on `packages/core/src/job.ts` (`Job.block`).
pub fn job_race(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("03-job-race", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "job.ts",
        "Steer guard & post-race lock in Job.block",
    )?;
    chip(&mut scene, "chip", Tone::Request, "core/src/job.ts")?;

    let steer_at = spoken.at_any(&["has user steer", "tool JSON", "streaming"]);
    let race_at = spoken.at_any(&["independent review", "same tick", "races against"]);
    let lock_at = spoken.at_any(&["is backgrounded", "registry lock", "Checking"]);
    let once_at = spoken.at_any(&["duplicate notice", "one owner", "never gets"]);

    let diff = Diff {
        file_name: "packages/core/src/job.ts",
        lines: vec![
            keep("if (yield* hasUserSteer(db, input.sessionID))")
                .range("steer_guard", "hasUserSteer")
                .inlay("dekker_hint", "steer_guard", " // streaming steer"),
            keep("  yield* backgroundAll({ sessionID: input.sessionID })"),
            remove(
                1,
                "const settled = yield* Effect.raceFirst(done, backgrounded)",
            )
            .range("old_race", "Effect.raceFirst(done, backgrounded)"),
            remove(1, "return { type: \"finished\", info: settled }"),
            add(1, "const settled = yield* Effect.raceFirst("),
            add(1, "  Effect.raceFirst(done, backgrounded), waitYield,"),
            add(1, ")"),
            add(
                1,
                "return yield* SynchronizedRef.modify(state.jobs, (jobs) => {",
            )
            .range("lock_mod", "SynchronizedRef.modify"),
            add(1, "  const job = jobs.get(input.id)"),
            add(1, "  if (job?.isBackgrounded)").range("bg_owner", "job?.isBackgrounded"),
            add(
                1,
                "    return [{ type: \"backgrounded\", info: snap(job) }, jobs]",
            ),
            add(
                1,
                "  return [{ type: \"finished\", info: settled }, consume(jobs)]",
            ),
            add(1, "})"),
        ],
    };

    let editor = diff.declare(&mut scene, &[lock_at], 0, true)?;
    editor.target(&mut scene, "steer_guard", 0, "steer_guard")?;
    editor.target(&mut scene, "old_race", 2, "old_race")?;
    editor.target(&mut scene, "lock_mod", 7, "lock_mod")?;
    editor.target(&mut scene, "bg_owner", 9, "bg_owner")?;

    let dekker_inlay = editor.inlay(&mut scene, "dekker_hint");
    dekker_inlay.show(&mut scene, steer_at);
    dekker_inlay.hide(&mut scene, lock_at);

    let caret = CursorActor::declare(
        &mut scene,
        "caret",
        &CursorPlan::new("steer_guard", "steer_guard")
            .anchor("old_race", "old_race")
            .anchor("bg_owner", "bg_owner"),
    )?;
    caret.show(&mut scene, lead + millis(200));
    caret.move_to(
        &mut scene,
        "old_race",
        1.0,
        race_at.saturating_sub(millis(200)),
    )?;

    let error = DiagnosticActor::declare(&mut scene, "err", &DiagnosticPlan::error("old_race"))?;
    error.show(&mut scene, race_at);

    let hover = HoverActor::declare(
        &mut scene,
        "race-hover",
        &HoverPlan::new("old_race")
            .code("Review Finding #2 · simultaneous yield_after & done")
            .text(vec![
                sp("If ", Tone::Plain),
                sp("yieldRunning", Tone::Warning),
                sp(" commits ", Tone::Plain),
                sp("isBackgrounded = true", Tone::Error),
                sp(" in the same tick ", Tone::Plain),
                sp("done", Tone::Accent),
                sp(" resolves,", Tone::Plain),
            ])
            .text(vec![sp(
                "returning 'finished' inline would also fire onSettle twice.",
                Tone::Muted,
            )])
            .below(),
    )?;
    hover.show(&mut scene, race_at + millis(250));
    hover.hide(&mut scene, lock_at.saturating_sub(millis(200)));

    caret.move_to(&mut scene, "bg_owner", 1.0, lock_at + millis(420))?;
    error.clear(&mut scene, lock_at + millis(650));
    caret.hide(&mut scene, once_at + millis(600));

    let mut resolved = CalloutActor::declare(
        &mut scene,
        "resolved",
        &CalloutPlan::new(
            CalloutAnchorPlan::Editor {
                id: "bg_owner".into(),
                target: "bg_owner".into(),
                edge: CalloutSide::Bottom,
                side: None,
            },
            vec![
                sp("1 owner under state.jobs lock ", Tone::Success),
                sp("· 0 duplicate notices or leaked KV markers", Tone::Plain),
            ],
        )
        .side(CalloutSide::Bottom)
        .reach(52.0)
        .tone(Tone::Success)
        .chip(),
    )?;
    resolved.show(&mut scene, once_at);

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("packages/core/src/job.ts:621–648 · ", Tone::Muted),
            sp("job.test.ts", Tone::Accent),
            sp(" · simultaneous yield & settle stress-tested", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, lock_at);

    sfx::TICK.play(&mut scene, "dekker-tick", steer_at, -16.5);
    sfx::GLITCH.play(&mut scene, "race-warn", race_at, -16.0);
    sfx::CONFIRM.play(&mut scene, "lock-fix", lock_at, -15.0);
    sfx::BLOOM.play(&mut scene, "once-ok", once_at, -16.5);

    scene.cue("job-race", 0, duration);
    Ok(scene.finish()?)
}
