use std::{
    collections::{BTreeSet, HashSet},
    fmt, fs, io,
    path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    canvas::CanvasPlan,
    face::FontFacePlan,
    math::{
        easing::{Ease, cubic_in_out},
        smoothstep, vec2,
    },
    paint::PalettePlan,
};

mod channels;
pub mod transition;
mod wipe;
pub use channels::{SpringPlan, compile_channels, destination_channel, effective_snapshots};
pub use transition::TransitionPhase;
pub use wipe::{ReelWipePlan, WipeDirection, WipeHoldPlan, WipePhase};

pub const SCENE_PLAN_VERSION: u32 = 2;

/// One native presentation containing independently authored Scene Plans.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckPlan {
    pub version: u32,
    pub id: String,
    pub slides: Vec<SlidePlan>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlidePlan {
    pub title: String,
    pub plan: ScenePlan,
}

impl DeckPlan {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version != 1 || self.id.trim().is_empty() || self.slides.is_empty() {
            anyhow::bail!("deck requires version 1, an ID, and at least one slide");
        }
        let mut ids = HashSet::new();
        for slide in &self.slides {
            slide.plan.validate()?;
            if slide.title.trim().is_empty()
                || slide.plan.presentation_steps.is_empty()
                || !ids.insert(&slide.plan.id)
            {
                anyhow::bail!(
                    "deck slides need a title, presentation steps, and distinct scene IDs"
                );
            }
        }
        Ok(())
    }

    /// Write the deck to `path` and each slide's plan beside it as
    /// `<scene id>.json`, creating the directory.
    pub fn write_with_slides(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        for slide in &self.slides {
            fs::write(
                parent.join(format!("{}.json", slide.plan.id)),
                slide.plan.to_json_pretty()?,
            )?;
        }
        Ok(())
    }
}

/// One encoded video that plays independently authored Scene Plans in order on a
/// single clock. Each segment keeps its own actors and local time and enters
/// through its own transition, so at most two segments overlap.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReelPlan {
    pub version: u32,
    pub id: String,
    pub segments: Vec<ReelSegmentPlan>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReelSegmentPlan {
    /// Transition from the previous segment. The first segment must use zero.
    #[serde(default)]
    pub transition_nanos: u64,
    #[serde(default)]
    pub transition_style: ReelTransitionStyle,
    /// For `zoom`: the rectangle (x, y, width, height) in the outgoing frame that
    /// becomes this segment, such as a card that opens into its code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_focus: Option<[f32; 4]>,
    /// For `wipe`: the divider's direction, mid-frame holds, and side labels.
    /// Omitted, the divider sweeps once from left to right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_wipe: Option<ReelWipePlan>,
    pub plan: ScenePlan,
}

impl ReelSegmentPlan {
    /// A segment that enters through `style` over `transition_nanos`.
    pub fn new(plan: ScenePlan, transition_nanos: u64, style: ReelTransitionStyle) -> Self {
        Self {
            transition_nanos,
            transition_style: style,
            transition_focus: None,
            transition_wipe: None,
            plan,
        }
    }

    /// Start from `focus` (x, y, width, height) in the outgoing frame: the card
    /// a zoom flies into, the rectangle a match carries, or the point an iris
    /// or ink opens from.
    pub fn focused(mut self, focus: [f32; 4]) -> Self {
        self.transition_focus = Some(focus);
        self
    }

    /// A hard cut: the segment starts as its predecessor ends.
    pub fn cut(plan: ScenePlan) -> Self {
        Self::new(plan, 0, ReelTransitionStyle::Crossfade)
    }

    /// A J-cut: the segment starts, and is heard, `lead_nanos` before the
    /// picture cuts to it at its predecessor's end. Its own picture is hidden
    /// for that lead, so open on sound rather than motion.
    pub fn j_cut(plan: ScenePlan, lead_nanos: u64) -> Self {
        Self::new(plan, lead_nanos, ReelTransitionStyle::JCut)
    }

    /// An L-cut: the picture cuts to this segment while its predecessor is
    /// still heard for `tail_nanos`, whose last picture is never shown.
    pub fn l_cut(plan: ScenePlan, tail_nanos: u64) -> Self {
        Self::new(plan, tail_nanos, ReelTransitionStyle::LCut)
    }

    /// The incoming segment fades in over the outgoing one.
    pub fn crossfaded(plan: ScenePlan, transition_nanos: u64) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Crossfade)
    }

    /// Fade to the empty background, then fade in, so dense frames never mix.
    pub fn dipped(plan: ScenePlan, transition_nanos: u64) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Dip)
    }

    /// Fly into `focus` in the outgoing frame while this segment grows out of it.
    pub fn zoomed(plan: ScenePlan, transition_nanos: u64, focus: [f32; 4]) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Zoom).focused(focus)
    }

    /// A segment that enters through `wipe` over `transition_nanos`.
    pub fn wiped(plan: ScenePlan, transition_nanos: u64, wipe: ReelWipePlan) -> Self {
        Self {
            transition_wipe: Some(wipe),
            ..Self::new(plan, transition_nanos, ReelTransitionStyle::Wipe)
        }
    }

    /// Both frames travel together toward `direction`, like a camera pan.
    pub fn pushed(plan: ScenePlan, transition_nanos: u64, direction: WipeDirection) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Push(direction))
    }

    /// This frame slides in over the outgoing one toward `direction` and settles.
    pub fn slid(plan: ScenePlan, transition_nanos: u64, direction: WipeDirection) -> Self {
        Self::new(
            plan,
            transition_nanos,
            ReelTransitionStyle::Slide(direction),
        )
    }

    /// A whip pan toward `direction`: the cut hides in a streak of motion blur.
    pub fn whipped(plan: ScenePlan, transition_nanos: u64, direction: WipeDirection) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Whip(direction))
    }

    /// A circle opens from the frame's center, ringed with light if `ring`.
    /// Add [`ReelSegmentPlan::focused`] to open from a rectangle's center.
    pub fn irised(plan: ScenePlan, transition_nanos: u64, ring: bool) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Iris { ring })
    }

    /// A shared-element zoom: `from` in the outgoing frame flies onto `to` in
    /// this one, so the element visibly becomes its counterpart.
    pub fn matched(plan: ScenePlan, transition_nanos: u64, from: [f32; 4], to: [f32; 4]) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Match(to)).focused(from)
    }

    /// [`Self::matched`] for a round element (a ball, a dot, an orb): the
    /// carried shape is the ellipse inside each rectangle, not a card.
    pub fn matched_round(
        plan: ScenePlan,
        transition_nanos: u64,
        from: [f32; 4],
        to: [f32; 4],
    ) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::MatchRound(to)).focused(from)
    }

    /// The frame turns over toward `direction` like a card with this segment
    /// on its back.
    pub fn flipped(plan: ScenePlan, transition_nanos: u64, direction: WipeDirection) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Flip(direction))
    }

    /// The two frames are faces of a cube that turns toward `direction`.
    pub fn cubed(plan: ScenePlan, transition_nanos: u64, direction: WipeDirection) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Cube(direction))
    }

    /// This frame spreads in like ink with a soft organic edge. Add
    /// [`ReelSegmentPlan::focused`] to spread from a rectangle.
    pub fn inked(plan: ScenePlan, transition_nanos: u64) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Ink)
    }

    /// A few frames of split color and torn blocks around a hard cut.
    pub fn glitched(plan: ScenePlan, transition_nanos: u64) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Glitch)
    }

    /// A white-out flash that hides the cut, then decays.
    pub fn flashed(plan: ScenePlan, transition_nanos: u64) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::Flash)
    }

    /// A warm light leak drifts across and hides the cut.
    pub fn leaked(plan: ScenePlan, transition_nanos: u64) -> Self {
        Self::new(plan, transition_nanos, ReelTransitionStyle::LightLeak)
    }
}

/// How a segment replaces its predecessor during `transition_nanos`. Simple
/// styles serialize as names (`"dip"`); styles with settings as one-key
/// objects (`{ "push": "left" }`, `{ "match": [x, y, w, h] }`).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ReelTransitionStyle {
    /// Both segments are visible while the incoming one fades in over the other.
    #[default]
    Crossfade,
    /// The outgoing segment fades to the empty background, then the incoming one
    /// fades in. Dense frames never overlap.
    Dip,
    /// The camera flies into `transition_focus`: the outgoing frame zooms past
    /// while the incoming segment grows out of that rectangle.
    Zoom,
    /// A divider sweeps across with the incoming segment behind it, optionally
    /// resting mid-frame so both are visible side by side.
    Wipe,
    /// The outgoing picture holds until the transition ends, then cuts: the
    /// incoming segment is heard first.
    JCut,
    /// The picture cuts at once while the outgoing segment is still heard.
    LCut,
    /// Both frames travel together toward the direction, like a camera pan.
    Push(WipeDirection),
    /// The incoming frame slides in over the outgoing one, which drifts back
    /// and dims, and settles like a critically damped spring.
    Slide(WipeDirection),
    /// A whip pan: a push that leans in, tears across in a streak of
    /// directional motion blur, and catches.
    Whip(WipeDirection),
    /// A circle opens from the `transition_focus` center, or the frame's, with
    /// a soft edge and an optional ring of light.
    Iris {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        ring: bool,
    },
    /// A shared-element zoom: `transition_focus` in the outgoing frame flies
    /// onto this rectangle (x, y, width, height) of the incoming frame.
    Match([f32; 4]),
    /// A match whose element is round: the ellipse inside the rectangles.
    MatchRound([f32; 4]),
    /// The frame turns over like a card, the incoming segment on its back.
    Flip(WipeDirection),
    /// The frames are two faces of a cube turning toward the direction.
    Cube(WipeDirection),
    /// The incoming frame spreads in like ink, with a soft organic edge, from
    /// `transition_focus` if set.
    Ink,
    /// Split color and torn, displaced blocks for a few frames around a hard cut.
    Glitch,
    /// A white-out flash hides the cut, then decays.
    Flash,
    /// A warm light leak drifts across the frame and hides the cut.
    LightLeak,
}

/// Screen transform of one layer during a zoom: `output = source * scale + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReelZoom {
    pub scale: f32,
    pub offset: [f32; 2],
    /// Corner radius of the incoming frame while it is still a card.
    pub radius: f32,
}

impl ReelZoom {
    /// Where the layer sits at `progress` of a zoom into `focus` on a
    /// `width` x `height` frame. The outgoing frame magnifies until `focus`
    /// fills the width; the incoming frame starts inside `focus`.
    pub fn at(focus: [f32; 4], width: f32, height: f32, progress: f32, incoming: bool) -> Self {
        let eased = cubic_in_out(progress.clamp(0.0, 1.0));
        let focus_center = vec2(focus[0] + focus[2] * 0.5, focus[1] + focus[3] * 0.5);
        let center = vec2(width, height) * 0.5;
        let fill = width / focus[2].max(1.0);
        // The focus center travels to the screen center while the scale changes
        // geometrically, so the zoom speed feels constant.
        let anchor = focus_center.lerp(center, eased);
        let (scale, pivot) = if incoming {
            (fill.powf(eased - 1.0), center)
        } else {
            (fill.powf(eased), focus_center)
        };
        Self {
            scale,
            offset: (anchor - pivot * scale).to_array(),
            radius: if incoming { 28.0 * (1.0 - eased) } else { 0.0 },
        }
    }
}

/// Where one segment sits on the reel clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReelSpan {
    pub start_nanos: u64,
    pub end_nanos: u64,
    pub transition_nanos: u64,
    pub transition_style: ReelTransitionStyle,
    pub transition_focus: Option<[f32; 4]>,
}

/// One segment visible at a reel time, sampled at its own local time. Layers are
/// mixed in order over what is below them by `weight`; when the first layer's
/// weight is below one it is mixed over the empty background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReelLayer {
    pub segment: usize,
    pub local_seconds: f64,
    pub weight: f32,
    /// Set during a zoom; the renderer resolves it with `ReelZoom::at`.
    pub zoom: Option<ZoomPhase>,
    /// Set on the incoming layer of a wipe: it shows only behind the divider.
    pub wipe: Option<WipePhase>,
    /// Set on the incoming layer of a composited transition (push, iris,
    /// flip, ...): the renderer combines it with the outgoing frame below.
    pub transition: Option<TransitionPhase>,
}

/// One layer's part in a zoom transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomPhase {
    pub focus: [f32; 4],
    pub progress: f32,
    pub incoming: bool,
}

impl ReelPlan {
    pub const VERSION: u32 = 1;

    /// A validated reel of `segments`, each entering through its own
    /// transition; the first should be a [`ReelSegmentPlan::cut`].
    pub fn new(id: impl Into<String>, segments: Vec<ReelSegmentPlan>) -> anyhow::Result<Self> {
        let reel = Self {
            version: Self::VERSION,
            id: id.into(),
            segments,
        };
        reel.validate()?;
        Ok(reel)
    }

    /// A validated reel that plays `plans` in order, each dipping through the
    /// empty background from the previous one over `transition_nanos`.
    pub fn dipped(
        id: impl Into<String>,
        plans: Vec<ScenePlan>,
        transition_nanos: u64,
    ) -> anyhow::Result<Self> {
        let reel = Self {
            version: Self::VERSION,
            id: id.into(),
            segments: plans
                .into_iter()
                .enumerate()
                .map(|(index, plan)| ReelSegmentPlan {
                    transition_nanos: if index == 0 { 0 } else { transition_nanos },
                    transition_style: ReelTransitionStyle::Dip,
                    transition_focus: None,
                    transition_wipe: None,
                    plan,
                })
                .collect(),
        };
        reel.validate()?;
        Ok(reel)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version != Self::VERSION {
            anyhow::bail!("reel requires version {}", Self::VERSION);
        }
        if self.id.trim().is_empty() || self.id.chars().any(char::is_whitespace) {
            anyhow::bail!("reel requires an ID without whitespace");
        }
        if self.segments.is_empty() {
            anyhow::bail!("reel requires at least one segment");
        }
        let mut ids = HashSet::new();
        for (index, segment) in self.segments.iter().enumerate() {
            segment.plan.validate()?;
            if !ids.insert(&segment.plan.id) {
                anyhow::bail!(
                    "reel segment {index} repeats scene ID '{}'",
                    segment.plan.id
                );
            }
            if index == 0 {
                if segment.transition_nanos != 0 {
                    anyhow::bail!("the first reel segment cannot transition from nothing");
                }
                continue;
            }
            if segment.transition_style == ReelTransitionStyle::Zoom {
                let focus = segment.transition_focus.unwrap_or_default();
                if focus.iter().any(|v| !v.is_finite()) || focus[2] < 8.0 || focus[3] < 8.0 {
                    anyhow::bail!(
                        "reel segment '{}' zooms without a transitionFocus rectangle",
                        segment.plan.id
                    );
                }
            }
            validate_transition(segment)
                .map_err(|error| anyhow::anyhow!("reel segment '{}': {error}", segment.plan.id))?;
            match (&segment.transition_wipe, segment.transition_style) {
                (Some(wipe), ReelTransitionStyle::Wipe) => {
                    wipe.validate(segment.transition_nanos).map_err(|error| {
                        anyhow::anyhow!("reel segment '{}': {error}", segment.plan.id)
                    })?
                }
                (Some(_), _) => anyhow::bail!(
                    "reel segment '{}' has transitionWipe without the wipe style",
                    segment.plan.id
                ),
                (None, _) => {}
            }
            let previous = &self.segments[index - 1];
            if segment.plan.canvas.size() != previous.plan.canvas.size()
                || segment.plan.canvas.fps != previous.plan.canvas.fps
            {
                anyhow::bail!(
                    "reel segment '{}' canvas ({:?} @ {}fps) does not match previous segment ({:?} @ {}fps)",
                    segment.plan.id,
                    segment.plan.canvas.size(),
                    segment.plan.canvas.fps,
                    previous.plan.canvas.size(),
                    previous.plan.canvas.fps
                );
            }
            // The previous segment must be alone on screen before this one starts,
            // so no instant ever blends three segments.
            let available = previous
                .plan
                .duration_nanos
                .saturating_sub(previous.transition_nanos);
            if segment.transition_nanos > available
                || segment.transition_nanos > segment.plan.duration_nanos
            {
                anyhow::bail!(
                    "reel segment '{}' transition is longer than the time either neighbor is alone on screen",
                    segment.plan.id
                );
            }
        }
        Ok(())
    }

    pub fn spans(&self) -> Vec<ReelSpan> {
        let mut spans = Vec::with_capacity(self.segments.len());
        let mut end = 0_u64;
        for segment in &self.segments {
            let start = end.saturating_sub(segment.transition_nanos);
            end = start + segment.plan.duration_nanos;
            spans.push(ReelSpan {
                start_nanos: start,
                end_nanos: end,
                transition_nanos: segment.transition_nanos,
                transition_style: segment.transition_style,
                transition_focus: segment.transition_focus,
            });
        }
        spans
    }

    pub fn duration_nanos(&self) -> u64 {
        self.spans().last().map_or(0, |span| span.end_nanos)
    }

    pub fn canvas(&self) -> CanvasPlan {
        self.segments
            .first()
            .map_or_else(CanvasPlan::default, |seg| seg.plan.canvas)
    }

    /// The segments visible at `seconds`, in draw order. Outside transitions this
    /// is one fully weighted segment. A crossfade mixes the incoming segment over
    /// the outgoing one; a dip shows one segment faded toward the background; a
    /// composited transition (push, iris, flip, ...) shows both at full weight
    /// with its phase on the incoming layer.
    pub fn layers_at(&self, seconds: f64) -> Vec<ReelLayer> {
        let spans = self.spans();
        let at = seconds.max(0.0);
        let local = |index: usize| {
            let span = spans[index];
            let start = span.start_nanos as f64 / 1e9;
            (at - start).clamp(0.0, (span.end_nanos - span.start_nanos) as f64 / 1e9)
        };
        // The latest segment that has started is the one being entered or shown.
        let Some(current) = spans
            .iter()
            .rposition(|span| span.start_nanos as f64 / 1e9 <= at)
        else {
            return Vec::new();
        };
        let span = spans[current];
        let progress = if span.transition_nanos == 0 {
            1.0
        } else {
            ((at - span.start_nanos as f64 / 1e9) / (span.transition_nanos as f64 / 1e9))
                .clamp(0.0, 1.0)
        };
        let layer = |segment: usize, weight: f64| ReelLayer {
            segment,
            local_seconds: local(segment),
            weight: smoothstep(weight as f32),
            zoom: None,
            wipe: None,
            transition: None,
        };
        if progress >= 1.0 || current == 0 {
            return vec![layer(current, 1.0)];
        }
        match span.transition_style {
            ReelTransitionStyle::Crossfade if progress <= 0.0 => vec![layer(current - 1, 1.0)],
            ReelTransitionStyle::Crossfade => {
                vec![layer(current - 1, 1.0), layer(current, progress)]
            }
            ReelTransitionStyle::Dip if progress < 0.5 => {
                vec![layer(current - 1, 1.0 - progress * 2.0)]
            }
            ReelTransitionStyle::Dip => vec![layer(current, progress * 2.0 - 1.0)],
            ReelTransitionStyle::Zoom => {
                let focus = span.transition_focus.unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let phase = |incoming| {
                    Some(ZoomPhase {
                        focus,
                        progress: progress as f32,
                        incoming,
                    })
                };
                // The incoming card fades in while it is still small.
                vec![
                    ReelLayer {
                        zoom: phase(false),
                        ..layer(current - 1, 1.0)
                    },
                    ReelLayer {
                        zoom: phase(true),
                        ..layer(current, (progress - 0.08) / 0.4)
                    },
                ]
            }
            ReelTransitionStyle::Wipe => {
                let default = ReelWipePlan::default();
                let wipe = self.segments[current]
                    .transition_wipe
                    .as_ref()
                    .unwrap_or(&default);
                let elapsed = at - span.start_nanos as f64 / 1e9;
                let position = wipe.position(elapsed, span.transition_nanos as f64 / 1e9);
                if position <= 0.0 {
                    return vec![layer(current - 1, 1.0)];
                }
                vec![
                    layer(current - 1, 1.0),
                    ReelLayer {
                        wipe: Some(WipePhase {
                            position,
                            direction: wipe.direction,
                        }),
                        ..layer(current, 1.0)
                    },
                ]
            }
            ReelTransitionStyle::JCut => vec![layer(current - 1, 1.0)],
            ReelTransitionStyle::LCut if progress <= 0.0 => vec![layer(current - 1, 1.0)],
            ReelTransitionStyle::LCut => vec![layer(current, 1.0)],
            // The rest are composited by the renderer from both frames; the
            // first instant is still the outgoing frame alone.
            _ if progress <= 0.0 => vec![layer(current - 1, 1.0)],
            style => vec![
                layer(current - 1, 1.0),
                ReelLayer {
                    transition: Some(TransitionPhase {
                        style,
                        progress: progress as f32,
                        seconds: (span.transition_nanos as f64 / 1e9) as f32,
                        focus: span.transition_focus,
                    }),
                    ..layer(current, 1.0)
                },
            ],
        }
    }
}

/// The settings a segment's transition style needs, and only those.
fn validate_transition(segment: &ReelSegmentPlan) -> anyhow::Result<()> {
    use ReelTransitionStyle::*;
    let rect_is_valid =
        |[x, y, w, h]: [f32; 4]| [x, y, w, h].iter().all(|v| v.is_finite()) && w >= 8.0 && h >= 8.0;
    let style = segment.transition_style;
    match (style, segment.transition_focus) {
        (Crossfade | Dip | Zoom | Wipe, _) => {}
        (Match(_) | MatchRound(_), None) => {
            anyhow::bail!("a match needs a transitionFocus rectangle to carry")
        }
        (Match(_) | MatchRound(_) | Iris { .. } | Ink, Some(focus)) if !rect_is_valid(focus) => {
            anyhow::bail!("transitionFocus must be a finite rectangle at least 8 pixels on a side")
        }
        (Match(_) | MatchRound(_) | Iris { .. } | Ink, _) => {}
        (_, Some(_)) => anyhow::bail!("a {style:?} transition takes no transitionFocus"),
        (_, None) => {}
    }
    if let Match(target) | MatchRound(target) = style
        && !rect_is_valid(target)
    {
        anyhow::bail!("a match target must be a finite rectangle at least 8 pixels on a side");
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenePlan {
    pub version: u32,
    pub id: String,
    pub duration_nanos: u64,
    #[serde(default, skip_serializing_if = "CanvasPlan::is_default")]
    pub canvas: CanvasPlan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<PalettePlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontFacePlan>,
    #[serde(default)]
    pub actors: Vec<ActorPlan>,
    #[serde(default)]
    pub semantic_targets: Vec<SemanticTargetPlan>,
    #[serde(default)]
    pub continuous_channels: Vec<ContinuousChannelPlan>,
    #[serde(default)]
    pub state_channels: Vec<StateChannelPlan>,
    #[serde(default)]
    pub cues: Vec<CuePlan>,
    #[serde(default)]
    pub media: Vec<MediaPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presentation_steps: Vec<PresentationStepPlan>,
}

impl ScenePlan {
    pub fn new(id: impl Into<String>, duration_nanos: u64) -> Self {
        Self {
            version: SCENE_PLAN_VERSION,
            id: id.into(),
            duration_nanos,
            canvas: CanvasPlan::default(),
            palette: None,
            fonts: Vec::new(),
            actors: Vec::new(),
            semantic_targets: Vec::new(),
            continuous_channels: Vec::new(),
            state_channels: Vec::new(),
            cues: Vec::new(),
            media: Vec::new(),
            presentation_steps: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), PlanValidationError> {
        let mut diagnostics = Vec::new();
        if self.version != SCENE_PLAN_VERSION {
            diagnostics.push(PlanDiagnostic::new(
                "unsupported-plan-version",
                "version",
                format!(
                    "scene plan version {} is not supported; expected {SCENE_PLAN_VERSION}",
                    self.version
                ),
            ));
        }
        validate_id(&self.id, "id", "scene", &mut diagnostics);
        if self.duration_nanos == 0 {
            diagnostics.push(PlanDiagnostic::new(
                "invalid-duration",
                "durationNanos",
                "scene duration must be positive",
            ));
        }
        if let Err(err) = self.canvas.validate() {
            diagnostics.push(PlanDiagnostic::new(
                "invalid-canvas",
                "canvas",
                err.to_string(),
            ));
        }
        for (index, font) in self.fonts.iter().enumerate() {
            if let Err(err) = font.validate() {
                diagnostics.push(PlanDiagnostic::new(
                    "invalid-font",
                    format!("fonts[{index}]"),
                    err.to_string(),
                ));
            }
        }

        let mut actor_ids = HashSet::new();
        for (index, actor) in self.actors.iter().enumerate() {
            let path = format!("actors[{index}]");
            validate_id(&actor.id, &format!("{path}.id"), "actor", &mut diagnostics);
            validate_id(
                &actor.recipe,
                &format!("{path}.recipe"),
                "actor recipe",
                &mut diagnostics,
            );
            if !actor_ids.insert(actor.id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-actor-id",
                    format!("{path}.id"),
                    format!("actor '{}' is declared more than once", actor.id),
                ));
            }
        }

        let mut channel_ids = HashSet::new();
        let mut target_ids = HashSet::new();
        for (index, target) in self.semantic_targets.iter().enumerate() {
            let path = format!("semanticTargets[{index}]");
            validate_id(
                &target.id,
                &format!("{path}.id"),
                "semantic target",
                &mut diagnostics,
            );
            if !target_ids.insert(target.id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-semantic-target-id",
                    format!("{path}.id"),
                    format!("semantic target '{}' is declared more than once", target.id),
                ));
            }
            if !actor_ids.contains(target.actor_id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "unknown-actor",
                    format!("{path}.actorId"),
                    format!(
                        "semantic target '{}' references unknown actor '{}'",
                        target.id, target.actor_id
                    ),
                ));
            }
        }

        let mut continuous_properties = HashSet::new();
        for (index, channel) in self.continuous_channels.iter().enumerate() {
            let path = format!("continuousChannels[{index}]");
            validate_channel(
                &channel.id,
                &channel.actor_id,
                &path,
                &actor_ids,
                &mut channel_ids,
                &mut diagnostics,
            );
            validate_id(
                &channel.property,
                &format!("{path}.property"),
                "continuous property",
                &mut diagnostics,
            );
            if !continuous_properties.insert((channel.actor_id.as_str(), channel.property.as_str()))
            {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-actor-property",
                    format!("{path}.property"),
                    format!(
                        "actor '{}' has more than one continuous '{}' channel",
                        channel.actor_id, channel.property
                    ),
                ));
            }
            validate_scalar(
                &channel.initial,
                &target_ids,
                &format!("{path}.initial"),
                &mut diagnostics,
            );
            validate_track_events(
                &channel.events,
                self.duration_nanos,
                &target_ids,
                &path,
                &mut diagnostics,
            );
        }

        let mut state_properties = HashSet::new();
        for (index, channel) in self.state_channels.iter().enumerate() {
            let path = format!("stateChannels[{index}]");
            validate_channel(
                &channel.id,
                &channel.actor_id,
                &path,
                &actor_ids,
                &mut channel_ids,
                &mut diagnostics,
            );
            validate_id(
                &channel.state,
                &format!("{path}.state"),
                "state property",
                &mut diagnostics,
            );
            if !state_properties.insert((channel.actor_id.as_str(), channel.state.as_str())) {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-actor-state",
                    format!("{path}.state"),
                    format!(
                        "actor '{}' has more than one '{}' state channel",
                        channel.actor_id, channel.state
                    ),
                ));
            }
            validate_ordered_times(
                channel.events.iter().map(|event| event.at_nanos),
                self.duration_nanos,
                &format!("{path}.events"),
                &mut diagnostics,
            );
        }

        let mut cue_ids = HashSet::new();
        for (index, cue) in self.cues.iter().enumerate() {
            let path = format!("cues[{index}]");
            validate_id(&cue.id, &format!("{path}.id"), "cue", &mut diagnostics);
            if !cue_ids.insert(cue.id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-cue-id",
                    format!("{path}.id"),
                    format!("cue '{}' is declared more than once", cue.id),
                ));
            }
            validate_range(
                cue.start_nanos,
                cue.end_nanos,
                self.duration_nanos,
                &path,
                &mut diagnostics,
            );
        }

        let mut step_ids = HashSet::new();
        let mut previous_hold = None;
        for (index, step) in self.presentation_steps.iter().enumerate() {
            let path = format!("presentationSteps[{index}]");
            validate_id(
                &step.id,
                &format!("{path}.id"),
                "presentation step",
                &mut diagnostics,
            );
            if !step_ids.insert(step.id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-presentation-step-id",
                    format!("{path}.id"),
                    format!("presentation step '{}' is declared more than once", step.id),
                ));
            }
            if step.title.trim().is_empty() {
                diagnostics.push(PlanDiagnostic::new(
                    "empty-step-title",
                    format!("{path}.title"),
                    "presentation step title must not be empty",
                ));
            }
            if step.start_nanos > step.hold_nanos || step.hold_nanos > self.duration_nanos {
                diagnostics.push(PlanDiagnostic::new(
                    "invalid-step-range",
                    &path,
                    "presentation step must satisfy 0 <= startNanos <= holdNanos <= scene duration",
                ));
            }
            if previous_hold.is_some_and(|hold| step.start_nanos < hold) {
                diagnostics.push(PlanDiagnostic::new(
                    "overlapping-presentation-steps",
                    &path,
                    "presentation steps must be in playback order and start at or after the preceding hold",
                ));
            }
            previous_hold = Some(step.hold_nanos);
        }

        let mut media_ids = HashSet::new();
        for (index, media) in self.media.iter().enumerate() {
            let path = format!("media[{index}]");
            validate_id(&media.id, &format!("{path}.id"), "media", &mut diagnostics);
            if !media_ids.insert(media.id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "duplicate-media-id",
                    format!("{path}.id"),
                    format!("media '{}' is declared more than once", media.id),
                ));
            }
            if media.path.as_os_str().is_empty() {
                diagnostics.push(PlanDiagnostic::new(
                    "empty-media-path",
                    format!("{path}.path"),
                    "media path must not be empty",
                ));
            }
            if !media.gain_db.is_finite() {
                diagnostics.push(PlanDiagnostic::new(
                    "non-finite-value",
                    format!("{path}.gainDb"),
                    "media gain must be finite",
                ));
            }
            validate_range(
                media.source_start_nanos,
                media.source_end_nanos,
                u64::MAX,
                &format!("{path}.source"),
                &mut diagnostics,
            );
            validate_range(
                media.timeline_start_nanos,
                media.timeline_end_nanos,
                self.duration_nanos,
                &format!("{path}.timeline"),
                &mut diagnostics,
            );
            if media
                .source_end_nanos
                .saturating_sub(media.source_start_nanos)
                != media
                    .timeline_end_nanos
                    .saturating_sub(media.timeline_start_nanos)
            {
                diagnostics.push(PlanDiagnostic::new(
                    "media-duration-mismatch",
                    path,
                    format!("media '{}' source and timeline durations differ", media.id),
                ));
            }
        }

        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(PlanValidationError { diagnostics })
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Write the plan's pretty JSON to `path`, creating its directory, or print
    /// it to stdout when there is no path: a Scene Program's usual output.
    pub fn write_or_print(&self, path: Option<impl AsRef<Path>>) -> io::Result<()> {
        let json = self.to_json_pretty()?;
        let Some(path) = path else {
            println!("{json}");
            return Ok(());
        };
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)
    }

    pub fn from_json(json: &str) -> Result<Self, ReadPlanError> {
        let plan = serde_json::from_str::<Self>(json).map_err(ReadPlanError::Json)?;
        plan.validate().map_err(ReadPlanError::Validation)?;
        Ok(plan)
    }

    pub fn schema() -> Value {
        serde_json::json!({
            "version": SCENE_PLAN_VERSION,
            "clock": "integer nanoseconds",
            "actor": {
                "required": ["id", "recipe"],
                "data": "recipe-owned JSON value"
            },
            "semanticTarget": {
                "required": ["id", "actorId", "selector"],
                "selector": "renderer-recipe-owned JSON value"
            },
            "continuousChannel": {
                "required": ["id", "actorId", "property", "initial"],
                "operations": ["set", "spring", "ease"],
                "scalar": "literal number or semantic target component"
            },
            "stateChannel": {
                "required": ["id", "actorId", "state", "initial"],
                "events": "ordered values at exact times"
            },
            "cue": {
                "required": ["id", "startNanos", "endNanos"]
            },
            "presentationStep": {
                "required": ["id", "title", "startNanos", "holdNanos"],
                "timing": "ordered, non-overlapping entry ranges; equal start and hold means a still step",
                "playback": "open at the first hold; Next plays the following entry then holds; Previous restores the preceding hold"
            },
            "media": {
                "required": [
                    "id", "path", "kind", "role", "sourceStartNanos", "sourceEndNanos",
                    "timelineStartNanos", "timelineEndNanos"
                ]
            }
        })
    }

    pub fn diff(&self, other: &Self) -> Result<Vec<PlanChange>, serde_json::Error> {
        let before = serde_json::to_value(self)?;
        let after = serde_json::to_value(other)?;
        let mut changes = Vec::new();
        diff_values("$", Some(&before), Some(&after), &mut changes);
        Ok(changes)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanChange {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorPlan {
    pub id: String,
    pub recipe: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTargetPlan {
    pub id: String,
    pub actor_id: String,
    pub selector: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ScalarPlan {
    Literal(f32),
    Target(TargetScalarPlan),
}

impl From<f32> for ScalarPlan {
    fn from(value: f32) -> Self {
        Self::Literal(value)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetScalarPlan {
    pub target_id: String,
    pub component: TargetComponentPlan,
    #[serde(default)]
    pub offset: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetComponentPlan {
    X,
    Width,
    CenterX,
    LineY,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuousChannelPlan {
    pub id: String,
    pub actor_id: String,
    pub property: String,
    pub initial: ScalarPlan,
    #[serde(default)]
    pub events: Vec<TrackEventPlan>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "operation"
)]
pub enum TrackEventPlan {
    Set {
        at_nanos: u64,
        value: ScalarPlan,
    },
    Spring {
        at_nanos: u64,
        target: ScalarPlan,
        response_seconds: f32,
        damping_ratio: f32,
        position_threshold: f32,
        velocity_threshold: f32,
    },
    /// From the current value to `target` along `curve`, over an exact duration.
    Ease {
        at_nanos: u64,
        target: ScalarPlan,
        duration_nanos: u64,
        curve: Ease,
    },
}

impl TrackEventPlan {
    pub fn at_nanos(&self) -> u64 {
        match self {
            Self::Set { at_nanos, .. }
            | Self::Spring { at_nanos, .. }
            | Self::Ease { at_nanos, .. } => *at_nanos,
        }
    }

    /// The value a Set holds or a Spring or Ease approaches.
    pub fn scalar(&self) -> &ScalarPlan {
        match self {
            Self::Set { value, .. } => value,
            Self::Spring { target, .. } | Self::Ease { target, .. } => target,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateChannelPlan {
    pub id: String,
    pub actor_id: String,
    pub state: String,
    pub initial: Value,
    #[serde(default)]
    pub events: Vec<StateEventPlan>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEventPlan {
    pub at_nanos: u64,
    pub value: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuePlan {
    pub id: String,
    pub start_nanos: u64,
    pub end_nanos: u64,
}

/// An authored entry range and exact held endpoint on the original scene clock.
/// Presentation waits do not alter scene time or the automatic video schedule.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationStepPlan {
    pub id: String,
    pub title: String,
    pub start_nanos: u64,
    pub hold_nanos: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaRolePlan {
    Script,
    Layer,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaKindPlan {
    Audio,
    Video,
    Image,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPlan {
    pub id: String,
    pub path: std::path::PathBuf,
    pub kind: MediaKindPlan,
    pub role: MediaRolePlan,
    pub source_start_nanos: u64,
    pub source_end_nanos: u64,
    pub timeline_start_nanos: u64,
    pub timeline_end_nanos: u64,
    #[serde(default)]
    pub gain_db: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

impl PlanDiagnostic {
    fn new(code: impl Into<String>, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            path: path.into(),
            message: message.into(),
            suggestions: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct PlanValidationError {
    diagnostics: Vec<PlanDiagnostic>,
}

impl PlanValidationError {
    pub fn diagnostics(&self) -> &[PlanDiagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for PlanValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "scene plan has {} validation error(s)",
            self.diagnostics.len()
        )?;
        // Name each problem: a bare count sends authors digging through JSON.
        for diagnostic in self.diagnostics.iter().take(5) {
            write!(
                formatter,
                "\n  {} at {}: {}",
                diagnostic.code, diagnostic.path, diagnostic.message
            )?;
        }
        if self.diagnostics.len() > 5 {
            write!(formatter, "\n  …and {} more", self.diagnostics.len() - 5)?;
        }
        Ok(())
    }
}

impl std::error::Error for PlanValidationError {}

#[derive(Debug)]
pub enum ReadPlanError {
    Json(serde_json::Error),
    Validation(PlanValidationError),
}

impl fmt::Display for ReadPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "parse scene plan: {error}"),
            Self::Validation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ReadPlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Validation(error) => Some(error),
        }
    }
}

fn validate_channel<'a>(
    id: &'a str,
    actor_id: &str,
    path: &str,
    actor_ids: &HashSet<&str>,
    channel_ids: &mut HashSet<&'a str>,
    diagnostics: &mut Vec<PlanDiagnostic>,
) {
    validate_id(id, &format!("{path}.id"), "channel", diagnostics);
    if !channel_ids.insert(id) {
        diagnostics.push(PlanDiagnostic::new(
            "duplicate-channel-id",
            format!("{path}.id"),
            format!("channel '{id}' is declared more than once"),
        ));
    }
    if !actor_ids.contains(actor_id) {
        let mut diagnostic = PlanDiagnostic::new(
            "unknown-actor",
            format!("{path}.actorId"),
            format!("channel '{id}' references unknown actor '{actor_id}'"),
        );
        diagnostic.suggestions = actor_ids.iter().map(|value| (*value).to_owned()).collect();
        diagnostic.suggestions.sort();
        diagnostics.push(diagnostic);
    }
}

fn validate_track_events(
    events: &[TrackEventPlan],
    duration_nanos: u64,
    target_ids: &HashSet<&str>,
    path: &str,
    diagnostics: &mut Vec<PlanDiagnostic>,
) {
    validate_ordered_times(
        events.iter().map(TrackEventPlan::at_nanos),
        duration_nanos,
        &format!("{path}.events"),
        diagnostics,
    );
    for (index, event) in events.iter().enumerate() {
        let event_path = format!("{path}.events[{index}]");
        match event {
            TrackEventPlan::Set { value, .. } => {
                validate_scalar(
                    value,
                    target_ids,
                    &format!("{event_path}.value"),
                    diagnostics,
                );
            }
            TrackEventPlan::Spring {
                target,
                response_seconds,
                damping_ratio,
                position_threshold,
                velocity_threshold,
                ..
            } => {
                validate_scalar(
                    target,
                    target_ids,
                    &format!("{event_path}.target"),
                    diagnostics,
                );
                if !response_seconds.is_finite()
                    || *response_seconds <= 0.0
                    || !(std::f32::consts::TAU / response_seconds).is_finite()
                {
                    diagnostics.push(PlanDiagnostic::new(
                        "invalid-spring",
                        format!("{event_path}.responseSeconds"),
                        "spring response must be positive, finite, and have a representable angular frequency",
                    ));
                }
                if !damping_ratio.is_finite() || *damping_ratio <= 0.0 || *damping_ratio > 1.0 {
                    diagnostics.push(PlanDiagnostic::new(
                        "invalid-spring",
                        format!("{event_path}.dampingRatio"),
                        "spring damping ratio must be finite and in (0, 1]",
                    ));
                }
                if !position_threshold.is_finite()
                    || *position_threshold <= 0.0
                    || !velocity_threshold.is_finite()
                    || *velocity_threshold <= 0.0
                {
                    diagnostics.push(PlanDiagnostic::new(
                        "invalid-spring",
                        event_path,
                        "spring thresholds must be positive and finite",
                    ));
                }
            }
            TrackEventPlan::Ease {
                target,
                duration_nanos,
                curve,
                ..
            } => {
                validate_scalar(
                    target,
                    target_ids,
                    &format!("{event_path}.target"),
                    diagnostics,
                );
                if *duration_nanos == 0 || !curve.is_valid() {
                    diagnostics.push(PlanDiagnostic::new(
                        "invalid-ease",
                        event_path,
                        "an ease needs a positive duration and a curve that rises from 0 to 1",
                    ));
                }
            }
        }
    }
}

fn validate_scalar(
    scalar: &ScalarPlan,
    target_ids: &HashSet<&str>,
    path: &str,
    diagnostics: &mut Vec<PlanDiagnostic>,
) {
    match scalar {
        ScalarPlan::Literal(value) if !value.is_finite() => diagnostics.push(PlanDiagnostic::new(
            "non-finite-value",
            path,
            "scalar value must be finite",
        )),
        ScalarPlan::Target(target) => {
            if !target_ids.contains(target.target_id.as_str()) {
                diagnostics.push(PlanDiagnostic::new(
                    "unknown-semantic-target",
                    format!("{path}.targetId"),
                    format!(
                        "scalar references unknown semantic target '{}'",
                        target.target_id
                    ),
                ));
            }
            if !target.offset.is_finite() {
                diagnostics.push(PlanDiagnostic::new(
                    "non-finite-value",
                    format!("{path}.offset"),
                    "semantic target offset must be finite",
                ));
            }
        }
        ScalarPlan::Literal(_) => {}
    }
}

fn validate_ordered_times(
    times: impl IntoIterator<Item = u64>,
    duration_nanos: u64,
    path: &str,
    diagnostics: &mut Vec<PlanDiagnostic>,
) {
    let mut previous = None;
    for (index, at) in times.into_iter().enumerate() {
        if at > duration_nanos {
            diagnostics.push(PlanDiagnostic::new(
                "event-after-scene",
                format!("{path}[{index}].atNanos"),
                format!("event at {at}ns exceeds scene duration {duration_nanos}ns"),
            ));
        }
        if previous.is_some_and(|value| at < value) {
            diagnostics.push(PlanDiagnostic::new(
                "unordered-events",
                format!("{path}[{index}].atNanos"),
                "events must be ordered by time; equal timestamps preserve source order",
            ));
        }
        previous = Some(at);
    }
}

fn validate_range(
    start: u64,
    end: u64,
    maximum_end: u64,
    path: &str,
    diagnostics: &mut Vec<PlanDiagnostic>,
) {
    if start >= end {
        diagnostics.push(PlanDiagnostic::new(
            "invalid-range",
            path,
            format!("range {start}ns..{end}ns must have positive duration"),
        ));
    }
    if end > maximum_end {
        diagnostics.push(PlanDiagnostic::new(
            "range-after-scene",
            path,
            format!("range end {end}ns exceeds scene duration {maximum_end}ns"),
        ));
    }
}

fn validate_id(id: &str, path: &str, kind: &str, diagnostics: &mut Vec<PlanDiagnostic>) {
    if id.is_empty() {
        diagnostics.push(PlanDiagnostic::new(
            "empty-id",
            path,
            format!("{kind} ID must not be empty"),
        ));
    } else if id.chars().any(char::is_whitespace) {
        diagnostics.push(PlanDiagnostic::new(
            "invalid-id",
            path,
            format!("{kind} ID '{id}' must not contain whitespace"),
        ));
    }
}

fn diff_values(
    path: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    changes: &mut Vec<PlanChange>,
) {
    match (before, after) {
        (Some(Value::Object(before)), Some(Value::Object(after))) => {
            let keys = before
                .keys()
                .chain(after.keys())
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            for key in keys {
                diff_values(
                    &format!("{path}.{key}"),
                    before.get(key),
                    after.get(key),
                    changes,
                );
            }
        }
        (Some(Value::Array(before)), Some(Value::Array(after))) => {
            for index in 0..before.len().max(after.len()) {
                diff_values(
                    &format!("{path}[{index}]"),
                    before.get(index),
                    after.get(index),
                    changes,
                );
            }
        }
        (Some(before), Some(after)) if before == after => {}
        (before, after) => changes.push(PlanChange {
            path: path.to_owned(),
            before: before.cloned(),
            after: after.cloned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ActorPlan, ContinuousChannelPlan, CuePlan, ScalarPlan, ScenePlan, SemanticTargetPlan,
        StateChannelPlan, StateEventPlan, TargetComponentPlan, TargetScalarPlan, TrackEventPlan,
    };

    fn plan() -> ScenePlan {
        let mut plan = ScenePlan::new("demo", 2_000_000_000);
        plan.actors.push(ActorPlan {
            id: "title".to_owned(),
            recipe: "text".to_owned(),
            data: json!({ "text": "Hello" }),
        });
        plan.continuous_channels.push(ContinuousChannelPlan {
            id: "title.opacity".to_owned(),
            actor_id: "title".to_owned(),
            property: "opacity".to_owned(),
            initial: 0.0.into(),
            events: vec![TrackEventPlan::Spring {
                at_nanos: 500_000_000,
                target: 1.0.into(),
                response_seconds: 0.4,
                damping_ratio: 1.0,
                position_threshold: 0.01,
                velocity_threshold: 0.01,
            }],
        });
        plan.state_channels.push(StateChannelPlan {
            id: "title.content".to_owned(),
            actor_id: "title".to_owned(),
            state: "content".to_owned(),
            initial: json!("Hello"),
            events: vec![StateEventPlan {
                at_nanos: 1_000_000_000,
                value: json!("World"),
            }],
        });
        plan.cues.push(CuePlan {
            id: "reveal".to_owned(),
            start_nanos: 500_000_000,
            end_nanos: 1_500_000_000,
        });
        plan
    }

    #[test]
    fn valid_plan_round_trips_as_deterministic_json() {
        let plan = plan();
        plan.validate().unwrap();
        let json = plan.to_json_pretty().unwrap();
        let decoded = ScenePlan::from_json(&json).unwrap();

        assert!(json.contains("\"responseSeconds\""));
        assert!(!json.contains("response_seconds"));
        assert_eq!(decoded.to_json_pretty().unwrap(), json);
    }

    #[test]
    fn unrepresentable_spring_frequency_returns_a_structured_diagnostic() {
        let mut plan = plan();
        let TrackEventPlan::Spring {
            response_seconds, ..
        } = &mut plan.continuous_channels[0].events[0]
        else {
            unreachable!()
        };
        *response_seconds = 1e-38;
        let error = plan.validate().unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "invalid-spring"
                    && diagnostic.path.ends_with("responseSeconds"))
        );
    }

    #[test]
    fn validation_returns_structured_paths_and_suggestions() {
        let mut plan = plan();
        plan.continuous_channels[0].actor_id = "missing".to_owned();
        let error = plan.validate().unwrap_err();
        let diagnostic = error
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code == "unknown-actor")
            .unwrap();

        assert_eq!(diagnostic.path, "continuousChannels[0].actorId");
        assert_eq!(diagnostic.suggestions, ["title"]);
    }

    #[test]
    fn equal_event_times_are_ordered_and_preserved() {
        let mut plan = plan();
        plan.continuous_channels[0].events = vec![
            TrackEventPlan::Set {
                at_nanos: 1_000_000_000,
                value: 0.5.into(),
            },
            TrackEventPlan::Set {
                at_nanos: 1_000_000_000,
                value: 1.0.into(),
            },
        ];

        plan.validate().unwrap();
        assert_eq!(plan.continuous_channels[0].events.len(), 2);
    }

    #[test]
    fn unordered_events_are_rejected() {
        let mut plan = plan();
        plan.continuous_channels[0].events = vec![
            TrackEventPlan::Set {
                at_nanos: 1_500_000_000,
                value: 1.0.into(),
            },
            TrackEventPlan::Set {
                at_nanos: 500_000_000,
                value: 0.0.into(),
            },
        ];

        let error = plan.validate().unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "unordered-events")
        );
    }

    #[test]
    fn duplicate_actor_properties_are_rejected_even_with_unique_channel_ids() {
        let mut plan = plan();
        let mut duplicate = plan.continuous_channels[0].clone();
        duplicate.id = "other-opacity-channel".to_owned();
        plan.continuous_channels.push(duplicate);

        let error = plan.validate().unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "duplicate-actor-property")
        );
    }

    #[test]
    fn unsupported_overdamped_springs_are_rejected_before_compilation() {
        let mut plan = plan();
        let TrackEventPlan::Spring { damping_ratio, .. } =
            &mut plan.continuous_channels[0].events[0]
        else {
            unreachable!();
        };
        *damping_ratio = 1.1;

        let error = plan.validate().unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.path.ends_with(".dampingRatio"))
        );

        let TrackEventPlan::Spring { damping_ratio, .. } =
            &mut plan.continuous_channels[0].events[0]
        else {
            unreachable!();
        };
        *damping_ratio = 0.0;
        assert!(plan.validate().is_err());
    }

    #[test]
    fn semantic_target_scalars_round_trip_and_validate_references() {
        let mut plan = plan();
        plan.semantic_targets.push(SemanticTargetPlan {
            id: "title-text".to_owned(),
            actor_id: "title".to_owned(),
            selector: json!({ "rangeId": "title" }),
        });
        plan.continuous_channels[0].initial = ScalarPlan::Target(TargetScalarPlan {
            target_id: "title-text".to_owned(),
            component: TargetComponentPlan::CenterX,
            offset: 40.0,
        });
        plan.validate().unwrap();
        let json = plan.to_json_pretty().unwrap();
        let decoded = ScenePlan::from_json(&json).unwrap();
        assert!(json.contains("\"component\": \"center-x\""));
        assert_eq!(decoded.to_json_pretty().unwrap(), json);

        plan.semantic_targets.clear();
        let error = plan.validate().unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "unknown-semantic-target")
        );
    }

    #[test]
    fn diff_is_stable_and_distinguishes_missing_values_from_null() {
        let before = plan();
        let mut after = plan();
        after.actors[0].data = json!({ "text": null, "color": "white" });
        after.cues[0].end_nanos = 1_750_000_000;

        let changes = before.diff(&after).unwrap();
        assert_eq!(
            changes
                .iter()
                .map(|change| change.path.as_str())
                .collect::<Vec<_>>(),
            [
                "$.actors[0].data.color",
                "$.actors[0].data.text",
                "$.cues[0].endNanos"
            ]
        );
        assert_eq!(changes[0].before, None);
        assert_eq!(changes[0].after, Some(json!("white")));
        assert_eq!(changes[1].after, Some(json!(null)));
    }

    #[test]
    fn presentation_metadata_is_optional_and_does_not_change_existing_json() {
        let original = plan().to_json_pretty().unwrap();
        assert!(!original.contains("presentationSteps"));
        let mut plan = ScenePlan::from_json(&original).unwrap();
        plan.presentation_steps = vec![
            super::PresentationStepPlan {
                id: "initial".into(),
                title: "Initial state".into(),
                start_nanos: 0,
                hold_nanos: 0,
            },
            super::PresentationStepPlan {
                id: "reveal".into(),
                title: "Reveal".into(),
                start_nanos: 500_000_000,
                hold_nanos: 2_000_000_000,
            },
        ];
        let json = plan.to_json_pretty().unwrap();
        assert_eq!(
            ScenePlan::from_json(&json)
                .unwrap()
                .to_json_pretty()
                .unwrap(),
            json
        );
        plan.presentation_steps.clear();
        assert_eq!(plan.to_json_pretty().unwrap(), original);
    }

    #[test]
    fn presentation_steps_validate_identity_titles_ranges_and_order() {
        let mut plan = plan();
        plan.presentation_steps = vec![
            super::PresentationStepPlan {
                id: "first".into(),
                title: "First".into(),
                start_nanos: 0,
                hold_nanos: 1_000_000_000,
            },
            super::PresentationStepPlan {
                id: "first".into(),
                title: " ".into(),
                start_nanos: 500_000_000,
                hold_nanos: 3_000_000_000,
            },
        ];
        let error = plan.validate().unwrap_err();
        for code in [
            "duplicate-presentation-step-id",
            "empty-step-title",
            "invalid-step-range",
            "overlapping-presentation-steps",
        ] {
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "{code}"
            );
        }
        plan.presentation_steps[0].start_nanos = 1_500_000_000;
        assert!(
            plan.validate()
                .unwrap_err()
                .diagnostics()
                .iter()
                .any(|d| d.path == "presentationSteps[0]" && d.code == "invalid-step-range")
        );
    }
}

#[cfg(test)]
mod reel_tests {
    use super::{
        ReelLayer, ReelPlan, ReelSegmentPlan, ReelSpan,
        ReelTransitionStyle::{self, Crossfade, Dip},
        ScenePlan,
    };

    const SECOND: u64 = 1_000_000_000;

    fn reel(segments: &[(&str, u64, u64)]) -> ReelPlan {
        ReelPlan {
            version: ReelPlan::VERSION,
            id: "walkthrough".to_owned(),
            segments: segments
                .iter()
                .map(|&(id, duration, transition)| ReelSegmentPlan {
                    transition_nanos: transition,
                    transition_style: ReelTransitionStyle::default(),
                    transition_focus: None,
                    transition_wipe: None,
                    plan: ScenePlan::new(id, duration),
                })
                .collect(),
        }
    }

    #[test]
    fn transitions_overlap_neighbors_on_one_clock() {
        let reel = reel(&[
            ("intro", 4 * SECOND, 0),
            ("a", 6 * SECOND, SECOND),
            ("b", 3 * SECOND, 0),
        ]);
        reel.validate().unwrap();
        assert_eq!(
            reel.spans(),
            vec![
                ReelSpan {
                    start_nanos: 0,
                    end_nanos: 4 * SECOND,
                    transition_nanos: 0,
                    transition_style: Crossfade,
                    transition_focus: None,
                },
                ReelSpan {
                    start_nanos: 3 * SECOND,
                    end_nanos: 9 * SECOND,
                    transition_nanos: SECOND,
                    transition_style: Crossfade,
                    transition_focus: None,
                },
                ReelSpan {
                    start_nanos: 9 * SECOND,
                    end_nanos: 12 * SECOND,
                    transition_nanos: 0,
                    transition_style: Crossfade,
                    transition_focus: None,
                },
            ]
        );
        assert_eq!(reel.duration_nanos(), 12 * SECOND);
    }

    #[test]
    fn crossfade_mixes_the_incoming_segment_over_the_outgoing_one() {
        let reel = reel(&[("intro", 4 * SECOND, 0), ("a", 6 * SECOND, SECOND)]);
        let only = |segment, local_seconds| {
            vec![ReelLayer {
                segment,
                local_seconds,
                weight: 1.0,
                zoom: None,
                wipe: None,
                transition: None,
            }]
        };
        assert_eq!(reel.layers_at(1.0), only(0, 1.0));
        let middle = reel.layers_at(3.5);
        assert_eq!(middle.len(), 2);
        assert_eq!((middle[0].segment, middle[0].weight), (0, 1.0));
        assert_eq!(middle[1].segment, 1);
        assert!((middle[1].weight - 0.5).abs() < 1e-6);
        assert!((middle[1].local_seconds - 0.5).abs() < 1e-9);
        assert!(
            reel.layers_at(3.1)[1].weight < 0.1,
            "smoothstep starts gently"
        );
        assert_eq!(
            reel.layers_at(3.0),
            only(0, 3.0),
            "the first instant is still the outgoing segment"
        );
        assert_eq!(reel.layers_at(4.0), only(1, 1.0));
        assert_eq!(
            reel.layers_at(9.0),
            only(1, 6.0),
            "the final frame samples the end"
        );
    }

    #[test]
    fn dip_passes_through_the_background_without_overlap() {
        let mut reel = reel(&[("intro", 4 * SECOND, 0), ("a", 6 * SECOND, SECOND)]);
        reel.segments[1].transition_style = Dip;
        let early = reel.layers_at(3.25);
        assert_eq!(early.len(), 1);
        assert_eq!(early[0].segment, 0);
        assert!((early[0].weight - 0.5).abs() < 1e-6);
        let midpoint = reel.layers_at(3.5);
        assert_eq!(midpoint.len(), 1);
        assert!(
            midpoint[0].weight.abs() < 1e-6,
            "the midpoint is the empty background"
        );
        let late = reel.layers_at(3.75);
        assert_eq!((late[0].segment, late.len()), (1, 1));
        assert!((late[0].weight - 0.5).abs() < 1e-6);
        assert!((late[0].local_seconds - 0.75).abs() < 1e-9);
    }

    #[test]
    fn zoom_opens_the_incoming_segment_out_of_the_focus_rectangle() {
        use super::ReelZoom;
        let focus = [240.0, 360.0, 480.0, 270.0];
        let start = ReelZoom::at(focus, 1920.0, 1080.0, 0.0, true);
        // The incoming frame begins exactly inside the focus rectangle.
        assert!((start.scale - 0.25).abs() < 1e-6);
        assert!((start.offset[0] - 240.0).abs() < 1e-3 && (start.offset[1] - 360.0).abs() < 1e-3);
        let end = ReelZoom::at(focus, 1920.0, 1080.0, 1.0, true);
        assert!((end.scale - 1.0).abs() < 1e-6 && end.offset[0].abs() < 1e-3 && end.radius == 0.0);
        let out = ReelZoom::at(focus, 1920.0, 1080.0, 1.0, false);
        // The outgoing frame magnifies the focus rectangle to fill the screen.
        assert!((out.scale - 4.0).abs() < 1e-5);
        assert!((240.0 * out.scale + out.offset[0]).abs() < 1e-2);
        let mut zoom = reel(&[("stage", 4 * SECOND, 0), ("code", 4 * SECOND, SECOND)]);
        zoom.segments[1].transition_style = super::ReelTransitionStyle::Zoom;
        assert!(zoom.validate().is_err(), "a zoom needs a focus rectangle");
        zoom.segments[1].transition_focus = Some(focus);
        zoom.validate().unwrap();
        let layers = zoom.layers_at(3.5);
        assert_eq!(layers.len(), 2);
        assert!(layers[0].zoom.is_some_and(|phase| !phase.incoming));
        assert!(layers[1].zoom.is_some_and(|phase| phase.incoming));
    }

    #[test]
    fn a_wipe_shows_the_incoming_segment_behind_a_held_divider() {
        use super::{ReelWipePlan, WipeDirection};
        let mut wiped = reel(&[("before", 6 * SECOND, 0), ("after", 6 * SECOND, 3 * SECOND)]);
        wiped.segments[1].transition_style = super::ReelTransitionStyle::Wipe;
        wiped.segments[1].transition_wipe =
            Some(ReelWipePlan::new(WipeDirection::Left).hold(0.5, 2 * SECOND));
        wiped.validate().unwrap();
        // The transition starts at 3 s: half-frame sweeps of 0.5 s around the hold.
        assert_eq!(wiped.layers_at(3.0).len(), 1, "nothing is wiped yet");
        let held = wiped.layers_at(4.5);
        assert_eq!(held.len(), 2);
        assert_eq!((held[0].segment, held[0].weight), (0, 1.0));
        let phase = held[1].wipe.unwrap();
        assert_eq!(
            (phase.position, phase.direction),
            (0.5, WipeDirection::Left)
        );
        assert!(
            (held[1].local_seconds - 1.5).abs() < 1e-9,
            "both clocks run"
        );
        assert_eq!(wiped.layers_at(6.0).len(), 1);
        wiped.segments[1].transition_wipe = Some(ReelWipePlan::default().hold(0.5, 3 * SECOND));
        assert!(wiped.validate().is_err(), "holds leave no time to sweep");
        wiped.segments[1].transition_style = super::ReelTransitionStyle::Dip;
        wiped.segments[1].transition_wipe = Some(ReelWipePlan::default());
        assert!(
            wiped.validate().is_err(),
            "wipe settings need the wipe style"
        );
    }

    #[test]
    fn invalid_reels_are_rejected() {
        assert!(reel(&[]).validate().is_err());
        assert!(
            reel(&[("a", SECOND, 1)]).validate().is_err(),
            "first segment cannot fade in"
        );
        assert!(
            reel(&[("a", SECOND, 0), ("a", SECOND, 0)])
                .validate()
                .is_err(),
            "IDs repeat"
        );
        assert!(
            reel(&[("a", SECOND, 0), ("b", 3 * SECOND, 2 * SECOND)])
                .validate()
                .is_err(),
            "transition longer than the previous segment"
        );
        assert!(
            reel(&[
                ("a", 3 * SECOND, 0),
                ("b", 2 * SECOND, 2 * SECOND),
                ("c", 4 * SECOND, SECOND)
            ])
            .validate()
            .is_err(),
            "three segments would overlap"
        );
        let mut versioned = reel(&[("a", SECOND, 0)]);
        versioned.version = 2;
        assert!(versioned.validate().is_err());
    }

    fn every_style() -> Vec<ReelSegmentPlan> {
        use super::{ReelWipePlan, WipeDirection::*};
        let plan = |id: &str| ScenePlan::new(id, 4 * SECOND);
        let card = [300.0, 400.0, 340.0, 124.0];
        vec![
            ReelSegmentPlan::cut(plan("cut")),
            ReelSegmentPlan::crossfaded(plan("crossfade"), SECOND),
            ReelSegmentPlan::dipped(plan("dip"), SECOND),
            ReelSegmentPlan::zoomed(plan("zoom"), SECOND, card),
            ReelSegmentPlan::wiped(plan("wipe"), SECOND, ReelWipePlan::new(Up)),
            ReelSegmentPlan::j_cut(plan("j-cut"), SECOND),
            ReelSegmentPlan::l_cut(plan("l-cut"), SECOND),
            ReelSegmentPlan::pushed(plan("push"), SECOND, Left),
            ReelSegmentPlan::slid(plan("slide"), SECOND, Down),
            ReelSegmentPlan::whipped(plan("whip"), SECOND, Right),
            ReelSegmentPlan::irised(plan("iris"), SECOND, true).focused(card),
            ReelSegmentPlan::matched(plan("match"), SECOND, card, [560.0, 200.0, 800.0, 292.0]),
            ReelSegmentPlan::flipped(plan("flip"), SECOND, Left),
            ReelSegmentPlan::cubed(plan("cube"), SECOND, Up),
            ReelSegmentPlan::inked(plan("ink"), SECOND),
            ReelSegmentPlan::glitched(plan("glitch"), SECOND),
            ReelSegmentPlan::flashed(plan("flash"), SECOND),
            ReelSegmentPlan::leaked(plan("leak"), SECOND),
        ]
    }

    #[test]
    fn constructors_build_a_valid_reel_of_every_transition() {
        let reel = ReelPlan::new("every", every_style()).unwrap();
        // Each 4 s segment overlaps its predecessor by its 1 s transition.
        assert_eq!(reel.duration_nanos(), 4 * SECOND + 17 * 3 * SECOND);
        for (index, span) in reel.spans().into_iter().enumerate().skip(1) {
            let start = span.start_nanos as f64 / 1e9;
            for step in 0..=40 {
                let at = start + f64::from(step) / 40.0;
                let layers = reel.layers_at(at);
                assert!(!layers.is_empty() && layers.len() <= 2, "{index} at {at}");
                // Outgoing first; only the incoming layer carries a phase.
                assert!(
                    layers
                        .windows(2)
                        .all(|pair| pair[0].segment + 1 == pair[1].segment)
                );
                assert!(layers[0].transition.is_none());
                if let [_, incoming] = layers.as_slice()
                    && let Some(phase) = incoming.transition
                {
                    assert_eq!(phase.style, span.transition_style);
                    assert!(phase.progress > 0.0 && phase.progress < 1.0);
                    assert_eq!(phase.seconds, 1.0);
                }
            }
            // Every transition starts on the outgoing frame and ends on the incoming.
            assert_eq!(reel.layers_at(start)[0].segment, index - 1);
            let after = reel.layers_at(start + 1.0);
            assert_eq!((after.len(), after[0].segment), (1, index));
        }
    }

    #[test]
    fn composited_transitions_carry_their_progress_and_focus() {
        let segments = every_style();
        let iris = &segments[10];
        let reel = ReelPlan::new("iris", vec![segments[0].clone(), iris.clone()]).unwrap();
        let layers = reel.layers_at(3.25);
        let phase = layers[1].transition.unwrap();
        assert_eq!(phase.style, ReelTransitionStyle::Iris { ring: true });
        assert!((phase.progress - 0.25).abs() < 1e-6);
        assert_eq!(phase.focus, iris.transition_focus);
        assert!(layers.iter().all(|layer| layer.weight == 1.0));
        assert_eq!(layers[1].local_seconds, 0.25);
    }

    #[test]
    fn j_and_l_cuts_overlap_sound_but_cut_the_picture() {
        let segments = every_style();
        let j = ReelPlan::new("j", vec![segments[0].clone(), segments[5].clone()]).unwrap();
        // The incoming segment starts at 3 s but is not seen until 4 s.
        assert_eq!(j.spans()[1].start_nanos, 3 * SECOND);
        assert_eq!(j.layers_at(3.9)[0].segment, 0);
        assert_eq!(j.layers_at(3.9).len(), 1);
        let cut = j.layers_at(4.0);
        assert_eq!((cut[0].segment, cut[0].local_seconds), (1, 1.0));
        let l = ReelPlan::new("l", vec![segments[0].clone(), segments[6].clone()]).unwrap();
        assert_eq!(l.layers_at(3.0)[0].segment, 0, "the first instant");
        let early = l.layers_at(3.1);
        assert_eq!((early.len(), early[0].segment), (1, 1));
    }

    #[test]
    fn transition_settings_are_strict() {
        let segments = every_style();
        let with =
            |segment: ReelSegmentPlan| ReelPlan::new("strict", vec![segments[0].clone(), segment]);
        let plan = || ScenePlan::new("next", 4 * SECOND);
        let push = ReelSegmentPlan::pushed(plan(), SECOND, super::WipeDirection::Left);
        assert!(
            with(push.focused([0.0, 0.0, 100.0, 100.0])).is_err(),
            "a push has no focus"
        );
        let mut matched = segments[11].clone();
        matched.transition_focus = None;
        assert!(with(matched).is_err(), "a match needs its source rectangle");
        let tiny = ReelSegmentPlan::matched(plan(), SECOND, [0.0; 4], [0.0, 0.0, 100.0, 100.0]);
        assert!(with(tiny).is_err());
        let nowhere = ReelSegmentPlan::matched(
            plan(),
            SECOND,
            [0.0, 0.0, 100.0, 100.0],
            [f32::NAN, 0.0, 100.0, 100.0],
        );
        assert!(with(nowhere).is_err());
        let bad_iris = ReelSegmentPlan::irised(plan(), SECOND, false).focused([0.0, 0.0, 2.0, 2.0]);
        assert!(with(bad_iris).is_err());
        assert!(with(ReelSegmentPlan::irised(plan(), SECOND, false)).is_ok());
        assert!(
            with(ReelSegmentPlan::inked(plan(), 5 * SECOND)).is_err(),
            "too long"
        );
    }

    #[test]
    fn transition_styles_serialize_as_names_or_one_key_objects() {
        let reel = ReelPlan::new("every", every_style()).unwrap();
        let json = serde_json::to_value(&reel).unwrap();
        let styles = json["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|segment| segment["transitionStyle"].to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            styles,
            [
                r#""crossfade""#,
                r#""crossfade""#,
                r#""dip""#,
                r#""zoom""#,
                r#""wipe""#,
                r#""j-cut""#,
                r#""l-cut""#,
                r#"{"push":"left"}"#,
                r#"{"slide":"down"}"#,
                r#"{"whip":"right"}"#,
                r#"{"iris":{"ring":true}}"#,
                r#"{"match":[560.0,200.0,800.0,292.0]}"#,
                r#"{"flip":"left"}"#,
                r#"{"cube":"up"}"#,
                r#""ink""#,
                r#""glitch""#,
                r#""flash""#,
                r#""light-leak""#,
            ]
        );
        let decoded: ReelPlan = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.spans(), reel.spans());
        let plain: ReelTransitionStyle = serde_json::from_str(r#"{"iris":{}}"#).unwrap();
        assert_eq!(plain, ReelTransitionStyle::Iris { ring: false });
        assert!(serde_json::from_str::<ReelTransitionStyle>(r#"{"push":"sideways"}"#).is_err());
    }

    #[test]
    fn earlier_reel_json_still_reads_the_same() {
        // A segment written before composited transitions existed.
        let json = r#"{ "version": 1, "id": "old", "segments": [
            { "transitionNanos": 0, "transitionStyle": "dip",
              "plan": { "version": 2, "id": "a", "durationNanos": 4000000000 } },
            { "transitionNanos": 1000000000, "transitionStyle": "zoom",
              "transitionFocus": [240, 360, 480, 270],
              "plan": { "version": 2, "id": "b", "durationNanos": 4000000000 } },
            { "transitionNanos": 1000000000, "transitionStyle": "wipe",
              "transitionWipe": { "direction": "left" },
              "plan": { "version": 2, "id": "c", "durationNanos": 4000000000 } },
            { "transitionNanos": 500000000,
              "plan": { "version": 2, "id": "d", "durationNanos": 4000000000 } }
        ] }"#;
        let reel: ReelPlan = serde_json::from_str(json).unwrap();
        reel.validate().unwrap();
        let styles = reel
            .segments
            .iter()
            .map(|segment| segment.transition_style)
            .collect::<Vec<_>>();
        assert_eq!(
            styles,
            [
                Dip,
                ReelTransitionStyle::Zoom,
                ReelTransitionStyle::Wipe,
                Crossfade
            ]
        );
        for at in [3.5, 6.5, 9.75] {
            assert!(
                reel.layers_at(at)
                    .iter()
                    .all(|layer| layer.transition.is_none())
            );
        }
        let rewritten = serde_json::to_value(&reel).unwrap();
        assert_eq!(rewritten["segments"][1]["transitionStyle"], "zoom");
    }

    #[test]
    fn reel_json_is_camel_case_and_strict() {
        let reel = reel(&[("intro", SECOND, 0), ("a", SECOND, 250_000_000)]);
        let json = serde_json::to_string(&reel).unwrap();
        assert!(json.contains("\"transitionNanos\":250000000"));
        let decoded: ReelPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.spans(), reel.spans());
        let unknown = json.replacen("\"segments\"", "\"extra\":1,\"segments\"", 1);
        assert!(serde_json::from_str::<ReelPlan>(&unknown).is_err());
    }
}
