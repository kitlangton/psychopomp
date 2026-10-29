//! Anchors: places an overlay can pin to whose position only the renderer
//! knows. A fixed canvas point, an edge of a positioned Stage element seen
//! through the camera, an edge of an editor Semantic Target (a measured code
//! range after line motion and the panel's projection), or a Sequence
//! Diagram's participant header or row. The renderer resolves every anchor at
//! every Temporal Sample, so a pinned overlay never lags its target; `anchor.<id>` weight channels blend the resolved points,
//! and springing those weights moves between anchors with velocity.
//!
//! Callouts, captions, Rolling Numbers, text, and images share this model. An
//! overlay lists its anchors (the first is where it starts) and the renderer
//! asks the prepared root for each weighted one. There is no hierarchy: an
//! anchor names a target, it does not parent one actor to another.
use std::collections::HashSet;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    author::{ActorHandle, PlanBuilder},
    math::{Vec2, shapes::Shape},
    plan::SpringPlan,
};

/// The most anchors one overlay may blend between.
pub const MAX_ANCHORS: usize = 8;

/// A compass point: as an anchor `edge`, a point on an outline (its center, a
/// side's midpoint, or a corner); as a callout label `side`, the direction
/// from the anchor to the label (never `center`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Edge {
    pub fn is_center(&self) -> bool {
        *self == Self::Center
    }

    /// Unit direction on screen (y down); zero for `center`.
    pub fn unit(self) -> Vec2 {
        Vec2::from(self.signs()).normalize_or_zero()
    }

    /// The direction's components, each -1, 0, or 1 (y down).
    pub fn signs(self) -> [f32; 2] {
        match self {
            Self::Center => [0.0, 0.0],
            Self::Top => [0.0, -1.0],
            Self::Bottom => [0.0, 1.0],
            Self::Left => [-1.0, 0.0],
            Self::Right => [1.0, 0.0],
            Self::TopLeft => [-1.0, -1.0],
            Self::TopRight => [1.0, -1.0],
            Self::BottomLeft => [-1.0, 1.0],
            Self::BottomRight => [1.0, 1.0],
        }
    }

    /// This edge of `shape`: a box's side midpoint or corner, the circle's
    /// or polygon's outline in this direction, or the point itself.
    pub fn on(self, shape: Shape) -> Vec2 {
        match shape {
            Shape::Box(bounds) => bounds.center() + Vec2::from(self.signs()) * bounds.extents(),
            Shape::Circle(circle) => circle.center + self.unit() * circle.radius,
            Shape::Point(point) => point,
            Shape::Polygon(_) if self.is_center() => shape.center(),
            Shape::Polygon(polygon) => polygon.along(self.unit()),
        }
    }
}

/// What an anchor points at, borrowed from any overlay's anchor plan. The
/// renderer resolves it against the prepared root; overlays differ only in
/// what they draw at the resolved point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnchorTarget<'a> {
    /// A fixed canvas point.
    Point(Vec2),
    /// A positioned element of the plan's Stage root, through its camera.
    Stage { element: &'a str, edge: Edge },
    /// A Semantic Target of the plan's editor root: a logical code range.
    Editor { target: &'a str, edge: Edge },
    /// A participant's header box in a Sequence Diagram actor.
    Participant {
        sequence: &'a str,
        participant: &'a str,
        edge: Edge,
    },
    /// The span of a row in a Sequence Diagram actor: a message's arrow, a
    /// note's box, or an End mark.
    Row {
        sequence: &'a str,
        row: &'a str,
        edge: Edge,
    },
    /// A keyed element inside a `RuneSurface` actor or a 2.5D `StageElement::RuneCard`.
    Rune {
        actor: &'a str,
        key: &'a str,
        edge: Edge,
    },
}

impl AnchorTarget<'_> {
    /// Whether this anchor moves with the Stage camera or a Rune surface spring.
    pub fn on_stage(&self) -> bool {
        matches!(self, Self::Stage { .. } | Self::Rune { .. })
    }
}

/// One place an overlay can pin to. `edge` picks the point on the target's
/// outline; `offset` (canvas pixels) moves the overlay's origin from it, so
/// a label can sit above one card and below another and glide between them.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AnchorPlan {
    #[serde(rename_all = "camelCase")]
    Point {
        id: String,
        at: [f32; 2],
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: [f32; 2],
    },
    #[serde(rename_all = "camelCase")]
    Stage {
        id: String,
        element: String,
        #[serde(default, skip_serializing_if = "Edge::is_center")]
        edge: Edge,
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: [f32; 2],
    },
    #[serde(rename_all = "camelCase")]
    Editor {
        id: String,
        target: String,
        #[serde(default, skip_serializing_if = "Edge::is_center")]
        edge: Edge,
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: [f32; 2],
    },
    #[serde(rename_all = "camelCase")]
    Participant {
        id: String,
        sequence: String,
        participant: String,
        #[serde(default, skip_serializing_if = "Edge::is_center")]
        edge: Edge,
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: [f32; 2],
    },
    #[serde(rename_all = "camelCase")]
    Row {
        id: String,
        sequence: String,
        row: String,
        #[serde(default, skip_serializing_if = "Edge::is_center")]
        edge: Edge,
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: [f32; 2],
    },
    #[serde(rename_all = "camelCase")]
    Rune {
        id: String,
        actor: String,
        key: String,
        #[serde(default, skip_serializing_if = "Edge::is_center")]
        edge: Edge,
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: [f32; 2],
    },
}

fn is_zero(offset: &[f32; 2]) -> bool {
    *offset == [0.0, 0.0]
}

impl AnchorPlan {
    /// A fixed canvas point.
    pub fn point(id: impl Into<String>, at: [f32; 2]) -> Self {
        Self::Point {
            id: id.into(),
            at,
            offset: [0.0, 0.0],
        }
    }

    /// `edge` of a positioned Stage element.
    pub fn stage(id: impl Into<String>, element: impl Into<String>, edge: Edge) -> Self {
        Self::Stage {
            id: id.into(),
            element: element.into(),
            edge,
            offset: [0.0, 0.0],
        }
    }

    /// `edge` of an editor Semantic Target.
    pub fn editor(id: impl Into<String>, target: impl Into<String>, edge: Edge) -> Self {
        Self::Editor {
            id: id.into(),
            target: target.into(),
            edge,
            offset: [0.0, 0.0],
        }
    }

    /// `edge` of `participant`'s header in the Sequence Diagram `sequence`.
    pub fn participant(
        id: impl Into<String>,
        sequence: impl Into<String>,
        participant: impl Into<String>,
        edge: Edge,
    ) -> Self {
        Self::Participant {
            id: id.into(),
            sequence: sequence.into(),
            participant: participant.into(),
            edge,
            offset: [0.0, 0.0],
        }
    }

    /// `edge` of `row`'s span in the Sequence Diagram `sequence`.
    pub fn row(
        id: impl Into<String>,
        sequence: impl Into<String>,
        row: impl Into<String>,
        edge: Edge,
    ) -> Self {
        Self::Row {
            id: id.into(),
            sequence: sequence.into(),
            row: row.into(),
            edge,
            offset: [0.0, 0.0],
        }
    }

    /// `edge` of a keyed element inside a `RuneSurface` or `StageElement::RuneCard`.
    pub fn rune(
        id: impl Into<String>,
        actor: impl Into<String>,
        key: impl Into<String>,
        edge: Edge,
    ) -> Self {
        Self::Rune {
            id: id.into(),
            actor: actor.into(),
            key: key.into(),
            edge,
            offset: [0.0, 0.0],
        }
    }

    /// Move the pinned origin `offset` canvas pixels from the anchor.
    pub fn with_offset(mut self, offset: [f32; 2]) -> Self {
        match &mut self {
            Self::Point { offset: own, .. }
            | Self::Stage { offset: own, .. }
            | Self::Editor { offset: own, .. }
            | Self::Participant { offset: own, .. }
            | Self::Row { offset: own, .. }
            | Self::Rune { offset: own, .. } => *own = offset,
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

    pub fn offset(&self) -> Vec2 {
        match self {
            Self::Point { offset, .. }
            | Self::Stage { offset, .. }
            | Self::Editor { offset, .. }
            | Self::Participant { offset, .. }
            | Self::Row { offset, .. }
            | Self::Rune { offset, .. } => Vec2::from(*offset),
        }
    }

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

/// The weight channel that pins an overlay to `anchor`.
pub fn weight_property(anchor: &str) -> String {
    format!("anchor.{anchor}")
}

/// A weight channel's value before any event: the first anchor holds the
/// overlay, the others wait at zero.
pub fn initial_weight(index: usize) -> f32 {
    if index == 0 { 1.0 } else { 0.0 }
}

/// True when `property` is the weight channel of one of `anchors`.
pub fn accepts_weight<'a>(property: &str, mut anchors: impl Iterator<Item = &'a str>) -> bool {
    property
        .strip_prefix("anchor.")
        .is_some_and(|id| anchors.any(|anchor| anchor == id))
}

/// True when `property` is the weight channel of one of `anchors`.
pub fn accepts(property: &str, anchors: &[AnchorPlan]) -> bool {
    accepts_weight(property, anchors.iter().map(AnchorPlan::id))
}

/// Check one anchor ID: usable in a channel name and not declared twice.
pub fn validate_id<'a>(kind: &str, id: &'a str, seen: &mut HashSet<&'a str>) -> Result<()> {
    ensure!(
        !id.is_empty() && !id.chars().any(|c| c.is_whitespace() || c == '.'),
        "{kind} anchor ID '{id}' must be non-empty, without whitespace or dots"
    );
    ensure!(seen.insert(id), "{kind} anchor '{id}' is declared twice");
    Ok(())
}

/// Check what an anchor names before any renderer sees it.
pub fn validate_target(kind: &str, id: &str, target: AnchorTarget<'_>) -> Result<()> {
    match target {
        AnchorTarget::Point(at) => ensure!(at.is_finite(), "{kind} anchor '{id}' must be finite"),
        AnchorTarget::Stage { element, .. } => ensure!(
            !element.is_empty(),
            "{kind} anchor '{id}' needs a stage element"
        ),
        AnchorTarget::Editor { target, .. } => ensure!(
            !target.is_empty(),
            "{kind} anchor '{id}' needs a semantic target"
        ),
        AnchorTarget::Participant {
            sequence,
            participant,
            ..
        } => ensure!(
            !sequence.is_empty() && !participant.is_empty(),
            "{kind} anchor '{id}' needs a sequence and a participant"
        ),
        AnchorTarget::Row { sequence, row, .. } => ensure!(
            !sequence.is_empty() && !row.is_empty(),
            "{kind} anchor '{id}' needs a sequence and a row"
        ),
        AnchorTarget::Rune { actor, key, .. } => ensure!(
            !actor.is_empty() && !key.is_empty(),
            "{kind} anchor '{id}' needs a rune actor and element key"
        ),
    }
    Ok(())
}

/// Validate an overlay's optional anchors: at most eight, unique usable IDs,
/// finite points and offsets, and named targets.
pub fn validate(kind: &str, anchors: &[AnchorPlan]) -> Result<()> {
    ensure!(
        anchors.len() <= MAX_ANCHORS,
        "a {kind} has at most {MAX_ANCHORS} anchors"
    );
    let mut ids = HashSet::new();
    for anchor in anchors {
        let id = anchor.id();
        validate_id(kind, id, &mut ids)?;
        validate_target(kind, id, anchor.target())?;
        ensure!(
            anchor.offset().is_finite(),
            "{kind} anchor '{id}' offset must be finite"
        );
    }
    Ok(())
}

/// The weighted mean of resolved anchor points, or `None` when the weights
/// vanish. Weights need not sum to one: the mean normalizes them, so a blend
/// in flight between two moving targets follows both.
pub fn blend(points: impl IntoIterator<Item = (Vec2, f32)>) -> Option<Vec2> {
    let mut sum = Vec2::ZERO;
    let mut total = 0.0;
    for (point, weight) in points {
        sum += point * weight;
        total += weight;
    }
    (total.abs() > 1e-6).then(|| sum / total)
}

/// The profile every anchor weight springs on: critically damped, with
/// thresholds tight enough for a dimensionless weight.
pub fn weight_spring() -> SpringPlan {
    SpringPlan::MOVE.with_thresholds(1e-5, 1e-5)
}

/// Glide `actor` to its anchor `to`. Every weight springs on one critically
/// damped profile, so they keep summing to one, an interrupted move carries
/// its velocity into the next, and the blended point follows both targets.
pub fn move_to(
    scene: &mut PlanBuilder,
    actor: &ActorHandle,
    anchors: &[String],
    to: &str,
    at_nanos: u64,
) -> Result<()> {
    ensure!(
        anchors.iter().any(|id| id == to),
        "actor '{}' has no anchor '{to}'",
        actor.id()
    );
    let spring = weight_spring();
    for (index, id) in anchors.iter().enumerate() {
        let weight = scene.channel(actor, &weight_property(id), initial_weight(index));
        scene.spring_with(&weight, at_nanos, f32::from(id == to), spring);
    }
    Ok(())
}

/// The anchor IDs of `anchors`, in order, for an authoring handle.
pub fn ids(anchors: &[AnchorPlan]) -> Vec<String> {
    anchors
        .iter()
        .map(|anchor| anchor.id().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        math::{
            shapes::{Box2, Circle},
            vec2,
        },
        plan::ScalarPlan,
    };

    #[test]
    fn anchors_round_trip_with_compact_defaults() {
        let anchors = vec![
            AnchorPlan::stage("api", "api", Edge::Bottom).with_offset([0.0, 24.0]),
            AnchorPlan::editor("call", "fetch", Edge::Center),
            AnchorPlan::point("corner", [40.0, 60.0]),
        ];
        validate("caption", &anchors).unwrap();
        let json = serde_json::to_value(&anchors).unwrap();
        assert_eq!(
            json,
            serde_json::json!([
                { "kind": "stage", "id": "api", "element": "api", "edge": "bottom", "offset": [0.0, 24.0] },
                { "kind": "editor", "id": "call", "target": "fetch" },
                { "kind": "point", "id": "corner", "at": [40.0, 60.0] }
            ])
        );
        assert_eq!(
            serde_json::from_value::<Vec<AnchorPlan>>(json).unwrap(),
            anchors
        );
        let sequence = vec![
            AnchorPlan::participant("db", "flow", "db", Edge::Bottom),
            AnchorPlan::row("query", "flow", "query", Edge::Right).with_offset([12.0, 0.0]),
        ];
        validate("callout", &sequence).unwrap();
        assert_eq!(
            serde_json::to_value(&sequence).unwrap(),
            serde_json::json!([
                { "kind": "participant", "id": "db", "sequence": "flow", "participant": "db", "edge": "bottom" },
                { "kind": "row", "id": "query", "sequence": "flow", "row": "query", "edge": "right", "offset": [12.0, 0.0] }
            ])
        );
        assert!(validate("caption", &[AnchorPlan::row("r", "flow", "", Edge::Top)]).is_err());
        let typo = serde_json::json!({ "kind": "stage", "id": "a", "element": "b", "egde": "top" });
        assert!(serde_json::from_value::<AnchorPlan>(typo).is_err());
    }

    #[test]
    fn invalid_anchors_are_rejected() {
        let twice = vec![AnchorPlan::point("a", [0.0; 2]); 2];
        assert!(validate("caption", &twice).is_err());
        let dotted = vec![AnchorPlan::point("a.b", [0.0; 2])];
        assert!(validate("caption", &dotted).is_err());
        let infinite = vec![AnchorPlan::point("a", [f32::NAN, 0.0])];
        assert!(validate("caption", &infinite).is_err());
        let far = vec![AnchorPlan::point("a", [0.0; 2]).with_offset([f32::INFINITY, 0.0])];
        assert!(validate("caption", &far).is_err());
        let unnamed = vec![AnchorPlan::stage("a", "", Edge::Top)];
        assert!(validate("caption", &unnamed).is_err());
        let many = (0..9)
            .map(|index| AnchorPlan::point(format!("p{index}"), [0.0; 2]))
            .collect::<Vec<_>>();
        assert!(validate("caption", &many).is_err());
        assert!(validate("caption", &[]).is_ok());
    }

    #[test]
    fn weights_are_strict_channel_names() {
        let anchors = [AnchorPlan::point("here", [0.0; 2])];
        assert!(accepts("anchor.here", &anchors));
        assert!(!accepts("anchor.there", &anchors));
        assert!(!accepts("here", &anchors));
    }

    #[test]
    fn edges_sit_on_outlines() {
        let card = Shape::Box(Box2::from_center_size(
            vec2(300.0, 200.0),
            vec2(200.0, 100.0),
        ));
        assert_eq!(Edge::Top.on(card), vec2(300.0, 150.0));
        assert_eq!(Edge::BottomLeft.on(card), vec2(200.0, 250.0));
        assert_eq!(Edge::Center.on(card), vec2(300.0, 200.0));
        let orb = Shape::Circle(Circle {
            center: Vec2::ZERO,
            radius: 10.0,
        });
        assert!(
            Edge::TopRight
                .on(orb)
                .abs_diff_eq(vec2(7.071_068, -7.071_068), 1e-4)
        );
    }

    #[test]
    fn blending_normalizes_weights_and_vanishes_without_them() {
        let a = vec2(100.0, 0.0);
        let b = vec2(300.0, 40.0);
        assert_eq!(blend([(a, 1.0), (b, 0.0)]), Some(a));
        assert_eq!(blend([(a, 0.5), (b, 0.5)]), Some(vec2(200.0, 20.0)));
        // An overshooting pair that sums past one still lands between them.
        assert_eq!(blend([(a, 0.75), (b, 0.75)]), Some(vec2(200.0, 20.0)));
        assert_eq!(blend([(a, 0.0), (b, 0.0)]), None);
        assert_eq!(blend([]), None);
    }

    #[test]
    fn moving_springs_every_weight_on_one_profile() {
        let mut scene = PlanBuilder::new("anchors", 4_000_000_000);
        let actor = scene.actor("label", "caption", ()).unwrap();
        let ids = vec!["a".to_owned(), "b".to_owned()];
        move_to(&mut scene, &actor, &ids, "b", 1_000_000_000).unwrap();
        move_to(&mut scene, &actor, &ids, "a", 1_300_000_000).unwrap();
        assert!(move_to(&mut scene, &actor, &ids, "c", 2_000_000_000).is_err());
        let plan = scene.finish().unwrap();
        let weight = |id: &str| {
            plan.continuous_channels
                .iter()
                .find(|channel| channel.property == weight_property(id))
                .unwrap()
        };
        assert!(matches!(weight("a").initial, ScalarPlan::Literal(1.0)));
        assert!(matches!(weight("b").initial, ScalarPlan::Literal(0.0)));
        assert_eq!(weight("a").events.len(), 2);
        assert_eq!(
            weight("a").events[0].spring_plan(),
            weight("b").events[0].spring_plan()
        );
    }
}
