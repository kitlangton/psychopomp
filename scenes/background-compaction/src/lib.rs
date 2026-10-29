//! A short pitch for OpenCode's background compaction
//! (`experimental.background_compaction`, opencode#57444 and #58294), paced
//! to one clip of Kit's ElevenLabs v4 narration. The clip is split before the
//! stats so the lanes can clear; the silent cut of the same plan loops.
//!
//! Two session lanes fill their context at the same pace. At the limit the
//! top one stops on a blocking summary while its pause counter rolls to the
//! median 55 s. The bottom one wrote its summary in the background from 85%,
//! so at the limit the summary swaps in, the recent turns slide up to it, and
//! the stream never stops. The two pause counters then carry into a stats card
//! of the first day of measurements on Kit's Studio.
use std::path::Path;

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, seconds},
    caption::{CaptionAlign, CaptionSpanPlan},
    math::easing::Ease,
    narration::{Narration, Spoken},
    plan::ScenePlan,
    rolling::{RollingNumberActor, RollingNumberPlan},
    stage::{Arrow, Figure, Fill, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

const CELLS: usize = 24;
/// Cells 0..PREFIX are what the summary covers; the rest stay word for word.
const PREFIX: usize = 20;
/// Cells present when the lanes settle in.
const START_FILLED: usize = 4;
const CELL: [f32; 2] = [56.0, 80.0];
const PITCH: f32 = 68.0;
const X0: f32 = 169.0;
const RIGHT: f32 = 1780.0;
const LEFT: f32 = 140.0;
/// Post-compaction cells in the lane that never stopped, and in the one that did.
const B_NEW: usize = 13;
const A_NEW: usize = 2;

struct Lane {
    key: &'static str,
    label_y: f32,
    bar_y: f32,
}

const A: Lane = Lane {
    key: "a",
    label_y: 250.0,
    bar_y: 360.0,
};
const B: Lane = Lane {
    key: "b",
    label_y: 610.0,
    bar_y: 720.0,
};
const TRACK_Y: f32 = 842.0;

const TITLE_IN: f64 = 0.25;
/// Silence before the first word.
const LEAD: f64 = 0.9;
/// Where the clip is cut, in the pause after "almost a minute".
const SPLIT: f64 = 16.98;

/// A plan time in seconds.
fn secs(nanos: u64) -> f64 {
    nanos as f64 / 1e9
}

fn said(spoken: &Spoken, phrase: &str) -> f64 {
    secs(spoken.at(phrase))
}

fn cell_x(slot: usize) -> f32 {
    X0 + PITCH * slot as f32
}

/// The summary block takes the first two slots.
fn block_x() -> f32 {
    (cell_x(0) + cell_x(1)) / 2.0
}
const BLOCK: [f32; 2] = [PITCH + CELL[0], CELL[1]];
const TRACK_H: f32 = 12.0;
const TRACK_LEFT: f32 = X0 - CELL[0] / 2.0;

fn t(at: f64) -> u64 {
    seconds(at)
}

fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn rect(id: String, at: [f32; 3], size: [f32; 2], tone: Tone, fill_opacity: f32) -> StageElement {
    StageElement::Shape {
        id,
        at,
        shape: Figure::Rect(size),
        corner: 9.0,
        fill: Some(Fill::Tone(tone)),
        fill_opacity,
        stroke: Some(tone),
        width: 1.6,
        dash: None,
        arrow: Arrow::None,
    }
}

fn bar(id: String, at: [f32; 3], size: [f32; 2], tone: Tone) -> StageElement {
    StageElement::Shape {
        id,
        at,
        shape: Figure::Rect(size),
        corner: 2.0,
        fill: Some(Fill::Tone(tone)),
        fill_opacity: 1.0,
        stroke: None,
        width: 1.0,
        dash: None,
        arrow: Arrow::None,
    }
}

fn left(id: &str, at: [f32; 2], size: f32, spans: &[(&str, Tone)]) -> StageElement {
    StageElement::label(id, [at[0], at[1], 0.0], size, spans).align(CaptionAlign::Left)
}

fn center(id: &str, at: [f32; 2], size: f32, spans: &[(&str, Tone)]) -> StageElement {
    StageElement::label(id, [at[0], at[1], 0.0], size, spans)
}

fn lane_elements(lane: &Lane, elements: &mut Vec<StageElement>) {
    let k = lane.key;
    let y = lane.bar_y;
    for index in 0..CELLS {
        elements.push(rect(
            format!("{k}-cell-{index}"),
            [cell_x(index), y, 0.0],
            CELL,
            Tone::Muted,
            0.42,
        ));
    }
    let new = if k == "a" { A_NEW } else { B_NEW };
    for n in 0..new {
        elements.push(rect(
            format!("{k}-new-{n}"),
            [cell_x(CELLS - PREFIX + 2 + n), y, 0.0],
            CELL,
            Tone::Muted,
            0.42,
        ));
    }
    let limit_x = cell_x(CELLS - 1) + CELL[0] / 2.0 + 12.0;
    elements.push(bar(
        format!("{k}-limit"),
        [limit_x, y, 0.0],
        [4.0, 112.0],
        Tone::Muted,
    ));
    elements.push(center(
        &format!("{k}-limit-label"),
        [limit_x, y + 80.0],
        26.0,
        &[("limit", Tone::Muted)],
    ));
}

fn stage_plan() -> StagePlan {
    let mut elements = vec![
        center(
            "title",
            [960.0, 500.0],
            84.0,
            &[("Background compaction", Tone::Plain)],
        ),
        center(
            "sub",
            [960.0, 600.0],
            36.0,
            &[("OpenCode", Tone::Accent), (" · experimental", Tone::Muted)],
        ),
        left(
            "a-name",
            [LEFT, A.label_y],
            52.0,
            &[("● ", Tone::Error), ("before", Tone::Plain)],
        ),
        left(
            "b-name",
            [LEFT, B.label_y],
            52.0,
            &[
                ("● ", Tone::Accent),
                ("with background compaction", Tone::Plain),
            ],
        ),
    ];
    lane_elements(&A, &mut elements);
    lane_elements(&B, &mut elements);

    // Lane A: the blocking summary.
    elements.extend([
        StageElement::Shape {
            id: "a-spinner".into(),
            at: [LEFT + 22.0, A.bar_y + 112.0, 0.0],
            shape: Figure::Arc {
                radius: 20.0,
                start: 0.0,
                sweep: 0.72,
            },
            corner: 0.0,
            fill: None,
            fill_opacity: 1.0,
            stroke: Some(Tone::Error),
            width: 4.0,
            dash: None,
            arrow: Arrow::None,
        },
        left(
            "a-wait",
            [LEFT + 62.0, A.bar_y + 112.0],
            42.0,
            &[
                ("Compacting context", Tone::Plain),
                ("  the session waits", Tone::Muted),
            ],
        ),
        rect(
            "a-block".into(),
            [block_x(), A.bar_y, 0.0],
            BLOCK,
            Tone::Accent,
            0.55,
        ),
    ]);

    // Lane B: the background track, the 85% tick, and the swap.
    let tick_x = (cell_x(PREFIX - 1) + cell_x(PREFIX)) / 2.0;
    elements.extend([
        bar(
            "b-85".into(),
            [tick_x, B.bar_y, 0.0],
            [4.0, 112.0],
            Tone::Accent,
        ),
        center(
            "b-85-label",
            [tick_x, B.bar_y + 80.0],
            26.0,
            &[("85%", Tone::Accent)],
        ),
        StageElement::Shape {
            id: "b-track".into(),
            at: [TRACK_LEFT + 0.5, TRACK_Y, 0.0],
            shape: Figure::Rect([1.0, TRACK_H]),
            corner: 6.0,
            fill: Some(Fill::Tone(Tone::Accent)),
            fill_opacity: 0.6,
            stroke: Some(Tone::Accent),
            width: 1.6,
            dash: None,
            arrow: Arrow::None,
        },
        left(
            "b-writing",
            [LEFT, TRACK_Y + 56.0],
            38.0,
            &[("summary written in the background", Tone::Accent)],
        ),
        left(
            "b-ready",
            [LEFT + 770.0, TRACK_Y + 56.0],
            38.0,
            &[("· ready", Tone::Success)],
        ),
        left(
            "b-swapped",
            [LEFT, TRACK_Y + 56.0],
            38.0,
            &[
                ("swapped in", Tone::Success),
                (" · recent turns kept as they were", Tone::Muted),
            ],
        ),
    ]);

    // The stats card.
    let rows = [
        [
            ("p90 pause   ", Tone::Muted),
            ("84 s", Tone::Plain),
            ("  →  ", Tone::Muted),
            ("11 s", Tone::Success),
        ],
        [
            ("failures    ", Tone::Muted),
            ("7", Tone::Plain),
            ("  →  ", Tone::Muted),
            ("0", Tone::Success),
        ],
        [
            ("cost each   ", Tone::Muted),
            ("$0.79", Tone::Plain),
            ("  →  ", Tone::Muted),
            ("$0.67", Tone::Success),
        ],
    ];
    elements.push(center(
        "stat-title",
        [960.0, 285.0],
        50.0,
        &[("median pause", Tone::Muted)],
    ));
    elements.push(center(
        "stat-arrow",
        [876.0, 430.0],
        72.0,
        &[("→", Tone::Muted)],
    ));
    for (index, row) in rows.iter().enumerate() {
        elements.push(left(
            &format!("row-{index}"),
            [ROWS_X, row_y(index)],
            50.0,
            row,
        ));
    }
    elements.push(left(
        "row-3",
        [ROWS_X, row_y(3)],
        50.0,
        &[("swapped in  ", Tone::Muted), ("97 of 99", Tone::Success)],
    ));
    elements.push(center(
        "note",
        [960.0, 905.0],
        32.0,
        &[(
            "first 99 compactions with the flag on Kit's Studio (Oct 24), vs 1,730 in the 7 days before",
            Tone::Muted,
        )],
    ));
    elements.push(center(
        "credit",
        [960.0, 1005.0],
        36.0,
        &[
            ("background compaction by ", Tone::Muted),
            ("Filip & Aiden", Tone::Plain),
            (" · ", Tone::Muted),
            ("experimental.background_compaction", Tone::Accent),
        ],
    ));

    StagePlan {
        post: StagePost {
            bloom: 0.16,
            grain: 0.0,
            vignette: 0.16,
            backdrop: 0.1,
        },
        elements,
    }
}

const ROWS_X: f32 = 562.0;
fn row_y(index: usize) -> f32 {
    600.0 + 74.0 * index as f32
}

/// A streamed cell lands: it pops in a little large, settles, and its light cools.
fn cell_in(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: f64) {
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.ease(&opacity, t(at), 1.0, 0.1, Ease::CubicOut);
    let scale = s.channel(sc, &format!("{id}.scale"), 1.0);
    sc.set(&scale, t(at), 1.14);
    sc.spring(&scale, t(at), 1.0, 0.32, 0.0);
    s.hit(sc, &format!("{id}.flash"), t(at), 1.0, 0.0);
}

/// The summarized cells fold left into the summary block; the kept tail
/// slides up beside it. Returns when the block is whole.
fn fold(s: &mut StageActor, sc: &mut PlanBuilder, lane: &Lane, at: f64) -> f64 {
    let k = lane.key;
    for index in 0..PREFIX {
        let id = format!("{k}-cell-{index}");
        let go = at + 0.012 * index as f64;
        s.glide(
            sc,
            &format!("{id}.x"),
            t(go),
            block_x() - cell_x(index),
            0.5,
        );
        s.glide(sc, &format!("{id}.scale"), t(go), 0.7, 0.5);
        s.ease(
            sc,
            &format!("{id}.opacity"),
            t(go + 0.22),
            0.0,
            0.28,
            Ease::Smoothstep,
        );
    }
    let shift = cell_x(2) - cell_x(PREFIX);
    for index in PREFIX..CELLS {
        let go = at + 0.08 + 0.03 * (index - PREFIX) as f64;
        s.glide(sc, &format!("{k}-cell-{index}.x"), t(go), shift, 0.55);
    }
    at + 0.5
}

pub fn build_plan(narration_dir: &Path) -> Result<ScenePlan> {
    let narration = Narration::load(narration_dir)?;
    let clip = narration.clip("pitch")?;
    let split = clip.split(SPLIT, "minute", "on my first");

    // The first half sets the lanes' clock: streaming starts on "fill", the
    // limit lands on "limit", and one cadence carries the cells between.
    let lanes_part = |scene: &mut PlanBuilder| clip.place_range(scene, t(LEAD), 0, split, "");
    let mut probe = PlanBuilder::new("probe", t(60.0));
    let first = lanes_part(&mut probe);
    let title_up = said(&first, "two") - 0.1;
    let lanes_in = said(&first, "sessions") - 0.1;
    let stream = said(&first, "fill") + 0.15;
    let limit = said(&first, "limit") + 0.1;
    let cadence = (limit - stream) / (CELLS - START_FILLED) as f64;
    let at_85 = stream + cadence * (PREFIX - START_FILLED) as f64 + 0.05;
    let track_done = limit - 0.15;
    let minute = said(&first, "minute");
    let resume = minute + 0.6;
    let exit = resume + 0.95;
    let stats_voice = exit + 0.3;
    let tail = secs(clip.duration() - split);
    let out = stats_voice + tail + 2.6;
    let end = out + 0.75;

    let mut scene = PlanBuilder::new("background-compaction", t(end));
    let first = lanes_part(&mut scene);
    let second = clip.place_range(&mut scene, t(stats_voice), split, clip.duration(), "-stats");
    let swaps = said(&first, "swaps");
    let plan = stage_plan();
    let mut stage = StageActor::declare(&mut scene, "stage", &plan)?;
    let s = &mut stage;
    let sc = &mut scene;

    // Everything starts hidden; the first and last frames are the empty stage.
    for element in &plan.elements {
        s.channel(sc, &format!("{}.opacity", element.id()), 0.0);
    }

    // --- Title, which then carries up into the header.
    let title_y = s.channel(sc, "title.y", 14.0);
    sc.spring(&title_y, t(TITLE_IN), 0.0, 0.5, 0.0);
    s.ease(sc, "title.opacity", t(TITLE_IN), 1.0, 0.3, Ease::CubicOut);
    let sub_y = s.channel(sc, "sub.y", 14.0);
    sc.spring(&sub_y, t(TITLE_IN + 0.12), 0.0, 0.5, 0.0);
    s.ease(
        sc,
        "sub.opacity",
        t(TITLE_IN + 0.12),
        1.0,
        0.3,
        Ease::CubicOut,
    );

    let scale = 0.5;
    let header_w = 21.0 * 0.6 * 84.0 * scale;
    let header_x = LEFT + header_w / 2.0;
    s.to(sc, "title.x", t(title_up), header_x - 960.0, 0.8);
    s.to(sc, "title.y", t(title_up), 96.0 - 500.0, 0.8);
    s.to(sc, "title.scale", t(title_up), scale, 0.8);
    s.ease(
        sc,
        "sub.opacity",
        t(title_up - 0.1),
        0.0,
        0.22,
        Ease::CubicOut,
    );
    s.to(sc, "sub.y", t(title_up - 0.1), -12.0, 0.3);

    // --- The lanes settle in, half full.
    s.type_in(sc, "a-name", t(lanes_in), 40.0);
    s.type_in(sc, "b-name", t(lanes_in + 0.12), 40.0);
    for lane in [&A, &B] {
        let k = lane.key;
        let base = lanes_in + 0.1 + if k == "b" { 0.12 } else { 0.0 };
        for index in 0..START_FILLED {
            let id = format!("{k}-cell-{index}");
            let at = base + 0.022 * index as f64;
            s.ease(
                sc,
                &format!("{id}.opacity"),
                t(at),
                1.0,
                0.2,
                Ease::CubicOut,
            );
            let y = s.channel(sc, &format!("{id}.y"), 12.0);
            sc.spring(&y, t(at), 0.0, 0.45, 0.0);
        }
        for id in [format!("{k}-limit"), format!("{k}-limit-label")] {
            s.ease(
                sc,
                &format!("{id}.opacity"),
                t(base + 0.3),
                1.0,
                0.3,
                Ease::CubicOut,
            );
        }
    }
    s.ease(
        sc,
        "b-85.opacity",
        t(lanes_in + 0.5),
        0.35,
        0.3,
        Ease::CubicOut,
    );
    s.ease(
        sc,
        "b-85-label.opacity",
        t(lanes_in + 0.5),
        0.5,
        0.3,
        Ease::CubicOut,
    );

    // --- Both sessions stream at the same pace.
    for index in START_FILLED..CELLS {
        let at = stream + cadence * (index - START_FILLED) as f64;
        for k in ["a", "b"] {
            cell_in(s, sc, &format!("{k}-cell-{index}"), at);
        }
    }

    // --- Lane B passes 85%: the summary starts in the background.
    s.to(sc, "b-85.opacity", t(at_85), 1.0, 0.2);
    s.hit(sc, "b-85.flash", t(at_85), 1.0, 0.0);
    s.to(sc, "b-85-label.opacity", t(at_85), 1.0, 0.2);
    s.set(sc, "b-track.opacity", t(at_85), 1.0);
    let grow = (track_done - at_85) as f32;
    let full = (cell_x(PREFIX - 1) + CELL[0] / 2.0) - TRACK_LEFT;
    s.ease(sc, "b-track.width", t(at_85), full - 1.0, grow, Ease::DRAW);
    s.ease(
        sc,
        "b-track.x",
        t(at_85),
        (full - 1.0) / 2.0,
        grow,
        Ease::DRAW,
    );
    s.type_in(sc, "b-writing", t(at_85 + 0.05), 52.0);
    s.hit(sc, "b-track.flash", t(track_done), 1.0, 0.0);
    s.type_in(sc, "b-ready", t(track_done), 40.0);

    // --- The limit.
    for k in ["a", "b"] {
        s.hit(sc, &format!("{k}-limit.flash"), t(limit), 1.0, 0.0);
        s.to(sc, &format!("{k}-limit-label.opacity"), t(limit), 1.0, 0.2);
    }

    // Lane A stops and waits on a blocking summary.
    for index in 0..CELLS {
        s.to(
            sc,
            &format!("a-cell-{index}.opacity"),
            t(limit + 0.05),
            0.45,
            0.35,
        );
    }
    s.ease(
        sc,
        "a-spinner.opacity",
        t(limit + 0.1),
        1.0,
        0.2,
        Ease::CubicOut,
    );
    s.ease(
        sc,
        "a-spinner.rotation",
        t(limit + 0.1),
        std::f32::consts::TAU * 4.5,
        (resume - limit) as f32,
        Ease::Linear,
    );
    s.type_in(sc, "a-wait", t(limit + 0.12), 48.0);
    let mut a_pause = RollingNumberActor::declare(
        sc,
        "a-pause",
        RollingNumberPlan::new([RIGHT, A.label_y], 96.0, "0")
            .aligned(CaptionAlign::Right)
            .tone(Tone::Error)
            .suffix(vec![span(" s", Tone::Muted)])
            .duration_nanos(seconds(0.34)),
    )?;
    a_pause.show(sc, t(limit + 0.1));
    // A clock run fast, reaching 55 on "almost a minute".
    let tick = (minute - limit - 0.35) / 7.0;
    for (step, value) in ["6", "13", "20", "27", "34", "41", "48", "55"]
        .iter()
        .enumerate()
    {
        a_pause.roll(sc, t(limit + 0.35 + tick * step as f64), *value)?;
    }

    // Lane B swaps the held summary in and keeps streaming.
    let swap = limit + 0.02;
    let rise = 0.5;
    s.to(
        sc,
        "b-track.x",
        t(swap),
        block_x() - (TRACK_LEFT + 0.5),
        rise,
    );
    s.to(sc, "b-track.width", t(swap), BLOCK[0] - 1.0, rise);
    s.to(sc, "b-track.y", t(swap), B.bar_y - TRACK_Y, rise);
    s.to(sc, "b-track.height", t(swap), BLOCK[1] - TRACK_H, rise);
    let landed = fold(s, sc, &B, swap);
    s.hit(sc, "b-track.flash", t(landed), 1.0, 0.0);
    for id in ["b-85", "b-85-label"] {
        s.to(sc, &format!("{id}.opacity"), t(swap), 0.0, 0.3);
    }
    for id in ["b-writing", "b-ready"] {
        s.to(sc, &format!("{id}.opacity"), t(swap), 0.0, 0.2);
    }
    s.type_in(sc, "b-swapped", t(swaps), 52.0);
    let mut b_pause = RollingNumberActor::declare(
        sc,
        "b-pause",
        RollingNumberPlan::new([RIGHT, B.label_y], 96.0, "0.002")
            .aligned(CaptionAlign::Right)
            .tone(Tone::Success)
            .suffix(vec![span(" s", Tone::Muted)]),
    )?;
    b_pause.show(sc, t(swap + 0.12));
    for n in 0..B_NEW {
        cell_in(
            s,
            sc,
            &format!("b-new-{n}"),
            limit + cadence * (n + 1) as f64,
        );
    }

    // Lane A's summary lands; it folds the same way and resumes.
    s.to(sc, "a-spinner.opacity", t(resume), 0.0, 0.2);
    s.to(sc, "a-wait.opacity", t(resume), 0.0, 0.2);
    for index in 0..CELLS {
        s.to(sc, &format!("a-cell-{index}.opacity"), t(resume), 1.0, 0.2);
    }
    let a_landed = fold(s, sc, &A, resume + 0.05);
    let block_scale = s.channel(sc, "a-block.scale", 0.96);
    sc.spring(&block_scale, t(resume + 0.2), 1.0, 0.35, 0.0);
    s.ease(
        sc,
        "a-block.opacity",
        t(resume + 0.2),
        1.0,
        0.3,
        Ease::CubicOut,
    );
    s.hit(sc, "a-block.flash", t(a_landed), 1.0, 0.0);
    for n in 0..A_NEW {
        cell_in(
            s,
            sc,
            &format!("a-new-{n}"),
            resume + 0.3 + cadence * n as f64,
        );
    }

    // --- The lanes leave; the two pause counters carry into the stats.
    let mut lane_ids = vec!["a-name", "b-name", "a-block", "b-track", "b-swapped"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    for k in ["a", "b"] {
        lane_ids.extend((0..CELLS).map(|index| format!("{k}-cell-{index}")));
        lane_ids.push(format!("{k}-limit"));
        lane_ids.push(format!("{k}-limit-label"));
    }
    lane_ids.extend((0..A_NEW).map(|n| format!("a-new-{n}")));
    lane_ids.extend((0..B_NEW).map(|n| format!("b-new-{n}")));
    for id in &lane_ids {
        s.to(sc, &format!("{id}.opacity"), t(exit), 0.0, 0.26);
        s.to(sc, &format!("{id}.scale"), t(exit), 0.96, 0.3);
        if plan
            .element(id)
            .is_some_and(|element| matches!(element, StageElement::Shape { .. }))
        {
            s.to(sc, &format!("{id}.blur"), t(exit), 4.0, 0.3);
        }
    }

    let carry = 0.75;
    for (counter, [dx, dy]) in [
        (&mut a_pause, [765.0 - RIGHT, 430.0 - A.label_y]),
        (&mut b_pause, [1383.0 - RIGHT, 430.0 - B.label_y]),
    ] {
        let x = counter.channel(sc, "x", 0.0);
        sc.spring(&x, t(exit + 0.05), dx, carry, 0.0);
        let y = counter.channel(sc, "y", 10.0);
        sc.spring(&y, t(exit + 0.05), dy, carry, 0.0);
    }

    // --- Stats.
    let rise_in = |s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: f64| {
        s.ease(
            sc,
            &format!("{id}.opacity"),
            t(at),
            1.0,
            0.3,
            Ease::CubicOut,
        );
        let y = s.channel(sc, &format!("{id}.y"), 12.0);
        sc.spring(&y, t(at), 0.0, 0.45, 0.0);
    };
    rise_in(s, sc, "stat-title", said(&second, "median pause") - 0.1);
    rise_in(s, sc, "stat-arrow", said(&second, "dropped"));
    let rows = said(&second, "milliseconds") + 0.5;
    for index in 0..4 {
        rise_in(s, sc, &format!("row-{index}"), rows + 0.14 * index as f64);
    }
    s.type_in(sc, "note", t(said(&second, "one day") - 0.1), 80.0);
    s.type_in(sc, "credit", second.at_any(&["philip", "filip"]), 60.0);

    // --- Out to the empty stage, so the loop restarts cleanly.
    for id in [
        "title",
        "stat-title",
        "stat-arrow",
        "row-0",
        "row-1",
        "row-2",
        "row-3",
        "note",
        "credit",
    ] {
        s.ease(
            sc,
            &format!("{id}.opacity"),
            t(out),
            0.0,
            0.4,
            Ease::Smoothstep,
        );
        s.to(
            sc,
            &format!("{id}.scale"),
            t(out),
            if id == "title" { scale * 0.98 } else { 0.98 },
            0.45,
        );
    }
    a_pause.hide(sc, t(out));
    b_pause.hide(sc, t(out));

    sc.cue("lanes", 0, t(exit + 0.4));
    sc.cue("stats", t(exit), t(end));
    scene.finish().context("background-compaction")
}
