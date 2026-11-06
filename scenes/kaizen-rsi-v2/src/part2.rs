use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    editor::diff::{Diff, add, keep, remove},
    grid::{
        GRID_RECIPE, GridAlignment, GridArrangement, GridAxisPlan, GridCellLabelPlan,
        GridEventPlan, GridFillPlan, GridRecipePlan, GridRules, GridSnapshotPlan, GridStylePlan,
    },
    ide::{CursorActor, CursorPlan, DiagnosticActor, DiagnosticPlan, HoverActor, HoverPlan},
    plan::ScenePlan,
    sfx,
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 3: Verbatim code fix in `hivemind/src/cluster.rs:2460`.
pub fn livelock_diff(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("03-livelock-diff", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "fix 1 · code receipt",
        "Filter ahead-waiters by core fit in hivemind/src/cluster.rs",
    )?;
    chip(&mut scene, "chip", Tone::Accent, "line 2460")?;

    let bug_at = spoken.at_any(&["waiters ahead", "counts every", "twenty-four sixty"]);
    let fix_at = spoken.at_any(&["Filtering", "candidate worker", "draining"]);
    let res_at = spoken.at_any(&["fifteen minutes", "under two", "unblocks"]);

    let diff = Diff {
        file_name: "hivemind/src/cluster.rs",
        lines: vec![
            keep("for (_, cand_name, cand_is_warm, cand_free_slots) in candidates {"),
            remove(1, "  let ahead = waiters_ahead_of(ticket, &waiters, Some(&cand_name));")
                .range("bug_call", "waiters_ahead_of"),
            add(1, "  let ahead = fitting_waiters_ahead(ticket, &waiters, &cand_name, st);")
                .range("fix_call", "fitting_waiters_ahead"),
            keep("  if ahead >= cand_free_slots {"),
            keep("    yielded_reason = Some(format!(\"yielding to {ahead} on {cand_name}\"));"),
            keep("    continue;"),
            keep("  }"),
            add(1, "  if reserved_for_high_drain(&cand_name, &waiters) { continue; }")
                .range("drain_line", "reserved_for_high_drain"),
            keep("  return Ok(cand_name);"),
            keep("}"),
        ],
    };

    let editor = diff.declare(&mut scene, &[fix_at], 0, true)?;
    editor.target(&mut scene, "bug_call", 1, "bug_call")?;
    editor.target(&mut scene, "fix_call", 2, "fix_call")?;
    editor.target(&mut scene, "drain_line", 7, "drain_line")?;

    let caret = CursorActor::declare(
        &mut scene,
        "caret",
        &CursorPlan::new("bug_call", "bug_call")
            .anchor("fix_call", "fix_call")
            .anchor("drain_line", "drain_line"),
    )?;
    caret.show(&mut scene, lead + millis(200));

    let error = DiagnosticActor::declare(&mut scene, "err", &DiagnosticPlan::error("bug_call"))?;
    error.show(&mut scene, bug_at);

    let hover = HoverActor::declare(
        &mut scene,
        "why-card",
        &HoverPlan::new("bug_call")
            .code("cluster.rs:2460 · blind slot yielding")
            .text(vec![
                sp("Counts ", Tone::Plain),
                sp("7-core", Tone::Error),
                sp(" waiters against ", Tone::Plain),
                sp("mini (2 free cores)", Tone::Success),
                sp(".", Tone::Plain),
            ])
            .text(vec![sp(
                "9 normal 2-core jobs yield forever while mini's slot sits idle.",
                Tone::Muted,
            )])
            .below(),
    )?;
    hover.show(&mut scene, bug_at + millis(500));
    hover.hide(&mut scene, fix_at.saturating_sub(millis(220)));

    caret.move_to(&mut scene, "fix_call", 1.0, fix_at + millis(400))?;
    error.clear(&mut scene, fix_at + millis(650));
    caret.hide(&mut scene, res_at + millis(500));

    let mut resolved = CalloutActor::declare(
        &mut scene,
        "resolved",
        &CalloutPlan::new(
            CalloutAnchorPlan::Editor {
                id: "drain_line".into(),
                target: "drain_line".into(),
                edge: CalloutSide::Bottom,
                side: None,
            },
            vec![
                sp("Cargo queue p95: 15+ min → < 2 min ", Tone::Success),
                sp("· seer land + Hex unblocked", Tone::Plain),
            ],
        )
        .side(CalloutSide::Bottom)
        .reach(96.0)
        .tone(Tone::Success)
        .chip(),
    )?;
    resolved.show(&mut scene, res_at);

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("hivemind/src/cluster.rs:2460 · ", Tone::Muted),
            sp("2-line filter + 120s high-priority drain", Tone::Accent),
        ],
    )?;
    foot.show(&mut scene, fix_at);

    sfx::GLITCH.play(&mut scene, "err-buzz", bug_at, -16.0);
    sfx::CONFIRM.play(&mut scene, "diff-step", fix_at, -15.0);
    sfx::BLOOM.play(&mut scene, "res-bloom", res_at, -16.0);

    scene.cue("livelock-diff", 0, duration);
    Ok(scene.finish()?)
}

/// Segment 4: Procedural `Grid` Banded Table ↔ 3D `Layers` auditing ~30 GB of
/// idle/leaked processes across Studio + Pro and 1.7 GB/day Motel OTLP churn.
pub fn reclaim_30gb(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("04-reclaim-30gb", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "fix 2 · ~30 gb",
        "Sweep leaked daemons and 6.56M Motel spans/day",
    )?;
    chip(&mut scene, "chip", Tone::Warning, "8.9G swap · pro thrm 2")?;

    let workerd_at = spoken.at_any(&["thirteen", "workerd", "orphaned"]);
    let spot_at = spoken.at_any(&["twelve CPU", "Spotlight", "indexing"]);
    let pro_at = spoken.at_any(&["ninety-six", "MacBook Pro", "thermally"]);
    let motel_at = spoken.at_any(&["six point five", "Motel", "file-system"]);
    let sweep_at = spoken.at_any(&["Sweeping", "thirty megabytes", "six fast"]);

    let rows = [
        [
            "Studio · cua-driver + Codex",
            "3.56 GB idle RAM (up 3d)",
            "Quit + build <15 MB Rust mac-ax",
        ],
        [
            "Studio · 13 workerd + 5 nexus",
            "4.30 GB leaked (PPID 1)",
            "Process-group reap in Rust devd",
        ],
        [
            "Studio · Spotlight + Audio",
            "12.3 CPU-h indexing trees",
            "Add .metadata_never_index roots",
        ],
        [
            "Pro · VM + spaceman + ng",
            "24 GB + thermal level 2",
            "Stop leaks → +6 fast Cargo slots",
        ],
        [
            "Motel · FileSystem.resolve",
            "6.56M spans (1.7 GB/day)",
            "Drop <10ms FS spans → 30 MB/day",
        ],
    ];

    let mut style = GridStylePlan::plain_table(vec![390.0, 430.0, 620.0]);
    style.fill = GridFillPlan::Banded {
        color: [22, 24, 29],
    };
    style.rules = GridRules::Rows;
    if let Some(table) = style.table.as_mut() {
        table.row_height = 76.0;
        table.font_size = 22.0;
        table.padding = 22.0;
        table.headers = vec![
            "Host & Culprit".into(),
            "Live Cost (17:22 PT)".into(),
            "Countermeasure".into(),
        ];
        table.alignments = vec![
            GridAlignment::Left,
            GridAlignment::Left,
            GridAlignment::Left,
        ];
    }

    let mut labels = Vec::new();
    for (r, cols) in rows.iter().enumerate() {
        for (c, text) in cols.iter().enumerate() {
            labels.push(GridCellLabelPlan {
                indices: [c, r, 0],
                primary: (*text).into(),
                secondary: String::new(),
            });
        }
    }

    let snap = |r: usize, arrangement: GridArrangement| GridSnapshotPlan {
        visible: [3, r, 1],
        arrangement,
        focus_slice: None,
    };

    let recipe = GridRecipePlan {
        axes: [
            GridAxisPlan {
                name: "Col".into(),
                values: vec!["C1".into(), "C2".into(), "C3".into()],
            },
            GridAxisPlan {
                name: "Row".into(),
                values: vec![
                    "Idle".into(),
                    "Orphan".into(),
                    "Index".into(),
                    "Pro".into(),
                    "Motel".into(),
                ],
            },
            GridAxisPlan {
                name: "Layer".into(),
                values: vec!["Host".into()],
            },
        ],
        labels,
        initial: snap(1, GridArrangement::Table),
        events: vec![
            GridEventPlan {
                at_nanos: workerd_at,
                snapshot: snap(2, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: spot_at,
                snapshot: snap(3, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: pro_at,
                snapshot: snap(4, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: motel_at,
                snapshot: snap(5, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: sweep_at,
                snapshot: snap(5, GridArrangement::Layers),
            },
            GridEventPlan {
                at_nanos: (sweep_at + millis(2400)).min(duration.saturating_sub(millis(900))),
                snapshot: snap(5, GridArrangement::Table),
            },
        ],
        style: Some(style),
    };

    scene.actor("leak-grid", GRID_RECIPE, recipe)?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("Studio swap 8.88 GB → < 2 GB · ", Tone::Success),
            sp("Motel −98% disk writes", Tone::Accent),
            sp(" · Pro thermal gate cleared", Tone::Plain),
        ],
    )?;
    foot.show(&mut scene, sweep_at);

    for (i, at) in [workerd_at, spot_at, pro_at, motel_at]
        .into_iter()
        .enumerate()
    {
        sfx::TICK.play(&mut scene, format!("leak-tick-{i}"), at, -16.5);
    }
    sfx::WHOOSH.play(&mut scene, "sweep-tilt", sweep_at, -16.0);

    scene.cue("reclaim-30gb", 0, duration);
    Ok(scene.finish()?)
}
