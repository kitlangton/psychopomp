//! Why OpenCode Go should not be its own row in `/connect`, for the team.
//!
//! One Stage film: the Console card on the left, the TUI's connect dialog on
//! the right. A Go subscriber picks the OpenCode Go row and gets a bare API key
//! prompt. The OpenCode Console row already signs in and brings the Go models
//! back from the Console. The fix drops the Go row, the rows below slide up,
//! and the Console row becomes "OpenCode". A checklist closes on the size of
//! the change and the pull request it replaces.
//!
//! Sources: `packages/core/src/plugin/models-dev.ts` and
//! `packages/core/src/plugin/provider/opencode.ts` in opencode (origin/dev),
//! `dialog-integration.tsx` (Go at rank 0), the opencode-console README ("Use Go
//! with the CLI"), and `gh pr view 57155` for its size.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    caption::{CaptionAlign, CaptionSpanPlan},
    checklist::{ChecklistActor, ChecklistItemPlan, ChecklistPlan, Outcome},
    chrome::{chip, footer, header},
    narration::Narration,
    plan::{ReelPlan, ReelSegmentPlan, ScenePlan},
    sfx,
    stage::{Fill, Material, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};

const CONSOLE: [f32; 3] = [330.0, 520.0, 0.0];
const CONSOLE_SIZE: [f32; 2] = [340.0, 140.0];

/// The connect dialog: a raised panel with the TUI's rows.
const DIALOG: [f32; 3] = [1230.0, 520.0, 0.0];
const DIALOG_SIZE: [f32; 2] = [820.0, 440.0];
const TEXT_LEFT: f32 = 860.0;
const TEXT_RIGHT: f32 = 1600.0;
const TITLE_Y: f32 = 350.0;
const POPULAR_Y: f32 = 400.0;
const ROW_Y: [f32; 4] = [450.0, 510.0, 570.0, 630.0];
const ROW_PITCH: f32 = 60.0;
const ROW_SIZE: [f32; 2] = [760.0, 50.0];

/// The API key prompt that replaces the rows.
const INPUT_Y: f32 = 470.0;
const SUBMIT_Y: f32 = 550.0;

/// Notes under the dialog.
const NOTE_Y: [f32; 2] = [800.0, 846.0];

/// Where the cursor rests when it points at the first row.
const CURSOR: [f32; 3] = [1180.0, 474.0, -12.0];

const ROWS: [(&str, &str, &str); 4] = [
    ("go", "OpenCode Go", "API key only"),
    ("console", "OpenCode Console", "sign in or API key"),
    ("anthropic", "Anthropic", "Claude Max or API key"),
    ("openai", "OpenAI", "ChatGPT Plus/Pro or API key"),
];

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

fn left(id: &str, at: [f32; 3], size: f32, spans: &[(&str, Tone)]) -> StageElement {
    StageElement::label(id, at, size, spans).align(CaptionAlign::Left)
}

fn right(id: &str, at: [f32; 3], size: f32, spans: &[(&str, Tone)]) -> StageElement {
    StageElement::label(id, at, size, spans).align(CaptionAlign::Right)
}

fn stage_plan() -> StagePlan {
    let label = StageElement::label;
    let mut elements = vec![
        StageElement::card("console", CONSOLE, CONSOLE_SIZE, "opencode console").statuses(&[
            ("opencode.ai/go", Tone::Muted),
            ("Go subscribed", Tone::Success),
            ("“sign in to OpenCode”", Tone::Request),
            ("Go models included", Tone::Success),
        ]),
        StageElement::rect("dialog", DIALOG, DIALOG_SIZE)
            .corner(18.0)
            .fill(Some(Fill::Material(Material::Raised)))
            .stroke(Some(Tone::Muted))
            .stroke_width(1.2),
        StageElement::rect("dialog-error", [DIALOG[0], DIALOG[1], -1.0], DIALOG_SIZE)
            .corner(18.0)
            .stroke(Some(Tone::Error))
            .stroke_width(2.0),
        left(
            "title",
            [TEXT_LEFT, TITLE_Y, -2.0],
            28.0,
            &[("Connect an integration", Tone::Plain)],
        ),
        right(
            "esc",
            [TEXT_RIGHT, TITLE_Y, -2.0],
            22.0,
            &[("esc", Tone::Muted)],
        ),
        left(
            "popular",
            [TEXT_LEFT, POPULAR_Y, -2.0],
            20.0,
            &[("Popular", Tone::Request)],
        ),
        StageElement::rect("row-hi", [DIALOG[0], ROW_Y[0], -1.0], ROW_SIZE)
            .corner(8.0)
            .fill(Some(Fill::Tone(Tone::Accent)))
            .fill_opacity(0.10)
            .stroke(Some(Tone::Accent))
            .stroke_width(1.2),
    ];
    for (index, &(id, name, hint)) in ROWS.iter().enumerate() {
        elements.push(left(
            &format!("{id}-name"),
            [TEXT_LEFT, ROW_Y[index], -2.0],
            26.0,
            &[(name, Tone::Plain)],
        ));
        elements.push(right(
            &format!("{id}-hint"),
            [TEXT_RIGHT, ROW_Y[index], -2.0],
            23.0,
            &[(hint, Tone::Muted)],
        ));
    }
    elements.extend([
        // The Go row in red, laid over it when it is the problem.
        left(
            "go-name-error",
            [TEXT_LEFT, ROW_Y[0], -3.0],
            26.0,
            &[("OpenCode Go", Tone::Error)],
        ),
        // The Console row after the rename, in the Console row's place.
        left(
            "opencode-name",
            [TEXT_LEFT, ROW_Y[1], -2.0],
            26.0,
            &[("OpenCode", Tone::Plain)],
        ),
        right(
            "opencode-hint",
            [TEXT_RIGHT, ROW_Y[1], -2.0],
            23.0,
            &[
                ("sign in or API key · ", Tone::Muted),
                ("Go included", Tone::Success),
            ],
        ),
        // The API key prompt.
        left(
            "prompt-title",
            [TEXT_LEFT, TITLE_Y, -2.0],
            28.0,
            &[("Connect OpenCode Go", Tone::Plain)],
        ),
        StageElement::rect("input", [DIALOG[0], INPUT_Y, -1.0], [ROW_SIZE[0], 64.0])
            .corner(10.0)
            .fill(Some(Fill::Material(Material::Well)))
            .stroke(Some(Tone::Muted))
            .stroke_width(1.0),
        StageElement::rect("caret", [TEXT_LEFT + 22.0, INPUT_Y, -2.0], [3.0, 32.0])
            .fill(Some(Fill::Tone(Tone::Accent)))
            .stroke(None),
        left(
            "placeholder",
            [TEXT_LEFT + 34.0, INPUT_Y, -2.0],
            26.0,
            &[("API key", Tone::Muted)],
        ),
        left(
            "submit",
            [TEXT_LEFT, SUBMIT_Y, -2.0],
            22.0,
            &[("submit", Tone::Plain), ("  enter", Tone::Muted)],
        ),
        // Notes under the dialog.
        label(
            "note-stuck",
            [DIALOG[0], NOTE_Y[0], 0.0],
            24.0,
            &[("no sign-in on this row", Tone::Error)],
        ),
        label(
            "note-models",
            [DIALOG[0], NOTE_Y[0], 0.0],
            24.0,
            &[
                ("+ Go models", Tone::Success),
                (" from the Console", Tone::Muted),
            ],
        ),
        label(
            "note-key",
            [DIALOG[0], NOTE_Y[0], 0.0],
            24.0,
            &[
                ("Go key", Tone::Accent),
                (" = OpenCode API key · paste it in this row", Tone::Muted),
            ],
        ),
        label(
            "note-env",
            [DIALOG[0], NOTE_Y[1], 0.0],
            24.0,
            &[("or set ", Tone::Muted), ("OPENCODE_API_KEY", Tone::Plain)],
        ),
        StageElement::beam("link", "dialog", "console"),
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
        StageElement::icon("cursor", CURSOR, 52.0, "cursor").tone(Tone::Plain),
    ]);
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

/// Everything that starts hidden.
const HIDDEN: [&str; 25] = [
    "console",
    "dialog",
    "dialog-error",
    "title",
    "esc",
    "popular",
    "row-hi",
    "go-name",
    "go-hint",
    "console-name",
    "console-hint",
    "anthropic-name",
    "anthropic-hint",
    "openai-name",
    "openai-hint",
    "go-name-error",
    "opencode-name",
    "opencode-hint",
    "prompt-title",
    "input",
    "caret",
    "placeholder",
    "submit",
    "note-stuck",
    "cursor",
];

/// The list's labels, top to bottom, as they enter and leave together.
const LIST: [&str; 11] = [
    "title",
    "esc",
    "popular",
    "go-name",
    "go-hint",
    "console-name",
    "console-hint",
    "anthropic-name",
    "anthropic-hint",
    "openai-name",
    "openai-hint",
];

const PROMPT: [&str; 5] = ["prompt-title", "input", "caret", "placeholder", "submit"];

/// A click: the cursor dips, and what it clicks flashes.
fn click(s: &mut StageActor, sc: &mut PlanBuilder, target: &str, at: u64) {
    s.to(sc, "cursor.scale", at, 0.86, 0.06);
    s.bounce(sc, "cursor.scale", at + millis(70), 1.0, 0.35, 0.2);
    s.hit(sc, &format!("{target}.flash"), at + millis(40), 0.7, 0.0);
    sfx::TICK.play(sc, format!("click-{at}"), at, -18.0);
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
    let s = &mut stage;
    let sc = &mut scene;

    for id in HIDDEN {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
    }
    for id in ["note-models", "note-key", "note-env"] {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
    }
    s.channel(sc, "link.draw", 0.0);
    s.channel(sc, "cursor.x", 160.0);
    s.channel(sc, "cursor.y", 140.0);
    // The renamed row sits where the Console row will be once the Go row goes.
    s.channel(sc, "opencode-name.y", -ROW_PITCH);
    s.channel(sc, "opencode-hint.y", -ROW_PITCH);

    // -------------------------------------------------------------- problem
    s.channel(sc, "camera.z", -120.0);
    s.to(sc, "camera.z", 0, 0.0, 2.2);
    let mut title = header(sc, "opencode", "connect and OpenCode Go")?;
    title.type_in(sc, seconds(0.3), 55.0, 0.6);
    let mut chip_problem = chip(sc, "chip-problem", Tone::Error, "the problem")?;
    chip_problem.show(sc, seconds(0.7));

    // You buy Go in the Console.
    let buy = problem.at("buy");
    s.settle_in(sc, "console", buy);
    sfx::BLOOM.play(sc, "console-in", buy, -24.0);
    let bought = problem.at("in the console");
    s.swap_status(sc, "console", bought, [0, 1], 0.35);
    s.hit(sc, "console.flash", bought + millis(200), 0.5, 0.0);
    sfx::MARK.play(sc, "bought", bought + millis(200), -20.0);

    // You open connect: the dialog settles in, its rows following.
    let open = problem.at("open connect");
    s.to(sc, "camera.x", open, 50.0, 1.6);
    for (property, from, to) in [("scale", 1.035, 1.0), ("y", 18.0, 0.0)] {
        s.channel(sc, &format!("dialog.{property}"), from);
        s.bounce(sc, &format!("dialog.{property}"), open, to, 0.6, 0.12);
    }
    s.fade_in(sc, "dialog", open, 1.0, 0.25);
    for (index, id) in LIST.iter().enumerate() {
        s.fade_in(sc, id, open + millis(120 + 40 * index as u64), 1.0, 0.3);
    }

    // The first row is OpenCode Go.
    let first = problem.at("first row");
    s.fade_in(sc, "row-hi", first, 1.0, 0.25);
    s.hit(sc, "row-hi.flash", first + millis(100), 0.5, 0.0);
    s.fade_in(sc, "cursor", first, 1.0, 0.3);
    s.glide(sc, "cursor.x", first, 0.0, 0.8);
    s.glide(sc, "cursor.y", first, 0.0, 0.8);

    // You pick it, and it asks for an API key.
    let pick = problem.at("pick it");
    click(s, sc, "row-hi", pick + millis(150));
    let asks = problem.at("asks for");
    for id in LIST.iter().chain(["row-hi"].iter()) {
        s.fade_out(sc, id, asks, 0.18);
    }
    s.fade_out(sc, "cursor", asks, 0.3);
    for (index, id) in PROMPT.iter().enumerate() {
        s.fade_in(sc, id, asks + millis(220 + 60 * index as u64), 1.0, 0.3);
    }
    s.hit(sc, "input.flash", problem.at("API key"), 0.4, 0.0);

    // No way to sign in: the dialog goes red at its edge.
    let no_way = problem.at("no way");
    s.fade_in(sc, "dialog-error", no_way, 1.0, 0.3);
    s.hit(sc, "dialog-error.flash", no_way, 0.6, 0.0);
    s.type_in(sc, "note-stuck", no_way + millis(150), 34.0);
    sfx::FAILURE.play(sc, "no-way", no_way, -24.0);
    let stuck = problem.at("stuck");
    s.hit(sc, "dialog-error.flash", stuck, 0.8, 0.0);
    let mut footer_problem = footer(
        sc,
        "footer-problem",
        vec![
            span("a Go subscriber gets ", Tone::Muted),
            span("a bare API key prompt", Tone::Error),
        ],
    )?;
    footer_problem.type_in(sc, problem.at("buying Go"), 46.0, 0.8);

    // -------------------------------------------------------------- console
    let start = console.start();
    footer_problem.hide(sc, start);
    chip_problem.hide(sc, start);
    let mut chip_console = chip(sc, "chip-console", Tone::Request, "what the Console does")?;
    chip_console.show(sc, start + millis(250));

    // Escape: the prompt goes, the rows come back.
    let back = console.at("already treats");
    for id in PROMPT.iter().chain(["dialog-error", "note-stuck"].iter()) {
        s.fade_out(sc, id, back, 0.2);
    }
    for (index, id) in LIST.iter().enumerate() {
        s.fade_in(sc, id, back + millis(200 + 30 * index as u64), 1.0, 0.3);
    }
    s.to(sc, "camera.x", back, -30.0, 1.8);

    // Go is part of your account; the Go page says to sign in to OpenCode.
    let account = console.at("part of your account");
    s.to(sc, "console.glow", account, 0.35, 0.5);
    s.hit(sc, "console.flash", account, 0.5, 0.0);
    let page = console.at("Go page");
    s.swap_status(sc, "console", page, [1, 2], 0.35);
    s.to(sc, "console.glow", page + seconds(1.5), 0.0, 0.6);

    // The second row does that sign-in today.
    let second = console.at("second row");
    s.to(sc, "camera.x", second, 30.0, 1.6);
    s.fade_in(sc, "row-hi", second, 1.0, 0.25);
    s.set(sc, "row-hi.y", second, ROW_PITCH);
    s.set(sc, "cursor.x", second, 120.0);
    s.set(sc, "cursor.y", second, ROW_PITCH + 120.0);
    s.fade_in(sc, "cursor", second, 1.0, 0.3);
    s.glide(sc, "cursor.x", second, 0.0, 0.7);
    s.glide(sc, "cursor.y", second, ROW_PITCH, 0.7);
    let signs_in = console.at("does that");
    click(s, sc, "row-hi", signs_in);
    let contact = s.connect(sc, "link", signs_in + millis(100), 0.5);
    sfx::TICK.play(sc, "link", contact, -20.0);
    s.twang(sc, "link", contact);
    let arrived = s.send(sc, "signin", contact + millis(350), 0.7);
    sfx::SEND.play(sc, "signin", contact + millis(350), -14.0);
    s.land(sc, "console", arrived);
    s.fade_out(sc, "cursor", arrived, 0.4);

    // Once you're in, the Console adds the Go models.
    let models_at = console.at("adds the Go");
    s.swap_status(sc, "console", console.at("once"), [2, 3], 0.35);
    let landed = s.send(sc, "models", models_at.saturating_sub(millis(300)), 0.75);
    sfx::SEND.play(sc, "models", models_at.saturating_sub(millis(300)), -14.0);
    s.land(sc, "dialog", landed);
    s.type_in(sc, "note-models", landed, 36.0);
    sfx::CONFIRM.play(sc, "models-land", landed, -22.0);
    s.hit(sc, "console.flash", console.at("subscribed"), 0.45, 0.0);

    // You'd have to know to skip the Go row.
    let skip = console.at("skip the");
    s.fade_in(sc, "go-name-error", skip, 1.0, 0.25);
    s.fade_out(sc, "go-name-error", skip + seconds(1.6), 0.4);
    let mut footer_console = footer(
        sc,
        "footer-console",
        vec![
            span("signing in to OpenCode ", Tone::Muted),
            span("already unlocks Go", Tone::Request),
        ],
    )?;
    footer_console.type_in(sc, console.at("as long as"), 46.0, 0.8);

    // ------------------------------------------------------------------ fix
    let start = fix.start();
    footer_console.hide(sc, start);
    chip_console.hide(sc, start);
    let mut chip_fix = chip(sc, "chip-fix", Tone::Success, "the fix")?;
    chip_fix.show(sc, start + millis(250));
    s.fade_out(sc, "note-models", start, 0.3);
    let one_row = fix.at("one OpenCode row");
    s.to(sc, "camera.x", one_row, 80.0, 1.8);
    s.to(sc, "camera.z", one_row, 50.0, 1.8);

    // The Go row goes, and the rows below slide up into its place.
    let drop = fix.at("drop the");
    s.fade_in(sc, "go-name-error", drop, 1.0, 0.15);
    for id in ["go-name", "go-hint"] {
        s.fade_out(sc, id, drop, 0.15);
    }
    s.fade_out(sc, "go-name-error", drop + millis(300), 0.3);
    sfx::WHOOSH.play(sc, "drop", drop + millis(300), -24.0);
    let rise = drop + millis(450);
    for id in [
        "console-name",
        "console-hint",
        "anthropic-name",
        "anthropic-hint",
        "openai-name",
        "openai-hint",
    ] {
        s.bounce(sc, &format!("{id}.y"), rise, -ROW_PITCH, 0.55, 0.12);
    }
    s.bounce(sc, "row-hi.y", rise, 0.0, 0.55, 0.12);

    // Console becomes OpenCode.
    let rename = fix.at("rename");
    s.swap_labels(sc, ["console-name", "opencode-name"], rename, millis(200));
    s.swap_labels(
        sc,
        ["console-hint", "opencode-hint"],
        rename + millis(80),
        millis(200),
    );
    s.hit(sc, "row-hi.flash", fix.at("just OpenCode"), 0.6, 0.0);
    sfx::MARK.play(sc, "renamed", fix.at("just OpenCode"), -20.0);

    // You sign in once, and Go comes with it.
    let once = fix.at("sign in once");
    s.set(sc, "cursor.x", once.saturating_sub(millis(600)), 120.0);
    s.set(sc, "cursor.y", once.saturating_sub(millis(600)), 120.0);
    s.fade_in(sc, "cursor", once.saturating_sub(millis(600)), 1.0, 0.3);
    s.glide(sc, "cursor.x", once.saturating_sub(millis(600)), 0.0, 0.6);
    s.glide(sc, "cursor.y", once.saturating_sub(millis(600)), 0.0, 0.6);
    click(s, sc, "row-hi", once);
    let arrived = s.send(sc, "signin-2", once + millis(200), 0.6);
    sfx::SEND.play(sc, "signin-2", once + millis(200), -14.0);
    s.land(sc, "console", arrived);
    s.fade_out(sc, "cursor", arrived, 0.4);
    let landed = s.send(sc, "models-2", fix.at("comes with"), 0.65);
    sfx::SEND.play(sc, "models-2", fix.at("comes with"), -14.0);
    s.land(sc, "dialog", landed);
    s.to(sc, "note-models.opacity", landed, 1.0, 0.3);
    sfx::CONFIRM.play(sc, "models-2-land", landed, -22.0);

    // Keys still work: a Go key is an OpenCode key, in the same row.
    let keys = fix.at("API keys still work");
    s.fade_out(sc, "note-models", keys, 0.25);
    s.type_in(sc, "note-key", keys + millis(300), 40.0);
    s.hit(sc, "row-hi.flash", fix.at("same row"), 0.5, 0.0);
    s.type_in(sc, "note-env", fix.at("or set"), 34.0);
    let mut footer_fix = footer(
        sc,
        "footer-fix",
        vec![
            span("one row", Tone::Success),
            span(
                " · sign in or paste a key · Go comes with the workspace",
                Tone::Muted,
            ),
        ],
    )?;
    footer_fix.type_in(sc, keys, 46.0, 0.8);

    scene.finish().context("film")
}

fn size(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.8), [("outro", seconds(2.2))])?;
    let mut scene = PlanBuilder::new("size", reading.duration());
    let [spoken] = reading.place(&mut scene);
    let sc = &mut scene;
    let mut title = header(sc, "opencode", "connect and OpenCode Go")?;
    title.show(sc, 0);
    let mut chip_plan = chip(sc, "chip-plan", Tone::Accent, "the plan")?;
    chip_plan.show(sc, seconds(0.3));

    let mut list = ChecklistActor::declare(
        sc,
        "plan",
        &ChecklistPlan::new([300.0, 360.0], 1320.0)
            .size(32.0)
            .title("recommendation")
            .rail()
            .panel()
            .item(ChecklistItemPlan::new("row", "one OpenCode row in connect").result("~30 lines"))
            .item(
                ChecklistItemPlan::new("pr", "#57155 · two rows sharing a login")
                    .failure("+1,394 lines · close"),
            ),
    )?;
    list.show(sc, seconds(0.4));
    list.reveal_item(sc, "row", spoken.at("the change"))?;
    list.start(sc, "row", spoken.at("the change"))?;
    let done = list.resolve(sc, "row", spoken.at("lines"), Outcome::Done)?;
    sfx::MARK.play(sc, "row-done", done.saturating_sub(millis(200)), -18.0);
    let replaces = spoken.at("replaces");
    list.reveal_item(sc, "pr", replaces)?;
    sfx::TICK.play(sc, "pr-in", replaces, -24.0);
    list.start(sc, "pr", spoken.at("both rows"))?;
    let closed = list.resolve(sc, "pr", spoken.at("close it"), Outcome::Failed)?;
    sfx::MARK.play(sc, "pr-closed", closed.saturating_sub(millis(200)), -20.0);
    let mut closing = footer(
        sc,
        "footer-plan",
        vec![
            span("ship ", Tone::Muted),
            span("one OpenCode row", Tone::Success),
            span(" · close #57155", Tone::Muted),
        ],
    )?;
    closing.type_in(sc, spoken.at("mostly tests"), 40.0, 0.8);
    scene.finish().context("size")
}
