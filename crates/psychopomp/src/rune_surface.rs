//! Declarative `RuneSurface` recipe (`recipe: "rune-surface"`), authoring handle
//! (`RuneSurfaceActor`), `score::RuneSurface` beat handle, and `@opencode/ui`
//! (`rune-kit`) / Seer note / `DiffView` builders.
//!
//! Keeps `ScenePlan` JSON renderer-independent and inspectable by `verify`,
//! while mounting real Rune components onto `psychopomp-render`'s shared `wgpu`
//! device and `Rgba16Float` shutter accumulator.

use std::{cell::RefCell, rc::Rc};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    author::{ActorHandle, ContinuousHandle, PlanBuilder, whole_millis},
    math::easing::Ease,
    plan::SpringPlan,
    score::{Beat, Span},
};

pub const RUNE_SURFACE_RECIPE: &str = "rune-surface";
pub const RUNE_SURFACE_CHANNELS: &[&str] = &["opacity", "x", "y", "scale"];

fn default_origin() -> [f32; 2] {
    [960.0, 540.0]
}

fn default_size() -> [f32; 2] {
    [1920.0, 1080.0]
}

fn default_scale_factor() -> f32 {
    2.0
}

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_default_origin(v: &[f32; 2]) -> bool {
    *v == default_origin()
}

fn is_default_size(v: &[f32; 2]) -> bool {
    *v == default_size()
}

fn is_default_scale(v: &f32) -> bool {
    (*v - default_scale_factor()).abs() <= f32::EPSILON
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuneTheme {
    #[default]
    Opencode,
    Neutral,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuneDiffMode {
    #[default]
    Unified,
    Split,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KitTone {
    #[default]
    Base,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneDiffFile {
    pub path: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneDiffSelection {
    #[serde(default)]
    pub file: usize,
    #[serde(default = "default_true")]
    pub new_side: bool,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneDiffAnnotation {
    #[serde(default)]
    pub file: usize,
    #[serde(default = "default_true")]
    pub new_side: bool,
    pub line: u32,
    pub author: String,
    pub body: String,
}

/// Composable `@opencode/ui` (`rune-kit`) widget tree for `RuneKind::Kit`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum KitNode {
    Dialog {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default = "default_dialog_pad")]
        padding: f32,
        #[serde(default = "default_dialog_gap")]
        gap: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        border_tone: Option<KitTone>,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
        children: Vec<KitNode>,
    },
    Card {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default = "default_card_pad")]
        padding: f32,
        #[serde(default = "default_card_gap")]
        gap: f32,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
        children: Vec<KitNode>,
    },
    Col {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default)]
        gap: f32,
        #[serde(default)]
        padding: f32,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
        children: Vec<KitNode>,
    },
    Row {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default)]
        gap: f32,
        #[serde(default)]
        padding: f32,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        center_y: bool,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
        children: Vec<KitNode>,
    },
    Spacer,
    Divider,
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        text: String,
        #[serde(default = "default_text_size")]
        size: f32,
        #[serde(default = "default_text_weight")]
        weight: f32,
        #[serde(default)]
        tone: KitTone,
        #[serde(default, skip_serializing_if = "is_false")]
        mono: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        ride_trailing: bool,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
    Badge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        label: String,
        #[serde(default)]
        tone: KitTone,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
    Button {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        label: String,
        #[serde(default, skip_serializing_if = "is_false")]
        primary: bool,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
    Segmented {
        key: String,
        tabs: Vec<String>,
        #[serde(default)]
        selected: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default, skip_serializing_if = "is_false")]
        static_pill: bool,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
    TextInput {
        key: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
    ListRow {
        key: String,
        title: String,
        #[serde(default)]
        hint: String,
        #[serde(default)]
        tone: KitTone,
        #[serde(default = "default_muted_tone")]
        hint_tone: KitTone,
        #[serde(default, skip_serializing_if = "is_false")]
        selected: bool,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
    Note {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        markdown: String,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        visible: bool,
    },
}

fn default_dialog_pad() -> f32 {
    18.0
}
fn default_dialog_gap() -> f32 {
    12.0
}
fn default_card_pad() -> f32 {
    16.0
}
fn default_card_gap() -> f32 {
    10.0
}
fn default_text_size() -> f32 {
    13.5
}
fn default_text_weight() -> f32 {
    450.0
}
fn default_muted_tone() -> KitTone {
    KitTone::Muted
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "recipe", rename_all = "kebab-case")]
pub enum RuneKind {
    Note {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        markdown: String,
        #[serde(default = "default_note_width")]
        width: f32,
    },
    Diff {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        patch: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<RuneDiffFile>,
        #[serde(default)]
        mode: RuneDiffMode,
        #[serde(default = "default_diff_width")]
        width: f32,
        #[serde(default = "default_diff_height")]
        height: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<RuneDiffSelection>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<RuneDiffAnnotation>,
    },
    Kit {
        root: KitNode,
    },
}

fn default_note_width() -> f32 {
    560.0
}
fn default_diff_width() -> f32 {
    720.0
}
fn default_diff_height() -> f32 {
    380.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum RuneAction {
    SetMarkdown {
        markdown: String,
    },
    SetDiffMode {
        mode: RuneDiffMode,
    },
    SelectDiffLines {
        selection: Option<RuneDiffSelection>,
    },
    ScrollDiffLine {
        #[serde(default)]
        file: usize,
        #[serde(default = "default_true")]
        new_side: bool,
        line: u32,
    },
    SetDiffAnnotations {
        annotations: Vec<RuneDiffAnnotation>,
    },
    SelectSegment {
        key: String,
        index: usize,
    },
    SetInput {
        key: String,
        value: String,
    },
    SetVisible {
        key: String,
        visible: bool,
    },
    SetSelected {
        key: String,
        selected: bool,
    },
    SetBorderTone {
        key: String,
        tone: Option<KitTone>,
    },
    SetText {
        key: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<KitTone>,
    },
    SetRow {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<KitTone>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint_tone: Option<KitTone>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<bool>,
    },
    Batch {
        actions: Vec<RuneAction>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneCommitPlan {
    pub at_nanos: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub action: RuneAction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneSurfacePlan {
    #[serde(default = "default_origin", skip_serializing_if = "is_default_origin")]
    pub origin: [f32; 2],
    #[serde(default = "default_size", skip_serializing_if = "is_default_size")]
    pub size: [f32; 2],
    #[serde(
        default = "default_scale_factor",
        skip_serializing_if = "is_default_scale"
    )]
    pub scale_factor: f32,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub dark: bool,
    #[serde(default)]
    pub theme: RuneTheme,
    #[serde(default, skip_serializing_if = "is_false")]
    pub opaque_backdrop: bool,
    pub surface: RuneKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<RuneCommitPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_motion: Vec<[String; 4]>,
}

impl RuneSurfacePlan {
    pub fn note(markdown: impl Into<String>) -> Self {
        Self::new(RuneKind::Note {
            title: None,
            markdown: markdown.into(),
            width: default_note_width(),
        })
    }

    pub fn diff_patch(patch: impl Into<String>) -> Self {
        Self::new(RuneKind::Diff {
            title: None,
            patch: patch.into(),
            files: Vec::new(),
            mode: RuneDiffMode::Unified,
            width: default_diff_width(),
            height: default_diff_height(),
            selection: None,
            annotations: Vec::new(),
        })
    }

    pub fn diff_files(files: Vec<RuneDiffFile>) -> Self {
        Self::new(RuneKind::Diff {
            title: None,
            patch: String::new(),
            files,
            mode: RuneDiffMode::Unified,
            width: default_diff_width(),
            height: default_diff_height(),
            selection: None,
            annotations: Vec::new(),
        })
    }

    pub fn kit(root: KitNode) -> Self {
        Self::new(RuneKind::Kit { root })
    }

    pub fn new(surface: RuneKind) -> Self {
        Self {
            origin: default_origin(),
            size: default_size(),
            scale_factor: default_scale_factor(),
            dark: true,
            theme: RuneTheme::Opencode,
            opaque_backdrop: false,
            surface,
            commits: Vec::new(),
            allow_motion: Vec::new(),
        }
    }

    pub fn at(mut self, origin: [f32; 2]) -> Self {
        self.origin = origin;
        self
    }

    pub fn sized(mut self, size: [f32; 2]) -> Self {
        self.size = size;
        self
    }

    pub fn scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    pub fn light(mut self) -> Self {
        self.dark = false;
        self
    }

    pub fn theme(mut self, theme: RuneTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn opaque_backdrop(mut self) -> Self {
        self.opaque_backdrop = true;
        self
    }

    pub fn titled(mut self, title: impl Into<String>) -> Self {
        match &mut self.surface {
            RuneKind::Note { title: t, .. } | RuneKind::Diff { title: t, .. } => {
                *t = Some(title.into());
            }
            RuneKind::Kit { .. } => {}
        }
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        match &mut self.surface {
            RuneKind::Note { width: w, .. } | RuneKind::Diff { width: w, .. } => *w = width,
            RuneKind::Kit { .. } => {}
        }
        self
    }

    pub fn box_size(mut self, width: f32, height: f32) -> Self {
        if let RuneKind::Diff {
            width: w,
            height: h,
            ..
        } = &mut self.surface
        {
            *w = width;
            *h = height;
        }
        self
    }

    pub fn diff_mode(mut self, mode: RuneDiffMode) -> Self {
        if let RuneKind::Diff { mode: m, .. } = &mut self.surface {
            *m = mode;
        }
        self
    }

    pub fn allow_motion_rule(mut self, rule: &str, key: &str, input: &str, reason: &str) -> Self {
        self.allow_motion
            .push([rule.into(), key.into(), input.into(), reason.into()]);
        self
    }

    pub fn with_commit(
        mut self,
        at_nanos: u64,
        label: impl Into<String>,
        action: RuneAction,
    ) -> Self {
        self.commits.push(RuneCommitPlan {
            at_nanos,
            label: label.into(),
            action,
        });
        self
    }

    pub fn accepts(property: &str) -> bool {
        RUNE_SURFACE_CHANNELS.contains(&property)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.origin.iter().all(|v| v.is_finite()),
            "rune-surface origin must be finite"
        );
        ensure!(
            self.size
                .iter()
                .all(|&v| v.is_finite() && (32.0..=4096.0).contains(&v)),
            "rune-surface size must be finite and in [32, 4096]"
        );
        ensure!(
            self.scale_factor.is_finite() && (0.5..=4.0).contains(&self.scale_factor),
            "rune-surface scaleFactor must be finite and in [0.5, 4.0]"
        );
        match &self.surface {
            RuneKind::Note {
                markdown, width, ..
            } => {
                ensure!(
                    !markdown.trim().is_empty(),
                    "rune-surface note markdown cannot be empty"
                );
                ensure!(
                    width.is_finite() && (120.0..=1800.0).contains(width),
                    "rune-surface note width must be in [120, 1800]"
                );
            }
            RuneKind::Diff {
                patch,
                files,
                width,
                height,
                ..
            } => {
                if patch.trim().is_empty() && files.is_empty() {
                    bail!("rune-surface diff requires a non-empty patch or files list");
                }
                ensure!(
                    width.is_finite()
                        && (160.0..=1800.0).contains(width)
                        && height.is_finite()
                        && (80.0..=1040.0).contains(height),
                    "rune-surface diff dimensions out of bounds"
                );
            }
            RuneKind::Kit { root } => validate_kit_node(root)?,
        }
        Ok(())
    }
}

fn validate_kit_node(node: &KitNode) -> Result<()> {
    match node {
        KitNode::Segmented {
            key,
            tabs,
            selected,
            ..
        } => {
            ensure!(
                !key.trim().is_empty(),
                "kit segmented control requires a non-empty key"
            );
            ensure!(
                tabs.len() >= 2,
                "kit segmented control '{}' needs at least 2 tabs",
                key
            );
            ensure!(
                *selected < tabs.len(),
                "kit segmented control '{}' selected index out of bounds",
                key
            );
        }
        KitNode::TextInput { key, .. } | KitNode::ListRow { key, .. } => {
            ensure!(
                !key.trim().is_empty(),
                "kit stateful control requires a non-empty key"
            );
        }
        KitNode::Dialog { children, .. }
        | KitNode::Card { children, .. }
        | KitNode::Col { children, .. }
        | KitNode::Row { children, .. } => {
            for child in children {
                validate_kit_node(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Ergonomic constructors for `RuneKind::Kit` trees.
pub mod kit {
    use super::{KitNode, KitTone};

    pub fn dialog(key: &str, width: f32, children: Vec<KitNode>) -> KitNode {
        KitNode::Dialog {
            key: Some(key.into()),
            width: Some(width),
            padding: 18.0,
            gap: 12.0,
            border_tone: None,
            visible: true,
            children,
        }
    }

    pub fn card(key: &str, title: Option<&str>, width: f32, children: Vec<KitNode>) -> KitNode {
        KitNode::Card {
            key: Some(key.into()),
            title: title.map(str::to_owned),
            width: Some(width),
            padding: 16.0,
            gap: 10.0,
            visible: true,
            children,
        }
    }

    pub fn col(key: &str, gap: f32, children: Vec<KitNode>) -> KitNode {
        KitNode::Col {
            key: (!key.is_empty()).then(|| key.into()),
            width: None,
            gap,
            padding: 0.0,
            visible: true,
            children,
        }
    }

    pub fn row(key: &str, gap: f32, children: Vec<KitNode>) -> KitNode {
        KitNode::Row {
            key: (!key.is_empty()).then(|| key.into()),
            width: None,
            gap,
            padding: 0.0,
            center_y: true,
            visible: true,
            children,
        }
    }

    pub fn spacer() -> KitNode {
        KitNode::Spacer
    }

    pub fn divider() -> KitNode {
        KitNode::Divider
    }

    pub fn text(label: impl Into<String>, size: f32, weight: f32, tone: KitTone) -> KitNode {
        KitNode::Text {
            key: None,
            text: label.into(),
            size,
            weight,
            tone,
            mono: false,
            ride_trailing: false,
            visible: true,
        }
    }

    pub fn keyed_text(
        key: &str,
        label: impl Into<String>,
        size: f32,
        weight: f32,
        tone: KitTone,
    ) -> KitNode {
        KitNode::Text {
            key: Some(key.into()),
            text: label.into(),
            size,
            weight,
            tone,
            mono: false,
            ride_trailing: false,
            visible: true,
        }
    }

    pub fn badge(label: impl Into<String>, tone: KitTone) -> KitNode {
        KitNode::Badge {
            key: None,
            label: label.into(),
            tone,
            visible: true,
        }
    }

    pub fn button(key: &str, label: impl Into<String>, primary: bool) -> KitNode {
        KitNode::Button {
            key: (!key.is_empty()).then(|| key.into()),
            label: label.into(),
            primary,
            visible: true,
        }
    }

    pub fn segmented(key: &str, tabs: &[&str], selected: usize, width: Option<f32>) -> KitNode {
        KitNode::Segmented {
            key: key.into(),
            tabs: tabs.iter().map(|s| (*s).to_owned()).collect(),
            selected,
            width,
            static_pill: false,
            visible: true,
        }
    }

    pub fn segmented_static(
        key: &str,
        tabs: &[&str],
        selected: usize,
        width: Option<f32>,
    ) -> KitNode {
        KitNode::Segmented {
            key: key.into(),
            tabs: tabs.iter().map(|s| (*s).to_owned()).collect(),
            selected,
            width,
            static_pill: true,
            visible: true,
        }
    }

    pub fn text_input(key: &str, value: &str, placeholder: &str) -> KitNode {
        KitNode::TextInput {
            key: key.into(),
            value: value.into(),
            placeholder: placeholder.into(),
            visible: true,
        }
    }

    pub fn list_row(
        key: &str,
        title: impl Into<String>,
        hint: impl Into<String>,
        tone: KitTone,
        selected: bool,
    ) -> KitNode {
        KitNode::ListRow {
            key: key.into(),
            title: title.into(),
            hint: hint.into(),
            tone,
            hint_tone: KitTone::Muted,
            selected,
            visible: true,
        }
    }

    pub fn note(key: &str, markdown: impl Into<String>) -> KitNode {
        KitNode::Note {
            key: (!key.is_empty()).then(|| key.into()),
            markdown: markdown.into(),
            visible: true,
        }
    }
}

impl KitNode {
    pub fn hidden(mut self) -> Self {
        match &mut self {
            Self::Dialog { visible, .. }
            | Self::Card { visible, .. }
            | Self::Col { visible, .. }
            | Self::Row { visible, .. }
            | Self::Text { visible, .. }
            | Self::Badge { visible, .. }
            | Self::Button { visible, .. }
            | Self::Segmented { visible, .. }
            | Self::TextInput { visible, .. }
            | Self::ListRow { visible, .. }
            | Self::Note { visible, .. } => *visible = false,
            Self::Spacer | Self::Divider => {}
        }
        self
    }

    pub fn mono(mut self) -> Self {
        if let Self::Text { mono, .. } = &mut self {
            *mono = true;
        }
        self
    }

    pub fn ride_trailing(mut self) -> Self {
        if let Self::Text { ride_trailing, .. } = &mut self {
            *ride_trailing = true;
        }
        self
    }
}

/// Imperative `PlanBuilder` authoring handle for a `RuneSurfacePlan`.
#[derive(Clone)]
pub struct RuneSurfaceActor {
    actor: ActorHandle,
    plan: RuneSurfacePlan,
}

impl RuneSurfaceActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: RuneSurfacePlan,
    ) -> Result<Self> {
        plan.validate()?;
        let actor = scene.actor(id, RUNE_SURFACE_RECIPE, &plan)?;
        Ok(Self { actor, plan })
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    pub fn channel(
        &self,
        scene: &mut PlanBuilder,
        property: &str,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, property, initial)
    }

    pub fn settle_in(&self, scene: &mut PlanBuilder, at_nanos: u64) -> u64 {
        let opacity = self.channel(scene, "opacity", 0.0);
        let y = self.channel(scene, "y", 18.0);
        let scale = self.channel(scene, "scale", 0.985);
        scene.ease(&opacity, at_nanos, 1.0, 0.26, Ease::CubicOut);
        scene.spring_with(&y, at_nanos, 0.0, SpringPlan::PANEL);
        scene.spring_with(&scale, at_nanos, 1.0, SpringPlan::PANEL);
        at_nanos + whole_millis(0.6)
    }

    pub fn show(&self, scene: &mut PlanBuilder, at_nanos: u64) -> u64 {
        let opacity = self.channel(scene, "opacity", 0.0);
        scene.ease(&opacity, at_nanos, 1.0, 0.24, Ease::CubicOut);
        at_nanos + whole_millis(0.24)
    }

    pub fn hide(&self, scene: &mut PlanBuilder, at_nanos: u64) -> u64 {
        let opacity = self.channel(scene, "opacity", 1.0);
        scene.ease(&opacity, at_nanos, 0.0, 0.22, Ease::CubicOut);
        at_nanos + whole_millis(0.22)
    }

    pub fn glide(
        &self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        dx: f32,
        dy: f32,
        seconds: f32,
    ) -> u64 {
        let x = self.channel(scene, "x", 0.0);
        let y = self.channel(scene, "y", 0.0);
        scene.ease(&x, at_nanos, dx, seconds, Ease::GLIDE);
        scene.ease(&y, at_nanos, dy, seconds, Ease::GLIDE);
        at_nanos + whole_millis(seconds)
    }

    pub fn commit(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        label: impl Into<String>,
        action: RuneAction,
    ) -> Result<()> {
        self.plan.commits.push(RuneCommitPlan {
            at_nanos,
            label: label.into(),
            action,
        });
        scene.replace_actor_data(&self.actor, &self.plan)?;
        Ok(())
    }

    pub fn beats(self) -> ScoreRuneSurface {
        ScoreRuneSurface {
            inner: Rc::new(RefCell::new(self)),
        }
    }
}

/// Composable `psychopomp::score` value handle for a `RuneSurfacePlan`.
#[derive(Clone)]
pub struct ScoreRuneSurface {
    inner: Rc<RefCell<RuneSurfaceActor>>,
}

impl ScoreRuneSurface {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: RuneSurfacePlan,
    ) -> Result<Self> {
        Ok(RuneSurfaceActor::declare(scene, id, plan)?.beats())
    }

    pub fn id(&self) -> String {
        self.inner.borrow().id().to_owned()
    }

    pub fn settle_in(&self) -> impl Beat {
        let inner = Rc::clone(&self.inner);
        move |scene: &mut PlanBuilder, at: u64| {
            let end = inner.borrow().settle_in(scene, at);
            Span::new(at, end)
        }
    }

    pub fn show(&self) -> impl Beat {
        let inner = Rc::clone(&self.inner);
        move |scene: &mut PlanBuilder, at: u64| {
            let end = inner.borrow().show(scene, at);
            Span::new(at, end)
        }
    }

    pub fn hide(&self) -> impl Beat {
        let inner = Rc::clone(&self.inner);
        move |scene: &mut PlanBuilder, at: u64| {
            let end = inner.borrow().hide(scene, at);
            Span::new(at, end)
        }
    }

    pub fn commit(&self, label: impl Into<String>, action: RuneAction) -> impl Beat {
        let inner = Rc::clone(&self.inner);
        let label = label.into();
        move |scene: &mut PlanBuilder, at: u64| {
            inner
                .borrow_mut()
                .commit(scene, at, label, action)
                .expect("update rune-surface actor commits");
            Span::new(at, at + whole_millis(0.38))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::author::seconds;

    #[test]
    fn rune_surface_plans_round_trip_and_validate() {
        let plan = RuneSurfacePlan::kit(kit::dialog(
            "dialog",
            430.0,
            vec![
                kit::segmented("seg", &["Cost", "Tokens", "Web Search"], 0, Some(260.0)),
                kit::list_row(
                    "console",
                    "OpenCode Console",
                    "sign in",
                    KitTone::Base,
                    true,
                ),
            ],
        ))
        .at([1230.0, 520.0])
        .with_commit(
            seconds(0.8),
            "select tokens",
            RuneAction::SelectSegment {
                key: "seg".into(),
                index: 1,
            },
        );
        plan.validate().unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let decoded: RuneSurfacePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, decoded);
    }

    #[test]
    fn invalid_rune_surfaces_are_rejected() {
        assert!(RuneSurfacePlan::note("   ").validate().is_err());
        assert!(RuneSurfacePlan::diff_patch("").validate().is_err());
        let bad_seg = RuneSurfacePlan::kit(kit::segmented("seg", &["OnlyOne"], 0, None));
        assert!(bad_seg.validate().is_err());
    }
}
