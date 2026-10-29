//! Showroom for Step 5: 2.5D `StageElement::RuneCard` + `AnchorPlan::Rune`!
//!
//! - A 2.5D perspective `StageElement::RuneCard` (`/connect` + `SegmentedControl`)
//!   on the GPU Stage under camera dolly and slight tilt.
//! - A Stage `Beam` and flying `Packet` docked directly to `"connect::rows/console"`,
//!   tracking `"rows/console"` as `"go"` exits and `"console"` springs upward.
//! - A `Callout` pinned via `CalloutAnchorPlan::rune` to `"rows/console"`.
//! - A glass `Lens` loupe pinned via `CalloutAnchorPlan::rune` magnifying `"seg"`
//!   as the `Tokens` indicator pill slides across it.

use std::{env, fs, path::PathBuf};

use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    lens::{LensActor, LensPlan},
    rune_surface::{KitTone, RuneAction, RuneSurfacePlan, kit},
    stage::{StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

fn main() -> Result<()> {
    let output = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("target/rune-anchors.json"), PathBuf::from);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut scene = PlanBuilder::new("rune-anchors", seconds(5.5));
    let mut hdr = header(&mut scene, "rune", "AnchorPlan::Rune & 2.5D RuneCard")?;
    hdr.show(&mut scene, 0);
    let mut tag = chip(&mut scene, "chip", Tone::Accent, "cross-layer pins")?;
    tag.show(&mut scene, millis(200));

    let card_ui = kit::dialog(
        "dialog",
        450.0,
        vec![
            kit::row(
                "hdr",
                8.0,
                vec![
                    kit::text("Connect an integration", 15.0, 600.0, KitTone::Base),
                    kit::spacer(),
                    kit::badge("esc", KitTone::Muted),
                ],
            ),
            kit::text_input("search", "", "Search 230 providers…"),
            kit::row(
                "seg-row",
                8.0,
                vec![
                    kit::text("Usage view", 12.5, 450.0, KitTone::Muted),
                    kit::spacer(),
                    kit::segmented("seg", &["Cost", "Tokens", "Web Search"], 0, Some(286.0)),
                ],
            ),
            kit::text("Popular", 11.5, 560.0, KitTone::Muted),
            kit::col(
                "rows",
                4.0,
                vec![
                    kit::list_row("go", "OpenCode Go", "API key only", KitTone::Danger, false),
                    kit::list_row(
                        "console",
                        "OpenCode Console",
                        "sign in or API key",
                        KitTone::Base,
                        true,
                    ),
                    kit::list_row(
                        "anthropic",
                        "Anthropic",
                        "Claude Max or API key",
                        KitTone::Base,
                        false,
                    ),
                ],
            ),
        ],
    );

    let rune_surface = RuneSurfacePlan::kit(card_ui)
        .with_commit(
            seconds(1.2),
            "slide to tokens",
            RuneAction::SelectSegment {
                key: "seg".into(),
                index: 1,
            },
        )
        .with_commit(
            seconds(2.4),
            "drop go row",
            RuneAction::Batch {
                actions: vec![
                    RuneAction::SetVisible {
                        key: "go".into(),
                        visible: false,
                    },
                    RuneAction::SetRow {
                        key: "console".into(),
                        title: Some("OpenCode".into()),
                        hint: Some("sign in or API key · Go included".into()),
                        tone: None,
                        hint_tone: Some(KitTone::Success),
                        selected: Some(true),
                    },
                ],
            },
        );

    let console_pin = StageElement::rune_pin("connect", "rows/console");
    let stage_plan = StagePlan {
        post: StagePost::RESTRAINED,
        elements: vec![
            StageElement::card(
                "gateway",
                [340.0, 520.0, 0.0],
                [310.0, 128.0],
                "oauth gateway",
            )
            .statuses(&[("ready", Tone::Muted), ("token issued", Tone::Success)]),
            StageElement::rune_card(
                "connect",
                [1220.0, 520.0, 35.0],
                [920.0, 620.0],
                rune_surface,
            )
            .corner(24.0),
            StageElement::beam("beam", "gateway", &console_pin).tone(Tone::Request),
            StageElement::packet("token", "beam")
                .labeled("OAuth + Go")
                .tone(Tone::Success),
        ],
    };

    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan)?;
    stage.channel(&mut scene, "beam.draw", 0.0);
    stage.channel(&mut scene, "camera.z", -90.0);
    stage.to(&mut scene, "camera.z", 0, 30.0, 2.4);
    stage.to(&mut scene, "camera.x", seconds(1.0), 40.0, 2.2);
    stage.to(&mut scene, "connect.rotation", seconds(0.4), -0.025, 1.8);

    let contact = stage.connect(&mut scene, "beam", millis(650), 0.55);
    stage.swap_status(&mut scene, "gateway", seconds(1.6), [0, 1], 0.35);
    let _arrived = stage.send(&mut scene, "token", contact + millis(350), 0.85);
    // Send a second packet AFTER "go" drops so we verify the beam & packet follow "rows/console" upward!
    let _arrived2 = stage.send(&mut scene, "token", seconds(3.0), 0.85);

    let mut callout = CalloutActor::declare(
        &mut scene,
        "row-callout",
        &CalloutPlan::new(
            CalloutAnchorPlan::rune("pin-row", "connect", "rows/console", CalloutSide::Left)
                .with_side(CalloutSide::BottomLeft),
            vec![
                CaptionSpanPlan::new("pinned to ", Tone::Muted),
                CaptionSpanPlan::new("rows/console", Tone::Success),
            ],
        )
        .elbow()
        .chip(),
    )?;
    callout.show(&mut scene, millis(800));

    let mut loupe = LensActor::declare(
        &mut scene,
        "seg-loupe",
        &LensPlan::capsule(
            CalloutAnchorPlan::rune("pin-seg", "connect", "seg/segment1", CalloutSide::Center),
            [250.0, 96.0],
        )
        .magnification(1.48),
    )?;
    loupe.show(&mut scene, seconds(1.0));
    loupe.hide(&mut scene, seconds(4.6));

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            CaptionSpanPlan::new("Stage beam, packet, Callout & Lens ", Tone::Accent),
            CaptionSpanPlan::new(
                "· locked onto live Rune keys inside a 2.5D RuneCard",
                Tone::Muted,
            ),
        ],
    )?;
    foot.show(&mut scene, millis(350));

    let plan = scene.finish()?;
    fs::write(&output, serde_json::to_string_pretty(&plan)? + "\n")?;
    eprintln!("wrote {}", output.display());
    Ok(())
}
