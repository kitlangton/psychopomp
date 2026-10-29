//! #53822: retain unavailable saved session model selection.
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
    number: "#53822",
    title: "retain unavailable saved session model",
    slug: "model",
};

const SESSION_CARD: [f32; 3] = [430.0, 500.0, -40.0];
const SESSION_SIZE: [f32; 2] = [360.0, 126.0];
const CATALOG_ORB: [f32; 3] = [1020.0, 390.0, 0.0];
const FALLBACK_CARD: [f32; 3] = [1020.0, 740.0, -20.0];
const FALLBACK_SIZE: [f32; 2] = [330.0, 108.0];
const PROMPT_CARD: [f32; 3] = [1510.0, 500.0, -40.0];
const PROMPT_SIZE: [f32; 2] = [330.0, 124.0];
const CLOSING_CAMERA: [f32; 3] = [-110.0, 0.0, 60.0];

pub fn build_53822(narration_dir: &Path) -> Result<ReelPlan> {
    let narration = Narration::load(narration_dir)?;
    let stage = stage_film(&narration)?;
    let code = model_code(&narration)?;
    let reel = ReelPlan {
        version: ReelPlan::VERSION,
        id: "pr-53822".to_owned(),
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
                transition_focus: Some(session_rect()),
                transition_wipe: None,
                plan: code,
            },
        ],
    };
    reel.validate()?;
    Ok(reel)
}

fn session_rect() -> [f32; 4] {
    Camera::at(Vec3::from(CLOSING_CAMERA), vec2(1920.0, 1080.0))
        .project_rect(Vec3::from(SESSION_CARD), Vec2::from(SESSION_SIZE))
        .expect("session card in front of camera")
}

fn stage_plan() -> StagePlan {
    let elements = vec![
        StageElement::orb("catalog", CATALOG_ORB, 118.0)
            .points(800)
            .tone(Tone::Plain),
        StageElement::label(
            "cat-name",
            [CATALOG_ORB[0], 545.0, 0.0],
            22.0,
            &[("models() catalog", Tone::Plain)],
        ),
        StageElement::label(
            "cat-missing",
            [CATALOG_ORB[0], 576.0, 0.0],
            18.0,
            &[("●", Tone::Warning), (" org/model-a missing", Tone::Error)],
        ),
        StageElement::label(
            "cat-healed",
            [CATALOG_ORB[0], 576.0, 0.0],
            18.0,
            &[
                ("●", Tone::Success),
                (" org/model-a restored", Tone::Success),
            ],
        ),
        StageElement::card("session", SESSION_CARD, SESSION_SIZE, "sessionSelection")
            .statuses(&[
                ("saved: org/model-a", Tone::Plain),
                ("filtered → local/fallback", Tone::Error),
                ("retained: org/model-a", Tone::Success),
            ])
            .tone(Tone::Accent)
            .mark(Mark::Check),
        StageElement::card("fallback", FALLBACK_CARD, FALLBACK_SIZE, "newSessionModel")
            .statuses(&[
                ("fallback: local/chat", Tone::Warning),
                ("skipped for existing session", Tone::Muted),
            ])
            .tone(Tone::Warning),
        StageElement::card("prompt", PROMPT_CARD, PROMPT_SIZE, "prompt.submit")
            .statuses(&[
                ("waiting for Enter", Tone::Muted),
                ("saved model-switched!", Tone::Error),
                ("blocked: Model unavailable", Tone::Warning),
            ])
            .tone(Tone::Request),
        StageElement::beam("sess-cat", "session", "catalog").tone(Tone::Request),
        StageElement::beam("sess-fb", "session", "fallback").tone(Tone::Warning),
        StageElement::beam("sess-prompt", "session", "prompt").tone(Tone::Accent),
        StageElement::packet("miss-pkt", "sess-cat")
            .reversed()
            .labeled("!isModelValid")
            .tone(Tone::Error),
        StageElement::packet("fb-pkt", "sess-fb")
            .reversed()
            .labeled("local/chat")
            .tone(Tone::Warning),
        StageElement::packet("switch-pkt", "sess-prompt")
            .labeled("model-switched")
            .tone(Tone::Error),
        StageElement::packet("hold-pkt", "sess-prompt")
            .labeled("available = false")
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
            ("model-before", seconds(2.0)),
            ("model-after", seconds(2.0)),
        ],
    )?;
    let mut scene = PlanBuilder::new("model-stage", reading.duration());
    let [before, after] = reading.place(&mut scene);
    let b = |phrase: &str| before.at(phrase);
    let a = |phrase: &str| after.at(phrase);

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;

    s.channel(sc, "camera.z", -140.0);
    s.channel(sc, "camera.dof", 0.38);
    s.to(sc, "camera.z", 0, 0.0, 2.0);
    s.orb_in(sc, "catalog", seconds(0.15), OrbEntrance::HERO);
    s.fade_in(sc, "cat-name", seconds(0.65), 1.0, 0.45);
    s.fade_in(sc, "cat-missing", seconds(0.8), 1.0, 0.45);

    let sess_land = s.settle_in(sc, "session", seconds(0.3));
    let fb_land = s.settle_in(sc, "fallback", seconds(0.45));
    let pr_land = s.settle_in(sc, "prompt", seconds(0.6));
    s.connect(sc, "sess-cat", sess_land + seconds(0.12), 0.48);
    s.connect(sc, "sess-fb", fb_land + seconds(0.12), 0.48);
    let c3 = s.connect(sc, "sess-prompt", pr_land + seconds(0.12), 0.48);
    sfx::TICK.play(sc, "connect-0", c3, -20.0);

    header(sc, &PR, Some(seconds(0.3)))?;
    let mut before_chip = chip(sc, "chip-before", Tone::Muted, "before")?;
    before_chip.show(sc, seconds(0.55));

    let mut note = CalloutActor::declare(
        sc,
        "durable-note",
        &CalloutPlan::new(
            CalloutAnchorPlan::Stage {
                id: "prompt".into(),
                element: "prompt".into(),
                edge: CalloutSide::Top,
                side: Some(CalloutSide::Top),
            },
            vec![
                span("durable ", Tone::Error),
                span("model-switched event", Tone::Plain),
            ],
        )
        .side(CalloutSide::Top)
        .reach(58.0)
        .tone(Tone::Error)
        .chip(),
    )?;

    // Act 1: missing catalog entry filters out saved model -> fallback committed on Enter
    let missing_at = b("temporarily missing");
    s.to(sc, "catalog.hurt", missing_at, 0.65, 0.25);
    let miss_hit = s.send(sc, "miss-pkt", missing_at, 0.45);
    sfx::FAILURE.play(sc, "miss-hit", miss_hit, -10.0);

    let fell_back = b("silently fell back");
    let fb_hit = s.send(sc, "fb-pkt", fell_back - seconds(0.25), 0.45);
    s.hit(sc, "session.alarm", fb_hit, 0.25, 0.0);
    s.to(sc, "session.status", fb_hit, 1.0, 0.25);

    let enter_at = b("pressing enter");
    let sw_hit = s.send(sc, "switch-pkt", enter_at, 0.48);
    sfx::FAILURE.play(sc, "sw-hit", sw_hit, -8.0);
    s.hit(sc, "prompt.alarm", sw_hit, 0.28, 0.0);
    s.set(sc, "prompt.damage", sw_hit, 1.0);
    s.glitch(sc, "prompt", sw_hit, [7.0, 9.0, 8.0]);
    s.to(sc, "prompt.status", sw_hit, 1.0, 0.2);
    note.show(sc, b("durable model switch"));

    let recovered_at = b("catalog recovered");
    s.swap_labels(sc, ["cat-missing", "cat-healed"], recovered_at, millis(220));
    s.to(sc, "catalog.hurt", recovered_at, 0.0, 0.35);

    let stuck_at = b("stayed stuck");
    let mut footer_before = footer(
        sc,
        "footer-before",
        vec![
            span("catalog heals, but session stays ", Tone::Plain),
            span("stuck on fallback", Tone::Error),
        ],
    )?;
    footer_before.type_in(sc, stuck_at, 46.0, 0.7);

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

    s.swap_labels(
        sc,
        ["cat-healed", "cat-missing"],
        switch + seconds(0.3),
        millis(200),
    );
    s.set(sc, "prompt.damage", switch + seconds(0.4), 0.0);
    s.to(sc, "prompt.status", switch + seconds(0.4), 0.0, 0.25);
    s.to(sc, "session.status", switch + seconds(0.4), 2.0, 0.25);
    s.to(sc, "fallback.status", switch + seconds(0.4), 1.0, 0.25);
    s.to(sc, "fallback.dim", switch + seconds(0.4), 0.5, 0.4);

    // Act 2: saved model retained, Enter blocked cleanly with Model unavailable
    let retain_at = a("retain their saved model");
    s.to(sc, "session.glow", retain_at, 0.55, 0.45);

    let false_at = a("becomes false");
    let hold_hit = s.send(sc, "hold-pkt", false_at, 0.45);
    s.hit(sc, "prompt.flash", hold_hit, 0.35, 0.0);
    s.to(sc, "prompt.status", hold_hit, 2.0, 0.25);
    sfx::BLOOM.play(sc, "hold-bloom", hold_hit, -10.0);

    let block_at = a("model unavailable");
    let mut footer_after = footer(
        sc,
        "footer-after",
        vec![
            span("saved model retained · ", Tone::Plain),
            span("Model unavailable", Tone::Warning),
            span(" blocks silent switch", Tone::Plain),
        ],
    )?;
    footer_after.type_in(sc, block_at, 44.0, 0.75);

    let close = after.end() + seconds(0.3);
    s.to(sc, "camera.x", close, CLOSING_CAMERA[0], 1.2);
    s.to(sc, "camera.y", close, CLOSING_CAMERA[1], 1.2);
    s.to(sc, "camera.z", close, CLOSING_CAMERA[2], 1.2);
    s.to(sc, "session.glow", close, 0.65, 0.8);
    after_chip.hide(sc, close);
    footer_after.hide(sc, close);

    scene.finish().context("model-stage")
}

fn model_code(narration: &Narration) -> Result<ScenePlan> {
    let diff = Diff {
        file_name: "packages/tui/src/context/local.tsx",
        lines: vec![
            keep("const selected = ["),
            keep("  selectionState.selectionBySessionAgent[sessionID]?.[current.id],"),
            keep("  !session?.agent || session.agent === current.id"),
            keep("    ? durableSelection(sessionID) : undefined,"),
            remove(
                1,
                "].find((selection) => selection && isModelValid(selection))",
            ),
            add(1, "].find((selection) => selection !== undefined)"),
            keep("if (selected) {"),
            keep("  const info = models()?.find((item) =>"),
            keep("    item.providerID === selected.providerID && item.id === selected.modelID)"),
            keep("  return {"),
            keep("    ...selected,"),
            remove(
                2,
                "    variant: info?.variants.some((v) => v.id === selected.variant)",
            ),
            add(
                2,
                "    variant: !info || info.variants.some((v) => v.id === selected.variant)",
            ),
            keep("      ? selected.variant : undefined,"),
            keep("  }"),
            keep("}"),
        ],
    };
    let steps = vec![
        "drops the model validity filter",
        "preserves the saved variant",
    ];
    code_with(
        &PR,
        narration,
        (
            diff,
            steps,
            "condensed for display · newSessionModel still filters unavailable CLI and recent models",
        ),
        false,
        |_, _, _| Ok(()),
    )
}
