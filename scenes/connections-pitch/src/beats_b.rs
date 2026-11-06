//! Beats 4-7: the branches PR 4 deletes, what Adam gets, where it stands,
//! and the ask.
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

use crate::look::{TAIL, begin, centred, left, mono, mono_aligned};

const INK_MUTED: &str = "#68625A";
const NODE: &str = "#1C2230";
const GREEN: &str = "#6EE796";
const RED: &str = "#FF6B6B";

/// Excerpt of Console `origin/dev`
/// `packages/core/src/integration-server-oauth.ts:281-287, 662-670`.
const BRANCHES: &str = r#"const rejections = new Set([
  "invalid_grant", "invalid_token",
  "bad_verification_code",
  "bad_refresh_token",
])
if (discovery.issuer === googleIssuer) {
  url.searchParams.set("access_type", "offline")
  url.searchParams.set("prompt", "consent")
}
if (discovery.issuer === entraIssuer)
  url.searchParams.delete("prompt")"#;

/// Beat 4: the issuer branches PR 4 deletes, and the twelve fields they become.
pub fn branches(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "04-branches", audio, TAIL);
    let size = 46.0;
    let code = CodeBlock::new("oauth", Lang::TypeScript, BRANCHES)
        .theme(broadsheet::THEME)
        .size(size)
        .at([150.0, 560.0 - 5.0 * size * 1.55])
        .mark(2..4, LineMark::Deleted)
        .mark(5..11, LineMark::Deleted);
    let deleted = [2, 3, 5, 6, 7, 8, 9, 10];
    let fields = [
        ("d-200", "dialect.errorsWith200", "GitHub"),
        ("d-params", "dialect.authorizeParams", "Google"),
        ("d-omit", "dialect.omitParams", "Entra ID"),
    ];

    let mut elements = vec![mono(
        "file",
        [150.0, 110.0],
        scale::LABEL,
        &[("core/src/integration-server-oauth.ts", INK_MUTED)],
    )];
    elements.extend(code.elements());
    elements.push(centred(
        "pr4",
        [1660.0, 260.0],
        broadsheet::shout("PR 4", scale::DISPLAY, Paint::hex(VERMILION)),
    ));
    elements.push(field::field("twelve-field", Paint::hex(COBALT)));
    elements.push(centred(
        "twelve",
        [520.0, 520.0],
        broadsheet::shout("12", 560.0, Paint::hex(SIGNAL)),
    ));
    for (index, (id, name, vendor)) in fields.iter().enumerate() {
        let y = 330.0 + index as f32 * 150.0;
        elements.push(mono(id, [960.0, y], scale::BODY, &[(name, PAPER)]));
        elements.push(left(
            &format!("{id}-for"),
            [962.0, y + 60.0],
            kit.caption(*vendor, scale::LABEL, Paint::hex(SIGNAL)),
        ));
    }
    elements.push(mono(
        "more",
        [960.0, 780.0],
        scale::LABEL,
        &[("+ 9 more in docs/console-adoption.md", "#AFC0FF")],
    ));
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    code.hide(&mut s, sc);
    code.reveal(&mut s, sc, millis(100), millis(40));
    let mut hidden = vec!["file", "pr4", "twelve-field", "twelve", "more"];
    let names = fields.map(|(id, ..)| [id.to_owned(), format!("{id}-for")]);
    hidden.extend(names.iter().flatten().map(String::as_str));
    hide(&mut s, sc, &hidden);
    for line in deleted {
        s.channel(sc, &format!("oauth-mark{line}.opacity"), 0.0);
    }
    show(&mut s, sc, "file", millis(100));
    stamp(&mut s, sc, "pr4", on(&spoken, "Console"));

    let deletes = on(&spoken, "deletes");
    for line in deleted {
        show(&mut s, sc, &format!("oauth-mark{line}"), deletes);
    }
    sfx::SEVER.play(sc, "sever", deletes, -17.0);
    for (word, lines) in [("Google", 5..9), ("Entra", 9..11), ("GitHub", 2..4)] {
        let at = on(&spoken, word);
        code.focus(&mut s, sc, lines, at, 0.3);
        sfx::TICK.play(sc, format!("tick-{word}"), at, -19.0);
    }

    let doc = on(&spoken, "adoption");
    code.leave(&mut s, sc, doc.saturating_sub(millis(200)), 0.2);
    out(&mut s, sc, "file", doc.saturating_sub(millis(200)));
    out(&mut s, sc, "pr4", doc.saturating_sub(millis(200)));
    field::flood(&mut s, sc, "twelve-field", doc, 0.2);
    let maps = on(&spoken, "maps");
    stamp(&mut s, sc, "twelve", maps);
    sfx::MARK.play(sc, "mark-twelve", maps, -16.0);
    for (index, (id, ..)) in fields.iter().enumerate() {
        let at = on(&spoken, "spots") + millis(180 * index as u64);
        slide(&mut s, sc, id, at, 60.0);
        arrive(&mut s, sc, &format!("{id}-for"), at + millis(120));
    }
    arrive(&mut s, sc, "more", on(&spoken, "fields"));

    scene.cue("branches", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 5: what PR 3 deletes, what a new vendor costs after, and the tool
/// effect marks nobody has filled in.
pub fn gets(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "05-gets", audio, TAIL);
    let paper = Paint::hex(PAPER);
    let ink = Paint::hex(INK);

    let catalog = CodeBlock::new(
        "cat",
        Lang::Plain,
        "vendors/acme/index.ts\nvendors/acme/mark.svg",
    )
    .theme(broadsheet::THEME)
    .size(52.0)
    .numbered(1)
    .at([250.0, 300.0])
    .mark(0..2, LineMark::Inserted);
    let pin = CodeBlock::new(
        "pin",
        Lang::Json,
        "\"connections\": \"github:anomalyco/connections#4cb35bd\",\n\"connections\": \"github:anomalyco/connections#<next>\",",
    )
    .theme(broadsheet::THEME)
    .size(44.0)
    .numbered(1)
    .at([250.0, 730.0])
    .mark(0..1, LineMark::Deleted)
    .mark(1..2, LineMark::Inserted);
    let effects = [
        (
            "x-read",
            "\"read\"",
            "list repos",
            GREEN,
            "read",
            150.0,
            360.0,
        ),
        (
            "x-write",
            "\"write\"",
            "open a pull request",
            SIGNAL,
            "write",
            1010.0,
            360.0,
        ),
        (
            "x-send",
            "\"external_send\"",
            "post to Slack",
            "#FF8A5C",
            "sends",
            150.0,
            620.0,
        ),
        (
            "x-destroy",
            "\"destructive\"",
            "delete a branch",
            RED,
            "destructive",
            1010.0,
            620.0,
        ),
    ];

    // Back to front: the vermilion room is the field everything starts on;
    // the ink room floods over it for the effect marks.
    let mut elements = vec![
        field::field("vermilion", Paint::hex(VERMILION)),
        centred(
            "logos",
            [600.0, 470.0],
            broadsheet::shout("137", 440.0, paper),
        ),
        centred(
            "logos-what",
            [600.0, 760.0],
            kit.caption("SVGs in web/src/assets", scale::BODY, ink),
        ),
        mono_aligned(
            "test",
            [1420.0, 470.0],
            56.0,
            &[("logos.test.ts", PAPER)],
            CaptionAlign::Center,
        ),
        centred(
            "test-what",
            [1420.0, 560.0],
            kit.caption("reads seed SQL", scale::LABEL, ink),
        ),
        StageElement::path(
            "logos-x",
            vec![
                Waypoint::Point([300.0, 520.0, 0.0]),
                Waypoint::Point([900.0, 400.0, 0.0]),
            ],
        )
        .tone(Tone::Plain)
        .stroke_width(22.0),
        StageElement::path(
            "test-x",
            vec![
                Waypoint::Point([1180.0, 472.0, 0.0]),
                Waypoint::Point([1660.0, 472.0, 0.0]),
            ],
        )
        .tone(Tone::Plain)
        .stroke_width(10.0),
        mono(
            "cat-repo",
            [150.0, 200.0],
            scale::LABEL,
            &[("anomalyco/connections", INK_MUTED)],
        ),
        mono(
            "pin-repo",
            [150.0, 630.0],
            scale::LABEL,
            &[("opencode-console · package.json", INK_MUTED)],
        ),
    ];
    elements.extend(catalog.elements());
    elements.extend(pin.elements());
    elements.push(field::field("ink", Paint::hex(INK)));
    elements.push(mono(
        "effect-key",
        [150.0, 200.0],
        scale::LABEL,
        &[("tools[name].effect", "#9C958A")],
    ));
    for (id, value, example, hex, _, x, y) in effects {
        elements.push(mono(id, [x, y], 72.0, &[(value, hex)]));
        elements.push(left(
            &format!("{id}-eg"),
            [x, y + 80.0],
            kit.caption(format!("e.g. {example}"), scale::LABEL, paper),
        ));
    }
    elements.push(centred(
        "zero",
        [960.0, 900.0],
        broadsheet::shout("0 of 131 vendors", scale::HEADLINE, Paint::hex(SIGNAL)),
    ));
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    diagram::ink(&mut s, sc, &["logos-x", "test-x"]);

    let mut hidden = vec![
        "logos-what",
        "test",
        "test-what",
        "logos-x",
        "test-x",
        "cat-repo",
        "pin-repo",
        "ink",
        "effect-key",
        "zero",
    ];
    let ids = effects.map(|e| [e.0.to_owned(), format!("{}-eg", e.0)]);
    hidden.extend(ids.iter().flatten().map(String::as_str));
    hide(&mut s, sc, &hidden);
    catalog.hide(&mut s, sc);
    pin.hide(&mut s, sc);
    for id in ["cat-mark0", "cat-mark1", "pin-mark0", "pin-mark1"] {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
    }

    // Room 1: what PR 3 deletes.
    stamp(&mut s, sc, "logos", millis(120));
    arrive(&mut s, sc, "logos-what", on(&spoken, "bundled"));
    let strike = on(&spoken, "logos") + millis(250);
    show(&mut s, sc, "logos-x", strike);
    s.connect(sc, "logos-x", strike, 0.3);
    sfx::SEVER.play(sc, "sever-logos", strike, -16.5);
    let test = on(&spoken, "test");
    slide(&mut s, sc, "test", test, 80.0);
    arrive(&mut s, sc, "test-what", on(&spoken, "scrapes"));
    let strike = on(&spoken, "check") + millis(150);
    show(&mut s, sc, "test-x", strike);
    s.connect(sc, "test-x", strike, 0.25);
    sfx::SEVER.play(sc, "sever-test", strike, -16.5);

    // Room 2, on paper: a new vendor is two added files and a pin.
    let after = on(&spoken, "After");
    for id in [
        "logos",
        "logos-what",
        "test",
        "test-what",
        "logos-x",
        "test-x",
    ] {
        out(&mut s, sc, id, after.saturating_sub(millis(150)));
    }
    field::drain(&mut s, sc, "vermilion", after, 0.2);
    let vendor = on(&spoken, "vendor");
    arrive(&mut s, sc, "cat-repo", vendor.saturating_sub(millis(300)));
    for id in ["cat-mark0", "cat-mark1"] {
        show(&mut s, sc, id, vendor);
    }
    catalog.reveal(&mut s, sc, vendor, millis(120));
    sfx::CONFIRM.play(sc, "confirm-vendor", vendor, -17.0);
    let bump = on(&spoken, "pin");
    arrive(&mut s, sc, "pin-repo", bump.saturating_sub(millis(300)));
    for id in ["pin-mark0", "pin-mark1"] {
        show(&mut s, sc, id, bump);
    }
    pin.reveal(&mut s, sc, bump, millis(150));

    // Room 3, on ink: the effect marks the schema has room for.
    let schema = on(&spoken, "schema");
    out(&mut s, sc, "cat-repo", schema.saturating_sub(millis(150)));
    out(&mut s, sc, "pin-repo", schema.saturating_sub(millis(150)));
    catalog.leave(&mut s, sc, schema.saturating_sub(millis(150)), 0.2);
    pin.leave(&mut s, sc, schema.saturating_sub(millis(150)), 0.2);
    field::flood(&mut s, sc, "ink", schema, 0.25);
    arrive(&mut s, sc, "effect-key", schema + millis(200));
    for (id, _, _, _, word, ..) in effects {
        let at = on(&spoken, word);
        pop(&mut s, sc, id, at);
        arrive(&mut s, sc, &format!("{id}-eg"), at + millis(150));
        sfx::TICK.play(sc, format!("tick-{id}"), at, -19.0);
    }
    let nobody = on(&spoken, "Nobody");
    for (id, ..) in effects {
        dim(&mut s, sc, id, nobody, 0.3);
        dim(&mut s, sc, &format!("{id}-eg"), nobody, 0.3);
    }
    stamp(&mut s, sc, "zero", nobody + millis(100));
    sfx::FAILURE.play(sc, "fail-zero", nobody + millis(100), -21.0);

    scene.cue("gets", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 6: the catalog side is done, Adam's four PRs, and the gaps.
pub fn status(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "06-status", audio, TAIL);
    let ink = Paint::hex(ide::INK);
    let prs = [
        (
            "pr3",
            "PR 3",
            "presets",
            "MEDIUM",
            ide::AMBER,
            660.0,
            "reads",
        ),
        ("pr4", "PR 4", "OAuth quirks", "HIGH", RED, 1000.0, "moves"),
        (
            "pr5",
            "PR 5",
            "tool effects",
            "",
            ide::MUTED,
            1340.0,
            "need",
        ),
        (
            "pr6",
            "PR 6",
            "triggers · Git",
            "",
            ide::MUTED,
            1680.0,
            "work",
        ),
    ];
    let gaps = [
        (
            "g-read",
            "readOnly: unknown",
            "on 117 of 145 sign-in methods",
            "facts",
        ),
        (
            "g-git",
            "private git dependency",
            "CI, Docker and deploys need access",
            "private",
        ),
        (
            "g-sign",
            "unsigned releases",
            "trust is the pinned commit",
            "releases",
        ),
    ];

    let mut elements = diagram::node(
        "catalog",
        [260.0, 360.0],
        [320.0, 140.0],
        Paint::hex("#163322"),
        kit.headline("CATALOG", 52.0, Paint::hex(GREEN)),
    );
    elements.push(centred(
        "cat-count",
        [260.0, 530.0],
        kit.headline("132 / 132", 64.0, Paint::hex(GREEN)),
    ));
    elements.push(mono_aligned(
        "cat-note",
        [260.0, 600.0],
        scale::LABEL,
        &[("presets match", ide::MUTED)],
        CaptionAlign::Center,
    ));
    let mut previous = "catalog";
    for (id, title, what, risk, hex, x, _) in prs {
        elements.extend(diagram::node(
            id,
            [x, 360.0],
            [280.0, 140.0],
            Paint::hex(NODE),
            kit.headline(title, 60.0, ink),
        ));
        elements.push(mono_aligned(
            &format!("{id}-what"),
            [x, 480.0],
            scale::LABEL,
            &[(what, ide::INK)],
            CaptionAlign::Center,
        ));
        if !risk.is_empty() {
            elements.push(centred(
                &format!("{id}-risk"),
                [x, 560.0],
                kit.headline(risk, 48.0, Paint::hex(hex)),
            ));
        }
        elements.push(diagram::edge(
            &format!("e-{id}"),
            previous,
            id,
            Tone::Accent,
        ));
        previous = id;
    }
    elements.push(mono_aligned(
        "waits",
        [1510.0, 560.0],
        scale::LABEL,
        &[("wait on catalog work", ide::MUTED)],
        CaptionAlign::Center,
    ));
    elements.push(left(
        "gaps",
        [150.0, 780.0],
        kit.headline("GAPS", 64.0, Paint::hex(ide::AMBER)),
    ));
    for (index, (id, title, why, _)) in gaps.iter().enumerate() {
        let y = 780.0 + index as f32 * 85.0;
        elements.push(mono(id, [480.0, y], 40.0, &[(title, ide::INK)]));
        elements.push(mono(
            &format!("{id}-why"),
            [1060.0, y],
            scale::LABEL,
            &[(why, ide::MUTED)],
        ));
    }
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();
    let home = cam.pose(sc, 0);
    let edges = prs.map(|(id, ..)| format!("e-{id}"));
    diagram::ink(&mut s, sc, &edges.each_ref().map(String::as_str));

    let mut hidden = vec!["cat-count", "cat-note", "waits", "gaps"];
    let pr_ids = prs.map(|(id, _, _, risk, ..)| {
        let mut ids = vec![
            id.to_owned(),
            format!("{id}-title"),
            format!("{id}-what"),
            format!("e-{id}"),
        ];
        if !risk.is_empty() {
            ids.push(format!("{id}-risk"));
        }
        ids
    });
    hidden.extend(pr_ids.iter().flatten().map(String::as_str));
    let gap_ids = gaps.map(|(id, ..)| [id.to_owned(), format!("{id}-why")]);
    hidden.extend(gap_ids.iter().flatten().map(String::as_str));
    hide(&mut s, sc, &hidden);

    // Close on the catalog, then pull back to the four PRs, then the gaps.
    cam.frame(
        sc,
        &["catalog", "cat-count", "cat-note"],
        200.0,
        0,
        Move::Cut,
    )?;
    pop(&mut s, sc, "catalog", millis(100));
    pop(&mut s, sc, "catalog-title", millis(100));
    let test = on(&spoken, "test");
    stamp(&mut s, sc, "cat-count", test);
    arrive(&mut s, sc, "cat-note", test + millis(300));
    sfx::CONFIRM.play(sc, "confirm-match", test, -17.0);

    let yours = on(&spoken, "Your");
    cam.move_to(
        sc,
        &home,
        yours.saturating_sub(millis(300)),
        Move::Glide(1.1),
    );
    for (index, (id, ..)) in prs.iter().enumerate() {
        let at = on(&spoken, "order") + millis(110 * index as u64);
        show(&mut s, sc, &format!("e-{id}"), at);
        s.connect(sc, &format!("e-{id}"), at, 0.3);
        pop(&mut s, sc, id, at + millis(150));
        pop(&mut s, sc, &format!("{id}-title"), at + millis(150));
    }
    for (id, _, _, risk, _, _, word) in prs {
        let at = on(&spoken, word);
        arrive(&mut s, sc, &format!("{id}-what"), at);
        if !risk.is_empty() {
            stamp(
                &mut s,
                sc,
                &format!("{id}-risk"),
                on(&spoken, if id == "pr3" { "medium" } else { "high" }),
            );
        }
        sfx::TICK.play(sc, format!("tick-{id}"), at, -19.0);
    }
    let first = on(&spoken, "first");
    for id in ["pr5", "pr6", "pr5-title", "pr6-title"] {
        dim(&mut s, sc, id, first, 0.45);
    }
    arrive(&mut s, sc, "waits", first);

    let gaps_at = on(&spoken, "gaps");
    cam.frame(
        sc,
        &["catalog", "pr6", "gaps", "g-sign-why"],
        80.0,
        gaps_at.saturating_sub(millis(400)),
        Move::Glide(1.0),
    )?;
    slide(&mut s, sc, "gaps", gaps_at, -60.0);
    sfx::MARK.play(sc, "mark-gaps", gaps_at, -18.0);
    for (id, _, _, word) in gaps {
        let at = on(&spoken, word);
        arrive(&mut s, sc, id, at);
        arrive(&mut s, sc, &format!("{id}-why"), at + millis(200));
    }

    scene.cue("status", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 7: the ask.
pub fn ask(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "07-ask", audio, 2_200_000_000);
    let paper = Paint::hex(PAPER);
    let elements = vec![
        field::field("cobalt", Paint::hex(COBALT)),
        left(
            "pr3",
            [150.0, 470.0],
            broadsheet::shout("PR 3", 380.0, paper),
        ),
        left(
            "pr3-what",
            [154.0, 740.0],
            kit.caption("presets from the catalog", scale::BODY, Paint::hex(SIGNAL)),
        ),
        mono(
            "doc",
            [1100.0, 440.0],
            44.0,
            &[("docs/console-adoption.md", PAPER)],
        ),
        left(
            "doc-what",
            [1100.0, 520.0],
            kit.caption("files, deletes and risk, per PR", 44.0, Paint::hex(SIGNAL)),
        ),
    ];
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    hide(&mut s, sc, &["pr3", "pr3-what", "doc", "doc-what"]);
    slide(&mut s, sc, "pr3", millis(150), -80.0);
    let pick = on(&spoken, "pick");
    arrive(&mut s, sc, "pr3-what", pick);
    sfx::CONFIRM.play(sc, "confirm-pick", pick, -17.0);
    let plan_at = on(&spoken, "plan");
    slide(&mut s, sc, "doc", plan_at, 60.0);
    arrive(&mut s, sc, "doc-what", plan_at + millis(250));

    scene.cue("ask", 0, duration);
    Ok(scene.finish()?)
}
