use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    lens::{LensActor, LensPlan},
    plan::ScenePlan,
    rune_surface::{KitTone, RuneAction, RuneSurfacePlan, kit},
    sfx,
    stage::{StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 2: 2.5D Perspective `StageElement::RuneCard` (`RuneKind::Kit`) +
/// docked `rune_pin` Stage Beam/Packet tracking dynamic row relayout +
/// Snell's-law `LensActor` glass loupe gliding across segments.
pub fn live_card(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("02-live-card", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "#60367 + #60375",
        "Server steer & 30s yield_after",
    )?;
    chip(&mut scene, "chip", Tone::Accent, "stacked on dev")?;

    let steer_at = spoken.at_any(&[
        "six oh three six seven",
        "steer a busy",
        "whenever you steer",
    ]);
    let delete_at = spoken.at_any(&["deletes", "three hundred", "web-only"]);
    let yield_at = spoken.at_any(&["six oh three seven five", "yield after", "thirty seconds"]);
    let running_at = spoken.at_any(&["job ID", "live output", "continuing"]);

    let card_ui = kit::dialog(
        "dialog",
        500.0,
        vec![
            kit::row(
                "hdr",
                8.0,
                vec![
                    kit::text("Session · Active Jobs", 15.0, 600.0, KitTone::Base),
                    kit::spacer(),
                    kit::badge("busy · 3 jobs", KitTone::Accent),
                ],
            ),
            kit::row(
                "seg-row",
                8.0,
                vec![
                    kit::text("Lifecycle", 12.0, 500.0, KitTone::Muted),
                    kit::spacer(),
                    kit::segmented(
                        "tabs",
                        &["Blocking", "Steered", "Yield 30s"],
                        0,
                        Some(330.0),
                    ),
                ],
            ),
            kit::col(
                "rows",
                5.0,
                vec![
                    kit::list_row(
                        "glue",
                        "app/steer-background.ts",
                        "334 LOC web-only glue",
                        KitTone::Danger,
                        false,
                    ),
                    kit::list_row(
                        "sh",
                        "sh_01 · cargo test",
                        "blocking turn",
                        KitTone::Base,
                        true,
                    ),
                    kit::list_row(
                        "ex",
                        "ex_02 · execute (Code Mode)",
                        "yield_after: 30s",
                        KitTone::Base,
                        false,
                    ),
                    kit::list_row(
                        "sub",
                        "ses_c3 · subagent",
                        "yield_after: 30s",
                        KitTone::Base,
                        false,
                    ),
                ],
            ),
        ],
    );

    let rune_surface = RuneSurfacePlan::kit(card_ui)
        .with_commit(
            steer_at,
            "steer moves blocking work to background",
            RuneAction::Batch {
                actions: vec![
                    RuneAction::SelectSegment {
                        key: "tabs".into(),
                        index: 1,
                    },
                    RuneAction::SetRow {
                        key: "sh".into(),
                        title: None,
                        hint: Some("steered → background".into()),
                        tone: None,
                        hint_tone: Some(KitTone::Accent),
                        selected: Some(true),
                    },
                ],
            },
        )
        .with_commit(
            delete_at,
            "delete web-only steer-background.ts",
            RuneAction::SetVisible {
                key: "glue".into(),
                visible: false,
            },
        )
        .with_commit(
            yield_at,
            "30s yield_after across all 3 tools",
            RuneAction::Batch {
                actions: vec![
                    RuneAction::SelectSegment {
                        key: "tabs".into(),
                        index: 2,
                    },
                    RuneAction::SetRow {
                        key: "sh".into(),
                        title: None,
                        hint: Some("running · jobID + snapshot".into()),
                        tone: None,
                        hint_tone: Some(KitTone::Success),
                        selected: Some(false),
                    },
                    RuneAction::SetRow {
                        key: "ex".into(),
                        title: None,
                        hint: Some("running · logs + toolCalls".into()),
                        tone: None,
                        hint_tone: Some(KitTone::Success),
                        selected: Some(true),
                    },
                    RuneAction::SetRow {
                        key: "sub".into(),
                        title: None,
                        hint: Some("running · child progress".into()),
                        tone: None,
                        hint_tone: Some(KitTone::Success),
                        selected: Some(false),
                    },
                ],
            },
        );

    let sh_pin = StageElement::rune_pin("jobs", "rows/sh");
    let stage_plan = StagePlan {
        post: StagePost::RESTRAINED,
        elements: vec![
            StageElement::card(
                "session",
                [300.0, 520.0, 0.0],
                [370.0, 142.0],
                "Session.prompt / block",
            )
            .statuses(&[
                ("foreground turn", Tone::Muted),
                ("#60367 · user steer", Tone::Accent),
                ("−334 LOC client glue", Tone::Success),
                ("#60375 · 30s yield_after", Tone::Request),
                ("returns { jobID, output }", Tone::Success),
            ]),
            StageElement::rune_card("jobs", [1155.0, 525.0, 30.0], [990.0, 620.0], rune_surface)
                .framed()
                .corner(24.0),
            StageElement::beam("beam", "session", &sh_pin).tone(Tone::Accent),
            StageElement::packet("steer-pkt", "beam")
                .labeled("steer")
                .tone(Tone::Accent),
            StageElement::packet("yield-pkt", "beam")
                .labeled("yield 30s")
                .tone(Tone::Success),
        ],
    };

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan)?;
    stage.channel(&mut scene, "beam.draw", 0.0);
    stage.channel(&mut scene, "camera.z", -65.0);
    stage.to(&mut scene, "camera.z", 0, 22.0, 2.3);
    stage.to(&mut scene, "jobs.rotation", millis(300), -0.022, 1.8);

    let wired = stage.connect(&mut scene, "beam", millis(450), 0.55);

    let fire_steer = steer_at.max(wired + millis(200));
    stage.swap_status(&mut scene, "session", fire_steer, [0, 1], 0.35);
    stage.send(&mut scene, "steer-pkt", fire_steer, 0.75);

    let fire_del = delete_at.max(fire_steer + millis(900));
    stage.swap_status(&mut scene, "session", fire_del, [1, 2], 0.35);

    // After "glue" exits and "rows/sh" springs upward, this second packet tracks the new pin position!
    let fire_yield = yield_at.max(fire_del + millis(900));
    stage.swap_status(&mut scene, "session", fire_yield, [2, 3], 0.35);
    stage.send(&mut scene, "yield-pkt", fire_yield, 0.78);

    let fire_ret = running_at.max(fire_yield + millis(1100));
    stage.swap_status(&mut scene, "session", fire_ret, [3, 4], 0.35);

    let mut callout = CalloutActor::declare(
        &mut scene,
        "pin-note",
        &CalloutPlan::new(
            CalloutAnchorPlan::rune("pin-sub", "jobs", "rows/sub", CalloutSide::Bottom)
                .with_side(CalloutSide::Bottom),
            vec![
                sp("TUI, Desktop, Web, iOS & SDK ", Tone::Success),
                sp("· one server path", Tone::Muted),
            ],
        )
        .reach(44.0)
        .chip(),
    )?;
    callout.show(&mut scene, fire_ret);

    let mut loupe = LensActor::declare(
        &mut scene,
        "tabs-loupe",
        &LensPlan::capsule(
            CalloutAnchorPlan::rune("pin-seg0", "jobs", "tabs/segment0", CalloutSide::Center),
            [230.0, 88.0],
        )
        .anchor(CalloutAnchorPlan::rune(
            "pin-seg1",
            "jobs",
            "tabs/segment1",
            CalloutSide::Center,
        ))
        .anchor(CalloutAnchorPlan::rune(
            "pin-seg2",
            "jobs",
            "tabs/segment2",
            CalloutSide::Center,
        ))
        .magnification(1.42),
    )?;
    loupe.show(&mut scene, lead + millis(350));
    loupe.move_to(&mut scene, "pin-seg1", fire_steer)?;
    loupe.move_to(&mut scene, "pin-seg2", fire_yield)?;
    loupe.hide(&mut scene, duration.saturating_sub(millis(850)));

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("anomalyco/opencode#60367", Tone::Accent),
            sp(" (+867 / −334) → ", Tone::Muted),
            sp("anomalyco/opencode#60375", Tone::Success),
            sp(" (+2,821 / −353)", Tone::Muted),
        ],
    )?;
    foot.show(&mut scene, fire_steer);

    sfx::MARK.play(&mut scene, "sfx-steer", fire_steer, -15.5);
    sfx::WHOOSH.play(&mut scene, "sfx-drop", fire_del, -17.0);
    sfx::CONFIRM.play(&mut scene, "sfx-yield", fire_yield, -15.0);

    scene.cue("live-card", 0, duration);
    Ok(scene.finish()?)
}
