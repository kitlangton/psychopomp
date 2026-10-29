//! #53821: retain validated wellknown remote config across refresh failures.
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
    number: "#53821",
    title: "retain validated wellknown remote config",
    slug: "cache",
};

const CONFIG_CARD: [f32; 3] = [390.0, 500.0, -40.0];
const CONFIG_SIZE: [f32; 2] = [340.0, 124.0];
const WELLKNOWN: [f32; 3] = [960.0, 500.0, 0.0];
const WK_SIZE: [f32; 2] = [340.0, 124.0];
const REMOTE_ORB: [f32; 3] = [1510.0, 380.0, 0.0];
const LOC_B: [f32; 3] = [960.0, 215.0, -20.0];
const LOC_B_SIZE: [f32; 2] = [310.0, 104.0];
const CLOSING_CAMERA: [f32; 3] = [0.0, 0.0, 60.0];

pub fn build_53821(narration_dir: &Path) -> Result<ReelPlan> {
    let narration = Narration::load(narration_dir)?;
    let stage = stage_film(&narration)?;
    let code = cache_code(&narration)?;
    let reel = ReelPlan {
        version: ReelPlan::VERSION,
        id: "pr-53821".to_owned(),
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
                transition_focus: Some(wellknown_rect()),
                transition_wipe: None,
                plan: code,
            },
        ],
    };
    reel.validate()?;
    Ok(reel)
}

fn wellknown_rect() -> [f32; 4] {
    Camera::at(Vec3::from(CLOSING_CAMERA), vec2(1920.0, 1080.0))
        .project_rect(Vec3::from(WELLKNOWN), Vec2::from(WK_SIZE))
        .expect("wellknown card in front of camera")
}

fn stage_plan() -> StagePlan {
    let elements = vec![
        StageElement::orb("remote", REMOTE_ORB, 118.0)
            .points(800)
            .tone(Tone::Plain),
        StageElement::ring("remote-ring", REMOTE_ORB, 130.0)
            .thickness(1.5)
            .tone(Tone::Success),
        StageElement::label(
            "remote-name",
            [REMOTE_ORB[0], 535.0, 0.0],
            22.0,
            &[("remote_config /config", Tone::Plain)],
        ),
        StageElement::label(
            "remote-ok",
            [REMOTE_ORB[0], 566.0, 0.0],
            18.0,
            &[("●", Tone::Success), (" 200 · valid policy", Tone::Muted)],
        ),
        StageElement::label(
            "remote-fault",
            [REMOTE_ORB[0], 566.0, 0.0],
            18.0,
            &[("●", Tone::Error), (" 503 / invalid 200", Tone::Error)],
        ),
        StageElement::card("config", CONFIG_CARD, CONFIG_SIZE, "Config.load")
            .statuses(&[
                ("providers + allowlist", Tone::Success),
                ("dropped → []", Tone::Error),
                ("strict parseRemote", Tone::Accent),
                ("retained last-good", Tone::Success),
            ])
            .tone(Tone::Request)
            .mark(Mark::Check),
        StageElement::card("wk", WELLKNOWN, WK_SIZE, "WellKnown.resolve")
            .statuses(&[
                ("no last-good cache", Tone::Muted),
                ("resolve failed", Tone::Error),
                ("validated cache", Tone::Accent),
                ("serves last-good", Tone::Success),
            ])
            .tone(Tone::Accent)
            .mark(Mark::Check),
        StageElement::card("loc-b", LOC_B, LOC_B_SIZE, "Location B")
            .statuses(&[
                ("open project", Tone::Muted),
                ("missed update · stale", Tone::Warning),
                ("global update · synced", Tone::Success),
            ])
            .tone(Tone::Plain),
        StageElement::beam("cfg-wk", "config", "wk").tone(Tone::Request),
        StageElement::beam("wk-remote", "wk", "remote").tone(Tone::Request),
        StageElement::beam("wk-locb", "wk", "loc-b").tone(Tone::Accent),
        StageElement::packet("req-1", "wk-remote")
            .labeled("GET /config")
            .tone(Tone::Request),
        StageElement::packet("err-503", "wk-remote")
            .reversed()
            .labeled("503 / invalid")
            .tone(Tone::Error),
        StageElement::packet("drop-pkt", "cfg-wk")
            .reversed()
            .labeled("[] dropped")
            .tone(Tone::Error),
        StageElement::packet("req-2", "wk-remote")
            .labeled("GET /config")
            .tone(Tone::Request),
        StageElement::packet("err-503-2", "wk-remote")
            .reversed()
            .labeled("503 / invalid")
            .tone(Tone::Warning),
        StageElement::packet("cached-pkt", "cfg-wk")
            .reversed()
            .labeled("last-good config")
            .tone(Tone::Success),
        StageElement::packet("fanout-pkt", "wk-locb")
            .labeled("global: true")
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
        [
            ("cache-before", seconds(2.0)),
            ("cache-after", seconds(2.0)),
        ],
    )?;
    let mut scene = PlanBuilder::new("cache-stage", reading.duration());
    let [before, after] = reading.place(&mut scene);
    let b = |phrase: &str| before.at(phrase);
    let a = |phrase: &str| after.at(phrase);

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;

    s.channel(sc, "camera.z", -140.0);
    s.channel(sc, "camera.dof", 0.38);
    s.to(sc, "camera.z", 0, 0.0, 2.0);
    s.orb_in(sc, "remote", seconds(0.15), OrbEntrance::HERO);
    s.fade_in(sc, "remote-name", seconds(0.65), 1.0, 0.45);
    s.fade_in(sc, "remote-ok", seconds(0.8), 1.0, 0.45);

    let cfg_land = s.settle_in(sc, "config", seconds(0.3));
    let wk_land = s.settle_in(sc, "wk", seconds(0.44));
    let loc_land = s.settle_in(sc, "loc-b", seconds(0.58));
    s.connect(sc, "cfg-wk", cfg_land + seconds(0.12), 0.48);
    let c2 = s.connect(sc, "wk-remote", wk_land + seconds(0.12), 0.48);
    s.connect(sc, "wk-locb", loc_land + seconds(0.12), 0.48);
    sfx::TICK.play(sc, "connect-0", c2, -20.0);

    header(sc, &PR, Some(seconds(0.3)))?;
    let mut before_chip = chip(sc, "chip-before", Tone::Muted, "before")?;
    before_chip.show(sc, seconds(0.55));

    let mut cache_callout = CalloutActor::declare(
        sc,
        "cache-note",
        &CalloutPlan::new(
            CalloutAnchorPlan::Stage {
                id: "wk".into(),
                element: "wk".into(),
                edge: CalloutSide::Bottom,
                side: Some(CalloutSide::Bottom),
            },
            vec![
                span("keyed by ", Tone::Muted),
                span("origin · credentialID · vars", Tone::Accent),
            ],
        )
        .side(CalloutSide::Bottom)
        .reach(58.0)
        .tone(Tone::Accent)
        .chip(),
    )?;

    // Act 1: 503 or invalid 200 drops providers and allowlist
    let req_at = b("remote config endpoint");
    s.clock(sc, "wk.spinner", req_at);
    let req_hit = s.send(sc, "req-1", req_at, 0.48);
    sfx::SEND.play(sc, "req-1-send", req_at, -9.0);

    let fault_at = b("transient 503");
    s.swap_labels(sc, ["remote-ok", "remote-fault"], fault_at, millis(220));
    s.to(sc, "remote.hurt", fault_at, 0.7, 0.25);
    let err_hit = s.send(sc, "err-503", fault_at.max(req_hit + seconds(0.1)), 0.48);
    sfx::FAILURE.play(sc, "err-503-hit", err_hit, -9.0);
    s.hit(sc, "wk.alarm", err_hit, 0.25, 0.0);
    s.to(sc, "wk.status", err_hit, 1.0, 0.25);
    s.clock(sc, "wk.release", err_hit);

    let dropped_at = b("dropped the remote providers");
    let drop_hit = s.send(sc, "drop-pkt", dropped_at, 0.45);
    s.hit(sc, "config.alarm", drop_hit, 0.28, 0.0);
    s.set(sc, "config.damage", drop_hit, 1.0);
    s.glitch(sc, "config", drop_hit, [7.0, 9.0, 8.0]);
    sfx::GLITCH.play(sc, "cfg-glitch", drop_hit, -18.0);
    s.to(sc, "config.status", drop_hit, 1.0, 0.2);

    let stale_at = b("leaving every other");
    s.to(sc, "wk-locb.break", stale_at - seconds(0.25), 1.0, 0.5);
    s.to(sc, "loc-b.status", stale_at, 1.0, 0.25);
    s.to(sc, "loc-b.dim", stale_at, 0.45, 0.5);

    let mut footer_before = footer(
        sc,
        "footer-before",
        vec![
            span("transient fault → ", Tone::Plain),
            span("providers & policies dropped", Tone::Error),
        ],
    )?;
    footer_before.type_in(sc, stale_at, 46.0, 0.7);

    // Rewind
    let switch = before.end() + seconds(0.45);
    s.clock_for(sc, "post.rewind", switch, 1.35);
    before_chip.hide(sc, switch);
    footer_before.hide(sc, switch);
    let mut rewind_chip = chip(sc, "chip-rewind", Tone::Accent, "◀◀ rewind")?;
    rewind_chip.show(sc, switch + seconds(0.22));
    rewind_chip.hide(sc, switch + seconds(1.4));
    let mut after_chip = chip(sc, "chip-after", Tone::Success, "after the fix")?;
    after_chip.show(sc, switch + seconds(1.55));
    sfx::LAUNCH.play(sc, "rewind", switch - seconds(0.08), -13.0);

    s.set(sc, "config.damage", switch + seconds(0.4), 0.0);
    s.to(sc, "config.status", switch + seconds(0.4), 2.0, 0.3);
    s.to(sc, "wk.status", switch + seconds(0.4), 2.0, 0.3);
    s.to(sc, "loc-b.dim", switch + seconds(0.4), 0.0, 0.4);
    s.to(sc, "loc-b.status", switch + seconds(0.4), 0.0, 0.3);
    s.to(sc, "wk-locb.break", switch + seconds(0.4), 0.0, 0.4);
    for clock in ["spinner", "release", "mark"] {
        s.set(sc, &format!("wk.{clock}"), switch + seconds(0.4), -1.0);
        s.set(sc, &format!("config.{clock}"), switch + seconds(0.4), -1.0);
    }

    // Act 2: validated cache retains snapshot and broadcasts globally
    let validates_at = a("validates and normalizes");
    cache_callout.show(sc, validates_at);
    let spin_at = validates_at + seconds(0.2);
    s.clock(sc, "wk.spinner", spin_at);

    let transient_at = a("transient 503s");
    let req2_hit = s.send(sc, "req-2", transient_at - seconds(0.35), 0.42);
    let err2_hit = s.send(sc, "err-503-2", req2_hit + seconds(0.25), 0.42);
    s.hit(sc, "wk.flash", err2_hit, 0.3, 0.0);
    s.to(sc, "wk.status", err2_hit, 3.0, 0.25);
    let wk_done = s.resolve_spinner(sc, "wk", spin_at, err2_hit);
    sfx::MARK.play(sc, "wk-mark", wk_done, -18.0);

    let keep_at = a("keep the previous");
    let cached_hit = s.send(sc, "cached-pkt", keep_at, 0.45);
    s.hit(sc, "config.flash", cached_hit, 0.35, 0.0);
    s.to(sc, "config.status", cached_hit, 3.0, 0.25);
    s.to(sc, "config.glow", cached_hit, 0.5, 0.45);

    let global_at = a("broadcast globally");
    let fanout_hit = s.send(sc, "fanout-pkt", global_at, 0.45);
    sfx::BLOOM.play(sc, "fanout-bloom", fanout_hit, -10.0);
    s.hit(sc, "loc-b.flash", fanout_hit, 0.35, 0.0);
    s.to(sc, "loc-b.status", fanout_hit, 2.0, 0.25);

    let mut footer_after = footer(
        sc,
        "footer-after",
        vec![
            span("validated ", Tone::Plain),
            span("last-good config retained", Tone::Success),
            span(" · global manifest fan-out", Tone::Muted),
        ],
    )?;
    footer_after.type_in(sc, global_at, 44.0, 0.75);

    let close = after.end() + seconds(0.3);
    s.to(sc, "camera.x", close, CLOSING_CAMERA[0], 1.2);
    s.to(sc, "camera.y", close, CLOSING_CAMERA[1], 1.2);
    s.to(sc, "camera.z", close, CLOSING_CAMERA[2], 1.2);
    s.to(sc, "wk.glow", close, 0.65, 0.8);
    after_chip.hide(sc, close);
    footer_after.hide(sc, close);
    cache_callout.hide(sc, close);

    scene.finish().context("cache-stage")
}

fn cache_code(narration: &Narration) -> Result<ScenePlan> {
    let diff = Diff {
        file_name: "packages/core/src/wellknown.ts",
        lines: vec![
            remove(1, "yield* bus.publish(Event.Updated, {})"),
            add(1, "yield* bus.publish(Event.Updated, {}, { global: true })"),
            keep("// … in resolve(entry, variables, credentialID, validate):"),
            keep("return resolveEntry(entry, variables).pipe("),
            keep("  Effect.provideService(HttpClient.HttpClient, http),"),
            add(
                2,
                "  Effect.tap((configs) => (validate ? validate(configs) : Effect.void)),",
            ),
            add(2, "  Effect.tap((configs) => Effect.sync(() => {"),
            add(2, "    remote.set(key, { origin: entry.origin, configs })"),
            add(2, "  })),"),
            add(3, "  Effect.catch((error) => {"),
            add(3, "    const cached = remote.get(key)"),
            add(3, "    if (!cached) return Effect.fail(error)"),
            add(3, "    return Effect.succeed(cached.configs)"),
            add(3, "  }),"),
            keep(")"),
        ],
    };
    let steps = vec![
        "publishes update events globally",
        "strict validation succeeds",
        "falls back to the last good",
    ];
    code_with(
        &PR,
        narration,
        (
            diff,
            steps,
            "condensed for display · Config.parseRemote runs strict normalization before caching",
        ),
        false,
        |_, _, _| Ok(()),
    )
}
