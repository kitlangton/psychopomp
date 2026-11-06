//! Kaizen × OpenCode (`t919`): an 8-segment explainer film on the Toyota
//! Production System, how Kit's agent fleet already runs Mecha-Kaizen with
//! measured receipts, and four product primitives OpenCode should ship next.
//!
//! Every segment uses a distinct visual medium in Psychopomp:
//! 1. `01-surge`       — `PlotActor` (42-week Anomaly commits) + `RollingNumberActor`
//! 2. `02-nummi`       — Geometric Takaoka Andon Lightboard & Track + `BarsActor`
//! 3. `03-techniques`  — Procedural `Grid` Banded Table pivoting into 3D `Layers`
//! 4. `04-andon-poka`  — Live `TerminalActor` + `ChecklistActor` rail
//! 5. `05-five-whys`   — Stepped `Diff` + `DiagnosticActor` + `HoverActor` + `MeterActor`
//! 6. `06-muda-kanban` — `RuneSurfaceActor` (`RuneKind::Note` Seer card on `wgpu`)
//! 7. `07-proposals`   — 2.5D `StageElement::RuneCard` (`RuneKind::Kit`) + `LensActor` loupe
//! 8. `08-close`       — 3D `Form` particle convergence + `Face::Shout` typography

mod act1;
mod act2;
mod act3;
mod act4;

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
        "seg1_surge",
        [(
            &kit,
            "[calm, conversational] In January, the Anomaly team merged two hundred and fifteen commits a week across our repos. Last week we merged five thousand four hundred and thirty-six. Agents multiplied our output twenty-five times, and they multiplied every hidden queue, flaky check, and wasted turn right along with it. [measured, clear] Sixty years ago, Taiichi Ohno and Toyota hit the same wall on physical assembly lines, and their answer was Kaizen.",
        )],
    )?;

    let s2 = media.dialogue(
        "seg2_nummi",
        [(
            &kit,
            "[measured, conversational] In nineteen eighty-two, General Motors closed its Fremont plant with twenty percent daily absenteeism and the worst quality in the company. Two years later, Toyota reopened that building as NUMMI with eighty-five percent of the same union workers, and absenteeism fell to two percent while quality matched Japan. Toyota gave every worker an overhead andon cord to stop the line on any defect. Workers in Georgetown pull that cord two thousand times a week, compared to twice a week at a rival Detroit plant, because Spear and Bowen showed every Toyota task is a falsifiable experiment.",
        )],
    )?;

    let s3 = media.dialogue(
        "seg3_techniques",
        [(
            &kit,
            "[clear, direct] Five mechanics run that factory. Liker pairs muda, or pure waste, with mura, unevenness, and muri, overburden, because past eighty-five percent utilization queues explode. Kanban caps work in progress while heijunka levels load across machines. When a defect still slips through, genchi genbutsu means reading raw traces yourself and asking the Five Whys until you can install a poka-yoke, a physical guard that prevents the mistake instead of scolding people. Standard work sets the baseline, and Rother's Improvement Kata tests one Deming prediction at a time.",
        )],
    )?;

    let s4 = media.dialogue(
        "seg4_andon_poka",
        [(
            &kit,
            "[conversational, grounded] Over the past two weeks, we wired these exact loops into my local agent fleet. After inherited environment variables booted a second OpenCode server four times in one night and corrupted live turns, a shell guard started refusing standalone servers at the door. When thirty Seer builders piled a hundred and fifty commits onto a broken main branch, we added seer andon, which blocks agent pushes after fifteen minutes of red main unless the push fixes a blamed test. Inherited test failures dropped from sixty-one percent to eleven, and a rustc token gate cut unbudgeted compiles by ninety-nine point six percent.",
        )],
    )?;

    let s5 = media.dialogue(
        "seg5_five_whys",
        [(
            &kit,
            "[measured, investigative] Yesterday our recursive self-improvement skill read twelve thousand HiveMind job traces and found seventy percent of fleet wall time was pure queue wait. Walking the Five Whys exposed a five gigabyte deadband on the Mac Studio. Admission blocked Cargo below thirty gigabytes free while the cleaner stopped evicting at twenty-five, so fifty subagents spilled onto a laptop throttled at load one sixty-three. Raising the cleaner floor to thirty-five gigabytes and adding core budgets dropped laptop load to thirteen point six.",
        )],
    )?;

    let s6 = media.dialogue(
        "seg6_muda_kanban",
        [(
            &kit,
            "[warm, direct] Cutting muda and capping work in progress paid off just as fast. Compacting Opus context around two hundred thousand tokens cut cost per turn by thirty percent, while deduplicating images into a sequel lite blob table shrank large message rows from nine hundred and fourteen a day to one. A blind replay promoted Sonnet for mechanical tasks at thirty-nine percent of Opus cost, and Seer now caps active builders at six so Cargo waits finish inside Anthropic's five-minute prompt cache window.",
        )],
    )?;

    let s7 = media.dialogue(
        "seg7_proposals",
        [(
            &kit,
            "[excited, clear] None of those guards should require a folder of personal shell scripts, so OpenCode should ship four primitives directly in the product. First, add a built-in andon breaker that halts a turn on repeated tool errors, broken baseline checks, or malformed context loops and posts a one-tap decision card. Second, enforce an exclusive database writer lease and native worktree pre-push hooks so dual servers and broken binaries cannot happen. Third, record zero-overhead wide events and ship a slash kaizen command that ranks any project's top P ninety-five queue from raw traces. Fourth, schedule subagents through a pull queue that wakes on events without polling and proposes benchmark-tested deltas to skill files.",
        )],
    )?;

    let s8 = media.dialogue(
        "seg8_close",
        [(
            &kit,
            "[calm, deliberate] Agents already gave our team twenty-five times the commits. Building jidoka, poka-yoke, leveled pull, and daily kata into OpenCode turns that raw speed into finished software, because you fix quality by changing the line the work runs on.",
        )],
    )?;

    media.finish()?;

    let reel = ReelPlan::new(
        "kaizen-opencode",
        vec![
            ReelSegmentPlan::cut(act1::surge(&s1)?),
            ReelSegmentPlan::wiped(
                act1::nummi(&s2)?,
                millis(800),
                ReelWipePlan::new(WipeDirection::Left).labeled("2026 SURGE", "NUMMI 1984"),
            ),
            ReelSegmentPlan::dipped(act2::techniques(&s3)?, millis(600)),
            ReelSegmentPlan::dipped(act2::andon_poka(&s4)?, millis(600)),
            ReelSegmentPlan::dipped(act3::five_whys(&s5)?, millis(600)),
            ReelSegmentPlan::dipped(act3::muda_kanban(&s6)?, millis(600)),
            ReelSegmentPlan::wiped(
                act4::proposals(&s7)?,
                millis(800),
                ReelWipePlan::new(WipeDirection::Left).labeled("LOCAL SCRIPTS", "OPENCODE PRODUCT"),
            ),
            ReelSegmentPlan::dipped(act4::close(&s8)?, millis(650)),
        ],
    )?;

    reel.validate().context("validate kaizen-opencode reel")?;
    let output = root.join("kaizen-opencode.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s across {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}
