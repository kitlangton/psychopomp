//! Kaizen Video #2 (`kaizen-rsi-v2`): Turning the Oct 29 Carbon RSI Engineering
//! Report (`2026-10-29-rsi-engineering/index.md`) into a 6-segment multi-form
//! Psychopomp film.
//!
//! 1. `01-inversion`    — `RollingNumberActor` + `BarsActor` (1.5% vs 63% desk wait)
//! 2. `02-livelock-rig` — Geometric 10-Core Worker & Queue Gate Rig on `StageActor`
//! 3. `03-livelock-diff`— Stepped `Diff` (`hivemind/src/cluster.rs:2460`) + `HoverActor`
//! 4. `04-reclaim-30gb` — Procedural `Grid` Banded Table ↔ 3D `Layers` (30 GB + Motel)
//! 5. `05-seer-rune`    — `RuneSurfaceActor` (`RuneKind::Note` Seer card on `wgpu`)
//! 6. `06-close-plan`   — 2.5D `StageElement::RuneCard` (`RuneKind::Kit`) + `LensActor`

mod part1;
mod part2;
mod part3;

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
        .stability(0.48)
        .similarity(0.75);

    let s1 = media.dialogue(
        "seg1_inversion",
        [(
            &kit,
            "[calm, conversational] Over the past seven days, I ran two thousand human turns and twenty-eight hundred subagents across twenty repos. Today's system trace found an upside-down bottleneck. I wait on running agents for just one point five percent of my sixty-eight desk hours, while finished agent work sits waiting on me for sixty-three percent of the day, backed up seven sessions deep because forty-six percent of Seer cards need multiple feedback rounds and sixty-four percent of HiveMind fleet time goes to Cargo queue wait.",
        )],
    )?;

    let s2 = media.dialogue(
        "seg2_livelock_rig",
        [(
            &kit,
            "[measured, investigative] Fix number one targets a head-of-line livelock in HiveMind. Live telemetry caught a high-priority Cargo job needing seven cores stalled for forty-seven minutes while nine two-core jobs waited behind it. The Mac mini had one free slot and two free cores open right then, enough to run those small jobs immediately, yet every two-core job yielded that slot to the seven-core waiter ahead of it, which could not fit. Reserving core drain on one machine while letting smaller jobs use free cores elsewhere clears the jam.",
        )],
    )?;

    let s3 = media.dialogue(
        "seg3_livelock_diff",
        [(
            &kit,
            "[clear, direct] Inside cluster dot R S at line twenty-four sixty, waiters ahead counts every queued ticket regardless of core fit. Filtering that count to jobs the candidate worker can actually run once a slot opens, and draining one worker after two minutes for a large high-priority build, cuts daytime Cargo P ninety-five queue wait from fifteen minutes to under two and unblocks seer land and Hex auto-installs.",
        )],
    )?;

    let s4 = media.dialogue(
        "seg4_reclaim_30gb",
        [(
            &kit,
            "[conversational, dryly amused] Fix number two reclaims thirty gigabytes of leaked memory and stops a silent telemetry flood. My thirty-six gigabyte Mac Studio is carrying nearly nine gigabytes of swap while holding idle cua-driver, ChatGPT and Codex helpers, thirteen orphaned workerd test servers, five zombie nexus daemons, and twelve CPU hours of Spotlight indexing worktrees. Meanwhile the ninety-six gigabyte MacBook Pro is thermally throttled under an eighteen gigabyte virtual machine, a five point six gigabyte leaked spaceman agent, and forgotten dev servers, while OpenCode writes six point five million file-system spans a day into Motel. Sweeping those ghosts and dropping sub-ten-millisecond file spans cuts Motel writes from one point seven gigabytes a day to thirty megabytes and brings six fast Pro build slots back online.",
        )],
    )?;

    let s5 = media.dialogue(
        "seg5_seer_rune",
        [(
            &kit,
            "[firm, direct] Fix number three protects my review queue and stops ten repos from recompiling Rune all day. Forty-six percent of Seer cards take two or more rounds, fifty-two of eighty working cards belong to dead sessions, and seer main watch is dead on a deleted worktree path. Gating Seer reviews on a headless Rune snapshot diff rejects zero-delta stills and text overflows automatically, while seer nudge reaps dead sessions every two minutes. At the same time, replacing per-commit Rune git bumps across thirty-four checkouts with one daily pinned tag and shared target caches stops eleven-gigabyte rustc storms on the Studio.",
        )],
    )?;

    let s6 = media.dialogue(
        "seg6_close_plan",
        [(
            &kit,
            "[warm, grounded] Those three moves attack the real constraint in order. Unstick cluster dot R S so merge trains land on their own, sweep thirty gigabytes of leaked daemons and Motel noise off Studio and Pro, and gate Seer and Rune churn so finished work clears in a single pass.",
        )],
    )?;

    media.finish()?;

    let reel = ReelPlan::new(
        "kaizen-rsi-v2",
        vec![
            ReelSegmentPlan::cut(part1::inversion(&s1)?),
            ReelSegmentPlan::wiped(
                part1::livelock_rig(&s2)?,
                millis(800),
                ReelWipePlan::new(WipeDirection::Left).labeled("63% WAIT", "FIX 1 · LIVELOCK"),
            ),
            ReelSegmentPlan::dipped(part2::livelock_diff(&s3)?, millis(550)),
            ReelSegmentPlan::wiped(
                part2::reclaim_30gb(&s4)?,
                millis(800),
                ReelWipePlan::new(WipeDirection::Left).labeled("FIX 1 · QUEUE", "FIX 2 · 30 GB"),
            ),
            ReelSegmentPlan::dipped(part3::seer_rune(&s5)?, millis(550)),
            ReelSegmentPlan::wiped(
                part3::close_plan(&s6)?,
                millis(800),
                ReelWipePlan::new(WipeDirection::Left).labeled("FIX 3 · GATES", "SHIP ORDER"),
            ),
        ],
    )?;

    reel.validate().context("validate kaizen-rsi-v2 reel")?;
    let output = root.join("kaizen-rsi-v2.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s across {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}
