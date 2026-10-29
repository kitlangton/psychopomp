//! Component playground stories for `psychopomp-studio` (`Surfaces/Connect
//! Dialog`, `Notes/Seer Primitives`, and `Code/DiffView`), sharing live `⌘⇧K`
//! knobs and deterministic `R` replay triggers.

use psychopomp_render::render_seer_note;
use rune::{
    Color, Cx, El, Family, Play, Playground, Shadow, Spring, Stage, Story, TextEdit, TextStyle,
    Transition, col, el, row, spacer, text,
};
use rune_kit::diff::{
    Annotation as DiffAnnot, DiffMode, DiffState, DiffView, FileDiff, LineRange, LineRef, Side,
};
use rune_kit::{Badge, Divider, SegmentedControl, TextInput};
use rune_knobs::Knobs;

use crate::{
    CARD_PAD, CARD_RADIUS, CARD_WIDTH, SPRING_BOUNCE, SPRING_SECS, STORY_GROUPS, StudioState,
    THEME_LOOK, kit_for_knobs, knob_style, studio_init, studio_view,
};

pub(crate) struct ConnectStory {
    pub knobs: Knobs,
    pub tab: usize,
    pub show_go: bool,
    pub renamed: bool,
    pub search: TextEdit,
}

impl Default for ConnectStory {
    fn default() -> Self {
        Self {
            knobs: Knobs::default(),
            tab: 0,
            show_go: true,
            renamed: false,
            search: TextEdit::new(""),
        }
    }
}

pub(crate) fn connect_card_el(
    knobs: &Knobs,
    tab: usize,
    show_go: bool,
    renamed: bool,
    search: &TextEdit,
) -> El {
    let k = kit_for_knobs(knobs);
    let c = &k.c;
    let width = knobs.get(&CARD_WIDTH);
    let pad = knobs.get(&CARD_PAD);
    let radius = knobs.get(&CARD_RADIUS);
    let spring = Spring::new(knobs.get(&SPRING_SECS), knobs.get(&SPRING_BOUNCE));

    let tabs = ["Cost", "Tokens", "Web Search"];
    let rows: &[(&str, &str, &str, bool)] = &[
        ("go", "OpenCode Go", "API key only", true),
        (
            "console",
            if renamed {
                "OpenCode"
            } else {
                "OpenCode Console"
            },
            if renamed {
                "sign in or API key · Go included"
            } else {
                "sign in or API key"
            },
            false,
        ),
        ("anthropic", "Anthropic", "Claude Max or API key", false),
        ("openai", "OpenAI", "ChatGPT Plus/Pro or API key", false),
    ];

    let mut list = col().key("rows").w_full().gap(4.0);
    for &(id, title, hint, warn) in rows {
        if id == "go" && !show_go {
            continue;
        }
        let selected = id == "console";
        list = list.child(
            row()
                .key(id)
                .w_full()
                .h(38.0)
                .px(12.0)
                .radius(6.0)
                .center_y()
                .fill(if selected {
                    c.layer_02
                } else {
                    Color::TRANSPARENT
                })
                .motion(spring)
                .transition(Transition::opacity())
                .child(
                    text(title)
                        .key("name")
                        .font(TextStyle::new(13.5, 520.0))
                        .color(if warn { c.danger.fg } else { c.text_base }),
                )
                .child(spacer())
                .child(text(hint).key("hint").ride_trailing().font(k.ty.ui).color(
                    if selected && renamed {
                        c.success.fg
                    } else {
                        c.text_muted
                    },
                )),
        );
    }

    col()
        .key("surface-card")
        .w(width)
        .padding(pad)
        .gap(13.0)
        .radius(radius)
        .fill(c.layer_01)
        .border(1.0, c.border_base)
        .box_shadow(Shadow {
            y: 16.0,
            blur: 34.0,
            spread: 0.0,
            color: Color::BLACK.alpha(0.36),
            ..Default::default()
        })
        .motion(spring)
        .child(
            row()
                .key("hdr")
                .w_full()
                .center_y()
                .child(
                    text("Connect an integration")
                        .font(TextStyle::new(15.0, 600.0))
                        .color(c.text_base),
                )
                .child(spacer())
                .child(Badge::new("esc").build(&k)),
        )
        .child(
            TextInput::new(search)
                .placeholder("Search 230 providers…")
                .build(&k),
        )
        .child(
            row()
                .key("seg-row")
                .w_full()
                .center_y()
                .child(text("Usage view").font(k.ty.ui).color(c.text_muted))
                .child(spacer())
                .child(
                    SegmentedControl::new(&tabs, Some(tab))
                        .width(Some((width - pad * 2.0 - 96.0).clamp(220.0, 340.0)))
                        .build(&k)
                        .key("seg"),
                ),
        )
        .child(
            text("Popular")
                .font(TextStyle::new(11.5, 560.0))
                .color(c.text_muted),
        )
        .child(list)
}

fn connect_story_view(s: &ConnectStory, _cx: &mut Cx<ConnectStory>) -> El {
    let k = kit_for_knobs(&s.knobs);
    col()
        .key("stage-connect")
        .w_full()
        .h_full()
        .fill(k.c.bg_deep)
        .center()
        .child(connect_card_el(
            &s.knobs, s.tab, s.show_go, s.renamed, &s.search,
        ))
}

fn connect_story_side(s: &ConnectStory, active: bool, cx: &mut Cx<ConnectStory>) -> El {
    rune_knobs::inline(
        &s.knobs,
        STORY_GROUPS,
        knob_style(cx.system_dark()),
        |st: &mut ConnectStory| &mut st.knobs,
        active,
    )
}

fn connect_replay(s: &mut ConnectStory, play: &mut Play<'_, ConnectStory>) {
    s.tab = 0;
    s.show_go = true;
    s.renamed = false;
    play.after(0.35, |s, _| s.tab = 1);
    play.after(0.80, |s, _| {
        s.show_go = false;
        s.renamed = true;
    });
    play.after(1.25, |s, _| s.tab = 2);
}

struct NoteStory {
    knobs: Knobs,
    show_steps: bool,
}

impl Default for NoteStory {
    fn default() -> Self {
        Self {
            knobs: Knobs::default(),
            show_steps: true,
        }
    }
}

fn note_story_view(s: &NoteStory, _cx: &mut Cx<NoteStory>) -> El {
    let k = kit_for_knobs(&s.knobs);
    let c = &k.c;
    let base = "\
Mount real Rune views on Psychopomp's `wgpu` device with GPU `Rgba16Float` shutter exposure.

```facts
Shutter (24x): {verified} 11.9 ms on Apple M2 Max
Determinism: {verified} 0 differing pixels out of order
```

```compare
Before: 597 LOC of hand-placed Stage rects and duplicate labels.
After: 55 LOC of real @opencode/ui controls with zero coordinates.
```";
    let md = if s.show_steps {
        format!(
            "{base}\n\n```steps\n{{built}} Shared Rgba16Float shutter · 11.9 ms\n{{built}} RuneSurface + AnchorPlan::Rune · 0 px lag\n{{built}} psychopomp-studio · playground + ⌘⇧K knobs\n```"
        )
    } else {
        base.to_owned()
    };

    let card = col()
        .key("surface-card")
        .w(s.knobs.get(&CARD_WIDTH) + 90.0)
        .padding(s.knobs.get(&CARD_PAD))
        .gap(12.0)
        .radius(s.knobs.get(&CARD_RADIUS))
        .fill(c.layer_01)
        .border(1.0, c.border_base)
        .child(
            text("t782 · Rune surfaces in Psychopomp")
                .font(TextStyle::new(15.5, 620.0))
                .color(c.text_base),
        )
        .child(Divider::horizontal().build(&k))
        .child(render_seer_note("story-note", &md, &k));

    col()
        .key("stage-note")
        .w_full()
        .h_full()
        .fill(c.bg_deep)
        .center()
        .child(card)
}

fn note_story_side(s: &NoteStory, active: bool, cx: &mut Cx<NoteStory>) -> El {
    rune_knobs::inline(
        &s.knobs,
        STORY_GROUPS,
        knob_style(cx.system_dark()),
        |st: &mut NoteStory| &mut st.knobs,
        active,
    )
}

fn note_replay(s: &mut NoteStory, play: &mut Play<'_, NoteStory>) {
    s.show_steps = false;
    play.after(0.45, |s, _| s.show_steps = true);
}

struct DiffStory {
    knobs: Knobs,
    diff: DiffState,
    commented: bool,
}

fn diff_story_init() -> DiffStory {
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
    let mut diff = DiffState::new(vec![FileDiff::from_texts(
        "packages/app/src/dialog-connect.ts",
        before,
        after,
    )]);
    diff.set_selection(Some(LineRange::line(0, LineRef::new(Side::New, 3))));
    DiffStory {
        knobs: Knobs::default(),
        diff,
        commented: true,
    }
}

fn sync_diff_comment(s: &mut DiffStory) {
    if s.commented {
        s.diff.set_annotations(vec![DiffAnnot::new(
            0,
            LineRef::new(Side::New, 3),
            "review",
        )]);
    } else {
        s.diff.set_annotations(Vec::new());
    }
}

fn diff_story_view(s: &DiffStory, _cx: &mut Cx<DiffStory>) -> El {
    let k = kit_for_knobs(&s.knobs);
    let c = &k.c;
    let width = (s.knobs.get(&CARD_WIDTH) + 210.0).min(780.0);
    let radius = s.knobs.get(&CARD_RADIUS);
    let k_ann = k.clone();

    let diff_el =
        DiffView::new(|st: &DiffStory| &st.diff, |st: &mut DiffStory| &mut st.diff)
            .annotation(move |_st: &DiffStory, _ann| {
                let cn = &k_ann.c;
                row()
            .w_full()
            .px(14.0)
            .py(8.0)
            .gap(10.0)
            .fill(cn.info.fg.alpha(0.09))
            .child(el().w(2.5).h_full().radius(1.0).fill(cn.info.fg))
            .child(
                col()
                    .grow(1.0)
                    .shrink()
                    .gap(2.0)
                    .child(
                        text("Kit")
                            .font(TextStyle::new(11.5, 600.0))
                            .color(cn.info.fg),
                    )
                    .child(
                        text("One OpenCode row handles OAuth sign-in, Go models, and API keys.")
                            .wrap()
                            .font(k_ann.ty.ui)
                            .color(cn.text_base),
                    ),
            )
            })
            .build(s, &k);

    let mode_label = match s.diff.mode() {
        DiffMode::Unified => "unified",
        DiffMode::Split => "split",
    };

    let card = col()
        .key("surface-card")
        .w(width)
        .h(320.0)
        .radius(radius)
        .clip_rounded(radius)
        .fill(c.layer_01)
        .border(1.0, c.border_base)
        .child(
            row()
                .w_full()
                .px(14.0)
                .py(9.0)
                .center_y()
                .fill(c.layer_02)
                .child(
                    text("packages/app/src/dialog-connect.ts")
                        .font(TextStyle {
                            family: Family::Mono,
                            ..TextStyle::new(12.5, 540.0)
                        })
                        .color(c.text_base),
                )
                .child(spacer())
                .child(Badge::new(mode_label).build(&k)),
        )
        .child(el().w_full().h(1.0).fill(c.border_weak))
        .child(el().w_full().grow(1.0).shrink().child(diff_el));

    col()
        .key("stage-diff")
        .w_full()
        .h_full()
        .fill(c.bg_deep)
        .center()
        .child(card)
}

fn diff_story_side(s: &DiffStory, active: bool, cx: &mut Cx<DiffStory>) -> El {
    rune_knobs::inline(
        &s.knobs,
        STORY_GROUPS,
        knob_style(cx.system_dark()),
        |st: &mut DiffStory| &mut st.knobs,
        active,
    )
}

fn diff_replay(s: &mut DiffStory, play: &mut Play<'_, DiffStory>) {
    s.diff.set_mode(DiffMode::Unified);
    s.diff.set_selection(None);
    s.commented = false;
    sync_diff_comment(s);
    play.after(0.30, |s, _| {
        s.diff
            .set_selection(Some(LineRange::line(0, LineRef::new(Side::New, 3))));
    });
    play.after(0.70, |s, _| {
        s.commented = true;
        sync_diff_comment(s);
    });
    play.after(1.15, |s, _| s.diff.set_mode(DiffMode::Split));
}

pub(crate) fn build_playground() -> Playground<StudioState> {
    Playground::<StudioState>::new()
        .story(Story::new("Windows/Studio Shell", studio_init, studio_view).stage(Stage::Fill))
        .story(
            Story::new(
                "Surfaces/Connect Dialog",
                ConnectStory::default,
                connect_story_view,
            )
            .side_keys(
                connect_story_side,
                rune_knobs::side_key(STORY_GROUPS, |s: &mut ConnectStory| &mut s.knobs),
            )
            .trigger('r', "Replay connect choreography", connect_replay)
            .trigger('g', "Toggle Go row", |s, _| {
                s.show_go = !s.show_go;
                s.renamed = !s.show_go;
            })
            .trigger('t', "Cycle tab", |s, _| s.tab = (s.tab + 1) % 3)
            .variant("Initial", |_| {})
            .variant("Merged OpenCode", |s| {
                s.tab = 1;
                s.show_go = false;
                s.renamed = true;
            })
            .variant("Neutral theme", |s| {
                s.knobs.set(&THEME_LOOK, 1.0);
            }),
        )
        .story(
            Story::new("Notes/Seer Primitives", NoteStory::default, note_story_view)
                .side_keys(
                    note_story_side,
                    rune_knobs::side_key(STORY_GROUPS, |s: &mut NoteStory| &mut s.knobs),
                )
                .trigger('r', "Replay steps reveal", note_replay)
                .trigger('s', "Toggle steps", |s, _| s.show_steps = !s.show_steps)
                .variant("Full note", |_| {})
                .variant("Before steps", |s| s.show_steps = false),
        )
        .story(
            Story::new(
                "Code/DiffView",
                || {
                    let mut st = diff_story_init();
                    sync_diff_comment(&mut st);
                    st
                },
                diff_story_view,
            )
            .side_keys(
                diff_story_side,
                rune_knobs::side_key(STORY_GROUPS, |s: &mut DiffStory| &mut s.knobs),
            )
            .trigger('r', "Replay diff walkthrough", diff_replay)
            .trigger('m', "Toggle Split/Unified", |s, _| {
                let next = match s.diff.mode() {
                    DiffMode::Unified => DiffMode::Split,
                    DiffMode::Split => DiffMode::Unified,
                };
                s.diff.set_mode(next);
            })
            .variant("Unified", |_| {})
            .variant("Split with note", |s| s.diff.set_mode(DiffMode::Split)),
        )
        .group_order(&["Surfaces", "Notes", "Code", "Windows"])
        .sizes(&[("Compact", (680.0, 480.0)), ("1080p Half", (960.0, 540.0))])
}
