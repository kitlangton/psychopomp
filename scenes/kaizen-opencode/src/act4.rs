use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    face::Face,
    lens::{LensActor, LensPlan},
    math::easing::Ease,
    plan::{ScenePlan, SpringPlan},
    rune_surface::{KitTone, RuneAction, RuneSurfacePlan, kit},
    sfx,
    stage::{StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn select_proposal(index: usize) -> RuneAction {
    let keys = ["p1", "p2", "p3", "p4"];
    let mut actions = vec![RuneAction::SelectSegment {
        key: "tabs".into(),
        index,
    }];
    for (i, key) in keys.into_iter().enumerate() {
        actions.push(RuneAction::SetSelected {
            key: key.into(),
            selected: i == index,
        });
    }
    RuneAction::Batch { actions }
}

/// Segment 7: 2.5D Perspective `StageElement::RuneCard` (`RuneKind::Kit`) +
/// docked `rune_pin` Stage Beam/Packet + Snell's-law `LensActor` glass loupe.
pub fn proposals(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("07-proposals", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "product proposals",
        "Four Kaizen primitives to ship natively in OpenCode",
    )?;
    chip(&mut scene, "chip", Tone::Accent, "opencode roadmap")?;

    let p1_at = spoken.at_any(&["First", "andon breaker", "built-in andon"]);
    let p2_at = spoken.at_any(&["Second", "writer lease", "exclusive database"]);
    let p3_at = spoken.at_any(&["Third", "wide events", "slash kaizen"]);
    let p4_at = spoken.at_any(&["Fourth", "pull queue", "skill files"]);

    let card_ui = kit::dialog(
        "dialog",
        488.0,
        vec![
            kit::row(
                "hdr",
                8.0,
                vec![
                    kit::text("OpenCode · Built-In Kaizen", 15.0, 600.0, KitTone::Base),
                    kit::spacer(),
                    kit::badge("v2 product", KitTone::Accent),
                ],
            ),
            kit::row(
                "seg-row",
                8.0,
                vec![
                    kit::text("Pillar", 12.0, 500.0, KitTone::Muted),
                    kit::spacer(),
                    kit::segmented(
                        "tabs",
                        &["Andon", "Lease", "/kaizen", "WIP Pull"],
                        0,
                        Some(360.0),
                    ),
                ],
            ),
            kit::col(
                "rows",
                5.0,
                vec![
                    kit::list_row(
                        "p1",
                        "1 · Turn Andon Breaker",
                        "stop loops → 1-tap card",
                        KitTone::Base,
                        true,
                    ),
                    kit::list_row(
                        "p2",
                        "2 · Single-Writer & Hooks",
                        "db.lock + --bins pre-push",
                        KitTone::Base,
                        false,
                    ),
                    kit::list_row(
                        "p3",
                        "3 · Wide Events & /kaizen",
                        "spans → top p95 queue",
                        KitTone::Base,
                        false,
                    ),
                    kit::list_row(
                        "p4",
                        "4 · Event Pull & Skill Ratchet",
                        "wake queue + SKILL deltas",
                        KitTone::Base,
                        false,
                    ),
                ],
            ),
        ],
    );

    let rune_surface = RuneSurfacePlan::kit(card_ui)
        .with_commit(p1_at, "proposal 1", select_proposal(0))
        .with_commit(p2_at, "proposal 2", select_proposal(1))
        .with_commit(p3_at, "proposal 3", select_proposal(2))
        .with_commit(p4_at, "proposal 4", select_proposal(3));

    let tabs_pin = StageElement::rune_pin("roadmap", "tabs");
    let stage_plan = StagePlan {
        post: StagePost::RESTRAINED,
        elements: vec![
            StageElement::card(
                "harness",
                [310.0, 520.0, 0.0],
                [320.0, 136.0],
                "agent harness",
            )
            .statuses(&[
                ("personal scripts today", Tone::Muted),
                ("1 · stop on 1st defect", Tone::Error),
                ("2 · zero split-brain DB", Tone::Accent),
                ("3 · p95 queue visible", Tone::Request),
                ("4 · cache-safe pull WIP", Tone::Success),
            ]),
            StageElement::rune_card(
                "roadmap",
                [1140.0, 525.0, 28.0],
                [990.0, 620.0],
                rune_surface,
            )
            .framed()
            .corner(24.0),
            StageElement::beam("beam", "harness", &tabs_pin).tone(Tone::Accent),
            StageElement::packet("pkt", "beam")
                .labeled("ship")
                .tone(Tone::Accent),
        ],
    };

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan)?;
    stage.channel(&mut scene, "beam.draw", 0.0);
    stage.channel(&mut scene, "camera.z", -60.0);
    stage.to(&mut scene, "camera.z", 0, 20.0, 2.2);
    stage.to(&mut scene, "roadmap.rotation", millis(300), -0.02, 1.8);

    let wired = stage.connect(&mut scene, "beam", millis(450), 0.55);

    for (idx, at) in [(1, p1_at), (2, p2_at), (3, p3_at), (4, p4_at)] {
        let fire = at.max(wired + millis(200));
        stage.swap_status(&mut scene, "harness", fire, [idx - 1, idx], 0.35);
        stage.send(&mut scene, "pkt", fire, 0.72);
        sfx::MARK.play(&mut scene, format!("prop-mark-{idx}"), fire, -16.0);
    }

    let mut callout = CalloutActor::declare(
        &mut scene,
        "pin-note",
        &CalloutPlan::new(
            CalloutAnchorPlan::rune("pin-p4", "roadmap", "rows/p4", CalloutSide::Bottom)
                .with_side(CalloutSide::Bottom),
            vec![
                sp("built into OpenCode ", Tone::Success),
                sp("· zero custom shell glue", Tone::Muted),
            ],
        )
        .reach(44.0)
        .chip(),
    )?;
    callout.show(&mut scene, p4_at + millis(500));

    let mut loupe = LensActor::declare(
        &mut scene,
        "tabs-loupe",
        &LensPlan::capsule(
            CalloutAnchorPlan::rune("pin-seg0", "roadmap", "tabs/segment0", CalloutSide::Center),
            [230.0, 88.0],
        )
        .anchor(CalloutAnchorPlan::rune(
            "pin-seg1",
            "roadmap",
            "tabs/segment1",
            CalloutSide::Center,
        ))
        .anchor(CalloutAnchorPlan::rune(
            "pin-seg2",
            "roadmap",
            "tabs/segment2",
            CalloutSide::Center,
        ))
        .anchor(CalloutAnchorPlan::rune(
            "pin-seg3",
            "roadmap",
            "tabs/segment3",
            CalloutSide::Center,
        ))
        .magnification(1.42),
    )?;
    loupe.show(&mut scene, p1_at);
    loupe.move_to(&mut scene, "pin-seg1", p2_at)?;
    loupe.move_to(&mut scene, "pin-seg2", p3_at)?;
    loupe.move_to(&mut scene, "pin-seg3", p4_at)?;
    loupe.hide(&mut scene, duration.saturating_sub(millis(900)));

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("1 Andon · ", Tone::Accent),
            sp("2 DB & Hook Lease · ", Tone::Plain),
            sp("3 Wide-Event /kaizen · ", Tone::Plain),
            sp("4 Pull WIP & Skill Ratchet", Tone::Success),
        ],
    )?;
    foot.show(&mut scene, p2_at);

    scene.cue("proposals", 0, duration);
    Ok(scene.finish()?)
}

/// Segment 8: 3D `Form` particle convergence (`plane → box → sphere`) +
/// monumental `Face::Shout` / `Face::SansBold` close.
pub fn close(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1800);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("08-close", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "kaizen × opencode",
        "Fix quality by changing the line the work runs on",
    )?;
    chip(&mut scene, "chip", Tone::Success, "215 → 5,436 / wk")?;

    let build_at = spoken.at_any(&["Building", "jidoka", "poka-yoke"]);
    let line_at = spoken.at_any(&["changing the line", "changing", "line"]);

    let mut plan: StagePlan = serde_json::from_value(serde_json::json!({
        "post": {
            "bloom": 0.16,
            "grain": 0.012,
            "vignette": 0.22,
            "backdrop": 0.10
        },
        "elements": [
            {
                "kind": "form",
                "id": "crystal",
                "at": [490.0, 495.0, 0.0],
                "points": 780,
                "tone": "accent",
                "shapes": [
                    { "shape": "plane", "size": [360.0, 210.0] },
                    { "shape": "box", "size": [224.0, 224.0, 224.0], "edges": 0.65 },
                    { "shape": "sphere", "radius": 148.0 }
                ]
            },
            {
                "kind": "form",
                "id": "slab",
                "at": [490.0, 735.0, 0.0],
                "points": 460,
                "tone": "muted",
                "shapes": [
                    { "shape": "box", "size": [430.0, 14.0, 250.0], "edges": 0.7 }
                ]
            }
        ]
    }))?;

    plan.elements.extend([
        StageElement::label(
            "hero-num",
            [1245.0, 395.0, 10.0],
            92.0,
            &[("215 → 5,436", Tone::Accent)],
        )
        .face(Face::Shout),
        StageElement::label(
            "hero-sub",
            [1245.0, 482.0, 10.0],
            24.0,
            &[("COMMITS PER WEEK · ANOMALY 2026", Tone::Muted)],
        )
        .face(Face::SansBold),
        StageElement::label(
            "law-1",
            [1245.0, 596.0, 10.0],
            34.0,
            &[("STOP THE LINE · GO AND SEE", Tone::Plain)],
        )
        .face(Face::SansBold),
        StageElement::label(
            "law-2",
            [1245.0, 662.0, 10.0],
            32.0,
            &[("PREVENT > DETECT > INSTRUCT", Tone::Success)],
        )
        .face(Face::SansBold),
    ]);

    let mut stage = StageActor::declare(&mut scene, "stage", &plan)?;
    stage.channel(&mut scene, "camera.z", -50.0);
    stage.to(&mut scene, "camera.z", 0, 24.0, 2.6);

    for (id, at) in [("law-1", build_at), ("law-2", line_at)] {
        let op = stage.channel(&mut scene, &format!("{id}.opacity"), 0.0);
        scene.ease(&op, at, 1.0, 0.28, Ease::GLIDE);
        let y = stage.channel(&mut scene, &format!("{id}.y"), 14.0);
        scene.spring_with(&y, at, 0.0, SpringPlan::ENTER);
    }

    let boxed = stage.morph(&mut scene, "crystal", build_at, 1, 1.5);
    stage.ease(
        &mut scene,
        "crystal.pitch",
        build_at + millis(150),
        std::f32::consts::FRAC_PI_4,
        1.5,
        Ease::Smootherstep,
    );
    stage.ease(
        &mut scene,
        "crystal.rotation",
        build_at + millis(150),
        0.55,
        1.8,
        Ease::Smootherstep,
    );

    let sphere_at = line_at.max(boxed + millis(200));
    stage.morph(&mut scene, "crystal", sphere_at, 2, 1.6);

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("Ohno · Imai · Liker · ", Tone::Muted),
            sp("Spear & Bowen · Shingo · Rother · Deming", Tone::Plain),
        ],
    )?;
    foot.show(&mut scene, line_at);

    sfx::WHOOSH.play(&mut scene, "morph-box", build_at, -16.5);
    sfx::BLOOM.play(&mut scene, "morph-sphere", sphere_at, -15.0);

    let _ = seconds(1.0);
    scene.cue("close", 0, duration);
    Ok(scene.finish()?)
}
