//! Lenses: an optical loupe of thick glass that magnifies and refracts the
//! frame composed beneath it. The flat top magnifies a focus point evenly, so
//! the middle stays legible; a rounded rim bends sight inward by Snell's law
//! (strongest at the edge), splits colors slightly, catches a specular light,
//! and casts a soft contact shadow. A lens pins to the same anchors as a
//! callout (a point, a Stage element, an editor Semantic Target), so it can
//! follow code or a card and glide between them on weight springs. Where an
//! anchor is at a given time is layout only the renderer knows, so the renderer
//! resolves anchors at every sample; everything here is GPU-free.
use std::collections::HashSet;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    author::{ActorHandle, ContinuousHandle, PlanBuilder},
    callout::CalloutAnchorPlan,
    math::{
        Vec2, lerp,
        optics::{GLASS_IOR, refraction_offset, superellipse_slope},
        shapes::{Box2, RoundedBox},
        vec2,
    },
    plan::SpringPlan,
};

pub const LENS_RECIPE: &str = "lens";

/// Where a lens can sit: a callout's anchor vocabulary (a fixed point, a
/// positioned Stage element, or an editor Semantic Target, at an `edge`). A
/// lens has no label, so an anchor's `side` must be omitted.
pub type LensAnchorPlan = CalloutAnchorPlan;

/// How far a presence of zero shrinks the glass: it condenses as it appears.
const CONDENSED: f32 = 0.82;
/// Superellipse power of the rim: flatter on top than a quarter circle.
const BEVEL_POWER: f32 = 3.0;
/// Default rim width, as a fraction of the shorter half side.
const BEVEL_FRACTION: f32 = 0.28;
/// The most a vertical rim bends sight, per unit of depth.
pub const MAX_BEND: f32 = 1.118;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LensPlan {
    /// Where the lens can sit. The first is where it starts.
    pub anchors: Vec<LensAnchorPlan>,
    /// Width and height of the glass at full presence, in pixels.
    pub size: [f32; 2],
    /// Corner radius; omitted, the glass is fully round (a circle or capsule)
    /// at every size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<f32>,
    /// How much the flat top enlarges what it shows.
    #[serde(default = "default_magnification")]
    pub magnification: f32,
    /// Width of the rounded rim in pixels; by default 28% of the shorter half side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bevel: Option<f32>,
    /// How far below the glass the page lies, in rim widths: how strongly the
    /// rim bends what is beneath it.
    #[serde(default = "default_refraction")]
    pub refraction: f32,
    /// How much farther the rim bends blue than red (0.04 is 4%).
    #[serde(default = "default_dispersion")]
    pub dispersion: f32,
    /// Frosting: 0 is clear glass, 1 blurs everything it shows.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub frost: f32,
    /// Strength of the soft drop shadow and contact darkening.
    #[serde(default = "default_shadow")]
    pub shadow: f32,
}

fn default_magnification() -> f32 {
    1.6
}
fn default_refraction() -> f32 {
    0.6
}
fn default_dispersion() -> f32 {
    0.04
}
fn default_shadow() -> f32 {
    0.5
}
fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

impl LensPlan {
    /// A round lens `diameter` pixels across, pinned to `anchor`.
    pub fn circle(anchor: LensAnchorPlan, diameter: f32) -> Self {
        Self::sized(anchor, [diameter, diameter])
    }

    /// A pill-shaped lens (round ends), as for reading along a line of code.
    pub fn capsule(anchor: LensAnchorPlan, size: [f32; 2]) -> Self {
        Self::sized(anchor, size)
    }

    fn sized(anchor: LensAnchorPlan, size: [f32; 2]) -> Self {
        Self {
            anchors: vec![anchor],
            size,
            corner: None,
            magnification: default_magnification(),
            bevel: None,
            refraction: default_refraction(),
            dispersion: default_dispersion(),
            frost: 0.0,
            shadow: default_shadow(),
        }
    }

    pub fn anchor(mut self, anchor: LensAnchorPlan) -> Self {
        self.anchors.push(anchor);
        self
    }

    pub fn magnification(mut self, magnification: f32) -> Self {
        self.magnification = magnification;
        self
    }

    pub fn corner(mut self, corner: f32) -> Self {
        self.corner = Some(corner);
        self
    }

    pub fn bevel(mut self, bevel: f32) -> Self {
        self.bevel = Some(bevel);
        self
    }

    pub fn refraction(mut self, refraction: f32) -> Self {
        self.refraction = refraction;
        self
    }

    pub fn dispersion(mut self, dispersion: f32) -> Self {
        self.dispersion = dispersion;
        self
    }

    pub fn frost(mut self, frost: f32) -> Self {
        self.frost = frost;
        self
    }

    pub fn shadow(mut self, shadow: f32) -> Self {
        self.shadow = shadow;
        self
    }

    /// The weight channel that pins the lens to `anchor`.
    pub fn weight_property(anchor: &str) -> String {
        format!("anchor.{anchor}")
    }

    /// True when `property` names one of this lens's channels.
    pub fn accepts(&self, property: &str) -> bool {
        matches!(
            property,
            "presence"
                | "x"
                | "y"
                | "width"
                | "height"
                | "magnification"
                | "focus-x"
                | "focus-y"
                | "frost"
        ) || property
            .strip_prefix("anchor.")
            .is_some_and(|id| self.anchors.iter().any(|anchor| anchor.id() == id))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=8).contains(&self.anchors.len()),
            "a lens has one to eight anchors"
        );
        let mut ids = HashSet::new();
        for anchor in &self.anchors {
            let id = anchor.id();
            ensure!(
                !id.is_empty() && !id.chars().any(|c| c.is_whitespace() || c == '.'),
                "lens anchor ID '{id}' must be non-empty, without whitespace or dots"
            );
            ensure!(ids.insert(id), "lens anchor '{id}' is declared twice");
            let (side, valid) = match anchor {
                CalloutAnchorPlan::Point { at, side, .. } => {
                    (side, at.iter().all(|v| v.is_finite()))
                }
                CalloutAnchorPlan::Stage { element, side, .. } => (side, !element.is_empty()),
                CalloutAnchorPlan::Editor { target, side, .. } => (side, !target.is_empty()),
                CalloutAnchorPlan::Participant {
                    sequence,
                    participant,
                    side,
                    ..
                } => (side, !sequence.is_empty() && !participant.is_empty()),
                CalloutAnchorPlan::Row {
                    sequence,
                    row,
                    side,
                    ..
                } => (side, !sequence.is_empty() && !row.is_empty()),
            };
            ensure!(
                side.is_none(),
                "lens anchor '{id}' has a label side, but a lens has no label"
            );
            ensure!(
                valid,
                "lens anchor '{id}' needs a finite point, a stage element, a semantic target, or a sequence participant or row"
            );
        }
        let [width, height] = self.size;
        ensure!(
            (24.0..=1600.0).contains(&width) && (24.0..=1600.0).contains(&height),
            "lens size must be between 24 and 1600 pixels"
        );
        if let Some(corner) = self.corner {
            ensure!(
                corner.is_finite() && corner >= 0.0,
                "lens corner must be a finite, non-negative radius"
            );
        }
        ensure!(
            (0.5..=4.0).contains(&self.magnification),
            "lens magnification must be between 0.5 and 4"
        );
        if let Some(bevel) = self.bevel {
            ensure!(
                bevel > 0.0 && bevel <= width.min(height) * 0.5,
                "lens bevel must be positive and at most half the shorter side"
            );
        }
        ensure!(
            (0.0..=2.0).contains(&self.refraction),
            "lens refraction must be between 0 and 2"
        );
        ensure!(
            (0.0..=0.25).contains(&self.dispersion),
            "lens dispersion must be between 0 and 0.25"
        );
        ensure!(
            (0.0..=1.0).contains(&self.frost),
            "lens frost must be between 0 and 1"
        );
        ensure!(
            (0.0..=1.0).contains(&self.shadow),
            "lens shadow must be between 0 and 1"
        );
        Ok(())
    }

    /// The glass at one sample, centered at `center` (the blended anchor),
    /// from this lens's channels; `None` while it has no presence. A presence
    /// above 1 (a springy settle) strengthens it slightly past rest.
    pub fn glass(&self, center: Vec2, value: impl Fn(&str, f32) -> f32) -> Option<Glass> {
        let presence = value("presence", 1.0).clamp(0.0, 1.3);
        if presence <= 1e-3 {
            return None;
        }
        let size = vec2(value("width", self.size[0]), value("height", self.size[1])).max(Vec2::ONE);
        let condense = lerp(CONDENSED, 1.0, presence);
        let half = size * 0.5 * condense;
        let corner = self
            .corner
            .map_or(f32::INFINITY, |corner| corner * condense);
        let bevel = (self
            .bevel
            .unwrap_or(BEVEL_FRACTION * size.min_element() * 0.5)
            * condense)
            .min(half.min_element())
            .max(1.0);
        let center = center + vec2(value("x", 0.0), value("y", 0.0));
        let magnification = lerp(1.0, value("magnification", self.magnification), presence);
        Some(Glass {
            outline: RoundedBox::new(center, half, corner),
            bevel,
            depth: self.refraction * bevel * presence,
            focus: center + vec2(value("focus-x", 0.0), value("focus-y", 0.0)),
            magnification: magnification.clamp(0.25, 8.0),
            dispersion: self.dispersion,
            frost: value("frost", self.frost).clamp(0.0, 1.0),
            dim: 0.0,
            shadow: self.shadow,
            presence,
        })
    }
}

/// A lens at one sample, in canvas pixels: the outline, its optics, and how
/// present it is. Pure geometry, shared by the renderer and tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    /// The outline, already condensed by presence.
    pub outline: RoundedBox,
    /// Width of the rounded rim, from the vertical edge to the flat top.
    pub bevel: f32,
    /// How far below the glass the page lies, in pixels: the rim bends sight
    /// sideways by up to `MAX_BEND` times this.
    pub depth: f32,
    /// The canvas point the center of the glass shows.
    pub focus: Vec2,
    pub magnification: f32,
    /// How much farther the rim bends blue than red.
    pub dispersion: f32,
    pub frost: f32,
    /// How much the glass darkens what it shows, for legible text on top.
    pub dim: f32,
    pub shadow: f32,
    /// 0 (absent) to 1, slightly more during a springy settle.
    pub presence: f32,
}

impl Glass {
    /// Liquid glass as a panel material: a frosted pane with a thick rounded
    /// rim and no magnification, for a chip that refracts the scene behind
    /// its text. Like a lens it condenses as `presence` rises.
    pub fn pane(outline: RoundedBox, presence: f32) -> Option<Self> {
        let presence = presence.clamp(0.0, 1.3);
        if presence <= 1e-3 {
            return None;
        }
        let condense = lerp(CONDENSED, 1.0, presence);
        let half = outline.half * condense;
        let bevel = (half.min_element() * PANE_BEVEL).max(1.0);
        Some(Self {
            outline: RoundedBox::new(outline.center, half, outline.corner * condense),
            bevel,
            depth: PANE_REFRACTION * bevel * presence,
            focus: outline.center,
            magnification: 1.0,
            dispersion: default_dispersion(),
            frost: PANE_FROST,
            dim: PANE_DIM,
            shadow: PANE_SHADOW,
            presence,
        })
    }

    /// How far into the rim `point` is: 0 at the edge, 1 on the flat top.
    pub fn rim(&self, point: Vec2) -> f32 {
        (-self.outline.distance(point) / self.bevel).clamp(0.0, 1.0)
    }

    /// How far the rim bends sight inward at rim position `t`, in pixels:
    /// a ray falling straight down refracts at the bevel's surface and travels
    /// `depth` to the page.
    pub fn bend(&self, t: f32) -> f32 {
        self.depth * refraction_offset(superellipse_slope(t, BEVEL_POWER), GLASS_IOR)
    }

    /// Where on the page the glass at `point` looks: bent inward by the rim,
    /// then scaled about the focus. `spread` scales the bend per color (1 for
    /// green, `1 ± dispersion` for red and blue), so only the rim splits color.
    pub fn source(&self, point: Vec2, spread: f32) -> Vec2 {
        self.source_bent(point, self.bend(self.rim(point)), spread)
    }

    /// `source`, with the rim's bend at `point` already known (as from a table).
    pub fn source_bent(&self, point: Vec2, bend: f32, spread: f32) -> Vec2 {
        let inward = -self.outline.normal(point);
        self.focus + (point - self.outline.center + inward * bend * spread) / self.magnification
    }

    /// The canvas pixels the glass changes: the outline and its shadow.
    pub fn bounds(&self) -> Box2 {
        let reach = Vec2::splat(SHADOW_REACH);
        Box2 {
            min: self.outline.center - self.outline.half - reach,
            max: self.outline.center + self.outline.half + reach + vec2(0.0, self.drop()),
        }
    }

    /// Everything `source` may return for points inside the outline.
    pub fn source_bounds(&self) -> Box2 {
        let bend = MAX_BEND * self.depth * (1.0 + self.dispersion);
        let half = (self.outline.half + bend) / self.magnification;
        Box2 {
            min: self.focus - half,
            max: self.focus + half,
        }
    }

    /// How far below the glass its shadow falls.
    pub fn drop(&self) -> f32 {
        4.0 + 0.04 * self.outline.half.min_element()
    }
}

/// How far a lens's shadow reaches beyond its outline.
const SHADOW_REACH: f32 = 48.0;
/// A pane's rim, as a fraction of its shorter half side: thick, like a slab.
const PANE_BEVEL: f32 = 0.6;
/// A pane's rim bends gently: it is a sheet, not a loupe.
const PANE_REFRACTION: f32 = 0.45;
/// Frosted enough that text over busy pixels stays legible.
const PANE_FROST: f32 = 0.9;
/// And dimmed, as dark-mode glass is, so light text reads over light pixels.
const PANE_DIM: f32 = 0.35;
const PANE_SHADOW: f32 = 0.35;

/// Authoring handle for one lens. Channels are declared on first use.
#[derive(Clone, Debug)]
pub struct LensActor {
    actor: ActorHandle,
    anchors: Vec<String>,
    size: [f32; 2],
    magnification: f32,
}

impl LensActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &LensPlan,
    ) -> Result<Self> {
        plan.validate()?;
        let actor = scene.actor(id, LENS_RECIPE, plan)?;
        Ok(Self {
            actor,
            anchors: plan.anchors.iter().map(|a| a.id().to_owned()).collect(),
            size: plan.size,
            magnification: plan.magnification,
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

    /// The glass condenses: it grows a little as its rim thickens and its
    /// magnification rises, settling with the palette's morph overshoot. A
    /// lens with a `show` starts absent.
    pub fn show(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        let presence = self.channel(scene, "presence", 0.0);
        scene.spring_with(&presence, at_nanos, 1.0, SpringPlan::MORPH_OPEN);
    }

    /// The glass thins and shrinks away.
    pub fn hide(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        let presence = self.channel(scene, "presence", 1.0);
        scene.spring_with(&presence, at_nanos, 0.0, SpringPlan::EXIT);
    }

    /// Glide to `anchor` like a puck of glass: every weight springs on one
    /// critically damped profile, so they keep summing to one, an interrupted
    /// glide carries its velocity into the next, and the lens follows both
    /// targets while it moves between them.
    pub fn move_to(&mut self, scene: &mut PlanBuilder, anchor: &str, at_nanos: u64) -> Result<()> {
        ensure!(
            self.anchors.iter().any(|id| id == anchor),
            "lens '{}' has no anchor '{anchor}'",
            self.actor.id()
        );
        let spring = SpringPlan::MOVE.with_thresholds(1e-5, 1e-5);
        for (index, id) in self.anchors.clone().iter().enumerate() {
            let initial = if index == 0 { 1.0 } else { 0.0 };
            let weight = self.channel(scene, &LensPlan::weight_property(id), initial);
            scene.spring_with(&weight, at_nanos, f32::from(id == anchor), spring);
        }
        Ok(())
    }

    /// Slide to `offset` pixels from the anchor, as when scanning along a line.
    pub fn slide(&mut self, scene: &mut PlanBuilder, offset: [f32; 2], at_nanos: u64) {
        for (property, target) in [("x", offset[0]), ("y", offset[1])] {
            let channel = self.channel(scene, property, 0.0);
            scene.spring_with(&channel, at_nanos, target, SpringPlan::MOVE);
        }
    }

    /// Change how much the lens enlarges.
    pub fn magnify(&mut self, scene: &mut PlanBuilder, magnification: f32, at_nanos: u64) {
        let initial = self.magnification;
        let channel = self.channel(scene, "magnification", initial);
        scene.spring_with(&channel, at_nanos, magnification, SpringPlan::MOVE);
    }

    /// Reshape the glass, as from a round loupe into a capsule along a line.
    /// Without a fixed corner it stays fully round as it stretches.
    pub fn resize(&mut self, scene: &mut PlanBuilder, size: [f32; 2], at_nanos: u64) {
        for (property, initial, target) in [
            ("width", self.size[0], size[0]),
            ("height", self.size[1], size[1]),
        ] {
            let channel = self.channel(scene, property, initial);
            scene.spring_with(&channel, at_nanos, target, SpringPlan::MOVE);
        }
    }

    /// Show the point `offset` pixels from the lens center, so the glass can
    /// float beside what it magnifies.
    pub fn focus(&mut self, scene: &mut PlanBuilder, offset: [f32; 2], at_nanos: u64) {
        for (property, target) in [("focus-x", offset[0]), ("focus-y", offset[1])] {
            let channel = self.channel(scene, property, 0.0);
            scene.spring_with(&channel, at_nanos, target, SpringPlan::MOVE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{callout::CalloutSide, plan::ScalarPlan};

    fn point(id: &str, at: [f32; 2]) -> LensAnchorPlan {
        CalloutAnchorPlan::Point {
            id: id.into(),
            at,
            side: None,
        }
    }

    fn plan() -> LensPlan {
        LensPlan::circle(point("here", [400.0, 300.0]), 300.0).anchor(LensAnchorPlan::Editor {
            id: "call".into(),
            target: "fetch".into(),
            edge: CalloutSide::Center,
            side: None,
        })
    }

    fn rest(_: &str, default: f32) -> f32 {
        default
    }

    #[test]
    fn lenses_round_trip_with_compact_defaults() {
        let plan = plan();
        plan.validate().unwrap();
        let json = serde_json::to_value(&plan).unwrap();
        assert!(json.get("corner").is_none() && json.get("frost").is_none());
        assert_eq!(json["anchors"][1]["kind"], "editor");
        assert_eq!(serde_json::from_value::<LensPlan>(json).unwrap(), plan);
    }

    #[test]
    fn invalid_lenses_are_rejected() {
        let mut labeled = plan();
        labeled.anchors[0] = CalloutAnchorPlan::Point {
            id: "here".into(),
            at: [0.0, 0.0],
            side: Some(CalloutSide::Top),
        };
        assert!(labeled.validate().is_err());
        assert!(plan().magnification(9.0).validate().is_err());
        assert!(plan().bevel(400.0).validate().is_err());
        assert!(plan().dispersion(-0.1).validate().is_err());
        let mut twice = plan();
        twice.anchors.push(twice.anchors[0].clone());
        assert!(twice.validate().is_err());
        assert!(plan().accepts("anchor.call") && plan().accepts("focus-x"));
        assert!(!plan().accepts("anchor.nowhere") && !plan().accepts("radius"));
    }

    #[test]
    fn absent_glass_is_none_and_presence_condenses_it() {
        let plan = plan();
        let at = |presence: f32| {
            plan.glass(vec2(400.0, 300.0), |name, default| {
                if name == "presence" {
                    presence
                } else {
                    default
                }
            })
        };
        assert!(at(0.0).is_none());
        let full = at(1.0).unwrap();
        assert_eq!(full.outline.half, vec2(150.0, 150.0));
        assert_eq!(full.magnification, 1.6);
        let half = at(0.5).unwrap();
        assert!(half.outline.half.x < 150.0 && half.outline.half.x > 120.0);
        assert!((half.magnification - 1.3).abs() < 1e-6);
        assert!(half.depth < full.depth, "the rim thickens as it condenses");
        assert!(at(1.2).unwrap().magnification > 1.6, "a settle overshoots");
    }

    #[test]
    fn the_flat_top_magnifies_its_focus_evenly() {
        let glass = plan().glass(vec2(400.0, 300.0), rest).unwrap();
        assert_eq!(glass.source(vec2(400.0, 300.0), 1.0), vec2(400.0, 300.0));
        // Inside the rim the glass is flat: an even 1.6× about the focus.
        let source = glass.source(vec2(480.0, 260.0), 1.0);
        assert!(source.abs_diff_eq(vec2(450.0, 275.0), 1e-3), "{source}");
        assert_eq!(
            glass.source(vec2(480.0, 260.0), 1.2),
            source,
            "no color split"
        );
        let focused = plan()
            .glass(vec2(400.0, 300.0), |name, default| match name {
                "focus-x" => 100.0,
                _ => default,
            })
            .unwrap();
        assert_eq!(
            focused.source(vec2(400.0, 300.0), 1.0),
            vec2(500.0, 300.0),
            "the center shows the focus"
        );
    }

    #[test]
    fn the_rim_bends_inward_most_at_the_edge_and_splits_color() {
        let glass = plan().glass(vec2(400.0, 300.0), rest).unwrap();
        assert_eq!(glass.bend(1.0), 0.0);
        let edge = glass.bend(0.0);
        assert!((edge - MAX_BEND * glass.depth).abs() < 1e-3);
        let mut previous = edge;
        for t in [0.05, 0.2, 0.5, 0.9] {
            let bend = glass.bend(t);
            assert!(bend < previous, "bends less toward the top");
            previous = bend;
        }
        // Just inside the right edge, sight bends toward the center.
        let near_edge = vec2(549.0, 300.0);
        let straight = 400.0 + 149.0 / 1.6;
        let green = glass.source(near_edge, 1.0);
        assert!(green.x < straight - 5.0, "{green}");
        assert_eq!(green.y, 300.0);
        let red = glass.source(near_edge, 1.0 - glass.dispersion);
        let blue = glass.source(near_edge, 1.0 + glass.dispersion);
        assert!(blue.x < green.x && green.x < red.x, "blue bends most");
        let bounds = glass.source_bounds();
        for spread in [1.0 - glass.dispersion, 1.0 + glass.dispersion] {
            let source = glass.source(near_edge, spread);
            assert!(source.cmpge(bounds.min).all() && source.cmple(bounds.max).all());
        }
        let outer = glass.bounds();
        assert!(outer.min.x < 250.0 && outer.max.y > 450.0 + glass.drop());
    }

    #[test]
    fn panes_frost_without_magnifying_and_condense_in() {
        let outline = RoundedBox::new(vec2(500.0, 900.0), vec2(300.0, 30.0), 30.0);
        assert!(Glass::pane(outline, 0.0).is_none());
        let pane = Glass::pane(outline, 1.0).unwrap();
        assert_eq!(pane.magnification, 1.0);
        assert_eq!(pane.outline, outline);
        assert!(pane.frost > 0.5 && pane.bevel < 30.0);
        assert_eq!(pane.source(vec2(600.0, 900.0), 1.0), vec2(600.0, 900.0));
        let rim = pane.source(vec2(799.0, 900.0), 1.0);
        assert!(rim.x < 799.0, "the rim still bends inward: {rim}");
        let half = Glass::pane(outline, 0.5).unwrap();
        assert!(half.outline.half.x < 300.0);
        assert!(
            (half.outline.corner / half.outline.half.y - 1.0).abs() < 1e-5,
            "a pill stays a pill"
        );
    }

    #[test]
    fn capsules_stay_round_as_they_stretch() {
        let plan = LensPlan::capsule(point("line", [0.0, 0.0]), [200.0, 200.0]);
        let glass = plan
            .glass(Vec2::ZERO, |name, default| match name {
                "width" => 600.0,
                "height" => 100.0,
                _ => default,
            })
            .unwrap();
        assert_eq!(glass.outline.half, vec2(300.0, 50.0));
        assert_eq!(glass.outline.corner, 50.0);
        assert!(glass.bevel < 50.0);
    }

    #[test]
    fn gliding_springs_every_weight_on_one_profile() {
        let mut scene = PlanBuilder::new("lens-demo", 4_000_000_000);
        let mut lens = LensActor::declare(&mut scene, "loupe", &plan()).unwrap();
        lens.show(&mut scene, 200_000_000);
        lens.move_to(&mut scene, "call", 1_000_000_000).unwrap();
        assert!(lens.move_to(&mut scene, "nowhere", 2_000_000_000).is_err());
        lens.slide(&mut scene, [40.0, 0.0], 2_000_000_000);
        lens.magnify(&mut scene, 2.0, 2_500_000_000);
        lens.resize(&mut scene, [500.0, 110.0], 2_500_000_000);
        lens.focus(&mut scene, [0.0, -80.0], 3_000_000_000);
        lens.hide(&mut scene, 3_500_000_000);
        let plan = scene.finish().unwrap();
        let channel = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|channel| channel.property == property)
                .unwrap()
        };
        assert!(matches!(
            channel("presence").initial,
            ScalarPlan::Literal(0.0)
        ));
        assert!(matches!(
            channel("anchor.here").initial,
            ScalarPlan::Literal(1.0)
        ));
        assert!(matches!(
            channel("anchor.call").initial,
            ScalarPlan::Literal(0.0)
        ));
        assert_eq!(
            channel("anchor.here").events[0].spring_plan(),
            channel("anchor.call").events[0].spring_plan()
        );
        assert!(matches!(
            channel("magnification").initial,
            ScalarPlan::Literal(1.6)
        ));
        assert!(matches!(
            channel("width").initial,
            ScalarPlan::Literal(300.0)
        ));
    }
}
