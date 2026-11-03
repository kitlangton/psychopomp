//! Background Jobs (`t901` #60367 + `t900` #60375): server-side steer-to-background
//! across every client and automatic `yield_after` backgrounding across `shell`,
//! `execute` (Code Mode), and `subagent`, narrated in Kit's ElevenLabs v4 voice.
//!
//! Each segment uses a distinct Psychopomp + Rune surface:
//! 1. `01-problem`     — `TerminalActor` + `RollingNumberActor` + `ChecklistActor` rail
//! 2. `02-live-card`   — 2.5D `StageElement::RuneCard` (`RuneKind::Kit`), `rune_pin`
//!                       Stage Beam/Packet tracking dynamic row relayout, & `LensActor` loupe
//! 3. `03-job-race`    — Stepped IDE `Diff` + `CursorActor` + `DiagnosticActor` + `HoverActor` + `InlayHint`
//! 4. `04-rune-diff`   — `RuneSurfaceActor` (`RuneKind::Diff` `rune_kit::DiffView` word-diff & split toggle)
//! 5. `05-seer-card`   — `RuneSurfaceActor` (`RuneKind::Note` `rune_kit::notes` Seer summary card)

mod seg1;
mod seg2;
mod seg3;
mod seg4;
mod seg5;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::millis,
    plan::{ReelPlan, ReelSegmentPlan, ReelWipePlan, WipeDirection},
};
use psychopomp_media::{ELEVEN_KIT, Media, Voice};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let media = Media::open(&root)?;
    let kit = Voice::eleven(ELEVEN_KIT)
        .v4()
        .stability(0.47)
        .similarity(0.75);

    let s1 = media.dialogue(
        "seg1_problem",
        [(
            &kit,
            "[calm, conversational] On dev today, a slow shell command blocks the whole turn and gets killed with SIGTERM after two minutes. Sending a follow-up message only moves work to the background in the web client, so the TUI and iOS wait behind the blocked tool. Even when a command starts in the background, its completion watcher runs inside the plugin scope, so reloading config drops the notice.",
        )],
    )?;

    let s2 = media.dialogue(
        "seg2_live_card",
        [(
            &kit,
            "[conversational, clear] Two stacked pull requests move both transitions into core. Pull request six oh three six seven moves blocking work to the background whenever you steer a busy session from any client, and deletes three hundred lines of web-only glue. Stacked on top, six oh three seven five adds yield after across shell, Code Mode, and subagents, so any call still running after thirty seconds returns its job ID and live output so far while continuing to run.",
        )],
    )?;

    let s3 = media.dialogue(
        "seg3_job_race",
        [(
            &kit,
            "[measured, investigative] Both paths meet inside Job dot block. If your steer lands while tool JSON is still streaming, has user steer catches it as soon as block starts. And when yield after races against completion, an independent review caught that both fibers can finish in the same tick. Checking is backgrounded under the registry lock picks one owner, so the session never gets a duplicate notice.",
        )],
    )?;

    let s4 = media.dialogue(
        "seg4_rune_diff",
        [(
            &kit,
            "[clear, direct] Inside the shell plugin, auto-backgrounding clears the implicit two-minute timeout while keeping any explicit timeout the caller passed. Completion notices now fire from global Job dot settle instead of the plugin scope, so config reloads keep every finished result. Agents also get four pinned controls, job list, job output, job wait, and job stop, and stopping a subagent cancels its nested background jobs too.",
        )],
    )?;

    let s5 = media.dialogue(
        "seg5_seer_card",
        [(
            &kit,
            "[warm, grounded] On the client side, Session dot active unions running turns with running jobs over SSE, so the spinner stays active until background work finishes. Six oh three six seven has all twenty-one CI checks green, and six oh three seven five sits right on top of it with two hundred and sixty stress-test runs passing on dev.",
        )],
    )?;

    media.finish()?;

    let reel = ReelPlan::new(
        "background-jobs",
        vec![
            ReelSegmentPlan::cut(seg1::problem(&s1)?),
            ReelSegmentPlan::wiped(
                seg2::live_card(&s2)?,
                millis(800),
                ReelWipePlan::new(WipeDirection::Left).labeled("BLOCKED & KILLED", "STEER & YIELD"),
            ),
            ReelSegmentPlan::dipped(seg3::job_race(&s3)?, millis(600)),
            ReelSegmentPlan::dipped(seg4::rune_diff(&s4)?, millis(600)),
            ReelSegmentPlan::wiped(
                seg5::seer_card(&s5)?,
                millis(750),
                ReelWipePlan::new(WipeDirection::Left).labeled("CODE RECEIPTS", "READY FOR #CORE"),
            ),
        ],
    )?;

    reel.validate().context("validate background-jobs reel")?;
    let output = root.join("background-jobs.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s across {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}
