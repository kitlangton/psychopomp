//! Uber Eats CLI explainer reel — zero generic Stage nodes.
//!
//! - Segment 1 (`ubereats-ui`): Real 60fps Browser Control screen recording
//!   (`ubereats-ui-flow.mp4`) with camera focus zooms into the search bar and
//!   the Karma Cafe "Espresso Nut" menu item, paired with a live `ChecklistActor`
//!   tracking the 4 slow LLM turns (25.0s).
//! - Segment 2 (`ubereats-harvest`): Wipes in (`UI CLICKING · 25s` -> `API HARVEST`).
//!   Left: `TerminalActor` running `browser-control network start` / `stop`.
//!   Right: `TreeActor` unfolding the 3 harvested JSON endpoints and highlighting
//!   the redacted `${BC_SECRET_1}` cookie and `x-csrf-token` header.
//! - Segment 3 (`ubereats-runtime`): Pushes in.
//!   Left: `BarsActor` benchmark (`25,000 ms` -> `60 ms`, `417×` speedup) +
//!   `TerminalActor` running `bun ubereats-cli.ts add "Espresso Nut"`.
//!   Right: Real 60fps Browser Control recording (`ubereats-handoff.mp4`) showing
//!   the in-page Liquid Glass Handoff modal (`Sign in to https://www.ubereats.com,
//!   then click Continue`) and zooming onto the Continue button as it is clicked.
//! - Segment 4 (`ubereats-code`): Pushes up into the 14-line `ubereats-cli.ts`
//!   Editor with pinned `CalloutActor` chips and a Liquid-Glass `LensActor` (loupe).

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    axis::AxisPlan,
    bars::{BarDeltaPlan, BarSeriesPlan, BarsActor, BarsPlan},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    caption::CaptionSpanPlan,
    checklist::{ChecklistActor, ChecklistItemPlan, ChecklistPlan, Outcome},
    code::StyledSpan,
    editor::{
        EDITOR_RECIPE, EditorLinePlan, EditorPartPlan, EditorRecipePlan, EditorSemanticRangePlan,
        EditorTargetSelector, diff,
    },
    effects::spinner::Mark,
    footage::{self, Clip, FootageActor, FootagePlan},
    highlight,
    lens::{LensActor, LensPlan},
    narration::Narration,
    plan::{
        ReelPlan, ReelSegmentPlan, ReelWipePlan, ScenePlan,
        WipeDirection::{Left, Up},
    },
    readout::ReadoutFormat,
    sfx,
    terminal::{TerminalActor, TerminalPlan},
    tone::Tone,
    tree::{TreeActor, TreePlan},
    video::{self, VideoActor, VideoPlan},
};
use psychopomp_pr_walkthrough::film::{Pr, chip, footer, header};
use serde_json::json;

const TITLE: Pr = Pr {
    number: "browser-control",
    title: "from browser flow to 60ms CLI",
    slug: "ubereats",
};

fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration = Narration::load(&root.join("narration"))?;

    let seg_ui = build_ui_segment(&narration)?;
    let (seg_harvest, seg_runtime) = build_after_segments(&narration)?;
    let seg_code = build_code_segment(&narration)?;

    let reel = ReelPlan::new(
        "ubereats-cli",
        vec![
            ReelSegmentPlan::cut(seg_ui),
            ReelSegmentPlan::wiped(
                seg_harvest,
                seconds(0.95),
                ReelWipePlan::new(Left).labeled("UI CLICKING · 25s", "API HARVEST"),
            ),
            ReelSegmentPlan::pushed(seg_runtime, seconds(0.8), Left),
            ReelSegmentPlan::pushed(seg_code, seconds(0.85), Up),
        ],
    )?;
    reel.validate()?;

    let output = root.join("ubereats-cli.reel.json");
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s, {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Segment 1: Real Browser Control UI Recording + 4-Turn Checklist
// ---------------------------------------------------------------------------

fn build_ui_segment(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.9), [("ubereats-before", seconds(1.4))])?;
    let mut scene = PlanBuilder::new("ubereats-ui", reading.duration());
    let [before] = reading.place(&mut scene);
    let b = |phrase: &str| before.at(phrase);

    header(&mut scene, &TITLE, Some(seconds(0.25)))?;
    let mut mode_chip = chip(
        &mut scene,
        "chip-ui",
        Tone::Warning,
        "UI clicking · 4 turns",
    )?;
    mode_chip.show(&mut scene, seconds(0.45));

    // Real 60fps CDP recording of Browser Control driving Uber Eats (1440x900).
    let clip_dur = reading.duration() - seconds(0.4);
    let mut browser_video = VideoActor::declare(
        &mut scene,
        "browser-ui",
        VideoPlan::new("ui-flow", [1440, 900], 60)
            .at([640.0, 525.0], 1120.0)
            .titled("🥤 karma-cafe  ·  ubereats.com/feed"),
        video::media(
            "ui-flow",
            "../../assets/ubereats-cli/ubereats-ui-flow-28s.mp4",
            (0, clip_dur),
            seconds(0.25),
        ),
    )?;
    browser_video.fly_in(&mut scene, seconds(0.25));

    // Camera focus moves on the live recording keyed to narration phrases.
    let search_at = b("type into the search");
    browser_video.focus(&mut scene, search_at, [250.0, 0.0, 940.0, 320.0], 0.85);
    sfx::TICK.play(&mut scene, "focus-search", search_at, -16.0);

    let results_at = b("wait for results");
    browser_video.unfocus(&mut scene, results_at, 0.85);

    let scroll_focus_at = b("scroll through a hundred") + seconds(1.2);
    browser_video.focus(
        &mut scene,
        scroll_focus_at,
        [280.0, 480.0, 880.0, 420.0],
        0.9,
    );
    browser_video.unfocus(&mut scene, b("25 seconds") + seconds(1.4), 0.9);

    // Right-hand Checklist tracking the 4 slow model turns.
    let check_plan = ChecklistPlan::new([1265.0, 225.0], 595.0)
        .size(24.0)
        .title("UI automation loop")
        .rail()
        .panel()
        .item(ChecklistItemPlan::new("t1", "Turn 1 · open /feed & snapshot").result("5.4s"))
        .item(ChecklistItemPlan::new("t2", "Turn 2 · search \"Karma Cafe\"").result("6.1s"))
        .item(ChecklistItemPlan::new("t3", "Turn 3 · dismiss closed dialog").result("6.7s"))
        .item(ChecklistItemPlan::new("t4", "Turn 4 · scroll 100 items & add").result("6.8s"));
    let mut checks = ChecklistActor::declare(&mut scene, "turns", &check_plan)?;
    checks.show(&mut scene, seconds(0.55));
    checks.reveal(&mut scene, seconds(0.8))?;

    let t1_start = b("open the feed");
    checks.start(&mut scene, "t1", t1_start)?;
    checks.resolve(&mut scene, "t1", search_at - millis(150), Outcome::Done)?;

    checks.start(&mut scene, "t2", search_at)?;
    let dialog_at = b("dismiss a closed store");
    checks.resolve(&mut scene, "t2", dialog_at - millis(150), Outcome::Done)?;

    checks.start(&mut scene, "t3", dialog_at)?;
    let scroll_at = b("scroll through a hundred");
    checks.resolve(&mut scene, "t3", scroll_at - millis(150), Outcome::Done)?;

    checks.start(&mut scene, "t4", scroll_at)?;
    let slow_at = b("25 seconds");
    checks.resolve(&mut scene, "t4", slow_at, Outcome::Done)?;
    sfx::FAILURE.play(&mut scene, "slow-total", slow_at, -11.0);

    let mut slow_footer = footer(
        &mut scene,
        "footer-ui",
        vec![
            span("UI clicking every run: ", Tone::Plain),
            span("25.0 seconds · 4 LLM turns", Tone::Error),
            span(" for a single smoothie", Tone::Muted),
        ],
    )?;
    slow_footer.type_in(&mut scene, slow_at, 46.0, 0.8);

    scene.finish().context("ubereats-ui")
}

// ---------------------------------------------------------------------------
// Segments 2 & 3: Harvest (Terminal + JSON Tree) & Runtime (Bars + Handoff Video)
// ---------------------------------------------------------------------------

fn build_after_segments(narration: &Narration) -> Result<(ScenePlan, ScenePlan)> {
    let clip = narration.clip("ubereats-after")?;
    let split_at = clip.split(23.0, "secret references", "now");

    // ── Segment 2: Terminal + Interactive JSON Tree (`0 .. split_at`) ──
    let lead1 = seconds(0.6);
    let tail1 = seconds(1.1);
    let dur1 = lead1 + split_at + tail1;
    let mut scene1 = PlanBuilder::new("ubereats-harvest", dur1);
    let spoken1 = clip.place_range(&mut scene1, lead1, 0, split_at, "-harvest");
    let a1 = |phrase: &str| spoken1.at(phrase);

    header(&mut scene1, &TITLE, Some(seconds(0.15)))?;
    let mut chip1 = chip(&mut scene1, "chip-harvest", Tone::Accent, "1. harvest once")?;
    chip1.show(&mut scene1, seconds(0.25));

    let mut term1 = TerminalActor::declare(
        &mut scene1,
        "term-harvest",
        TerminalPlan::new([70.0, 175.0], 850.0, 13)
            .size(20.0)
            .titled("zsh — browser-control network")
            .prompt(vec![
                span("karma-cafe", Tone::Accent),
                span(" ❯ ", Tone::Success),
            ]),
    )?;
    let t_shown = term1.show(&mut scene1, seconds(0.3));

    let cap_at = a1("start network capture");
    let cmd1_done = term1.type_command(
        &mut scene1,
        cap_at.max(t_shown + millis(200)),
        "browser-control network start --url \"/_p/api/\"",
    )?;
    sfx::SEND.play(&mut scene1, "net-start", cap_at, -12.0);

    let add_once = a1("add the smoothie once");
    let printed1 = term1.print(
        &mut scene1,
        cmd1_done + millis(200),
        [
            vec![
                span("● ", Tone::Accent),
                span("Capturing ", Tone::Plain),
                span("🥤 karma-cafe (/_p/api/*)", Tone::Muted),
            ],
            vec![],
        ],
    )?;
    let spin_capture = term1.spin(
        &mut scene1,
        add_once.max(printed1 + millis(150)),
        vec![span("Adding 1x Espresso Nut in browser once…", Tone::Muted)],
    )?;

    let stop_at = a1("stop capture");
    let resolved_cap = term1.resolve(
        &mut scene1,
        &spin_capture,
        stop_at,
        Mark::Check,
        vec![
            span("3 POST routes captured", Tone::Success),
            span("  [/_p/api/*]", Tone::Muted),
        ],
    )?;
    term1.prompt(&mut scene1, resolved_cap + millis(180))?;
    let stop_cmd = term1.type_command(
        &mut scene1,
        stop_at + millis(350),
        "browser-control network stop --secrets ubereats",
    )?;
    let digest_at = a1("clean digest");
    let redact_at = a1("cookies redacted");
    term1.print(
        &mut scene1,
        stop_cmd + millis(200),
        [
            vec![
                span("✓ ", Tone::Success),
                span("Endpoints: ", Tone::Plain),
                span(
                    "getSearchFeedV1, getStoreV1, createDraftOrderV2",
                    Tone::Accent,
                ),
            ],
            vec![
                span("✓ ", Tone::Success),
                span("Secrets:   ", Tone::Plain),
                span(
                    "cookie → ${BC_SECRET_1}, x-csrf-token → \"x\"",
                    Tone::Warning,
                ),
            ],
        ],
    )?;

    // Right side: Interactive TreeActor unfolding the harvested JSON digest.
    let digest_json = json!({
        "session": "🥤 karma-cafe",
        "endpoints": [
            {
                "method": "POST",
                "path": "/_p/api/getSearchFeedV1",
                "requestKeys": ["userQuery", "vertical"]
            },
            {
                "method": "POST",
                "path": "/_p/api/getStoreV1",
                "requestKeys": ["storeUuid", "diningMode"]
            },
            {
                "method": "POST",
                "path": "/_p/api/createDraftOrderV2",
                "requestKeys": ["isMulticart", "shoppingCartItems"]
            }
        ],
        "secrets": {
            "cookie": "${BC_SECRET_1}",
            "x-csrf-token": "x"
        }
    });
    let mut tree = TreeActor::declare(
        &mut scene1,
        "digest-tree",
        TreePlan::new([960.0, 175.0], 890.0, digest_json)
            .max_rows(19)
            .expanded(["$"]),
    )?;
    tree.show(&mut scene1, seconds(0.45));
    tree.open(&mut scene1, "$.endpoints", digest_at - millis(300))?;
    tree.open(&mut scene1, "$.endpoints[0]", digest_at)?;
    tree.open(&mut scene1, "$.endpoints[1]", digest_at + millis(350))?;
    tree.open(&mut scene1, "$.endpoints[2]", digest_at + millis(700))?;
    tree.highlight(
        &mut scene1,
        "$.endpoints[2].path",
        digest_at + millis(900),
        2.4,
    )?;
    sfx::CONFIRM.play(&mut scene1, "tree-open", digest_at, -12.0);

    tree.open(&mut scene1, "$.secrets", redact_at - millis(200))?;
    tree.reveal(&mut scene1, "$.secrets", redact_at)?;
    tree.highlight(
        &mut scene1,
        "$.secrets.cookie",
        redact_at + millis(200),
        2.6,
    )?;
    sfx::MARK.play(
        &mut scene1,
        "secret-highlight",
        redact_at + millis(200),
        -14.0,
    );

    let mut footer1 = footer(
        &mut scene1,
        "footer-harvest",
        vec![
            span("network.stop() ", Tone::Accent),
            span("extracts JSON routes & redacts cookies into ", Tone::Plain),
            span("${BC_SECRET_1}", Tone::Success),
        ],
    )?;
    footer1.type_in(&mut scene1, redact_at, 46.0, 0.8);

    // ── Segment 3: 60ms Benchmark Bars + CLI Terminal + Live Handoff Video ──
    let lead2 = seconds(0.5);
    let tail2 = seconds(1.3);
    let part2_len = clip.duration() - split_at;
    let dur2 = lead2 + part2_len + tail2;
    let mut scene2 = PlanBuilder::new("ubereats-runtime", dur2);
    let spoken2 = clip.place_range(&mut scene2, lead2, split_at, clip.duration(), "-runtime");
    let a2 = |phrase: &str| spoken2.at(phrase);

    header(&mut scene2, &TITLE, Some(seconds(0.15)))?;
    let mut chip2 = chip(
        &mut scene2,
        "chip-runtime",
        Tone::Success,
        "2. 60ms CLI + auto-handoff",
    )?;
    chip2.show(&mut scene2, seconds(0.25));

    // Top-left Benchmark Bars: 25,000ms UI vs 60ms CLI.
    let rows = [
        ("search", "Search store", 6200.0, 18.0),
        ("menu", "Read menu", 9400.0, 22.0),
        ("add", "Add to cart", 9400.0, 20.0),
    ];
    let bars_plan = rows.iter().fold(
        BarsPlan::new(
            [275.0, 215.0],
            470.0,
            AxisPlan::new([0.0, 10000.0]).every(5000.0),
        )
        .row_height(78.0)
        .size(21.0)
        .series(BarSeriesPlan::new("before", "UI clicks", Tone::Error))
        .series(BarSeriesPlan::new("after", "CLI fetch", Tone::Success))
        .readout(ReadoutFormat::new(0).grouped().unit("ms"))
        .delta(BarDeltaPlan::new("before", "after").factor()),
        |plan, (id, label, ..)| plan.row(*id, *label),
    );
    let mut bench = BarsActor::declare(&mut scene2, "bench", &bars_plan)?;
    bench.show(&mut scene2, seconds(0.3));
    let before_vals = rows
        .iter()
        .map(|(id, _, b, _)| (*id, *b))
        .collect::<Vec<_>>();
    let after_vals = rows
        .iter()
        .map(|(id, _, _, a)| (*id, *a))
        .collect::<Vec<_>>();
    let direct_at = a2("calls uber eats directly");
    let ms_at = a2("60 milliseconds");
    bench.grow(&mut scene2, direct_at, "before", &before_vals)?;
    bench.grow(&mut scene2, direct_at + millis(550), "after", &after_vals)?;
    bench.reveal_deltas(&mut scene2, ms_at)?;
    sfx::SUCCESS.play(&mut scene2, "bars-delta", ms_at, -10.0);

    // Bottom-left Terminal running `bun ubereats-cli.ts add "Espresso Nut"`.
    let mut term2 = TerminalActor::declare(
        &mut scene2,
        "term-cli",
        TerminalPlan::new([70.0, 545.0], 840.0, 9)
            .size(19.0)
            .titled("zsh — ubereats-cli.ts")
            .prompt(vec![
                span("~/ubereats", Tone::Request),
                span(" ❯ ", Tone::Success),
            ]),
    )?;
    let t2_shown = term2.show(&mut scene2, seconds(0.4));
    let run_cmd = term2.type_command(
        &mut scene2,
        t2_shown + millis(250),
        "bun ubereats-cli.ts add \"Espresso Nut\"",
    )?;
    let fast_spin = term2.spin(
        &mut scene2,
        run_cmd + millis(150),
        vec![span("POST /_p/api/createDraftOrderV2", Tone::Muted)],
    )?;
    let fast_done = term2.resolve(
        &mut scene2,
        &fast_spin,
        ms_at,
        Mark::Check,
        vec![
            span("1x Espresso Nut ($14.00) in cart", Tone::Success),
            span("  [60ms]", Tone::Accent),
        ],
    )?;
    term2.highlight(&mut scene2, &fast_spin, fast_done + millis(100), 1.5)?;

    // Expired session (401) -> In-page Continue pill -> Auto-retry!
    let expires_at = a2("expires");
    let err_401_at = a2("401");
    let pause_at = a2("pauses");
    let retry_at = a2("retries the request");

    term2.print(
        &mut scene2,
        err_401_at,
        [
            vec![],
            vec![
                span("! ", Tone::Warning),
                span("401 Unauthorized ", Tone::Error),
                span("→ pausing for in-page Continue pill…", Tone::Warning),
            ],
        ],
    )?;
    sfx::FAILURE.play(&mut scene2, "sfx-401", err_401_at, -11.0);

    let retry_spin = term2.spin(
        &mut scene2,
        pause_at,
        vec![span(
            "Waiting for sign-in on 🥤 karma-cafe tab…",
            Tone::Warning,
        )],
    )?;
    term2.resolve(
        &mut scene2,
        &retry_spin,
        retry_at,
        Mark::Check,
        vec![
            span("Signed in · auto-retried createDraftOrderV2", Tone::Success),
            span("  [200 OK]", Tone::Accent),
        ],
    )?;
    sfx::BLOOM.play(&mut scene2, "sfx-retry-ok", retry_at, -9.0);

    // Right side: Real recorded Browser Control tab showing the Handoff modal & Continue click!
    let handoff_start = err_401_at;
    let handoff_dur = seconds(7.8);
    let mut handoff_video = VideoActor::declare(
        &mut scene2,
        "handoff-video",
        VideoPlan::new("handoff-clip", [1440, 900], 60)
            .at([1415.0, 535.0], 860.0)
            .titled("🥤 karma-cafe  ·  in-page auth handoff"),
        video::media(
            "handoff-clip",
            "../../assets/ubereats-cli/ubereats-handoff.mp4",
            (0, handoff_dur),
            handoff_start,
        ),
    )?;
    // Keep a still frame of the Karma Cafe tab before `handoff_start`, then play the handoff clip.
    let pre_handoff = FootageActor::declare(
        &mut scene2,
        "pre-handoff",
        &FootagePlan::new(
            Clip::new("pre-handoff-media").trimmed(0.0, 0.5).looping(),
            [1415.0, 535.0],
            [860.0, 538.0],
        )
        .framed(),
        footage::media(
            "pre-handoff-media",
            "../../assets/ubereats-cli/ubereats-handoff.mp4",
            0,
            handoff_start + millis(400),
        ),
    )?;
    pre_handoff.fly_in(&mut scene2, seconds(0.4));
    let pre_op = pre_handoff.channel(&mut scene2, "opacity");
    scene2.spring(&pre_op, handoff_start, 0.0, 0.25, 0.0);

    let hv_op = handoff_video.channel(&mut scene2, "opacity", 0.0);
    scene2.spring(&hv_op, handoff_start, 1.0, 0.25, 0.0);

    let pill_at = a2("continue pill");
    handoff_video.focus(
        &mut scene2,
        pill_at - millis(300),
        [760.0, 500.0, 680.0, 400.0],
        0.75,
    );
    sfx::CONFIRM.play(&mut scene2, "zoom-pill", pill_at, -11.0);

    let sign_in_at = a2("sign in once");
    handoff_video.unfocus(&mut scene2, retry_at + millis(400), 0.85);
    let _ = (expires_at, sign_in_at);

    let mut footer2 = footer(
        &mut scene2,
        "footer-runtime",
        vec![
            span("60ms in-tab fetch · ", Tone::Plain),
            span("417× faster", Tone::Success),
            span(" · auto-handoff on 401", Tone::Accent),
        ],
    )?;
    footer2.type_in(&mut scene2, retry_at, 46.0, 0.8);

    Ok((
        scene1.finish().context("ubereats-harvest")?,
        scene2.finish().context("ubereats-runtime")?,
    ))
}

// ---------------------------------------------------------------------------
// Segment 4: 14-Line Code Editor + Liquid-Glass Loupe + Pinned Callouts
// ---------------------------------------------------------------------------

fn plain_line(id: &str, text: &str) -> EditorLinePlan {
    EditorLinePlan {
        id: id.into(),
        parts: vec![EditorPartPlan {
            id: "code".into(),
            spans: highlight::typescript(text),
        }],
        semantic_ranges: Vec::new(),
        mark: None,
    }
}

fn targeted_line(id: &str, lead: &str, range: (&str, &str), tail: &str) -> EditorLinePlan {
    let part = |id: &str, spans: Vec<StyledSpan>| EditorPartPlan {
        id: id.into(),
        spans,
    };
    let mut parts = vec![
        part("lead", highlight::typescript(lead)),
        part(range.0, highlight::typescript(range.1)),
    ];
    if !tail.is_empty() {
        parts.push(part("tail", highlight::typescript(tail)));
    }
    EditorLinePlan {
        id: id.into(),
        parts,
        semantic_ranges: vec![EditorSemanticRangePlan {
            id: range.0.into(),
            first_part_id: range.0.into(),
            last_part_id: range.0.into(),
        }],
        mark: None,
    }
}

fn editor_anchor(target: &str, edge: CalloutSide) -> CalloutAnchorPlan {
    CalloutAnchorPlan::Editor {
        id: target.into(),
        target: target.into(),
        edge,
        side: None,
    }
}

fn build_code_segment(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.8), [("ubereats-code", seconds(1.6))])?;
    let mut scene = PlanBuilder::new("ubereats-code", reading.duration());
    let [spoken] = reading.place(&mut scene);
    let c = |phrase: &str| spoken.at(phrase);

    header(&mut scene, &TITLE, Some(seconds(0.15)))?;
    let mut code_chip = chip(
        &mut scene,
        "chip-code",
        Tone::Accent,
        "3. 14 lines of TypeScript",
    )?;
    code_chip.show(&mut scene, seconds(0.25));

    let s1 = c("point browser control client");
    let s2 = c("get store");
    let s3 = c("create draft order");

    let initial = vec!["import".to_owned()];
    let step1 = vec![
        "import", "origin", "session", "headers", "handoff", "close1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let step2 = vec![
        "import",
        "origin",
        "session",
        "headers",
        "handoff",
        "close1",
        "blank1",
        "store",
        "storeUuid",
        "mode",
        "close2",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let final_ids = vec![
        "import",
        "origin",
        "session",
        "headers",
        "handoff",
        "close1",
        "blank1",
        "store",
        "storeUuid",
        "mode",
        "close2",
        "draft",
        "items",
        "close3",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();

    let mut snapshots = Vec::new();
    snapshots.extend(diff::step_snapshots(&initial, &step1, s1));
    snapshots.extend(diff::step_snapshots(&step1, &step2, s2));
    snapshots.extend(diff::step_snapshots(&step2, &final_ids, s3));

    let lines = vec![
        plain_line(
            "import",
            "import { BrowserControlClient } from \"@opencode-ai/browser-control\"",
        ),
        targeted_line(
            "origin",
            "const ubereats = ",
            (
                "originCall",
                "BrowserControlClient.origin(\"https://www.ubereats.com\"",
            ),
            ", {",
        ),
        targeted_line(
            "session",
            "  ",
            ("sessionProp", "session: \"karma-cafe\","),
            "",
        ),
        targeted_line(
            "headers",
            "  ",
            ("csrfProp", "headers: { \"x-csrf-token\": \"x\" },"),
            "",
        ),
        targeted_line(
            "handoff",
            "  ",
            ("handoffProp", "handoffOnAuthFailure: true,"),
            "",
        ),
        plain_line("close1", "})"),
        plain_line("blank1", " "),
        targeted_line(
            "store",
            "const store = await ",
            ("getStoreCall", "ubereats.post(\"/_p/api/getStoreV1\""),
            ", {",
        ),
        plain_line(
            "storeUuid",
            "  storeUuid: \"78cb1602-9f58-57bb-ad08-f6b8f80bb788\",",
        ),
        plain_line("mode", "  diningMode: \"DELIVERY\","),
        plain_line("close2", "})"),
        targeted_line(
            "draft",
            "await ",
            (
                "createDraftCall",
                "ubereats.post(\"/_p/api/createDraftOrderV2\"",
            ),
            ", {",
        ),
        plain_line(
            "items",
            "  isMulticart: true, shoppingCartItems: [espressoNutSmoothie],",
        ),
        targeted_line("close3", "", ("draftEnd", "})"), ""),
    ]
    .into_iter()
    .chain(diff::gap_lines(&snapshots))
    .collect::<Vec<_>>();

    let recipe = EditorRecipePlan {
        file_name: "ubereats-cli.ts".into(),
        focus_line_id: "handoff".into(),
        lines,
        initial_line_ids: initial,
        final_line_ids: final_ids,
        snapshots,
        line_height: 42.0,
        entering_offset_x: 0.0,
        focus_height: 42.0,
        inline_reveal: None,
        additional_inline_reveals: Vec::new(),
    };
    let editor = scene.actor("editor", EDITOR_RECIPE, &recipe)?;
    for (target, line) in [
        ("originCall", "origin"),
        ("sessionProp", "session"),
        ("csrfProp", "headers"),
        ("handoffProp", "handoff"),
        ("getStoreCall", "store"),
        ("createDraftCall", "draft"),
        ("draftEnd", "close3"),
    ] {
        scene.semantic_target(
            target,
            &editor,
            EditorTargetSelector {
                line_id: line.into(),
                range_id: target.into(),
            },
        )?;
    }
    let panel_y = scene.continuous(&editor, "panel-y", 60.0);
    let panel_opacity = scene.continuous(&editor, "panel-opacity", 0.0);
    scene.spring(&panel_y, 0, 0.0, 0.65, 0.0);
    scene.spring(&panel_opacity, 0, 1.0, 0.45, 0.0);

    // Pinned Callouts explaining the key properties right in the right margin.
    let mut session_callout = CalloutActor::declare(
        &mut scene,
        "callout-session",
        &CalloutPlan::new(
            editor_anchor("sessionProp", CalloutSide::Right),
            vec![
                span("reuses live tab ", Tone::Plain),
                span("cookies & TLS", Tone::Accent),
            ],
        )
        .side(CalloutSide::Right)
        .elbow()
        .reach(260.0)
        .tone(Tone::Accent)
        .chip(),
    )?;
    let mut handoff_callout = CalloutActor::declare(
        &mut scene,
        "callout-handoff",
        &CalloutPlan::new(
            editor_anchor("handoffProp", CalloutSide::Right),
            vec![
                span("401 → ", Tone::Warning),
                span("in-page Continue pill + retry", Tone::Success),
            ],
        )
        .side(CalloutSide::Right)
        .elbow()
        .reach(210.0)
        .tone(Tone::Success)
        .chip(),
    )?;
    let mut draft_callout = CalloutActor::declare(
        &mut scene,
        "callout-draft",
        &CalloutPlan::new(
            editor_anchor("draftEnd", CalloutSide::BottomRight),
            vec![
                span("60ms in-tab fetch · ", Tone::Plain),
                span("cart only, never orders", Tone::Success),
            ],
        )
        .side(CalloutSide::BottomRight)
        .elbow()
        .reach(44.0)
        .tone(Tone::Success)
        .chip(),
    )?;

    session_callout.show(&mut scene, c("karma cafe session"));
    handoff_callout.show(&mut scene, c("handoff on auth failure"));
    handoff_callout.emphasize(&mut scene, c("handoff on auth failure") + millis(300));
    draft_callout.show(&mut scene, c("without placing the order") - millis(400));

    // Liquid-Glass Loupe gliding across the key expressions as they are spoken.
    let mut loupe = LensActor::declare(
        &mut scene,
        "loupe",
        &LensPlan::capsule(
            editor_anchor("handoffProp", CalloutSide::Center),
            [420.0, 74.0],
        )
        .anchor(editor_anchor("getStoreCall", CalloutSide::Center))
        .anchor(editor_anchor("createDraftCall", CalloutSide::Center))
        .magnification(1.38),
    )?;
    let handoff_at = c("handoff on auth failure");
    loupe.show(&mut scene, handoff_at);
    loupe.move_to(&mut scene, "getStoreCall", s2 + millis(650))?;
    loupe.move_to(&mut scene, "createDraftCall", s3 + millis(650))?;
    loupe.hide(&mut scene, c("without placing the order") - millis(200));

    let mut code_footer = footer(
        &mut scene,
        "footer-code",
        vec![
            span("BrowserControlClient.origin() ", Tone::Accent),
            span("· live session cookies · auto-handoff on 401", Tone::Plain),
        ],
    )?;
    code_footer.type_in(&mut scene, s1, 46.0, 0.8);

    scene.finish().context("ubereats-code")
}
