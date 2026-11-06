//! Connections catalog pitch for Adam (Console integrations owner).
//!
//! Seven beats that change room as they go: broadsheet colour fields for
//! claims and numbers, the IDE room for GitHub's entry and the PR map, and
//! paper with day-theme code for Console's own lines.
//!
//! Sources: `anomalyco/connections` main (`4cb35bd`, `docs/console-adoption.md`,
//! `vendors/github/index.ts`, `dist/catalog.v2.json`) and
//! `anomalyco/opencode-console` `origin/dev` (`6b526254a`).
//!
//! 1. `01-today`    four Console places one vendor lives in
//! 2. `02-rule`     the catalog/Console seam
//! 3. `03-entry`    GitHub's entry
//! 4. `04-branches` the issuer branches PR 4 deletes
//! 5. `05-gets`     deletions, data-only vendors, tool marks
//! 6. `06-status`   done vs. PRs 3-6 vs. gaps
//! 7. `07-ask`      the ask

mod beats_a;
mod beats_b;
mod look;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::plan::{ReelPlan, ReelSegmentPlan};
use psychopomp_media::{ELEVEN_KIT, Media, Voice};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let media = Media::open(&root)?;
    let kit = Voice::eleven(ELEVEN_KIT)
        .v4()
        .stability(0.48)
        .similarity(0.75);

    let s1 = media.dialogue(
        "s1_today",
        [(
            &kit,
            "[calm, conversational] Adam, this is a short pitch for the connections catalog. Today, adding one vendor to Console means editing several places. For GitHub, that's a row in a SQL seed migration, OAuth special cases in integration server oauth dot T S, and a logo in the web assets. Slack and Salesforce also need their own setup guide in own app dot T S X. None of that changes from one customer to the next.",
        )],
    )?;
    let s2 = media.dialogue(
        "s2_rule",
        [(
            &kit,
            "[clear, direct] The catalog is a private repo with one entry per vendor, and it follows one rule. A fact that's true of a vendor for every OpenCode deployment goes in the catalog. Secrets, workspaces and policy stay in Console.",
        )],
    )?;
    let s3 = media.dialogue(
        "s3_entry",
        [(
            &kit,
            "[conversational] This is GitHub's entry. It's plain data in one TypeScript file, so the editor checks it as you type. The surface is where GitHub's MCP server lives, and auth lists the ways to sign in. The dialect holds GitHub's OAuth quirks, like answering a bad code with status two hundred. Each quirk cites GitHub's docs and the Console line it came from.",
        )],
    )?;
    let s4 = media.dialogue(
        "s4_branches",
        [(
            &kit,
            "[measured] In Console, P R four deletes branches like these, which pick out Google, Entra or GitHub by issuer. The adoption doc maps twelve of these spots to catalog fields.",
        )],
    )?;
    let s5 = media.dialogue(
        "s5_gets",
        [(
            &kit,
            "[conversational] P R three deletes a hundred and thirty-seven bundled logos, and the test that scrapes migration SQL to check them. After that, a new vendor is a data-only P R in the catalog and a pin bump in Console. The schema also has room to mark each tool as read, write, sends outside, or destructive, for agent policy. Nobody has filled those in yet.",
        )],
    )?;
    let s6 = media.dialogue(
        "s6_status",
        [(
            &kit,
            "[calm, direct] The catalog side is done, and a test checks it against all one hundred and thirty-two Console presets. Your side is four P Rs, in order. P R three reads presets from the catalog, and it's medium risk. P R four moves the OAuth quirks, and it's high risk, because it's the sign-in path. Five and six need catalog work first. There are gaps. Most read-only facts are still unknown, the package is a private git dependency, and releases aren't signed.",
        )],
    )?;
    let s7 = media.dialogue(
        "s7_ask",
        [(
            &kit,
            "[warm, low-key] When your agents work leaves you room, I'd like you to pick up P R three. It's your code, so if anything in the plan looks wrong, tell me.",
        )],
    )?;

    media.finish()?;

    let reel = ReelPlan::new(
        "connections-pitch",
        vec![
            ReelSegmentPlan::cut(beats_a::today(&s1)?),
            ReelSegmentPlan::cut(beats_a::rule(&s2)?),
            ReelSegmentPlan::cut(beats_a::entry(&s3)?),
            ReelSegmentPlan::cut(beats_b::branches(&s4)?),
            ReelSegmentPlan::cut(beats_b::gets(&s5)?),
            ReelSegmentPlan::cut(beats_b::status(&s6)?),
            ReelSegmentPlan::cut(beats_b::ask(&s7)?),
        ],
    )?;

    reel.validate().context("validate connections-pitch reel")?;
    let output = root.join("connections-pitch.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s across {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}
