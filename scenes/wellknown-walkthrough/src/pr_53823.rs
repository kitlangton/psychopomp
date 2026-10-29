//! #53823: recover wellknown sources and login plugin after startup outage.
use std::path::Path;

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    chrome::{chip, footer},
    editor::diff::{Diff, add, keep, remove},
    effects::spinner::Mark,
    math::{Vec2, Vec3, vec2},
    narration::Narration,
    plan::{ReelPlan, ReelSegmentPlan, ReelTransitionStyle, ScenePlan},
    sfx,
    stage::{Camera, OrbEntrance, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

use crate::{Pr, code_with, header, span};

const PR: Pr = Pr {
    number: "#53823",
    title: "recover cold wellknown sources & plugin",
    slug: "cold",
};

const TIMER_CARD: [f32; 3] = [420.0, 360.0, -40.0];
const TIMER_SIZE: [f32; 2] = [340.0, 120.0];
const PLUGIN_CARD: [f32; 3] = [420.0, 660.0, -40.0];
const PLUGIN_SIZE: [f32; 2] = [340.0, 120.0];
const KV_CARD: [f32; 3] = [960.0, 220.0, -20.0];
const KV_SIZE: [f32; 2] = [340.0, 104.0];
const ENDPOINT_ORB: [f32; 3] = [1040.0, 520.0, 0.0];
const INT_CARD: [f32; 3] = [1520.0, 520.0, -40.0];
const INT_SIZE: [f32; 2] = [320.0, 120.0];
const CLOSING_CAMERA: [f32; 3] = [-120.0, -40.0, 60.0];

pub fn build_53823(narration_dir: &Path) -> Result<ReelPlan> {
    let narration = Narration::load(narration_dir)?;
    let stage = stage_film(&narration)?;
    let code = cold_code(&narration)?;
    let reel = ReelPlan {
        version: ReelPlan::VERSION,
        id: "pr-53823".to_owned(),
        segments: vec![
            ReelSegmentPlan {
                transition_nanos: 0,
                transition_style: ReelTransitionStyle::Dip,
                transition_focus: None,
                transition_wipe: None,
                plan: stage,
            },
            ReelSegmentPlan {
                transition_nanos: seconds(1.15),
                transition_style: ReelTransitionStyle::Zoom,
                transition_focus: Some(timer_rect()),
                transition_wipe: None,
                plan: code,
            },
        ],
    };
    reel.validate()?;
    Ok(reel)
}

fn timer_rect() -> [f32; 4] {
    Camera::at(Vec3::from(CLOSING_CAMERA), vec2(1920.0, 1080.0))
        .project_rect(Vec3::from(TIMER_CARD), Vec2::from(TIMER_SIZE))
        .expect("timer card in front of camera")
}

fn stage_plan() -> StagePlan {
    let elements = vec![
        StageElement::orb("endpoint", ENDPOINT_ORB, 118.0)
            .points(800)
            .tone(Tone::Plain),
        StageElement::label(
            "ep-name",
            [ENDPOINT_ORB[0], 675.0, 0.0],
            22.0,
            &[("/.well-known/opencode", Tone::Plain)],
        ),
        StageElement::label(
            "ep-down",
            [ENDPOINT_ORB[0], 706.0, 0.0],
            18.0,
            &[("●", Tone::Error), (" cold startup · 503", Tone::Error)],
        ),
        StageElement::label(
            "ep-up",
            [ENDPOINT_ORB[0], 706.0, 0.0],
            18.0,
            &[
                ("●", Tone::Success),
                (" 200 · manifest ready", Tone::Success),
            ],
        ),
        StageElement::card("kv", KV_CARD, KV_SIZE, "KV wellknown:sources")
            .statuses(&[("persisted origin", Tone::Accent)]),
        StageElement::card("timer", TIMER_CARD, TIMER_SIZE, "Config 10m timer")
            .statuses(&[
                ("snapshot empty → return", Tone::Error),
                ("calls wellknown.refresh()", Tone::Success),
            ])
            .tone(Tone::Request)
            .mark(Mark::Check),
        StageElement::card("plugin", PLUGIN_CARD, PLUGIN_SIZE, "WellKnownPlugin")
            .statuses(&[
                ("Effect.orDie → failed", Tone::Error),
                ("catches error · subscribed", Tone::Accent),
                ("login method registered", Tone::Success),
            ])
            .tone(Tone::Accent)
            .mark(Mark::Check),
        StageElement::card("int", INT_CARD, INT_SIZE, "Integration.Service").statuses(&[
            ("no login command", Tone::Warning),
            ("command: login ready", Tone::Success),
        ]),
        StageElement::beam("timer-ep", "timer", "endpoint").tone(Tone::Request),
        StageElement::beam("timer-kv", "timer", "kv").tone(Tone::Request),
        StageElement::beam("plugin-ep", "plugin", "endpoint").tone(Tone::Accent),
        StageElement::beam("plugin-int", "plugin", "int").tone(Tone::Success),
        StageElement::packet("cold-fail", "plugin-ep")
            .reversed()
            .labeled("503 → orDie")
            .tone(Tone::Error),
        StageElement::packet("kv-read", "timer-kv")
            .labeled("read origins")
            .tone(Tone::Request),
        StageElement::packet("refresh-ok", "timer-ep")
            .labeled("refresh()")
            .tone(Tone::Success),
        StageElement::packet("reg-login", "plugin-int")
            .labeled("register login")
            .tone(Tone::Success),
    ];
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

fn stage_film(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(1.4),
        [("cold-before", seconds(2.0)), ("cold-after", seconds(2.0))],
    )?;
    let mut scene = PlanBuilder::new("cold-stage", reading.duration());
    let [before, after] = reading.place(&mut scene);
    let b = |phrase: &str| before.at(phrase);
    let a = |phrase: &str| after.at(phrase);

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;

    s.channel(sc, "camera.z", -140.0);
    s.channel(sc, "camera.dof", 0.38);
    s.to(sc, "camera.z", 0, 0.0, 2.0);
    s.orb_in(sc, "endpoint", seconds(0.15), OrbEntrance::HERO);
    s.fade_in(sc, "ep-name", seconds(0.65), 1.0, 0.45);
    s.fade_in(sc, "ep-down", seconds(0.8), 1.0, 0.45);
    s.to(sc, "endpoint.hurt", seconds(0.5), 0.7, 0.3);

    let kv_land = s.settle_in(sc, "kv", seconds(0.28));
    let timer_land = s.settle_in(sc, "timer", seconds(0.42));
    let plugin_land = s.settle_in(sc, "plugin", seconds(0.56));
    let int_land = s.settle_in(sc, "int", seconds(0.7));
    s.connect(sc, "timer-kv", kv_land + seconds(0.12), 0.45);
    s.connect(sc, "timer-ep", timer_land + seconds(0.12), 0.45);
    s.connect(sc, "plugin-ep", plugin_land + seconds(0.12), 0.45);
    let c4 = s.connect(sc, "plugin-int", int_land + seconds(0.12), 0.45);
    sfx::TICK.play(sc, "connect-0", c4, -20.0);

    header(sc, &PR, Some(seconds(0.3)))?;
    let mut before_chip = chip(sc, "chip-before", Tone::Muted, "before")?;
    before_chip.show(sc, seconds(0.55));

    let mut note = CalloutActor::declare(
        sc,
        "cold-note",
        &CalloutPlan::new(
            CalloutAnchorPlan::Stage {
                id: "timer".into(),
                element: "timer".into(),
                edge: CalloutSide::Top,
                side: Some(CalloutSide::Top),
            },
            vec![
                span("!snapshot().length ", Tone::Error),
                span("skipped refresh()", Tone::Plain),
            ],
        )
        .side(CalloutSide::Top)
        .reach(58.0)
        .tone(Tone::Error)
        .chip(),
    )?;

    // Act 1: Cold startup fails discovery -> timer skips refresh -> plugin dies
    let empty_at = b("snapshot empty");
    s.hit(sc, "timer.alarm", empty_at, 0.25, 0.0);
    note.show(sc, b("never retried"));
    s.to(sc, "timer-ep.break", b("never retried"), 1.0, 0.45);

    let died_at = b("login plugin died");
    let die_hit = s.send(sc, "cold-fail", died_at - seconds(0.35), 0.45);
    sfx::FAILURE.play(sc, "die-hit", die_hit, -8.0);
    s.hit(sc, "plugin.alarm", die_hit, 0.28, 0.0);
    s.set(sc, "plugin.damage", die_hit, 1.0);
    s.glitch(sc, "plugin", die_hit, [7.0, 9.0, 8.0]);
    s.to(sc, "plugin-int.break", die_hit + seconds(0.1), 1.0, 0.5);

    let mut footer_before = footer(
        sc,
        "footer-before",
        vec![
            span("cold outage → ", Tone::Plain),
            span("timer & login plugin never recover", Tone::Error),
        ],
    )?;
    footer_before.type_in(sc, died_at, 46.0, 0.7);

    // Rewind
    let switch = before.end() + seconds(0.45);
    s.clock_for(sc, "post.rewind", switch, 1.35);
    before_chip.hide(sc, switch);
    footer_before.hide(sc, switch);
    note.hide(sc, switch);
    let mut rewind_chip = chip(sc, "chip-rewind", Tone::Accent, "◀◀ rewind")?;
    rewind_chip.show(sc, switch + seconds(0.22));
    rewind_chip.hide(sc, switch + seconds(1.4));
    let mut after_chip = chip(sc, "chip-after", Tone::Success, "after the fix")?;
    after_chip.show(sc, switch + seconds(1.55));
    sfx::LAUNCH.play(sc, "rewind", switch - seconds(0.08), -13.0);

    s.set(sc, "plugin.damage", switch + seconds(0.4), 0.0);
    s.to(sc, "plugin.status", switch + seconds(0.4), 1.0, 0.3);
    s.to(sc, "timer.status", switch + seconds(0.4), 1.0, 0.3);
    s.to(sc, "timer-ep.break", switch + seconds(0.4), 0.0, 0.4);
    s.to(sc, "plugin-int.break", switch + seconds(0.4), 0.0, 0.4);

    // Act 2: plugin stays subscribed, timer refreshes from KV, endpoint recovers
    let catches_at = a("catches initial discovery");
    s.to(sc, "plugin.glow", catches_at, 0.45, 0.4);

    let refresh_at = a("always calls refresh");
    s.send(sc, "kv-read", refresh_at - seconds(0.2), 0.42);
    s.swap_labels(sc, ["ep-down", "ep-up"], refresh_at, millis(220));
    s.to(sc, "endpoint.hurt", refresh_at, 0.0, 0.35);
    let ref_hit = s.send(sc, "refresh-ok", refresh_at + seconds(0.2), 0.45);
    sfx::SEND.play(sc, "ref-send", refresh_at + seconds(0.2), -9.0);
    s.hit(sc, "timer.flash", ref_hit, 0.35, 0.0);

    let recover_at = a("recover automatically");
    let reg_hit = s.send(sc, "reg-login", recover_at, 0.45);
    sfx::BLOOM.play(sc, "reg-bloom", reg_hit, -10.0);
    s.to(sc, "plugin.status", reg_hit, 2.0, 0.25);
    s.hit(sc, "int.flash", reg_hit, 0.35, 0.0);
    s.to(sc, "int.status", reg_hit, 1.0, 0.25);

    let mut footer_after = footer(
        sc,
        "footer-after",
        vec![
            span("timer refreshes KV origins · ", Tone::Plain),
            span("login command recovers", Tone::Success),
        ],
    )?;
    footer_after.type_in(sc, recover_at, 44.0, 0.75);

    let close = after.end() + seconds(0.3);
    s.to(sc, "camera.x", close, CLOSING_CAMERA[0], 1.2);
    s.to(sc, "camera.y", close, CLOSING_CAMERA[1], 1.2);
    s.to(sc, "camera.z", close, CLOSING_CAMERA[2], 1.2);
    s.to(sc, "timer.glow", close, 0.65, 0.8);
    after_chip.hide(sc, close);
    footer_after.hide(sc, close);

    scene.finish().context("cold-stage")
}

fn cold_code(narration: &Narration) -> Result<ScenePlan> {
    let diff = Diff {
        file_name: "core/src/wellknown/plugin.ts & config.ts",
        lines: vec![
            keep("// packages/core/src/wellknown/plugin.ts"),
            remove(1, "yield* wellknown.entries().pipe(Effect.orDie)"),
            add(1, "yield* wellknown.entries().pipe("),
            add(
                1,
                "  Effect.catch(() => Effect.logWarning(\"awaiting refresh\")),",
            ),
            add(1, ")"),
            keep("// packages/core/src/config.ts (10m timer)"),
            remove(2, "if (!wellknown.snapshot().length) return Effect.void"),
            keep("const changed = yield* wellknown.refresh().pipe("),
            keep("  Effect.catch((error) =>"),
            keep("    Effect.logWarning(\"failed\", { error }).pipe(Effect.as(false))),"),
            keep(")"),
            remove(2, "if (!changed) yield* reload()"),
            add(
                2,
                "if (!changed && wellknown.snapshot().length) yield* reload()",
            ),
        ],
    };
    let steps = vec![
        "warning catch before subscribing",
        "empty snapshot check moves after",
    ];
    code_with(
        &PR,
        narration,
        (
            diff,
            steps,
            "condensed for display · persisted cold sources and login registration both recover",
        ),
        false,
        |_, _, _| Ok(()),
    )
}
