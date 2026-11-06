//! Composable cloud agents: a narrated walkthrough for Adam (agents owner).
//!
//! Eleven beats that change room as they go: broadsheet colour fields for
//! claims (`beats` on paper, cobalt, and vermilion) and the IDE room for code
//! and diagrams, each keyed to the narration's word timings.
//!
//! Sources: `~/code/home/artifacts/2026-11-05-general-agents/lessons/lessons.md`,
//! `.../proposal/proposal.md`, opencode-console `origin/dev` (PR 4, #5683).

mod beats_a;
mod beats_b;
mod look;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::plan::{ReelPlan, ReelSegmentPlan, ScenePlan};
use psychopomp_media::{Audio, ELEVEN_KIT, Media, Voice};

const SEGMENTS: &[(&str, &str)] = &[
    (
        "s01_today",
        "[calm, conversational] Adam, here's how we think the Slack bot could become a few composable pieces, built on your P R four tables. Today it's one program shaped around Slack. So Discord has to copy it, an agent just for the admin channel has nowhere to live, and schedules pretend to be Slack.",
    ),
    (
        "s02_idea",
        "[clear, direct] The idea is to keep platform code at the edges. A connector turns a Slack message into an event. A route picks one agent, which runs in that thread's conversation, and the reply goes back out through the same connector. Nothing in the middle imports Slack.",
    ),
    (
        "s03_event",
        "[conversational] Every connector produces the same event. Someone said something, someone pressed a button, or a schedule fired. Its place is an address, like team, channel and thread, and that's also where the reply goes.",
    ),
    (
        "s04_connection",
        "[conversational] A connector is code we ship once per platform. A connection is a workspace's link, like Anomaly's Slack. Agents get grants, which are slices of a connection, and Console mints each git token for one repo and one permission, for at most an hour.",
    ),
    (
        "s05_route",
        "[conversational] A route picks exactly one agent, and the most specific place wins: the thread, then the channel, then the install-wide default. A paused agent refuses instead of falling through. The agent is pinned when a thread starts, so route edits only affect new threads.",
    ),
    (
        "s05b_watch",
        "[conversational] A watch only listens. My laptop can hear a thread's replies, but it never answers. One catch: direct messages always get the default agent.",
    ),
    (
        "s06_policy",
        "[conversational] An agent is a job: a service account with instructions, a model, tools and permissions. Those permissions combine with the route, who's in the room, and the workspace's controls, and each one can only take permissions away. So if a guest joins the admin channel, internal tools turn off.",
    ),
    (
        "s07_conversation",
        "[conversational] A conversation is one per thread. It keeps one agent, runs one turn at a time, and drops duplicate events. Once it has seen internal data, it won't answer in a shared channel.",
    ),
    (
        "s08_cases",
        "[clear, direct] Those pieces cover every case we looked at. Discord and Teams are connectors, the admin agent is a route, a schedule is a clock, the public A P I is a caller bound to an agent, and my laptop is a watch.",
    ),
    (
        "s09_prs",
        "[calm, direct] The P R four tables stay. A channel route is an agent trigger row whose key adds the channel, under the same unique index. We'd add a connection I D, plus tables for GitHub installs, grants, watches and agent sessions. And we'd rename agent audience to serves, since audience will mean who's in the room, and drop git server slug. All of that is proposed. Only your tables and today's bot are built.",
    ),
    (
        "s10_ask",
        "[warm, low-key] Before you start P R five, could we compare this with your plan, and settle the twelve open decisions together?",
    ),
];

type Beat = fn(&Audio) -> Result<ScenePlan>;

const BEATS: [Beat; 11] = [
    beats_a::today,
    beats_a::idea,
    beats_a::event,
    beats_a::connection,
    beats_a::route,
    beats_b::watch,
    beats_b::policy,
    beats_b::conversation,
    beats_b::cases,
    beats_b::prs,
    beats_b::ask,
];

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let media = Media::open(&root)?;
    let kit = Voice::eleven(ELEVEN_KIT)
        .v4()
        .stability(0.48)
        .similarity(0.75);
    let mut segments = Vec::new();
    for ((name, line), beat) in SEGMENTS.iter().zip(BEATS) {
        let audio = media.dialogue(name, [(&kit, *line)])?;
        let plan = beat(&audio).with_context(|| format!("beat {name}"))?;
        // Every beat changes room on a hard cut, on its first move.
        segments.push(ReelSegmentPlan::cut(plan));
    }
    media.finish()?;

    let reel = ReelPlan::new("agents-pitch", segments)?;
    reel.validate().context("validate agents-pitch reel")?;
    let output = root.join("agents-pitch.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s across {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}
