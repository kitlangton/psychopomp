use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    caption::CaptionSpanPlan,
    checklist::{ChecklistActor, ChecklistItemPlan, ChecklistPlan, Outcome},
    chrome::{chip, footer, header},
    effects::spinner::Mark,
    grid::{
        GRID_RECIPE, GridAlignment, GridArrangement, GridAxisPlan, GridCellLabelPlan,
        GridEventPlan, GridFillPlan, GridRecipePlan, GridRules, GridSnapshotPlan, GridStylePlan,
    },
    plan::ScenePlan,
    sfx,
    terminal::{TerminalActor, TerminalLinePlan, TerminalPlan},
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 3: Procedural `Grid` Banded Table revealing all 5 TPS mechanics and
/// pivoting smoothly into 3D `GridArrangement::Layers`.
pub fn techniques(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("03-techniques", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "core mechanics",
        "Five loops that turn a factory into an experiment",
    )?;
    chip(&mut scene, "chip", Tone::Request, "TPS · Shingo · Rother")?;

    let kanban_at = spoken.at_any(&["Kanban", "work in progress", "heijunka"]);
    let genchi_at = spoken.at_any(&["genchi", "raw traces", "Five Whys"]);
    let poka_at = spoken.at_any(&["poka-yoke", "poka", "physical guard"]);
    let kata_at = spoken.at_any(&["Standard work", "Standard", "baseline"]);
    let layers_at = spoken.at_any(&["Improvement Kata", "Rother", "Deming"]);

    let rows = [
        [
            "3Ms · Muda / Mura / Muri",
            "Unevenness & overload create waste",
            "Measure p50/p95 wait; keep load < 85%",
        ],
        [
            "Kanban & Heijunka",
            "Pull cards cap WIP; level the mix",
            "Cap builders to cache TTL; budget cores",
        ],
        [
            "Genchi Genbutsu & 5 Whys",
            "Go see the floor; ask Why 5 times",
            "Read raw traces & SQLite; cite file:line",
        ],
        [
            "Poka-Yoke (Shingo 1961)",
            "Prevent > detect > scold workers",
            "Shell & git hooks block bad states",
        ],
        [
            "Standard Work & Kata",
            "Baseline + 1 PDCA prediction/step",
            "SKILL.md + read-only benchmark gate",
        ],
    ];

    let mut style = GridStylePlan::plain_table(vec![360.0, 520.0, 560.0]);
    style.fill = GridFillPlan::Banded {
        color: [22, 24, 29],
    };
    style.rules = GridRules::Rows;
    if let Some(table) = style.table.as_mut() {
        table.row_height = 76.0;
        table.font_size = 22.0;
        table.padding = 22.0;
        table.headers = vec![
            "Technique".into(),
            "Shop-Floor Mechanism".into(),
            "Agent Harness Equivalent".into(),
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
                    "3Ms".into(),
                    "Kanban".into(),
                    "Genchi".into(),
                    "Poka".into(),
                    "Kata".into(),
                ],
            },
            GridAxisPlan {
                name: "Layer".into(),
                values: vec!["TPS".into()],
            },
        ],
        labels,
        initial: snap(1, GridArrangement::Table),
        events: vec![
            GridEventPlan {
                at_nanos: kanban_at,
                snapshot: snap(2, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: genchi_at,
                snapshot: snap(3, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: poka_at,
                snapshot: snap(4, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: kata_at,
                snapshot: snap(5, GridArrangement::Table),
            },
            GridEventPlan {
                at_nanos: layers_at,
                snapshot: snap(5, GridArrangement::Layers),
            },
            GridEventPlan {
                at_nanos: (layers_at + millis(2600)).min(duration.saturating_sub(millis(900))),
                snapshot: snap(5, GridArrangement::Table),
            },
        ],
        style: Some(style),
    };

    scene.actor("mechanics-grid", GRID_RECIPE, recipe)?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("Liker (2004) · ", Tone::Plain),
            sp("prevent > detect > instruct", Tone::Accent),
            sp(" · Rother's Kata & Deming's PDSA", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, poka_at);

    for (i, at) in [kanban_at, genchi_at, poka_at, kata_at]
        .into_iter()
        .enumerate()
    {
        sfx::TICK.play(&mut scene, format!("row-tick-{i}"), at, -16.5);
    }
    sfx::WHOOSH.play(&mut scene, "layers-tilt", layers_at, -16.0);

    scene.cue("techniques", 0, duration);
    Ok(scene.finish()?)
}

/// Segment 4: Live `TerminalActor` + `ChecklistActor` rail showing Jidoka & Poka-Yoke.
pub fn andon_poka(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("04-andon-poka", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "live receipts · 1",
        "Stopping the line in my local agent fleet",
    )?;
    chip(&mut scene, "chip", Tone::Success, "studio + seer")?;

    let second_srv = spoken.at_any(&["second OpenCode", "four times", "inherited"]);
    let guard_at = spoken.at_any(&["shell guard", "refusing", "standalone"]);
    let seer_red = spoken.at_any(&["thirty Seer", "hundred and fifty", "broken main"]);
    let andon_at = spoken.at_any(&["seer andon", "blocks agent", "fifteen minutes"]);
    let drop_at = spoken.at_any(&["sixty-one", "eleven", "dropped"]);
    let rustc_at = spoken.at_any(&["rustc", "ninety-nine", "token gate"]);

    let mut term_plan = TerminalPlan::new([140.0, 195.0], 920.0, 13)
        .size(19.0)
        .titled("studio · ~/code/open-source/seer")
        .prompt(vec![
            sp("seer ", Tone::Accent),
            sp("❯ ", Tone::Muted),
        ]);
    // Frame 0 alive: initial status line already printed in the terminal window.
    term_plan.lines.push(TerminalLinePlan::Output {
        id: "boot".into(),
        spans: vec![
            sp("[studio] ", Tone::Accent),
            sp("OpenCode Beta :4096 active · single-writer DB", Tone::Muted),
        ],
    });

    let mut term = TerminalActor::declare(&mut scene, "term", term_plan)?;

    let enter1 = term.type_at(
        &mut scene,
        second_srv.saturating_sub(millis(300)),
        "opencode serve --standalone",
        24.0,
    )?;
    let out1_at = enter1.max(guard_at.saturating_sub(millis(200)));
    term.print(
        &mut scene,
        out1_at,
        vec![
            vec![
                sp("studio-guard: ", Tone::Error),
                sp("'--standalone' refused (exit 1)", Tone::Plain),
            ],
            vec![sp(
                "  opencode.db stays single-writer (4/night → 0)",
                Tone::Muted,
            )],
        ],
    )?;

    let enter2 = term.type_at(
        &mut scene,
        seer_red.max(out1_at + millis(600)),
        "git push origin main",
        24.0,
    )?;
    let out2_at = enter2.max(andon_at.saturating_sub(millis(150)));
    term.print(
        &mut scene,
        out2_at,
        vec![
            vec![
                sp("seer-andon: ", Tone::Warning),
                sp("main red 18m (blamed 1b737295)", Tone::Plain),
            ],
            vec![
                sp("  known red: 10 · yours: 1 ", Tone::Error),
                sp("— push refused until blamed test passes", Tone::Muted),
            ],
        ],
    )?;

    let spin_start = drop_at.saturating_sub(millis(900)).max(out2_at + millis(500));
    let task_id = term.spin(
        &mut scene,
        spin_start,
        vec![sp("seer-main-watch verifying blamed test fix...", Tone::Request)],
    )?;
    term.resolve(
        &mut scene,
        &task_id,
        drop_at + millis(350),
        Mark::Check,
        vec![
            sp("blamed test green ", Tone::Success),
            sp("· inherited red 61% → 11%", Tone::Plain),
        ],
    )?;

    term.print(
        &mut scene,
        rustc_at,
        vec![vec![
            sp("rustc-gate: ", Tone::Success),
            sp("bypass compiles 2,111/day → 8/day (−99.6%)", Tone::Plain),
        ]],
    )?;

    let check_plan = ChecklistPlan::new([1105.0, 215.0], 665.0)
        .size(24.0)
        .title("poka-yoke & andon")
        .rail()
        .panel()
        .item(ChecklistItemPlan::new("guard", "studio-guard").result("0 DB splits"))
        .item(ChecklistItemPlan::new("andon", "seer-andon hook").result("61% → 11%"))
        .item(ChecklistItemPlan::new("rustc", "rustc token gate").result("−99.6%"))
        .item(ChecklistItemPlan::new("alerts", "alerts storm rule").result("load 506"));

    let mut checks = ChecklistActor::declare(&mut scene, "checks", &check_plan)?;
    checks.show(&mut scene, lead + millis(150));
    checks.reveal(&mut scene, lead + millis(350))?;

    checks.start(&mut scene, "guard", second_srv)?;
    checks.resolve(&mut scene, "guard", out1_at + millis(250), Outcome::Done)?;

    checks.start(&mut scene, "andon", seer_red)?;
    checks.resolve(&mut scene, "andon", drop_at + millis(400), Outcome::Done)?;

    checks.start(&mut scene, "rustc", rustc_at.saturating_sub(millis(500)))?;
    checks.resolve(&mut scene, "rustc", rustc_at + millis(600), Outcome::Done)?;

    checks.start(&mut scene, "alerts", rustc_at + millis(200))?;
    checks.resolve(&mut scene, "alerts", rustc_at + millis(1100), Outcome::Done)?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("note 1ce6a2f6 · ", Tone::Muted),
            sp("seer/docs/red-main.md", Tone::Accent),
            sp(" · kitlangton/hivemind#28", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, andon_at);

    sfx::FAILURE.play(&mut scene, "refuse-1", out1_at, -16.0);
    sfx::FAILURE.play(&mut scene, "refuse-2", out2_at, -16.0);
    sfx::CONFIRM.play(&mut scene, "green-andon", drop_at + millis(350), -15.0);
    sfx::MARK.play(&mut scene, "green-rustc", rustc_at + millis(600), -16.0);

    scene.cue("andon-poka", 0, duration);
    Ok(scene.finish()?)
}
