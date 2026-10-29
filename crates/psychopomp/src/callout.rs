//! Callouts: a short label on a crisp leader line pinned to something on
//! screen. A callout names its anchors (a fixed point, a Stage element's edge,
//! or an editor Semantic Target) and blends between them with `anchor.<id>`
//! weight channels. Where an anchor is at a given time is layout only the
//! renderer knows (Stage projection, editor glyph geometry), so the renderer
//! resolves every anchor at every sample and the leader never lags its target.
//! The anchor model is shared with other overlays in [`crate::anchor`].
use std::collections::HashSet;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

/// A compass direction: as an anchor `edge`, a point on an outline; as a label
/// `side`, the direction from the anchor to the label (never `center`).
pub use crate::anchor::Edge as CalloutSide;
use crate::{
    anchor::{self, AnchorTarget},
    author::{ActorHandle, ContinuousHandle, PlanBuilder},
    caption::CaptionSpanPlan,
    math::{Vec2, easing::Ease, shapes::Box2, vec2},
    stage::DRAW_CURVE,
    tone::Tone,
};

pub const CALLOUT_RECIPE: &str = "callout";

/// Horizontal run of an elbowed leader into its label.
pub const SHELF: f32 = 28.0;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalloutPlan {
    /// Where the callout can point. The first is where it starts.
    pub anchors: Vec<CalloutAnchorPlan>,
    pub lines: Vec<Vec<CaptionSpanPlan>>,
    #[serde(default = "default_size")]
    pub size: f32,
    /// Where the label sits relative to its anchor.
    #[serde(default = "default_side")]
    pub side: CalloutSide,
    /// Length of the leader's first leg, in pixels.
    #[serde(default = "default_reach")]
    pub reach: f32,
    /// Diagonal leaders turn into a short horizontal shelf before the label.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub elbow: bool,
    /// Color of the leader and the anchor mark.
    #[serde(default = "accent", skip_serializing_if = "is_accent")]
    pub tone: Tone,
    /// A rounded surface behind the label, for legibility over busy frames.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chip: bool,
}

/// One place a callout can point. `edge` picks the point on the target's
/// outline; `side` optionally overrides where the label sits for this anchor,
/// so moving between anchors can also swing the label around.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CalloutAnchorPlan {
    /// A fixed canvas point.
    #[serde(rename_all = "camelCase")]
    Point {
        id: String,
        at: [f32; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<CalloutSide>,
    },
    /// A positioned element of the plan's Stage root, through its camera.
    #[serde(rename_all = "camelCase")]
    Stage {
        id: String,
        element: String,
        #[serde(default, skip_serializing_if = "CalloutSide::is_center")]
        edge: CalloutSide,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<CalloutSide>,
    },
    /// A Semantic Target of the plan's editor root: a logical code range.
    #[serde(rename_all = "camelCase")]
    Editor {
        id: String,
        target: String,
        #[serde(default, skip_serializing_if = "CalloutSide::is_center")]
        edge: CalloutSide,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<CalloutSide>,
    },
    /// A participant's header in a Sequence Diagram actor.
    #[serde(rename_all = "camelCase")]
    Participant {
        id: String,
        sequence: String,
        participant: String,
        #[serde(default, skip_serializing_if = "CalloutSide::is_center")]
        edge: CalloutSide,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<CalloutSide>,
    },
    /// A row of a Sequence Diagram actor: its arrow, note, or End mark.
    #[serde(rename_all = "camelCase")]
    Row {
        id: String,
        sequence: String,
        row: String,
        #[serde(default, skip_serializing_if = "CalloutSide::is_center")]
        edge: CalloutSide,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<CalloutSide>,
    },
    /// A keyed element inside a `RuneSurface` actor or a `StageElement::RuneCard`.
    #[serde(rename_all = "camelCase")]
    Rune {
        id: String,
        actor: String,
        key: String,
        #[serde(default, skip_serializing_if = "CalloutSide::is_center")]
        edge: CalloutSide,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<CalloutSide>,
    },
}

fn default_size() -> f32 {
    24.0
}
fn default_side() -> CalloutSide {
    CalloutSide::TopRight
}
fn default_reach() -> f32 {
    64.0
}
fn accent() -> Tone {
    Tone::Accent
}
fn is_accent(tone: &Tone) -> bool {
    *tone == Tone::Accent
}

impl CalloutAnchorPlan {
    pub fn rune(
        id: impl Into<String>,
        actor: impl Into<String>,
        key: impl Into<String>,
        edge: CalloutSide,
    ) -> Self {
        Self::Rune {
            id: id.into(),
            actor: actor.into(),
            key: key.into(),
            edge,
            side: None,
        }
    }

    pub fn with_side(mut self, label_side: CalloutSide) -> Self {
        match &mut self {
            Self::Point { side, .. }
            | Self::Stage { side, .. }
            | Self::Editor { side, .. }
            | Self::Participant { side, .. }
            | Self::Row { side, .. }
            | Self::Rune { side, .. } => *side = Some(label_side),
        }
        self
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Point { id, .. }
            | Self::Stage { id, .. }
            | Self::Editor { id, .. }
            | Self::Participant { id, .. }
            | Self::Row { id, .. }
            | Self::Rune { id, .. } => id,
        }
    }

    fn side(&self) -> Option<CalloutSide> {
        match self {
            Self::Point { side, .. }
            | Self::Stage { side, .. }
            | Self::Editor { side, .. }
            | Self::Participant { side, .. }
            | Self::Row { side, .. }
            | Self::Rune { side, .. } => *side,
        }
    }

    /// What this anchor points at, for the shared anchor resolution.
    pub fn target(&self) -> AnchorTarget<'_> {
        match self {
            Self::Point { at, .. } => AnchorTarget::Point(Vec2::from(*at)),
            Self::Stage { element, edge, .. } => AnchorTarget::Stage {
                element,
                edge: *edge,
            },
            Self::Editor { target, edge, .. } => AnchorTarget::Editor {
                target,
                edge: *edge,
            },
            Self::Participant {
                sequence,
                participant,
                edge,
                ..
            } => AnchorTarget::Participant {
                sequence,
                participant,
                edge: *edge,
            },
            Self::Row {
                sequence,
                row,
                edge,
                ..
            } => AnchorTarget::Row {
                sequence,
                row,
                edge: *edge,
            },
            Self::Rune {
                actor, key, edge, ..
            } => AnchorTarget::Rune {
                actor,
                key,
                edge: *edge,
            },
        }
    }
}

/// The shape of a leader relative to its anchor: the knee and end of the line,
/// which point of the label meets it (`attach`, a fraction of the label's
/// size), and the direction from the leader's end to the label (`away`).
/// Every field is linear, so anchors with different sides blend smoothly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalloutLeg {
    pub knee: Vec2,
    pub end: Vec2,
    pub attach: Vec2,
    pub away: Vec2,
}

impl CalloutLeg {
    pub fn new(side: CalloutSide, reach: f32, elbow: bool) -> Self {
        let direction = side.unit();
        let [x, y] = side.signs();
        if y == 0.0 || x == 0.0 {
            // Straight out: the label meets the line at its facing edge.
            let end = direction * reach;
            return Self {
                knee: end * 0.5,
                end,
                attach: vec2(0.5 - 0.5 * x, 0.5 - 0.5 * y),
                away: vec2(x, y),
            };
        }
        // Diagonals meet the label at its side, vertically centered.
        let knee = direction * reach;
        let (knee, end) = if elbow {
            (knee, knee + vec2(x * SHELF, 0.0))
        } else {
            (knee * 0.5, knee)
        };
        Self {
            knee,
            end,
            attach: vec2(0.5 - 0.5 * x, 0.5),
            away: vec2(x, 0.0),
        }
    }

    /// The weighted mean of `legs`, or `None` when the weights vanish.
    pub fn blend(legs: impl IntoIterator<Item = (Self, f32)>) -> Option<Self> {
        let mut sum = Self::default();
        let mut total = 0.0;
        for (leg, weight) in legs {
            sum.knee += leg.knee * weight;
            sum.end += leg.end * weight;
            sum.attach += leg.attach * weight;
            sum.away += leg.away * weight;
            total += weight;
        }
        (total.abs() > 1e-6).then(|| Self {
            knee: sum.knee / total,
            end: sum.end / total,
            attach: sum.attach / total,
            away: sum.away / total,
        })
    }
}

/// A callout laid out on the canvas: the leader from the anchor through the
/// knee to its end, and the label's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalloutLayout {
    pub leader: [Vec2; 3],
    pub label: Box2,
}

/// Place a label of `size` at the end of `leg` from `anchor`, `gap` pixels
/// beyond the line. A label that would leave `frame` slides back inside and
/// the leader's knee and end follow it, so line and label stay joined.
pub fn layout(anchor: Vec2, leg: CalloutLeg, size: Vec2, gap: f32, frame: Box2) -> CalloutLayout {
    let end = anchor + leg.end;
    let origin = end + leg.away * gap - leg.attach * size;
    let clamped = origin.clamp(frame.min, (frame.max - size).max(frame.min));
    let shift = clamped - origin;
    CalloutLayout {
        leader: [anchor, anchor + leg.knee + shift, end + shift],
        label: Box2 {
            min: clamped,
            max: clamped + size,
        },
    }
}

impl CalloutPlan {
    /// A one-line callout of `spans` pinned to `anchor`.
    pub fn new(anchor: CalloutAnchorPlan, spans: Vec<CaptionSpanPlan>) -> Self {
        Self {
            anchors: vec![anchor],
            lines: vec![spans],
            size: default_size(),
            side: default_side(),
            reach: default_reach(),
            elbow: false,
            tone: accent(),
            chip: false,
        }
    }

    pub fn anchor(mut self, anchor: CalloutAnchorPlan) -> Self {
        self.anchors.push(anchor);
        self
    }

    pub fn side(mut self, side: CalloutSide) -> Self {
        self.side = side;
        self
    }

    pub fn reach(mut self, reach: f32) -> Self {
        self.reach = reach;
        self
    }

    pub fn elbow(mut self) -> Self {
        self.elbow = true;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn chip(mut self) -> Self {
        self.chip = true;
        self
    }

    pub fn line_height(&self) -> f32 {
        self.size * 1.4
    }

    /// The leader shape while pinned to `anchor`.
    pub fn leg(&self, anchor: &CalloutAnchorPlan) -> CalloutLeg {
        CalloutLeg::new(anchor.side().unwrap_or(self.side), self.reach, self.elbow)
    }

    /// The weight channel that pins the callout to `anchor`.
    pub fn weight_property(anchor: &str) -> String {
        anchor::weight_property(anchor)
    }

    /// True when `property` names one of this callout's channels.
    pub fn accepts(&self, property: &str) -> bool {
        matches!(property, "opacity" | "draw" | "label" | "emphasis")
            || anchor::accepts_weight(property, self.anchors.iter().map(CalloutAnchorPlan::id))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=8).contains(&self.anchors.len()),
            "a callout has one to eight anchors"
        );
        let mut ids = HashSet::new();
        for anchor in &self.anchors {
            let id = anchor.id();
            anchor::validate_id("callout", id, &mut ids)?;
            ensure!(
                anchor.side() != Some(CalloutSide::Center),
                "callout anchor '{id}' cannot put its label at the center"
            );
            anchor::validate_target("callout", id, anchor.target())?;
        }
        ensure!(
            self.side != CalloutSide::Center,
            "a callout label cannot sit at its anchor's center"
        );
        ensure!(
            (12.0..=72.0).contains(&self.size),
            "callout size must be between 12 and 72"
        );
        ensure!(
            (16.0..=600.0).contains(&self.reach),
            "callout reach must be between 16 and 600 pixels"
        );
        ensure!(
            (1..=3).contains(&self.lines.len()),
            "a callout has one to three lines"
        );
        let mut chars = 0;
        for line in &self.lines {
            ensure!(
                line.iter().any(|span| !span.text.is_empty()),
                "callout lines cannot be empty"
            );
            for span in line {
                if span.text.contains(['\n', '\r']) {
                    bail!("callout spans are single-line; use another line instead");
                }
                chars += span.text.chars().count();
            }
        }
        ensure!(chars <= 160, "callouts are limited to 160 characters");
        Ok(())
    }
}

/// Authoring handle for one callout. Channels are declared on first use.
#[derive(Clone, Debug)]
pub struct CalloutActor {
    actor: ActorHandle,
    anchors: Vec<String>,
}

/// How long the leader takes to draw on, and when the label follows it.
const DRAW_SECONDS: f32 = crate::author::DRAW_SECONDS;
const LABEL_DELAY: u64 = 180_000_000;

impl CalloutActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &CalloutPlan,
    ) -> Result<Self> {
        plan.validate()?;
        let actor = scene.actor(id, CALLOUT_RECIPE, plan)?;
        Ok(Self {
            actor,
            anchors: plan.anchors.iter().map(|a| a.id().to_owned()).collect(),
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    /// The channel for `property`, declared on first use with `initial`.
    pub fn channel(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, property, initial)
    }

    /// The anchor mark appears, the leader draws out from it, and the label
    /// lands at its end. A callout with a `show` starts hidden. Returns when
    /// the leader reaches the label.
    pub fn show(&mut self, scene: &mut PlanBuilder, at_nanos: u64) -> u64 {
        let draw = self.channel(scene, "draw", 0.0);
        let label = self.channel(scene, "label", 0.0);
        scene.ease(&draw, at_nanos, 1.0, DRAW_SECONDS, DRAW_CURVE);
        scene.spring(&label, at_nanos + LABEL_DELAY, 1.0, 0.3, 0.0);
        at_nanos + crate::author::whole_millis(DRAW_SECONDS)
    }

    /// The label fades, then the leader retracts into its anchor.
    pub fn hide(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        let draw = self.channel(scene, "draw", 1.0);
        let label = self.channel(scene, "label", 1.0);
        scene.spring(&label, at_nanos, 0.0, 0.2, 0.0);
        scene.ease(&draw, at_nanos + 80_000_000, 0.0, 0.28, Ease::GLIDE);
    }

    /// Glide to `anchor`. Every weight springs on one critically damped
    /// profile, so they keep summing to one, an interrupted move carries its
    /// velocity into the next, and the blended anchor follows both targets.
    pub fn move_to(&mut self, scene: &mut PlanBuilder, anchor: &str, at_nanos: u64) -> Result<()> {
        anchor::move_to(scene, &self.actor, &self.anchors, anchor, at_nanos)
    }

    /// Strike the callout: the anchor ring flares and the leader brightens at
    /// once, then decay with a long tail.
    pub fn emphasize(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        let emphasis = self.channel(scene, "emphasis", 0.0);
        scene.set(&emphasis, at_nanos, 1.0);
        scene.ease(&emphasis, at_nanos, 0.0, 1.1, Ease::CubicOut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::ScalarPlan;

    fn plan() -> CalloutPlan {
        CalloutPlan::new(
            CalloutAnchorPlan::Stage {
                id: "api".into(),
                element: "api".into(),
                edge: CalloutSide::Top,
                side: None,
            },
            vec![CaptionSpanPlan::new("retries here", Tone::Plain)],
        )
        .anchor(CalloutAnchorPlan::Point {
            id: "corner".into(),
            at: [200.0, 200.0],
            side: Some(CalloutSide::BottomRight),
        })
    }

    #[test]
    fn callouts_round_trip_with_compact_defaults() {
        let plan = plan();
        plan.validate().unwrap();
        let json = serde_json::to_value(&plan).unwrap();
        assert!(json.get("tone").is_none() && json.get("elbow").is_none());
        assert_eq!(json["anchors"][0]["kind"], "stage");
        assert_eq!(json["anchors"][0]["edge"], "top");
        assert_eq!(json["anchors"][1]["side"], "bottom-right");
        assert_eq!(serde_json::from_value::<CalloutPlan>(json).unwrap(), plan);
    }

    #[test]
    fn invalid_callouts_are_rejected() {
        let mut centered = plan();
        centered.side = CalloutSide::Center;
        assert!(centered.validate().is_err());
        let mut twice = plan();
        twice.anchors.push(twice.anchors[0].clone());
        assert!(twice.validate().is_err());
        let mut empty = plan();
        empty.lines = vec![vec![CaptionSpanPlan::new("", Tone::Plain)]];
        assert!(empty.validate().is_err());
        assert!(plan().accepts("anchor.corner") && !plan().accepts("anchor.nowhere"));
    }

    #[test]
    fn labels_meet_the_leader_at_their_facing_edge() {
        let frame = Box2 {
            min: Vec2::ZERO,
            max: vec2(1920.0, 1080.0),
        };
        let size = vec2(200.0, 40.0);
        let elbow = CalloutLeg::new(CalloutSide::TopRight, 64.0, true);
        let placed = layout(vec2(500.0, 500.0), elbow, size, 10.0, frame);
        let [anchor, knee, end] = placed.leader;
        assert_eq!(anchor, vec2(500.0, 500.0));
        assert_eq!(knee.y, end.y, "the shelf is horizontal");
        assert_eq!(placed.label.min.x, end.x + 10.0);
        assert_eq!(placed.label.center().y, end.y);
        let top = layout(
            vec2(500.0, 500.0),
            CalloutLeg::new(CalloutSide::Top, 64.0, false),
            size,
            10.0,
            frame,
        );
        assert_eq!(top.label.max.y, 500.0 - 64.0 - 10.0);
        assert_eq!(top.label.center().x, 500.0);
    }

    #[test]
    fn labels_stay_in_frame_and_the_leader_follows() {
        let frame = Box2 {
            min: vec2(40.0, 40.0),
            max: vec2(1880.0, 1040.0),
        };
        let leg = CalloutLeg::new(CalloutSide::TopRight, 64.0, true);
        let placed = layout(vec2(1800.0, 60.0), leg, vec2(300.0, 40.0), 10.0, frame);
        assert!(placed.label.min.cmpge(frame.min).all() && placed.label.max.cmple(frame.max).all());
        let [_, _, end] = placed.leader;
        assert_eq!(
            placed.label.min.x,
            end.x + 10.0,
            "line and label stay joined"
        );
        assert_eq!(placed.label.center().y, end.y);
    }

    #[test]
    fn legs_blend_linearly_between_sides() {
        let left = CalloutLeg::new(CalloutSide::Left, 60.0, false);
        let right = CalloutLeg::new(CalloutSide::Right, 60.0, false);
        let middle = CalloutLeg::blend([(left, 0.5), (right, 0.5)]).unwrap();
        assert_eq!(middle.end, Vec2::ZERO);
        assert_eq!(middle.attach, vec2(0.5, 0.5));
        assert!(CalloutLeg::blend([(left, 0.0)]).is_none());
    }

    #[test]
    fn moving_springs_every_weight_on_one_profile() {
        let mut scene = PlanBuilder::new("callout-demo", 4_000_000_000);
        let mut callout = CalloutActor::declare(&mut scene, "note", &plan()).unwrap();
        let drawn = callout.show(&mut scene, 500_000_000);
        assert_eq!(drawn, 920_000_000);
        callout
            .move_to(&mut scene, "corner", 1_000_000_000)
            .unwrap();
        assert!(
            callout
                .move_to(&mut scene, "nowhere", 2_000_000_000)
                .is_err()
        );
        callout.emphasize(&mut scene, 2_000_000_000);
        callout.hide(&mut scene, 3_000_000_000);
        let plan = scene.finish().unwrap();
        let weight = |id: &str| {
            plan.continuous_channels
                .iter()
                .find(|channel| channel.property == format!("anchor.{id}"))
                .unwrap()
        };
        assert!(matches!(weight("api").initial, ScalarPlan::Literal(1.0)));
        assert!(matches!(weight("corner").initial, ScalarPlan::Literal(0.0)));
        assert_eq!(
            weight("api").events[0].spring_plan(),
            weight("corner").events[0].spring_plan()
        );
    }
}
