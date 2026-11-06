use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    axis::AxisPlan,
    bars::{BarSeriesPlan, BarsActor, BarsPlan},
    caption::{CaptionActor, CaptionPlan, CaptionSpanPlan},
    chrome::{chip, footer, header},
    math::easing::Ease,
    plan::ScenePlan,
    readout::ReadoutFormat,
    rolling::{RollPlan, RollingNumberActor, RollingNumberPlan},
    sfx,
    stage::{Arrow, Curve, Fill, Material, StageActor, StageElement, StagePlan, StagePost, Waypoint},
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 1: The Factory Inversion (`RollingNumberActor` + `BarsActor`).
pub fn inversion(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("01-inversion", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "rsi · oct 22–29",
        "The inversion: production outran verification",
    )?;
    chip(&mut scene, "chip", Tone::Warning, "68h desk trace")?;

    let one_five = spoken.at_any(&["one point five", "one point", "running agents"]);
    let sixty_three = spoken.at_any(&["sixty-three", "sixty three", "waiting on me"]);
    let rework_at = spoken.at_any(&["forty-six", "forty six", "Seer cards"]);
    let cargo_at = spoken.at_any(&["sixty-four", "sixty four", "Cargo queue"]);

    CaptionActor::declare(
        &mut scene,
        "kicker",
        &CaptionPlan::line(
            [150.0, 268.0],
            20.0,
            vec![sp("DESK HOURS WITH FINISHED WORK WAITING", Tone::Muted)],
        ),
    )?;

    let rolling_plan = RollingNumberPlan {
        origin: [150.0, 358.0],
        align: Default::default(),
        size: 98.0,
        bold: true,
        tone: Tone::Accent,
        prefix: Vec::new(),
        suffix: vec![sp("%", Tone::Accent)],
        value: "1.5".into(),
        rolls: vec![RollPlan {
            at_nanos: sixty_three,
            value: "63.0".into(),
        }],
        duration_nanos: 650_000_000,
        stagger: Default::default(),
        direction: Default::default(),
        blur: 1.0,
        chip: false,
        anchors: Vec::new(),
    };
    RollingNumberActor::declare(&mut scene, "wait-pct", rolling_plan)?;

    CaptionActor::declare(
        &mut scene,
        "scale-line",
        &CaptionPlan::line(
            [150.0, 456.0],
            21.0,
            vec![
                sp("43 of 68h", Tone::Plain),
                sp(" · 2,038 human · 2,798 subagents", Tone::Muted),
            ],
        ),
    )?;

    let mut pile_chip = CaptionActor::declare(
        &mut scene,
        "pile-chip",
        &CaptionPlan::line(
            [150.0, 560.0],
            22.0,
            vec![
                sp("p90 = 7 sessions ", Tone::Error),
                sp("backed up on Kit at once", Tone::Plain),
            ],
        )
        .glass(),
    )?;
    pile_chip.show(&mut scene, sixty_three + millis(350));

    let mut switch_chip = CaptionActor::declare(
        &mut scene,
        "switch-chip",
        &CaptionPlan::line(
            [150.0, 642.0],
            22.0,
            vec![
                sp("51% turn switches ", Tone::Warning),
                sp("· median 41s apart", Tone::Muted),
            ],
        )
        .glass(),
    )?;
    switch_chip.show(&mut scene, rework_at);

    let bars_plan = BarsPlan::new(
        [1120.0, 275.0],
        570.0,
        AxisPlan::new([0.0, 70.0])
            .every(20.0)
            .unit("%")
            .label("measured 7-day share (%)"),
    )
    .row_height(106.0)
    .size(22.0)
    .series(BarSeriesPlan::new("pct", "measured %", Tone::Accent))
    .readout(ReadoutFormat::new(1).unit("%"))
    .row("agent-wait", "Kit waits on agents")
    .row("rework", "Seer 2+ round cards")
    .row("kit-wait", "Work waits on Kit")
    .row("cargo-q", "HiveMind queue share");

    let mut bars = BarsActor::declare(&mut scene, "inv-bars", &bars_plan)?;
    bars.channel(&mut scene, "axes", 1.0);
    bars.reveal_rows(&mut scene, lead + millis(150));
    bars.set(&mut scene, "agent-wait", "pct", one_five, 1.5)?;
    bars.set(&mut scene, "kit-wait", "pct", sixty_three, 63.0)?;
    bars.set(&mut scene, "rework", "pct", rework_at, 46.3)?;
    bars.set(&mut scene, "cargo-q", "pct", cargo_at, 64.3)?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("2026-10-29-rsi-engineering · ", Tone::Muted),
            sp("523.9h queued", Tone::Error),
            sp(" vs 290.5h run across 8,000 HiveMind jobs", Tone::Plain),
        ],
    )?;
    foot.show(&mut scene, cargo_at);

    sfx::TICK.play(&mut scene, "tick-15", one_five, -16.5);
    sfx::MARK.play(&mut scene, "roll-63", sixty_three, -15.0);
    sfx::FAILURE.play(&mut scene, "warn-rework", rework_at, -16.5);
    sfx::IMPACT.play(&mut scene, "hit-cargo", cargo_at, -15.5);

    scene.cue("inversion", 0, duration);
    Ok(scene.finish()?)
}

/// Segment 2: Physical 10-Core `mini` Worker & Head-of-Line Queue Livelock Rig.
pub fn livelock_rig(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("02-livelock-rig", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "fix 1 · hivemind",
        "The 47-minute head-of-line core-budget livelock",
    )?;
    chip(&mut scene, "chip", Tone::Error, "cluster.rs:2460")?;

    let stalled_at = spoken.at_any(&["seven cores", "forty-seven", "stalled"]);
    let mini_at = spoken.at_any(&["Mac mini", "two free cores", "free slot"]);
    let yield_at = spoken.at_any(&["yielded", "ahead of it", "could not fit"]);
    let clear_at = spoken.at_any(&["Reserving", "smaller jobs", "clears"]);

    let mut elements = vec![
        // Top worker bay: `mini` with 10 physical core blocks (`8` busy, `2` free).
        StageElement::rect("mini-bay", [960.0, 268.0, 3.0], [1460.0, 152.0])
            .corner(18.0)
            .fill(Some(Fill::Material(Material::Well))),
        StageElement::label(
            "mini-title",
            [960.0, 214.0, 0.0],
            19.0,
            &[
                ("WORKER mini · ", Tone::Plain),
                ("8 / 10 CORES CHARGED (glyph-cache 28m)  ·  ", Tone::Warning),
                ("2 CORES + 1 SLOT FREE", Tone::Success),
            ],
        ),
    ];

    for i in 0..10 {
        let x = 352.0 + i as f32 * 135.0;
        let is_free = i >= 8;
        elements.push(
            StageElement::rect(
                &format!("core-{i}"),
                [x, 286.0, 1.0],
                [112.0, 62.0],
            )
            .corner(10.0)
            .fill(Some(Fill::Tone(if is_free {
                Tone::Success
            } else {
                Tone::Warning
            })))
            .fill_opacity(if is_free { 0.22 } else { 0.26 }),
        );
        elements.push(StageElement::label(
            &format!("core-lbl-{i}"),
            [x, 286.0, 0.0],
            16.0,
            &[(
                if is_free { "FREE" } else { "BUSY" },
                if is_free { Tone::Success } else { Tone::Warning },
            )],
        ));
    }

    // Queue lanes below:
    // Lane A (`y = 485`): `[high] perf-init-and-poll · needs 7 cores` (cannot fit 2 free cores).
    // Lane B (`y = 655`): `9 × [normal] 2-core Cargo jobs` (fits 2 free cores, blocked by blind yield!).
    elements.extend([
        StageElement::rect("big-job", [510.0, 488.0, 0.0], [640.0, 92.0])
            .corner(14.0)
            .fill(Some(Fill::Material(Material::Raised))),
        StageElement::label(
            "big-txt",
            [510.0, 474.0, -1.0],
            20.0,
            &[
                ("[high] perf-init-and-poll  ", Tone::Error),
                ("needs 7 cores", Tone::Plain),
            ],
        ),
        StageElement::label(
            "big-sub",
            [510.0, 506.0, -1.0],
            17.0,
            &[("stalled 47m 23s — 7 cores > 2 free on mini", Tone::Muted)],
        ),
        StageElement::rect("small-job", [510.0, 656.0, 0.0], [640.0, 92.0])
            .corner(14.0)
            .fill(Some(Fill::Material(Material::Raised))),
        StageElement::label(
            "small-txt",
            [510.0, 642.0, -1.0],
            20.0,
            &[
                ("9 × [normal] Cargo checks  ", Tone::Request),
                ("need 2 cores each", Tone::Plain),
            ],
        ),
        StageElement::label(
            "small-sub",
            [510.0, 674.0, -1.0],
            17.0,
            &[("fits mini's 2 free cores right now", Tone::Success)],
        ),
        // Red head-of-line yield gate blocking Lane B at x = 960.
        StageElement::rect("yield-gate", [980.0, 656.0, -1.0], [26.0, 118.0])
            .corner(8.0)
            .fill(Some(Fill::Tone(Tone::Error)))
            .fill_opacity(0.85),
        StageElement::label(
            "yield-lbl",
            [1355.0, 656.0, 0.0],
            20.0,
            &[
                ("ahead >= cand_free_slots  ", Tone::Error),
                ("→ blind yield!", Tone::Plain),
            ],
        ),
        StageElement::Path {
            id: "route".into(),
            through: vec![
                Waypoint::Point([835.0, 656.0, 1.0]),
                Waypoint::Point([1552.0, 656.0, 1.0]),
                Waypoint::Point([1552.0, 326.0, 1.0]),
            ],
            curve: Curve::Straight,
            corner: 24.0,
            bend: 0.0,
            tone: Tone::Success,
            width: 2.4,
            dash: Some([10.0, 8.0]),
            arrow: Arrow::End,
        },
        // Moving 2-core job token that travels into `mini`'s free cores once unlocked!
        StageElement::rect("runner", [1180.0, 656.0, -2.0], [220.0, 54.0])
            .corner(10.0)
            .fill(Some(Fill::Tone(Tone::Success)))
            .fill_opacity(0.28),
        StageElement::label(
            "runner-txt",
            [1180.0, 656.0, -3.0],
            18.0,
            &[("2c job → ADMITTED", Tone::Success)],
        ),
        StageElement::rect("drain-chip", [1355.0, 488.0, 0.0], [660.0, 76.0])
            .corner(14.0)
            .fill(Some(Fill::Material(Material::Surface))),
        StageElement::label(
            "drain-txt",
            [1355.0, 488.0, -1.0],
            19.0,
            &[
                ("studio core-drain reserved (>120s) ", Tone::Accent),
                ("→ 7c runs next", Tone::Plain),
            ],
        ),
    ]);

    let mut stage = StageActor::declare(
        &mut scene,
        "livelock-stage",
        &StagePlan {
            post: StagePost::RESTRAINED,
            elements,
        },
    )?;

    for id in [
        "yield-gate.opacity",
        "yield-lbl.opacity",
        "route.draw",
        "runner.opacity",
        "runner-txt.opacity",
        "drain-chip.opacity",
        "drain-txt.opacity",
    ] {
        stage.channel(&mut scene, id, 0.0);
    }

    // Show the blind yield gate slamming shut on `yield_at`.
    stage.ease(
        &mut scene,
        "yield-gate.opacity",
        yield_at,
        1.0,
        0.18,
        Ease::Linear,
    );
    stage.ease(
        &mut scene,
        "yield-lbl.opacity",
        yield_at + millis(120),
        1.0,
        0.24,
        Ease::Smootherstep,
    );

    // Unlock on `clear_at`: drop the blind yield gate, draw the route to `mini`'s 2 free cores,
    // and show `studio` core-drain reservation for the 7-core job!
    stage.ease(
        &mut scene,
        "yield-gate.opacity",
        clear_at,
        0.0,
        0.25,
        Ease::Smootherstep,
    );
    stage.ease(
        &mut scene,
        "yield-lbl.opacity",
        clear_at,
        0.0,
        0.22,
        Ease::Smootherstep,
    );
    stage.ease(
        &mut scene,
        "route.draw",
        clear_at + millis(150),
        1.0,
        0.65,
        Ease::CubicBezier([0.45, 0.0, 0.2, 1.0]),
    );
    for id in ["runner.opacity", "runner-txt.opacity"] {
        stage.ease(
            &mut scene,
            id,
            clear_at + millis(350),
            1.0,
            0.25,
            Ease::Smootherstep,
        );
    }
    for id in ["runner.x", "runner-txt.x"] {
        let ch = stage.channel(&mut scene, id, 0.0);
        scene.ease(&ch, clear_at + millis(400), 372.0, 0.95, Ease::Smootherstep);
    }
    for id in ["drain-chip.opacity", "drain-txt.opacity"] {
        stage.ease(
            &mut scene,
            id,
            clear_at + millis(500),
            1.0,
            0.3,
            Ease::Smootherstep,
        );
    }

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("main-watch aborted at 1,800s · ", Tone::Error),
            sp("seer land & Hex auto-installs timed out", Tone::Plain),
            sp(" behind this livelock", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, mini_at);

    sfx::TICK.play(&mut scene, "tick-stall", stalled_at, -16.5);
    sfx::FAILURE.play(&mut scene, "gate-shut", yield_at, -15.5);
    sfx::CONFIRM.play(&mut scene, "gate-open", clear_at + millis(250), -15.0);

    scene.cue("livelock-rig", 0, duration);
    Ok(scene.finish()?)
}
