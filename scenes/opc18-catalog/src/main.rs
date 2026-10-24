//! OPC-18, Model Catalog Once: why OpenCode rebuilds its provider catalog
//! many times while plugins activate, and the proposed fix.
//!
//! One Stage film carries the catalog: the models.dev snapshot feeds the
//! plugin loop, the loop transforms and reads the provider catalog, and every
//! rebuild pulls a copy of the whole catalog out onto a deck. The twist draws
//! the batch around the loop: notifications wait in a tray, but reads still
//! rebuild. The fix rewinds the deck and replays the batch with one build. A
//! proposed diff and the three-PR plan close on decision D-7.
//!
//! Sources: the OPC-18 proposal (`artifacts/2026-10-23-perf-walkthrough/
//! catalog-proposal.md`), the measured boot profile (`artifacts/2026-10-23-
//! rsi-opencode/research/profile.md`, section b), and `packages/core/src/
//! {state,model,provider,plugin}.ts` on opencode `dev`. Measured: 17 provider
//! rebuilds, 220 transform runs, warm activation 230–253 ms, rebuilds 34–46 ms,
//! end-of-batch cascade 64–73 ms. Estimated: ~85 ms after, and the per-PR
//! savings.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    axis::AxisPlan,
    caption::{CaptionActor, CaptionAlign, CaptionPlan, CaptionSpanPlan},
    chrome::{chip, footer, header},
    editor::diff::{Diff, add, keep, remove},
    math::easing::Ease,
    meter::{MeterActor, MeterPlan},
    narration::Narration,
    plan::{ReelPlan, ReelSegmentPlan, ScenePlan},
    readout::ReadoutFormat,
    rolling::{RollingNumberActor, RollingNumberPlan},
    sfx,
    stage::{Figure, Fill, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

/// The camera holds this zoom, so the stage fills the frame; screen-space
/// overlays sit where the zoomed world puts the deck.
const ZOOM: f32 = 1.15;
const SNAPSHOT: [f32; 3] = [430.0, 400.0, 0.0];
const LOOP: [f32; 3] = [430.0, 580.0, 0.0];
const LOOP_SIZE: [f32; 2] = [300.0, 120.0];
const CATALOG: [f32; 3] = [930.0, 580.0, 0.0];
const CATALOG_SIZE: [f32; 2] = [340.0, 132.0];
/// The deck of catalog copies, one per rebuild.
const COPIES: usize = 17;
const COPY_SIZE: [f32; 2] = [250.0, 92.0];
/// The batch drawn around the loop and the catalog.
const BATCH: [f32; 3] = [690.0, 630.0, 4.0];
const BATCH_SIZE: [f32; 2] = [870.0, 330.0];
/// Notifications held by the batch.
const TRAY: [f32; 3] = [930.0, 745.0, 2.0];
const HELD: usize = 6;
/// The provider a plugin changes, copied on write.
const COW: [f32; 3] = [930.0, 420.0, -2.0];
/// The rebuild counter, above the deck (screen space).
const COUNTER: [f32; 2] = [1450.0, 372.0];
/// Activation, warm, in milliseconds (screen space).
const METER: [f32; 2] = [900.0, 905.0];

/// Catalog statuses.
const PROVIDERS: usize = 0;
const DIRTY: usize = 1;
const REBUILDING: usize = 2;
const FRESH: usize = 3;
const FROZEN: usize = 4;

fn copy_id(index: usize) -> String {
    format!("copy-{index}")
}

fn copy_at(index: usize) -> [f32; 3] {
    let k = index as f32;
    [1270.0 + k * 14.0, 500.0 + k * 9.0, -k * 0.5]
}

fn held_id(index: usize) -> String {
    format!("held-{index}")
}

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration = Narration::load(&root.join("narration"))?;
    let reel = ReelPlan::new(
        "opc18-catalog",
        vec![
            ReelSegmentPlan::cut(catalog(&narration)?),
            ReelSegmentPlan::dipped(code(&narration)?, millis(700)),
            ReelSegmentPlan::dipped(plan(&narration)?, millis(700)),
        ],
    )?;
    let output = root.join("opc18-catalog.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s, {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}

fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn line(
    scene: &mut PlanBuilder,
    id: &str,
    origin: [f32; 2],
    size: f32,
    spans: Vec<CaptionSpanPlan>,
) -> Result<CaptionActor> {
    CaptionActor::declare(scene, id, &CaptionPlan::line(origin, size, spans))
}

fn stage_plan() -> StagePlan {
    let label = StageElement::label;
    let mut elements = vec![
        StageElement::rect("batch", BATCH, BATCH_SIZE)
            .corner(22.0)
            .dash([10.0, 8.0])
            .stroke(Some(Tone::Request))
            .stroke_width(1.6),
        label(
            "batch-name",
            [
                BATCH[0] - BATCH_SIZE[0] * 0.5 + 88.0,
                BATCH[1] - BATCH_SIZE[1] * 0.5 + 26.0,
                3.0,
            ],
            20.0,
            &[("State.batch", Tone::Request)],
        ),
        StageElement::icon("snapshot", SNAPSHOT, 64.0, "database").tone(Tone::Muted),
        label(
            "snapshot-name",
            [SNAPSHOT[0] + 150.0, SNAPSHOT[1] - 12.0, 0.0],
            19.0,
            &[("models.dev snapshot", Tone::Plain)],
        ),
        label(
            "snapshot-note",
            [SNAPSHOT[0] + 150.0, SNAPSHOT[1] + 16.0, 0.0],
            17.0,
            &[("230 providers", Tone::Muted)],
        ),
        StageElement::card("loop", LOOP, LOOP_SIZE, "plugin loop").statuses(&[
            ("90 plugins, one at a time", Tone::Muted),
            ("models.dev", Tone::Plain),
            ("azure", Tone::Plain),
            ("github-copilot", Tone::Plain),
            ("openai", Tone::Plain),
            ("all loaded", Tone::Success),
        ]),
        StageElement::card("catalog", CATALOG, CATALOG_SIZE, "provider catalog").statuses(&[
            ("230 providers", Tone::Muted),
            ("dirty", Tone::Warning),
            ("rebuilding all 230", Tone::Warning),
            ("fresh", Tone::Success),
            ("built once · frozen", Tone::Accent),
        ]),
        StageElement::beam("load", "snapshot", "loop"),
        StageElement::beam("apply", "loop", "catalog"),
        StageElement::packet("seed", "load").tone(Tone::Muted),
        label(
            "snapshot-read",
            [680.0, 500.0, 0.0],
            18.0,
            &[
                ("model.transform", Tone::Plain),
                (" → providers.snapshot()", Tone::Warning),
            ],
        ),
        label(
            "settings-read",
            [680.0, 662.0, 0.0],
            18.0,
            &[
                ("configuredSettings", Tone::Plain),
                (" → providers.get()", Tone::Warning),
            ],
        ),
        StageElement::rect("tray", TRAY, [330.0, 54.0])
            .corner(14.0)
            .dash([6.0, 6.0])
            .stroke(Some(Tone::Muted)),
        label(
            "tray-name",
            [TRAY[0] - 250.0, TRAY[1], 2.0],
            18.0,
            &[("notify, held", Tone::Muted)],
        ),
        StageElement::card("cow", COW, [250.0, 66.0], "azure · copy")
            .statuses(&[("copied on write", Tone::Accent)]),
    ];
    for index in 0..HELD {
        elements.push(StageElement::Shape {
            id: held_id(index),
            at: [TRAY[0] - 125.0 + index as f32 * 50.0, TRAY[1], 1.0],
            shape: Figure::Circle(9.0),
            corner: 0.0,
            fill: Some(Fill::Tone(Tone::Request)),
            fill_opacity: 1.0,
            stroke: None,
            width: 1.0,
            dash: None,
            arrow: psychopomp::stage::Arrow::None,
        });
    }
    for index in 0..COPIES {
        elements.push(
            StageElement::card(&copy_id(index), copy_at(index), COPY_SIZE, "catalog copy")
                .statuses(&[("230 providers", Tone::Muted)]),
        );
    }
    // Reads and transforms travel the apply wire; each gets its own packet
    // so close sends never share a clock.
    for index in 0..COPIES {
        elements.push(StageElement::packet(&format!("read-{index}"), "apply").tone(Tone::Warning));
    }
    for index in 0..4 {
        elements.push(
            StageElement::packet(&format!("transform-{index}"), "apply")
                .labeled("transform")
                .tone(Tone::Request),
        );
        elements.push(
            StageElement::packet(&format!("twist-read-{index}"), "apply")
                .labeled("read")
                .tone(Tone::Warning),
        );
        elements.push(
            StageElement::packet(&format!("after-transform-{index}"), "apply").tone(Tone::Request),
        );
    }
    for index in 0..2 {
        elements.push(
            StageElement::packet(&format!("frozen-read-{index}"), "apply")
                .reversed()
                .labeled("frozen")
                .tone(Tone::Accent),
        );
    }
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

/// The catalog's status, so swaps always start from what is shown.
struct Catalog {
    status: usize,
}

impl Catalog {
    fn swap(&mut self, s: &mut StageActor, sc: &mut PlanBuilder, at: u64, to: usize) {
        if self.status != to {
            s.swap_status(sc, "catalog", at, [self.status, to], 0.18);
            self.status = to;
        }
    }
}

/// A copy of the whole catalog is pulled out of it onto the deck.
fn clone_out(s: &mut StageActor, sc: &mut PlanBuilder, index: usize, at: u64) {
    let id = copy_id(index);
    let [x, y, _] = copy_at(index);
    let dx = s.channel(sc, &format!("{id}.x"), CATALOG[0] - x);
    sc.set(&dx, at, CATALOG[0] - x);
    sc.spring(&dx, at, 0.0, 0.55, 0.08);
    let dy = s.channel(sc, &format!("{id}.y"), CATALOG[1] - y);
    sc.set(&dy, at, CATALOG[1] - y);
    sc.spring(&dy, at, 0.0, 0.55, 0.08);
    let scale = s.channel(sc, &format!("{id}.scale"), 1.3);
    sc.set(&scale, at, 1.3);
    sc.spring(&scale, at, 1.0, 0.5, 0.0);
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.set(&opacity, at, 0.0);
    sc.ease(&opacity, at, 1.0, 0.14, Ease::Smootherstep);
    let blur = s.channel(sc, &format!("{id}.blur"), 3.0);
    sc.set(&blur, at, 3.0);
    sc.ease(&blur, at, 0.0, 0.3, Ease::Smootherstep);
}

/// The copy flies back into the catalog and is gone.
fn clone_in(s: &mut StageActor, sc: &mut PlanBuilder, index: usize, at: u64) {
    let id = copy_id(index);
    let [x, y, _] = copy_at(index);
    s.ease(
        sc,
        &format!("{id}.x"),
        at,
        CATALOG[0] - x,
        0.45,
        Ease::CubicBezier([0.55, 0.0, 0.9, 0.5]),
    );
    s.ease(
        sc,
        &format!("{id}.y"),
        at,
        CATALOG[1] - y,
        0.45,
        Ease::CubicBezier([0.55, 0.0, 0.9, 0.5]),
    );
    s.ease(
        sc,
        &format!("{id}.scale"),
        at,
        0.8,
        0.45,
        Ease::CubicBezier([0.55, 0.0, 0.9, 0.5]),
    );
    s.ease(
        sc,
        &format!("{id}.opacity"),
        at + millis(250),
        0.0,
        0.2,
        Ease::Linear,
    );
}

/// A transform reaches the catalog and marks it dirty; a read then makes it
/// rebuild everything, and the newest copy on the deck lights.
fn cycle(
    s: &mut StageActor,
    sc: &mut PlanBuilder,
    cat: &mut Catalog,
    index: usize,
    transform: u64,
    read: u64,
) {
    let landed = s.send(sc, &format!("transform-{index}"), transform, 0.5);
    s.land(sc, "catalog", landed);
    cat.swap(s, sc, landed, DIRTY);
    s.hit(sc, "catalog.alarm", landed, 0.35, 0.0);
    if 3 + index < HELD {
        s.fade_in(sc, &held_id(3 + index), landed, 1.0, 0.25);
    }
    let read = s.send(
        sc,
        &format!("twist-read-{index}"),
        read.max(landed + millis(250)),
        0.5,
    );
    s.land(sc, "catalog", read);
    cat.swap(s, sc, read, REBUILDING);
    let still = s.glitch(sc, "catalog", read, [0.6, -0.4, 0.25]);
    sfx::GLITCH.play(sc, format!("twist-rebuild-{index}"), read, -22.0);
    s.hit(
        sc,
        &format!("{}.flash", copy_id(COPIES - 1)),
        read,
        0.8,
        0.0,
    );
    cat.swap(s, sc, still + millis(350), FRESH);
}

fn catalog(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(1.2),
        [
            ("intro", seconds(0.9)),
            ("catalog-before", seconds(1.1)),
            ("catalog-twist", seconds(1.1)),
            ("catalog-after", seconds(1.8)),
        ],
    )?;
    let mut scene = PlanBuilder::new("catalog", reading.duration());
    let [intro, before, twist, after] = reading.place(&mut scene);
    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;
    let mut cat = Catalog { status: PROVIDERS };

    // Everything that arrives later starts hidden.
    for id in [
        "batch",
        "batch-name",
        "snapshot-read",
        "settings-read",
        "tray",
        "tray-name",
        "cow",
    ] {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
    }
    for index in 0..HELD {
        s.channel(sc, &format!("{}.opacity", held_id(index)), 0.0);
    }
    for index in 0..COPIES {
        s.channel(sc, &format!("{}.opacity", copy_id(index)), 0.0);
    }
    s.channel(sc, "load.draw", 0.0);
    s.channel(sc, "apply.draw", 0.0);

    // ---------------------------------------------------------------- intro
    s.channel(sc, "camera.zoom", ZOOM);
    s.channel(sc, "camera.z", -140.0);
    s.channel(sc, "camera.dof", 0.4);
    s.to(sc, "camera.z", 0, 0.0, 2.4);
    let mut title = header(sc, "OPC-18", "model catalog once")?;
    title.type_in(sc, seconds(0.3), 55.0, 0.6);
    let mut chip_boot = chip(sc, "chip-boot", Tone::Request, "every boot")?;
    chip_boot.show(sc, seconds(0.7));

    let builds = intro.at("builds a catalog");
    s.settle_in(sc, "catalog", builds);
    sfx::BLOOM.play(sc, "catalog-in", builds, -22.0);
    let providers = intro.at("providers come from");
    s.fade_in(
        sc,
        "snapshot",
        providers.saturating_sub(millis(500)),
        1.0,
        0.5,
    );
    s.type_in(
        sc,
        "snapshot-name",
        providers.saturating_sub(millis(350)),
        40.0,
    );
    s.type_in(sc, "snapshot-note", providers, 30.0);
    let plugins = intro.at_any(&["plugins each", "plugins"]);
    let ready = s.settle_in(sc, "loop", plugins.saturating_sub(millis(300)));
    let contact = s.connect(sc, "load", ready, 0.45);
    sfx::TICK.play(sc, "load-connect", contact, -22.0);
    let contact = s.connect(sc, "apply", contact + millis(60), 0.45);
    sfx::TICK.play(sc, "apply-connect", contact, -22.0);
    let seeded = s.send(sc, "seed", contact + millis(100), 0.55);
    s.land(sc, "loop", seeded);
    let turn = intro.at("turn to adjust");
    s.swap_status(sc, "loop", turn, [0, 1], 0.2);
    s.swap_status(sc, "loop", turn + millis(500), [1, 2], 0.2);
    s.swap_status(sc, "loop", turn + millis(1000), [2, 3], 0.2);
    s.swap_status(sc, "loop", turn + millis(1500), [3, 0], 0.2);

    // --------------------------------------------------------------- before
    let measured = before.at("what we measured");
    chip_boot.hide(sc, measured);
    let mut chip_measured = chip(sc, "chip-measured", Tone::Success, "measured")?;
    chip_measured.show(sc, measured + millis(200));
    let mut counter = RollingNumberActor::declare(
        sc,
        "counter",
        RollingNumberPlan::new(COUNTER, 40.0, "0")
            .aligned(CaptionAlign::Center)
            .bold()
            .tone(Tone::Warning)
            .prefix(vec![span("provider rebuilds  ", Tone::Muted)]),
    )?;
    counter.show(sc, before.at("built once"));

    // Seventeen rebuilds: each read reaches the catalog, which rebuilds and
    // leaves a copy on the deck. They start slow and pile up.
    let mut at = before.at_any(&["rebuilt", "rebuilds"]);
    let mut loop_status = 0;
    for index in 0..COPIES {
        let read = s.send(
            sc,
            &format!("read-{index}"),
            at.saturating_sub(millis(300)),
            0.3,
        );
        s.land(sc, "catalog", read);
        if index == 0 {
            cat.swap(s, sc, read, REBUILDING);
        }
        s.hit(sc, "catalog.flash", read, 0.45, 0.0);
        clone_out(s, sc, index, read);
        counter.roll(sc, read, (index + 1).to_string())?;
        sfx::TICK.play(
            sc,
            format!("rebuild-{index}"),
            read,
            if index < 3 { -18.0 } else { -25.0 },
        );
        if index % 4 == 0 && index / 4 < 4 {
            let next = index / 4 + 1;
            s.swap_status(sc, "loop", read, [loop_status, next], 0.15);
            loop_status = next;
        }
        at += if index < 3 { millis(520) } else { millis(190) };
    }
    s.glitch(sc, "catalog", at, [0.7, -0.5, 0.3]);
    cat.swap(s, sc, at + millis(100), FRESH);
    let copies = before.at_any(&["copies about", "copies"]);
    for index in 0..COPIES {
        s.hit(
            sc,
            &format!("{}.flash", copy_id(index)),
            copies + millis(30 * index as u64),
            0.5,
            0.0,
        );
    }

    let mut meter = MeterActor::declare(
        sc,
        "activation",
        &MeterPlan::bar(METER, 1000.0, AxisPlan::new([0.0, 300.0]).every(50.0))
            .label("plugin activation, warm boot")
            .readout(ReadoutFormat::new(0).unit(" ms"))
            .tone(Tone::Warning)
            .tick_labels(),
        0.0,
    )?;
    let activation = before.at("activation takes");
    meter.show(sc, activation);
    meter.sweep(sc, activation + millis(400), 250.0, 1.6);
    let mut tag = line(
        sc,
        "tag-measured",
        [METER[0] + 540.0, METER[1] + 6.0],
        20.0,
        vec![span("measured · 230–253 ms", Tone::Success)],
    )?;
    tag.type_in(sc, activation + millis(2000), 40.0, 0.4);
    let mut footer_before = footer(
        sc,
        "footer-before",
        vec![
            span("17 rebuilds · 220 transform runs · ", Tone::Muted),
            span("rebuilds ~40 ms", Tone::Warning),
            span(" · ", Tone::Muted),
            span("end-of-batch cascade ~70 ms", Tone::Warning),
        ],
    )?;
    footer_before.type_in(
        sc,
        before.at_any(&["rebuilds are", "the rebuilds"]),
        50.0,
        0.8,
    );

    // ---------------------------------------------------------------- twist
    let inside = twist.at("inside a batch");
    footer_before.hide(sc, inside.saturating_sub(millis(600)));
    chip_measured.hide(sc, inside.saturating_sub(millis(600)));
    let mut chip_twist = chip(sc, "chip-twist", Tone::Warning, "the batch")?;
    chip_twist.show(sc, inside);
    s.fade_in(sc, "batch", inside, 1.0, 0.4);
    let draw = s.channel(sc, "batch.draw", 0.0);
    sc.set(&draw, inside, 0.0);
    sc.ease(&draw, inside, 1.0, 0.9, Ease::Smootherstep);
    s.type_in(sc, "batch-name", inside + millis(400), 30.0);
    sfx::WHOOSH.play(sc, "batch-in", inside, -24.0);
    for index in 0..COPIES {
        s.to(sc, &format!("{}.dim", copy_id(index)), inside, 0.5, 0.6);
    }

    let notifications = twist.at("for notifications");
    s.fade_in(sc, "tray", notifications, 1.0, 0.4);
    s.type_in(sc, "tray-name", notifications + millis(200), 30.0);
    let holds = twist.at("holds them back");
    for index in 0..3 {
        let at = holds + millis(260 * index as u64);
        s.fade_in(sc, &held_id(index), at, 1.0, 0.25);
        sfx::TICK.play(sc, format!("held-{index}"), at, -26.0);
    }
    let mut footer_doc = footer(
        sc,
        "footer-doc",
        vec![
            span("state.ts · ", Tone::Muted),
            span(
                "“Coalesces notifications … Reads inside stay fresh”",
                Tone::Plain,
            ),
        ],
    )?;
    footer_doc.type_in(sc, twist.at("hold back reads"), 50.0, 0.8);

    // A transform marks the catalog dirty; the next read rebuilds it.
    let dirty = twist.at("marks the catalog dirty");
    cycle(
        s,
        sc,
        &mut cat,
        0,
        dirty.saturating_sub(millis(500)),
        twist.at("next read"),
    );

    let snapshot_read = twist.at("provider snapshot");
    footer_doc.hide(sc, snapshot_read.saturating_sub(millis(400)));
    s.type_in(
        sc,
        "snapshot-read",
        snapshot_read.saturating_sub(millis(300)),
        40.0,
    );
    cycle(
        s,
        sc,
        &mut cat,
        1,
        snapshot_read.saturating_sub(millis(200)),
        twist.at_any(&["that's a read", "a read"]),
    );
    let settings = twist.at("check provider settings");
    s.type_in(
        sc,
        "settings-read",
        settings.saturating_sub(millis(200)),
        40.0,
    );
    cycle(s, sc, &mut cat, 2, settings, twist.at("read too"));

    let saves = twist.at("saves the notifications");
    s.hit(sc, "tray.emphasis", saves, 1.0, 0.0);
    for index in 0..HELD {
        s.hit(
            sc,
            &format!("{}.flash", held_id(index)),
            saves + millis(40 * index as u64),
            1.0,
            0.0,
        );
    }
    let pays = twist.at("still pays");
    cycle(s, sc, &mut cat, 3, pays.saturating_sub(millis(700)), pays);
    for index in 0..COPIES {
        s.hit(
            sc,
            &format!("{}.flash", copy_id(index)),
            pays + millis(25 * index as u64),
            0.6,
            0.0,
        );
    }
    let mut footer_twist = footer(
        sc,
        "footer-twist",
        vec![
            span("the batch defers ", Tone::Muted),
            span("notify", Tone::Request),
            span(" · every read of a dirty catalog ", Tone::Muted),
            span("rebuilds it from scratch", Tone::Warning),
        ],
    )?;
    footer_twist.type_in(sc, pays, 50.0, 0.8);

    // ---------------------------------------------------------------- after
    let fix = after.at("build once per batch");
    footer_twist.hide(sc, fix.saturating_sub(millis(500)));
    chip_twist.hide(sc, fix.saturating_sub(millis(500)));
    let mut chip_fix = chip(sc, "chip-fix", Tone::Accent, "proposed fix")?;
    chip_fix.show(sc, fix);
    let rewound = s.rewind(sc, fix, 0.25);
    sfx::RESET.play(sc, "rewind", fix, -20.0);
    for index in (0..COPIES).rev() {
        clone_in(s, sc, index, fix + millis(28 * (COPIES - 1 - index) as u64));
    }
    counter.roll(sc, fix + millis(300), "0")?;
    for index in 0..HELD {
        s.fade_out(sc, &held_id(index), fix, 0.3);
    }
    cat.swap(s, sc, rewound, PROVIDERS);

    let stop = after.at("stop taking a snapshot");
    s.fade_out(sc, "snapshot-read", stop + millis(300), 0.4);
    let register = after.at("register their changes");
    let mut last = register;
    for index in 0..4 {
        let at = register.saturating_sub(millis(400)) + millis(260 * index as u64);
        let landed = s.send(sc, &format!("after-transform-{index}"), at, 0.45);
        s.land(sc, "catalog", landed);
        if index == 0 {
            cat.swap(s, sc, landed, DIRTY);
        }
        s.fade_in(sc, &held_id(index), landed, 1.0, 0.25);
        last = landed;
    }
    s.swap_status(sc, "loop", last, [loop_status, 5], 0.2);

    let closes = after.at("batch closes").max(last + millis(400));
    s.hit(sc, "batch.emphasis", closes, 1.0, 0.0);
    s.fade_out(sc, "batch", closes + millis(300), 0.5);
    s.fade_out(sc, "batch-name", closes + millis(300), 0.4);
    s.fade_out(sc, "settings-read", closes, 0.4);
    let still = s.glitch(sc, "catalog", closes, [0.5, -0.3, 0.2]);
    cat.swap(s, sc, still, FROZEN);
    s.to(sc, "catalog.cool", still, 1.0, 0.6);
    clone_out(s, sc, 0, still);
    s.to(sc, &format!("{}.dim", copy_id(0)), still, 0.0, 0.3);
    counter.roll(sc, still, "1")?;
    sfx::MARK.play(sc, "built-once", still, -18.0);
    for index in 0..4 {
        s.fade_out(sc, &held_id(index), still + millis(60 * index as u64), 0.25);
    }
    s.fade_out(sc, "tray", still + millis(300), 0.4);
    s.fade_out(sc, "tray-name", still + millis(300), 0.4);

    let frozen = after.at("frozen catalog");
    let first = s.send(sc, "frozen-read-0", frozen, 0.5);
    s.land(sc, "loop", first);
    let second = s.send(sc, "frozen-read-1", frozen + millis(500), 0.5);
    s.land(sc, "loop", second);
    let copies_just = after.at("copies just that");
    let cow = s.channel(sc, "cow.y", CATALOG[1] - COW[1]);
    sc.set(&cow, copies_just, CATALOG[1] - COW[1]);
    sc.spring(&cow, copies_just, 0.0, 0.55, 0.1);
    s.fade_in(sc, "cow", copies_just, 1.0, 0.2);
    sfx::CONFIRM.play(sc, "cow", copies_just, -22.0);

    let mut note = CaptionActor::declare(
        sc,
        "counter-note",
        &CaptionPlan::line(
            [COUNTER[0], COUNTER[1] + 40.0],
            18.0,
            vec![span("one or two, per the proposal", Tone::Muted)],
        )
        .aligned(CaptionAlign::Center),
    )?;
    let note_at = after.at_any(&["one or two", "1 or 2"]);
    note.type_in(sc, note_at, 40.0, 0.4);

    let estimate = after.at("proposal estimates");
    meter.set(sc, estimate + millis(400), 85.0);
    tag.hide(sc, estimate);
    let mut tag_estimate = line(
        sc,
        "tag-estimate",
        [METER[0] + 540.0, METER[1] + 6.0],
        20.0,
        vec![span("estimate · ~85 ms", Tone::Warning)],
    )?;
    tag_estimate.type_in(sc, estimate + millis(500), 40.0, 0.4);
    let mut footer_after = footer(
        sc,
        "footer-after",
        vec![
            span("one build per batch", Tone::Accent),
            span(" · reads share a frozen catalog · ", Tone::Muted),
            span("copy on write", Tone::Accent),
        ],
    )?;
    footer_after.type_in(sc, after.at("not a measurement"), 50.0, 0.8);

    scene.finish().context("catalog")
}

fn code(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.9), [("catalog-code", seconds(1.6))])?;
    let mut scene = PlanBuilder::new("catalog-code", reading.duration());
    let [spoken] = reading.place(&mut scene);
    let sc = &mut scene;
    let mut title = header(sc, "OPC-18", "model catalog once")?;
    title.show(sc, 0);
    let mut proposed = chip(sc, "chip-proposed", Tone::Warning, "proposed · no PR yet")?;
    proposed.show(sc, seconds(0.2));
    let diff = Diff {
        file_name: "packages/core/src",
        lines: vec![
            keep("// model.ts"),
            remove(
                1,
                "transform: (u) => prepare.pipe(Effect.andThen(state.transform(u))),",
            ),
            remove(
                1,
                "reload: () => prepare.pipe(Effect.andThen(state.reload())),",
            ),
            add(1, "transform: (u) => state.transform(u),"),
            add(1, "reload: () => state.reload(),"),
            keep("// state.ts · notify"),
            remove(2, "const value = get()"),
            remove(2, "if (options.notify) yield* options.notify(value)"),
            add(2, "if (options.notify) yield* options.notify(get)"),
            add(2, "else get()"),
            keep("// provider.ts · Editor"),
            remove(3, "const provider = structuredClone(indexed.info)"),
            add(3, "const provider = indexed.info // frozen, shared"),
            keep("update: (id, fn) => {"),
            add(4, "  const draft = structuredClone(current.provider)"),
            add(4, "  fn(draft)"),
            add(4, "  current.provider = freeze(draft, true)"),
        ],
    };
    let times = [
        spoken.at("drops the snapshot"),
        spoken.at_any(&["hands the model", "notify hands"]),
        spoken.at("stops cloning"),
        spoken.at("clones only"),
    ];
    diff.declare(sc, &times, seconds(0.9), true)?;
    let mut caption = footer(
        sc,
        "footer",
        vec![span(
            "condensed from the OPC-18 proposal · not yet written or tested",
            Tone::Muted,
        )],
    )?;
    caption.show(sc, seconds(0.6));
    scene.finish().context("catalog-code")
}

fn plan(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(0.8),
        [("catalog-plan", seconds(1.0)), ("outro", seconds(2.2))],
    )?;
    let mut scene = PlanBuilder::new("catalog-plan", reading.duration());
    let [split, outro] = reading.place(&mut scene);
    let sc = &mut scene;
    let mut title = header(sc, "OPC-18", "model catalog once")?;
    title.show(sc, 0);
    let mut chip_plan = chip(sc, "chip-plan", Tone::Accent, "three PRs")?;
    chip_plan.show(sc, seconds(0.3));

    const X: f32 = 260.0;
    const ESTIMATE_X: f32 = 1660.0;
    let rows: [(&str, &str, &str, &str, f32); 3] = [
        (
            "pr1",
            "1",
            "build once per batch, copy on write",
            "−140 to −165 ms",
            300.0,
        ),
        (
            "pr2",
            "2",
            "skip no-op reloads right after boot",
            "−25 to −35 ms",
            400.0,
        ),
        (
            "pr3",
            "3",
            "skip the models.dev cache re-read",
            "−15 to −19 ms warm",
            472.0,
        ),
    ];
    let phrases = ["the first builds", "the second skips", "the third skips"];
    let mut heading = CaptionActor::declare(
        sc,
        "estimates-heading",
        &CaptionPlan::line(
            [ESTIMATE_X, 244.0],
            18.0,
            vec![span("estimated activation savings", Tone::Muted)],
        )
        .aligned(CaptionAlign::Right),
    )?;
    heading.show(sc, split.at("three pull requests"));
    for ((id, number, text, estimate, y), phrase) in rows.into_iter().zip(phrases) {
        let at = split.at(phrase);
        let mut row = line(
            sc,
            id,
            [X, y],
            32.0,
            vec![
                span(&format!("PR {number}   "), Tone::Accent),
                span(text, Tone::Plain),
            ],
        )?;
        row.type_in(sc, at, 60.0, 0.4);
        let mut saving = CaptionActor::declare(
            sc,
            &format!("{id}-estimate"),
            &CaptionPlan::line([ESTIMATE_X, y], 26.0, vec![span(estimate, Tone::Warning)])
                .aligned(CaptionAlign::Right),
        )?;
        saving.type_in(sc, at + millis(500), 40.0, 0.3);
        sfx::TICK.play(sc, format!("{id}-in"), at, -22.0);
    }
    let mut most = line(
        sc,
        "most",
        [X + 128.0, 342.0],
        18.0,
        vec![span("most of the estimated savings", Tone::Muted)],
    )?;
    most.type_in(sc, split.at("most of the"), 40.0, 0.3);

    // Decision D-7: the two open PRs that overlap PR 1.
    let decision = outro.at_any(&["decision D", "decision"]);
    chip_plan.hide(sc, decision);
    let mut chip_decision = chip(sc, "chip-decision", Tone::Warning, "decision D-7")?;
    chip_decision.show(sc, decision + millis(200));
    let mut d7 = line(
        sc,
        "d7",
        [X, 580.0],
        30.0,
        vec![
            span("D-7  ", Tone::Warning),
            span("two open PRs already quiet no-op reloads", Tone::Plain),
        ],
    )?;
    d7.type_in(sc, decision, 50.0, 0.4);
    let mut first = line(
        sc,
        "pr-56994",
        [X + 40.0, 650.0],
        24.0,
        vec![
            span("#56994  ", Tone::Accent),
            span("beta · ", Tone::Muted),
            span(
                "reuse unchanged provider and model snapshots across reloads",
                Tone::Plain,
            ),
        ],
    )?;
    first.type_in(
        sc,
        outro
            .at_any(&["targets beta", "beta"])
            .saturating_sub(millis(900)),
        70.0,
        0.3,
    );
    let mut second = line(
        sc,
        "pr-57315",
        [X + 40.0, 700.0],
        24.0,
        vec![
            span("#57315  ", Tone::Accent),
            span("dev · Aiden · ", Tone::Muted),
            span(
                "skip catalog updates when a reload changes nothing",
                Tone::Plain,
            ),
        ],
    )?;
    second.type_in(sc, outro.at_any(&["Aiden's", "Aiden"]), 70.0, 0.3);
    let mut overlap = line(
        sc,
        "overlap",
        [X + 40.0, 770.0],
        24.0,
        vec![
            span("both overlap PR 1 · ", Tone::Muted),
            span("fold one in and close the others?", Tone::Warning),
        ],
    )?;
    overlap.type_in(sc, outro.at("overlap"), 50.0, 0.4);
    let mut closing = footer(
        sc,
        "footer-decision",
        vec![
            span("D-7 needs ", Tone::Muted),
            span("Kit and Aiden", Tone::Warning),
        ],
    )?;
    closing.type_in(sc, outro.at_any(&["a call for", "call for"]), 40.0, 0.8);
    scene.finish().context("catalog-plan")
}
