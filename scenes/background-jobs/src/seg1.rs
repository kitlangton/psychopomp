use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis},
    caption::{CaptionActor, CaptionPlan, CaptionSpanPlan},
    checklist::{ChecklistActor, ChecklistItemPlan, ChecklistPlan, Outcome},
    chrome::{chip, footer, header},
    effects::spinner::Mark,
    plan::ScenePlan,
    rolling::{RollPlan, RollingNumberActor, RollingNumberPlan},
    sfx,
    terminal::{TerminalActor, TerminalLinePlan, TerminalPlan},
    tone::Tone,
};
use psychopomp_media::Audio;

fn sp(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

/// Segment 1: Live `TerminalActor` + `RollingNumberActor` + `ChecklistActor` rail
/// demonstrating the 2-minute foreground SIGTERM, web-only steer, and reload drop on `dev`.
pub fn problem(audio: &Audio) -> Result<ScenePlan> {
    let lead = millis(450);
    let tail = millis(1400);
    let duration = lead + audio.duration() + tail;
    let mut scene = PlanBuilder::new("01-problem", duration);
    let spoken = audio.place(&mut scene, lead);

    header(
        &mut scene,
        "today on dev",
        "2m foreground kills & web-only steer",
    )?;
    chip(&mut scene, "chip", Tone::Error, "origin/dev")?;

    let kill_at = spoken.at_any(&["SIGTERM", "killed", "two minutes"]);
    let steer_at = spoken.at_any(&["follow-up", "web client", "TUI"]);
    let reload_at = spoken.at_any(&["plugin scope", "reloading", "drops"]);

    let mut term_plan = TerminalPlan::new([120.0, 200.0], 935.0, 13)
        .size(19.0)
        .titled("tui · ~/code/opencode")
        .prompt(vec![sp("opencode ", Tone::Accent), sp("❯ ", Tone::Muted)]);
    term_plan.lines.push(TerminalLinePlan::Output {
        id: "boot".into(),
        spans: vec![
            sp("[ses_9c2e] ", Tone::Accent),
            sp("turn active · shell blocking foreground step", Tone::Muted),
        ],
    });

    let mut term = TerminalActor::declare(&mut scene, "term", term_plan)?;

    let cmd1 = term.type_at(
        &mut scene,
        lead + millis(200),
        "cargo test --workspace",
        28.0,
    )?;
    let spin_start = cmd1 + millis(150);
    let spin_id = term.spin(
        &mut scene,
        spin_start,
        vec![sp(
            "running shell sh_01 (timeout 120,000 ms)...",
            Tone::Request,
        )],
    )?;
    let kill_resolve = kill_at.max(spin_start + millis(500));
    term.resolve(
        &mut scene,
        &spin_id,
        kill_resolve,
        Mark::Cross,
        vec![
            sp("SIGTERM at 120,000 ms ", Tone::Error),
            sp("· slow build killed mid-suite", Tone::Plain),
        ],
    )?;

    let steer_cmd = term.type_at(
        &mut scene,
        steer_at
            .saturating_sub(millis(350))
            .max(kill_resolve + millis(400)),
        "steer \"check job.ts while tests finish\"",
        28.0,
    )?;
    let steer_out = steer_cmd.max(steer_at + millis(250));
    term.print(
        &mut scene,
        steer_out,
        vec![
            vec![
                sp("tui / ios: ", Tone::Warning),
                sp("message waits behind blocked foreground tool", Tone::Plain),
            ],
            vec![sp(
                "  app/steer-background.ts only runs in web composer",
                Tone::Muted,
            )],
        ],
    )?;

    let reload_out = reload_at.max(steer_out + millis(600));
    term.print(
        &mut scene,
        reload_out,
        vec![
            vec![
                sp("LocationServiceMap.reload: ", Tone::Error),
                sp("notifyWhenDone fiber interrupted (#56729)", Tone::Plain),
            ],
            vec![sp(
                "  sh_02 finished in global Job, notice never delivered",
                Tone::Muted,
            )],
        ],
    )?;

    CaptionActor::declare(
        &mut scene,
        "kicker",
        &CaptionPlan::line(
            [1115.0, 224.0],
            19.0,
            vec![sp("DEFAULT FOREGROUND LIMIT", Tone::Muted)],
        ),
    )?;

    let rolling_plan = RollingNumberPlan {
        origin: [1115.0, 304.0],
        align: Default::default(),
        size: 62.0,
        bold: true,
        tone: Tone::Error,
        prefix: Vec::new(),
        suffix: vec![sp(" @ 2m", Tone::Muted)],
        value: "120,000 ms".into(),
        rolls: vec![RollPlan {
            at_nanos: kill_resolve,
            value: "SIGTERM".into(),
        }],
        duration_nanos: 580_000_000,
        stagger: Default::default(),
        direction: Default::default(),
        blur: 1.0,
        chip: false,
        anchors: Vec::new(),
    };
    RollingNumberActor::declare(&mut scene, "limit-roll", rolling_plan)?;

    let check_plan = ChecklistPlan::new([1115.0, 415.0], 680.0)
        .size(23.0)
        .title("three gaps on dev")
        .rail()
        .panel()
        .item(
            ChecklistItemPlan::new("kill", "2m shell timeout")
                .failure("SIGTERM")
                .result("SIGTERM"),
        )
        .item(
            ChecklistItemPlan::new("steer", "steer-background.ts")
                .failure("web only")
                .result("web only"),
        )
        .item(
            ChecklistItemPlan::new("reload", "notifyWhenDone scope")
                .failure("dropped")
                .result("dropped"),
        );

    let mut checks = ChecklistActor::declare(&mut scene, "checks", &check_plan)?;
    checks.show(&mut scene, lead + millis(120));
    checks.reveal(&mut scene, lead + millis(300))?;

    checks.start(&mut scene, "kill", spin_start)?;
    checks.resolve(
        &mut scene,
        "kill",
        kill_resolve + millis(180),
        Outcome::Failed,
    )?;

    checks.start(&mut scene, "steer", steer_cmd)?;
    checks.resolve(
        &mut scene,
        "steer",
        steer_out + millis(220),
        Outcome::Failed,
    )?;

    checks.start(&mut scene, "reload", reload_out.saturating_sub(millis(450)))?;
    checks.resolve(
        &mut scene,
        "reload",
        reload_out + millis(250),
        Outcome::Failed,
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            sp("origin/dev · ", Tone::Muted),
            sp("shell.ts:182", Tone::Accent),
            sp(
                " · app/src/session/requests/steer-background.ts · #56729",
                Tone::Muted,
            ),
        ],
    )?;
    foot.show(&mut scene, steer_out);

    sfx::FAILURE.play(&mut scene, "kill-sfx", kill_resolve, -16.0);
    sfx::TICK.play(&mut scene, "steer-sfx", steer_out, -16.5);
    sfx::GLITCH.play(&mut scene, "reload-sfx", reload_out, -16.0);

    scene.cue("problem", 0, duration);
    Ok(scene.finish()?)
}
