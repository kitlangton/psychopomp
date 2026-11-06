//! Beats 1-5: today's bot, the idea, the event, connections, and routes.
use anyhow::Result;
use psychopomp::{
    author::millis,
    kit::{
        broadsheet::{self, COBALT, INK, PAPER, SIGNAL, VERMILION},
        code::CodeBlock,
        diagram, field, ide,
        moves::{arrive, dim, hide, on, out, pop, show, slide, stamp},
        scale,
    },
    math::easing::Ease,
    paint::Paint,
    plan::ScenePlan,
    sfx,
    stage::{Arrow, Move, StageActor, StageElement, Waypoint},
    syntax::Lang,
    tone::Tone,
    typography::SplitUnit,
};
use psychopomp_media::Audio;

use crate::look::{TAIL, begin, centred, mono, mono_aligned};

const NODE: &str = "#1C2230";
const PAPER_MUTED: &str = "#9C958A";
const INK_MUTED: &str = "#68625A";

fn point(x: f32, y: f32) -> Waypoint {
    Waypoint::Point([x, y, 0.0])
}

/// Beat 1: one program shaped around Slack, and what that costs.
pub fn today(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "01-today", audio, TAIL);
    let paper = Paint::hex(PAPER);
    let ink = Paint::hex(INK);

    let steps = [
        ("s-check", "CHECK", 330.0, 250.0),
        ("s-wake", "WAKE?", 640.0, 240.0),
        ("s-prompt", "PROMPT", 960.0, 270.0),
        ("s-opencode", "OPENCODE", 1300.0, 330.0),
        ("s-post", "POST", 1610.0, 210.0),
    ];
    let costs = [
        ("c-discord", "DISCORD", 400.0, "a second copy", "Discord"),
        ("c-admin", "#ADMIN", 960.0, "no slot for its agent", "admin"),
        ("c-cron", "SCHEDULES", 1520.0, "", "schedules"),
    ];

    let mut elements = vec![
        field::slab("mono", [160.0, 200.0, 1600.0, 330.0], Paint::hex(INK)),
        mono(
            "mono-path",
            [210.0, 250.0],
            scale::LABEL,
            &[("apps/agent/src/slack/", PAPER_MUTED)],
        ),
        mono_aligned(
            "mono-key",
            [1710.0, 250.0],
            scale::LABEL,
            &[("keyed ", PAPER_MUTED), ("appID:teamID", SIGNAL)],
            psychopomp::caption::CaptionAlign::Right,
        ),
    ];
    for (id, title, x, width) in steps {
        elements.extend(diagram::node(
            id,
            [x, 400.0],
            [width, 120.0],
            Paint::hex("#262626"),
            broadsheet::shout(title, 64.0, paper),
        ));
    }
    for pair in steps.windows(2) {
        elements.push(diagram::edge(
            &format!("e-{}", pair[0].0),
            pair[0].0,
            pair[1].0,
            Tone::Accent,
        ));
    }
    for (id, title, x, note, _) in costs {
        elements.push(
            StageElement::path(
                &format!("{id}-hang"),
                vec![point(x, 534.0), point(x, 672.0)],
            )
            .tone(Tone::Accent)
            .stroke_width(diagram::EDGE_WIDTH)
            .arrow(Arrow::End),
        );
        elements.push(centred(
            &format!("{id}-title"),
            [x, 760.0],
            broadsheet::shout(title, 112.0, ink),
        ));
        if !note.is_empty() {
            elements.push(centred(
                &format!("{id}-note"),
                [x, 860.0],
                kit.caption(note, scale::BODY, Paint::hex(INK_MUTED)),
            ));
        }
    }
    elements.push(mono_aligned(
        "c-cron-note",
        [1520.0, 860.0],
        scale::LABEL,
        &[("slackChannel", "#B3261E"), (": string", INK)],
        psychopomp::caption::CaptionAlign::Center,
    ));
    elements.push(mono_aligned(
        "c-cron-file",
        [1520.0, 920.0],
        scale::LABEL,
        &[("schema/src/agent.ts:119", INK_MUTED)],
        psychopomp::caption::CaptionAlign::Center,
    ));
    // The title room sits over the paper room until "Today".
    elements.push(field::field("cobalt", Paint::hex(COBALT)));
    elements.push(centred(
        "title",
        [960.0, 450.0],
        broadsheet::shout("Composable\nagents", scale::HERO, paper)
            .split(SplitUnit::Line)
            .masked(true),
    ));
    elements.push(centred(
        "sub",
        [960.0, 760.0],
        kit.caption(
            "for Adam · built on the PR 4 tables",
            scale::BODY,
            Paint::hex(SIGNAL),
        ),
    ));

    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();
    let home = cam.pose(sc, 0);
    let mut edges = steps
        .windows(2)
        .map(|pair| format!("e-{}", pair[0].0))
        .collect::<Vec<_>>();
    edges.extend(costs.map(|(id, ..)| format!("{id}-hang")));
    diagram::ink(
        &mut s,
        sc,
        &edges.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    s.rise_type(
        sc,
        "title",
        millis(150),
        millis(140),
        240.0,
        psychopomp::plan::SpringPlan::ENTER,
    );
    // The paper room waits under the title room, empty, until the cut.
    hide(&mut s, sc, &["sub", "mono", "mono-path", "mono-key"]);
    arrive(&mut s, sc, "sub", on(&spoken, "built on"));

    let today = on(&spoken, "Today");
    out(&mut s, sc, "title", today.saturating_sub(millis(250)));
    out(&mut s, sc, "sub", today.saturating_sub(millis(250)));
    cam.frame(
        sc,
        &["s-check", "s-wake"],
        150.0,
        today.saturating_sub(millis(40)),
        Move::Cut,
    )?;
    field::drain(&mut s, sc, "cobalt", today.saturating_sub(millis(30)), 0.18);
    for id in ["mono", "mono-path", "mono-key"] {
        s.ease(
            sc,
            &format!("{id}.opacity"),
            today.saturating_sub(millis(40)),
            1.0,
            0.05,
            Ease::ARRIVE,
        );
    }
    sfx::MARK.play(sc, "mark-today", today, -18.0);

    // The pipeline builds step by step while the camera rides along it.
    let mut ids = Vec::new();
    for (id, ..) in steps {
        ids.push(id.to_owned());
        ids.push(format!("{id}-title"));
    }
    let id_refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
    hide(&mut s, sc, &id_refs);
    for (index, (id, ..)) in steps.iter().enumerate() {
        let at = today + millis(150 + 330 * index as u64);
        pop(&mut s, sc, id, at);
        pop(&mut s, sc, &format!("{id}-title"), at);
        if index > 0 {
            s.connect(
                sc,
                &format!("e-{}", steps[index - 1].0),
                at.saturating_sub(millis(200)),
                0.3,
            );
        }
    }
    cam.frame(
        sc,
        &["s-prompt", "s-opencode", "s-post"],
        140.0,
        today + millis(500),
        Move::Glide(1.8),
    )?;

    let discord = on(&spoken, "Discord");
    cam.move_to(
        sc,
        &home,
        discord.saturating_sub(millis(500)),
        Move::Glide(1.0),
    );
    for (id, _, _, note, word) in costs {
        let at = on(&spoken, word);
        s.connect(
            sc,
            &format!("{id}-hang"),
            at.saturating_sub(millis(250)),
            0.3,
        );
        hide(&mut s, sc, &[&format!("{id}-title")]);
        stamp(&mut s, sc, &format!("{id}-title"), at);
        sfx::TICK.play(sc, format!("tick-{id}"), at, -18.0);
        if !note.is_empty() {
            hide(&mut s, sc, &[&format!("{id}-note")]);
            arrive(&mut s, sc, &format!("{id}-note"), at + millis(250));
        }
    }
    let cron = on(&spoken, "pretend");
    hide(&mut s, sc, &["c-cron-note", "c-cron-file"]);
    arrive(&mut s, sc, "c-cron-note", cron);
    arrive(&mut s, sc, "c-cron-file", cron + millis(120));

    scene.cue("today", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 2: platform code at the edges, nothing in the middle imports Slack.
pub fn idea(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "02-idea", audio, TAIL);
    let ink = Paint::hex(ide::INK);
    let title = |text: &str| kit.headline(text, 44.0, ink);

    let sources = ["slack", "discord", "github", "cron", "api"];
    let mut elements = Vec::new();
    for (index, name) in sources.iter().enumerate() {
        let colour = if index == 0 { ide::INK } else { ide::MUTED };
        elements.push(mono(
            &format!("src-{name}"),
            [110.0, 360.0 + 60.0 * index as f32],
            scale::LABEL,
            &[(name, colour)],
        ));
    }
    elements.push(
        diagram::frame(
            "middle",
            [1250.0, 620.0],
            [1060.0, 520.0],
            Tone::Accent,
            3.0,
        )
        .dash([14.0, 12.0]),
    );
    elements.extend(diagram::node(
        "connector",
        [520.0, 480.0],
        [300.0, 130.0],
        Paint::hex(NODE),
        title("CONNECTOR"),
    ));
    elements.extend(diagram::node(
        "event",
        [880.0, 480.0],
        [230.0, 130.0],
        Paint::hex(NODE),
        title("EVENT"),
    ));
    elements.extend(diagram::node(
        "route",
        [1190.0, 480.0],
        [230.0, 130.0],
        Paint::hex(NODE),
        title("ROUTE"),
    ));
    elements.extend(diagram::node(
        "convo",
        [1560.0, 480.0],
        [400.0, 130.0],
        Paint::hex(NODE),
        title("CONVERSATION"),
    ));
    elements.extend(diagram::node(
        "agent",
        [1560.0, 750.0],
        [300.0, 120.0],
        Paint::hex(NODE),
        title("AGENT"),
    ));
    elements.push(mono_aligned(
        "agent-note",
        [1560.0, 840.0],
        scale::LABEL,
        &[("+ policy", ide::MUTED)],
        psychopomp::caption::CaptionAlign::Center,
    ));
    elements.push(
        StageElement::path(
            "e0",
            vec![point(260.0, 480.0), Waypoint::Element("connector".into())],
        )
        .tone(Tone::Accent)
        .stroke_width(diagram::EDGE_WIDTH)
        .arrow(Arrow::End),
    );
    elements.push(diagram::edge("e1", "connector", "event", Tone::Accent));
    elements.push(diagram::edge("e2", "event", "route", Tone::Accent));
    elements.push(diagram::edge("e3", "route", "convo", Tone::Accent));
    elements.push(diagram::edge("e4", "agent", "convo", Tone::Accent).arrow(Arrow::Both));
    elements.push(diagram::route(
        "reply",
        "convo",
        &[[1560.0, 260.0], [520.0, 260.0]],
        "connector",
        Tone::Success,
    ));
    for edge in ["e0", "e1", "e2", "e3"] {
        elements.push(diagram::packet(&format!("{edge}-p"), edge, Tone::Accent));
    }
    elements.push(diagram::packet("reply-p", "reply", Tone::Success));
    elements.push(centred(
        "middle-title",
        [1250.0, 940.0],
        kit.headline("SLACK-FREE", 64.0, Paint::hex(ide::AMBER)),
    ));

    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();
    let home = cam.pose(sc, 0);

    let mut hidden = vec!["middle", "middle-title", "agent-note"];
    let nodes = ["connector", "event", "route", "convo", "agent"];
    let titles = nodes.map(|node| format!("{node}-title"));
    hidden.extend(nodes);
    hidden.extend(titles.iter().map(String::as_str));
    let src_ids = sources.map(|name| format!("src-{name}"));
    hidden.extend(src_ids.iter().map(String::as_str));
    hide(&mut s, sc, &hidden);

    cam.frame(
        sc,
        &["src-slack", "src-api", "connector"],
        160.0,
        0,
        Move::Cut,
    )?;
    for (index, id) in src_ids.iter().enumerate() {
        slide(&mut s, sc, id, millis(150 + 70 * index as u64), -40.0);
    }
    let pop_node =
        |s: &mut StageActor, sc: &mut psychopomp::author::PlanBuilder, node: &str, at: u64| {
            pop(s, sc, node, at);
            pop(s, sc, &format!("{node}-title"), at);
        };
    pop_node(&mut s, sc, "connector", on(&spoken, "edges"));

    let connector = on(&spoken, "turns");
    s.connect(sc, "e0", connector, 0.35);
    s.send(sc, "e0-p", connector + millis(300), 0.5);
    let event = on(&spoken, "event");
    s.connect(sc, "e1", event.saturating_sub(millis(350)), 0.3);
    s.send(sc, "e1-p", event.saturating_sub(millis(100)), 0.4);
    pop_node(&mut s, sc, "event", event.saturating_sub(millis(150)));
    cam.frame(
        sc,
        &["src-slack", "src-api", "route"],
        110.0,
        event.saturating_sub(millis(600)),
        Move::Glide(1.2),
    )?;

    let route = on(&spoken, "route");
    pop_node(&mut s, sc, "route", route);
    s.connect(sc, "e2", route.saturating_sub(millis(150)), 0.3);
    s.send(sc, "e2-p", route + millis(100), 0.4);
    let agent = on(&spoken, "agent");
    cam.move_to(
        sc,
        &home,
        agent.saturating_sub(millis(300)),
        Move::Glide(1.2),
    );
    pop_node(&mut s, sc, "agent", agent + millis(200));
    show(&mut s, sc, "agent-note", agent + millis(450));
    let convo = on(&spoken, "conversation");
    pop_node(&mut s, sc, "convo", convo.saturating_sub(millis(250)));
    s.connect(sc, "e3", convo.saturating_sub(millis(300)), 0.3);
    s.send(sc, "e3-p", convo, 0.45);
    s.connect(sc, "e4", convo + millis(250), 0.3);

    let reply = on(&spoken, "reply");
    s.connect(sc, "reply", reply, 0.9);
    let landed = s.send(sc, "reply-p", reply + millis(700), 1.1);
    sfx::SEND.play(sc, "send-reply", reply + millis(700), -20.0);
    sfx::TICK.play(sc, "tick-landed", landed, -18.0);

    let middle = on(&spoken, "Nothing");
    show(&mut s, sc, "middle", middle);
    stamp(&mut s, sc, "middle-title", on(&spoken, "imports"));
    for id in [
        "src-slack",
        "src-discord",
        "src-github",
        "src-cron",
        "src-api",
    ] {
        dim(&mut s, sc, id, middle, 0.5);
    }
    sfx::MARK.play(sc, "mark-middle", on(&spoken, "imports"), -17.0);

    scene.cue("idea", 0, duration);
    Ok(scene.finish()?)
}

const EVENT: &str = r#"type Event = {
  source: ConnectionRef | ClockID | ApiBindingID
  place: Place                    // "slack:T1:C7"
  thread?: string
  id: InputID                     // stable across redelivery
  actor: Actor
  body:
    | { tag: "said"; text: string; mentioned: boolean }
    | { tag: "did";  what: "clicked" | "reacted" | ... }
    | { tag: "time"; prompt: string; plannedAt: Instant }
}
// slack:T123:C0ADMIN:1730.1  →  reply goes to the same place"#;

/// Beat 3: every connector produces the same event.
pub fn event(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "03-event", audio, TAIL);
    let code = CodeBlock::new("event", Lang::TypeScript, EVENT)
        .theme(ide::THEME)
        .size(40.0)
        .at([150.0, 540.0 - 5.5 * 40.0 * 1.55]);
    let plan = kit.stage(code.elements());
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();

    code.hide(&mut s, sc);
    code.reveal(&mut s, sc, millis(120), millis(45));

    let said = on(&spoken, "said");
    code.focus(&mut s, sc, 7..8, said, 0.35);
    code.frame(
        &cam,
        sc,
        6..10,
        120.0,
        said.saturating_sub(millis(400)),
        Move::Glide(1.0),
    )?;
    code.focus(&mut s, sc, 8..9, on(&spoken, "pressed"), 0.3);
    code.focus(&mut s, sc, 9..10, on(&spoken, "schedule"), 0.3);
    for (word, index) in [("said", 0), ("pressed", 1), ("schedule", 2)] {
        sfx::TICK.play(sc, format!("tick-{index}"), on(&spoken, word), -19.0);
    }

    let place = on(&spoken, "place");
    code.focus(&mut s, sc, 2..4, place, 0.35);
    code.frame(
        &cam,
        sc,
        1..5,
        140.0,
        place.saturating_sub(millis(300)),
        Move::Glide(1.0),
    )?;

    let reply = on(&spoken, "reply");
    code.focus(&mut s, sc, 11..12, on(&spoken, "also"), 0.35);
    code.frame(
        &cam,
        sc,
        10..12,
        120.0,
        on(&spoken, "also").saturating_sub(millis(300)),
        Move::Glide(1.0),
    )?;
    sfx::MARK.play(sc, "mark-reply", reply, -18.0);

    scene.cue("event", 0, duration);
    Ok(scene.finish()?)
}

const GRANT: &str = r#"type Grant = {
  agent: AgentID
  connection: ConnectionRef
  op: "read" | "write" | "tools"
  resources: string[]
}
mint(connection, agent, { op, resource }): Token | Refused"#;

/// Beat 4: connector vs connection, then grants and minted tokens.
pub fn connection(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "04-connection", audio, TAIL);
    let paper = Paint::hex(PAPER);
    let code = CodeBlock::new("grant", Lang::TypeScript, GRANT)
        .theme(broadsheet::THEME)
        .size(46.0)
        .at([150.0, 200.0]);

    let mut elements = vec![
        field::slab("left", [0.0, 0.0, 960.0, 1080.0], Paint::hex(VERMILION)),
        field::slab("right", [960.0, 0.0, 960.0, 1080.0], Paint::hex(COBALT)),
        centred(
            "connector",
            [480.0, 470.0],
            broadsheet::shout("Connector", 150.0, paper),
        ),
        centred(
            "connector-sub",
            [480.0, 610.0],
            kit.caption("code · one per platform", scale::BODY, paper),
        ),
        mono_aligned(
            "connector-file",
            [480.0, 690.0],
            scale::LABEL,
            &[("apps/agent/src/slack/", "#FFD9CF")],
            psychopomp::caption::CaptionAlign::Center,
        ),
        centred(
            "connection",
            [1440.0, 470.0],
            broadsheet::shout("Connection", 150.0, paper),
        ),
        centred(
            "connection-sub",
            [1440.0, 610.0],
            kit.caption("a row a workspace owns", scale::BODY, paper),
        ),
        mono_aligned(
            "connection-row",
            [1440.0, 690.0],
            scale::LABEL,
            &[("Anomaly · slack · T0ANOMALY", SIGNAL)],
            psychopomp::caption::CaptionAlign::Center,
        ),
        field::slab("sheet", [0.0, 0.0, 1920.0, 1080.0], paper),
    ];
    elements.extend(code.elements());
    let stamps = [
        ("one-repo", "1 repo", 360.0, "repo"),
        ("one-perm", "1 permission", 960.0, "permission"),
        ("one-hour", "≤ 1 hour", 1560.0, "hour"),
    ];
    for (id, text, x, _) in stamps {
        elements.push(centred(
            id,
            [x, 850.0],
            broadsheet::shout(text, scale::HEADLINE, Paint::hex(VERMILION)),
        ));
    }
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    field::wipe(&mut s, sc, "left", -960.0, 0, 0.55);
    hide(
        &mut s,
        sc,
        &[
            "connector",
            "connector-sub",
            "connector-file",
            "connection",
            "connection-sub",
            "connection-row",
        ],
    );
    stamp(&mut s, sc, "connector", millis(300));
    arrive(&mut s, sc, "connector-sub", on(&spoken, "ship"));
    arrive(&mut s, sc, "connector-file", on(&spoken, "platform"));
    let link = on(&spoken, "connection");
    field::wipe(
        &mut s,
        sc,
        "right",
        960.0,
        link.saturating_sub(millis(350)),
        0.55,
    );
    stamp(&mut s, sc, "connection", link);
    arrive(&mut s, sc, "connection-sub", on(&spoken, "workspace's"));
    arrive(&mut s, sc, "connection-row", on(&spoken, "Anomaly's"));
    sfx::TICK.play(sc, "tick-connector", millis(300), -19.0);
    sfx::TICK.play(sc, "tick-connection", link, -19.0);

    // The paper sheet slides up over both halves for grants.
    let grants = on(&spoken, "Agents");
    let y = s.channel(sc, "sheet.y", 1080.0);
    sc.ease(&y, grants, 0.0, 0.55, Ease::TRAVEL);
    // Text paints above shapes at its depth, so the split's words leave as
    // the sheet covers them.
    for id in [
        "connector",
        "connector-sub",
        "connector-file",
        "connection",
        "connection-sub",
        "connection-row",
    ] {
        out(&mut s, sc, id, grants);
    }
    code.hide(&mut s, sc);
    code.reveal_lines(&mut s, sc, 0..6, grants + millis(400), millis(50));
    code.focus(&mut s, sc, 2..5, on(&spoken, "slices"), 0.35);
    let mints = on(&spoken, "mints");
    code.reveal_lines(&mut s, sc, 6..7, mints.saturating_sub(millis(300)), 0);
    code.focus(&mut s, sc, 6..7, mints, 0.35);
    for (id, _, _, word) in stamps {
        hide(&mut s, sc, &[id]);
        let at = on(&spoken, word);
        stamp(&mut s, sc, id, at);
        sfx::TICK.play(sc, format!("tick-{id}"), at, -18.0);
    }

    scene.cue("connection", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 5: the most specific place wins, shown as nested places the camera
/// pulls back through.
pub fn route(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "05-route", audio, TAIL);
    let amber = Paint::hex(ide::AMBER);
    let elements = vec![
        diagram::frame(
            "f-install",
            [960.0, 560.0],
            [1700.0, 840.0],
            Tone::Muted,
            3.0,
        ),
        diagram::frame(
            "f-channel",
            [960.0, 540.0],
            [1160.0, 560.0],
            Tone::Request,
            3.0,
        ),
        diagram::frame(
            "f-thread",
            [960.0, 520.0],
            [620.0, 260.0],
            Tone::Accent,
            3.0,
        ),
        mono(
            "l-install",
            [150.0, 190.0],
            scale::LABEL,
            &[("slack:A1:T1", ide::INK), ("  →  Helper", ide::MUTED)],
        ),
        mono(
            "l-channel",
            [420.0, 310.0],
            scale::LABEL,
            &[("slack:A1:T1:C7", ide::INK), ("  →  Admin", "#5CD4FF")],
        ),
        mono(
            "l-thread",
            [690.0, 440.0],
            scale::LABEL,
            &[("…:C7:1730.1", ide::INK), ("  no route", ide::MUTED)],
        ),
        centred(
            "winner",
            [960.0, 545.0],
            kit.headline("ADMIN", scale::HEADLINE, Paint::hex("#5CD4FF")),
        ),
        centred(
            "paused",
            [960.0, 735.0],
            kit.headline("PAUSED → REFUSES", 56.0, Paint::hex("#FF6B6B")),
        ),
        centred(
            "fall",
            [960.0, 900.0],
            kit.headline("never falls back to Helper", 44.0, Paint::hex(ide::MUTED)),
        ),
        centred(
            "pinned",
            [960.0, 900.0],
            kit.headline("PINNED AT THREAD START", 56.0, amber),
        ),
    ];
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();
    let home = cam.pose(sc, 0);

    hide(
        &mut s,
        sc,
        &[
            "winner",
            "paused",
            "fall",
            "pinned",
            "l-channel",
            "l-install",
        ],
    );
    cam.frame(sc, &["f-thread"], 90.0, 0, Move::Cut)?;
    show(&mut s, sc, "l-thread", 0);

    let thread = on(&spoken, "thread");
    dim(&mut s, sc, "l-thread", thread + millis(200), 0.55);
    let channel = on(&spoken, "channel");
    cam.frame(
        sc,
        &["f-channel"],
        70.0,
        channel.saturating_sub(millis(350)),
        Move::Glide(0.9),
    )?;
    arrive(&mut s, sc, "l-channel", channel);
    stamp(&mut s, sc, "winner", channel + millis(150));
    sfx::MARK.play(sc, "mark-winner", channel + millis(150), -17.0);
    let install = on(&spoken, "install-wide");
    cam.move_to(
        sc,
        &home,
        install.saturating_sub(millis(300)),
        Move::Glide(1.1),
    );
    arrive(&mut s, sc, "l-install", install + millis(250));
    dim(&mut s, sc, "f-install", install + millis(900), 0.6);

    let paused = on(&spoken, "paused");
    stamp(&mut s, sc, "paused", paused);
    dim(&mut s, sc, "winner", paused, 0.35);
    sfx::FAILURE.play(sc, "fail-paused", paused, -22.0);
    arrive(&mut s, sc, "fall", on(&spoken, "falling"));
    let pinned = on(&spoken, "pinned");
    out(&mut s, sc, "fall", pinned.saturating_sub(millis(250)));
    stamp(&mut s, sc, "pinned", pinned);
    out(&mut s, sc, "paused", pinned);
    s.ease(sc, "winner.opacity", pinned, 1.0, 0.35, Ease::TRAVEL);

    scene.cue("route", 0, duration);
    Ok(scene.finish()?)
}
