//! Beats 1-3: where one vendor lives today, the rule, and GitHub's entry.
use anyhow::Result;
use psychopomp::{
    author::millis,
    caption::CaptionAlign,
    kit::{
        broadsheet::{self, COBALT, INK, PAPER, SIGNAL, VERMILION},
        code::CodeBlock,
        field, ide,
        moves::{arrive, dim, hide, on, out, slide, stamp},
        scale,
    },
    paint::Paint,
    plan::ScenePlan,
    sfx,
    stage::{Move, StageActor},
    syntax::Lang,
    typography::SplitUnit,
};
use psychopomp_media::Audio;

use crate::look::{TAIL, begin, centred, left, mono, mono_aligned};

const INK_MUTED: &str = "#68625A";
const PAPER_MUTED: &str = "#C9C2B6";

/// The four places GitHub lives in Console today, from `origin/dev`.
const PLACES: [(&str, &str, &str, Lang, &str, &str); 4] = [
    (
        "sql",
        "SEED",
        "database/migrations/…gateway/migration.sql:210",
        Lang::Sql,
        "('github', 'GitHub',\n 'https://api.githubcopilot.com/mcp/', 'oauth', …)",
        "row",
    ),
    (
        "oauth",
        "OAUTH",
        "core/src/integration-server-oauth.ts:77",
        Lang::TypeScript,
        "issuer: \"https://github.com/login/oauth\",\n\"bad_verification_code\", \"bad_refresh_token\",",
        "OAuth",
    ),
    (
        "logo",
        "LOGO",
        "web/src/assets/integrations/github.svg",
        Lang::Tsx,
        "<svg viewBox=\"0 0 320 320\">\n  <path fill=\"#fff\" d=\"M41.43 69.38…\"/>",
        "logo",
    ),
    (
        "guide",
        "GUIDE",
        "web/src/pages/admin/integrations/own-app.tsx",
        Lang::Tsx,
        "<Match when={props.slug === \"slack\"}>\n<Match when={props.slug === \"salesforce\"}>",
        "Slack",
    ),
];

/// Beat 1: one vendor, four files in three packages.
pub fn today(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "01-today", audio, TAIL);
    let ink = Paint::hex(INK);
    let paper = Paint::hex(PAPER);

    let blocks = PLACES.map(|(id, _, _, lang, source, _)| {
        CodeBlock::new(id, lang, source)
            .theme(broadsheet::THEME)
            .size(38.0)
            .at([640.0, 0.0])
    });
    let rows = [180.0, 430.0, 680.0, 930.0];
    let blocks = blocks
        .into_iter()
        .zip(rows)
        .map(|(block, y)| block.at([640.0, y - 10.0]))
        .collect::<Vec<_>>();

    let mut elements = Vec::new();
    for (((id, word, path, ..), block), y) in PLACES.iter().zip(&blocks).zip(rows) {
        elements.push(left(
            &format!("{id}-word"),
            [150.0, y + 20.0],
            broadsheet::shout(word, scale::HEADLINE, ink),
        ));
        elements.push(mono(
            &format!("{id}-path"),
            [640.0, y - 75.0],
            scale::LABEL,
            &[(path, INK_MUTED)],
        ));
        elements.extend(block.elements());
    }
    // The verdict room and the title room both sit over the paper room.
    elements.push(field::field("verdict", Paint::hex(VERMILION)));
    elements.push(centred(
        "packages",
        [960.0, 430.0],
        broadsheet::shout("3 packages", scale::HERO, paper),
    ));
    elements.push(centred(
        "packages-what",
        [960.0, 640.0],
        kit.caption("database · core · web, for one vendor", scale::BODY, ink),
    ));
    elements.push(field::field("title-field", Paint::hex(COBALT)));
    elements.push(centred(
        "title",
        [960.0, 450.0],
        broadsheet::shout("Connections\ncatalog", scale::HERO, paper)
            .split(SplitUnit::Line)
            .masked(true),
    ));
    elements.push(centred(
        "sub",
        [960.0, 770.0],
        kit.caption(
            "a pitch for Console · for Adam",
            scale::BODY,
            Paint::hex(SIGNAL),
        ),
    ));

    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    s.rise_type(
        sc,
        "title",
        millis(150),
        millis(140),
        240.0,
        psychopomp::plan::SpringPlan::ENTER,
    );
    hide(&mut s, sc, &["sub", "verdict", "packages", "packages-what"]);
    arrive(&mut s, sc, "sub", on(&spoken, "pitch"));

    let today = on(&spoken, "Today");
    out(&mut s, sc, "title", today.saturating_sub(millis(250)));
    out(&mut s, sc, "sub", today.saturating_sub(millis(250)));
    field::drain(
        &mut s,
        sc,
        "title-field",
        today.saturating_sub(millis(30)),
        0.18,
    );
    sfx::MARK.play(sc, "mark-today", today, -18.0);

    // At "Today" the four names come in faint, then each lights up
    // with its file and real lines as the narration reaches it.
    let several = today + millis(300);
    for (index, ((id, _, _, _, _, word), block)) in PLACES.iter().zip(&blocks).enumerate() {
        let at = on(&spoken, word);
        let name = format!("{id}-word");
        block.hide(&mut s, sc);
        hide(&mut s, sc, &[&format!("{id}-path")]);
        slide(
            &mut s,
            sc,
            &name,
            several + millis(110 * index as u64),
            -90.0,
        );
        dim(
            &mut s,
            sc,
            &name,
            several + millis(110 * index as u64 + 350),
            0.22,
        );
        s.ease(
            sc,
            &format!("{name}.opacity"),
            at,
            1.0,
            0.2,
            psychopomp::math::easing::Ease::ARRIVE,
        );
        arrive(&mut s, sc, &format!("{id}-path"), at + millis(120));
        block.reveal(&mut s, sc, at + millis(200), millis(90));
        sfx::TICK.play(sc, format!("tick-{id}"), at, -19.0);
    }

    // "None of that changes": the paper room clears and the verdict lands.
    let none = on(&spoken, "None");
    for ((id, ..), block) in PLACES.iter().zip(&blocks) {
        out(&mut s, sc, &format!("{id}-word"), none);
        out(&mut s, sc, &format!("{id}-path"), none);
        block.leave(&mut s, sc, none, 0.2);
    }
    field::flood(&mut s, sc, "verdict", none + millis(120), 0.2);
    stamp(&mut s, sc, "packages", none + millis(250));
    arrive(&mut s, sc, "packages-what", none + millis(700));
    sfx::MARK.play(sc, "mark-none", none + millis(250), -16.0);

    scene.cue("today", 0, duration);
    Ok(scene.finish()?)
}

/// Beat 2: the catalog holds what's true everywhere; Console keeps the rest.
pub fn rule(audio: &Audio) -> Result<ScenePlan> {
    let kit = broadsheet::kit();
    let (mut scene, spoken, duration) = begin(&kit, "02-rule", audio, TAIL);
    let paper = Paint::hex(PAPER);
    let ink = Paint::hex(INK);

    let entries = [
        "vendors/github/index.ts",
        "vendors/slack/index.ts",
        "vendors/salesforce/index.ts",
        "vendors/…",
    ];
    let catalog = [
        ("f-url", "url: …githubcopilot.com/mcp/", "fact"),
        ("f-auth", "auth: key | oauth", "true"),
        ("f-quirk", "errorsWith200: [ … ]", "OpenCode"),
        ("f-mark", "mark.svg", "deployment"),
    ];
    let console = [
        ("c-secret", "INTEGRATION_GITHUB_CLIENT_SECRET", "Secrets"),
        ("c-token", "a member's refresh_token", "workspaces"),
        ("c-policy", "workspace policy: read-only", "policy"),
    ];

    let mut elements = vec![
        field::slab("left", [0.0, 0.0, 960.0, 1080.0], Paint::hex(COBALT)),
        field::field("cover", Paint::hex(COBALT)),
        mono(
            "repo",
            [150.0, 230.0],
            scale::LABEL,
            &[("anomalyco/connections", PAPER_MUTED)],
        ),
        centred(
            "count",
            [1420.0, 480.0],
            broadsheet::shout("131", 420.0, Paint::hex(SIGNAL)),
        ),
        centred(
            "count-what",
            [1420.0, 760.0],
            kit.caption("entries, one folder each", scale::BODY, paper),
        ),
    ];
    for (index, path) in entries.iter().enumerate() {
        elements.push(mono(
            &format!("e{index}"),
            [150.0, 360.0 + index as f32 * 110.0],
            scale::BODY,
            &[(path, PAPER)],
        ));
    }
    elements.push(left(
        "catalog",
        [120.0, 220.0],
        broadsheet::shout("Catalog", scale::DISPLAY, paper),
    ));
    elements.push(left(
        "catalog-what",
        [124.0, 345.0],
        kit.caption("ships in the package", scale::BODY, Paint::hex(SIGNAL)),
    ));
    elements.push(left(
        "console",
        [1080.0, 220.0],
        broadsheet::shout("Console", scale::DISPLAY, ink),
    ));
    elements.push(left(
        "console-what",
        [1084.0, 345.0],
        kit.caption("set per deployment", scale::BODY, Paint::hex(VERMILION)),
    ));
    for (index, (id, text, _)) in catalog.iter().enumerate() {
        elements.push(mono(
            id,
            [124.0, 500.0 + index as f32 * 120.0],
            40.0,
            &[(text, PAPER)],
        ));
    }
    for (index, (id, text, _)) in console.iter().enumerate() {
        elements.push(mono(
            id,
            [1084.0, 500.0 + index as f32 * 120.0],
            40.0,
            &[(text, INK)],
        ));
    }

    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    let intro = ["repo", "count", "count-what", "e0", "e1", "e2", "e3"];
    let mut hidden = intro.to_vec();
    hidden.extend(["catalog", "catalog-what", "console", "console-what"]);
    hidden.extend(catalog.iter().map(|(id, ..)| *id));
    hidden.extend(console.iter().map(|(id, ..)| *id));
    hide(&mut s, sc, &hidden);

    arrive(&mut s, sc, "repo", millis(100));
    for index in 0..entries.len() {
        arrive(
            &mut s,
            sc,
            &format!("e{index}"),
            on(&spoken, "entry") + millis(110 * index as u64),
        );
    }
    stamp(&mut s, sc, "count", on(&spoken, "vendor"));
    arrive(
        &mut s,
        sc,
        "count-what",
        on(&spoken, "vendor") + millis(250),
    );

    // The rule splits the frame: the catalog keeps cobalt, Console gets paper.
    let fact = on(&spoken, "A fact");
    for id in intro {
        out(&mut s, sc, id, fact.saturating_sub(millis(350)));
    }
    slide(&mut s, sc, "catalog", fact, -80.0);
    arrive(&mut s, sc, "catalog-what", fact + millis(300));
    for (id, _, word) in catalog {
        arrive(&mut s, sc, id, on(&spoken, word));
    }
    let secrets = on(&spoken, "Secrets");
    field::drain(
        &mut s,
        sc,
        "cover",
        secrets.saturating_sub(millis(200)),
        0.25,
    );
    slide(
        &mut s,
        sc,
        "console",
        secrets.saturating_sub(millis(100)),
        80.0,
    );
    arrive(&mut s, sc, "console-what", secrets + millis(250));
    for (id, _, word) in console {
        arrive(&mut s, sc, id, on(&spoken, word));
    }
    sfx::MARK.play(sc, "mark-fact", fact, -18.0);
    sfx::MARK.play(sc, "mark-secrets", secrets, -18.0);

    scene.cue("rule", 0, duration);
    Ok(scene.finish()?)
}

/// GitHub's entry, condensed from `vendors/github/index.ts`.
const ENTRY: &str = r#"export default defineVendor({
  slug: 'github', name: 'GitHub',
  surfaces: [{ kind: 'mcp',
    url: 'https://api.githubcopilot.com/mcp/' }],
  auth: [{ type: 'key', keyFormat: 'bearer' },
    { type: 'oauth', dialect: {
      errorsWith200: {
        codes: ['bad_verification_code',
                'bad_refresh_token'],
        source: 'https://docs.github.com/…',
        consoleRef: 'integration-server-oauth.ts:280' },
} }] })"#;

/// Beat 3: GitHub's entry is typed data, and its quirks cite their sources.
pub fn entry(audio: &Audio) -> Result<ScenePlan> {
    let kit = ide::kit();
    let (mut scene, spoken, duration) = begin(&kit, "03-entry", audio, TAIL);
    let size = 40.0;
    let code = CodeBlock::new("entry", Lang::TypeScript, ENTRY)
        .theme(ide::THEME)
        .size(size)
        .numbered(1)
        .at([260.0, 540.0 - 5.5 * size * 1.55]);
    let mut elements = code.elements();
    elements.push(mono_aligned(
        "two-hundred",
        [260.0 + 25.0 * size * 0.6, code.line_y(6)],
        44.0,
        &[("← HTTP 200, error in the body", ide::AMBER)],
        CaptionAlign::Left,
    ));
    elements.push(mono_aligned(
        "typed",
        [260.0 + 32.0 * size * 0.6, code.line_y(0)],
        size,
        &[("// typed: a wrong field is red", ide::MUTED)],
        CaptionAlign::Left,
    ));
    let plan = kit.stage(elements);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;
    let cam = s.camera();
    let home = cam.pose(sc, 0);

    code.hide(&mut s, sc);
    code.reveal(&mut s, sc, millis(100), millis(45));
    hide(&mut s, sc, &["two-hundred", "typed"]);

    let editor = on(&spoken, "editor");
    code.focus(&mut s, sc, 0..1, editor, 0.3);
    arrive(&mut s, sc, "typed", editor + millis(150));

    let surface = on(&spoken, "surface");
    out(&mut s, sc, "typed", surface.saturating_sub(millis(200)));
    code.focus(&mut s, sc, 2..4, surface, 0.3);
    code.frame(
        &cam,
        sc,
        1..5,
        110.0,
        surface.saturating_sub(millis(250)),
        Move::Glide(0.9),
    )?;
    sfx::TICK.play(sc, "tick-surface", surface, -19.0);

    let auth = on(&spoken, "auth");
    code.focus(&mut s, sc, 4..6, auth, 0.3);
    sfx::TICK.play(sc, "tick-auth", auth, -19.0);

    let dialect = on(&spoken, "dialect");
    code.focus(&mut s, sc, 5..7, dialect, 0.3);
    code.frame(
        &cam,
        sc,
        5..11,
        110.0,
        dialect.saturating_sub(millis(250)),
        Move::Glide(0.9),
    )?;
    sfx::TICK.play(sc, "tick-dialect", dialect, -19.0);

    let status = on(&spoken, "status");
    code.focus(&mut s, sc, 6..9, status.saturating_sub(millis(500)), 0.3);
    slide(&mut s, sc, "two-hundred", status, 60.0);
    sfx::MARK.play(sc, "mark-200", status, -16.0);

    let cites = on(&spoken, "cites");
    dim(&mut s, sc, "two-hundred", cites, 0.35);
    code.focus(&mut s, sc, 9..11, cites, 0.3);
    sfx::TICK.play(sc, "tick-cites", cites, -19.0);
    cam.move_to(sc, &home, on(&spoken, "came"), Move::Glide(1.0));

    scene.cue("entry", 0, duration);
    Ok(scene.finish()?)
}
