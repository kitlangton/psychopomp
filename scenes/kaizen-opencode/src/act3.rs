use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    editor::diff::{Diff, add, keep, remove},
    ide::{CursorActor, CursorPlan, DiagnosticActor, DiagnosticPlan, HoverActor, HoverPlan},
    plan::ScenePlan,
    rune_surface::{RuneAction, RuneSurfaceActor, RuneSurfacePlan},
    sfx,
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 5: Real IDE Stepped `Diff` + `DiagnosticActor` + `HoverActor` + `InlayHint`
/// tracing the 5-Whys HiveMind 25-to-30 GiB deadband (`kitlangton/hivemind#36`).
pub fn five_whys(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("05-five-whys", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "live receipts · 2",
        "Genchi Genbutsu & 5 Whys: the 25–30 GiB deadband",
    )?;
    chip(&mut scene, "chip", Tone::Error, "70.2% queue wait")?;

    let deadband_at = spoken.at_any(&["deadband", "Five Whys", "five gigabyte"]);
    let twenty_five_at = spoken.at_any(&["twenty-five", "twenty five", "stopped evicting"]);
    let spill_at = spoken.at_any(&["one sixty-three", "sixty-three", "throttled", "spilled"]);
    let fix_at = spoken.at_any(&["thirty-five", "thirty five", "Raising"]);
    let cool_at = spoken.at_any(&["thirteen", "dropped", "core budgets"]);

    let diff = Diff {
        file_name: "hivemind/src/clean.rs",
        lines: vec![
            keep("pub fn default_min_free_gb_for_host(host: &str) -> f64 {"),
            keep("  if host.to_ascii_lowercase().contains(\"studio\") {"),
            remove(1, "    25.0 // < 30 GiB gate: clean reclaims 0.00 GiB x12")
                .range("deadband", "25.0"),
            add(1, "    35.0 // gate 30 + 5 GiB margin: zero deadband")
                .range("clamped", "35.0"),
            keep("  } else { 80.0 }"),
            keep("}"),
            keep(""),
            keep("let warm_cargo = has_warm_dd && demand.kind == CargoBuild")
                .range("cargo_kind", "CargoBuild")
                .inlay("spill_hint", "cargo_kind", " // load 163.5"),
            add(1, "  && node.thermal_pressure == 0 && node.cpu_busy_pct < 85.0;")
                .range("spill", "node.cpu_busy_pct < 85.0;"),
        ],
    };

    let editor = diff.declare(&mut scene, &[fix_at], 0, true)?;
    editor.target(&mut scene, "deadband", 2, "deadband")?;
    editor.target(&mut scene, "clamped", 3, "clamped")?;
    editor.target(&mut scene, "spill", 8, "spill")?;

    let spill_inlay = editor.inlay(&mut scene, "spill_hint");
    spill_inlay.show(&mut scene, spill_at.saturating_sub(millis(300)));
    spill_inlay.hide(&mut scene, fix_at);

    let caret = CursorActor::declare(
        &mut scene,
        "caret",
        &CursorPlan::new("deadband", "deadband")
            .anchor("clamped", "clamped")
            .anchor("spill", "spill"),
    )?;
    caret.show(&mut scene, lead + millis(250));

    let error = DiagnosticActor::declare(&mut scene, "err", &DiagnosticPlan::error("deadband"))?;
    error.show(&mut scene, deadband_at);

    let hover = HoverActor::declare(
        &mut scene,
        "why-card",
        &HoverPlan::new("deadband")
            .code("5 Whys · 321.3h queued vs 136.7h run (2,904 jobs)")
            .text(vec![
                sp("Studio gated < ", Tone::Plain),
                sp("30 GiB", Tone::Error),
                sp(" for 11.1h while clean slept at ", Tone::Plain),
                sp("25 GiB", Tone::Warning),
                sp(".", Tone::Plain),
            ])
            .text(vec![sp(
                "50 subagents spilled to pro (load 163.5, 46 exit-75 kills).",
                Tone::Muted,
            )])
            .below(),
    )?;
    hover.show(&mut scene, twenty_five_at);
    hover.hide(&mut scene, fix_at.saturating_sub(millis(220)));

    caret.move_to(&mut scene, "clamped", 1.0, fix_at + millis(420))?;
    error.clear(&mut scene, fix_at + millis(700));
    caret.hide(&mut scene, cool_at + millis(500));

    let mut resolved = CalloutActor::declare(
        &mut scene,
        "resolved",
        &CalloutPlan::new(
            CalloutAnchorPlan::Editor {
                id: "spill".into(),
                target: "spill".into(),
                edge: CalloutSide::Bottom,
                side: None,
            },
            vec![
                sp("Studio 26 → 63 GiB free ", Tone::Success),
                sp("· pro load 163.5 → 13.6", Tone::Plain),
            ],
        )
        .side(CalloutSide::Bottom)
        .reach(58.0)
        .tone(Tone::Success)
        .chip(),
    )?;
    resolved.show(&mut scene, cool_at);

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("2026-10-28-rsi-system-3.md · ", Tone::Muted),
            sp("kitlangton/hivemind#36", Tone::Accent),
            sp(" · nextest exit 100 | 101 fixed in same PR", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, fix_at);

    sfx::GLITCH.play(&mut scene, "deadband-err", deadband_at, -16.0);
    sfx::FAILURE.play(&mut scene, "spill-spike", spill_at, -16.0);
    sfx::CONFIRM.play(&mut scene, "fix-land", fix_at, -15.0);
    sfx::BLOOM.play(&mut scene, "load-cool", cool_at, -16.5);

    scene.cue("five-whys", 0, duration);
    Ok(scene.finish()?)
}

/// Segment 6: Live Seer card rendered on `wgpu` via `RuneSurfaceActor` (`RuneKind::Note`).
pub fn muda_kanban(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("06-muda-kanban", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "live receipts · 3",
        "Eliminating Muda & enforcing Kanban WIP limits",
    )?;
    chip(&mut scene, "chip", Tone::Success, "−30% $/turn · WIP ≤ 6")?;

    let blob_at = spoken.at_any(&["sequel lite", "blob", "nine hundred"]);
    let sonnet_at = spoken.at_any(&["blind replay", "Sonnet", "thirty-nine"]);
    let wip_at = spoken.at_any(&["caps active", "at six", "five-minute", "five minute"]);

    let initial_md = "\
Cut token, SQLite, and review waste first, then cap builders to match fleet slots.

```facts
Opus 200K compaction: {verified} $0.091 → $0.061 / turn (−30%, 1,599 ok)
SQLite media_blob: {verified} 914 → 1 row/day > 256 KB (16 → 0.3 MB)
Blind 24-run A/B: {verified} Sonnet 39% cost · Zen −97% console
```

```compare
Unbounded push: 30 builders queue > 10 min and expire Anthropic's 5m cache.
Kanban pull (seer wip <= 6): excess cards queue FIFO and wake as slots free up.
```";

    let updated_md = format!(
        "{initial_md}\n\n```steps\n{{built}} limit.input 444k → 235k · >200K spend 67.8% → 0.3%\n{{built}} Review diet −89% ($437 → $20/day, small-diff check kept)\n{{built}} seer wip --max 6 + seer land warm merge train (0.3s swap)\n```"
    );

    let mut note = RuneSurfaceActor::declare(
        &mut scene,
        "seer-ledger",
        RuneSurfacePlan::note(initial_md)
            .titled("t793 · Flow, Compaction & Kanban WIP Receipts")
            .width(650.0)
            .scale_factor(1.56)
            .at([960.0, 520.0]),
    )?;
    note.settle_in(&mut scene, millis(80));
    note.commit(
        &mut scene,
        wip_at,
        "reveal verified steps",
        RuneAction::SetMarkdown {
            markdown: updated_md,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("compaction-200k-review · ", Tone::Muted),
            sp("Little's Law: L = λW", Tone::Accent),
            sp(" · seer_wip.py keeps Cargo inside 5m cache TTL", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, sonnet_at);

    sfx::TICK.play(&mut scene, "tick-blob", blob_at, -16.5);
    sfx::MARK.play(&mut scene, "mark-sonnet", sonnet_at, -16.0);
    sfx::CONFIRM.play(&mut scene, "commit-steps", wip_at, -15.0);

    scene.cue("muda-kanban", 0, duration);
    Ok(scene.finish()?)
}
