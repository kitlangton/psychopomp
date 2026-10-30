use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    lens::{LensActor, LensPlan},
    plan::ScenePlan,
    rune_surface::{KitTone, RuneAction, RuneSurfaceActor, RuneSurfacePlan, kit},
    sfx,
    stage::{StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 5: Live Seer card rendered on `wgpu` (`RuneKind::Note`) for Fix #3:
/// Gating Seer `Needs you` and stopping the 10-repo Rune recompile storm.
pub fn seer_rune(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1500);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("05-seer-rune", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "fix 3 · seer & rune",
        "Pre-gate Seer reviews and pin Rune once daily",
    )?;
    chip(&mut scene, "chip", Tone::Accent, "46.3% → <20% rework")?;

    let dead_at = spoken.at_any(&["fifty-two", "dead sessions", "deleted worktree"]);
    let gate_at = spoken.at_any(&["Gating Seer", "headless Rune", "zero-delta"]);
    let pin_at = spoken.at_any(&["thirty-four", "daily pinned", "eleven-gigabyte"]);

    let initial_md = "\
Verify UI diffs automatically before waking Kit, and pin Rune once daily across apps.

```facts
Seer rework rate: {verified} 365 / 789 cards (46.3%) needed 2+ Kit rounds
Zombie working cards: {verified} 52 / 80 have no active session · main-watch exit 78
Rune git-SHA churn: {verified} 34 checkouts · 478 bumps · 11.3 GB rustc spikes
```

```compare
Today (unverified): 0%-delta stills reach Kit & every bump recompiles wgpu x10.
Pre-gated + pinned: Rune --snap diff gate + 1 daily rune-pinned tag on Pro/mini.
```";

    let updated_md = format!(
        "{initial_md}\n\n```steps\n{{built}} Reject 0%-pixel-delta stills & text overflows before Needs-you\n{{built}} Reap dead working sessions every 2m + fix main-watch path\n{{built}} Cut 1 daily rune-pinned tag + cap Studio to 2 local Cargo jobs\n```"
    );

    let mut note = RuneSurfaceActor::declare(
        &mut scene,
        "seer-gate-note",
        RuneSurfacePlan::note(initial_md)
            .titled("RSI Fix #3 · Seer Pre-Gate & Daily Rune Pin")
            .width(650.0)
            .scale_factor(1.56)
            .at([960.0, 520.0]),
    )?;
    note.settle_in(&mut scene, millis(80));
    note.commit(
        &mut scene,
        gate_at,
        "reveal 3 steps",
        RuneAction::SetMarkdown {
            markdown: updated_md,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("41 open OpenCode PRs → ", Tone::Muted),
            sp("kit-live shadow batch card", Tone::Accent),
            sp(" · 40–60% less Cargo compile volume", Tone::Plain),
        ],
    )?;
    foot.show(&mut scene, pin_at);

    sfx::FAILURE.play(&mut scene, "dead-warn", dead_at, -16.5);
    sfx::CONFIRM.play(&mut scene, "gate-commit", gate_at, -15.0);
    sfx::MARK.play(&mut scene, "pin-mark", pin_at, -16.0);

    scene.cue("seer-rune", 0, duration);
    Ok(scene.finish()?)
}

fn select_step(index: usize) -> RuneAction {
    let keys = ["f1", "f2", "f3"];
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

/// Segment 6: 2.5D Perspective `StageElement::RuneCard` (`RuneKind::Kit`) +
/// `LensActor` glass loupe stepping through the 3 fixes in execution order.
pub fn close_plan(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1800);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("06-close-plan", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "execution order",
        "Three Kaizen moves to clear the 63% landing bottleneck",
    )?;
    chip(&mut scene, "chip", Tone::Success, "single-pass flow")?;

    let m1_at = spoken.at_any(&["Unstick", "cluster", "merge trains"]);
    let m2_at = spoken.at_any(&["sweep thirty", "thirty gigabytes", "Motel noise"]);
    let m3_at = spoken.at_any(&["gate Seer", "Rune churn", "single pass"]);

    let card_ui = kit::dialog(
        "dialog",
        492.0,
        vec![
            kit::row(
                "hdr",
                8.0,
                vec![
                    kit::text("Engineering RSI · Next 3 Moves", 15.0, 600.0, KitTone::Base),
                    kit::spacer(),
                    kit::badge("ready", KitTone::Success),
                ],
            ),
            kit::row(
                "seg-row",
                8.0,
                vec![
                    kit::text("Order", 12.0, 500.0, KitTone::Muted),
                    kit::spacer(),
                    kit::segmented(
                        "tabs",
                        &["1 · Cluster", "2 · 30 GB", "3 · Seer/Rune"],
                        0,
                        Some(360.0),
                    ),
                ],
            ),
            kit::col(
                "rows",
                6.0,
                vec![
                    kit::list_row(
                        "f1",
                        "1 · Fix cluster.rs:2460 Livelock",
                        "Cargo p95: 15m → < 2m",
                        KitTone::Base,
                        true,
                    ),
                    kit::list_row(
                        "f2",
                        "2 · Reclaim 30 GB & Filter Motel",
                        "swap < 2 GB · 30 MB/day",
                        KitTone::Base,
                        false,
                    ),
                    kit::list_row(
                        "f3",
                        "3 · Pre-Gate Seer & Pin Rune Daily",
                        "rework < 20% · 0 storms",
                        KitTone::Base,
                        false,
                    ),
                ],
            ),
        ],
    );

    let rune_surface = RuneSurfacePlan::kit(card_ui)
        .with_commit(m1_at, "move 1", select_step(0))
        .with_commit(m2_at, "move 2", select_step(1))
        .with_commit(m3_at, "move 3", select_step(2));

    let tabs_pin = StageElement::rune_pin("plan-card", "tabs");
    let stage_plan = StagePlan {
        post: StagePost::RESTRAINED,
        elements: vec![
            StageElement::card(
                "desk",
                [310.0, 520.0, 0.0],
                [325.0, 136.0],
                "Kit's desk queue",
            )
            .statuses(&[
                ("63% waiting · 7 deep", Tone::Error),
                ("1 · merge train live", Tone::Accent),
                ("2 · +6 Pro slots cool", Tone::Request),
                ("3 · single-pass ship", Tone::Success),
            ]),
            StageElement::rune_card(
                "plan-card",
                [1140.0, 525.0, 28.0],
                [990.0, 580.0],
                rune_surface,
            )
            .framed()
            .corner(24.0),
            StageElement::beam("beam", "desk", &tabs_pin).tone(Tone::Accent),
            StageElement::packet("pkt", "beam")
                .labeled("ship")
                .tone(Tone::Success),
        ],
    };

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan)?;
    stage.channel(&mut scene, "beam.draw", 0.0);
    stage.channel(&mut scene, "camera.z", -55.0);
    stage.to(&mut scene, "camera.z", 0, 22.0, 2.2);
    stage.to(&mut scene, "plan-card.rotation", millis(300), -0.02, 1.8);

    let wired = stage.connect(&mut scene, "beam", millis(420), 0.55);

    for (idx, at) in [(1, m1_at), (2, m2_at), (3, m3_at)] {
        let fire = at.max(wired + millis(200));
        stage.swap_status(&mut scene, "desk", fire, [idx - 1, idx], 0.35);
        stage.send(&mut scene, "pkt", fire, 0.72);
        sfx::MARK.play(&mut scene, format!("step-mark-{idx}"), fire, -15.5);
    }

    let mut callout = CalloutActor::declare(
        &mut scene,
        "done-pin",
        &CalloutPlan::new(
            CalloutAnchorPlan::rune("pin-f3", "plan-card", "rows/f3", CalloutSide::Bottom)
                .with_side(CalloutSide::Bottom),
            vec![
                sp("63% wait → calm single-pass cockpit ", Tone::Success),
                sp("· zero new features required", Tone::Muted),
            ],
        )
        .reach(82.0)
        .chip(),
    )?;
    callout.show(&mut scene, m3_at + millis(550));

    let mut loupe = LensActor::declare(
        &mut scene,
        "step-loupe",
        &LensPlan::capsule(
            CalloutAnchorPlan::rune("s0", "plan-card", "tabs/segment0", CalloutSide::Center),
            [258.0, 90.0],
        )
        .anchor(CalloutAnchorPlan::rune(
            "s1",
            "plan-card",
            "tabs/segment1",
            CalloutSide::Center,
        ))
        .anchor(CalloutAnchorPlan::rune(
            "s2",
            "plan-card",
            "tabs/segment2",
            CalloutSide::Center,
        ))
        .magnification(1.42),
    )?;
    loupe.show(&mut scene, m1_at);
    loupe.move_to(&mut scene, "s1", m2_at)?;
    loupe.move_to(&mut scene, "s2", m3_at)?;
    loupe.hide(&mut scene, duration.saturating_sub(millis(900)));

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("1 cluster.rs:2460 · ", Tone::Accent),
            sp("2 30 GB + Motel filter · ", Tone::Plain),
            sp("3 Seer snap-gate & daily Rune pin", Tone::Success),
        ],
    )?;
    foot.show(&mut scene, m2_at);

    sfx::BLOOM.play(&mut scene, "final-bloom", m3_at + millis(550), -15.5);

    scene.cue("close-plan", 0, duration);
    Ok(scene.finish()?)
}
