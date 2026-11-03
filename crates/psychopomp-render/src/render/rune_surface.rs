//! Compiles a declarative `RuneSurfacePlan` (`Note`, `Diff`, `Kit`) into a
//! deterministic `RuneTimeline<SurfaceState>` with live pose channels and
//! `motion_gate` verification.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use rune::{
    Color, Cx, El, Family, Shadow, Spring, TextEdit, TextStyle, Transition, Tx, col, el, row,
    spacer, text,
};
use rune_kit::diff::{
    Annotation as DiffAnnot, DiffMode, DiffState, DiffView, FileDiff, LineRange, LineRef, Side,
    parse as parse_patch,
};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Divider, Kit, SegmentedControl, TextInput,
};

use psychopomp::rune_surface::{
    KitNode, KitTone, RuneAction, RuneDiffAnnotation, RuneDiffMode, RuneDiffSelection, RuneKind,
    RuneSurfacePlan, RuneTheme,
};

use super::rune_notes::render_note;
use super::rune_timeline::RuneTimeline;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfacePose {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: f32,
}

impl Default for SurfacePose {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            opacity: 1.0,
        }
    }
}

pub(crate) struct SurfaceState {
    pub plan: RuneSurfacePlan,
    pub pose: SurfacePose,
    pub markdown: String,
    pub diff: DiffState,
    pub diff_notes: HashMap<String, (String, String)>,
    pub inputs: HashMap<String, TextEdit>,
    pub root_node: Option<KitNode>,
}

impl SurfaceState {
    pub(crate) fn from_plan(plan: &RuneSurfacePlan) -> Self {
        let mut markdown = String::new();
        let mut diff = DiffState::default();
        let mut diff_notes = HashMap::new();
        let mut inputs = HashMap::new();
        let mut root_node = None;

        match &plan.surface {
            RuneKind::Note { markdown: md, .. } => {
                markdown = md.clone();
            }
            RuneKind::Diff {
                patch,
                files,
                mode,
                selection,
                annotations,
                ..
            } => {
                let parsed = if !patch.trim().is_empty() {
                    parse_patch(patch)
                } else {
                    files
                        .iter()
                        .map(|f| FileDiff::from_texts(&f.path, &f.before, &f.after))
                        .collect()
                };
                diff = DiffState::new(parsed);
                diff.set_mode(to_diff_mode(*mode));
                diff.set_selection(selection.map(to_line_range));
                set_diff_annotations(&mut diff, &mut diff_notes, annotations);
            }
            RuneKind::Kit { root } => {
                collect_inputs(root, &mut inputs);
                root_node = Some(root.clone());
            }
        }

        Self {
            plan: plan.clone(),
            pose: SurfacePose::default(),
            markdown,
            diff,
            diff_notes,
            inputs,
            root_node,
        }
    }

    pub(crate) fn apply(&mut self, action: &RuneAction, tx: &mut Tx) {
        match action {
            RuneAction::SetMarkdown { markdown } => {
                self.markdown = markdown.clone();
            }
            RuneAction::SetDiffMode { mode } => {
                self.diff.set_mode(to_diff_mode(*mode));
            }
            RuneAction::SelectDiffLines { selection } => {
                self.diff.set_selection(selection.map(to_line_range));
            }
            RuneAction::ScrollDiffLine {
                file,
                new_side,
                line,
            } => {
                let side = if *new_side { Side::New } else { Side::Old };
                self.diff
                    .scroll_to_line(*file, LineRef::new(side, *line), tx);
            }
            RuneAction::SetDiffAnnotations { annotations } => {
                set_diff_annotations(&mut self.diff, &mut self.diff_notes, annotations);
            }
            RuneAction::SelectSegment { key, index } => {
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| {
                        if let KitNode::Segmented { selected, tabs, .. } = node {
                            *selected = (*index).min(tabs.len().saturating_sub(1));
                        }
                    });
                }
            }
            RuneAction::SetInput { key, value } => {
                self.inputs.insert(key.clone(), TextEdit::new(value));
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| {
                        if let KitNode::TextInput { value: v, .. } = node {
                            *v = value.clone();
                        }
                    });
                }
            }
            RuneAction::SetVisible { key, visible } => {
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| set_node_visible(node, *visible));
                }
            }
            RuneAction::SetSelected { key, selected } => {
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| {
                        if let KitNode::ListRow { selected: sel, .. } = node {
                            *sel = *selected;
                        }
                    });
                }
            }
            RuneAction::SetBorderTone { key, tone } => {
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| {
                        if let KitNode::Dialog { border_tone, .. } = node {
                            *border_tone = *tone;
                        }
                    });
                }
            }
            RuneAction::SetText { key, text, tone } => {
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| match node {
                        KitNode::Text {
                            text: t, tone: tn, ..
                        } => {
                            *t = text.clone();
                            if let Some(tone) = tone {
                                *tn = *tone;
                            }
                        }
                        KitNode::Badge {
                            label, tone: tn, ..
                        } => {
                            *label = text.clone();
                            if let Some(tone) = tone {
                                *tn = *tone;
                            }
                        }
                        KitNode::Note { markdown, .. } => {
                            *markdown = text.clone();
                        }
                        _ => {}
                    });
                }
            }
            RuneAction::SetRow {
                key,
                title,
                hint,
                tone,
                hint_tone,
                selected,
            } => {
                if let Some(root) = &mut self.root_node {
                    mutate_node(root, key, &mut |node| {
                        if let KitNode::ListRow {
                            title: t,
                            hint: h,
                            tone: tn,
                            hint_tone: htn,
                            selected: sel,
                            ..
                        } = node
                        {
                            if let Some(title) = title {
                                *t = title.clone();
                            }
                            if let Some(hint) = hint {
                                *h = hint.clone();
                            }
                            if let Some(tone) = tone {
                                *tn = *tone;
                            }
                            if let Some(hint_tone) = hint_tone {
                                *htn = *hint_tone;
                            }
                            if let Some(selected) = selected {
                                *sel = *selected;
                            }
                        }
                    });
                }
            }
            RuneAction::Batch { actions } => {
                for sub in actions {
                    self.apply(sub, tx);
                }
            }
        }
    }
}

fn to_diff_mode(mode: RuneDiffMode) -> DiffMode {
    match mode {
        RuneDiffMode::Unified => DiffMode::Unified,
        RuneDiffMode::Split => DiffMode::Split,
    }
}

fn to_line_range(sel: RuneDiffSelection) -> LineRange {
    let side = if sel.new_side { Side::New } else { Side::Old };
    LineRange {
        file: sel.file,
        start: LineRef::new(side, sel.start_line),
        end: LineRef::new(side, sel.end_line),
    }
}

fn set_diff_annotations(
    diff: &mut DiffState,
    notes: &mut HashMap<String, (String, String)>,
    annotations: &[RuneDiffAnnotation],
) {
    notes.clear();
    let mut built = Vec::with_capacity(annotations.len());
    for (idx, a) in annotations.iter().enumerate() {
        let id = format!("note-{idx}");
        let side = if a.new_side { Side::New } else { Side::Old };
        notes.insert(id.clone(), (a.author.clone(), a.body.clone()));
        built.push(DiffAnnot::new(a.file, LineRef::new(side, a.line), id));
    }
    diff.set_annotations(built);
}

fn collect_inputs(node: &KitNode, inputs: &mut HashMap<String, TextEdit>) {
    match node {
        KitNode::TextInput { key, value, .. } => {
            inputs.insert(key.clone(), TextEdit::new(value));
        }
        KitNode::Dialog { children, .. }
        | KitNode::Card { children, .. }
        | KitNode::Col { children, .. }
        | KitNode::Row { children, .. } => {
            for child in children {
                collect_inputs(child, inputs);
            }
        }
        _ => {}
    }
}

fn node_key(node: &KitNode) -> Option<&str> {
    match node {
        KitNode::Dialog { key, .. }
        | KitNode::Card { key, .. }
        | KitNode::Col { key, .. }
        | KitNode::Row { key, .. }
        | KitNode::Text { key, .. }
        | KitNode::Badge { key, .. }
        | KitNode::Button { key, .. }
        | KitNode::Note { key, .. } => key.as_deref(),
        KitNode::Segmented { key, .. }
        | KitNode::TextInput { key, .. }
        | KitNode::ListRow { key, .. } => Some(key.as_str()),
        KitNode::Spacer | KitNode::Divider => None,
    }
}

fn set_node_visible(node: &mut KitNode, vis: bool) {
    match node {
        KitNode::Dialog { visible, .. }
        | KitNode::Card { visible, .. }
        | KitNode::Col { visible, .. }
        | KitNode::Row { visible, .. }
        | KitNode::Text { visible, .. }
        | KitNode::Badge { visible, .. }
        | KitNode::Button { visible, .. }
        | KitNode::Segmented { visible, .. }
        | KitNode::TextInput { visible, .. }
        | KitNode::ListRow { visible, .. }
        | KitNode::Note { visible, .. } => *visible = vis,
        KitNode::Spacer | KitNode::Divider => {}
    }
}

fn mutate_node(node: &mut KitNode, target_key: &str, f: &mut dyn FnMut(&mut KitNode)) {
    if node_key(node) == Some(target_key) {
        f(node);
    }
    match node {
        KitNode::Dialog { children, .. }
        | KitNode::Card { children, .. }
        | KitNode::Col { children, .. }
        | KitNode::Row { children, .. } => {
            for child in children {
                mutate_node(child, target_key, f);
            }
        }
        _ => {}
    }
}

fn tone_color(tone: KitTone, k: &Kit) -> Color {
    let c = &k.c;
    match tone {
        KitTone::Base => c.text_base,
        KitTone::Muted => c.text_muted,
        KitTone::Accent => c.info.fg,
        KitTone::Success => c.success.fg,
        KitTone::Warning => c.warning.fg,
        KitTone::Danger => c.danger.fg,
    }
}

fn resolve_kit(plan: &RuneSurfacePlan) -> Kit {
    match plan.theme {
        RuneTheme::Opencode => rune_theme_opencode::kit(plan.dark).clone(),
        RuneTheme::Neutral => Kit::neutral(plan.dark),
    }
}

pub(crate) fn surface_view(s: &SurfaceState, _cx: &mut Cx<SurfaceState>) -> El {
    let k = resolve_kit(&s.plan);
    let c = &k.c;
    let scale = s.plan.scale_factor.max(0.25);

    let content = match &s.plan.surface {
        RuneKind::Note { title, width, .. } => {
            let mut card = col()
                .key("note-card")
                .w(*width)
                .padding(20.0)
                .gap(14.0)
                .radius(12.0)
                .fill(c.layer_01)
                .border(1.0, c.border_base)
                .box_shadow(Shadow {
                    y: 14.0,
                    blur: 32.0,
                    spread: 0.0,
                    color: Color::BLACK.alpha(0.36),
                    ..Default::default()
                });
            if let Some(title) = title {
                card = card
                    .child(
                        text(title)
                            .key("note-title")
                            .font(TextStyle::new(16.0, 620.0))
                            .color(c.text_base),
                    )
                    .child(Divider::horizontal().build(&k));
            }
            card.child(render_note("note-body", &s.markdown, &k))
        }
        RuneKind::Diff {
            title,
            width,
            height,
            ..
        } => {
            let mut frame = col()
                .key("diff-card")
                .w(*width)
                .h(*height)
                .radius(12.0)
                .clip_rounded(12.0)
                .fill(c.layer_01)
                .border(1.0, c.border_base)
                .box_shadow(Shadow {
                    y: 14.0,
                    blur: 32.0,
                    spread: 0.0,
                    color: Color::BLACK.alpha(0.36),
                    ..Default::default()
                });
            if let Some(title) = title {
                frame = frame
                    .child(
                        row()
                            .key("diff-hdr")
                            .w_full()
                            .px(14.0)
                            .py(9.0)
                            .center_y()
                            .fill(c.layer_02)
                            .child(
                                text(title)
                                    .font(TextStyle {
                                        family: Family::Mono,
                                        ..TextStyle::new(12.5, 540.0)
                                    })
                                    .color(c.text_base),
                            )
                            .child(spacer())
                            .child({
                                let mode_label = match s.diff.mode() {
                                    DiffMode::Unified => "unified",
                                    DiffMode::Split => "split",
                                };
                                Badge::new(mode_label)
                                    .build(&k)
                                    .key(format!("mode-{mode_label}"))
                                    .motion(Spring::new(0.26, 0.0))
                                    .transition(Transition::blur_replace())
                            }),
                    )
                    .child(el().w_full().h(1.0).fill(c.border_weak));
            }
            let k_note = k.clone();
            let diff_el = DiffView::new(
                |st: &SurfaceState| &st.diff,
                |st: &mut SurfaceState| &mut st.diff,
            )
            .annotation(move |st: &SurfaceState, ann| {
                let (author, body) = st.diff_notes.get(&ann.id).cloned().unwrap_or_default();
                let cn = &k_note.c;
                row()
                    .key(format!("annot-{}", ann.id))
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
                                text(author)
                                    .font(TextStyle::new(11.5, 600.0))
                                    .color(cn.info.fg),
                            )
                            .child(text(body).wrap().font(k_note.ty.ui).color(cn.text_base)),
                    )
            })
            .build(s, &k);
            frame.child(
                el().key("diff-body")
                    .w_full()
                    .grow(1.0)
                    .shrink()
                    .child(diff_el),
            )
        }
        RuneKind::Kit { .. } => s
            .root_node
            .as_ref()
            .and_then(|root| build_kit_node(root, s, &k))
            .unwrap_or_else(|| el().key("empty")),
    };

    // Place the surface centered at `origin + (pose.x, pose.y)` in canvas pixels
    // by offsetting from the centered viewport `(size / 2)`.
    let default_center = [s.plan.size[0] * 0.5, s.plan.size[1] * 0.5];
    let dx_pt = (s.plan.origin[0] - default_center[0] + s.pose.x) / scale;
    let dy_pt = (s.plan.origin[1] - default_center[1] + s.pose.y) / scale;

    let positioned = el()
        .key("surface-anchor")
        .offset(dx_pt, dy_pt)
        .scale(s.pose.scale)
        .opacity(s.pose.opacity.clamp(0.0, 1.0))
        .child(content);

    col()
        .key("surface-canvas")
        .w_full()
        .h_full()
        .fill(if s.plan.opaque_backdrop {
            c.bg_deep
        } else {
            Color::TRANSPARENT
        })
        .center()
        .child(positioned)
}

fn build_kit_node(node: &KitNode, s: &SurfaceState, k: &Kit) -> Option<El> {
    let c = &k.c;
    match node {
        KitNode::Dialog {
            key,
            width,
            padding,
            gap,
            border_tone,
            visible,
            children,
        } => {
            if !*visible {
                return None;
            }
            let border = border_tone
                .map(|t| tone_color(t, k))
                .unwrap_or(c.border_base);
            let mut d = col()
                .padding(*padding)
                .gap(*gap)
                .radius(12.0)
                .fill(c.layer_01)
                .border(1.0, border)
                .box_shadow(Shadow {
                    y: 16.0,
                    blur: 36.0,
                    spread: 0.5,
                    color: Color::BLACK.alpha(0.38),
                    ..Default::default()
                })
                .motion(Spring::new(0.36, 0.0))
                .transition(Transition::opacity());
            if let Some(k_str) = key {
                d = d.key(k_str);
            }
            if let Some(w) = width {
                d = d.w(*w);
            }
            for ch in children {
                if let Some(child_el) = build_kit_node(ch, s, k) {
                    d = d.child(child_el);
                }
            }
            Some(d)
        }
        KitNode::Card {
            key,
            title,
            width,
            visible,
            children,
            ..
        } => {
            if !*visible {
                return None;
            }
            let mut card = Card::new();
            if let Some(t) = title {
                card = card.title(t, None);
            }
            for ch in children {
                if let Some(child_el) = build_kit_node(ch, s, k) {
                    card = card.child(child_el);
                }
            }
            let mut built = card.build(k);
            if let Some(k_str) = key {
                built = built.key(k_str);
            }
            if let Some(w) = width {
                built = el().w(*w).child(built);
            }
            Some(built)
        }
        KitNode::Col {
            key,
            width,
            gap,
            padding,
            visible,
            children,
        } => {
            if !*visible {
                return None;
            }
            let mut container = col()
                .w_full()
                .gap(*gap)
                .padding(*padding)
                .motion(Spring::new(0.36, 0.0))
                .transition(Transition::opacity());
            if let Some(k_str) = key {
                container = container.key(k_str);
            }
            if let Some(w) = width {
                container = container.w(*w);
            }
            for ch in children {
                if let Some(child_el) = build_kit_node(ch, s, k) {
                    container = container.child(child_el);
                }
            }
            Some(container)
        }
        KitNode::Row {
            key,
            width,
            gap,
            padding,
            center_y,
            visible,
            children,
        } => {
            if !*visible {
                return None;
            }
            let mut container = row()
                .w_full()
                .gap(*gap)
                .padding(*padding)
                .motion(Spring::new(0.36, 0.0))
                .transition(Transition::opacity());
            if *center_y {
                container = container.center_y();
            }
            if let Some(k_str) = key {
                container = container.key(k_str);
            }
            if let Some(w) = width {
                container = container.w(*w);
            }
            for ch in children {
                if let Some(child_el) = build_kit_node(ch, s, k) {
                    container = container.child(child_el);
                }
            }
            Some(container)
        }
        KitNode::Spacer => Some(spacer()),
        KitNode::Divider => Some(Divider::horizontal().build(k)),
        KitNode::Text {
            key,
            text: content,
            size,
            weight,
            tone,
            mono,
            ride_trailing,
            visible,
        } => {
            if !*visible {
                return None;
            }
            let mut font = TextStyle::new(*size, *weight);
            if *mono {
                font.family = Family::Mono;
            }
            let mut t = text(content)
                .font(font)
                .color(tone_color(*tone, k))
                .motion(Spring::new(0.32, 0.0))
                .transition(Transition::opacity());
            if *ride_trailing {
                t = t.ride_trailing();
            }
            if let Some(k_str) = key {
                t = t.key(k_str);
            }
            Some(t)
        }
        KitNode::Badge {
            key,
            label,
            tone,
            visible,
        } => {
            if !*visible {
                return None;
            }
            let variant = match tone {
                KitTone::Accent => BadgeVariant::Accent,
                KitTone::Muted => BadgeVariant::Subtle,
                _ => BadgeVariant::Neutral,
            };
            let mut b = Badge::new(label).variant(variant).build(k);
            if let Some(k_str) = key {
                b = b.key(k_str);
            }
            Some(b)
        }
        KitNode::Button {
            key,
            label,
            primary,
            visible,
        } => {
            if !*visible {
                return None;
            }
            let variant = if *primary {
                ButtonVariant::Submit
            } else {
                ButtonVariant::Neutral
            };
            let mut b = Button::new(label).variant(variant).build(k);
            if let Some(k_str) = key {
                b = b.key(k_str);
            }
            Some(b)
        }
        KitNode::Segmented {
            key,
            tabs,
            selected,
            width,
            static_pill,
            visible,
        } => {
            if !*visible {
                return None;
            }
            if *static_pill {
                // Static pre-fix toggle mode (used in Before/After wipes to contrast
                // against SegmentedControl's sliding spring indicator).
                let mut bar = row()
                    .key(key)
                    .h(28.0)
                    .padding(2.0)
                    .gap(2.0)
                    .radius(6.0)
                    .fill(c.layer_02)
                    .border(1.0, c.border_weak);
                if let Some(w) = width {
                    bar = bar.w(*w);
                }
                for (idx, tab) in tabs.iter().enumerate() {
                    let active = idx == *selected;
                    bar = bar.child(
                        row()
                            .key(format!("segment{idx}"))
                            .grow(1.0)
                            .h_full()
                            .radius(4.5)
                            .center()
                            .fill(if active {
                                c.layer_01
                            } else {
                                Color::TRANSPARENT
                            })
                            .child(
                                text(tab)
                                    .font(TextStyle::new(12.0, 520.0))
                                    .color(if active { c.text_base } else { c.text_muted }),
                            ),
                    );
                }
                return Some(bar);
            }
            let refs: Vec<&str> = tabs.iter().map(String::as_str).collect();
            Some(
                SegmentedControl::new(&refs, Some(*selected))
                    .width(*width)
                    .build(k)
                    .key(key),
            )
        }
        KitNode::TextInput {
            key,
            value,
            placeholder,
            visible,
        } => {
            if !*visible {
                return None;
            }
            let fallback = TextEdit::new(value);
            let edit = s.inputs.get(key).unwrap_or(&fallback);
            Some(
                TextInput::new(edit)
                    .placeholder(placeholder)
                    .build(k)
                    .key(key),
            )
        }
        KitNode::ListRow {
            key,
            title,
            hint,
            tone,
            hint_tone,
            selected,
            visible,
        } => {
            if !*visible {
                return None;
            }
            let item = row()
                .key(key)
                .w_full()
                .h(38.0)
                .px(12.0)
                .gap(12.0)
                .radius(6.0)
                .center_y()
                .fill(if *selected {
                    c.layer_02
                } else {
                    Color::TRANSPARENT
                })
                .motion(Spring::new(0.38, 0.0))
                .transition(Transition::opacity())
                .child(
                    text(title)
                        .key(format!("name-{title}"))
                        .font(TextStyle::new(13.5, 520.0))
                        .color(tone_color(*tone, k))
                        .motion(Spring::new(0.26, 0.0))
                        .transition(Transition::blur_replace()),
                )
                .child(spacer())
                .child(
                    text(hint)
                        .key(format!("hint-{hint}"))
                        .ride_trailing()
                        .font(k.ty.ui)
                        .color(tone_color(*hint_tone, k))
                        .motion(Spring::new(0.26, 0.0))
                        .transition(Transition::blur_replace()),
                );
            Some(item)
        }
        KitNode::Note {
            key,
            markdown,
            visible,
        } => {
            if !*visible {
                return None;
            }
            let k_str = key.as_deref().unwrap_or("kit-note");
            Some(render_note(k_str, markdown, k))
        }
    }
}

pub(crate) fn build_surface_timeline(plan: &RuneSurfacePlan) -> RuneTimeline<SurfaceState> {
    let px_size = [
        plan.size[0].round().max(1.0) as u32,
        plan.size[1].round().max(1.0) as u32,
    ];
    let plan_for_factory = plan.clone();
    let mut tl = RuneTimeline::new_with_factory(
        move || SurfaceState::from_plan(&plan_for_factory),
        surface_view,
        px_size,
        plan.scale_factor,
        plan.dark,
    )
    .with_transparent(!plan.opaque_backdrop);
    for commit in &plan.commits {
        let action = commit.action.clone();
        let label = if commit.label.is_empty() {
            "commit".to_owned()
        } else {
            commit.label.clone()
        };
        tl.commit(commit.at_nanos, label, move |state, tx| {
            state.apply(&action, tx);
        });
    }
    tl
}

const MAX_CACHED_SURFACES: usize = 16;

thread_local! {
    static SURFACE_CACHE: RefCell<VecDeque<(u64, RuneTimeline<SurfaceState>)>> =
        const { RefCell::new(VecDeque::new()) };
}

pub(crate) fn with_cached_surface_timeline<R>(
    uid: u64,
    plan: &RuneSurfacePlan,
    f: impl FnOnce(&mut RuneTimeline<SurfaceState>) -> R,
) -> R {
    SURFACE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some(pos) = cache.iter().position(|(id, _)| *id == uid) {
            let mut entry = cache.remove(pos).expect("entry exists");
            let out = f(&mut entry.1);
            cache.push_front(entry);
            return out;
        }
        let mut tl = build_surface_timeline(plan);
        let out = f(&mut tl);
        if cache.len() >= MAX_CACHED_SURFACES {
            cache.pop_back();
        }
        cache.push_front((uid, tl));
        out
    })
}

fn plan_uid(plan: &RuneSurfacePlan) -> u64 {
    let bytes = serde_json::to_vec(plan).unwrap_or_default();
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in &bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // Keep high bit set so plan hashes never collide with `NEXT_SURFACE_UID` counters.
    h | (1_u64 << 63)
}

pub(crate) fn with_plan_timeline<R>(
    plan: &RuneSurfacePlan,
    f: impl FnOnce(&mut RuneTimeline<SurfaceState>) -> R,
) -> R {
    with_cached_surface_timeline(plan_uid(plan), plan, f)
}

pub(crate) fn rune_card_child_rect(
    plan: &RuneSurfacePlan,
    time: f64,
    key: &str,
) -> Option<[f32; 4]> {
    let base = plan_uid(plan);
    let slots = [base ^ (1_u64 << 62), base ^ (1_u64 << 61)];
    let chosen = SURFACE_CACHE.with(|cell| {
        let cache = cell.borrow();
        let mut best: Option<(u64, f64)> = None;
        for &slot in &slots {
            if let Some((_, tl)) = cache.iter().find(|(id, _)| *id == slot) {
                let cur = tl.current_time();
                if cur <= time + 1.0e-12 && best.is_none_or(|(_, b)| cur > b) {
                    best = Some((slot, cur));
                }
            } else if best.is_none() {
                return slot;
            }
        }
        best.map(|(slot, _)| slot).unwrap_or(slots[0])
    });
    with_cached_surface_timeline(chosen, plan, |tl| {
        sync_surface_pose(tl, time, SurfacePose::default());
        tl.presented_px(time, key).map(|(rect, _)| rect)
    })
}

pub(crate) fn sync_surface_pose(tl: &mut RuneTimeline<SurfaceState>, time: f64, pose: SurfacePose) {
    let rt = tl.sample_at(time);
    if rt.state.pose != pose {
        rt.update(|state, _| state.pose = pose);
        rt.frame(0.0);
    }
}
