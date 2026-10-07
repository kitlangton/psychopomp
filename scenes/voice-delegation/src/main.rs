//! How a voice call works in OpenCode's iOS app, and the open choice of what
//! the session shows: opaque or transparent delegation.
//!
//! One Stage film carries the call: you, the voice agent (GPT-Live, an orb),
//! the app that relays, and the delegation agent (the OpenCode session, a
//! card). One turn plays end to end, then what you heard and what happened
//! sit above and below the stage. Opaque drops a panel over the voice agent
//! and the app while the session opens into the prototype's phone;
//! transparent lifts the panel and the same phone shows a Voice message. A
//! checklist closes on the open choices.
//!
//! Sources: `docs/voice-agent-design.md` (terms, D1–D5, open questions) and
//! `prototypes/voice-delegation-display` in the opencode-ios repository; the
//! phones are screenshots of that prototype.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    callout::CalloutAnchorPlan,
    caption::{CaptionActor, CaptionPlan, CaptionSpanPlan},
    checklist::{ChecklistActor, ChecklistItemPlan, ChecklistPlan, Outcome},
    chrome::{chip, footer, header},
    footage::{self, Clip, Fit, FootageActor, FootagePlan},
    lens::{LensActor, LensPlan},
    math::easing::Ease,
    narration::Narration,
    plan::{ReelPlan, ReelSegmentPlan, ScenePlan},
    sfx,
    stage::{
        Arrow, Figure, Fill, Material, OrbEntrance, StageActor, StageElement, StagePlan, StagePost,
        reply_after,
    },
    tone::Tone,
};

const YOU: [f32; 3] = [200.0, 520.0, 0.0];
const VOICE: [f32; 3] = [640.0, 520.0, 0.0];
const VOICE_RADIUS: f32 = 118.0;
const APP: [f32; 3] = [1060.0, 520.0, 0.0];
const APP_SIZE: f32 = 56.0;
const SESSION: [f32; 3] = [1580.0, 520.0, 0.0];
const SESSION_SIZE: [f32; 2] = [380.0, 132.0];
/// Each agent's name and role, under it.
const NAME_Y: f32 = 680.0;
const ROLE_Y: f32 = 714.0;
/// Notes above and below the wires.
const ABOVE: f32 = 458.0;
const BELOW: f32 = 558.0;
/// The middle of the free wire between the voice agent and the app, and
/// between the app and the session.
const HANDOFF_X: f32 = (VOICE[0] + VOICE_RADIUS + APP[0] - APP_SIZE * 0.5) * 0.5;
const PROMPT_X: f32 = (APP[0] + APP_SIZE * 0.5 + SESSION[0] - SESSION_SIZE[0] * 0.5) * 0.5;

/// The panel opaque delegation draws over the voice agent and the app.
const CURTAIN: [f32; 3] = [860.0, 560.0, -30.0];
const CURTAIN_SIZE: [f32; 2] = [880.0, 420.0];

/// The prototype's phone, shown where the session card sits: the top of
/// the screenshot, faded out below the reply.
const PHONE: [f32; 2] = [1580.0, 504.0];
const PHONE_SIZE: [f32; 2] = [520.0, 607.7];
/// The cropped screenshot's pixels.
const PHONE_SOURCE: [f32; 2] = [984.0, 1150.0];

/// Captions: what you heard above the stage, what happened below it.
const TEXT_X: f32 = 140.0;
const HEARD_Y: [f32; 4] = [180.0, 224.0, 264.0, 304.0];
const HAPPENED_Y: [f32; 4] = [826.0, 870.0, 908.0, 946.0];

/// A point in the phone screenshot, on screen.
fn on_phone(source: [f32; 2]) -> [f32; 2] {
    let scale = PHONE_SIZE[1] / PHONE_SOURCE[1];
    let left = PHONE[0] - PHONE_SOURCE[0] * scale * 0.5;
    let top = PHONE[1] - PHONE_SIZE[1] * 0.5;
    [left + source[0] * scale, top + source[1] * scale]
}

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration = Narration::load(&root.join("narration"))?;
    let reel = ReelPlan::new(
        "voice-delegation",
        vec![
            ReelSegmentPlan::cut(call(&narration)?),
            ReelSegmentPlan::dipped(choices(&narration)?, millis(700)),
        ],
    )?;
    let output = root.join("voice-delegation.reel.json");
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

fn icon(id: &str, at: [f32; 3], size: f32, name: &str, tone: Tone) -> StageElement {
    StageElement::Icon {
        id: id.into(),
        at,
        size,
        icon: name.into(),
        path: String::new(),
        view: 256.0,
        ink: None,
        tone,
    }
}

fn stage_plan() -> StagePlan {
    let label = StageElement::label;
    let elements = vec![
        icon("you", YOU, 72.0, "user", Tone::Plain),
        label(
            "you-name",
            [YOU[0], 604.0, 0.0],
            24.0,
            &[("you", Tone::Plain)],
        ),
        StageElement::orb("voice", VOICE, VOICE_RADIUS)
            .points(900)
            .tone(Tone::Request),
        label(
            "voice-name",
            [VOICE[0], NAME_Y, 0.0],
            28.0,
            &[("voice agent", Tone::Plain)],
        ),
        label(
            "voice-role",
            [VOICE[0], ROLE_Y, 0.0],
            21.0,
            &[
                ("GPT-Live", Tone::Request),
                (" · hears you, talks", Tone::Muted),
            ],
        ),
        label(
            "voice-limit",
            [VOICE[0], ROLE_Y + 32.0, 0.0],
            21.0,
            &[("no tools", Tone::Muted)],
        ),
        icon("app", APP, APP_SIZE, "device-mobile", Tone::Muted),
        label(
            "app-name",
            [APP[0], 612.0, 0.0],
            21.0,
            &[("the app", Tone::Muted)],
        ),
        StageElement::card("session", SESSION, SESSION_SIZE, "opencode session").statuses(&[
            ("shell · edit · search · tests", Tone::Muted),
            ("running the tests", Tone::Plain),
            ("found a retry race", Tone::Plain),
            ("editing UploadQueue.swift", Tone::Plain),
            ("running the tests again", Tone::Plain),
            ("12/12 pass · 1 flaky on CI", Tone::Warning),
        ]),
        label(
            "session-name",
            [SESSION[0], NAME_Y, 0.0],
            28.0,
            &[("delegation agent", Tone::Plain)],
        ),
        label(
            "session-role",
            [SESSION[0], ROLE_Y, 0.0],
            21.0,
            &[("the session you’re in", Tone::Muted)],
        ),
        StageElement::beam("speech", "you", "voice"),
        StageElement::beam("handoff", "voice", "app"),
        StageElement::beam("prompt", "app", "session"),
        label(
            "signal-name",
            [HANDOFF_X, ABOVE, 0.0],
            19.0,
            &[("delegation.created", Tone::Plain)],
        ),
        label(
            "signal-note",
            [HANDOFF_X, BELOW, 0.0],
            19.0,
            &[("no text", Tone::Accent)],
        ),
        label(
            "prompt-name",
            [PROMPT_X, ABOVE, 0.0],
            19.0,
            &[("POST /prompt", Tone::Plain)],
        ),
        label(
            "prompt-note",
            [PROMPT_X, BELOW, 0.0],
            17.0,
            &[
                ("your exact words", Tone::Request),
                (" + transcript", Tone::Muted),
            ],
        ),
        label(
            "quiet-name",
            [HANDOFF_X, BELOW, 0.0],
            19.0,
            &[("thinking.append", Tone::Plain), (" quiet", Tone::Muted)],
        ),
        label(
            "spoken-name",
            [HANDOFF_X, BELOW, 0.0],
            19.0,
            &[
                ("commentary.append", Tone::Plain),
                (" spoken", Tone::Accent),
            ],
        ),
        StageElement::packet("say", "speech")
            .labeled("your words")
            .tone(Tone::Request),
        StageElement::packet("ack", "speech")
            .reversed()
            .labeled("“sure, checking…”")
            .tone(Tone::Accent),
        StageElement::packet("signal", "handoff").tone(Tone::Request),
        StageElement::packet("words", "prompt")
            .labeled("your words")
            .tone(Tone::Request),
        StageElement::packet("progress", "prompt")
            .reversed()
            .labeled("progress")
            .tone(Tone::Muted),
        StageElement::packet("quiet", "handoff")
            .reversed()
            .tone(Tone::Muted),
        StageElement::packet("result", "prompt")
            .reversed()
            .labeled("reply"),
        StageElement::packet("spoken", "handoff")
            .reversed()
            .tone(Tone::Accent),
        StageElement::packet("back", "speech")
            .reversed()
            .labeled("“tests pass now”")
            .tone(Tone::Accent),
        // The opaque replay: the same reply, mostly out of sight.
        StageElement::packet("result-2", "prompt").reversed(),
        StageElement::packet("spoken-2", "handoff")
            .reversed()
            .tone(Tone::Accent),
        StageElement::packet("back-2", "speech")
            .reversed()
            .labeled("“tests pass now”")
            .tone(Tone::Accent),
        // The transparent replay: the voice agent prompts the session.
        StageElement::packet("signal-2", "handoff").tone(Tone::Request),
        StageElement::packet("voice-message", "prompt")
            .labeled("Voice message")
            .tone(Tone::Request),
        StageElement::Shape {
            id: "curtain".into(),
            at: CURTAIN,
            shape: Figure::Rect(CURTAIN_SIZE),
            corner: 22.0,
            fill: Some(Fill::Material(Material::Surface)),
            fill_opacity: 1.0,
            stroke: Some(Tone::Muted),
            width: 1.4,
            dash: None,
            arrow: Arrow::None,
        },
        label(
            "curtain-name",
            [CURTAIN[0], CURTAIN[1] - 14.0, CURTAIN[2] - 1.0],
            32.0,
            &[("one assistant", Tone::Plain)],
        ),
        label(
            "curtain-note",
            [CURTAIN[0], CURTAIN[1] + 30.0, CURTAIN[2] - 1.0],
            19.0,
            &[("voice and delegation hidden", Tone::Muted)],
        ),
    ];
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

/// A caption line at `origin`, hidden until it is typed in or shown.
fn line(
    scene: &mut PlanBuilder,
    id: &str,
    origin: [f32; 2],
    size: f32,
    spans: Vec<CaptionSpanPlan>,
) -> Result<CaptionActor> {
    CaptionActor::declare(scene, id, &CaptionPlan::line(origin, size, spans))
}

fn call(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(1.6),
        [
            ("intro", seconds(1.0)),
            ("turn", seconds(1.2)),
            ("replies", seconds(1.4)),
            ("opaque", seconds(1.1)),
            ("transparent", seconds(1.8)),
        ],
    )?;
    let mut scene = PlanBuilder::new("call", reading.duration());
    let [intro, turn, replies, opaque, transparent] = reading.place(&mut scene);
    let mut stage = StageActor::declare(&mut scene, "stage", &stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;

    // ---------------------------------------------------------------- intro
    // Dolly in on an empty stage; you are there first.
    s.channel(sc, "camera.z", -160.0);
    s.channel(sc, "camera.dof", 0.45);
    s.to(sc, "camera.z", 0, 0.0, 2.2);
    s.fade_in(sc, "you", seconds(0.5), 1.0, 0.6);
    s.fade_in(sc, "you-name", seconds(0.65), 1.0, 0.6);
    let mut title = header(sc, "opencode ios", "voice mode")?;
    title.type_in(sc, seconds(0.4), 55.0, 0.6);
    let mut chip_call = chip(sc, "chip-call", Tone::Request, "how a call works")?;
    chip_call.show(sc, seconds(0.8));

    // Two agents: the voice agent's orb gathers in, the session settles.
    let two = intro.at("two agents");
    s.orb_in(sc, "voice", two, OrbEntrance::HERO);
    s.settle_in(sc, "session", two + millis(240));
    sfx::BLOOM.play(sc, "voice-in", two, -20.0);

    // The voice agent hears you and talks: the speech wire connects and
    // carries one breath each way.
    let voice_agent = intro.at("the voice agent");
    s.to(sc, "camera.x", voice_agent, -50.0, 1.6);
    s.to(sc, "camera.z", voice_agent, 40.0, 1.6);
    s.type_in(sc, "voice-name", voice_agent, 30.0);
    s.type_in(sc, "voice-role", intro.at("GPT Live"), 34.0);
    let hears = intro.at("hears you");
    let contact = s.connect(sc, "speech", hears.saturating_sub(millis(300)), 0.6);
    sfx::TICK.play(sc, "speech-connect", contact, -20.0);
    s.to(sc, "speech.flow", intro.at("talks back"), 0.5, 0.4);
    s.to(
        sc,
        "speech.flow",
        intro.at("talks back") + seconds(1.4),
        0.0,
        0.5,
    );
    s.type_in(sc, "voice-limit", intro.at("do the work"), 30.0);

    // It delegates: the wire runs through the app to the session.
    let delegates = intro.at("delegates");
    s.fade_in(sc, "app", delegates, 1.0, 0.5);
    s.fade_in(sc, "app-name", delegates + millis(120), 1.0, 0.5);
    let reached = s.connect(sc, "handoff", delegates, 0.55);
    sfx::TICK.play(sc, "handoff-connect", reached, -21.0);
    let reached = s.connect(sc, "prompt", reached + millis(60), 0.55);
    sfx::TICK.play(sc, "prompt-connect", reached, -21.0);
    s.hit(sc, "prompt.surge", reached, 0.35, 0.0);
    s.twang(sc, "prompt", reached);
    let delegation_agent = intro.at("delegation agent");
    s.to(sc, "camera.x", delegation_agent, 80.0, 1.6);
    s.to(sc, "camera.z", delegation_agent, 40.0, 1.6);
    s.type_in(sc, "session-name", delegation_agent, 32.0);
    s.type_in(sc, "session-role", intro.at("OpenCode session"), 34.0);
    let tools = intro.at("has the tools");
    s.hit(sc, "session.flash", tools, 0.5, 0.0);
    s.to(sc, "session.glow", tools, 0.35, 0.5);
    let mut footer_agents = footer(
        sc,
        "footer-agents",
        vec![
            span("the voice agent ", Tone::Muted),
            span("talks", Tone::Request),
            span(" · the session ", Tone::Muted),
            span("works", Tone::Plain),
        ],
    )?;
    footer_agents.type_in(sc, tools, 44.0, 0.8);
    footer_agents.hide(sc, intro.end() + millis(500));
    s.to(sc, "session.glow", intro.end() + millis(500), 0.0, 0.6);

    // ----------------------------------------------------------------- turn
    // What you heard builds up above the stage, one line per utterance.
    let mut heard_title = line(
        sc,
        "heard-title",
        [TEXT_X, HEARD_Y[0]],
        19.0,
        vec![span("what you heard", Tone::Accent)],
    )?;
    let mut heard_you = line(
        sc,
        "heard-you",
        [TEXT_X, HEARD_Y[1]],
        23.0,
        vec![
            span("you    ", Tone::Muted),
            span(
                "“uh can you check why the upload test keeps failing — on the staging branch I mean”",
                Tone::Plain,
            ),
        ],
    )?;
    let mut heard_ack = line(
        sc,
        "heard-ack",
        [TEXT_X, HEARD_Y[2]],
        23.0,
        vec![
            span("voice  ", Tone::Muted),
            span(
                "“Sure, checking why the upload test fails on staging.”",
                Tone::Plain,
            ),
        ],
    )?;
    let mut heard_result = line(
        sc,
        "heard-result",
        [TEXT_X, HEARD_Y[3]],
        23.0,
        vec![
            span("voice  ", Tone::Muted),
            span(
                "“Found it — the retry races the upload. I’ve added a lock; ",
                Tone::Plain,
            ),
            span("tests pass now", Tone::Accent),
            span(".”", Tone::Plain),
        ],
    )?;

    // You ask. Your words travel to the voice agent, which answers at once.
    let ask = turn.at("ask it");
    s.to(sc, "camera.x", ask, -40.0, 1.4);
    s.to(sc, "camera.z", ask, 30.0, 1.4);
    heard_title.show(sc, ask);
    heard_you.type_in(sc, ask + millis(150), 48.0, 0.4);
    let said = s.send(sc, "say", turn.at("upload test"), 0.85);
    sfx::SEND.play(sc, "say", turn.at("upload test"), -12.0);
    s.hit(sc, "voice.pulse", said, 0.6, 0.0);
    let answers = turn.at("answers right away");
    s.send(sc, "ack", answers, 0.8);
    heard_ack.type_in(sc, answers + millis(200), 46.0, 0.4);

    // The handoff: a signal with no text, then the app's own prompt.
    let hands_off = turn.at("hands off");
    s.to(sc, "camera.x", hands_off, 60.0, 1.6);
    let signal = s.send(sc, "signal", hands_off, 0.8);
    sfx::SEND.play(sc, "signal", hands_off, -14.0);
    s.land(sc, "app", signal);
    s.type_in(sc, "signal-name", signal, 40.0);
    s.type_in(sc, "signal-note", turn.at("carries no text"), 24.0);
    let sends = turn.at("the app sends");
    let words = s.send(sc, "words", sends + millis(200), 0.85);
    sfx::SEND.play(sc, "words", sends + millis(200), -12.0);
    s.land(sc, "session", words);
    s.type_in(sc, "prompt-name", sends, 36.0);
    s.type_in(sc, "prompt-note", turn.at("exact words"), 36.0);
    let mut footer_handoff = footer(
        sc,
        "footer-handoff",
        vec![
            span("the delegation carries no text, so ", Tone::Muted),
            span("the app sends your words", Tone::Request),
        ],
    )?;
    footer_handoff.type_in(sc, turn.at("call transcript"), 46.0, 0.8);

    // The session works: its status steps through the run while the
    // spinner turns.
    let runs = turn.at("runs the tests");
    s.to(sc, "camera.x", runs, 80.0, 1.6);
    s.to(sc, "camera.z", runs, 50.0, 1.6);
    s.to(sc, "camera.focus", runs, 0.0, 1.0);
    s.swap_status(sc, "session", runs, [0, 1], 0.35);
    s.clock(sc, "session.spinner", runs);
    s.to(sc, "session.glow", runs, 0.4, 0.5);
    s.swap_status(sc, "session", turn.at("finds a race"), [1, 2], 0.35);
    s.swap_status(sc, "session", turn.at("adds a lock"), [2, 3], 0.35);
    s.swap_status(sc, "session", turn.at("runs them again"), [3, 4], 0.35);

    // Progress goes back quietly; the wire notes swap to the return lanes.
    let progress = turn.at("progress goes back");
    footer_handoff.hide(sc, progress);
    s.to(sc, "camera.x", progress, 0.0, 1.6);
    s.to(sc, "camera.z", progress, 0.0, 1.6);
    for name in ["signal-name", "signal-note", "prompt-name", "prompt-note"] {
        s.fade_out(sc, name, progress, 0.3);
    }
    let relayed = s.send(sc, "progress", progress + millis(300), 0.8);
    s.send(sc, "quiet", reply_after(relayed), 0.8);
    s.fade_in(sc, "quiet-name", turn.at("quietly"), 1.0, 0.4);
    sfx::TICK.play(sc, "quiet", reply_after(relayed), -22.0);

    // The result goes back to be spoken; the voice agent says it its way.
    let result = turn.at("the result");
    let done = s.resolve_spinner(sc, "session", runs, result);
    s.swap_status(
        sc,
        "session",
        done.saturating_sub(millis(250)),
        [4, 5],
        0.35,
    );
    sfx::MARK.play(sc, "done", done, -19.0);
    s.to(sc, "session.glow", done, 0.0, 0.6);
    let relayed = s.send(sc, "result", done + millis(100), 0.8);
    let spoken_at = turn.at("be spoken");
    s.swap_labels(sc, ["quiet-name", "spoken-name"], spoken_at, millis(250));
    let spoken = s.send(sc, "spoken", reply_after(relayed).max(spoken_at), 0.8);
    s.hit(sc, "voice.pulse", spoken, 0.75, 0.0);
    sfx::SEND.play(sc, "spoken", spoken, -13.0);
    let own = turn.at("own words");
    s.send(sc, "back", own.saturating_sub(millis(500)), 0.85);
    heard_result.type_in(sc, own, 50.0, 0.6);

    // -------------------------------------------------------------- replies
    // Two replies: what you heard above, what happened below.
    let two_replies = replies.at("two replies");
    chip_call.hide(sc, two_replies);
    let mut chip_replies = chip(sc, "chip-replies", Tone::Accent, "two replies")?;
    chip_replies.show(sc, two_replies + millis(250));
    s.fade_out(sc, "spoken-name", two_replies, 0.4);
    let heard = replies.at("what I heard");
    heard_title.hide(sc, heard);
    let mut heard_title_2 = line(
        sc,
        "heard-title-2",
        [TEXT_X, HEARD_Y[0]],
        19.0,
        vec![
            span("what you heard", Tone::Accent),
            span("  fast, paraphrased", Tone::Muted),
        ],
    )?;
    heard_title_2.type_in(sc, heard + millis(150), 40.0, 0.4);
    let happened = replies.at("what happened");
    let mut happened_title = line(
        sc,
        "happened-title",
        [TEXT_X, HAPPENED_Y[0]],
        19.0,
        vec![
            span("what happened", Tone::Plain),
            span("  the session’s real reply", Tone::Muted),
        ],
    )?;
    happened_title.type_in(sc, happened, 40.0, 0.4);
    let indent = "         ";
    let mut happened_reply = line(
        sc,
        "happened-reply",
        [TEXT_X, HAPPENED_Y[1]],
        23.0,
        vec![
            span("session  ", Tone::Muted),
            span(
                "“I serialized retries with an actor lock in UploadQueue.swift;",
                Tone::Plain,
            ),
        ],
    )?;
    let typed = happened_reply.type_in(sc, happened + millis(500), 70.0, 0.0);
    let mut happened_pass = line(
        sc,
        "happened-pass",
        [TEXT_X, HAPPENED_Y[2]],
        23.0,
        vec![
            span(indent, Tone::Muted),
            span("UploadTests passes (12/12).", Tone::Plain),
        ],
    )?;
    happened_pass.type_in(sc, typed, 60.0, 0.2);
    let mut happened_flaky = line(
        sc,
        "happened-flaky",
        [TEXT_X, HAPPENED_Y[3]],
        23.0,
        vec![
            span(indent, Tone::Muted),
            span("testCancelMidUpload is still flaky on CI.", Tone::Warning),
            span("”", Tone::Plain),
        ],
    )?;
    happened_flaky.type_in(sc, replies.at("still flaky"), 40.0, 0.6);
    // The lines that differ: you heard a pass; the session wrote a flake.
    heard_you.hide(sc, replies.at("can differ"));
    heard_ack.hide(sc, replies.at("can differ") + millis(100));
    let rise = heard_result.channel(sc, "y", 0.0);
    sc.spring(
        &rise,
        replies.at("can differ") + millis(350),
        HEARD_Y[1] - HEARD_Y[3],
        0.7,
        0.0,
    );
    s.hit(sc, "session.flash", replies.at("still flaky"), 0.5, 0.0);
    let mut footer_replies = footer(
        sc,
        "footer-replies",
        vec![
            span("what you heard", Tone::Accent),
            span(" can differ from ", Tone::Muted),
            span("what happened", Tone::Plain),
        ],
    )?;
    footer_replies.type_in(sc, replies.at("tests pass now"), 44.0, 0.8);

    // --------------------------------------------------------------- opaque
    let question = opaque.at("open question");
    footer_replies.hide(sc, question);
    chip_replies.hide(sc, question);
    let mut chip_opaque = chip(sc, "chip-opaque", Tone::Muted, "opaque")?;
    let opaque_at = opaque.at("is opaque");
    chip_opaque.show(sc, opaque_at);

    // The session opens into its phone: the card gives way as the phone
    // rises into its place.
    let phone_at = opaque_at + millis(200);
    let phone_opaque = FootageActor::declare(
        sc,
        "phone-opaque",
        &FootagePlan::new(Clip::new("phone-opaque"), PHONE, PHONE_SIZE).fit(Fit::Contain),
        footage::still(
            "phone-opaque",
            "assets/opaque-phone.png",
            0,
            reading.duration(),
        ),
    )?;
    phone_opaque.fly_in(sc, phone_at);
    s.fade_out(sc, "session", phone_at + millis(250), 0.4);
    for name in ["session-name", "session-role"] {
        s.fade_out(sc, name, phone_at, 0.3);
    }

    // The voice agent and the delegation go behind one panel.
    let hidden = opaque.at("hidden");
    s.channel(sc, "curtain.opacity", 0.0);
    s.channel(sc, "curtain.y", -36.0);
    s.channel(sc, "curtain.scale", 1.03);
    s.to(sc, "curtain.opacity", hidden, 1.0, 0.45);
    s.bounce(sc, "curtain.y", hidden, 0.0, 0.6, 0.12);
    s.to(sc, "curtain.scale", hidden, 1.0, 0.6);
    sfx::WHOOSH.play(sc, "curtain-down", hidden, -22.0);
    s.fade_in(sc, "curtain-name", opaque.at("one assistant"), 1.0, 0.5);
    s.fade_in(
        sc,
        "curtain-note",
        opaque.at("one assistant") + millis(400),
        1.0,
        0.5,
    );

    // Your words appear as your own message.
    let anchor = |id: &str, source: [f32; 2]| CalloutAnchorPlan::Point {
        id: id.into(),
        at: on_phone(source),
        side: None,
    };
    let mut lens = LensActor::declare(
        sc,
        "lens",
        &LensPlan::capsule(anchor("opaque-message", [567.0, 401.0]), [500.0, 190.0])
            .anchor(anchor("opaque-reply", [465.0, 942.0]))
            .anchor(anchor("voice-message", [567.0, 379.0]))
            .anchor(anchor("voice-reply", [465.0, 897.0]))
            .magnification(1.3),
    )?;
    let own_messages = opaque.at("own messages");
    lens.show(sc, own_messages);
    lens.hide(sc, opaque.at("nicest ideal") + millis(600));

    // It leaks a little: the reply runs behind the panel, and its light
    // shows through as the panel thins at the seams.
    let leaks = opaque.at("leaks a little");
    let relayed = s.send(sc, "result-2", leaks.saturating_sub(millis(600)), 0.8);
    let spoken = s.send(sc, "spoken-2", reply_after(relayed), 0.8);
    s.hit(sc, "voice.pulse", spoken, 0.9, 0.0);
    s.ease(sc, "curtain.fill", leaks, 0.86, 0.9, Ease::Smootherstep);
    s.send(sc, "back-2", reply_after(spoken), 0.85);
    let mut footer_opaque = footer(
        sc,
        "footer-opaque",
        vec![
            span("opaque: ", Tone::Plain),
            span("one assistant, but the seams show", Tone::Muted),
        ],
    )?;
    footer_opaque.type_in(sc, leaks, 44.0, 0.8);
    let seams = opaque.at("from the seams");
    s.hit(sc, "curtain.emphasis", seams, 1.0, 0.0);
    s.ease(sc, "curtain.fill", seams, 0.78, 0.8, Ease::Smootherstep);
    s.hit(
        sc,
        "voice.pulse",
        opaque.at("delegation agent behind"),
        0.8,
        0.0,
    );

    // What I heard didn't match what was written.
    let mismatch = opaque.at("match what");
    lens.move_to(sc, "opaque-reply", mismatch.saturating_sub(millis(500)))?;
    lens.resize(sc, [540.0, 104.0], mismatch.saturating_sub(millis(500)));
    lens.magnify(sc, 1.1, mismatch.saturating_sub(millis(500)));
    lens.show(sc, mismatch.saturating_sub(millis(500)));

    // ---------------------------------------------------------- transparent
    // The panel lifts: two actors in plain view, and the same phone now
    // shows the voice agent's message.
    let lift = transparent.at("is transparent");
    lens.hide(sc, lift.saturating_sub(millis(700)));
    footer_opaque.hide(sc, lift.saturating_sub(millis(300)));
    chip_opaque.hide(sc, lift.saturating_sub(millis(300)));
    let mut chip_transparent = chip(sc, "chip-transparent", Tone::Success, "transparent")?;
    chip_transparent.show(sc, lift);
    s.to(sc, "curtain.y", lift, -70.0, 0.8);
    s.to(sc, "curtain.opacity", lift, 0.0, 0.6);
    s.to(sc, "curtain-name.opacity", lift, 0.0, 0.3);
    s.to(sc, "curtain-note.opacity", lift, 0.0, 0.3);
    sfx::WHOOSH.play(sc, "curtain-up", lift, -22.0);
    s.hit(sc, "voice.pulse", lift + millis(250), 0.6, 0.0);
    let phone_transparent = FootageActor::declare(
        sc,
        "phone-transparent",
        &FootagePlan::new(Clip::new("phone-transparent"), PHONE, PHONE_SIZE).fit(Fit::Contain),
        footage::still(
            "phone-transparent",
            "assets/transparent-phone.png",
            0,
            reading.duration(),
        ),
    )?;
    let swap = lift + millis(400);
    let incoming = phone_transparent.channel(sc, "opacity");
    sc.set(&incoming, 0, 0.0);
    sc.ease(&incoming, swap, 1.0, 0.5, Ease::Smootherstep);
    let outgoing = phone_opaque.channel(sc, "opacity");
    sc.ease(&outgoing, swap + millis(450), 0.0, 0.2, Ease::Linear);

    // The voice agent prompts the session; its message lands in the phone.
    let delegates = transparent.at("delegates to this session");
    let signal = s.send(sc, "signal-2", delegates, 0.8);
    sfx::SEND.play(sc, "signal-2", delegates, -14.0);
    s.land(sc, "app", signal);
    let landed = s.send(sc, "voice-message", reply_after(signal), 0.85);
    sfx::SEND.play(sc, "voice-message", reply_after(signal), -12.0);
    let voice_message = transparent.at("voice message").max(landed);
    lens.move_to(
        sc,
        "voice-message",
        voice_message.saturating_sub(millis(300)),
    )?;
    lens.resize(
        sc,
        [500.0, 180.0],
        voice_message.saturating_sub(millis(300)),
    );
    lens.magnify(sc, 1.3, voice_message.saturating_sub(millis(300)));
    lens.show(sc, voice_message);
    sfx::CONFIRM.play(sc, "voice-message-lands", voice_message, -20.0);

    // Then the session's real reply.
    let reply = transparent.at("real reply");
    lens.move_to(sc, "voice-reply", reply.saturating_sub(millis(400)))?;
    lens.resize(sc, [540.0, 104.0], reply.saturating_sub(millis(400)));
    lens.magnify(sc, 1.1, reply.saturating_sub(millis(400)));
    let sense = transparent.at("makes sense");
    lens.hide(sc, sense.saturating_sub(millis(300)));
    let mut footer_transparent = footer(
        sc,
        "footer-transparent",
        vec![
            span("transparent: ", Tone::Success),
            span("two actors, so the difference makes sense", Tone::Muted),
        ],
    )?;
    footer_transparent.type_in(sc, transparent.at("matches how"), 44.0, 0.8);
    s.hit(sc, "voice.pulse", sense, 0.5, 0.0);

    scene.finish().context("call")
}

fn choices(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.8), [("outro", seconds(2.2))])?;
    let mut scene = PlanBuilder::new("choices", reading.duration());
    let [spoken] = reading.place(&mut scene);
    let sc = &mut scene;
    let mut title = header(sc, "opencode ios", "voice mode")?;
    title.show(sc, 0);

    let mut list = ChecklistActor::declare(
        sc,
        "choices",
        &ChecklistPlan::new([440.0, 300.0], 1040.0)
            .size(34.0)
            .title("open choices")
            .rail()
            .panel()
            .item(
                ChecklistItemPlan::new("display", "what the session shows")
                    .result("transparent first"),
            )
            .item(ChecklistItemPlan::new("delivery", "steer or queue"))
            .item(ChecklistItemPlan::new(
                "scope",
                "one session per call, or a coordinator",
            ))
            .item(ChecklistItemPlan::new(
                "saved",
                "what gets saved in the session",
            ))
            .item(ChecklistItemPlan::new(
                "runtime",
                "the phone, or the OpenCode server",
            )),
    )?;
    list.show(sc, seconds(0.4));
    list.reveal_item(sc, "display", spoken.at("part of the design"))?;
    list.start(sc, "display", spoken.at("experiment"))?;
    let starting = spoken.at("starting with transparent");
    let marked = list.resolve(sc, "display", starting, Outcome::Done)?;
    sfx::MARK.play(sc, "display", marked.saturating_sub(millis(200)), -18.0);
    for (item, phrase) in [
        ("delivery", "steer or queue"),
        ("scope", "coordinator"),
        ("saved", "gets saved"),
        ("runtime", "on the phone"),
    ] {
        let at = spoken.at(phrase);
        list.reveal_item(sc, item, at)?;
        sfx::TICK.play(sc, format!("reveal-{item}"), at, -24.0);
    }
    let mut closing = footer(
        sc,
        "footer-choices",
        vec![
            span("starting with ", Tone::Muted),
            span("transparent", Tone::Success),
        ],
    )?;
    closing.type_in(sc, starting, 40.0, 0.8);
    scene.finish().context("choices")
}

#[cfg(test)]
mod tests {
    use super::*;
    use psychopomp::{
        math::{Vec3, vec2},
        stage::Camera,
    };

    #[test]
    fn the_agents_fit_every_camera_composition() {
        for position in [
            [0.0, 0.0, -160.0],
            [-50.0, 0.0, 40.0],
            [80.0, 0.0, 40.0],
            [-40.0, 0.0, 30.0],
            [60.0, 0.0, 30.0],
            [80.0, 0.0, 50.0],
            [0.0, 0.0, 0.0],
        ] {
            let camera = Camera::at(Vec3::from(position), vec2(1920.0, 1080.0));
            for element in stage_plan().elements {
                if let StageElement::Card { id, at, size, .. } = element {
                    let (center, scale) = camera.project(Vec3::from(at)).unwrap();
                    let half = vec2(size[0], size[1]) * (0.5 * scale);
                    let (low, high) = (center - half, center + half);
                    assert!(
                        low.x > 60.0 && high.x < 1860.0 && low.y > 330.0 && high.y < 800.0,
                        "{id} leaves the stage band at camera {position:?}: {low:?}..{high:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_phone_covers_the_session_card() {
        let [x, y] = PHONE;
        let half = [PHONE_SIZE[0] * 0.5, PHONE_SIZE[1] * 0.5];
        assert!(x - half[0] <= SESSION[0] - SESSION_SIZE[0] * 0.5 + 4.0);
        assert!(x + half[0] >= SESSION[0] + SESSION_SIZE[0] * 0.5 - 4.0);
        assert!(y - half[1] < SESSION[1] - SESSION_SIZE[1] * 0.5);
        let shown = PHONE_SOURCE[0] * PHONE_SIZE[1] / PHONE_SOURCE[1];
        assert!(
            (shown - PHONE_SIZE[0]).abs() < 1.0,
            "the phone keeps its aspect"
        );
    }

    #[test]
    fn phone_points_map_into_the_phone() {
        let top_left = on_phone([0.0, 0.0]);
        let bottom_right = on_phone(PHONE_SOURCE);
        assert!((top_left[1] - (PHONE[1] - PHONE_SIZE[1] * 0.5)).abs() < 0.01);
        assert!((bottom_right[0] - top_left[0] - PHONE_SIZE[0]).abs() < 1.0);
    }
}
