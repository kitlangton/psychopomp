use std::{env, fs, path::PathBuf};

use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan, CalloutSide},
    editor::diff::{Diff, add, keep, remove},
    face::Face,
    math::easing::Ease,
    narration::Narration,
    plan::{
        ReelPlan, ReelSegmentPlan, ReelTransitionStyle, ReelWipePlan, ScenePlan, WipeDirection,
    },
    sfx,
    stage::{Fill, Material, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
    video::{self, VideoActor, VideoPlan},
};
use psychopomp_pr_walkthrough::film::{Pr, chip, code, footer, header, span};

const PR: Pr = Pr {
    number: "#53825",
    title: "segmented control & hit-testing",
    slug: "cursor",
};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let narration_dir = root.join("narration");
    let output = env::args().nth(1).map_or_else(
        || root.join("console-segmented-control.reel.json"),
        PathBuf::from,
    );
    let reel = build_reel(&narration_dir)?;
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!(
        "wrote {} ({:.1}s, {} segments)",
        output.display(),
        reel.duration_nanos() as f64 / 1e9,
        reel.segments.len()
    );
    Ok(())
}

fn build_reel(narration_dir: &std::path::Path) -> Result<ReelPlan> {
    let narration = Narration::load(narration_dir)?;
    let wipe_nanos = seconds(2.2);
    let before_video = build_video_before(&narration)?;
    let after_video = build_video_after(&narration)?;
    let stage = build_cursor_stage(&narration)?;
    let diff = code(&PR, &narration, cursor_diff(), true)?;

    let reel = ReelPlan {
        version: ReelPlan::VERSION,
        id: "console-segmented-control".to_owned(),
        segments: vec![
            ReelSegmentPlan {
                transition_nanos: 0,
                transition_style: ReelTransitionStyle::Dip,
                transition_focus: None,
                transition_wipe: None,
                plan: before_video,
            },
            ReelSegmentPlan::wiped(
                after_video,
                wipe_nanos,
                ReelWipePlan::new(WipeDirection::Left)
                    .hold(0.5, seconds(1.0))
                    .labeled("BEFORE", "AFTER"),
            ),
            ReelSegmentPlan {
                transition_nanos: millis(700),
                transition_style: ReelTransitionStyle::Dip,
                transition_focus: None,
                transition_wipe: None,
                plan: stage,
            },
            ReelSegmentPlan {
                transition_nanos: millis(700),
                transition_style: ReelTransitionStyle::Dip,
                transition_focus: None,
                transition_wipe: None,
                plan: diff,
            },
        ],
    };
    reel.validate()?;
    Ok(reel)
}

fn build_video_before(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(0.6), [("compare-before", seconds(2.4))])?;
    let duration = reading.duration();
    let mut scene = PlanBuilder::new("compare-before", duration);
    let [spoken] = reading.place(&mut scene);

    header(&mut scene, &PR, Some(seconds(0.2)))?;
    let mut before_chip = chip(&mut scene, "chip-before", Tone::Error, "before")?;
    before_chip.show(&mut scene, seconds(0.4));

    let mut card = VideoActor::declare(
        &mut scene,
        "recording-before",
        VideoPlan::new("before-clip", [1120, 488], 60)
            .at([960.0, 510.0], 1440.0)
            .titled("opencode-console  /  segmented-control (before)"),
        video::media(
            "before-clip",
            "../../assets/console-segmented-control/before.mp4",
            (0, duration),
            0,
        ),
    )?;
    card.fly_in(&mut scene, seconds(0.15));

    let focus_at = spoken.at("cost and tokens");
    card.focus(&mut scene, focus_at, [18.0, 62.0, 580.0, 215.0], 0.85);
    card.unfocus(&mut scene, spoken.at("no feedback"), 0.85);

    let mut cap = footer(
        &mut scene,
        "footer-before",
        vec![
            span("static toggle ", Tone::Error),
            span(
                "· each button flips [data-pressed] in place with no hover state",
                Tone::Muted,
            ),
        ],
    )?;
    cap.type_in(&mut scene, spoken.at("no sliding pill"), 46.0, 0.7);

    Ok(scene.finish()?)
}

fn build_video_after(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(seconds(1.5), [("compare-after", seconds(1.2))])?;
    let duration = reading.duration();
    let mut scene = PlanBuilder::new("compare-after", duration);
    let [spoken] = reading.place(&mut scene);

    header(&mut scene, &PR, None)?;
    let mut after_chip = chip(&mut scene, "chip-after", Tone::Success, "after the fix")?;
    after_chip.show(&mut scene, seconds(0.3));

    let mut card = VideoActor::declare(
        &mut scene,
        "recording-after",
        VideoPlan::new("after-clip", [1120, 488], 60)
            .at([960.0, 510.0], 1440.0)
            .titled("opencode-console  /  segmented-control (solid-motion)"),
        video::media(
            "after-clip",
            "../../assets/console-segmented-control/after.mp4",
            (0, duration),
            0,
        ),
    )?;
    card.fly_in(&mut scene, 0);

    let focus_at = spoken.at("position and width");
    card.focus(&mut scene, focus_at, [18.0, 62.0, 580.0, 215.0], 0.85);
    card.unfocus(&mut scene, spoken.at("first paint"), 0.85);

    let mut cap = footer(
        &mut scene,
        "footer-after",
        vec![
            span("solid-motion ", Tone::Success),
            span(
                "· 150ms snap spring on x + width · hover brightens inactive labels",
                Tone::Muted,
            ),
        ],
    )?;
    cap.type_in(&mut scene, spoken.at("shared indicator pill"), 46.0, 0.7);

    Ok(scene.finish()?)
}

fn stage_anchor(id: &str, element: &str, edge: CalloutSide) -> CalloutAnchorPlan {
    CalloutAnchorPlan::Stage {
        id: id.into(),
        element: element.into(),
        edge,
        side: None,
    }
}

fn cursor_stage_plan() -> StagePlan {
    let elements = vec![
        // Single centered hero UI control: recessed Well trough + tactile Raised sliding pill
        StageElement::rect("seg-track", [960.0, 500.0, 0.0], [980.0, 152.0])
            .corner(24.0)
            .fill(Some(Fill::Material(Material::Well)))
            .stroke(Some(Tone::Muted))
            .stroke_width(1.2),
        StageElement::rect("seg-pill", [636.0, 500.0, -2.0], [284.0, 126.0])
            .corner(18.0)
            .fill(Some(Fill::Material(Material::Raised)))
            .stroke(Some(Tone::Accent))
            .stroke_width(1.2),
        // The "Tokens" segment button shell (used as the outer hit-target and callout anchor)
        StageElement::rect("token-btn", [940.0, 500.0, -3.0], [284.0, 126.0])
            .corner(18.0)
            .fill(None)
            .stroke(Some(Tone::Request))
            .stroke_width(1.2),
        // The inner <span data-slot="segmented-control-v2-item-label"> X-ray hit-box around "Tokens"
        StageElement::rect("span-box", [940.0, 500.0, -4.0], [184.0, 72.0])
            .corner(10.0)
            .fill(Some(Fill::Tone(Tone::Error)))
            .fill_opacity(0.10)
            .stroke(Some(Tone::Error))
            .stroke_width(1.5)
            .dash([7.0, 6.0]),
        // UI segment labels in Face::Sans (Helvetica Neue) so the control reads as real UI
        StageElement::label(
            "seg-cost-muted",
            [636.0, 500.0, -5.0],
            36.0,
            &[("Cost", Tone::Muted)],
        )
        .face(Face::Sans),
        StageElement::label(
            "seg-cost",
            [636.0, 500.0, -5.0],
            36.0,
            &[("Cost", Tone::Plain)],
        )
        .face(Face::Sans),
        StageElement::label(
            "seg-tokens",
            [940.0, 500.0, -5.0],
            36.0,
            &[("Tokens", Tone::Muted)],
        )
        .face(Face::Sans),
        StageElement::label(
            "seg-tokens-lit",
            [940.0, 500.0, -5.0],
            36.0,
            &[("Tokens", Tone::Plain)],
        )
        .face(Face::Sans),
        StageElement::label(
            "seg-search",
            [1264.0, 500.0, -5.0],
            36.0,
            &[("Web Search", Tone::Muted)],
        )
        .face(Face::Sans),
        StageElement::label(
            "seg-search-lit",
            [1264.0, 500.0, -5.0],
            36.0,
            &[("Web Search", Tone::Plain)],
        )
        .face(Face::Sans),
        // Solid, dark-rimmed, shadow-casting Stage cursors at z = -12
        StageElement::icon("cur-hand", [720.0, 540.0, -12.0], 64.0, "hand-pointing")
            .tone(Tone::Plain),
        StageElement::icon("cur-arrow", [720.0, 540.0, -12.0], 64.0, "cursor").tone(Tone::Error),
        StageElement::icon("cur-text", [720.0, 540.0, -12.0], 64.0, "cursor-text")
            .tone(Tone::Warning),
    ];
    StagePlan {
        post: StagePost::RESTRAINED,
        elements,
    }
}

fn build_cursor_stage(narration: &Narration) -> Result<ScenePlan> {
    let reading = narration.reading(
        seconds(1.2),
        [
            ("cursor-before", seconds(2.0)),
            ("cursor-after", seconds(1.6)),
        ],
    )?;
    let mut scene = PlanBuilder::new("cursor-stage", reading.duration());
    let [before, after] = reading.place(&mut scene);
    let b = |phrase: &str| before.at(phrase);
    let a = |phrase: &str| after.at(phrase);

    let mut stage = StageActor::declare(&mut scene, "stage", &cursor_stage_plan())?;
    let s = &mut stage;
    let sc = &mut scene;

    // Initial state: quiet, clean hero control
    s.channel(sc, "cur-arrow.opacity", 0.0);
    s.channel(sc, "cur-text.opacity", 0.0);
    s.channel(sc, "token-btn.opacity", 0.0);
    s.channel(sc, "span-box.opacity", 0.0);
    s.channel(sc, "seg-tokens-lit.opacity", 0.0);
    s.channel(sc, "seg-search-lit.opacity", 0.0);

    // Subtle opening dolly-in and rigid-panel entrance for the hero SegmentedControl
    s.channel(sc, "camera.z", -90.0);
    s.to(sc, "camera.z", 0, 0.0, 2.0);
    for (id, delay) in [("seg-track", 0.12), ("seg-pill", 0.24)] {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
        s.channel(sc, &format!("{id}.scale"), 1.035);
        s.channel(sc, &format!("{id}.blur"), 4.0);
        s.ease(
            sc,
            &format!("{id}.opacity"),
            seconds(delay),
            1.0,
            0.22,
            Ease::Linear,
        );
        s.to(sc, &format!("{id}.scale"), seconds(delay), 1.0, 0.55);
        s.to(sc, &format!("{id}.blur"), seconds(delay), 0.0, 0.35);
    }

    // Callouts with analytic dot, ring, and elbow leader lines
    let mut btn_callout = CalloutActor::declare(
        sc,
        "callout-btn",
        &CalloutPlan::new(
            stage_anchor("btn", "token-btn", CalloutSide::Top),
            vec![
                span("<button> ", Tone::Plain),
                span("· cursor: pointer", Tone::Muted),
            ],
        )
        .side(CalloutSide::TopLeft)
        .elbow()
        .reach(76.0)
        .tone(Tone::Muted)
        .chip(),
    )?;
    let mut span_callout = CalloutActor::declare(
        sc,
        "callout-span",
        &CalloutPlan::new(
            stage_anchor("span", "span-box", CalloutSide::Bottom),
            vec![
                span("<span> ", Tone::Error),
                span("· pointer-events: auto · user-select: auto", Tone::Muted),
            ],
        )
        .side(CalloutSide::BottomRight)
        .elbow()
        .reach(96.0)
        .tone(Tone::Error)
        .chip(),
    )?;
    let mut fixed_callout = CalloutActor::declare(
        sc,
        "callout-fixed",
        &CalloutPlan::new(
            stage_anchor("fixed", "token-btn", CalloutSide::Bottom),
            vec![
                span("atomic hit-target ", Tone::Success),
                span("· pointer-events: none · select: none", Tone::Muted),
            ],
        )
        .side(CalloutSide::BottomRight)
        .elbow()
        .reach(82.0)
        .tone(Tone::Success)
        .chip(),
    )?;

    header(sc, &PR, Some(seconds(0.25)))?;
    let mut before_chip = chip(sc, "chip-before", Tone::Error, "before")?;
    before_chip.show(sc, seconds(0.45));

    let set_cursor_x = |s: &mut StageActor, sc: &mut PlanBuilder, at: u64, dx: f32, dur: f32| {
        for id in ["cur-hand", "cur-arrow", "cur-text"] {
            s.to(sc, &format!("{id}.x"), at, dx, dur);
        }
    };

    // 1. Cursor enters the "Tokens" button padding and camera drifts closer to inspect
    let strobing = b("cursor strobing");
    set_cursor_x(s, sc, strobing - seconds(0.3), 100.0, 0.65);
    s.fade_in(sc, "token-btn", strobing, 0.55, 0.35);
    btn_callout.show(sc, strobing + millis(150));

    // 2. Reveal the inner <span> hit-box and elbow callout when mentioned
    let dropped = b("dropped user select");
    s.to(sc, "camera.z", dropped, 180.0, 1.4);
    s.to(sc, "camera.y", dropped, -12.0, 1.4);
    s.fade_in(sc, "span-box", dropped, 1.0, 0.35);
    span_callout.show(sc, b("inner label spans"));

    // 3. Sweep across "T-o-k-e-n-s" (dx = 145 -> 305) and visibly flip the cursor shape
    // between Pointing Hand, Arrow, and Text I-Beam as it crosses the inner <span>!
    let cross_start = b("pointer crossed");
    let strobe_steps: [(f64, f32, &str); 6] = [
        (0.0, 150.0, "cur-arrow"),
        (0.55, 182.0, "cur-text"),
        (1.10, 214.0, "cur-hand"),
        (1.65, 246.0, "cur-arrow"),
        (2.20, 278.0, "cur-text"),
        (2.75, 316.0, "cur-hand"),
    ];
    for (i, (offset_s, dx, active_icon)) in strobe_steps.into_iter().enumerate() {
        let at = cross_start + seconds(offset_s);
        set_cursor_x(s, sc, at, dx, 0.45);
        for icon in ["cur-hand", "cur-arrow", "cur-text"] {
            let show = if icon == active_icon { 1.0 } else { 0.0 };
            s.ease(sc, &format!("{icon}.opacity"), at, show, 0.07, Ease::Linear);
        }
        if active_icon != "cur-hand" {
            s.hit(sc, "span-box.flash", at, 0.25, 0.0);
            s.hit(sc, &format!("{active_icon}.flash"), at, 0.22, 0.0);
            span_callout.emphasize(sc, at);
            sfx::GLITCH.play(sc, format!("strobe-{i}"), at, -16.0);
        } else {
            sfx::TICK.play(sc, format!("strobe-{i}"), at, -18.0);
        }
    }

    let zero_spec = b("zero specificity");
    s.hit(sc, "token-btn.flash", zero_spec, 0.3, 0.0);
    s.jolt(sc, zero_spec, [55.0, 0.0], 0.65);

    let mut foot_before = footer(
        sc,
        "foot-before",
        vec![
            span("cursor strobe ", Tone::Error),
            span(
                "· inner <span> steals the hit-target and flips the cursor over button text",
                Tone::Muted,
            ),
        ],
    )?;
    foot_before.type_in(sc, b("hit target flipped"), 46.0, 0.7);

    // Rewind and replay the fixed interaction on the same hero control
    let rewind_at = before.end() + seconds(0.4);
    let replay_at = rewind_at + seconds(1.1);
    s.rewind(sc, rewind_at, 0.12);
    before_chip.hide(sc, rewind_at);
    foot_before.hide(sc, rewind_at);
    span_callout.hide(sc, rewind_at);

    let mut rewind_chip = chip(sc, "chip-rewind", Tone::Accent, "◀◀ rewind")?;
    rewind_chip.show(sc, rewind_at + seconds(0.2));
    rewind_chip.hide(sc, replay_at);
    let mut after_chip = chip(sc, "chip-after", Tone::Success, "after the fix")?;
    after_chip.show(sc, replay_at + seconds(0.1));

    sfx::LAUNCH.play(sc, "rewind-sfx", rewind_at - seconds(0.1), -13.0);
    set_cursor_x(s, sc, rewind_at, -60.0, 0.5);
    s.to(sc, "camera.z", rewind_at, 70.0, 1.1);
    s.to(sc, "camera.y", rewind_at, 0.0, 1.1);
    s.ease(sc, "cur-hand.opacity", rewind_at, 1.0, 0.2, Ease::Linear);
    s.ease(sc, "cur-arrow.opacity", rewind_at, 0.0, 0.2, Ease::Linear);
    s.ease(sc, "cur-text.opacity", rewind_at, 0.0, 0.2, Ease::Linear);
    s.fade_in(sc, "span-box", rewind_at, 0.0, 0.25);

    // 4. After: inner span is masked out, button becomes an atomic hit-target,
    // the Pointing Hand glides smoothly onto "Tokens", clicks it (springing the pill),
    // and then glides across to "Web Search" and clicks it!
    let set_none = a("pointer events none");
    fixed_callout.show(sc, set_none);
    set_cursor_x(s, sc, set_none + millis(200), 220.0, 0.75);

    let atomic = a("atomic hit target");
    s.to(sc, "token-btn.emphasis", atomic, 1.0, 0.4);
    s.hit(sc, "token-btn.flash", atomic, 0.25, 0.0);
    fixed_callout.emphasize(sc, atomic);
    sfx::CONFIRM.play(sc, "atomic-ok", atomic, -8.0);

    // Click "Tokens": tactile cursor press (scale 0.90 -> 1.0), then pill springs to "Tokens"!
    let click_tokens = atomic + millis(280);
    s.to(sc, "cur-hand.scale", click_tokens, 0.88, 0.09);
    s.to(sc, "cur-hand.scale", click_tokens + millis(90), 1.0, 0.16);
    s.to(sc, "seg-pill.x", click_tokens + millis(60), 304.0, 0.18);
    s.to(
        sc,
        "seg-pill.emphasis",
        click_tokens + millis(60),
        0.7,
        0.25,
    );
    s.fade_in(sc, "seg-cost", click_tokens + millis(60), 0.0, 0.18);
    s.fade_in(sc, "seg-tokens-lit", click_tokens + millis(60), 1.0, 0.18);

    // Sweep smoothly across "Tokens" into "Web Search" (dx = 548), keeping the Pointing Hand
    // locked at 120Hz, then click "Web Search" to spring both x and width of the pill!
    let trace = a("120 hz");
    set_cursor_x(s, sc, trace, 548.0, 1.35);
    s.fade_in(sc, "token-btn", trace + millis(600), 0.0, 0.25);
    btn_callout.hide(sc, trace + millis(600));
    fixed_callout.hide(sc, trace + millis(600));
    let click_search = trace + seconds(1.4);
    s.to(sc, "cur-hand.scale", click_search, 0.88, 0.09);
    s.to(sc, "cur-hand.scale", click_search + millis(90), 1.0, 0.16);
    s.to(sc, "seg-pill.x", click_search + millis(60), 628.0, 0.18);
    s.to(sc, "seg-pill.width", click_search + millis(60), 36.0, 0.18);
    s.fade_in(sc, "seg-tokens-lit", click_search + millis(60), 0.0, 0.18);
    s.fade_in(sc, "seg-search-lit", click_search + millis(60), 1.0, 0.18);
    sfx::TICK.play(sc, "seed-locked", trace + seconds(0.4), -12.0);
    sfx::TICK.play(sc, "pill-slide", click_search + millis(60), -11.0);

    let mut foot_after = footer(
        sc,
        "foot-after",
        vec![
            span("verified ", Tone::Success),
            span(
                "· pointer-events: none + select: none keeps HAND locked while the pill springs x + width",
                Tone::Muted,
            ),
        ],
    )?;
    foot_after.type_in(sc, a("pointing hand seed"), 46.0, 0.7);

    Ok(scene.finish()?)
}

fn cursor_diff() -> (Diff, Vec<&'static str>, &'static str) {
    let diff = Diff {
        file_name: "segmented-control.tsx",
        lines: vec![
            keep("export function SegmentedControl(props: SegmentedControlProps) {"),
            add(
                1,
                "  const x = useMotionValue(0), width = useMotionValue(0)",
            ),
            add(1, "  animate(x, next.x, SEGMENTED_CONTROL_SPRING)"),
            add(1, "  animate(width, next.width, SEGMENTED_CONTROL_SPRING)"),
            keep("  return ("),
            keep("    <div role=\"group\" data-slot=\"segmented-control-v2\">"),
            add(
                1,
                "      <motion.span data-slot=\"segmented-control-v2-indicator\"",
            ),
            add(1, "        style={{ x, width }} />"),
            remove(
                2,
                "      <span data-slot=\"segmented-control-v2-item-label\">",
            ),
            add(2, "      <span class=\"pointer-events-none select-none\">"),
            keep("        {local.children}"),
            keep("      </span>"),
            keep("    </div>"),
            keep("  )"),
        ],
    };
    (
        diff,
        vec!["indicator position and width", "disable pointer events"],
        "condensed from packages/ui · @opencode/ui (#53825) & opencode-console (#3247)",
    )
}
