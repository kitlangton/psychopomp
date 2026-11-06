//! Beats 6-11: watches, policy, conversations, the cases, the tables, the ask.
use anyhow::Result;
use psychopomp::{
    author::millis,
    caption::CaptionAlign,
    kit::{
        broadsheet::{self, COBALT, INK, PAPER, SIGNAL, VERMILION},
        code::{CodeBlock, LineMark},
        diagram, field, ide,
        moves::{arrive, dim, hide, on, out, pop, show, slide, stamp},
        scale,
    },
    paint::Paint,
    plan::ScenePlan,
    sfx,
    stage::{Move, StageActor, StageElement, Waypoint},
    syntax::Lang,
    tone::Tone,
};
use psychopomp_media::Audio;

use crate::look::{TAIL, begin, centred, left, mono_aligned};

const NODE: &str = "#1C2230";

/// Beat 6: a watch hears a thread and never answers.
pub fn watch(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "06-watch", audio, TAIL);
    let ink = Paint::hex(INK);
    let mut elements = vec![field::field("cobalt", Paint::hex(COBALT))];
    elements.extend(diagram::node(
        "thread",
        [540.0, 430.0],
        [620.0, 200.0],
        Paint::hex(PAPER),
        broadsheet::shout("#admin thread", 84.0, ink),
    ));
    elements.extend(diagram::node(
        "laptop",
        [1400.0, 430.0],
        [520.0, 200.0],
        Paint::hex(PAPER),
        broadsheet::shout("Kit's laptop", 84.0, ink),
    ));
    elements.push(diagram::edge("hear", "thread", "laptop", Tone::Warning).stroke_width(7.0));
    elements.push(diagram::packet("hear-p", "hear", Tone::Warning));
    elements.push(
        diagram::route(
            "answer",
            "laptop",
            &[[1400.0, 660.0], [540.0, 660.0]],
            "thread",
            Tone::Muted,
        )
        .dash([18.0, 14.0])
        .stroke_width(6.0),
    );
    elements.push(centred(
        "never",
        [970.0, 660.0],
        broadsheet::shout("never", 96.0, Paint::hex(SIGNAL)),
    ));
    elements.push(mono_aligned(
        "law",
        [960.0, 860.0],
        40.0,
        &[("deliveries(e) = responder(e) ∪ matching(watches, e)", PAPER)],
        CaptionAlign::Center,
    ));
    elements.push(centred(
        "dm",
        [960.0, 960.0],
        kit.caption(
            "DMs always go to the default agent",
            scale::BODY,
            Paint::hex(SIGNAL),
        ),
    ));
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    hide(
        &mut s,
        sc,
        &["laptop", "laptop-title", "never", "law", "dm", "answer"],
    );
    diagram::ink(&mut s, sc, &["hear", "answer"]);

    pop(&mut s, sc, "thread", millis(100));
    pop(&mut s, sc, "thread-title", millis(100));
    show(&mut s, sc, "law", on(&spoken, "listens"));
    let laptop = on(&spoken, "laptop");
    slide(&mut s, sc, "laptop", laptop, 120.0);
    slide(&mut s, sc, "laptop-title", laptop, 120.0);
    let hear = on(&spoken, "hear");
    s.connect(sc, "hear", hear.saturating_sub(millis(150)), 0.4);
    let landed = s.send(sc, "hear-p", hear + millis(250), 0.7);
    sfx::TICK.play(sc, "tick-hear", landed, -18.0);
    let never = on(&spoken, "never");
    show(&mut s, sc, "answer", never.saturating_sub(millis(400)));
    s.connect(sc, "answer", never.saturating_sub(millis(400)), 0.5);
    dim(&mut s, sc, "answer", never + millis(200), 0.35);
    stamp(&mut s, sc, "never", never);
    sfx::MARK.play(sc, "mark-never", never, -17.0);
    arrive(&mut s, sc, "dm", on(&spoken, "direct"));

    scene.cue("watch", 0, duration);
    Ok(scene.finish()?)
}

const JOB: &str = r#"const admin: Agent = {
  account: "svc_admin",
  instructions: "Triage requests in #admin",
  model: "sonnet",
  tools: ["repo", "docs", "deploy", "web"],
  permissions: { "repo:write": "ask" },
}"#;

const MEET: &str = r#"type Verdict = "allow" | "ask" | "deny"
effective = meet(        // stricter wins
  agent.permissions,
  route.narrow,
  audience(place),       // "internal"
  workspace.controls,
)"#;

/// Beat 7: an agent is a job, and every policy factor can only take away.
pub fn policy(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "07-policy", audio, TAIL);
    let job = CodeBlock::new("job", Lang::TypeScript, JOB)
        .theme(ide::THEME)
        .size(58.0)
        .at([150.0, 540.0 - 3.0 * 58.0 * 1.55]);
    let meet = CodeBlock::new("meet", Lang::TypeScript, MEET)
        .theme(ide::THEME)
        .size(52.0)
        .at([150.0, 150.0]);
    let tools = [
        ("t-read", "repo:read", 330.0),
        ("t-write", "repo:write", 645.0),
        ("t-docs", "internal-docs", 960.0),
        ("t-deploy", "deploy", 1275.0),
        ("t-web", "web", 1590.0),
    ];
    let mut elements = job.elements();
    elements.extend(meet.elements());
    elements.push(mono_aligned(
        "guests",
        [150.0 + 39.0 * 0.6 * 52.0, meet.line_y(4)],
        52.0,
        &[("→ \"guests\"", "#FF6B6B")],
        CaptionAlign::Left,
    ));
    for (id, name, x) in tools {
        elements.extend(diagram::node(
            id,
            [x, 870.0],
            [290.0, 100.0],
            Paint::hex(NODE),
            kit.caption(name, scale::LABEL, Paint::hex(ide::INK)),
        ));
        elements.push(
            StageElement::path(
                &format!("{id}-x"),
                vec![
                    Waypoint::Point([x - 130.0, 870.0, 0.0]),
                    Waypoint::Point([x + 130.0, 870.0, 0.0]),
                ],
            )
            .tone(Tone::Error)
            .stroke_width(5.0),
        );
    }
    elements.push(centred(
        "deny",
        [960.0, 1000.0],
        kit.headline("DENY", 64.0, Paint::hex("#FF6B6B")),
    ));
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    // The job: its fields arrive with the words that name them.
    job.hide(&mut s, sc);
    job.reveal_lines(&mut s, sc, 0..1, millis(100), 0);
    job.reveal_lines(&mut s, sc, 6..7, millis(100), 0);
    for (line, word) in [
        (1, "service"),
        (2, "instructions"),
        (3, "model"),
        (4, "tools"),
        (5, "permissions"),
    ] {
        job.reveal_lines(&mut s, sc, line..line + 1, on(&spoken, word), 0);
    }
    let combine = on(&spoken, "combine");
    job.leave(&mut s, sc, combine.saturating_sub(millis(700)), 0.25);

    // The meet: each factor names a tool it takes away.
    meet.hide(&mut s, sc);
    meet.reveal(&mut s, sc, combine.saturating_sub(millis(450)), millis(40));
    let mut hidden = vec!["guests", "deny"];
    let ids = tools.map(|(id, ..)| [id.to_owned(), format!("{id}-title")]);
    for pair in &ids {
        hidden.extend(pair.iter().map(String::as_str));
    }
    let strikes = tools.map(|(id, ..)| format!("{id}-x"));
    hidden.extend(strikes.iter().map(String::as_str));
    hide(&mut s, sc, &hidden);
    for (index, pair) in ids.iter().enumerate() {
        for id in pair {
            pop(
                &mut s,
                sc,
                id,
                combine.saturating_sub(millis(300)) + millis(60 * index as u64),
            );
        }
    }
    let knock =
        |s: &mut StageActor, sc: &mut psychopomp::author::PlanBuilder, tool: &str, at: u64| {
            show(s, sc, &format!("{tool}-x"), at);
            s.connect(sc, &format!("{tool}-x"), at, 0.25);
            dim(s, sc, tool, at + millis(100), 0.3);
            dim(s, sc, &format!("{tool}-title"), at + millis(100), 0.3);
            sfx::TICK.play(sc, format!("tick-{tool}"), at, -19.0);
        };
    let route = on(&spoken, "route");
    meet.focus(&mut s, sc, 3..4, route, 0.3);
    knock(&mut s, sc, "t-deploy", route + millis(200));
    meet.focus(&mut s, sc, 4..5, on(&spoken, "who's"), 0.3);
    let controls = on(&spoken, "workspace's");
    meet.focus(&mut s, sc, 5..6, controls, 0.3);
    knock(&mut s, sc, "t-write", controls + millis(400));
    meet.focus(&mut s, sc, 0..1, on(&spoken, "each"), 0.35);

    let guest = on(&spoken, "guest");
    meet.focus(&mut s, sc, 4..5, guest, 0.3);
    stamp(&mut s, sc, "guests", on(&spoken, "joins"));
    let internal = on(&spoken, "internal");
    knock(&mut s, sc, "t-docs", internal);
    stamp(&mut s, sc, "deny", internal + millis(150));
    sfx::FAILURE.play(sc, "fail-docs", internal + millis(150), -22.0);

    scene.cue("policy", 0, duration);
    Ok(scene.finish()?)
}

const CONVERSATION: &str = r#"type Conversation = {
  key: Address          // "slack:T1:C7:1730.1"
  agent: AgentID        // pinned at creation
  inbox: Queue<Event>   // one turn at a time
  exposed: boolean      // false → true, never back
  executor: Cloud | Local
}"#;

/// Beat 8: one conversation per thread, and the flag that never clears.
pub fn conversation(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "08-conversation", audio, TAIL);
    let code = CodeBlock::new("conv", Lang::TypeScript, CONVERSATION)
        .theme(broadsheet::THEME)
        .size(46.0)
        .at([140.0, 450.0]);
    let words = [
        ("w-key", "key = thread", "conversation", 1..2),
        ("w-agent", "agent pinned", "keeps", 2..3),
        ("w-turn", "1 turn at a time", "runs", 3..4),
        ("w-dupe", "retries × 2 → 1", "drops", 3..4),
        ("w-exposed", "exposed: true", "seen", 4..5),
        ("w-shared", "#shared: refused", "won't", 4..5),
    ];
    let mut elements = code.elements();
    for (id, text, ..) in &words {
        let paint = if *id == "w-shared" {
            Paint::hex(VERMILION)
        } else {
            Paint::hex(INK)
        };
        elements.push(left(
            id,
            [140.0, 230.0],
            broadsheet::shout(text, 150.0, paint),
        ));
    }
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    code.hide(&mut s, sc);
    code.reveal(&mut s, sc, millis(100), millis(50));
    let ids = words.each_ref().map(|(id, ..)| *id);
    hide(&mut s, sc, &ids);
    for (index, (id, _, word, lines)) in words.iter().enumerate() {
        let at = on(&spoken, word);
        if index > 0 {
            out(
                &mut s,
                sc,
                words[index - 1].0,
                at.saturating_sub(millis(60)),
            );
        }
        slide(&mut s, sc, id, at, -60.0);
        code.focus(&mut s, sc, lines.clone(), at, 0.3);
        sfx::TICK.play(sc, format!("tick-{id}"), at, -19.0);
    }
    sfx::MARK.play(sc, "mark-shared", on(&spoken, "won't"), -17.0);

    scene.cue("conversation", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 9: every case maps onto the pieces.
pub fn cases(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "09-cases", audio, 900_000_000);
    let paper = Paint::hex(PAPER);
    let ink = Paint::hex(INK);
    let cases = [
        (
            "k-slack",
            "Slack bot",
            "connector + route",
            150.0,
            330.0,
            "pieces",
        ),
        (
            "k-discord",
            "Discord · Teams",
            "connectors",
            1010.0,
            330.0,
            "Discord",
        ),
        ("k-admin", "#admin", "a route", 150.0, 590.0, "admin"),
        ("k-cron", "Schedule", "a clock", 1010.0, 590.0, "schedule"),
        (
            "k-api",
            "/v1 API",
            "a caller bound to an agent",
            150.0,
            850.0,
            "public",
        ),
        (
            "k-laptop",
            "Kit's laptop",
            "a watch",
            1010.0,
            850.0,
            "laptop",
        ),
    ];
    let mut elements = vec![field::field("vermilion", Paint::hex(VERMILION))];
    elements.push(left(
        "head",
        [150.0, 140.0],
        kit.caption("7 cases, 0 new pieces", scale::BODY, ink),
    ));
    for (id, title, note, x, y, _) in cases {
        elements.push(left(
            &format!("{id}-t"),
            [x, y],
            broadsheet::shout(title, scale::HEADLINE, paper),
        ));
        elements.push(left(
            &format!("{id}-n"),
            [x, y + 88.0],
            kit.caption(note, 44.0, ink),
        ));
    }
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    hide(&mut s, sc, &["head"]);
    arrive(&mut s, sc, "head", millis(100));
    for (id, _, _, x, _, word) in cases {
        let at = on(&spoken, word);
        let dx = if x < 960.0 { -80.0 } else { 80.0 };
        hide(&mut s, sc, &[&format!("{id}-t"), &format!("{id}-n")]);
        slide(&mut s, sc, &format!("{id}-t"), at, dx);
        arrive(&mut s, sc, &format!("{id}-n"), at + millis(200));
        sfx::TICK.play(sc, format!("tick-{id}"), at, -19.0);
    }

    scene.cue("cases", 0, duration);
    Ok(scene.finish()?)
}

const TABLES: &str = r#"agent
  audience          internal | external
  serves            internal | external
  sandbox { …, gitServerSlug }
agent_trigger
  external_key      slack:<app>:<team>
                  | slack:<app>:<team>:<channel>
  connection_id
  unique (kind, external_key, enabled_token)  -- unchanged
github_installation
agent_connection_grant
watch
agent_session"#;

/// Beat 10: the PR 4 tables stay; what we'd add, rename, and drop.
pub fn prs(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "10-prs", audio, TAIL);
    let code = CodeBlock::new("tables", Lang::Sql, TABLES)
        .theme(ide::THEME)
        .size(36.0)
        .numbered(1)
        .at([150.0, 540.0 - 6.0 * 36.0 * 1.55])
        .mark(1..2, LineMark::Deleted)
        .mark(2..3, LineMark::Inserted)
        .mark(3..4, LineMark::Deleted)
        .mark(6..8, LineMark::Inserted)
        .mark(9..13, LineMark::Inserted);
    let marked = [1, 2, 3, 6, 7, 9, 10, 11, 12];
    let mut elements = code.elements();
    elements.push(centred(
        "proposed",
        [1660.0, 200.0],
        kit.headline("PROPOSED", 72.0, Paint::hex(ide::AMBER)),
    ));
    elements.push(centred(
        "built",
        [1660.0, 330.0],
        kit.headline("BUILT", 72.0, Paint::hex("#6EE796")),
    ));
    elements.push(mono_aligned(
        "built-what",
        [1660.0, 410.0],
        scale::LABEL,
        &[("agent · agent_trigger", ide::INK)],
        CaptionAlign::Center,
    ));
    elements.push(mono_aligned(
        "built-bot",
        [1660.0, 465.0],
        scale::LABEL,
        &[("agent_preset · the bot", ide::INK)],
        CaptionAlign::Center,
    ));
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();
    let home = cam.pose(sc, 0);

    code.hide(&mut s, sc);
    hide(
        &mut s,
        sc,
        &["proposed", "built", "built-what", "built-bot"],
    );
    // Marks start hidden and come in with their lines.
    for line in marked {
        s.channel(sc, &format!("tables-mark{line}.opacity"), 0.0);
    }
    let unmarked = [0, 4, 5, 8];
    for (index, line) in unmarked.iter().enumerate() {
        code.reveal_lines(
            &mut s,
            sc,
            *line..*line + 1,
            millis(120 + 70 * index as u64),
            0,
        );
    }
    let reveal = |s: &mut StageActor,
                  sc: &mut psychopomp::author::PlanBuilder,
                  lines: std::ops::Range<usize>,
                  at: u64| {
        for line in lines.clone() {
            show(s, sc, &format!("tables-mark{line}"), at);
        }
        code.reveal_lines(s, sc, lines, at, millis(60));
    };

    let channel = on(&spoken, "channel");
    reveal(&mut s, sc, 6..7, channel);
    code.focus(&mut s, sc, 5..7, channel + millis(300), 0.35);
    code.frame(
        &cam,
        sc,
        4..9,
        110.0,
        channel.saturating_sub(millis(200)),
        Move::Glide(1.0),
    )?;
    code.focus(&mut s, sc, 8..9, on(&spoken, "unique"), 0.35);
    let id = on(&spoken, "connection");
    reveal(&mut s, sc, 7..8, id);
    code.focus(&mut s, sc, 7..8, id + millis(150), 0.3);
    let github = on(&spoken, "GitHub");
    code.frame(
        &cam,
        sc,
        7..13,
        110.0,
        github.saturating_sub(millis(300)),
        Move::Glide(1.0),
    )?;
    for (line, word) in [
        (9, "GitHub"),
        (10, "grants"),
        (11, "watches"),
        (12, "sessions"),
    ] {
        let at = on(&spoken, word);
        reveal(&mut s, sc, line..line + 1, at);
        code.focus(&mut s, sc, 9..line + 1, at + millis(100), 0.3);
        sfx::TICK.play(sc, format!("tick-{line}"), at, -19.0);
    }
    let rename = on(&spoken, "rename");
    code.frame(
        &cam,
        sc,
        0..7,
        110.0,
        rename.saturating_sub(millis(300)),
        Move::Glide(1.0),
    )?;
    code.focus(&mut s, sc, 1..2, rename, 0.3);
    reveal(&mut s, sc, 2..3, on(&spoken, "serves"));
    code.focus(&mut s, sc, 1..3, on(&spoken, "serves") + millis(100), 0.3);
    let drop = on(&spoken, "drop");
    reveal(&mut s, sc, 3..4, drop.saturating_sub(millis(200)));
    code.focus(&mut s, sc, 3..4, drop, 0.3);

    let proposed = on(&spoken, "All");
    cam.move_to(
        sc,
        &home,
        proposed.saturating_sub(millis(200)),
        Move::Glide(1.0),
    );
    code.clear(&mut s, sc, proposed, 0.3);
    stamp(&mut s, sc, "proposed", on(&spoken, "proposed"));
    let built = on(&spoken, "Only");
    for line in marked {
        dim(&mut s, sc, &code.line(line), built, 0.25);
        dim(&mut s, sc, &format!("tables-n{line}"), built, 0.25);
        dim(&mut s, sc, &format!("tables-mark{line}"), built, 0.25);
    }
    dim(&mut s, sc, "proposed", built, 0.4);
    stamp(&mut s, sc, "built", built);
    arrive(&mut s, sc, "built-what", built + millis(300));
    arrive(&mut s, sc, "built-bot", built + millis(900));
    sfx::MARK.play(sc, "mark-built", built, -17.0);

    scene.cue("prs", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 11: the ask.
pub fn ask(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "11-ask", audio, 2_200_000_000);
    let paper = Paint::hex(PAPER);
    let elements = vec![
        field::field("cobalt", Paint::hex(COBALT)),
        left(
            "before",
            [150.0, 430.0],
            broadsheet::shout("Before\nPR 5", scale::HERO, paper),
        ),
        left(
            "compare",
            [150.0, 760.0],
            kit.caption("your plan, next to this one", scale::BODY, paper),
        ),
        centred(
            "twelve",
            [1420.0, 430.0],
            broadsheet::shout("12", 480.0, Paint::hex(SIGNAL)),
        ),
        centred(
            "decisions",
            [1420.0, 760.0],
            broadsheet::shout("open decisions", 96.0, paper),
        ),
    ];
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    hide(&mut s, sc, &["before", "compare", "twelve", "decisions"]);
    slide(&mut s, sc, "before", millis(100), -80.0);
    arrive(&mut s, sc, "compare", on(&spoken, "compare"));
    let twelve = on(&spoken, "twelve");
    stamp(&mut s, sc, "twelve", twelve);
    sfx::MARK.play(sc, "mark-twelve", twelve, -16.0);
    arrive(&mut s, sc, "decisions", on(&spoken, "decisions"));

    scene.cue("ask", 0, duration);
    Ok(scene.finish()?)
}
