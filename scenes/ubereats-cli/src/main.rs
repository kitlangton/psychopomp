//! Uber Eats CLI walkthrough as a Stage film + animated code zoom.
//! Act 1 (`ubereats-before`): clicking through the website takes 4 turns and 25s,
//! getting snagged on a closed-store dialog and scrolling 100 menu items.
//! Rewind (`◀◀ rewind`).
//! Act 2 (`ubereats-after`): capture the flow once in `🥤 karma-cafe`, get a
//! redacted 3-endpoint digest, and run `ubereats-cli` in 60ms. When login
//! expires (401), Browser Control pauses with an in-page Continue pill and
//! retries automatically.
//! Then the camera zooms inside the `ubereats-cli` card to reveal the 15-line
//! TypeScript implementation (`ubereats-code`).

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, PlanTime, seconds},
    editor::diff::{Diff, add, keep},
    math::{Vec2, Vec3, easing::Ease, vec2},
    narration::Narration,
    plan::{ReelPlan, ReelSegmentPlan, ReelTransitionStyle, ScenePlan},
    sfx,
    stage::{Camera, OrbEntrance, StageActor, StageElement, StagePlan, StagePost, reply_after},
    tone::Tone,
};
use psychopomp_pr_walkthrough::film::{Pr, chip, code, footer, header, span};

const AGENT: [f32; 3] = [340.0, 350.0, 0.0];
const CLI: [f32; 3] = [340.0, 690.0, -50.0];
const BROWSER: [f32; 3] = [920.0, 520.0, 0.0];
const UBEREATS: [f32; 3] = [1540.0, 520.0, 0.0];
const DIALOG: [f32; 3] = [920.0, 260.0, -80.0];

const CARD: [f32; 2] = [346.0, 120.0];
const BROWSER_SIZE: [f32; 2] = [392.0, 128.0];
const OVERLAY_SIZE: [f32; 2] = [364.0, 108.0];
const CLOSING_CAMERA: [f32; 3] = [-160.0, 70.0, 60.0];

const PR: Pr = Pr {
    number: "browser-control",
    title: "from browser flow to 60ms CLI",
    slug: "ubereats",
};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration = Narration::load(&root.join("narration"))?;
    let stage = stage_film(&narration)?;
    let code_scene = code(&PR, &narration, cli_diff(), false)?;
    let reel = ReelPlan {
        version: ReelPlan::VERSION,
        id: "ubereats-cli".to_owned(),
        segments: vec![
            ReelSegmentPlan {
                transition_nanos: 0,
                transition_style: ReelTransitionStyle::Dip,
                transition_focus: None,
                transition_wipe: None,
                plan: stage,
            },
            ReelSegmentPlan {
                transition_nanos: seconds(1.15),
                transition_style: ReelTransitionStyle::Zoom,
                transition_focus: Some(cli_rect()),
                transition_wipe: None,
                plan: code_scene,
            },
        ],
    };
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

fn cli_rect() -> [f32; 4] {
    Camera::at(Vec3::from(CLOSING_CAMERA), vec2(1920.0, 1080.0))
        .project_rect(Vec3::from(CLI), Vec2::from(CARD))
        .expect("cli card is in front of the camera")
}

fn cli_diff() -> (Diff, Vec<&'static str>, &'static str) {
    let diff = Diff {
        file_name: "ubereats-cli.ts",
        lines: vec![
            keep("import { BrowserControlClient } from \"@opencode-ai/browser-control\""),
            add(
                1,
                "const ubereats = BrowserControlClient.origin(\"https://www.ubereats.com\", {",
            ),
            add(1, "  session: \"karma-cafe\","),
            add(1, "  headers: { \"x-csrf-token\": \"x\" },"),
            add(1, "  handoffOnAuthFailure: true,"),
            add(1, "})"),
            keep(""),
            add(
                2,
                "const store = await ubereats.post(\"/_p/api/getStoreV1\", {",
            ),
            add(2, "  storeUuid: \"78cb1602-9f58-57bb-ad08-f6b8f80bb788\","),
            add(2, "  diningMode: \"DELIVERY\","),
            add(2, "})"),
            add(3, "await ubereats.post(\"/_p/api/createDraftOrderV2\", {"),
            add(
                3,
                "  isMulticart: true, shoppingCartItems: [espressoNutSmoothie],",
            ),
            add(3, "})"),
        ],
    };
    (
        diff,
        vec![
            "point browser control client",
            "get store",
            "create draft order",
        ],
        "condensed for display · 15 lines · live cookies + auto-handoff on 401",
    )
}

fn stage_plan() -> StagePlan {
    let elements = vec![
        StageElement::orb("ubereats", UBEREATS, 132.0)
            .points(900)
            .tone(Tone::Plain),
        StageElement::label(
            "ubereats-name",
            [UBEREATS[0], 688.0, 0.0],
            24.0,
            &[("Uber Eats API", Tone::Plain)],
        ),
        StageElement::label(
            "ubereats-sub",
            [UBEREATS[0], 720.0, 0.0],
            19.0,
            &[("●", Tone::Success), (" Karma Cafe · $14.00", Tone::Muted)],
        ),
        StageElement::card("agent", AGENT, CARD, "coding agent")
            .statuses(&[
                ("goal: 1x Espresso Nut", Tone::Plain),
                ("clicking UI · turn 1..4", Tone::Warning),
                ("25s · 4 model turns", Tone::Error),
                ("capturing flow once", Tone::Accent),
                ("endpoints harvested ✓", Tone::Success),
            ])
            .tone(Tone::Request),
        StageElement::card("browser", BROWSER, BROWSER_SIZE, "karma-cafe · browser tab")
            .statuses(&[
                ("attached · cookies live", Tone::Muted),
                ("searching & scrolling…", Tone::Warning),
                ("blocked by dialog", Tone::Error),
                ("network.start(/_p/api/)", Tone::Accent),
                ("window.fetch · 60ms ✓", Tone::Success),
                ("401 · waiting for sign-in", Tone::Warning),
            ])
            .tone(Tone::Plain),
        StageElement::card("cli", CLI, CARD, "ubereats-cli.ts")
            .statuses(&[
                ("BrowserControlClient.origin", Tone::Accent),
                ("POST createDraftOrderV2", Tone::Request),
                ("1x Espresso Nut in cart ✓", Tone::Success),
                ("paused for login handoff", Tone::Warning),
            ])
            .tone(Tone::Accent),
        StageElement::card("dialog", DIALOG, OVERLAY_SIZE, "store closed modal")
            .statuses(&[("intercepts pointer events", Tone::Error)])
            .tone(Tone::Error),
        StageElement::card("pill", DIALOG, OVERLAY_SIZE, "in-page handoff pill")
            .statuses(&[
                ("Sign in, then Continue →", Tone::Accent),
                ("signed in · continuing ✓", Tone::Success),
            ])
            .tone(Tone::Accent),
        StageElement::beam("ui-link", "agent", "browser").tone(Tone::Request),
        StageElement::beam("web-link", "browser", "ubereats").tone(Tone::Plain),
        StageElement::beam("cli-link", "cli", "browser").tone(Tone::Accent),
        StageElement::packet("click-1", "ui-link")
            .labeled("goto /feed")
            .tone(Tone::Request),
        StageElement::packet("click-2", "ui-link")
            .labeled("fill \"Karma Cafe\"")
            .tone(Tone::Request),
        StageElement::packet("click-3", "ui-link")
            .labeled("scroll 100 items")
            .tone(Tone::Warning),
        StageElement::packet("capture-once", "ui-link")
            .labeled("network.start + add once")
            .tone(Tone::Accent),
        StageElement::packet("api-probe", "web-link")
            .labeled("POST /_p/api/*")
            .tone(Tone::Accent),
        StageElement::packet("digest-back", "ui-link")
            .reversed()
            .labeled("3 endpoints · ${BC_SECRET_1}")
            .tone(Tone::Success),
        StageElement::packet("cli-call", "cli-link")
            .labeled("post(createDraftOrderV2)")
            .tone(Tone::Accent),
        StageElement::packet("fetch-fast", "web-link")
            .labeled("window.fetch · x-csrf-token")
            .tone(Tone::Accent),
        StageElement::packet("fetch-ok", "web-link")
            .reversed()
            .labeled("200 OK · 60ms")
            .tone(Tone::Success),
        StageElement::packet("cli-ok", "cli-link")
            .reversed()
            .labeled("1x Espresso Nut ✓")
            .tone(Tone::Success),
        StageElement::packet("cli-expired-try", "cli-link")
            .labeled("post(createDraftOrderV2)")
            .tone(Tone::Request),
        StageElement::packet("fetch-401", "web-link")
            .reversed()
            .labeled("401 Unauthorized")
            .tone(Tone::Error),
        StageElement::packet("fetch-retry", "web-link")
            .labeled("retry POST createDraftOrderV2")
            .tone(Tone::Accent),
        StageElement::packet("fetch-retry-ok", "web-link")
            .reversed()
            .labeled("200 OK · cart updated")
            .tone(Tone::Success),
        StageElement::packet("cli-retry-ok", "cli-link")
            .reversed()
            .labeled("1x Espresso Nut ✓")
            .tone(Tone::Success),
        StageElement::label(
            "digest-line",
            [BROWSER[0], 770.0, 0.0],
            19.0,
            &[
                ("endpoints ▸ ", Tone::Muted),
                ("getSearchFeed · getStore · createDraftOrder", Tone::Success),
            ],
        ),
        StageElement::label(
            "secret-line",
            [BROWSER[0], 804.0, 0.0],
            19.0,
            &[
                ("redacted  ▸ ", Tone::Muted),
                ("cookie=${BC_SECRET_1} · x-csrf-token=x", Tone::Accent),
            ],
        ),
        StageElement::ring("calm", UBEREATS, 146.0)
            .thickness(1.3)
            .tone(Tone::Success),
        StageElement::ring("calm-outer", UBEREATS, 152.0)
            .thickness(1.3)
            .tone(Tone::Success),
    ];
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

fn stage_film(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(1.4),
        [
            ("ubereats-before", seconds(2.0)),
            ("ubereats-after", seconds(2.2)),
        ],
    )?;
    let mut scene = PlanBuilder::new("ubereats-stage", reading.duration());
    let [before, after] = reading.place(&mut scene);
    let b = |phrase: &str| before.at(phrase);
    let a = |phrase: &str| after.at(phrase);
    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;

    // ── Establish: agent, browser tab, and Uber Eats API orb ──
    s.channel(sc, "camera.z", -140.0);
    s.channel(sc, "camera.dof", 0.42);
    s.to(sc, "camera.z", 0, 0.0, 2.0);
    s.orb_in(sc, "ubereats", seconds(0.15), OrbEntrance::HERO);
    s.fade_in(sc, "ubereats-name", seconds(0.8), 1.0, 0.5);
    s.fade_in(sc, "ubereats-sub", seconds(0.95), 1.0, 0.5);

    let browser_ready = s.settle_in(sc, "browser", seconds(0.45));
    let web_contact = s.connect(sc, "web-link", browser_ready + seconds(0.15), 0.65);
    sfx::TICK.play(sc, "connect-web", web_contact, -20.0);

    let agent_ready = s.settle_in(sc, "agent", seconds(0.65));
    let ui_contact = s.connect(sc, "ui-link", agent_ready + seconds(0.15), 0.65);
    sfx::TICK.play(sc, "connect-ui", ui_contact, -20.0);

    header(sc, &PR, Some(seconds(0.35)))?;
    let mut before_chip = chip(sc, "chip-before", Tone::Warning, "UI clicking")?;
    before_chip.show(sc, seconds(0.65));

    // ── Act 1: Clicking through the UI every time ──
    let clicks = b("clicks through the website");
    s.to(sc, "agent.status", clicks, 1.0, 0.35);
    s.clock(sc, "agent.spinner", clicks);
    s.to(sc, "ui-link.emphasis", clicks, 1.0, 0.5);

    let open_feed = s.send(sc, "click-1", b("open the feed"), 0.65);
    sfx::SEND.play(sc, "send-feed", b("open the feed"), -10.0);
    s.land(sc, "browser", open_feed);
    s.to(sc, "browser.status", open_feed, 1.0, 0.3);

    let search_box = s.send(sc, "click-2", b("type into the search"), 0.65);
    sfx::SEND.play(sc, "send-search", b("type into the search"), -10.0);
    s.land(sc, "browser", search_box);

    // Closed-store dialog pops and blocks pointer events.
    let dialog_at = b("dismiss a closed store");
    s.settle_in(sc, "dialog", dialog_at);
    s.hit(sc, "dialog.alarm", dialog_at + seconds(0.08), 0.45, 0.0);
    s.hit(sc, "browser.alarm", dialog_at + seconds(0.08), 0.35, 0.0);
    s.to(sc, "browser.status", dialog_at, 2.0, 0.25);
    s.glitch(sc, "dialog", dialog_at + seconds(0.06), [6.0, 8.0, 6.0]);
    sfx::FAILURE.play(sc, "dialog-block", dialog_at, -10.0);

    let scroll_at = b("scroll through a hundred");
    s.fade_out(sc, "dialog", scroll_at - seconds(0.2), 0.35);
    s.to(sc, "browser.status", scroll_at, 1.0, 0.3);
    let scroll_pkt = s.send(sc, "click-3", scroll_at, 0.7);
    s.land(sc, "browser", scroll_pkt);

    let slow_at = b("25 seconds");
    s.to(sc, "agent.status", slow_at, 2.0, 0.3);
    s.set(sc, "agent.spinner", slow_at, -1.0);
    s.hit(sc, "agent.alarm", slow_at, 0.45, 0.15);
    sfx::FAILURE.play(sc, "slow-verdict", slow_at, -11.0);
    let mut footer_before = footer(
        sc,
        "footer-before",
        vec![
            span("UI clicks every time: ", Tone::Plain),
            span("25 seconds · 4 LLM turns", Tone::Error),
        ],
    )?;
    footer_before.type_in(sc, slow_at, 44.0, 0.8);

    // ── Rewind ──
    let switch = before.end() + seconds(0.35);
    s.rewind(sc, switch, 0.12);
    sfx::LAUNCH.play(sc, "rewind", switch - seconds(0.1), -14.0);
    before_chip.hide(sc, switch);
    footer_before.hide(sc, switch);
    let mut rewind_chip = chip(sc, "chip-rewind", Tone::Accent, "◀◀ rewind")?;
    rewind_chip.show(sc, switch + seconds(0.25));
    rewind_chip.hide(sc, switch + seconds(1.4));
    let mut after_chip = chip(sc, "chip-after", Tone::Success, "harvest + CLI")?;
    after_chip.show(sc, switch + seconds(1.55));

    s.to(sc, "agent.alarm", switch, 0.0, 0.4);
    s.to(sc, "browser.alarm", switch, 0.0, 0.4);
    s.to(sc, "agent.status", switch + seconds(0.2), 0.0, 0.4);
    s.to(sc, "browser.status", switch + seconds(0.2), 0.0, 0.4);
    s.to(sc, "ui-link.emphasis", switch, 0.0, 0.4);

    // ── Act 2: Capture once -> Harvest endpoints -> 60ms CLI ──
    let start_cap = a("start network capture");
    s.to(sc, "agent.status", start_cap, 3.0, 0.3);
    s.to(sc, "browser.status", start_cap, 3.0, 0.3);
    let cap_pkt = s.send(sc, "capture-once", start_cap, 0.65);
    sfx::SEND.play(sc, "cap-send", start_cap, -10.0);
    s.land(sc, "browser", cap_pkt);

    let add_once = a("add the smoothie once");
    let probe_pkt = s.send(
        sc,
        "api-probe",
        add_once.not_before(reply_after(cap_pkt)),
        0.65,
    );
    s.land(sc, "ubereats", probe_pkt);

    // Clean digest returns with redacted secret refs.
    let digest_at = a("clean digest");
    let digest_pkt = s.send(sc, "digest-back", digest_at - seconds(0.2), 0.65);
    s.land(sc, "agent", digest_pkt);
    s.to(sc, "agent.status", digest_pkt, 4.0, 0.3);
    s.type_in(sc, "digest-line", digest_at, 52.0);
    sfx::CONFIRM.play(sc, "digest-land", digest_pkt, -12.0);

    let redacted_at = a("cookies redacted");
    s.type_in(sc, "secret-line", redacted_at, 52.0);
    sfx::MARK.play(sc, "redacted-mark", redacted_at, -14.0);

    // The generated CLI card settles in and connects to the browser session.
    let cli_at = a("command line tool");
    let cli_ready = s.settle_in(sc, "cli", cli_at);
    let cli_contact = s.connect(sc, "cli-link", cli_ready + seconds(0.15), 0.55);
    s.hit(sc, "cli-link.surge", cli_contact, 0.4, 0.0);
    s.twang(sc, "cli-link", cli_contact);
    sfx::TICK.play(sc, "connect-cli", cli_contact, -18.0);

    // Direct 60ms API call through the live browser tab!
    let direct_at = a("calls uber eats directly");
    s.to(sc, "cli.status", direct_at, 1.0, 0.25);
    s.to(sc, "cli.glow", direct_at, 0.5, 0.5);
    let cli_call = s.send(sc, "cli-call", direct_at, 0.45);
    sfx::SEND.play(sc, "cli-send", direct_at, -9.0);
    s.land(sc, "browser", cli_call);

    let fast_fetch = s.send(sc, "fetch-fast", reply_after(cli_call), 0.45);
    s.land(sc, "ubereats", fast_fetch);
    let fast_ok = s.send(sc, "fetch-ok", reply_after(fast_fetch), 0.45);
    s.land(sc, "browser", fast_ok);
    s.to(sc, "browser.status", fast_ok, 4.0, 0.25);

    let cli_ok = s.send(sc, "cli-ok", reply_after(fast_ok), 0.45);
    s.land(sc, "cli", cli_ok);
    s.to(sc, "cli.status", cli_ok, 2.0, 0.25);
    s.hit(sc, "cli.flash", cli_ok, 0.5, 0.0);
    sfx::SUCCESS.play(sc, "cli-60ms", cli_ok, -8.0);

    // ── Act 3: Expired login (401) -> In-page Continue pill -> Auto-retry ──
    let expires_at = a("expires");
    s.to(sc, "digest-line.opacity", expires_at, 0.0, 0.4);
    s.to(sc, "secret-line.opacity", expires_at, 0.0, 0.4);
    let try_expired = s.send(sc, "cli-expired-try", expires_at, 0.45);
    s.land(sc, "browser", try_expired);

    let err_401_at = a("401");
    let pkt_401 = s.send(sc, "fetch-401", err_401_at - seconds(0.25), 0.45);
    s.land(sc, "browser", pkt_401);
    s.to(sc, "browser.status", pkt_401, 5.0, 0.25);
    s.hit(sc, "browser.alarm", pkt_401, 0.35, 0.0);
    sfx::FAILURE.play(sc, "err-401", pkt_401, -11.0);

    // Browser Control pauses and shows the in-page Continue pill.
    let pill_at = a("continue pill");
    s.to(sc, "cli.status", a("pauses"), 3.0, 0.25);
    s.settle_in(sc, "pill", pill_at - seconds(0.2));
    s.hit(sc, "pill.flash", pill_at, 0.45, 0.0);
    sfx::CONFIRM.play(sc, "pill-show", pill_at, -11.0);

    // User signs in once; pill turns green and fades; request retries automatically!
    let sign_in_at = a("sign in once");
    s.to(sc, "pill.status", sign_in_at, 1.0, 0.25);
    s.hit(sc, "pill.flash", sign_in_at, 0.5, 0.0);
    sfx::SUCCESS.play(sc, "signed-in", sign_in_at, -9.0);

    let retry_at = a("retries the request");
    s.fade_out(sc, "pill", retry_at, 0.35);
    let retry_pkt = s.send(sc, "fetch-retry", retry_at, 0.42);
    sfx::SEND.play(sc, "retry-send", retry_at, -9.0);
    s.land(sc, "ubereats", retry_pkt);
    s.hit(sc, "ubereats.pulse", retry_pkt + seconds(0.1), 0.75, 0.0);
    s.ease(sc, "ubereats.rotation", retry_pkt, 1.4, 2.0, Ease::CubicOut);
    s.halo(sc, [("calm", 0.35), ("calm-outer", 0.5)], retry_pkt, 0.22);

    let retry_ok = s.send(sc, "fetch-retry-ok", reply_after(retry_pkt), 0.42);
    s.land(sc, "browser", retry_ok);
    s.to(sc, "browser.status", retry_ok, 4.0, 0.25);

    let final_ok = s.send(sc, "cli-retry-ok", reply_after(retry_ok), 0.42);
    s.land(sc, "cli", final_ok);
    s.to(sc, "cli.status", final_ok, 2.0, 0.25);
    s.hit(sc, "cli.flash", final_ok, 0.55, 0.0);
    s.hit(sc, "post.bloom", final_ok, 0.28, 0.18);
    sfx::BLOOM.play(sc, "final-bloom", final_ok, -8.0);

    let mut footer_after = footer(
        sc,
        "footer-after",
        vec![
            span("60ms in-tab fetch · ", Tone::Plain),
            span("auto-handoff on 401", Tone::Success),
        ],
    )?;
    footer_after.type_in(sc, retry_at, 44.0, 0.8);

    // ── Closing camera glides to `cli` card before zooming into its code ──
    let close = after.end() + seconds(0.3);
    s.to(sc, "camera.x", close, CLOSING_CAMERA[0], 1.2);
    s.to(sc, "camera.y", close, CLOSING_CAMERA[1], 1.2);
    s.to(sc, "camera.z", close, CLOSING_CAMERA[2], 1.2);
    s.to(sc, "camera.focus", close, -50.0, 1.0);
    s.to(sc, "cli.glow", close, 0.65, 0.8);
    after_chip.hide(sc, close);
    footer_after.hide(sc, close);
    s.halo_out(sc, ["calm", "calm-outer"], close, 0.6);

    scene.finish().context("ubereats-stage")
}
