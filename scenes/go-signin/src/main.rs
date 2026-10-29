//! Why OpenCode Go should not be its own row in `/connect`, for the team.
//!
//! Left: the 2.5D GPU Stage `console` card, wire beam, and flying packets.
//! Right: the real `@opencode/ui` `/connect` dialog mounted via `RuneSurface::kit`,
//! followed by a `RuneSurface::note` summary with Seer `size` and `steps` blocks.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    narration::Narration,
    plan::{ReelPlan, ReelSegmentPlan, ScenePlan},
    rune_surface::{KitNode, KitTone, RuneAction, RuneSurfaceActor, RuneSurfacePlan, kit},
    sfx,
    stage::{StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

const CONSOLE: [f32; 3] = [330.0, 520.0, 0.0];
const CONSOLE_SIZE: [f32; 2] = [340.0, 140.0];
const DIALOG_POS: [f32; 2] = [1240.0, 520.0];
const DIALOG_ANCHOR: [f32; 3] = [1240.0, 520.0, 0.0];
const DIALOG_ANCHOR_SIZE: [f32; 2] = [840.0, 440.0];

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration = Narration::load(&root.join("narration"))?;
    let reel = ReelPlan::new(
        "go-signin",
        vec![
            ReelSegmentPlan::cut(film(&narration)?),
            ReelSegmentPlan::dipped(size(&narration)?, millis(700)),
        ],
    )?;
    let output = root.join("go-signin.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s, {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}

fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn connect_dialog() -> KitNode {
    kit::dialog(
        "dialog",
        420.0,
        vec![
            kit::row(
                "hdr",
                8.0,
                vec![
                    kit::keyed_text(
                        "dlg-title",
                        "Connect an integration",
                        15.0,
                        600.0,
                        KitTone::Base,
                    ),
                    kit::spacer(),
                    kit::badge("esc", KitTone::Muted),
                ],
            ),
            kit::text_input("search", "", "Search 230 providers…"),
            kit::col(
                "list-view",
                6.0,
                vec![
                    kit::text("Popular", 11.5, 560.0, KitTone::Muted),
                    kit::col(
                        "rows",
                        4.0,
                        vec![
                            kit::list_row(
                                "go",
                                "OpenCode Go",
                                "API key only",
                                KitTone::Base,
                                false,
                            ),
                            kit::list_row(
                                "console",
                                "OpenCode Console",
                                "sign in or API key",
                                KitTone::Base,
                                false,
                            ),
                            kit::list_row(
                                "anthropic",
                                "Anthropic",
                                "Claude Max or API key",
                                KitTone::Base,
                                false,
                            ),
                            kit::list_row(
                                "openai",
                                "OpenAI",
                                "ChatGPT Plus/Pro or API key",
                                KitTone::Base,
                                false,
                            ),
                        ],
                    ),
                ],
            ),
            kit::col(
                "api-prompt",
                10.0,
                vec![
                    kit::text(
                        "Enter your OpenCode Go API key",
                        13.0,
                        520.0,
                        KitTone::Muted,
                    ),
                    kit::text_input("api-key", "", "sk-opencode-…"),
                    kit::row(
                        "prompt-actions",
                        8.0,
                        vec![
                            kit::keyed_text(
                                "stuck-hint",
                                "No browser sign-in on this row",
                                12.0,
                                500.0,
                                KitTone::Danger,
                            )
                            .hidden(),
                            kit::spacer(),
                            kit::button("submit", "Submit", true),
                        ],
                    ),
                ],
            )
            .hidden(),
            kit::keyed_text("status-note", "", 12.0, 500.0, KitTone::Muted).hidden(),
        ],
    )
}

fn stage_plan() -> StagePlan {
    let elements = vec![
        StageElement::card("console", CONSOLE, CONSOLE_SIZE, "opencode console").statuses(&[
            ("opencode.ai/go", Tone::Muted),
            ("Go subscribed", Tone::Success),
            ("“sign in to OpenCode”", Tone::Request),
            ("Go models included", Tone::Success),
        ]),
        // Invisible Stage docking bounds so Stage beams and packets land cleanly on the Rune dialog.
        StageElement::rect("dialog", DIALOG_ANCHOR, DIALOG_ANCHOR_SIZE),
        StageElement::beam("link", "dialog", "console").tone(Tone::Request),
        StageElement::packet("signin", "link")
            .labeled("sign in")
            .tone(Tone::Request),
        StageElement::packet("models", "link")
            .reversed()
            .labeled("Go models")
            .tone(Tone::Success),
        StageElement::packet("signin-2", "link")
            .labeled("sign in")
            .tone(Tone::Request),
        StageElement::packet("models-2", "link")
            .reversed()
            .labeled("Go models")
            .tone(Tone::Success),
    ];
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

fn film(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(1.2),
        [
            ("problem", seconds(1.0)),
            ("console", seconds(1.2)),
            ("fix", seconds(1.6)),
        ],
    )?;
    let mut scene = PlanBuilder::new("film", reading.duration());
    let [problem, console, fix] = reading.place(&mut scene);
    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let mut connect = RuneSurfaceActor::declare(
        &mut scene,
        "connect",
        RuneSurfacePlan::kit(connect_dialog()).at(DIALOG_POS),
    )?;
    // Initially hidden until "open connect".
    connect.channel(&mut scene, "opacity", 0.0);
    stage.channel(&mut scene, "dialog.opacity", 0.0);
    stage.channel(&mut scene, "link.draw", 0.0);

    stage.channel(&mut scene, "camera.z", -120.0);
    stage.to(&mut scene, "camera.z", 0, 0.0, 2.2);
    let mut title = header(&mut scene, "opencode", "connect and OpenCode Go")?;
    title.type_in(&mut scene, seconds(0.3), 55.0, 0.6);
    let mut chip_problem = chip(&mut scene, "chip-problem", Tone::Error, "the problem")?;
    chip_problem.show(&mut scene, seconds(0.7));

    // 1. Buy Go in the Console.
    let buy = problem.at("buy");
    stage.settle_in(&mut scene, "console", buy);
    sfx::BLOOM.play(&mut scene, "console-in", buy, -24.0);
    let bought = problem.at("in the console");
    stage.swap_status(&mut scene, "console", bought, [0, 1], 0.35);
    stage.hit(&mut scene, "console.flash", bought + millis(200), 0.5, 0.0);
    sfx::MARK.play(&mut scene, "bought", bought + millis(200), -20.0);

    // 2. Open `/connect`: real @opencode/ui dialog settles in; Go row selects -> bare API key prompt.
    let open = problem.at("open connect");
    connect.settle_in(&mut scene, open);
    connect.commit(
        &mut scene,
        problem.at("first row"),
        "highlight go row",
        RuneAction::SetSelected {
            key: "go".into(),
            selected: true,
        },
    )?;
    sfx::TICK.play(
        &mut scene,
        "pick-go",
        problem.at("pick it") + millis(150),
        -18.0,
    );
    connect.commit(
        &mut scene,
        problem.at("asks for"),
        "show bare api key prompt",
        RuneAction::Batch {
            actions: vec![
                RuneAction::SetVisible {
                    key: "list-view".into(),
                    visible: false,
                },
                RuneAction::SetVisible {
                    key: "api-prompt".into(),
                    visible: true,
                },
            ],
        },
    )?;
    let no_way = problem.at("no way");
    connect.commit(
        &mut scene,
        no_way,
        "flag missing oauth on go row",
        RuneAction::Batch {
            actions: vec![
                RuneAction::SetBorderTone {
                    key: "dialog".into(),
                    tone: Some(KitTone::Danger),
                },
                RuneAction::SetVisible {
                    key: "stuck-hint".into(),
                    visible: true,
                },
            ],
        },
    )?;
    sfx::FAILURE.play(&mut scene, "no-way", no_way, -24.0);
    let mut footer_problem = footer(
        &mut scene,
        "footer-problem",
        vec![
            span("a Go subscriber gets ", Tone::Muted),
            span("a bare API key prompt", Tone::Error),
        ],
    )?;
    footer_problem.type_in(&mut scene, problem.at("buying Go"), 46.0, 0.8);

    // 3. What Console already does: Escape back to rows, select Console, OAuth brings Go models.
    let console_start = console.start();
    footer_problem.hide(&mut scene, console_start);
    chip_problem.hide(&mut scene, console_start);
    let mut chip_console = chip(
        &mut scene,
        "chip-console",
        Tone::Request,
        "what the Console does",
    )?;
    chip_console.show(&mut scene, console_start + millis(250));

    connect.commit(
        &mut scene,
        console.at("already treats"),
        "escape back to provider list",
        RuneAction::Batch {
            actions: vec![
                RuneAction::SetBorderTone {
                    key: "dialog".into(),
                    tone: None,
                },
                RuneAction::SetVisible {
                    key: "api-prompt".into(),
                    visible: false,
                },
                RuneAction::SetVisible {
                    key: "list-view".into(),
                    visible: true,
                },
                RuneAction::SetSelected {
                    key: "go".into(),
                    selected: false,
                },
            ],
        },
    )?;
    let account = console.at("part of your account");
    stage.to(&mut scene, "console.glow", account, 0.35, 0.5);
    stage.hit(&mut scene, "console.flash", account, 0.5, 0.0);
    let page = console.at("Go page");
    stage.swap_status(&mut scene, "console", page, [1, 2], 0.35);
    stage.to(&mut scene, "console.glow", page + seconds(1.5), 0.0, 0.6);

    connect.commit(
        &mut scene,
        console.at("second row"),
        "select console row",
        RuneAction::SetSelected {
            key: "console".into(),
            selected: true,
        },
    )?;
    let signs_in = console.at("does that");
    let contact = stage.connect(&mut scene, "link", signs_in + millis(100), 0.5);
    sfx::TICK.play(&mut scene, "link", contact, -20.0);
    stage.twang(&mut scene, "link", contact);
    let arrived = stage.send(&mut scene, "signin", contact + millis(350), 0.7);
    sfx::SEND.play(&mut scene, "signin", contact + millis(350), -14.0);
    stage.land(&mut scene, "console", arrived);

    let models_at = console.at("adds the Go");
    stage.swap_status(&mut scene, "console", console.at("once"), [2, 3], 0.35);
    let landed = stage.send(
        &mut scene,
        "models",
        models_at.saturating_sub(millis(300)),
        0.75,
    );
    sfx::SEND.play(
        &mut scene,
        "models",
        models_at.saturating_sub(millis(300)),
        -14.0,
    );
    connect.commit(
        &mut scene,
        landed,
        "show go models unlocked",
        RuneAction::Batch {
            actions: vec![
                RuneAction::SetText {
                    key: "status-note".into(),
                    text: "+ Go models unlocked from Console workspace".into(),
                    tone: Some(KitTone::Success),
                },
                RuneAction::SetVisible {
                    key: "status-note".into(),
                    visible: true,
                },
            ],
        },
    )?;
    sfx::CONFIRM.play(&mut scene, "models-land", landed, -22.0);
    connect.commit(
        &mut scene,
        console.at("skip the"),
        "warn on duplicate go row",
        RuneAction::SetRow {
            key: "go".into(),
            title: None,
            hint: None,
            tone: Some(KitTone::Danger),
            hint_tone: None,
            selected: None,
        },
    )?;

    let mut footer_console = footer(
        &mut scene,
        "footer-console",
        vec![
            span("signing in to OpenCode ", Tone::Muted),
            span("already unlocks Go", Tone::Request),
        ],
    )?;
    footer_console.type_in(&mut scene, console.at("as long as"), 46.0, 0.8);

    // 4. The fix: drop the Go row (remaining rows spring up smoothly) and rename Console -> OpenCode.
    let fix_start = fix.start();
    footer_console.hide(&mut scene, fix_start);
    chip_console.hide(&mut scene, fix_start);
    connect.commit(
        &mut scene,
        fix_start,
        "clear console note",
        RuneAction::SetVisible {
            key: "status-note".into(),
            visible: false,
        },
    )?;
    let mut chip_fix = chip(&mut scene, "chip-fix", Tone::Success, "the fix")?;
    chip_fix.show(&mut scene, fix_start + millis(250));

    let drop_at = fix.at("drop the");
    sfx::WHOOSH.play(&mut scene, "drop", drop_at + millis(180), -24.0);
    connect.commit(
        &mut scene,
        drop_at,
        "drop go row",
        RuneAction::SetVisible {
            key: "go".into(),
            visible: false,
        },
    )?;
    let rename = fix.at("rename");
    connect.commit(
        &mut scene,
        rename,
        "rename console to opencode",
        RuneAction::SetRow {
            key: "console".into(),
            title: Some("OpenCode".into()),
            hint: Some("sign in or API key · Go included".into()),
            tone: Some(KitTone::Base),
            hint_tone: Some(KitTone::Success),
            selected: Some(true),
        },
    )?;
    sfx::MARK.play(&mut scene, "renamed", fix.at("just OpenCode"), -20.0);

    let once = fix.at("sign in once");
    let arrived2 = stage.send(&mut scene, "signin-2", once + millis(200), 0.6);
    sfx::SEND.play(&mut scene, "signin-2", once + millis(200), -14.0);
    stage.land(&mut scene, "console", arrived2);
    let _landed2 = stage.send(&mut scene, "models-2", fix.at("comes with"), 0.65);
    sfx::SEND.play(&mut scene, "models-2", fix.at("comes with"), -14.0);

    let keys = fix.at("API keys still work");
    connect.commit(
        &mut scene,
        keys,
        "show api key support note",
        RuneAction::Batch {
            actions: vec![
                RuneAction::SetText {
                    key: "status-note".into(),
                    text: "Paste an API key or set OPENCODE_API_KEY in the same row".into(),
                    tone: Some(KitTone::Muted),
                },
                RuneAction::SetVisible {
                    key: "status-note".into(),
                    visible: true,
                },
            ],
        },
    )?;

    let mut footer_fix = footer(
        &mut scene,
        "footer-fix",
        vec![
            span("one row", Tone::Success),
            span(
                " · sign in or paste a key · Go comes with the workspace",
                Tone::Muted,
            ),
        ],
    )?;
    footer_fix.type_in(&mut scene, keys, 46.0, 0.8);

    scene.finish().context("film")
}

fn size(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.8), [("outro", seconds(2.2))])?;
    let mut scene = PlanBuilder::new("size", reading.duration());
    let [spoken] = reading.place(&mut scene);

    let mut title = header(&mut scene, "opencode", "connect and OpenCode Go")?;
    title.show(&mut scene, 0);
    let mut chip_plan = chip(&mut scene, "chip-plan", Tone::Accent, "the plan")?;
    chip_plan.show(&mut scene, seconds(0.3));

    let base_md = "\
Keep a single **OpenCode** row in `/connect` so Go subscribers sign in or paste a key in one place.

```facts
Change: {built} ~30 lines in dialog-integration.tsx
Replaces: {risk} #57155 (+1,394 lines across two rows)
```";

    let full_md = format!(
        "{base_md}\n\n```size\napp +24 −6 · tests +18 −0 · 2 files\n```\n\n```steps\n{{built}} One OpenCode row in connect · OAuth + API key + Go models\n{{done}} Close #57155 · avoids two rows sharing one login\n```"
    );

    let mut note = RuneSurfaceActor::declare(
        &mut scene,
        "plan-note",
        RuneSurfacePlan::note(base_md)
            .titled("Recommendation")
            .width(620.0)
            .at([960.0, 520.0]),
    )?;
    note.settle_in(&mut scene, seconds(0.35));
    note.commit(
        &mut scene,
        spoken.at("replaces"),
        "reveal size and steps",
        RuneAction::SetMarkdown { markdown: full_md },
    )?;
    sfx::MARK.play(&mut scene, "row-done", spoken.at("lines"), -18.0);
    sfx::MARK.play(&mut scene, "pr-closed", spoken.at("close it"), -20.0);

    let mut closing = footer(
        &mut scene,
        "footer-plan",
        vec![
            span("ship ", Tone::Muted),
            span("one OpenCode row", Tone::Success),
            span(" · close #57155", Tone::Muted),
        ],
    )?;
    closing.type_in(&mut scene, spoken.at("mostly tests"), 40.0, 0.8);
    scene.finish().context("size")
}
