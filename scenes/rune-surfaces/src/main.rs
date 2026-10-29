//! Showroom reel for `RuneSurface` (`Kit`, `Note`, and `Diff`):
//! 1. `@opencode/ui` `/connect` + `SegmentedControl` card (`RuneKind::Kit`)
//! 2. Seer note primitives (`facts`, `compare`, `steps`, `size`, `md_code`) (`RuneKind::Note`)
//! 3. `rune_kit::DiffView` unified-to-split diff with line selection and inline review (`RuneKind::Diff`)

use std::{env, fs, path::PathBuf};

use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, millis, seconds},
    caption::CaptionSpanPlan,
    chrome::{chip, footer, header},
    plan::{ReelPlan, ReelSegmentPlan, ScenePlan},
    rune_surface::{
        KitTone, RuneAction, RuneDiffAnnotation, RuneDiffFile, RuneDiffMode, RuneDiffSelection,
        RuneSurfaceActor, RuneSurfacePlan, kit,
    },
    tone::Tone,
};

fn main() -> Result<()> {
    let output = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("target/rune-surfaces.json"), PathBuf::from);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let reel = ReelPlan::new(
        "rune-surfaces",
        vec![
            ReelSegmentPlan::cut(kit_segment()?),
            ReelSegmentPlan::dipped(note_segment()?, millis(550)),
            ReelSegmentPlan::dipped(diff_segment()?, millis(550)),
        ],
    )?;
    fs::write(&output, serde_json::to_string_pretty(&reel)? + "\n")?;
    eprintln!("wrote {}", output.display());
    Ok(())
}

fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

fn kit_segment() -> Result<ScenePlan> {
    let mut scene = PlanBuilder::new("rune-kit-card", seconds(4.5));
    let mut hdr = header(&mut scene, "rune", "@opencode/ui kit surface")?;
    hdr.show(&mut scene, 0);
    let mut tag = chip(&mut scene, "chip", Tone::Success, "rune-kit · 24x shutter")?;
    tag.show(&mut scene, millis(200));

    let dialog = kit::dialog(
        "dialog",
        470.0,
        vec![
            kit::row(
                "hdr",
                8.0,
                vec![
                    kit::text("Connect an integration", 15.0, 600.0, KitTone::Base),
                    kit::spacer(),
                    kit::badge("esc", KitTone::Muted),
                ],
            ),
            kit::text_input("search", "", "Search 230 providers…"),
            kit::row(
                "seg-row",
                8.0,
                vec![
                    kit::text("Usage view", 12.5, 450.0, KitTone::Muted),
                    kit::spacer(),
                    kit::segmented("seg", &["Cost", "Tokens", "Web Search"], 0, Some(300.0)),
                ],
            ),
            kit::text("Popular", 11.5, 560.0, KitTone::Muted),
            kit::col(
                "rows",
                4.0,
                vec![
                    kit::list_row("go", "OpenCode Go", "API key only", KitTone::Danger, false),
                    kit::list_row(
                        "console",
                        "OpenCode Console",
                        "sign in or API key",
                        KitTone::Base,
                        true,
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
    );

    let mut ui = RuneSurfaceActor::declare(
        &mut scene,
        "connect-card",
        RuneSurfacePlan::kit(dialog).at([960.0, 525.0]),
    )?;
    ui.settle_in(&mut scene, millis(150));
    ui.commit(
        &mut scene,
        seconds(1.1),
        "select tokens",
        RuneAction::SelectSegment {
            key: "seg".into(),
            index: 1,
        },
    )?;
    ui.commit(
        &mut scene,
        seconds(2.1),
        "merge go into opencode",
        RuneAction::Batch {
            actions: vec![
                RuneAction::SetVisible {
                    key: "go".into(),
                    visible: false,
                },
                RuneAction::SetRow {
                    key: "console".into(),
                    title: Some("OpenCode".into()),
                    hint: Some("sign in or API key · Go included".into()),
                    tone: Some(KitTone::Base),
                    hint_tone: Some(KitTone::Success),
                    selected: Some(true),
                },
            ],
        },
    )?;
    ui.commit(
        &mut scene,
        seconds(3.1),
        "select web search",
        RuneAction::SelectSegment {
            key: "seg".into(),
            index: 2,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            span("real @opencode/ui tokens ", Tone::Success),
            span(
                "· Parley + swash dilated type · Taffy flex layout",
                Tone::Muted,
            ),
        ],
    )?;
    foot.show(&mut scene, millis(350));
    Ok(scene.finish()?)
}

fn note_segment() -> Result<ScenePlan> {
    let mut scene = PlanBuilder::new("rune-seer-note", seconds(4.0));
    let mut hdr = header(&mut scene, "rune", "Seer note primitives")?;
    hdr.show(&mut scene, 0);
    let mut tag = chip(&mut scene, "chip", Tone::Accent, "facts · compare · steps")?;
    tag.show(&mut scene, millis(200));

    let initial_md = "\
Mount real Rune views on Psychopomp's `wgpu` device with GPU `Rgba16Float` shutter exposure.

```facts
Shutter (24x): {verified} 11.9 ms on Apple M2 Max
Determinism: {verified} 0 differing pixels across 2,073,600 px
```

```compare
Before: 597 LOC of hand-placed Stage rects and duplicate labels.
After: 55 LOC of real @opencode/ui controls with zero pixel coordinates.
```";

    let updated_md = format!(
        "{initial_md}\n\n```steps\n{{built}} Shared Rgba16Float shutter · 11.9 ms at 1080p\n{{built}} RuneSurface Note, Diff & Kit · motion_gate verified\n{{working}} AnchorPlan::Rune & Studio preview\n```"
    );

    let mut note = RuneSurfaceActor::declare(
        &mut scene,
        "seer-note",
        RuneSurfacePlan::note(initial_md)
            .titled("t782 · Mount Rune surfaces in Psychopomp")
            .width(640.0)
            .scale_factor(1.7)
            .at([960.0, 525.0]),
    )?;
    note.settle_in(&mut scene, millis(150));
    note.commit(
        &mut scene,
        seconds(1.8),
        "reveal steps",
        RuneAction::SetMarkdown {
            markdown: updated_md,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            span("Seer note primitives ", Tone::Accent),
            span(
                "· facts, compare, size, and steps rendered directly in Rune",
                Tone::Muted,
            ),
        ],
    )?;
    foot.show(&mut scene, millis(350));
    Ok(scene.finish()?)
}

fn diff_segment() -> Result<ScenePlan> {
    let mut scene = PlanBuilder::new("rune-diff-view", seconds(4.5));
    let mut hdr = header(&mut scene, "rune", "rune_kit::DiffView")?;
    hdr.show(&mut scene, 0);
    let mut tag = chip(&mut scene, "chip", Tone::Request, "word diff · split")?;
    tag.show(&mut scene, millis(200));

    let before = "\
export function connectRows(workspace: Workspace) {
  return [
    { id: \"go\", label: \"OpenCode Go\", mode: \"api-key\" },
    { id: \"console\", label: \"OpenCode Console\", mode: \"oauth\" },
    { id: \"anthropic\", label: \"Anthropic\", mode: \"oauth\" },
  ]
}
";
    let after = "\
export function connectRows(workspace: Workspace) {
  return [
    { id: \"opencode\", label: \"OpenCode\", mode: \"oauth-or-key\", go: workspace.hasGo },
    { id: \"anthropic\", label: \"Anthropic\", mode: \"oauth\" },
  ]
}
";

    let mut diff = RuneSurfaceActor::declare(
        &mut scene,
        "code-diff",
        RuneSurfacePlan::diff_files(vec![RuneDiffFile {
            path: "packages/app/src/dialog-connect.ts".into(),
            before: before.into(),
            after: after.into(),
        }])
        .titled("packages/app/src/dialog-connect.ts")
        .box_size(760.0, 340.0)
        .at([960.0, 520.0]),
    )?;
    diff.settle_in(&mut scene, millis(150));
    diff.commit(
        &mut scene,
        seconds(1.2),
        "highlight merged row",
        RuneAction::SelectDiffLines {
            selection: Some(RuneDiffSelection {
                file: 0,
                new_side: true,
                start_line: 3,
                end_line: 3,
            }),
        },
    )?;
    diff.commit(
        &mut scene,
        seconds(2.2),
        "add review note",
        RuneAction::SetDiffAnnotations {
            annotations: vec![RuneDiffAnnotation {
                file: 0,
                new_side: true,
                line: 3,
                author: "Kit".into(),
                body: "One OpenCode row handles OAuth sign-in, Go models, and API keys.".into(),
            }],
        },
    )?;
    diff.commit(
        &mut scene,
        seconds(3.3),
        "switch to split diff",
        RuneAction::SetDiffMode {
            mode: RuneDiffMode::Split,
        },
    )?;

    let mut foot = footer(
        &mut scene,
        "foot",
        vec![
            span("Myers word-diff ", Tone::Request),
            span(
                "· line selection, inline review notes, and unified/split toggle",
                Tone::Muted,
            ),
        ],
    )?;
    foot.show(&mut scene, millis(350));
    Ok(scene.finish()?)
}
