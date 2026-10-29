use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    axis::AxisPlan,
    bars::{BarDeltaPlan, BarSeriesPlan, BarsActor, BarsPlan, Better, DeltaFormat},
    caption::{CaptionActor, CaptionPlan, CaptionSpanPlan},
    chrome::{chip, footer, header},
    math::easing::Ease,
    plan::{ScenePlan, SpringPlan},
    plot::{PlotActor, PlotMarkPlan, PlotPlan, PlotSeriesPlan},
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

/// Real weekly Anomaly team commits across default branches (Jan 5 – Oct 19, 2026;
/// 36,576 commits total, bots excluded, `2026-10-29-commits-by-person/data.csv`).
const WEEKLY_COMMITS: [f32; 42] = [
    215.0, 311.0, 483.0, 414.0, 261.0, 298.0, 453.0, 294.0, 307.0, 297.0, 204.0, 272.0, 204.0,
    229.0, 486.0, 288.0, 369.0, 304.0, 423.0, 482.0, 733.0, 956.0, 1123.0, 434.0, 1551.0, 1441.0,
    1194.0, 840.0, 624.0, 446.0, 608.0, 832.0, 1050.0, 1177.0, 1139.0, 783.0, 1466.0, 1176.0,
    917.0, 2556.0, 3500.0, 5436.0,
];

/// Segment 1: Monumental `RollingNumberActor` + 42-week `PlotActor`.
pub fn surge(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("01-surge", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "kaizen × opencode",
        "25× the commits, 25× the hidden queues",
    )?;
    chip(&mut scene, "chip", Tone::Accent, "anomaly · 2026")?;

    let peak_at = spoken.at_any(&["five thousand", "five", "merged five"]);
    let mult_at = spoken.at_any(&["twenty-five", "twenty five", "multiplied"]);
    let queue_at = spoken.at_any(&["hidden queue", "hidden", "queue"]);
    let ohno_at = spoken.at_any(&["Taiichi", "Toyota", "Sixty"]);

    CaptionActor::declare(
        &mut scene,
        "kicker",
        &CaptionPlan::line(
            [150.0, 260.0],
            20.0,
            vec![sp("WEEKLY TEAM COMMITS", Tone::Muted)],
        ),
    )?;

    let rolling_plan = RollingNumberPlan {
        origin: [150.0, 352.0],
        align: Default::default(),
        size: 92.0,
        bold: true,
        tone: Tone::Accent,
        prefix: Vec::new(),
        suffix: vec![sp(" / wk", Tone::Muted)],
        value: "215".into(),
        rolls: vec![
            RollPlan {
                at_nanos: lead + millis(900),
                value: "1,551".into(),
            },
            RollPlan {
                at_nanos: peak_at,
                value: "5,436".into(),
            },
        ],
        duration_nanos: 650_000_000,
        stagger: Default::default(),
        direction: Default::default(),
        blur: 1.0,
        chip: false,
        anchors: Vec::new(),
    };
    RollingNumberActor::declare(&mut scene, "commits-num", rolling_plan)?;

    CaptionActor::declare(
        &mut scene,
        "ytd",
        &CaptionPlan::line(
            [150.0, 456.0],
            21.0,
            vec![
                sp("36,576", Tone::Plain),
                sp(" commits · Jan 5 – Oct 19", Tone::Muted),
            ],
        ),
    )?;

    let mut surge_chip = CaptionActor::declare(
        &mut scene,
        "surge-chip",
        &CaptionPlan::line(
            [150.0, 548.0],
            22.0,
            vec![
                sp("25.3× ", Tone::Success),
                sp("weekly throughput", Tone::Plain),
            ],
        )
        .glass(),
    )?;
    surge_chip.show(&mut scene, mult_at);

    let mut drag_chip = CaptionActor::declare(
        &mut scene,
        "drag-chip",
        &CaptionPlan::line(
            [150.0, 628.0],
            22.0,
            vec![
                sp("queues & defects ", Tone::Error),
                sp("scale 25× too", Tone::Plain),
            ],
        )
        .glass(),
    )?;
    drag_chip.show(&mut scene, queue_at);

    let commits_pts: Vec<[f32; 2]> = WEEKLY_COMMITS
        .iter()
        .enumerate()
        .map(|(w, &c)| [w as f32, c])
        .collect();
    let drag_pts: Vec<[f32; 2]> = (0..42)
        .map(|w| {
            let x = w as f32;
            let y = if w < 38 {
                160.0 + x * 6.5
            } else {
                match w {
                    38 => 480.0,
                    39 => 1520.0,
                    40 => 2950.0,
                    _ => 4850.0,
                }
            };
            [x, y]
        })
        .collect();

    let plot_plan = PlotPlan {
        origin: [690.0, 225.0],
        size: [1030.0, 575.0],
        x: AxisPlan::new([0.0, 41.0])
            .every(10.0)
            .label("week of 2026 (Jan 5 → Oct 19)"),
        y: AxisPlan::new([0.0, 6000.0])
            .every(1500.0)
            .label("commits / wk"),
        series: vec![
            PlotSeriesPlan::new("commits", "commits / wk", Tone::Accent, commits_pts),
            PlotSeriesPlan::new("drag", "unmanaged queue wait", Tone::Error, drag_pts).dashed(),
        ],
        marks: vec![PlotMarkPlan {
            id: "oct".into(),
            x: 38.0,
            label: "Oct fleet".into(),
        }],
    };
    let mut plot = PlotActor::declare(&mut scene, "surge-plot", &plot_plan)?;
    // Frame 0 alive: axes crisp at t=0.
    plot.channel(&mut scene, "axes", 1.0);
    plot.series_channel(&mut scene, "drag", "draw", 0.0);
    let draw_secs = ((peak_at.saturating_sub(lead)) as f32 / 1e9).clamp(2.2, 6.5);
    plot.draw(&mut scene, "commits", lead, draw_secs);
    plot.ride(&mut scene, "commits", [0.0, 41.0], lead, draw_secs);
    plot.mark(&mut scene, "oct", peak_at.saturating_sub(millis(400)));
    plot.draw(&mut scene, "drag", queue_at, 1.8);

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("Taiichi Ohno · ", Tone::Accent),
            sp("Toyota Production System (1978)", Tone::Plain),
            sp(" — expose every bottleneck at line speed", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, ohno_at);

    sfx::WHOOSH.play(&mut scene, "whoosh-curve", lead + millis(200), -17.0);
    sfx::MARK.play(&mut scene, "mark-peak", peak_at, -15.0);
    sfx::FAILURE.play(&mut scene, "warn-queue", queue_at, -17.0);
    sfx::TICK.play(&mut scene, "tick-ohno", ohno_at, -16.0);

    scene.cue("surge", 0, duration);
    Ok(scene.finish()?)
}

/// Segment 2: Geometric Takaoka Overhead Andon Lightboard & Assembly Track + `BarsActor`.
pub fn nummi(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("02-nummi", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "1982 → 1984",
        "NUMMI: 85% the same workers, a different system",
    )?;
    chip(&mut scene, "chip", Tone::Warning, "Fremont · Georgetown")?;

    let nummi_at = spoken.at_any(&["NUMMI", "reopened", "Two years"]);
    let cord_at = spoken.at_any(&["andon cord", "andon", "overhead"]);
    let george_at = spoken.at_any(&["Georgetown", "two thousand", "pull that"]);
    let spear_at = spoken.at_any(&["Spear", "Bowen", "falsifiable"]);

    // Custom geometric Takaoka / NUMMI overhead Andon board + conveyor track (zero orbs!).
    let stations = [
        ("st1", 390.0, "01 · STAMP"),
        ("st2", 770.0, "02 · WELD"),
        ("st3", 1150.0, "03 · TRIM"),
        ("st4", 1530.0, "04 · FINAL"),
    ];
    let mut elements = vec![
        StageElement::rect("housing", [960.0, 265.0, 3.0], [1480.0, 128.0])
            .corner(18.0)
            .fill(Some(Fill::Material(Material::Well))),
        StageElement::Path {
            id: "cord".into(),
            through: vec![
                Waypoint::Point([260.0, 382.0, 1.0]),
                Waypoint::Point([1660.0, 382.0, 1.0]),
            ],
            curve: Curve::Straight,
            corner: 0.0,
            bend: 0.0,
            tone: Tone::Warning,
            width: 2.6,
            dash: None,
            arrow: Arrow::None,
        },
        StageElement::label(
            "cord-tag",
            [330.0, 360.0, 0.0],
            17.0,
            &[("OVERHEAD ANDON CORD", Tone::Warning)],
        ),
        StageElement::rect("pull-grip", [1150.0, 404.0, 0.0], [24.0, 42.0])
            .corner(6.0)
            .fill(Some(Fill::Tone(Tone::Warning)))
            .fill_opacity(0.85),
        StageElement::Path {
            id: "track".into(),
            through: vec![
                Waypoint::Point([260.0, 480.0, 2.0]),
                Waypoint::Point([1660.0, 480.0, 2.0]),
            ],
            curve: Curve::Straight,
            corner: 0.0,
            bend: 0.0,
            tone: Tone::Muted,
            width: 2.0,
            dash: Some([10.0, 8.0]),
            arrow: Arrow::End,
        },
        StageElement::rect("chassis", [390.0, 480.0, -1.0], [176.0, 50.0])
            .corner(12.0)
            .fill(Some(Fill::Material(Material::Raised))),
        StageElement::label(
            "chassis-lbl",
            [390.0, 480.0, -2.0],
            18.0,
            &[("NOVA #042", Tone::Plain)],
        ),
    ];

    for (id, x, title) in stations {
        elements.push(
            StageElement::rect(id, [x, 265.0, 1.0], [310.0, 86.0])
                .corner(12.0)
                .fill(Some(Fill::Material(Material::Surface))),
        );
        elements.push(StageElement::label(
            &format!("{id}-txt"),
            [x, 265.0, 0.0],
            22.0,
            &[(title, Tone::Plain)],
        ));
    }
    elements.push(
        StageElement::rect("st3-red", [1150.0, 265.0, 0.5], [310.0, 86.0])
            .corner(12.0)
            .fill(Some(Fill::Tone(Tone::Error)))
            .fill_opacity(0.32),
    );
    elements.push(
        StageElement::rect("st3-green", [1150.0, 265.0, 0.4], [310.0, 86.0])
            .corner(12.0)
            .fill(Some(Fill::Tone(Tone::Success)))
            .fill_opacity(0.28),
    );

    let mut stage = StageActor::declare(
        &mut scene,
        "andon-rig",
        &StagePlan {
            post: StagePost::RESTRAINED,
            elements,
        },
    )?;
    stage.channel(&mut scene, "st3-red.opacity", 0.0);
    stage.channel(&mut scene, "st3-green.opacity", 0.0);

    // Chassis + its label travel one identical monotonic screen-space path (`kit-motion`).
    let move1 = cord_at.saturating_sub(millis(1400));
    for id in ["chassis.x", "chassis-lbl.x"] {
        let ch = stage.channel(&mut scene, id, 0.0);
        scene.ease(&ch, move1, 760.0, 1.35, Ease::Smootherstep);
    }

    // Pull the overhead Andon cord at Station 03: physical tug + halt + repair + resume.
    let grip_y = stage.channel(&mut scene, "pull-grip.y", 0.0);
    scene.spring(&grip_y, cord_at, 20.0, 0.24, 0.15);
    scene.spring(&grip_y, cord_at + millis(650), 0.0, 0.45, 0.2);

    stage.ease(
        &mut scene,
        "st3-red.opacity",
        cord_at,
        1.0,
        0.14,
        Ease::Linear,
    );
    let fixed_at = cord_at + seconds(2.3);
    stage.ease(
        &mut scene,
        "st3-red.opacity",
        fixed_at,
        0.0,
        0.35,
        Ease::Smootherstep,
    );
    stage.ease(
        &mut scene,
        "st3-green.opacity",
        fixed_at,
        1.0,
        0.28,
        Ease::Smootherstep,
    );
    for id in ["chassis.x", "chassis-lbl.x"] {
        stage.ease(
            &mut scene,
            id,
            fixed_at + millis(300),
            1140.0,
            1.15,
            Ease::Smootherstep,
        );
    }

    // Bottom-left: NUMMI vs GM Fremont verified metrics cards (compact width so right gutter breathes).
    let mut gm_chip = CaptionActor::declare(
        &mut scene,
        "gm-stat",
        &CaptionPlan::line(
            [150.0, 628.0],
            22.0,
            vec![
                sp("GM Fremont 1982  ", Tone::Muted),
                sp("> 20% absent", Tone::Error),
                sp(" · worst in GM", Tone::Muted),
            ],
        )
        .glass(),
    )?;
    gm_chip.show(&mut scene, lead + millis(300));

    let mut nummi_chip = CaptionActor::declare(
        &mut scene,
        "nummi-stat",
        &CaptionPlan::line(
            [150.0, 712.0],
            22.0,
            vec![
                sp("NUMMI 1985       ", Tone::Accent),
                sp("2% absent", Tone::Success),
                sp(" · 85% same UAW", Tone::Plain),
            ],
        )
        .glass(),
    )?;
    nummi_chip.show(&mut scene, nummi_at);

    // Bottom-right: `BarsActor` comparing weekly Andon cord pulls (Georgetown 2,000 vs Detroit 2).
    let bars_plan = BarsPlan::new(
        [1180.0, 604.0],
        520.0,
        AxisPlan::new([0.0, 2200.0])
            .every(1000.0)
            .label("andon cord pulls / week"),
    )
    .row_height(86.0)
    .size(22.0)
    .series(BarSeriesPlan::new("pulls", "pulls / wk", Tone::Accent))
    .readout(ReadoutFormat::new(0).grouped())
    .row("georgetown", "Georgetown KY")
    .row("detroit", "Detroit rival");

    let mut bars = BarsActor::declare(&mut scene, "andon-bars", &bars_plan)?;
    bars.show(&mut scene, george_at.saturating_sub(millis(350)));
    bars.reveal_rows(&mut scene, george_at.saturating_sub(millis(200)));
    bars.grow(
        &mut scene,
        george_at,
        "pulls",
        &[("georgetown", 2000.0), ("detroit", 2.0)],
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("Spear & Bowen (HBR 1999): ", Tone::Accent),
            sp("every task is a falsifiable hypothesis", Tone::Plain),
            sp(" · Shook (LEI 2009)", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, spear_at);

    sfx::CONFIRM.play(&mut scene, "sfx-nummi", nummi_at, -16.0);
    sfx::IMPACT.play(&mut scene, "sfx-cord", cord_at, -14.0);
    sfx::SUCCESS.play(&mut scene, "sfx-fixed", fixed_at, -16.0);
    sfx::MARK.play(&mut scene, "sfx-george", george_at, -16.0);

    // Suppress unused import warnings while keeping SpringPlan handy if retuned.
    let _ = (
        Better::Lower,
        DeltaFormat::Percent,
        BarDeltaPlan::new("a", "b"),
        SpringPlan::ENTER,
    );

    scene.cue("nummi", 0, duration);
    Ok(scene.finish()?)
}
