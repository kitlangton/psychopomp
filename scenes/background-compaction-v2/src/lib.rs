//! v2 of the background compaction pitch (`experimental.background_compaction`,
//! opencode#57444 and #58294) for X: one clip of Kit's ElevenLabs v4 voice,
//! burned-in captions, and a plan whose last frame is its first, so the
//! silent cut loops.
//!
//! Two session lanes stream at the same pace behind a caret. The bottom lane
//! passes 85% and starts writing its summary on a track beneath the row; at
//! the limit that summary lifts into the row over the turns it covers, shrinks
//! to its compact size, and the kept turns and the caret slide after it, so
//! the stream carries on. The top lane hits the limit, stops, and writes the
//! same summary while its pause counter runs to the median 55 s, then folds
//! the same way. The two counters carry into a card of the first 99
//! compactions measured on Kit's Studio.
//!
//! Motion: no bounce anywhere; travel is minimum-jerk; the caret leaves rest
//! on a quadratic ease-in and then moves at one constant speed; things that
//! leave are covered or fade in place; persistent items never blur.
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
    subtitles::{SubtitlesActor, SubtitlesPlan},
    tone::Tone,
};

const CELLS: usize = 24;
/// Cells 0..PREFIX are what the summary covers; the rest stay word for word.
const PREFIX: usize = 20;
/// Cells present in the opening (and closing) frame.
const START: usize = 2;
const CELL: [f32; 2] = [56.0, 80.0];
const PITCH: f32 = 68.0;
const X0: f32 = 169.0;
const LEFT: f32 = 140.0;
const RIGHT: f32 = 1780.0;
/// Turns each lane streams after its fold, at most.
const A_NEW: usize = 4;
const B_NEW: usize = 16;
const TRACK_H: f32 = 12.0;
const TRACK_LEFT: f32 = X0 - CELL[0] / 2.0;
const BLOCK: [f32; 2] = [PITCH + CELL[0], CELL[1]];

/// Silence before the first word.
const LEAD: f64 = 0.35;
/// Source time of the cut, inside the measured silence after "minute"
/// (14.33–14.51 s); the word timings run about 0.2 s early here.
const SPLIT: f64 = 14.42;
/// When the caret leaves rest.
const CARET_GO: f64 = 0.15;
/// How long the stats card holds after the last word.
const HOLD: f64 = 5.4;

const RISE: f32 = 0.42;
const COMPRESS_AT: f64 = 0.3;
const COMPRESS: f32 = 0.62;
/// `y = x²` as a cubic Bézier: leaves rest and arrives at twice its average speed.
const EASE_IN: Ease = Ease::CubicBezier([1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0 / 3.0]);

struct Lane {
    key: &'static str,
    label_y: f32,
    row_y: f32,
    track_y: f32,
    text_y: f32,
    new: usize,
}

const A: Lane = Lane {
    key: "a",
    label_y: 220.0,
    row_y: 322.0,
    track_y: 432.0,
    text_y: 484.0,
    new: A_NEW,
};
const B: Lane = Lane {
    key: "b",
    label_y: 580.0,
    row_y: 682.0,
    track_y: 792.0,
    text_y: 844.0,
    new: B_NEW,
};

/// Stats card layout.
const STATS_Y: f32 = 360.0;
const A_STAT_RIGHT: f32 = 785.0;
const ARROW_X: f32 = 874.0;
const B_STAT_RIGHT: f32 = 1366.0;
const ROWS_X: f32 = 590.0;

fn row_y(index: usize) -> f32 {
    482.0 + 68.0 * index as f32
}

fn secs(nanos: u64) -> f64 {
    nanos as f64 / 1e9
}

fn t(at: f64) -> u64 {
    seconds(at)
}

fn said(spoken: &Spoken, phrase: &str) -> f64 {
    secs(spoken.at(phrase))
}

fn cell_x(slot: usize) -> f32 {
    X0 + PITCH * slot as f32
}

/// The track's full width: under every cell the summary covers.
fn track_full() -> f32 {
    cell_x(PREFIX - 1) + CELL[0] / 2.0 - TRACK_LEFT
}

/// Kept turns slide this far when the summary shrinks to its block.
fn shift() -> f32 {
    cell_x(2) - cell_x(PREFIX)
}

fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn shape(id: String, at: [f32; 3], size: [f32; 2], corner: f32, tone: Tone, fill: f32, stroke: bool) -> StageElement {
    StageElement::Shape {
        id,
        at,
        shape: Figure::Rect(size),
        corner,
        fill: Some(Fill::Tone(tone)),
        fill_opacity: fill,
        stroke: stroke.then_some(tone),
        width: 1.6,
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
    let y = lane.row_y;
    for index in 0..CELLS {
        elements.push(shape(format!("{k}-cell-{index}"), [cell_x(index), y, 0.0], CELL, 9.0, Tone::Muted, 0.42, true));
    }
    for n in 0..lane.new {
        elements.push(shape(
            format!("{k}-new-{n}"),
            [cell_x(CELLS - PREFIX + 2 + n), y, 0.0],
            CELL,
            9.0,
            Tone::Muted,
            0.42,
            true,
        ));
    }
    let limit_x = cell_x(CELLS) - PITCH / 2.0 + 6.0;
    elements.push(shape(format!("{k}-limit"), [limit_x, y, 0.0], [4.0, 112.0], 2.0, Tone::Muted, 1.0, false));
    elements.push(center(&format!("{k}-limit-label"), [limit_x, y + 84.0], 32.0, &[("limit", Tone::Muted)]));
    // The summary: a thin track under the row while it is written.
    elements.push(StageElement::Shape {
        id: format!("{k}-track"),
        at: [TRACK_LEFT + 0.5, lane.track_y, 0.0],
        shape: Figure::Rect([1.0, TRACK_H]),
        corner: 6.0,
        fill: Some(Fill::Tone(Tone::Accent)),
        fill_opacity: 0.6,
        stroke: Some(Tone::Accent),
        width: 1.6,
        dash: None,
        arrow: Arrow::None,
    });
    // The caret sits in the gap before the next slot; drawn last, over the cells.
    elements.push(shape(
        format!("{k}-caret"),
        [cell_x(0) - PITCH / 2.0, y, 0.0],
        [5.0, 64.0],
        2.5,
        Tone::Accent,
        1.0,
        false,
    ));
}

fn stage_plan() -> StagePlan {
    let mut elements = vec![
        left(
            "header",
            [LEFT, 100.0],
            44.0,
            &[("Background compaction", Tone::Plain), ("  ·  OpenCode", Tone::Muted)],
        ),
        left("a-name", [LEFT, A.label_y], 52.0, &[("● ", Tone::Error), ("without", Tone::Plain)]),
        left(
            "b-name",
            [LEFT, B.label_y],
            52.0,
            &[("● ", Tone::Accent), ("with background compaction", Tone::Plain)],
        ),
    ];
    lane_elements(&A, &mut elements);
    lane_elements(&B, &mut elements);

    let tick_x = (cell_x(PREFIX - 1) + cell_x(PREFIX)) / 2.0;
    elements.extend([
        left(
            "a-text",
            [LEFT, A.text_y],
            40.0,
            &[("writing the summary", Tone::Accent), ("  ·  the session waits", Tone::Error)],
        ),
        shape("b-85".into(), [tick_x, B.row_y, 0.0], [4.0, 112.0], 2.0, Tone::Accent, 1.0, false),
        center("b-85-label", [tick_x, B.row_y + 84.0], 32.0, &[("85%", Tone::Accent)]),
        left(
            "b-writing",
            [LEFT, B.text_y],
            40.0,
            &[("writing the summary in the background", Tone::Accent)],
        ),
        left("b-ready", [LEFT + 900.0, B.text_y], 40.0, &[("·  ready", Tone::Success)]),
        left(
            "b-swapped",
            [LEFT, B.text_y],
            40.0,
            &[("swapped in", Tone::Success), ("  ·  recent turns kept as they were", Tone::Muted)],
        ),
    ]);

    let rows = [
        [("p90 pause   ", Tone::Muted), ("84 s", Tone::Plain), ("  →  ", Tone::Muted), ("11 s", Tone::Success)],
        [("failures    ", Tone::Muted), ("7", Tone::Plain), ("  →  ", Tone::Muted), ("0", Tone::Success)],
        [("cost each   ", Tone::Muted), ("$0.79", Tone::Plain), ("  →  ", Tone::Muted), ("$0.67", Tone::Success)],
    ];
    elements.push(center("stat-title", [960.0, 226.0], 50.0, &[("median pause", Tone::Muted)]));
    elements.push(center("stat-arrow", [ARROW_X, STATS_Y], 72.0, &[("→", Tone::Muted)]));
    for (index, row) in rows.iter().enumerate() {
        elements.push(left(&format!("row-{index}"), [ROWS_X, row_y(index)], 48.0, row));
    }
    elements.push(left(
        "row-3",
        [ROWS_X, row_y(3)],
        48.0,
        &[("swapped in  ", Tone::Muted), ("97 of 99", Tone::Success)],
    ));
    elements.push(center(
        "note",
        [960.0, 774.0],
        34.0,
        &[("my first 99 compactions with the flag, Oct 24  ·  vs 1,730 in the 7 days before", Tone::Muted)],
    ));
    elements.push(center(
        "credit",
        [960.0, 838.0],
        36.0,
        &[
            ("background compaction by ", Tone::Muted),
            ("Filip & Aiden", Tone::Plain),
            ("  ·  ", Tone::Muted),
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

/// The opening (and closing) composition: element, resting opacity.
fn opening() -> Vec<(String, f32)> {
    let mut shown = vec![("a-name".to_owned(), 1.0), ("b-name".to_owned(), 1.0)];
    for k in ["a", "b"] {
        shown.extend((0..START).map(|index| (format!("{k}-cell-{index}"), 1.0)));
        shown.push((format!("{k}-limit"), 1.0));
        shown.push((format!("{k}-limit-label"), 1.0));
        shown.push((format!("{k}-caret"), 1.0));
    }
    shown.push(("b-85".into(), 0.35));
    shown.push(("b-85-label".into(), 0.5));
    shown
}

/// A streamed turn lands in place under the caret: it fades up from a slight
/// blur and 96% scale, and its light cools.
fn cell_in(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: f64) {
    s.ease(sc, &format!("{id}.opacity"), t(at), 1.0, 0.16, Ease::CubicOut);
    let scale = s.channel(sc, &format!("{id}.scale"), 1.0);
    sc.set(&scale, t(at), 0.96);
    sc.spring(&scale, t(at), 1.0, 0.3, 0.0);
    let blur = s.channel(sc, &format!("{id}.blur"), 0.0);
    sc.set(&blur, t(at), 3.0);
    sc.ease(&blur, t(at), 0.0, 0.2, Ease::CubicOut);
    s.hit(sc, &format!("{id}.flash"), t(at), 0.6, 0.0);
}

/// Fade a label up 12 px into place.
fn rise_in(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: f64) {
    s.ease(sc, &format!("{id}.opacity"), t(at), 1.0, 0.32, Ease::CubicOut);
    let y = s.channel(sc, &format!("{id}.y"), 0.0);
    sc.set(&y, t(at - 0.05), 12.0);
    sc.spring(&y, t(at), 0.0, 0.45, 0.0);
}

/// The caret's x for a position in slots, measured from the gap before slot 0.
fn caret(slots: f64) -> f32 {
    PITCH * slots as f32
}

/// Stream turns behind the caret from rest at `go`: the caret eases up to
/// speed over the first half slot and then holds one speed. The caret starts
/// in the gap before `slot` (a lane-local slot index for cell `first`).
/// Returns the times each listed cell lands.
fn stream(
    s: &mut StageActor,
    sc: &mut PlanBuilder,
    lane: &str,
    go: f64,
    cadence: f64,
    slot: usize,
    cells: &[String],
    until: Option<f64>,
) -> Vec<f64> {
    let x = format!("{lane}-caret.x");
    s.ease(sc, &x, t(go), caret(slot as f64 + 0.5), cadence as f32, EASE_IN);
    let first = go + cadence;
    let last = slot as f64 + cells.len() as f64;
    s.ease(
        sc,
        &x,
        t(first),
        caret(last),
        (cadence * (cells.len() as f64 - 0.5)) as f32,
        Ease::Linear,
    );
    let mut landed = Vec::new();
    for (n, id) in cells.iter().enumerate() {
        let at = first + cadence * n as f64 - 0.04;
        if until.is_some_and(|until| at > until) {
            break;
        }
        cell_in(s, sc, id, at);
        landed.push(at);
    }
    landed
}

/// At the limit: the summary track lifts into the row over the turns it
/// covers, then shrinks to its block while the kept turns and the caret slide
/// after its right edge. Returns when everything rests.
fn fold(s: &mut StageActor, sc: &mut PlanBuilder, lane: &Lane, at: f64) -> f64 {
    let k = lane.key;
    let track = format!("{k}-track");
    s.glide(sc, &format!("{track}.y"), t(at), lane.row_y - lane.track_y, RISE);
    s.glide(sc, &format!("{track}.height"), t(at), CELL[1] - TRACK_H, RISE);
    for index in 0..PREFIX {
        s.glide(sc, &format!("{k}-cell-{index}.opacity"), t(at + 0.08), 0.0, 0.34);
    }
    let squeeze = at + COMPRESS_AT;
    s.glide(sc, &format!("{track}.width"), t(squeeze), BLOCK[0] - 1.0, COMPRESS);
    s.glide(sc, &format!("{track}.x"), t(squeeze), (BLOCK[0] - 1.0) / 2.0, COMPRESS);
    for index in PREFIX..CELLS {
        s.glide(sc, &format!("{k}-cell-{index}.x"), t(squeeze), shift(), COMPRESS);
    }
    s.glide(sc, &format!("{k}-caret.x"), t(squeeze), caret(CELLS as f64) + shift(), COMPRESS);
    let rest = squeeze + f64::from(COMPRESS);
    s.hit(sc, &format!("{track}.flash"), t(rest - 0.1), 0.5, 0.0);
    rest
}

fn lane_ids(lane: &Lane) -> Vec<String> {
    let k = lane.key;
    let mut ids = vec![
        format!("{k}-name"),
        format!("{k}-limit"),
        format!("{k}-limit-label"),
        format!("{k}-track"),
        format!("{k}-caret"),
    ];
    ids.extend((0..CELLS).map(|index| format!("{k}-cell-{index}")));
    ids.extend((0..lane.new).map(|n| format!("{k}-new-{n}")));
    ids
}

pub fn build_plan(narration_dir: &Path) -> Result<ScenePlan> {
    let narration = Narration::load(narration_dir)?;
    let clip = narration.clip("pitch")?;
    let split = seconds(SPLIT);

    let lanes_part = |scene: &mut PlanBuilder| clip.place_range(scene, t(LEAD), 0, split, "");
    let mut probe = PlanBuilder::new("probe", t(60.0));
    let first = lanes_part(&mut probe);

    // One caret speed carries both lanes from rest to the limit on "limit".
    let limit = said(&first, "limit") + 0.1;
    let cadence = (limit - CARET_GO) / (CELLS as f64 - START as f64 + 0.5);
    let at_85 = CARET_GO + cadence * (PREFIX as f64 - START as f64 + 0.5);
    let swap = limit + 0.03;
    let minute = said(&first, "minute") + 0.2;
    let resume = minute + 0.1;
    let a_rest = resume + f64::from(COMPRESS) + COMPRESS_AT;
    let exit = a_rest + 0.5;
    let stats_voice = resume + 0.45;
    let tail = secs(clip.duration() - split);
    let voice_end = stats_voice + tail;
    let out = voice_end + HOLD;
    let back = out + 0.38;
    let end = back + 0.65;

    let mut scene = PlanBuilder::new("background-compaction-v2", t(end));
    let first = lanes_part(&mut scene);
    let second = clip.place_range(&mut scene, t(stats_voice), split, clip.duration(), "-stats");
    let plan = stage_plan();
    let mut stage = StageActor::declare(&mut scene, "stage", &plan)?;
    let s = &mut stage;
    let sc = &mut scene;

    // Every element rests hidden except the opening composition; the header never moves.
    let opening = opening();
    for element in &plan.elements {
        let id = element.id();
        let rest = opening
            .iter()
            .find(|(shown, _)| shown == id)
            .map_or(if id == "header" { 1.0 } else { 0.0 }, |(_, opacity)| *opacity);
        s.channel(sc, &format!("{id}.opacity"), rest);
    }
    for k in ["a", "b"] {
        s.channel(sc, &format!("{k}-caret.x"), caret(START as f64));
    }

    // --- Both lanes stream at the same pace, to the limit.
    for lane in [&A, &B] {
        let k = lane.key;
        let cells: Vec<String> = (START..CELLS).map(|index| format!("{k}-cell-{index}")).collect();
        stream(s, sc, k, CARET_GO, cadence, START, &cells, None);
    }

    // --- The bottom lane passes 85%: its summary starts on the track.
    s.ease(sc, "b-85.opacity", t(at_85), 1.0, 0.2, Ease::CubicOut);
    s.hit(sc, "b-85.flash", t(at_85), 1.0, 0.0);
    s.ease(sc, "b-85-label.opacity", t(at_85), 1.0, 0.2, Ease::CubicOut);
    let track_done = limit - 0.6;
    let full = track_full() - 1.0;
    s.ease(sc, "b-track.opacity", t(at_85), 1.0, 0.15, Ease::CubicOut);
    s.ease(sc, "b-track.width", t(at_85), full, (track_done - at_85) as f32, Ease::DRAW);
    s.ease(sc, "b-track.x", t(at_85), full / 2.0, (track_done - at_85) as f32, Ease::DRAW);
    rise_in(s, sc, "b-writing", at_85 + 0.04);
    s.hit(sc, "b-track.flash", t(track_done), 0.8, 0.0);
    rise_in(s, sc, "b-ready", track_done);

    // --- The limit: both carets strike it.
    for k in ["a", "b"] {
        s.hit(sc, &format!("{k}-limit.flash"), t(limit), 1.0, 0.0);
        s.hit(sc, &format!("{k}-caret.flash"), t(limit), 0.8, 0.0);
    }

    // The top lane stops and writes its summary while the session waits.
    for index in 0..CELLS {
        s.glide(sc, &format!("a-cell-{index}.opacity"), t(limit + 0.05), 0.5, 0.4);
    }
    s.glide(sc, "a-caret.opacity", t(limit + 0.05), 0.3, 0.5);
    s.ease(sc, "a-track.opacity", t(limit + 0.15), 1.0, 0.15, Ease::CubicOut);
    let wait = (minute - limit - 0.15) as f32;
    s.ease(sc, "a-track.width", t(limit + 0.15), full, wait, Ease::DRAW);
    s.ease(sc, "a-track.x", t(limit + 0.15), full / 2.0, wait, Ease::DRAW);
    rise_in(s, sc, "a-text", limit + 0.2);
    let mut a_pause = RollingNumberActor::declare(
        sc,
        "a-pause",
        RollingNumberPlan::new([RIGHT, A.label_y], 96.0, "0")
            .aligned(CaptionAlign::Right)
            .tone(Tone::Error)
            .suffix(vec![span(" s", Tone::Muted)])
            .duration_nanos(seconds(0.22)),
    )?;
    a_pause.show(sc, t(limit + 0.1));
    // A clock run fast: every second from 1 to 55, landing on "minute".
    let count_from = limit + 0.3;
    let tick = (minute - count_from) / 54.0;
    for value in 1..=55 {
        a_pause.roll(sc, t(count_from + tick * f64::from(value - 1)), value.to_string())?;
    }

    // The bottom lane swaps the finished summary in and keeps streaming.
    for id in ["b-85", "b-85-label", "b-writing", "b-ready"] {
        s.glide(sc, &format!("{id}.opacity"), t(swap), 0.0, 0.28);
    }
    let b_rest = fold(s, sc, &B, swap);
    rise_in(s, sc, "b-swapped", said(&first, "swaps"));
    let mut b_pause = RollingNumberActor::declare(
        sc,
        "b-pause",
        RollingNumberPlan::new([RIGHT, B.label_y], 96.0, "0.002")
            .aligned(CaptionAlign::Right)
            .tone(Tone::Success)
            .suffix(vec![span(" s", Tone::Muted)]),
    )?;
    b_pause.show(sc, t(swap + 0.15));
    let b_new: Vec<String> = (0..B_NEW).map(|n| format!("b-new-{n}")).collect();
    stream(s, sc, "b", b_rest, cadence, CELLS - PREFIX + 2, &b_new, Some(exit + 0.05));

    // The top lane's summary is done: it folds the same way and resumes.
    s.glide(sc, "a-text.opacity", t(resume), 0.0, 0.25);
    for index in PREFIX..CELLS {
        s.glide(sc, &format!("a-cell-{index}.opacity"), t(resume), 1.0, 0.3);
    }
    s.glide(sc, "a-caret.opacity", t(resume), 1.0, 0.3);
    let a_folded = fold(s, sc, &A, resume);
    let a_new: Vec<String> = (0..A_NEW).map(|n| format!("a-new-{n}")).collect();
    stream(s, sc, "a", a_folded, cadence, CELLS - PREFIX + 2, &a_new, Some(exit + 0.05));

    // --- The lanes leave in place; the two counters carry into the stats.
    let mut leaving = lane_ids(&A);
    leaving.extend(lane_ids(&B));
    leaving.push("b-swapped".into());
    for id in &leaving {
        s.glide(sc, &format!("{id}.opacity"), t(exit), 0.0, 0.32);
        s.glide(sc, &format!("{id}.scale"), t(exit), 0.97, 0.36);
        if matches!(plan.element(id), Some(StageElement::Shape { .. })) {
            s.glide(sc, &format!("{id}.blur"), t(exit), 3.0, 0.36);
        }
    }
    let carry = 0.8;
    for (counter, [dx, dy], delay) in [
        (&mut a_pause, [A_STAT_RIGHT - RIGHT, STATS_Y - A.label_y], 0.0),
        (&mut b_pause, [B_STAT_RIGHT - RIGHT, STATS_Y - B.label_y], 0.1),
    ] {
        let x = counter.channel(sc, "x", 0.0);
        sc.ease(&x, t(exit + delay), dx, carry, Ease::GLIDE);
        let y = counter.channel(sc, "y", 10.0);
        sc.ease(&y, t(exit + delay), dy, carry, Ease::GLIDE);
    }

    // Reset the hidden lanes to the opening composition for the return.
    let reset = exit + 0.6;
    for id in &leaving {
        let Some(element) = plan.element(id) else { continue };
        for property in ["x", "y", "width", "height", "blur"] {
            if let Some(rest) = element.channel_default(property) {
                s.set(sc, &format!("{id}.{property}"), t(reset), rest);
            }
        }
        s.set(sc, &format!("{id}.scale"), t(reset), 0.98);
        if matches!(element, StageElement::Shape { .. }) {
            s.set(sc, &format!("{id}.blur"), t(reset), 3.0);
        }
    }
    for k in ["a", "b"] {
        s.set(sc, &format!("{k}-caret.x"), t(reset), caret(START as f64));
        for index in 0..START {
            s.set(sc, &format!("{k}-cell-{index}.opacity"), t(reset), 0.0);
        }
    }

    // --- Stats.
    rise_in(s, sc, "stat-title", said(&second, "median pause") - 0.1);
    rise_in(s, sc, "stat-arrow", said(&second, "went"));
    let rows = said(&second, "milliseconds") + 0.3;
    for index in 0..4 {
        rise_in(s, sc, &format!("row-{index}"), rows + 0.12 * index as f64);
    }
    rise_in(s, sc, "note", rows + 0.6);
    rise_in(s, sc, "credit", secs(second.at_any(&["philip", "filip"])));

    // --- Back to the opening composition, so the last frame is the first.
    for id in ["stat-title", "stat-arrow", "row-0", "row-1", "row-2", "row-3", "note", "credit"] {
        s.glide(sc, &format!("{id}.opacity"), t(out), 0.0, 0.4);
        s.glide(sc, &format!("{id}.scale"), t(out), 0.98, 0.45);
    }
    a_pause.hide(sc, t(out));
    b_pause.hide(sc, t(out));
    for (id, opacity) in &opening {
        s.glide(sc, &format!("{id}.opacity"), t(back), *opacity, 0.5);
        s.glide(sc, &format!("{id}.scale"), t(back), 1.0, 0.55);
        if matches!(plan.element(id), Some(StageElement::Shape { .. })) {
            s.glide(sc, &format!("{id}.blur"), t(back), 0.0, 0.5);
        }
    }

    // --- Burned-in captions, with the names spelled as the people spell them.
    let mut captions = SubtitlesPlan::new([960.0, 990.0], 1560.0).size(46.0).max_lines(1);
    // A word straddling the cut shows once, in the second range.
    let cut = t(LEAD + SPLIT);
    for (index, spoken) in [&first, &second].into_iter().enumerate() {
        for (word, from, to) in spoken.words() {
            if index == 0 && to > cut {
                continue;
            }
            let text = match word.trim() {
                "Philip" => "Filip",
                "Aidan" => "Aiden",
                other => other,
            };
            captions = captions.word(text, from, to);
        }
    }
    SubtitlesActor::declare(sc, "captions", &captions)?;

    sc.cue("lanes", 0, t(exit + 0.4));
    sc.cue("stats", t(exit), t(end));
    scene.finish().context("background-compaction-v2")
}
