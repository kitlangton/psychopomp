//! Stage: a 2.5D motion-graphics surface for explainers. Elements (cards, particle
//! orbs and forms, light beams, drawn paths, flat shapes, icons, travelling
//! packets, labels, rings) sit at world positions seen
//! through a perspective camera, and every change is an ordinary Continuous
//! Channel. Geometry that depends on time (orb spin, beam flow) is a pure function
//! of the sample time, so any frame renders identically in any order.
use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    author::{ActorHandle, ContinuousHandle, PlanBuilder, whole_millis},
    caption::{self, CaptionAlign, CaptionSpanPlan},
    effects::{dissolve, lightning, shield, spinner::Mark},
    face::Face,
    footage::{Clip, Fit, Mask, Playhead, STAGE_FOOTAGE_CHANNELS},
    math::{
        Vec2, Vec3,
        easing::{Ease, smootherstep},
        random::hash,
        shapes::{
            Box2, Circle, Polygon, Shape, box_points, cylinder_points, fibonacci_sphere,
            grid_points, helix_points, knot_points, match_points, torus_points,
        },
        vec3,
    },
    paint::Paint,
    plan::MediaPlan,
    plan::SpringPlan,
    tone::Tone,
};

pub const STAGE_RECIPE: &str = "stage";

/// Distance from the camera to the z = 0 plane. World x/y are canvas pixels at
/// z = 0, so an element with default camera channels lands exactly where authored.
pub const FOCAL: f32 = 1400.0;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StagePlan {
    #[serde(default)]
    pub post: StagePost,
    pub elements: Vec<StageElement>,
}

/// Look of the whole frame. The `post.bloom` and `post.vignette` channels
/// override `bloom` and `vignette`; `grain` and `backdrop` stay fixed.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StagePost {
    pub bloom: f32,
    pub grain: f32,
    pub vignette: f32,
    /// A soft neutral light behind the scene, 0 for none.
    pub backdrop: f32,
}

impl Default for StagePost {
    fn default() -> Self {
        Self {
            bloom: 0.55,
            grain: 0.035,
            vignette: 0.4,
            backdrop: 0.35,
        }
    }
}

impl StagePost {
    /// Flat editorial graphics: no bloom, grain, vignette, or backdrop light.
    /// Shapes, SVG paths, footage, and camera motion keep their ordinary channels.
    pub const FLAT: Self = Self {
        bloom: 0.0,
        grain: 0.0,
        vignette: 0.0,
        backdrop: 0.0,
    };

    /// The explainer films' look: restrained bloom on a quiet, nearly flat
    /// frame, so only what is alive glows.
    pub const RESTRAINED: Self = Self {
        bloom: 0.18,
        grain: 0.012,
        vignette: 0.22,
        backdrop: 0.12,
    };
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusText {
    pub text: String,
    #[serde(default, skip_serializing_if = "Tone::is_default")]
    pub tone: Tone,
}

impl StatusText {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum StageElement {
    /// A floating panel with a title and an optional status line. `status`
    /// entries cross-fade by the fractional `status` channel, or straight
    /// from entry `status-from` to `status` by the `swap` channel while
    /// `status-from` is set (-1 is unset).
    #[serde(rename_all = "camelCase")]
    Card {
        id: String,
        at: [f32; 3],
        size: [f32; 2],
        title: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        status: Vec<StatusText>,
        #[serde(default, skip_serializing_if = "Tone::is_default")]
        tone: Tone,
        /// What the card's status spinner resolves into (`mark` channel).
        #[serde(default, skip_serializing_if = "Mark::is_check")]
        mark: Mark,
    },
    /// A sphere of glowing points that spins, breathes, and can shatter.
    #[serde(rename_all = "camelCase")]
    Orb {
        id: String,
        at: [f32; 3],
        radius: f32,
        #[serde(default = "default_points")]
        points: u32,
        #[serde(default = "accent")]
        tone: Tone,
    },
    /// A curved light connection between two positioned elements.
    #[serde(rename_all = "camelCase")]
    Beam {
        id: String,
        from: String,
        to: String,
        /// Sideways bow of the curve, in pixels.
        #[serde(default)]
        bend: f32,
        #[serde(default, skip_serializing_if = "Tone::is_default")]
        tone: Tone,
    },
    /// A glowing message that travels along a beam, with an optional label.
    #[serde(rename_all = "camelCase")]
    Packet {
        id: String,
        beam: String,
        /// Travel from the beam's `to` back to its `from`.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reverse: bool,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
        #[serde(default, skip_serializing_if = "Tone::is_default")]
        tone: Tone,
    },
    /// Styled text in the scene, seen through the camera.
    #[serde(rename_all = "camelCase")]
    Label {
        id: String,
        at: [f32; 3],
        size: f32,
        #[serde(default = "center", skip_serializing_if = "is_center")]
        align: CaptionAlign,
        spans: Vec<CaptionSpanPlan>,
        #[serde(default, skip_serializing_if = "Face::is_mono")]
        face: Face,
    },
    /// A circle or arc: a timer when its sweep grows, a ripple when it expands.
    #[serde(rename_all = "camelCase")]
    Ring {
        id: String,
        at: [f32; 3],
        radius: f32,
        #[serde(default = "default_thickness")]
        thickness: f32,
        #[serde(default, skip_serializing_if = "Tone::is_default")]
        tone: Tone,
    },
    /// A particle form: the orb's glowing points arranged on one or more
    /// `shapes` (a box, a dot-matrix plane, a lattice, a cylinder, a torus, or
    /// a sphere). It turns in 3D, and the `morph` channel carries every point
    /// from one shape to the next with stable identity.
    #[serde(rename_all = "camelCase")]
    Form {
        id: String,
        at: [f32; 3],
        shapes: Vec<FormShape>,
        #[serde(default = "default_points")]
        points: u32,
        #[serde(default = "accent")]
        tone: Tone,
        /// How far the form leans back toward the camera, in radians (the
        /// orb's view). 0 faces the camera squarely.
        #[serde(default = "default_tilt", skip_serializing_if = "is_default_tilt")]
        tilt: f32,
    },
    /// A flat figure with no card chrome: a rectangle, circle, arc, or
    /// polygon, filled and stroked. The stroke draws on along its outline.
    #[serde(rename_all = "camelCase")]
    Shape {
        id: String,
        at: [f32; 3],
        shape: Figure,
        /// Corner radius of a rectangle or polygon.
        #[serde(default, skip_serializing_if = "is_zero")]
        corner: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Fill>,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        fill_opacity: f32,
        /// `null` for no stroke.
        #[serde(default = "muted_stroke", skip_serializing_if = "is_muted_stroke")]
        stroke: Option<Tone>,
        #[serde(default = "default_width", skip_serializing_if = "is_default_width")]
        width: f32,
        /// Dash and gap lengths in pixels.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dash: Option<[f32; 2]>,
        /// Arrowheads at an arc's ends.
        #[serde(default, skip_serializing_if = "Arrow::is_none")]
        arrow: Arrow,
    },
    /// A drawn connection through world points and positioned elements, with
    /// optional arrowheads. Packets ride it like a beam; element waypoints
    /// between its ends are stops where a riding packet lands and relays.
    #[serde(rename_all = "camelCase")]
    Path {
        id: String,
        through: Vec<Waypoint>,
        #[serde(default, skip_serializing_if = "Curve::is_straight")]
        curve: Curve,
        /// Radius that rounds a straight path's corners at point waypoints.
        #[serde(default, skip_serializing_if = "is_zero")]
        corner: f32,
        /// Sideways bow of each hop that leaves or enters an element.
        #[serde(default, skip_serializing_if = "is_zero")]
        bend: f32,
        #[serde(default, skip_serializing_if = "Tone::is_default")]
        tone: Tone,
        #[serde(default = "default_width", skip_serializing_if = "is_default_width")]
        width: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dash: Option<[f32; 2]>,
        #[serde(default, skip_serializing_if = "Arrow::is_none")]
        arrow: Arrow,
    },
    /// A monochrome SVG icon `size` world pixels square, tinted by its tone:
    /// a bundled Phosphor `icon` by name, or SVG `path` data in a `view`-unit
    /// square.
    #[serde(rename_all = "camelCase")]
    Icon {
        id: String,
        at: [f32; 3],
        size: f32,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        icon: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        path: String,
        #[serde(default = "default_view", skip_serializing_if = "is_default_view")]
        view: f32,
        /// An explicit sRGB pigment for artwork; otherwise the theme supplies `tone`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ink: Option<[u8; 3]>,
        #[serde(default, skip_serializing_if = "Tone::is_default")]
        tone: Tone,
    },
    /// Lightning between two positioned elements (or shields) or world
    /// points. Its `age` clock runs a stepped leader, `strikes` strobing
    /// return strokes that re-roll the path, contact sparks, and afterglow;
    /// `hum` keeps a writhing arc alive.
    #[serde(rename_all = "camelCase")]
    Bolt {
        id: String,
        from: BoltEnd,
        to: BoltEnd,
        #[serde(default = "default_strikes")]
        strikes: u32,
        /// Forks per stroke, about six at 1.
        #[serde(default = "default_branching")]
        branching: f32,
        #[serde(default = "request")]
        tone: Tone,
    },
    /// A forcefield bubble around a positioned element: faint hexagonal
    /// cells that ripple wherever a packet crosses it or a bolt strikes it.
    #[serde(rename_all = "camelCase")]
    Shield {
        id: String,
        around: String,
        radius: f32,
        #[serde(default = "accent")]
        tone: Tone,
    },
    /// Footage in the scene: an image, a video, or an image sequence on a
    /// camera-facing quad `size` world pixels, cut to its mask, optionally
    /// framed. Its `time` channel is the clip's playhead (see
    /// [`crate::footage`]); `tint` is the tone its `tint` channel moves toward.
    #[serde(rename_all = "camelCase")]
    Footage {
        id: String,
        at: [f32; 3],
        size: [f32; 2],
        clip: Clip,
        #[serde(default, skip_serializing_if = "Fit::is_cover")]
        fit: Fit,
        #[serde(default, skip_serializing_if = "Mask::is_square")]
        mask: Mask,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        framed: bool,
        #[serde(default = "accent", skip_serializing_if = "is_accent")]
        tint: Tone,
    },
    /// A real Rune surface (`Note`, `Diff`, or `Kit`) rendered onto a 2.5D
    /// perspective Stage quad `size` world pixels, with optional card framing
    /// and internal `"<id>::<key>"` beam/packet docking.
    #[serde(rename_all = "camelCase")]
    RuneCard {
        id: String,
        at: [f32; 3],
        size: [f32; 2],
        surface: Box<crate::rune_surface::RuneSurfacePlan>,
        #[serde(default = "default_rune_corner")]
        corner: f32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        framed: bool,
    },
    /// Full-canvas backdrop painted in linear HDR (solid sRGB/hex, linear
    /// gradient, or radial gradient) behind Stage elements.
    #[serde(rename_all = "camelCase")]
    Backdrop { id: String, paint: Paint },
}

fn default_rune_corner() -> f32 {
    20.0
}

fn is_accent(tone: &Tone) -> bool {
    *tone == Tone::Accent
}

/// One end of a bolt: a positioned element or shield by ID, or a world point.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BoltEnd {
    Element(String),
    Point([f32; 3]),
}

/// One shape a form's points can take, in world pixels about its center.
/// Boxes carry a share of their points on their edges, so they read as
/// solids; planes and lattices are exact grids, so their point count must
/// factor into one.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "shape", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FormShape {
    Sphere {
        radius: f32,
    },
    /// A box (a cube, a slab) of `size` width, height, and depth.
    Box {
        size: [f32; 3],
        /// Share of the points on its twelve edges, 0..1.
        #[serde(default = "default_edges")]
        edges: f32,
    },
    /// A flat dot matrix facing the camera (at tilt 0).
    Plane {
        size: [f32; 2],
    },
    /// A grid of points filling a box.
    Lattice {
        size: [f32; 3],
    },
    /// An open tube about the vertical axis, its rims drawn as rings.
    Cylinder {
        radius: f32,
        height: f32,
    },
    /// A ring lying flat: `radius` to the middle of its tube.
    Torus {
        radius: f32,
        tube: f32,
    },
    /// A double helix about the vertical axis: two opposing strands wound
    /// through `turns` revolutions over `height`, joined by cross-rungs.
    Helix {
        radius: f32,
        height: f32,
        #[serde(default = "default_turns")]
        turns: f32,
    },
    /// A `(p, q)` torus knot: a tube wound `p` times around a ring of `radius`
    /// and `q` times through its hole (a trefoil knot for `2, 3`).
    Knot {
        radius: f32,
        tube: f32,
        #[serde(default = "default_knot_p")]
        p: u32,
        #[serde(default = "default_knot_q")]
        q: u32,
    },
}

fn default_turns() -> f32 {
    2.5
}

fn default_knot_p() -> u32 {
    2
}

fn default_knot_q() -> u32 {
    3
}

impl FormShape {
    pub fn cube(size: f32) -> Self {
        Self::Box {
            size: [size; 3],
            edges: default_edges(),
        }
    }

    /// Distance from the center to the farthest point.
    pub fn radius(&self) -> f32 {
        match *self {
            Self::Sphere { radius } => radius,
            Self::Box { size, .. } | Self::Lattice { size } => Vec3::from(size).length() * 0.5,
            Self::Plane { size } => Vec2::from(size).length() * 0.5,
            Self::Cylinder { radius, height } | Self::Helix { radius, height, .. } => {
                radius.hypot(height * 0.5)
            }
            Self::Torus { radius, tube } | Self::Knot { radius, tube, .. } => radius + tube,
        }
    }

    /// The silhouette's width and height at rest, unturned.
    fn extent(&self) -> Vec2 {
        match *self {
            Self::Sphere { radius } => Vec2::splat(radius * 2.0),
            Self::Box { size, .. } | Self::Lattice { size } => Vec2::new(size[0], size[1]),
            Self::Plane { size } => Vec2::from(size),
            Self::Cylinder { radius, height } | Self::Helix { radius, height, .. } => {
                Vec2::new(radius * 2.0, height)
            }
            Self::Torus { radius, tube } => Vec2::new((radius + tube) * 2.0, tube * 2.0),
            Self::Knot { radius, tube, .. } => {
                Vec2::new((radius + tube) * 2.0, (radius + tube) * 2.0)
            }
        }
    }

    /// `count` points in this shape's natural, deterministic arrangement, or
    /// `None` for a grid that `count` cannot fill exactly.
    pub fn points(&self, count: u32) -> Option<Vec<Vec3>> {
        Some(match *self {
            Self::Sphere { radius } => fibonacci_sphere(count)
                .into_iter()
                .map(|p| p * radius)
                .collect(),
            Self::Box { size, edges } => box_points(count, Vec3::from(size) * 0.5, edges),
            Self::Plane { size } => grid_points(count, size)?,
            Self::Lattice { size } => grid_points(count, size)?,
            Self::Cylinder { radius, height } => cylinder_points(count, radius, height),
            Self::Torus { radius, tube } => torus_points(count, radius, tube),
            Self::Helix {
                radius,
                height,
                turns,
            } => helix_points(count, radius, height, turns),
            Self::Knot { radius, tube, p, q } => knot_points(count, radius, tube, p, q),
        })
    }

    fn validate(&self, id: &str, count: u32) -> Result<()> {
        let sized = |v: &[f32], min: f32| {
            v.iter()
                .all(|v| v.is_finite() && (min..=2000.0).contains(v))
        };
        let ok = match *self {
            Self::Sphere { radius } => sized(&[radius], 10.0),
            Self::Box { size, edges } => sized(&size, 1.0) && (0.0..=1.0).contains(&edges),
            Self::Plane { size } => sized(&size, 10.0),
            Self::Lattice { size } => sized(&size, 10.0),
            Self::Cylinder { radius, height } => sized(&[radius, height], 4.0),
            Self::Torus { radius, tube } => sized(&[radius, tube], 4.0) && tube < radius,
            Self::Helix {
                radius,
                height,
                turns,
            } => {
                sized(&[radius, height], 10.0) && turns.is_finite() && (0.5..=16.0).contains(&turns)
            }
            Self::Knot { radius, tube, p, q } => {
                sized(&[radius, tube], 4.0)
                    && tube < radius
                    && (1..=12).contains(&p)
                    && (1..=12).contains(&q)
            }
        };
        ensure!(ok, "form '{id}' has a shape whose size is out of range");
        ensure!(
            self.points(count).is_some(),
            "form '{id}' has {count} points, which do not fill a grid of at least 2 per side; choose a count with convenient factors (720 = 36 × 20)"
        );
        Ok(())
    }
}

/// Every shape of a form as `count` points, each shape reordered to pair its
/// points with the previous shape's, so morphing moves each point a short
/// way. Shapes must have validated.
pub fn form_points(shapes: &[FormShape], count: u32) -> Vec<Vec<Vec3>> {
    let mut matched: Vec<Vec<Vec3>> = Vec::with_capacity(shapes.len());
    for shape in shapes {
        let points = shape
            .points(count)
            .unwrap_or_else(|| fibonacci_sphere(count));
        let points = match matched.last() {
            Some(previous) => match_points(previous, points),
            None => points,
        };
        matched.push(points);
    }
    matched
}

/// How much of a morph's span the per-point stagger takes.
pub const MORPH_STAGGER: f32 = 0.35;

/// Where point `index` of a form sits at `morph`, a fractional index into
/// its matched `shapes` (see [`form_points`]): each point leaves one shape
/// for the next at a moment staggered by its `seed` (0..1), bows slightly
/// outward on the way, and arrives exactly on the next whole `morph`.
pub fn morph_point(shapes: &[Vec<Vec3>], index: usize, seed: f32, morph: f32) -> Vec3 {
    let last = shapes.len().saturating_sub(1);
    let morph = morph.clamp(0.0, last as f32);
    let from = (morph.floor() as usize).min(last.saturating_sub(1));
    let a = shapes[from][index];
    let t = morph - from as f32;
    if last == 0 || t <= 0.0 {
        return a;
    }
    let b = shapes[from + 1][index];
    let local = smootherstep(((t - MORPH_STAGGER * seed) / (1.0 - MORPH_STAGGER)).clamp(0.0, 1.0));
    if local <= 0.0 {
        return a;
    }
    if local >= 1.0 {
        return b;
    }
    let bow =
        (a + b).normalize_or_zero() * (a.distance(b) * 0.18 * (std::f32::consts::PI * local).sin());
    a.lerp(b, local) + bow
}

/// A flat figure's geometry, in world pixels about the shape's `at`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Figure {
    /// Width and height.
    Rect([f32; 2]),
    /// Radius.
    Circle(f32),
    /// Part of a circle, clockwise from twelve o'clock: `start` and `sweep`
    /// in turns.
    #[serde(rename_all = "camelCase")]
    Arc {
        radius: f32,
        #[serde(default)]
        start: f32,
        sweep: f32,
    },
    /// Corners relative to `at`, in order.
    Polygon(Vec<[f32; 2]>),
}

impl Figure {
    /// The outline at rest, about `center`, at `scale`.
    pub fn outline(&self, center: Vec2, scale: f32) -> Shape {
        match self {
            Self::Rect(size) => {
                Shape::Box(Box2::from_center_size(center, Vec2::from(*size) * scale))
            }
            Self::Circle(radius) | Self::Arc { radius, .. } => Shape::Circle(Circle {
                center,
                radius: radius * scale,
            }),
            Self::Polygon(points) => Shape::Polygon(Polygon::hull(
                center,
                points.iter().map(|p| center + Vec2::from(*p) * scale),
            )),
        }
    }
}

/// What fills a shape: a semantic tone, a theme material (`surface`,
/// `background`, `raised`, `well`), or an explicit `Paint` (solid sRGB/hex,
/// linear gradient, or radial gradient).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Fill {
    Tone(Tone),
    Material(Material),
    Paint(Paint),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Material {
    Surface,
    Background,
    Raised,
    Well,
}

/// Which ends of an open line carry an arrowhead.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Arrow {
    #[default]
    None,
    Start,
    End,
    Both,
}

impl Arrow {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
    pub fn at_start(self) -> bool {
        matches!(self, Self::Start | Self::Both)
    }
    pub fn at_end(self) -> bool {
        matches!(self, Self::End | Self::Both)
    }
}

/// A path's waypoint: a positioned element's id, or a world point.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Waypoint {
    Element(String),
    Point([f32; 3]),
}

impl Waypoint {
    pub fn element(&self) -> Option<&str> {
        match self {
            Self::Element(id) => Some(id),
            Self::Point(_) => None,
        }
    }
}

/// How a path runs between its waypoints. `smooth` passes through every point
/// on a Catmull-Rom curve; with `bezier` the points are a cubic chain (start,
/// two controls, end, two controls, end, ...). Both take points only.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Curve {
    #[default]
    Straight,
    Smooth,
    Bezier,
}

impl Curve {
    pub fn is_straight(&self) -> bool {
        *self == Self::Straight
    }
}

/// Bundled Phosphor icons (MIT, `assets/icons`), by name.
pub const ICONS: [&str; 43] = [
    "arrows-clockwise",
    "bell",
    "brain",
    "broadcast",
    "chart-line-up",
    "check-circle",
    "clock",
    "cloud",
    "code",
    "cpu",
    "cube",
    "cursor",
    "cursor-text",
    "database",
    "desktop",
    "device-mobile",
    "envelope",
    "file",
    "fingerprint",
    "folder",
    "gear",
    "git-branch",
    "globe",
    "hand-pointing",
    "hard-drives",
    "hourglass",
    "key",
    "lightning",
    "lock",
    "lock-open",
    "magnifying-glass",
    "package",
    "plug",
    "queue",
    "robot",
    "shield-check",
    "sparkle",
    "stack",
    "terminal",
    "user",
    "users",
    "warning",
    "x-circle",
];

fn default_points() -> u32 {
    720
}
fn default_thickness() -> f32 {
    3.0
}
fn default_tilt() -> f32 {
    0.42
}
fn is_default_tilt(tilt: &f32) -> bool {
    *tilt == default_tilt()
}
fn default_edges() -> f32 {
    0.5
}
fn default_width() -> f32 {
    1.4
}
fn is_default_width(width: &f32) -> bool {
    *width == default_width()
}
fn default_view() -> f32 {
    256.0
}
fn is_default_view(view: &f32) -> bool {
    *view == default_view()
}
fn is_zero(value: &f32) -> bool {
    *value == 0.0
}
fn one() -> f32 {
    1.0
}
fn is_one(value: &f32) -> bool {
    *value == 1.0
}
fn muted_stroke() -> Option<Tone> {
    Some(Tone::Muted)
}
fn is_muted_stroke(stroke: &Option<Tone>) -> bool {
    *stroke == muted_stroke()
}
fn accent() -> Tone {
    Tone::Accent
}
fn request() -> Tone {
    Tone::Request
}
fn default_strikes() -> u32 {
    3
}
fn default_branching() -> f32 {
    0.6
}
fn is_center(align: &CaptionAlign) -> bool {
    *align == CaptionAlign::Center
}
fn center() -> CaptionAlign {
    CaptionAlign::Center
}

/// Constructors with each kind's serialized defaults, and builder-style
/// options: `StageElement::card("client", at, size, "client").tone(Tone::Request)`.
/// An option on a kind that lacks it is a programming error and panics.
impl StageElement {
    /// A card with a title, no statuses, the plain tone, and a check mark.
    pub fn card(id: &str, at: [f32; 3], size: [f32; 2], title: &str) -> Self {
        Self::Card {
            id: id.into(),
            at,
            size,
            title: title.into(),
            status: Vec::new(),
            tone: Tone::default(),
            mark: Mark::default(),
        }
    }

    /// An orb of 720 points in the accent tone.
    pub fn orb(id: &str, at: [f32; 3], radius: f32) -> Self {
        Self::Orb {
            id: id.into(),
            at,
            radius,
            points: default_points(),
            tone: accent(),
        }
    }

    /// A straight, plain beam from one positioned element to another.
    pub fn beam(id: &str, from: &str, to: &str) -> Self {
        Self::Beam {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            bend: 0.0,
            tone: Tone::default(),
        }
    }

    /// An unlabeled, plain packet that travels `beam` from its `from` end.
    pub fn packet(id: &str, beam: &str) -> Self {
        Self::Packet {
            id: id.into(),
            beam: beam.into(),
            reverse: false,
            label: String::new(),
            tone: Tone::default(),
        }
    }

    /// Centered text of `(text, tone)` spans.
    pub fn label(id: &str, at: [f32; 3], size: f32, spans: &[(&str, Tone)]) -> Self {
        Self::Label {
            id: id.into(),
            at,
            size,
            align: center(),
            spans: spans
                .iter()
                .map(|&(text, tone)| CaptionSpanPlan::new(text, tone))
                .collect(),
            face: Face::Mono,
        }
    }

    /// A plain 3 px ring.
    pub fn ring(id: &str, at: [f32; 3], radius: f32) -> Self {
        Self::Ring {
            id: id.into(),
            at,
            radius,
            thickness: default_thickness(),
            tone: Tone::default(),
        }
    }

    /// A rectangle shape with a muted stroke and no fill.
    pub fn rect(id: &str, at: [f32; 3], size: [f32; 2]) -> Self {
        Self::Shape {
            id: id.into(),
            at,
            shape: Figure::Rect(size),
            corner: 0.0,
            fill: None,
            fill_opacity: 1.0,
            stroke: muted_stroke(),
            width: default_width(),
            dash: None,
            arrow: Arrow::None,
        }
    }

    /// A bundled Phosphor icon (`ICONS`), `size` world pixels square.
    pub fn icon(id: &str, at: [f32; 3], size: f32, icon: &str) -> Self {
        Self::Icon {
            id: id.into(),
            at,
            size,
            icon: icon.into(),
            path: String::new(),
            view: default_view(),
            ink: None,
            tone: Tone::default(),
        }
    }

    /// A real Rune surface (`Note`, `Diff`, or `Kit`) on a 2.5D Stage quad
    /// `size` world pixels at `at`.
    pub fn rune_card(
        id: &str,
        at: [f32; 3],
        size: [f32; 2],
        mut surface: crate::rune_surface::RuneSurfacePlan,
    ) -> Self {
        surface.size = size;
        surface.origin = [size[0] * 0.5, size[1] * 0.5];
        Self::RuneCard {
            id: id.into(),
            at,
            size,
            surface: Box::new(surface),
            corner: default_rune_corner(),
            framed: false,
        }
    }

    /// Name a keyed Rune element inside a `StageElement::RuneCard` as a Stage
    /// beam or bolt endpoint (`"card::rows/console"`).
    pub fn rune_pin(card_id: &str, rune_key: &str) -> String {
        format!("{card_id}::{rune_key}")
    }

    /// Put a tactile card mat and overhead specular rim behind footage or a `RuneCard`.
    pub fn framed(mut self) -> Self {
        match &mut self {
            Self::Footage { framed, .. } | Self::RuneCard { framed, .. } => *framed = true,
            other => panic!("stage element '{}' cannot be framed", other.id()),
        }
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        match &mut self {
            Self::Card { tone: own, .. }
            | Self::Orb { tone: own, .. }
            | Self::Beam { tone: own, .. }
            | Self::Packet { tone: own, .. }
            | Self::Ring { tone: own, .. }
            | Self::Form { tone: own, .. }
            | Self::Path { tone: own, .. }
            | Self::Icon { tone: own, .. }
            | Self::Bolt { tone: own, .. }
            | Self::Shield { tone: own, .. } => *own = tone,
            other => panic!("stage element '{}' has no tone", other.id()),
        }
        self
    }

    /// Corner radius of a rectangle, polygon, or `RuneCard`.
    pub fn corner(mut self, radius: f32) -> Self {
        match &mut self {
            Self::Shape { corner, .. }
            | Self::Path { corner, .. }
            | Self::RuneCard { corner, .. } => *corner = radius,
            other => panic!("stage element '{}' has no corner", other.id()),
        }
        self
    }

    /// Full-canvas linear/radial/solid backdrop painted behind all Stage elements.
    pub fn backdrop(id: &str, paint: Paint) -> Self {
        Self::Backdrop {
            id: id.into(),
            paint,
        }
    }

    /// Fill of a flat shape with an explicit `Paint` (solid hex, linear, or radial gradient).
    pub fn paint(mut self, paint: Paint) -> Self {
        let Self::Shape { fill, .. } = &mut self else {
            panic!("only shapes take paint(), not '{}'", self.id());
        };
        *fill = Some(Fill::Paint(paint));
        self
    }

    /// Fill of a flat shape (`Some(Fill::Material(Material::Surface))` or `Some(Fill::Tone(Tone::...))`).
    pub fn fill(mut self, value: Option<Fill>) -> Self {
        let Self::Shape { fill, .. } = &mut self else {
            panic!("only shapes have a fill, not '{}'", self.id());
        };
        *fill = value;
        self
    }

    /// Fill opacity of a flat shape (`0.0..=1.0`).
    pub fn fill_opacity(mut self, alpha: f32) -> Self {
        let Self::Shape { fill_opacity, .. } = &mut self else {
            panic!("only shapes have a fill_opacity, not '{}'", self.id());
        };
        *fill_opacity = alpha;
        self
    }

    /// Stroke tone of a flat shape (`None` for no stroke).
    pub fn stroke(mut self, value: Option<Tone>) -> Self {
        let Self::Shape { stroke, .. } = &mut self else {
            panic!("only shapes have a stroke, not '{}'", self.id());
        };
        *stroke = value;
        self
    }

    /// Stroke width of a flat shape or path, in world pixels.
    pub fn stroke_width(mut self, value: f32) -> Self {
        match &mut self {
            Self::Shape { width, .. } | Self::Path { width, .. } => *width = value,
            other => panic!("stage element '{}' has no stroke_width", other.id()),
        }
        self
    }

    /// Dash and gap lengths of a flat shape or path, in world pixels.
    pub fn dash(mut self, value: [f32; 2]) -> Self {
        match &mut self {
            Self::Shape { dash, .. } | Self::Path { dash, .. } => *dash = Some(value),
            other => panic!("stage element '{}' has no dash", other.id()),
        }
        self
    }

    /// A card's status lines, cross-faded by its `status` channel.
    pub fn statuses(mut self, statuses: &[(&str, Tone)]) -> Self {
        let Self::Card { status, .. } = &mut self else {
            panic!("only cards have statuses, not '{}'", self.id());
        };
        *status = statuses
            .iter()
            .map(|&(text, tone)| StatusText::new(text, tone))
            .collect();
        self
    }

    /// What a card's status spinner resolves into.
    pub fn mark(mut self, mark: Mark) -> Self {
        let Self::Card { mark: own, .. } = &mut self else {
            panic!("only cards have marks, not '{}'", self.id());
        };
        *own = mark;
        self
    }

    pub fn points(mut self, points: u32) -> Self {
        let Self::Orb { points: own, .. } = &mut self else {
            panic!("only orbs have points, not '{}'", self.id());
        };
        *own = points;
        self
    }

    /// A beam's sideways bow, in pixels.
    pub fn bend(mut self, bend: f32) -> Self {
        let Self::Beam { bend: own, .. } = &mut self else {
            panic!("only beams bend, not '{}'", self.id());
        };
        *own = bend;
        self
    }

    /// The packet travels from its beam's `to` end back to its `from`.
    pub fn reversed(mut self) -> Self {
        let Self::Packet { reverse, .. } = &mut self else {
            panic!("only packets reverse, not '{}'", self.id());
        };
        *reverse = true;
        self
    }

    /// The text a packet carries.
    pub fn labeled(mut self, text: &str) -> Self {
        let Self::Packet { label, .. } = &mut self else {
            panic!("only packets carry labels, not '{}'", self.id());
        };
        *label = text.into();
        self
    }

    pub fn align(mut self, align: CaptionAlign) -> Self {
        let Self::Label { align: own, .. } = &mut self else {
            panic!("only labels align, not '{}'", self.id());
        };
        *own = align;
        self
    }

    /// Set a label in `face` rather than CommitMono.
    pub fn face(mut self, face: Face) -> Self {
        let Self::Label { face: own, .. } = &mut self else {
            panic!("only labels have a face, not '{}'", self.id());
        };
        *own = face;
        self
    }

    /// A ring's stroke, in pixels.
    pub fn thickness(mut self, thickness: f32) -> Self {
        let Self::Ring { thickness: own, .. } = &mut self else {
            panic!("only rings have a thickness, not '{}'", self.id());
        };
        *own = thickness;
        self
    }
}

impl StageElement {
    pub fn id(&self) -> &str {
        match self {
            Self::Card { id, .. }
            | Self::Orb { id, .. }
            | Self::Beam { id, .. }
            | Self::Packet { id, .. }
            | Self::Label { id, .. }
            | Self::Ring { id, .. }
            | Self::Bolt { id, .. }
            | Self::Shield { id, .. }
            | Self::Form { id, .. }
            | Self::Shape { id, .. }
            | Self::Path { id, .. }
            | Self::Icon { id, .. }
            | Self::Footage { id, .. }
            | Self::RuneCard { id, .. }
            | Self::Backdrop { id, .. } => id,
        }
    }

    /// World position of elements that have one (beams, paths, and packets
    /// derive theirs).
    pub fn anchor(&self) -> Option<[f32; 3]> {
        match self {
            Self::Card { at, .. }
            | Self::Orb { at, .. }
            | Self::Label { at, .. }
            | Self::Ring { at, .. }
            | Self::Form { at, .. }
            | Self::Shape { at, .. }
            | Self::Icon { at, .. }
            | Self::Footage { at, .. }
            | Self::RuneCard { at, .. } => Some(*at),
            Self::Beam { .. }
            | Self::Packet { .. }
            | Self::Path { .. }
            | Self::Bolt { .. }
            | Self::Shield { .. }
            | Self::Backdrop { .. } => None,
        }
    }

    /// The outline beams attach to, around the element's projected `center`, at
    /// its total on-screen `scale`.
    pub fn outline(&self, center: Vec2, scale: f32) -> Shape {
        match self {
            Self::Card { size, .. } => {
                Shape::Box(Box2::from_center_size(center, Vec2::from(*size) * scale))
            }
            Self::Orb { radius, .. } => Shape::Circle(Circle {
                center,
                radius: radius * scale,
            }),
            Self::Ring { radius, .. } | Self::Shield { radius, .. } => Shape::Circle(Circle {
                center,
                radius: radius * scale,
            }),
            Self::Label { .. }
            | Self::Beam { .. }
            | Self::Packet { .. }
            | Self::Path { .. }
            | Self::Bolt { .. }
            | Self::Backdrop { .. } => Shape::Point(center),
            // The resting silhouette of the first shape; the renderer attaches
            // to the sampled, turned silhouette instead.
            Self::Form { shapes, .. } => match shapes.first() {
                Some(FormShape::Sphere { radius }) | Some(FormShape::Torus { radius, .. }) => {
                    Shape::Circle(Circle {
                        center,
                        radius: radius * scale,
                    })
                }
                Some(shape) => Shape::Box(Box2::from_center_size(center, shape.extent() * scale)),
                None => Shape::Point(center),
            },
            Self::Shape { shape, .. } => shape.outline(center, scale),
            Self::Icon { size, .. } => {
                Shape::Box(Box2::from_center_size(center, Vec2::splat(size * scale)))
            }
            Self::Footage {
                size,
                mask: Mask::Circle,
                ..
            } => Shape::Circle(Circle {
                center,
                radius: size[0].min(size[1]) * 0.5 * scale,
            }),
            Self::Footage { size, .. } | Self::RuneCard { size, .. } => {
                Shape::Box(Box2::from_center_size(center, Vec2::from(*size) * scale))
            }
        }
    }

    /// Channel properties this element reads (after its `<id>.` prefix).
    pub fn properties(&self) -> &'static [&'static str] {
        match self {
            Self::Card { .. } => &[
                "opacity",
                "x",
                "y",
                "z",
                "scale",
                "blur",
                "glow",
                "flash",
                "alarm",
                "dim",
                "status",
                "content",
                "cool",
                "damage",
                "glitch",
                "cut",
                "ghost",
                "spinner",
                "release",
                "mark",
                "status-from",
                "swap",
                "charge",
                "dissolve",
                "scan",
            ],
            Self::Orb { .. } => &[
                "opacity", "x", "y", "z", "scale", "blur", "rotation", "burst", "shatter", "pulse",
                "hurt", "spin", "charge",
            ],
            Self::Beam { .. } => &[
                "opacity", "sweep", "port", "draw", "break", "flow", "emphasis", "surge", "twang",
            ],
            Self::Packet { .. } => &["opacity", "age", "flight"],
            Self::Label { .. } => &["opacity", "x", "y", "z", "scale", "typed"],
            Self::Ring { .. } => &["opacity", "x", "y", "z", "scale", "sweep", "expand"],
            Self::Bolt { .. } => &["opacity", "age", "seed", "hum"],
            Self::Shield { .. } => &["opacity", "up", "scale"],
            Self::Form { .. } => &[
                "opacity", "x", "y", "z", "scale", "blur", "rotation", "spin", "pitch", "roll",
                "morph", "burst", "shatter", "pulse", "hurt", "solid",
            ],
            Self::Shape { .. } => &[
                "opacity", "x", "y", "z", "width", "height", "scale", "rotation", "blur", "draw",
                "fill", "emphasis", "flash",
            ],
            Self::Path { .. } => &["opacity", "draw", "trim", "flow", "emphasis", "surge"],
            Self::Icon { .. } => &["opacity", "x", "y", "z", "scale", "blur", "flash"],
            Self::Footage { .. } => &STAGE_FOOTAGE_CHANNELS,
            Self::RuneCard { .. } => &[
                "opacity",
                "x",
                "y",
                "z",
                "scale",
                "blur",
                "rotation",
                "focus-x",
                "focus-y",
                "focus-size",
                "saturation",
                "tint",
                "dim",
            ],
            Self::Backdrop { .. } => &["opacity"],
        }
    }

    /// True for what the camera can follow (`camera.track.<id>`): packets
    /// and positioned elements.
    pub fn followable(&self) -> bool {
        self.anchor().is_some() || matches!(self, Self::Packet { .. })
    }

    /// True for the particle bodies (orbs and forms): wires end beneath their
    /// occluding shell, and arrivals are absorbed out of sight.
    pub fn is_body(&self) -> bool {
        matches!(self, Self::Orb { .. } | Self::Form { .. })
    }

    /// The Stage's channel defaults: what each of [`Self::properties`] reads
    /// before anything writes it. `StageActor` declares a new channel at this
    /// value and the renderer falls back to it, so the two cannot disagree.
    /// A property added to `properties` needs its default here (a test checks).
    pub fn channel_defaults(&self) -> &'static [(&'static str, f32)] {
        match self {
            Self::Card { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("blur", 0.0),
                ("glow", 0.0),
                ("flash", 0.0),
                ("alarm", 0.0),
                ("dim", 0.0),
                ("status", 0.0),
                ("content", 1.0),
                ("cool", 0.0),
                ("damage", 0.0),
                ("glitch", 0.0),
                ("cut", 0.0),
                ("ghost", 0.0),
                ("spinner", -1.0),
                ("release", -1.0),
                ("mark", -1.0),
                ("status-from", -1.0),
                ("swap", 1.0),
                ("charge", 0.0),
                ("dissolve", -1.0),
                ("scan", 0.0),
            ],
            Self::Orb { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("blur", 0.0),
                ("rotation", 0.0),
                ("burst", -1.0),
                ("shatter", 0.0),
                ("pulse", 0.0),
                ("hurt", 0.0),
                ("spin", 1.0),
                ("charge", 0.0),
            ],
            Self::Beam { .. } => &[
                ("opacity", 1.0),
                ("sweep", 0.0),
                ("port", 0.0),
                ("draw", 1.0),
                ("break", 0.0),
                ("flow", 0.0),
                ("emphasis", 0.0),
                ("surge", 0.0),
                ("twang", 0.0),
            ],
            Self::Packet { .. } => &[("opacity", 1.0), ("age", -1.0), ("flight", 0.8)],
            Self::Label { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("typed", 1.0),
            ],
            Self::Ring { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("sweep", 1.0),
                ("expand", 0.0),
            ],
            Self::Bolt { .. } => &[("opacity", 1.0), ("age", -1.0), ("seed", 0.0), ("hum", 0.0)],
            Self::Shield { .. } => &[("opacity", 1.0), ("up", 1.0), ("scale", 1.0)],
            Self::Form { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("blur", 0.0),
                ("rotation", 0.0),
                ("spin", 1.0),
                ("pitch", 0.0),
                ("roll", 0.0),
                ("morph", 0.0),
                ("burst", -1.0),
                ("shatter", 0.0),
                ("pulse", 0.0),
                ("hurt", 0.0),
                ("solid", 1.0),
            ],
            Self::Shape { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("width", 0.0),
                ("height", 0.0),
                ("scale", 1.0),
                ("rotation", 0.0),
                ("blur", 0.0),
                ("draw", 1.0),
                ("fill", 1.0),
                ("emphasis", 0.0),
                ("flash", 0.0),
            ],
            Self::Path { .. } => &[
                ("opacity", 1.0),
                ("draw", 1.0),
                ("trim", 0.0),
                ("flow", 0.0),
                ("emphasis", 0.0),
                ("surge", 0.0),
            ],
            Self::Icon { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("blur", 0.0),
                ("flash", 0.0),
            ],
            // An unwritten `time` plays the clip naturally: the renderer reads
            // its placement, not this 0, until something writes the channel
            // (declare it through `StageActor::footage_playhead`).
            Self::Footage { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("blur", 0.0),
                ("rotation", 0.0),
                ("focus-x", 0.5),
                ("focus-y", 0.5),
                ("focus-size", 1.0),
                ("time", 0.0),
                ("saturation", 1.0),
                ("tint", 0.0),
                ("dim", 0.0),
            ],
            Self::RuneCard { .. } => &[
                ("opacity", 1.0),
                ("x", 0.0),
                ("y", 0.0),
                ("z", 0.0),
                ("scale", 1.0),
                ("blur", 0.0),
                ("rotation", 0.0),
                ("focus-x", 0.5),
                ("focus-y", 0.5),
                ("focus-size", 1.0),
                ("saturation", 1.0),
                ("tint", 0.0),
                ("dim", 0.0),
            ],
            Self::Backdrop { .. } => &[("opacity", 1.0)],
        }
    }

    /// What `property` reads before anything writes it, if this element reads it.
    pub fn channel_default(&self, property: &str) -> Option<f32> {
        self.channel_defaults()
            .iter()
            .find(|(name, _)| *name == property)
            .map(|&(_, value)| value)
    }
}

/// Channels that belong to the whole stage rather than an element.
/// `camera.shake` is the trauma jolts write; `camera.quake` is sustained
/// trauma a scene ramps itself (up to 2 for overdrive). They add.
/// `post.zoom` streaks the developed frame toward its center (a radial blur);
/// `post.flash` washes it toward white (0..1), for impacts that blind.
/// The camera's orientation, zoom, pivot, and handheld sway are described in
/// [`Camera`] and [`CameraRig`]; `camera.track.<id>` weights also count.
pub const STAGE_PROPERTIES: [&str; 23] = [
    "camera.x",
    "camera.y",
    "camera.z",
    "camera.focus",
    "camera.dof",
    "camera.shake",
    "camera.quake",
    "camera.kick-x",
    "camera.kick-y",
    "camera.punch",
    "camera.yaw",
    "camera.pitch",
    "camera.roll",
    "camera.zoom",
    "camera.pivot",
    "camera.handheld",
    "post.bloom",
    "post.chroma",
    "post.exposure",
    "post.vignette",
    "post.rewind",
    "post.zoom",
    "post.flash",
];

impl StagePost {
    /// The defaults of the [`STAGE_PROPERTIES`]: what each reads before
    /// anything writes it. The camera's come from [`CAMERA_CHANNELS`];
    /// `post.bloom` and `post.vignette` rest at this look.
    /// A stage property added above needs its default here (a test checks).
    pub fn channel_default(&self, property: &str) -> Option<f32> {
        if let Some(&(_, value)) = CAMERA_CHANNELS.iter().find(|(name, _)| *name == property) {
            return Some(value);
        }
        Some(match property {
            "post.bloom" => self.bloom,
            "post.vignette" => self.vignette,
            "post.exposure" => 1.0,
            "post.rewind" => -1.0,
            "post.chroma" | "post.zoom" | "post.flash" => 0.0,
            _ => return None,
        })
    }
}

impl StagePlan {
    pub fn element(&self, id: &str) -> Option<&StageElement> {
        self.elements.iter().find(|element| element.id() == id)
    }

    /// What a stage channel (`camera.z`) or an element's (`client.opacity`)
    /// reads before anything writes it, from the one table of Stage channel
    /// defaults ([`StagePost::channel_default`], [`StageElement::channel_defaults`]).
    /// `None` for a property the Stage does not read. A `camera.track.<id>`
    /// follow weight rests at 0.
    pub fn channel_default(&self, property: &str) -> Option<f32> {
        if property.starts_with(TRACK) {
            return self.accepts(property).then_some(0.0);
        }
        self.post.channel_default(property).or_else(|| {
            let (id, rest) = property.split_once('.')?;
            self.element(id)?.channel_default(rest)
        })
    }

    /// True when `property` names a stage channel or a property of an element.
    pub fn accepts(&self, property: &str) -> bool {
        if STAGE_PROPERTIES.contains(&property) {
            return true;
        }
        if let Some(id) = property.strip_prefix(TRACK) {
            return self.element(id).is_some_and(StageElement::followable);
        }
        property.split_once('.').is_some_and(|(id, rest)| {
            self.element(id)
                .is_some_and(|element| element.properties().contains(&rest))
        })
    }

    /// The legs a packet flies, in order, by the element each one arrives
    /// at (`None` for a path's free end). A beam is one leg; a path has one
    /// more leg per stop, an element waypoint between its ends.
    pub fn legs(&self, packet: &str) -> Vec<Option<&str>> {
        let Some(StageElement::Packet { beam, reverse, .. }) = self.element(packet) else {
            return Vec::new();
        };
        match self.element(beam) {
            Some(StageElement::Beam { from, to, .. }) => {
                vec![Some(if *reverse { from.as_str() } else { to.as_str() })]
            }
            Some(StageElement::Path { through, .. }) => {
                let mut waypoints = through.iter().collect::<Vec<_>>();
                if *reverse {
                    waypoints.reverse();
                }
                let last = waypoints.len() - 1;
                waypoints
                    .iter()
                    .enumerate()
                    .skip(1)
                    .filter(|(index, waypoint)| *index == last || waypoint.element().is_some())
                    .map(|(_, waypoint)| waypoint.element())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (0.0..=4.0).contains(&self.post.bloom)
                && (0.0..=0.3).contains(&self.post.grain)
                && (0.0..=1.0).contains(&self.post.vignette)
                && (0.0..=1.0).contains(&self.post.backdrop),
            "stage post values are out of range"
        );
        ensure!(
            !self.elements.is_empty() && self.elements.len() <= 128,
            "a stage has one to 128 elements"
        );
        let mut ids = HashSet::new();
        for element in &self.elements {
            let id = element.id();
            if id.is_empty()
                || id.chars().any(|c| c.is_whitespace() || c == '.')
                || matches!(id, "camera" | "post")
            {
                bail!(
                    "stage element ID '{id}' must be non-empty, without whitespace or dots, and not camera/post"
                );
            }
            ensure!(ids.insert(id), "stage element '{id}' is declared twice");
            if let Some(at) = element.anchor() {
                ensure!(
                    at.iter().all(|v| v.is_finite()) && at[2] > -FOCAL * 0.8,
                    "stage element '{id}' must be in front of the camera"
                );
            }
            match element {
                StageElement::Card {
                    size,
                    title,
                    status,
                    ..
                } => {
                    ensure!(
                        (40.0..=1400.0).contains(&size[0]) && (30.0..=800.0).contains(&size[1]),
                        "card '{id}' size is out of range"
                    );
                    line(id, title, 48)?;
                    let title_width = if title.trim().is_empty() {
                        0.0
                    } else {
                        title.chars().count() as f32 * 15.6 + 5.6
                    };
                    ensure!(
                        size[0] + 1e-3 >= title_width,
                        "card '{id}' title '{title}' needs width >= {:.0}, got {:.0}",
                        title_width.ceil(),
                        size[0]
                    );
                    ensure!(status.len() <= 6, "card '{id}' has more than six statuses");
                    for entry in status {
                        line(id, &entry.text, 48)?;
                        let status_width = entry.text.chars().count() as f32 * 10.8 + 6.0;
                        ensure!(
                            size[0] + 1e-3 >= status_width,
                            "card '{id}' status '{}' needs width >= {:.0}, got {:.0}",
                            entry.text,
                            status_width.ceil(),
                            size[0]
                        );
                    }
                }
                StageElement::Orb { radius, points, .. } => {
                    ensure!(
                        (10.0..=600.0).contains(radius) && (8..=4000).contains(points),
                        "orb '{id}' needs a radius of 10..600 and 8..4000 points"
                    );
                }
                StageElement::Beam { from, to, bend, .. } => {
                    for end in [from, to] {
                        let valid_end = self.element(end).and_then(StageElement::anchor).is_some()
                            || end.split_once("::").is_some_and(|(card_id, rune_key)| {
                                !rune_key.is_empty()
                                    && matches!(
                                        self.element(card_id),
                                        Some(StageElement::RuneCard { .. })
                                    )
                            });
                        ensure!(
                            valid_end,
                            "beam '{id}' must connect positioned elements or '<rune-card>::<key>' pins; '{end}' is not one"
                        );
                    }
                    ensure!(
                        from != to && bend.is_finite(),
                        "beam '{id}' needs two different ends"
                    );
                }
                StageElement::Packet { beam, label, .. } => {
                    ensure!(
                        matches!(
                            self.element(beam),
                            Some(StageElement::Beam { .. } | StageElement::Path { .. })
                        ),
                        "packet '{id}' must travel an existing beam or path"
                    );
                    line(id, label, 40)?;
                }
                StageElement::Form {
                    shapes,
                    points,
                    tilt,
                    ..
                } => {
                    ensure!(
                        (1..=8).contains(&shapes.len()) && (8..=4000).contains(points),
                        "form '{id}' needs one to eight shapes and 8..4000 points"
                    );
                    ensure!(tilt.is_finite(), "form '{id}' tilt must be finite");
                    for shape in shapes {
                        shape.validate(id, *points)?;
                    }
                }
                StageElement::Shape {
                    shape,
                    corner,
                    fill,
                    fill_opacity,
                    stroke,
                    width,
                    dash,
                    arrow,
                    ..
                } => {
                    let finite = |v: &[f32]| v.iter().all(|v| v.is_finite());
                    let ok = match shape {
                        Figure::Rect(size) => size.iter().all(|v| (1.0..=4000.0).contains(v)),
                        Figure::Circle(radius) => (1.0..=2000.0).contains(radius),
                        Figure::Arc {
                            radius,
                            start,
                            sweep,
                        } => {
                            (1.0..=2000.0).contains(radius)
                                && start.is_finite()
                                && *sweep > 0.0
                                && *sweep <= 1.0
                        }
                        Figure::Polygon(points) => {
                            (3..=64).contains(&points.len())
                                && points.iter().all(|p| {
                                    finite(p) && p[0].abs() <= 4000.0 && p[1].abs() <= 4000.0
                                })
                        }
                    };
                    ensure!(ok, "shape '{id}' geometry is out of range");
                    ensure!(
                        fill.is_some() || stroke.is_some(),
                        "shape '{id}' needs a fill or a stroke"
                    );
                    if let Some(Fill::Paint(p)) = fill {
                        p.validate().with_context(|| format!("shape '{id}' fill"))?;
                    }
                    ensure!(
                        (0.0..=400.0).contains(corner) && (0.0..=1.0).contains(fill_opacity),
                        "shape '{id}' corner or fill opacity is out of range"
                    );
                    stroke_style(id, *width, *dash, 256.0)?;
                    ensure!(
                        arrow.is_none() || matches!(shape, Figure::Arc { .. }),
                        "shape '{id}' is closed; only an arc can carry arrowheads"
                    );
                }
                StageElement::Path {
                    through,
                    curve,
                    corner,
                    bend,
                    width,
                    dash,
                    ..
                } => {
                    ensure!(
                        (2..=32).contains(&through.len()),
                        "path '{id}' needs two to 32 waypoints"
                    );
                    for (index, waypoint) in through.iter().enumerate() {
                        match waypoint {
                            Waypoint::Element(end) => {
                                ensure!(
                                    end != id
                                        && self
                                            .element(end)
                                            .and_then(StageElement::anchor)
                                            .is_some(),
                                    "path '{id}' runs through '{end}', which is not a positioned element"
                                );
                                ensure!(
                                    index == 0 || through[index - 1] != *waypoint,
                                    "path '{id}' visits '{end}' twice in a row"
                                );
                            }
                            Waypoint::Point(at) => ensure!(
                                at.iter().all(|v| v.is_finite()) && at[2] > -FOCAL * 0.8,
                                "path '{id}' has a point behind the camera"
                            ),
                        }
                    }
                    let points = through.iter().all(|w| w.element().is_none());
                    match curve {
                        Curve::Straight => {}
                        Curve::Smooth => ensure!(points, "a smooth path '{id}' takes points only"),
                        Curve::Bezier => ensure!(
                            points && through.len() % 3 == 1,
                            "a bezier path '{id}' takes 3n + 1 points: start, then two controls and an end per curve"
                        ),
                    }
                    ensure!(
                        (0.0..=400.0).contains(corner) && bend.is_finite(),
                        "path '{id}' corner or bend is out of range"
                    );
                    stroke_style(id, *width, *dash, 24.0)?;
                }
                StageElement::Icon {
                    size,
                    icon,
                    path,
                    view,
                    ..
                } => {
                    ensure!(
                        (8.0..=2048.0).contains(size) && *view > 0.0 && view.is_finite(),
                        "icon '{id}' size is out of range"
                    );
                    ensure!(
                        icon.is_empty() != path.is_empty(),
                        "icon '{id}' needs either a bundled `icon` or SVG `path` data"
                    );
                    ensure!(
                        icon.is_empty() || ICONS.contains(&icon.as_str()),
                        "icon '{id}' names '{icon}', which is not bundled; one of: {}",
                        ICONS.join(", ")
                    );
                    ensure!(
                        path.len() <= 20_000 && !path.contains(['"', '<', '>', '&']),
                        "icon '{id}' path data must be at most 20000 characters of SVG path commands"
                    );
                }
                StageElement::Footage {
                    size, clip, mask, ..
                } => {
                    ensure!(
                        size.iter().all(|v| (8.0..=4096.0).contains(v)),
                        "footage '{id}' size is out of range"
                    );
                    clip.validate()
                        .and_then(|()| mask.validate())
                        .with_context(|| format!("stage footage '{id}'"))?;
                }
                StageElement::RuneCard {
                    size,
                    surface,
                    corner,
                    ..
                } => {
                    ensure!(
                        size.iter().all(|v| (32.0..=2048.0).contains(v)),
                        "rune-card '{id}' size is out of range"
                    );
                    ensure!(
                        corner.is_finite() && (0.0..=256.0).contains(corner),
                        "rune-card '{id}' corner is out of range"
                    );
                    surface
                        .validate()
                        .with_context(|| format!("stage rune-card '{id}'"))?;
                }
                StageElement::Backdrop { paint, .. } => {
                    paint
                        .validate()
                        .with_context(|| format!("stage backdrop '{id}'"))?;
                }
                StageElement::Label { size, spans, .. } => {
                    ensure!(
                        (10.0..=320.0).contains(size),
                        "label '{id}' size is out of range"
                    );
                    ensure!(
                        spans.iter().any(|span| !span.text.is_empty()),
                        "label '{id}' needs text"
                    );
                    for span in spans {
                        line(id, &span.text, 120)?;
                    }
                }
                StageElement::Ring {
                    radius, thickness, ..
                } => {
                    ensure!(
                        (2.0..=900.0).contains(radius) && (0.5..=80.0).contains(thickness),
                        "ring '{id}' radius or thickness is out of range"
                    );
                }
                StageElement::Bolt {
                    from,
                    to,
                    strikes,
                    branching,
                    ..
                } => {
                    for end in [from, to] {
                        match end {
                            BoltEnd::Element(end) => ensure!(
                                end != id
                                    && self.element(end).is_some_and(|element| {
                                        element.anchor().is_some()
                                            || matches!(element, StageElement::Shield { .. })
                                    }),
                                "bolt '{id}' must strike positioned elements or shields; '{end}' is not one"
                            ),
                            BoltEnd::Point(at) => ensure!(
                                at.iter().all(|v| v.is_finite()) && at[2] > -FOCAL * 0.8,
                                "bolt '{id}' points must be finite and in front of the camera"
                            ),
                        }
                    }
                    ensure!(from != to, "bolt '{id}' needs two different ends");
                    ensure!(
                        (1..=lightning::MAX_STRIKES).contains(strikes)
                            && (0.0..=1.5).contains(branching),
                        "bolt '{id}' needs 1..=8 strikes and branching in 0..1.5"
                    );
                }
                StageElement::Shield { around, radius, .. } => {
                    ensure!(
                        self.element(around)
                            .and_then(StageElement::anchor)
                            .is_some(),
                        "shield '{id}' must surround a positioned element"
                    );
                    ensure!(
                        (10.0..=900.0).contains(radius),
                        "shield '{id}' radius is out of range"
                    );
                }
            }
        }
        Ok(())
    }
}

fn stroke_style(id: &str, width: f32, dash: Option<[f32; 2]>, max_width: f32) -> Result<()> {
    ensure!(
        (0.25..=max_width).contains(&width)
            && dash.is_none_or(|[on, off]| (0.5..=400.0).contains(&on) && (0.5..=400.0).contains(&off)),
        "stage element '{id}' stroke width or dash is out of range"
    );
    Ok(())
}

fn line(id: &str, text: &str, max: usize) -> Result<()> {
    ensure!(
        !text.contains(['\n', '\r']) && text.chars().count() <= max,
        "stage element '{id}' text must be one line of at most {max} characters"
    );
    Ok(())
}

mod camera;
pub use camera::{CAMERA_CHANNELS, Camera, CameraRig, Footprint, Move, TRACK, camera_default};

/// One point of an orb's shell and the seeds that shape its shatter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbPoint {
    pub unit: Vec3,
    pub seed: Vec3,
}

pub fn orb_points(count: u32) -> Vec<OrbPoint> {
    fibonacci_sphere(count)
        .into_iter()
        .zip(0..)
        .map(|(unit, i)| OrbPoint {
            unit,
            seed: vec3(hash(i, 3), hash(i, 7), hash(i, 11)),
        })
        .collect()
}

/// Where a shell point sits relative to the orb's center, before projection.
/// Points burst outward by different amounts, then fall; `shatter` runs 0..1.
pub fn shatter_offset(point: OrbPoint, radius: f32, shatter: f32) -> Vec3 {
    let seed = point.seed;
    let burst = 1.0 + shatter * (0.6 + 2.4 * seed.x);
    let fall = shatter * shatter * (160.0 + 460.0 * seed.y);
    let drift = shatter * (seed.z - 0.5) * 120.0;
    point.unit * (radius * burst) + vec3(drift, fall, 0.0)
}

/// A packet's life, derived from one dispatch clock (`age`, in seconds) and its
/// flight time, so every phase is exact at any sample time. Light gathers at the
/// start port, the packet flies on an acceleration-continuous quintic ease,
/// then it is absorbed as a small ring while its trail cools.
pub mod packet {
    use crate::math::easing::{smootherstep, smootherstep_inverse};

    pub const GATHER: f32 = 0.34;
    pub const LANDING: f32 = 0.72;
    /// How long the trail takes to cool behind the packet.
    pub const COOLING: f32 = 0.45;
    /// How long the glow left at the start port lasts.
    pub const EMBER: f32 = 2.2;
    /// How long light floods in from the end port.
    pub const FLOOD: f32 = 1.2;
    /// Every phase has finished by this age.
    pub const LIFETIME: f32 = 4.0;

    /// Progress (0..1) through the gather at `age`, if it is gathering.
    pub fn gather(age: f32) -> Option<f32> {
        window(age, 0.0, GATHER)
    }

    /// Progress (0..1) through the flight, before easing, if it is flying.
    pub fn flight(age: f32, flight: f32) -> Option<f32> {
        window(age, GATHER, flight)
    }

    /// Progress (0..1) through the landing, if it is landing.
    pub fn landing(age: f32, flight: f32) -> Option<f32> {
        window(age, GATHER + flight, LANDING)
    }

    /// Seconds since the packet arrived, if it has.
    pub fn since_arrival(age: f32, flight: f32) -> Option<f32> {
        (age >= GATHER + flight).then_some(age - GATHER - flight)
    }

    /// Where the packet is along its beam, as a fraction of the length.
    pub fn travel(age: f32, flight: f32) -> f32 {
        smootherstep(((age - GATHER) / flight).clamp(0.0, 1.0))
    }

    /// Seconds since the packet crossed the point at `fraction` of its beam, if
    /// it has reached it.
    pub fn since_crossing(age: f32, flight: f32, fraction: f32) -> Option<f32> {
        let crossed = GATHER + flight * smootherstep_inverse(fraction);
        (age >= crossed).then_some(age - crossed)
    }

    /// Heat of the trail at a point crossed `since` seconds ago.
    pub fn heat(since: f32) -> f32 {
        0.7 * (1.0 - (since / COOLING).clamp(0.0, 1.0)).powf(1.7)
    }

    /// How long a relaying packet rests at a stop after arriving before it
    /// gathers at the stop's far side and flies on: the reply never prepares
    /// before the request lands.
    pub const RELAY: f32 = 0.12;

    /// Seconds after dispatch that leg `leg` of `legs` starts gathering when
    /// the whole route flies in `flight` seconds, shared equally by its legs.
    /// Each leg is a whole packet life on its own clock, `age - leg_start`.
    pub fn leg_start(leg: usize, legs: usize, flight: f32) -> f32 {
        leg as f32 * (GATHER + flight / legs.max(1) as f32 + RELAY)
    }

    /// Seconds after dispatch that leg `leg` arrives.
    pub fn leg_arrival(leg: usize, legs: usize, flight: f32) -> f32 {
        leg_start(leg, legs, flight) + GATHER + flight / legs.max(1) as f32
    }

    /// Every phase of every leg has finished by this age.
    pub fn lifetime(legs: usize, flight: f32) -> f32 {
        leg_start(legs.max(1) - 1, legs, flight) + LIFETIME
    }

    fn window(age: f32, start: f32, length: f32) -> Option<f32> {
        (age >= start && age < start + length).then(|| (age - start) / length)
    }
}

/// Wire draw-on, after the blog diagrams: the port pops, then the wire draws
/// with a gentle start and stop. (A frame `sweep` before it is opt-in.)
pub const PORT_POP_SECONDS: f32 = 0.3;
pub const DRAW_CURVE: Ease = Ease::DRAW;

/// How long a receiver takes to react once a request has landed, before its
/// reply starts to gather.
pub const REACT_SECONDS: f32 = 0.08;

/// The earliest launch of a reply to a packet that arrives at `arrival`:
/// reaction follows contact, so even the reply's gather waits for the arrival
/// (340 ms gather plus an 80 ms beat). Pass it to [`StageActor::send`].
pub fn reply_after(arrival: u64) -> u64 {
    arrival + whole_millis(packet::GATHER) + whole_millis(REACT_SECONDS)
}

/// Authoring handle: declares each stage channel once, on first use.
#[derive(Clone, Debug)]
pub struct StageActor {
    actor: ActorHandle,
    /// The declared recipe, for helpers that follow a beam to its ends.
    plan: StagePlan,
}

impl StageActor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &StagePlan,
    ) -> Result<Self> {
        plan.validate()?;
        let actor = scene.actor(id, STAGE_RECIPE, plan)?;
        Ok(Self {
            actor,
            plan: plan.clone(),
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    pub fn plan(&self) -> &StagePlan {
        &self.plan
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

    /// The channel for `property`, declared on first use at the value the
    /// renderer reads when nothing writes it ([`StagePlan::channel_default`]).
    /// An unknown property starts at 0; the renderer's preflight rejects it.
    fn resting(&mut self, scene: &mut PlanBuilder, property: &str) -> ContinuousHandle {
        let initial = self.plan.channel_default(property).unwrap_or(0.0);
        self.channel(scene, property, initial)
    }

    /// Spring `property` to `target` at `at_nanos`. Channels not declared
    /// with [`Self::channel`] start at their resting value, the Stage channel
    /// default (opacity 1, burst -1, most others 0): declare another starting
    /// pose, such as an opacity of 0 to fade in, with `channel`.
    pub fn to(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        seconds: f32,
    ) {
        self.bounce(scene, property, at_nanos, target, seconds, 0.0);
    }

    /// Spring `property` to `target` with a named feel, such as
    /// [`SpringPlan::CAMERA`] or [`SpringPlan::PANEL`].
    pub fn spring(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        feel: SpringPlan,
    ) {
        let channel = self.resting(scene, property);
        scene.spring_with(&channel, at_nanos, target, feel);
    }

    /// Like `to`, with overshoot: `bounce` 0.2 reads as a lively landing.
    pub fn bounce(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        seconds: f32,
        bounce: f32,
    ) {
        let channel = self.resting(scene, property);
        scene.spring(&channel, at_nanos, target, seconds, bounce);
    }

    /// Ease `property` to `target` over `seconds` along `curve`.
    pub fn ease(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        seconds: f32,
        curve: Ease,
    ) {
        let channel = self.resting(scene, property);
        scene.ease(&channel, at_nanos, target, seconds, curve);
    }

    /// Glide `property` to `target` in exactly `seconds` on the minimum-jerk
    /// curve (smootherstep): velocity and acceleration meet the resting holds
    /// at both ends. Use it between resting compositions; use [`Self::to`]
    /// when an interruption must keep its momentum.
    pub fn glide(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        seconds: f32,
    ) {
        self.ease(
            scene,
            property,
            at_nanos,
            target,
            seconds,
            Ease::Smootherstep,
        );
    }

    /// The camera rig: framing, dollies, orbits, follows, and focus pulls,
    /// written as this stage's `camera.*` channels.
    pub fn camera(&self) -> CameraRig {
        CameraRig::new(self.actor.clone(), self.plan.clone())
    }

    /// Fade `element` in to `opacity` on a `seconds` spring. It starts hidden:
    /// the first write declares its opacity at 0, though the Stage rests visible.
    pub fn fade_in(
        &mut self,
        scene: &mut PlanBuilder,
        element: &str,
        at_nanos: u64,
        opacity: f32,
        seconds: f32,
    ) {
        let channel = self.channel(scene, &format!("{element}.opacity"), 0.0);
        scene.spring(&channel, at_nanos, opacity, seconds, 0.0);
    }

    /// Fade `element` out on a `seconds` spring.
    pub fn fade_out(
        &mut self,
        scene: &mut PlanBuilder,
        element: &str,
        at_nanos: u64,
        seconds: f32,
    ) {
        self.to(scene, &format!("{element}.opacity"), at_nanos, 0.0, seconds);
    }

    /// Jump `property` to `value` at `at_nanos`.
    pub fn set(&mut self, scene: &mut PlanBuilder, property: &str, at_nanos: u64, value: f32) {
        let channel = self.resting(scene, property);
        scene.set(&channel, at_nanos, value);
    }

    /// Start a clock of elapsed seconds at `at_nanos` that runs to the end of
    /// the scene; before it starts the channel reads -1 ("not yet"). Effect
    /// rigs sample their own poses from it.
    pub fn clock(&mut self, scene: &mut PlanBuilder, property: &str, at_nanos: u64) {
        // Whole milliseconds, as `ease` durations are, so the slope is exactly 1.
        let seconds = (scene.duration_nanos().saturating_sub(at_nanos) / 1_000_000) as f32 / 1000.0;
        self.clock_for(scene, property, at_nanos, seconds);
    }

    /// Like [`Self::clock`], but the clock stops at `seconds`, as for an effect
    /// with a fixed lifetime (a packet, a burst, a rewind).
    pub fn clock_for(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        seconds: f32,
    ) {
        let channel = self.channel(scene, property, -1.0);
        scene.set(&channel, at_nanos, 0.0);
        if seconds > 0.0 {
            scene.ease(&channel, at_nanos, seconds, seconds, Ease::Linear);
        }
    }

    /// Light `property` to `peak` at once, then let it decay to `rest`, fast
    /// and then with a long tail: how a flash, a hit, or a pulse behaves.
    pub fn hit(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        peak: f32,
        rest: f32,
    ) {
        let channel = self.channel(scene, property, rest);
        scene.set(&channel, at_nanos, peak);
        scene.ease(&channel, at_nanos, rest, 0.8, Ease::CubicOut);
    }

    /// Shove the `x`/`y` channel pair by `offset` in about two frames, then let
    /// it spring back past rest and settle: a hit with weight.
    pub fn kick(
        &mut self,
        scene: &mut PlanBuilder,
        [x, y]: [&str; 2],
        at_nanos: u64,
        offset: [f32; 2],
    ) {
        for (property, amount) in [(x, offset[0]), (y, offset[1])] {
            self.to(scene, property, at_nanos, amount, 0.04);
            self.bounce(scene, property, at_nanos + 40_000_000, 0.0, 0.6, 0.35);
        }
    }

    /// An impact jolts the camera: the frame is shoved along `direction` (the
    /// way the blow pushes the scene) and rebounds, a trauma rumble decays, and
    /// the frame punches in slightly. `strength` 1 is a full hit.
    pub fn jolt(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        direction: [f32; 2],
        strength: f32,
    ) {
        let length = direction[0].hypot(direction[1]).max(1e-6);
        // The camera moves against the push, so the scene moves with it.
        let shove = -16.0 * strength / length;
        self.kick(
            scene,
            ["camera.kick-x", "camera.kick-y"],
            at_nanos,
            [direction[0] * shove, direction[1] * shove],
        );
        self.hit(scene, "camera.shake", at_nanos, strength.min(1.0), 0.0);
        self.hit(scene, "camera.punch", at_nanos, 0.022 * strength, 0.0);
    }

    /// A rigid panel settles with a small vertical drift and restrained scale.
    /// Its content follows 65 ms later, so the body leads and the ink settles.
    /// Returns the time the panel is ready to connect.
    pub fn settle_in(&mut self, scene: &mut PlanBuilder, card: &str, at_nanos: u64) -> u64 {
        let scale = self.channel(scene, &format!("{card}.scale"), 1.035);
        scene.set(&scale, at_nanos, 1.035);
        scene.spring(&scale, at_nanos, 1.0, 0.6, 0.12);
        let y = self.channel(scene, &format!("{card}.y"), 16.0);
        scene.set(&y, at_nanos, 16.0);
        scene.spring(&y, at_nanos, 0.0, 0.55, 0.16);
        let content = self.channel(scene, &format!("{card}.content"), 0.0);
        scene.set(&content, at_nanos, 0.0);
        scene.spring(&content, at_nanos + 65_000_000, 1.0, 0.36, 0.0);
        for (property, from, to, seconds) in [("opacity", 0.0, 1.0, 0.18), ("blur", 3.0, 0.0, 0.3)]
        {
            let channel = self.channel(scene, &format!("{card}.{property}"), from);
            scene.set(&channel, at_nanos, from);
            scene.ease(&channel, at_nanos, to, seconds, Ease::Smootherstep);
        }
        at_nanos + 500_000_000
    }

    /// Type a label in at `chars_per_second`, one exact step per character.
    /// The label becomes visible when typing starts. Returns when it finishes.
    pub fn type_in(
        &mut self,
        scene: &mut PlanBuilder,
        label: &str,
        at_nanos: u64,
        chars_per_second: f32,
    ) -> u64 {
        let chars = match self.plan.element(label) {
            Some(StageElement::Label { spans, .. }) => {
                spans.iter().map(|span| span.text.chars().count()).sum()
            }
            _ => 0,
        }
        .max(1);
        let opacity = self.channel(scene, &format!("{label}.opacity"), 0.0);
        let typed = self.channel(scene, &format!("{label}.typed"), 0.0);
        scene.set(&opacity, at_nanos, 1.0);
        caption::type_steps(scene, &typed, at_nanos, chars, chars_per_second)
    }

    /// Send a packet so that it launches at `at_nanos` and flies for `seconds`.
    /// Light gathers at its port just before, and after arriving it lands as a
    /// small ring while its trail cools. Returns the arrival time.
    pub fn send(
        &mut self,
        scene: &mut PlanBuilder,
        packet: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> u64 {
        let dispatch = at_nanos.saturating_sub(whole_millis(packet::GATHER));
        let legs = self.plan.legs(packet).len().max(1);
        // The clock runs at real speed until every phase has finished. A
        // packet can be sent again once its previous life has ended: the new
        // dispatch restarts the same clock.
        self.clock_for(
            scene,
            &format!("{packet}.age"),
            dispatch,
            packet::lifetime(legs, seconds),
        );
        let flight = self.channel(scene, &format!("{packet}.flight"), seconds);
        scene.set(&flight, dispatch, seconds);
        leg_arrival(dispatch, legs - 1, legs, seconds)
    }

    /// Send `packet` along its whole route, landing on each element it
    /// reaches: a path's stop lights as the packet arrives, then gathers it at
    /// its far side and relays it on. `seconds` is the whole route's flight,
    /// shared equally by its legs. Returns each leg's arrival time.
    pub fn relay(
        &mut self,
        scene: &mut PlanBuilder,
        packet: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> Vec<u64> {
        let ends = self
            .plan
            .legs(packet)
            .into_iter()
            .map(|end| end.map(str::to_owned))
            .collect::<Vec<_>>();
        let dispatch = at_nanos.saturating_sub(whole_millis(packet::GATHER));
        self.send(scene, packet, at_nanos, seconds);
        let legs = ends.len().max(1);
        ends.iter()
            .enumerate()
            .map(|(leg, end)| {
                let arrival = leg_arrival(dispatch, leg, legs, seconds);
                if let Some(end) = end {
                    self.land(scene, end, arrival);
                }
                arrival
            })
            .collect()
    }

    /// Carry `form`'s points to its shape `index` over `seconds` on a
    /// minimum-jerk curve; each point's own move is staggered inside it.
    /// Returns when every point has arrived.
    pub fn morph(
        &mut self,
        scene: &mut PlanBuilder,
        form: &str,
        at_nanos: u64,
        index: usize,
        seconds: f32,
    ) -> u64 {
        self.ease(
            scene,
            &format!("{form}.morph"),
            at_nanos,
            index as f32,
            seconds,
            Ease::Smootherstep,
        );
        at_nanos + whole_millis(seconds)
    }

    /// Send `packet` so that it arrives at `arrival` after flying for
    /// `seconds`, as when a hit must land on a spoken word: it launches one
    /// flight earlier and gathers before that. Returns the arrival time.
    pub fn send_arriving(
        &mut self,
        scene: &mut PlanBuilder,
        packet: &str,
        arrival: u64,
        seconds: f32,
    ) -> u64 {
        self.send(
            scene,
            packet,
            arrival.saturating_sub(whole_millis(seconds)),
            seconds,
        )
    }

    /// Plug `beam` in so that the wire reaches its far end at `contact`,
    /// drawing for `seconds` after the port resolves. Returns the contact time.
    pub fn connect_contacting(
        &mut self,
        scene: &mut PlanBuilder,
        beam: &str,
        contact: u64,
        seconds: f32,
    ) -> u64 {
        let lead = whole_millis(PORT_POP_SECONDS) + whole_millis(seconds);
        self.connect(scene, beam, contact.saturating_sub(lead), seconds)
    }

    /// Plug `beam` in, starting at `at_nanos`: the port resolves softly and
    /// the wire draws over `seconds`, then stays still. Returns the contact
    /// time. Packets, impacts, twangs, and flow are separate authored actions.
    pub fn connect(
        &mut self,
        scene: &mut PlanBuilder,
        beam: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> u64 {
        // A path has no port channel: its sockets follow its own draw.
        let start = if matches!(self.plan.element(beam), Some(StageElement::Path { .. })) {
            at_nanos
        } else {
            let port = self.channel(scene, &format!("{beam}.port"), 0.0);
            scene.ease(&port, at_nanos, 1.0, PORT_POP_SECONDS, Ease::Smootherstep);
            at_nanos + whole_millis(PORT_POP_SECONDS)
        };
        let draw = self.channel(scene, &format!("{beam}.draw"), 0.0);
        scene.set(&draw, start, 0.0);
        scene.ease(&draw, start, 1.0, seconds, DRAW_CURVE);
        start + whole_millis(seconds)
    }

    /// `beam` is struck like a cable: it bows out over a few frames, then its
    /// momentum carries into an underdamped spring that vibrates back to rest.
    pub fn twang(&mut self, scene: &mut PlanBuilder, beam: &str, at_nanos: u64) {
        let twang = self.channel(scene, &format!("{beam}.twang"), 0.0);
        scene.spring(&twang, at_nanos, 1.0, 0.09, 0.0);
        scene.spring(&twang, at_nanos + 90_000_000, 0.0, 0.42, 0.28);
    }

    /// Something arrives at `element`: a card's ink flashes, an orb lights up.
    /// Nothing scales on a hit; the arrival's own light floods in from its port.
    pub fn land(&mut self, scene: &mut PlanBuilder, element: &str, at_nanos: u64) {
        match self.plan.element(element) {
            Some(StageElement::Card { .. }) => {
                self.hit(scene, &format!("{element}.flash"), at_nanos, 0.6, 0.0);
            }
            Some(StageElement::Orb { .. } | StageElement::Form { .. }) => {
                self.hit(scene, &format!("{element}.pulse"), at_nanos, 0.6, 0.0);
            }
            Some(StageElement::Shape { .. } | StageElement::Icon { .. }) => {
                self.hit(scene, &format!("{element}.flash"), at_nanos, 0.6, 0.0);
            }
            _ => {}
        }
    }
}

/// Effect beats: lightning, charge, dissolve, scans, and shields. Each writes
/// one clock or amount channel; the renderer derives every phase from it.
impl StageActor {
    /// Lightning strikes along `bolt`: its stepped leader sets out at
    /// `at_nanos` and the first return stroke connects [`lightning::LEADER`]
    /// later; further strokes strobe a few frames apart, each re-rolling the
    /// path, then the channel cools. Every zap rolls a fresh seed. Returns the
    /// contact time; pair it with `land` or `jolt` for the receiver.
    pub fn zap(&mut self, scene: &mut PlanBuilder, bolt: &str, at_nanos: u64) -> u64 {
        let strikes = match self.plan.element(bolt) {
            Some(StageElement::Bolt { strikes, .. }) => *strikes,
            _ => default_strikes(),
        };
        let seed = lightning::seed_for(at_nanos);
        let discharge = lightning::Discharge::new(strikes, seed);
        // A receiver's reaction (an orb's surface wave, a shield's ripple)
        // outlasts the bolt's own glow.
        let last = discharge.strike_time(discharge.strikes - 1);
        let lifetime = discharge.lifetime().max(last + shield::RIPPLE);
        let channel = self.channel(scene, &format!("{bolt}.seed"), 0.0);
        scene.set(&channel, at_nanos, seed as f32);
        // Whole milliseconds, so the clock's slope is exactly one.
        let seconds = (lifetime * 1000.0).ceil() / 1000.0;
        self.clock_for(scene, &format!("{bolt}.age"), at_nanos, seconds);
        at_nanos + whole_millis(lightning::LEADER)
    }

    /// Ease `element`'s (a card's or orb's) charge to `intensity` (0..1,
    /// overdriven to 1.5) over `seconds`: short arcs crawl its outline and
    /// strobe, lighting its rim. Zero discharges it; zero seconds is instant.
    pub fn charge(
        &mut self,
        scene: &mut PlanBuilder,
        element: &str,
        at_nanos: u64,
        intensity: f32,
        seconds: f32,
    ) {
        self.amount(
            scene,
            &format!("{element}.charge"),
            at_nanos,
            intensity,
            seconds,
        );
    }

    /// Keep an arc alive along `bolt` at `intensity` (eased over `seconds`):
    /// it re-strikes 24 times a second while its channel writhes. Zero stops it.
    pub fn hum(
        &mut self,
        scene: &mut PlanBuilder,
        bolt: &str,
        at_nanos: u64,
        intensity: f32,
        seconds: f32,
    ) {
        self.amount(scene, &format!("{bolt}.hum"), at_nanos, intensity, seconds);
    }

    fn amount(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        seconds: f32,
    ) {
        if seconds > 0.0 {
            self.ease(scene, property, at_nanos, target, seconds, Ease::GLIDE);
        } else {
            self.set(scene, property, at_nanos, target);
        }
    }

    /// Burn `card` away from `at_nanos`: a noisy front with a hot rim crosses
    /// it in [`dissolve::BURN`] seconds, shedding ash that drifts up and
    /// cools. Returns when the card is gone; the ash cools a little longer.
    pub fn dissolve(&mut self, scene: &mut PlanBuilder, card: &str, at_nanos: u64) -> u64 {
        self.clock_for(
            scene,
            &format!("{card}.dissolve"),
            at_nanos,
            dissolve::DURATION,
        );
        at_nanos + whole_millis(dissolve::BURN)
    }

    /// Form `card` out of ash: the dissolve played backwards over `seconds`
    /// (its full clock is [`dissolve::DURATION`]), so flakes fly home and the
    /// rim recedes. It rushes through the empty end of the clock and settles
    /// at a third of its average speed. The card is cold ash until `at_nanos`;
    /// returns when it is whole.
    pub fn materialize(
        &mut self,
        scene: &mut PlanBuilder,
        card: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> u64 {
        let channel = self.channel(scene, &format!("{card}.dissolve"), dissolve::DURATION);
        scene.set(&channel, at_nanos, dissolve::DURATION);
        scene.ease(&channel, at_nanos, 0.0, seconds, Ease::Decelerate(0.35));
        at_nanos + whole_millis(seconds)
    }

    /// Sweep a scan line down `card` over `seconds`: a bright line with a
    /// fading wake, lighting the rim where it crosses. Returns when it ends.
    pub fn scan(
        &mut self,
        scene: &mut PlanBuilder,
        card: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> u64 {
        let channel = self.channel(scene, &format!("{card}.scan"), 0.0);
        scene.set(&channel, at_nanos, 0.0);
        scene.ease(&channel, at_nanos, 1.0, seconds, Ease::GLIDE);
        at_nanos + whole_millis(seconds)
    }

    /// Raise `shield` over `seconds`: its cells switch on in seeded order.
    /// A shield is up by default; raising one first declares it down.
    pub fn raise(&mut self, scene: &mut PlanBuilder, shield: &str, at_nanos: u64, seconds: f32) {
        let channel = self.channel(scene, &format!("{shield}.up"), 0.0);
        scene.ease(&channel, at_nanos, 1.0, seconds, Ease::GLIDE);
    }

    /// Lower `shield` over `seconds`: its cells switch off in reverse order.
    pub fn lower(&mut self, scene: &mut PlanBuilder, shield: &str, at_nanos: u64, seconds: f32) {
        let channel = self.channel(scene, &format!("{shield}.up"), 1.0);
        scene.ease(&channel, at_nanos, 0.0, seconds, Ease::GLIDE);
    }
}

impl StageActor {
    /// The playhead of footage element `element`, whose clip `media`
    /// places: freeze, ramp, stutter, or scrub it on its `<id>.time` channel.
    pub fn footage_playhead(&self, element: &str, media: &MediaPlan) -> Result<Playhead> {
        let Some(StageElement::Footage { clip, .. }) = self.plan.element(element) else {
            bail!("the stage has no footage element '{element}'");
        };
        ensure!(
            media.id == clip.media,
            "footage '{element}' plays media '{}', not '{}'",
            clip.media,
            media.id
        );
        Ok(Playhead::new(
            self.actor.clone(),
            format!("{element}.time"),
            clip,
            media,
        ))
    }
}

/// When leg `leg` of `legs` arrives for a packet dispatched at `dispatch`,
/// in whole milliseconds like every authored clock.
fn leg_arrival(dispatch: u64, leg: usize, legs: usize, seconds: f32) -> u64 {
    dispatch
        + whole_millis(packet::leg_start(leg, legs, seconds))
        + whole_millis(packet::GATHER)
        + whole_millis(seconds / legs as f32)
}

/// How an orb gathers out of a blur while turning into place: it scales up on
/// a lively spring, sharpens, turns its last angular offset away, and fades in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbEntrance {
    pub scale: f32,
    pub scale_seconds: f32,
    pub blur: f32,
    pub blur_seconds: f32,
    /// The angular offset it turns away, in radians.
    pub rotation: f32,
    pub turn_seconds: f32,
    pub fade_seconds: f32,
}

impl OrbEntrance {
    /// The calibrated hero entrance (TECHNIQUES.md): scale 0.58 on a 0.85 s /
    /// 0.2-bounce spring, blur 11 over 0.7 s, −1.8 rad over 1.25 s cubic-out.
    pub const HERO: Self = Self {
        scale: 0.58,
        scale_seconds: 0.85,
        blur: 11.0,
        blur_seconds: 0.7,
        rotation: -1.8,
        turn_seconds: 1.25,
        fade_seconds: 0.6,
    };
}

/// A card's VHS-style glitch steps through layouts this far apart (about a
/// frame and a half at 60 fps).
pub const GLITCH_STEP_SECONDS: f32 = 0.027;

/// How long `post.rewind`'s tape interference runs.
pub const REWIND_SECONDS: f32 = 1.4;

/// Explainer beats composed from the primitives above. Each declares the
/// channels it needs at their starting poses and returns when it settles,
/// where a later beat would chain from it.
impl StageActor {
    /// The orb's entrance: see [`OrbEntrance`]. Call before any other write
    /// to the orb's scale, blur, rotation, or opacity.
    pub fn orb_in(&mut self, scene: &mut PlanBuilder, orb: &str, at: u64, entrance: OrbEntrance) {
        let property = |name: &str| format!("{orb}.{name}");
        self.channel(scene, &property("scale"), entrance.scale);
        self.channel(scene, &property("blur"), entrance.blur);
        self.channel(scene, &property("rotation"), entrance.rotation);
        self.bounce(
            scene,
            &property("scale"),
            at,
            1.0,
            entrance.scale_seconds,
            0.2,
        );
        self.to(scene, &property("blur"), at, 0.0, entrance.blur_seconds);
        let turn = entrance.turn_seconds;
        self.ease(scene, &property("rotation"), at, 0.0, turn, Ease::CubicOut);
        self.fade_in(scene, orb, at, 1.0, entrance.fade_seconds);
    }

    /// Glitch `card` through three layouts `GLITCH_STEP_SECONDS` apart, then
    /// still. Returns when it is still.
    pub fn glitch(&mut self, scene: &mut PlanBuilder, card: &str, at: u64, seeds: [f32; 3]) -> u64 {
        let step = whole_millis(GLITCH_STEP_SECONDS);
        let property = format!("{card}.glitch");
        for (index, seed) in (0..).zip(seeds.into_iter().chain([0.0])) {
            self.set(scene, &property, at + step * index, seed);
        }
        at + step * 3
    }

    /// Rewind the tape: `post.rewind`'s interference runs its course, with a
    /// `chroma` hit (0 for none). Returns when the interference has passed.
    pub fn rewind(&mut self, scene: &mut PlanBuilder, at: u64, chroma: f32) -> u64 {
        self.clock_for(scene, "post.rewind", at, REWIND_SECONDS);
        if chroma > 0.0 {
            self.hit(scene, "post.chroma", at, chroma, 0.0);
        }
        at + whole_millis(REWIND_SECONDS)
    }

    /// Play `orb`'s burst backwards: the clock eases back to 0 over `seconds`
    /// and the orb rests intact again; its hurt heals half a second in.
    /// Returns when it is whole.
    pub fn unburst(&mut self, scene: &mut PlanBuilder, orb: &str, at: u64, seconds: f32) -> u64 {
        let whole = at + whole_millis(seconds);
        self.ease(
            scene,
            &format!("{orb}.burst"),
            at,
            0.0,
            seconds,
            Ease::Smootherstep,
        );
        self.set(scene, &format!("{orb}.burst"), whole, -1.0);
        self.to(scene, &format!("{orb}.hurt"), at + 500_000_000, 0.0, 0.6);
        whole
    }

    /// Knock `card` away from `source` as the pressure wave of a burst that
    /// starts at `at` passes it ([`combustion::shock_arrival`]): a shove of
    /// `push` pixels, weakened in proportion beyond `falloff` pixels from the
    /// source when given. Returns when the front passes.
    ///
    /// [`combustion::shock_arrival`]: crate::effects::combustion::shock_arrival
    pub fn shock_kick(
        &mut self,
        scene: &mut PlanBuilder,
        source: &str,
        at: u64,
        card: &str,
        push: f32,
        falloff: Option<f32>,
    ) -> u64 {
        let anchor = |id: &str| {
            self.plan
                .element(id)
                .and_then(StageElement::anchor)
                .map(Vec3::from)
                .unwrap_or_else(|| panic!("stage element '{id}' has no position"))
        };
        let away = (anchor(card) - anchor(source)).truncate();
        let reach = away.length();
        let passes = at
            + crate::author::seconds(f64::from(crate::effects::combustion::shock_arrival(reach)));
        let mut shove = away.normalize() * push;
        if let Some(falloff) = falloff {
            shove *= (falloff / reach).min(1.0);
        }
        let [x, y] = [format!("{card}.x"), format!("{card}.y")];
        self.kick(scene, [&x, &y], passes, shove.into());
        passes
    }

    /// `card`'s status spinner, started at `started`, resolves into its mark
    /// at the motor's next top-right crossing after `done`. Returns when the
    /// mark has finished drawing, where its sound belongs.
    pub fn resolve_spinner(
        &mut self,
        scene: &mut PlanBuilder,
        card: &str,
        started: u64,
        done: u64,
    ) -> u64 {
        use crate::effects::spinner;
        let waited = done.saturating_sub(started) as f32 / 1e9;
        let handoff = started + crate::author::seconds(f64::from(spinner::handoff(waited)));
        self.clock(scene, &format!("{card}.mark"), handoff);
        handoff + crate::author::seconds(f64::from(spinner::DRAW))
    }

    /// The blog's tile glow: an inner ring rises to its opacity, the outer
    /// ring 60 ms later, both on `seconds` springs.
    pub fn halo(
        &mut self,
        scene: &mut PlanBuilder,
        [(inner, inner_opacity), (outer, outer_opacity)]: [(&str, f32); 2],
        at: u64,
        seconds: f32,
    ) {
        self.fade_in(scene, inner, at, inner_opacity, seconds);
        self.fade_in(scene, outer, at + 60_000_000, outer_opacity, seconds);
    }

    /// Release a [`Self::halo`] in reverse: the outer ring first, the inner
    /// 80 ms later.
    pub fn halo_out(
        &mut self,
        scene: &mut PlanBuilder,
        [inner, outer]: [&str; 2],
        at: u64,
        seconds: f32,
    ) {
        self.fade_out(scene, outer, at, seconds);
        self.fade_out(scene, inner, at + 80_000_000, seconds);
    }

    /// A ring as a timer: it appears and its arc sweeps from nothing to
    /// `sweep` (1 is a full circle) over `seconds`. Returns when it stops.
    pub fn ring_timer(
        &mut self,
        scene: &mut PlanBuilder,
        ring: &str,
        at: u64,
        seconds: f32,
        sweep: f32,
    ) -> u64 {
        self.fade_in(scene, ring, at, 1.0, 0.3);
        let channel = self.channel(scene, &format!("{ring}.sweep"), 0.0);
        scene.ease(&channel, at, sweep, seconds, Ease::Smootherstep);
        at + whole_millis(seconds)
    }

    /// Step `cards` back to `amount` of dimness on `seconds` springs, so the
    /// focal action reads alone; an `amount` of 0 brings them forward again.
    pub fn dim<S: AsRef<str>>(
        &mut self,
        scene: &mut PlanBuilder,
        cards: impl IntoIterator<Item = S>,
        at: u64,
        amount: f32,
        seconds: f32,
    ) {
        for card in cards {
            self.to(
                scene,
                &format!("{}.dim", card.as_ref()),
                at,
                amount,
                seconds,
            );
        }
    }

    /// Swap two labels that share a spot, one at a time: `from` fades out
    /// quickly at `at`, and `to` fades in `gap` later, once `from` is mostly
    /// gone, so two readable words never overlap. Returns when `to` starts.
    pub fn swap_labels(
        &mut self,
        scene: &mut PlanBuilder,
        [from, to]: [&str; 2],
        at: u64,
        gap: u64,
    ) -> u64 {
        self.fade_out(scene, from, at, 0.15);
        self.fade_in(scene, to, at + gap, 1.0, 0.25);
        at + gap
    }

    /// Cross-fade `card`'s status line straight from entry `from` to entry
    /// `to` over `seconds`, without passing the entries between them (as the
    /// fractional `status` channel would). Returns when the swap completes;
    /// afterwards `status` rests at `to` as usual.
    pub fn swap_status(
        &mut self,
        scene: &mut PlanBuilder,
        card: &str,
        at: u64,
        [from, to]: [usize; 2],
        seconds: f32,
    ) -> u64 {
        let done = at + whole_millis(seconds);
        let property = |name: &str| format!("{card}.{name}");
        self.set(scene, &property("status-from"), at, from as f32);
        self.set(scene, &property("status"), at, to as f32);
        self.set(scene, &property("swap"), at, 0.0);
        self.ease(
            scene,
            &property("swap"),
            at,
            1.0,
            seconds,
            Ease::Smootherstep,
        );
        self.set(scene, &property("status-from"), done, -1.0);
        done
    }

    /// Unplug `beam`, the reverse of [`Self::connect`]: the wire withdraws
    /// over `seconds`, and its port resolves away as it finishes. Returns when
    /// the port is gone.
    pub fn disconnect(
        &mut self,
        scene: &mut PlanBuilder,
        beam: &str,
        at: u64,
        seconds: f32,
    ) -> u64 {
        self.ease(scene, &format!("{beam}.draw"), at, 0.0, seconds, DRAW_CURVE);
        let port = at + whole_millis(seconds) - 100_000_000;
        let pop = PORT_POP_SECONDS;
        self.ease(
            scene,
            &format!("{beam}.port"),
            port,
            0.0,
            pop,
            Ease::Smootherstep,
        );
        port + whole_millis(pop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "orb", "id": "service", "at": [960, 460, 0], "radius": 150 },
                { "kind": "card", "id": "client", "at": [420, 300, -40], "size": [300, 120], "title": "client",
                  "status": [{ "text": "reconnecting" }, { "text": "disconnected", "tone": "error" }] },
                { "kind": "beam", "id": "link", "from": "client", "to": "service", "bend": 60 },
                { "kind": "packet", "id": "probe", "beam": "link", "label": "GET /api/info", "tone": "request" },
                { "kind": "label", "id": "caption", "at": [960, 700, 0], "size": 28,
                  "spans": [{ "text": "healthy", "tone": "success" }] },
                { "kind": "ring", "id": "timer", "at": [960, 460, 0], "radius": 190 }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn stage_plans_validate_and_round_trip() {
        let plan = plan();
        plan.validate().unwrap();
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(
            json["elements"][0]["tone"], "accent",
            "orbs default to the accent"
        );
        assert_eq!(serde_json::from_value::<StagePlan>(json).unwrap(), plan);
        assert!(
            plan.accepts("camera.z")
                && plan.accepts("service.shatter")
                && plan.accepts("probe.age")
        );
        assert!(
            !plan.accepts("probe.travel")
                && !plan.accepts("missing.opacity")
                && !plan.accepts("camera.spin")
                && !plan.accepts("camera.track.link")
                && !plan.accepts("camera.track.nowhere")
        );
        assert!(plan.accepts("camera.roll") && plan.accepts("camera.track.probe"));
    }

    #[test]
    fn constructors_carry_the_serialized_defaults() {
        let built = vec![
            StageElement::orb("service", [960.0, 460.0, 0.0], 150.0),
            StageElement::card("client", [420.0, 300.0, -40.0], [300.0, 120.0], "client")
                .statuses(&[("reconnecting", Tone::Plain), ("disconnected", Tone::Error)]),
            StageElement::beam("link", "client", "service").bend(60.0),
            StageElement::packet("probe", "link")
                .labeled("GET /api/info")
                .tone(Tone::Request),
            StageElement::label(
                "caption",
                [960.0, 700.0, 0.0],
                28.0,
                &[("healthy", Tone::Success)],
            ),
            StageElement::ring("timer", [960.0, 460.0, 0.0], 190.0),
        ];
        assert_eq!(built, plan().elements);
        let options = StageElement::card("c", [0.0; 3], [100.0, 50.0], "c")
            .mark(Mark::Cross)
            .tone(Tone::Accent);
        assert!(matches!(
            options,
            StageElement::Card {
                mark: Mark::Cross,
                tone: Tone::Accent,
                ..
            }
        ));
        assert!(matches!(
            StageElement::packet("p", "link").reversed(),
            StageElement::Packet { reverse: true, .. }
        ));
        let overflow_card = StagePlan {
            post: StagePost::default(),
            elements: vec![
                StageElement::card("c", [0.0, 0.0, 0.0], [200.0, 90.0], "c")
                    .statuses(&[("status text that is far wider than 200 pixels", Tone::Plain)]),
            ],
        };
        assert!(overflow_card.validate().is_err());
    }

    #[test]
    #[should_panic(expected = "only orbs have points")]
    fn an_option_on_the_wrong_kind_panics() {
        let _ = StageElement::ring("timer", [0.0; 3], 10.0).points(9);
    }

    /// A plan with one element of every kind, so each kind's table is checked.
    fn every_kind() -> StagePlan {
        let mut plan = plan();
        plan.elements.extend(
            serde_json::from_value::<Vec<StageElement>>(serde_json::json!([
                { "kind": "bolt", "id": "zap", "from": "client", "to": "service" },
                { "kind": "shield", "id": "guard", "around": "service", "radius": 200 },
                { "kind": "form", "id": "cube", "at": [1400, 460, 0],
                  "shapes": [{ "shape": "box", "size": [120, 120, 120] }] },
                { "kind": "shape", "id": "frame", "at": [960, 900, 0], "shape": { "rect": [200, 80] } },
                { "kind": "path", "id": "route", "through": ["client", [960, 900, 0]] },
                { "kind": "icon", "id": "glyph", "at": [200, 900, 0], "size": 48, "icon": "cloud" },
                { "kind": "footage", "id": "clip", "at": [1600, 900, -200], "size": [320, 180],
                  "clip": { "media": "clip" }, "mask": { "shape": "circle" } },
                { "kind": "rune-card", "id": "rune", "at": [1200, 500, 0], "size": [480, 320],
                  "surface": { "surface": { "recipe": "note", "markdown": "ok" } } }
            ]))
            .unwrap(),
        );
        plan.validate().unwrap();
        let kinds = [
            "card",
            "orb",
            "beam",
            "packet",
            "label",
            "ring",
            "bolt",
            "shield",
            "form",
            "shape",
            "path",
            "icon",
            "footage",
            "rune-card",
        ];
        for kind in kinds {
            assert!(
                plan.elements
                    .iter()
                    .any(|e| serde_json::to_value(e).unwrap()["kind"] == kind),
                "the test plan lacks a {kind}"
            );
        }
        plan
    }

    #[test]
    fn every_stage_property_has_exactly_one_default() {
        let plan = every_kind();
        for property in STAGE_PROPERTIES {
            assert!(
                plan.channel_default(property).is_some(),
                "stage property '{property}' has no default"
            );
        }
        for element in &plan.elements {
            let defaults = element.channel_defaults();
            let names = defaults.iter().map(|(name, _)| *name).collect::<Vec<_>>();
            let properties = element.properties();
            assert_eq!(
                names.iter().collect::<HashSet<_>>(),
                properties.iter().collect::<HashSet<_>>(),
                "'{}' defaults must name exactly its properties",
                element.id()
            );
            assert_eq!(names.len(), properties.len(), "a property is listed twice");
            for property in properties {
                let id = format!("{}.{property}", element.id());
                assert_eq!(plan.channel_default(&id), element.channel_default(property));
            }
        }
        assert_eq!(plan.channel_default("camera.spin"), None);
        assert_eq!(plan.channel_default("camera.track.probe"), Some(0.0));
        assert_eq!(plan.channel_default("camera.track.cube"), Some(0.0));
        assert_eq!(
            plan.channel_default("camera.track.link"),
            None,
            "beams cannot be followed"
        );
        assert_eq!(
            plan.channel_default("client.age"),
            None,
            "cards have no age"
        );
        assert_eq!(plan.channel_default("missing.opacity"), None);
    }

    #[test]
    fn defaults_are_resting_poses() {
        let plan = plan();
        for (property, rest) in [
            ("client.opacity", 1.0),
            ("client.content", 1.0),
            ("client.mark", -1.0),
            ("service.burst", -1.0),
            ("service.spin", 1.0),
            ("link.draw", 1.0),
            ("probe.age", -1.0),
            ("probe.flight", 0.8),
            ("caption.typed", 1.0),
            ("timer.sweep", 1.0),
            ("camera.z", 0.0),
            ("post.exposure", 1.0),
            ("post.rewind", -1.0),
            ("post.bloom", StagePost::default().bloom),
        ] {
            assert_eq!(plan.channel_default(property), Some(rest), "{property}");
        }
        let custom = StagePlan {
            post: StagePost {
                vignette: 0.22,
                ..StagePost::default()
            },
            ..plan
        };
        assert_eq!(custom.channel_default("post.vignette"), Some(0.22));
    }

    #[test]
    fn undeclared_channels_start_at_their_default() {
        let mut scene = PlanBuilder::new("stage-demo", 5_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        stage.to(&mut scene, "link.opacity", 1_000_000_000, 0.0, 0.5);
        stage.set(&mut scene, "client.mark", 1_000_000_000, -1.0);
        stage.ease(&mut scene, "camera.x", 0, 40.0, 1.0, Ease::Smootherstep);
        stage.channel(&mut scene, "service.opacity", 0.0);
        stage.to(&mut scene, "service.opacity", 0, 1.0, 0.5);
        let plan = scene.finish().unwrap();
        let initial = |property: &str| match plan
            .continuous_channels
            .iter()
            .find(|c| c.property == property)
            .unwrap()
            .initial
        {
            crate::plan::ScalarPlan::Literal(value) => value,
            _ => unreachable!(),
        };
        assert_eq!(
            initial("link.opacity"),
            1.0,
            "a wire fading out was visible"
        );
        assert_eq!(initial("client.mark"), -1.0);
        assert_eq!(initial("camera.x"), 0.0);
        assert_eq!(initial("service.opacity"), 0.0, "a declared pose wins");
    }

    #[test]
    fn invalid_references_are_rejected() {
        let mut dangling = plan();
        if let StageElement::Beam { to, .. } = &mut dangling.elements[2] {
            *to = "nowhere".into();
        }
        assert!(dangling.validate().is_err());
        let mut packet_on_card = plan();
        if let StageElement::Packet { beam, .. } = &mut packet_on_card.elements[3] {
            *beam = "client".into();
        }
        assert!(packet_on_card.validate().is_err());
        let mut reserved = plan();
        if let StageElement::Ring { id, .. } = &mut reserved.elements[5] {
            *id = "camera".into();
        }
        assert!(reserved.validate().is_err());
    }

    #[test]
    fn the_default_camera_is_pixel_exact_at_depth_zero() {
        use crate::math::vec2;
        let camera = Camera::new(vec2(1920.0, 1080.0));
        assert_eq!(
            camera.project(vec3(300.0, 200.0, 0.0)),
            Some((vec2(300.0, 200.0), 1.0))
        );
        let (far, scale) = camera.project(vec3(300.0, 200.0, 700.0)).unwrap();
        assert!(
            scale < 1.0 && far.x > 300.0,
            "farther points shrink toward the center"
        );
        let dolly = Camera::at(vec3(0.0, 0.0, 700.0), camera.size);
        assert!(dolly.project(vec3(300.0, 200.0, 0.0)).unwrap().1 > 1.0);
        assert!(camera.project(vec3(0.0, 0.0, -FOCAL)).is_none());
    }

    #[test]
    fn label_alignment_survives_plan_serialization() {
        for align in [
            CaptionAlign::Left,
            CaptionAlign::Center,
            CaptionAlign::Right,
        ] {
            let label = StageElement::Label {
                id: "name".into(),
                at: [960.0, 640.0, 0.0],
                size: 24.0,
                align,
                spans: vec![CaptionSpanPlan::new("service", Tone::Plain)],
                face: Default::default(),
            };
            let json = serde_json::to_value(&label).unwrap();
            assert_eq!(serde_json::from_value::<StageElement>(json).unwrap(), label);
        }
    }

    #[test]
    fn orbs_shatter_deterministically() {
        let points = orb_points(64);
        assert_eq!(points, orb_points(64));
        let point = points[5];
        assert_eq!(shatter_offset(point, 100.0, 0.0), point.unit * 100.0);
        let burst = shatter_offset(point, 100.0, 1.0);
        assert!(
            burst.y > point.unit.y * 100.0 + 100.0,
            "shattered points fall"
        );
    }

    #[test]
    fn beams_attach_to_the_facing_side_of_a_card_and_an_orb_outline() {
        use crate::math::{shapes::connect, vec2};
        let plan = plan();
        let card = plan
            .element("client")
            .unwrap()
            .outline(vec2(420.0, 300.0), 1.0);
        let orb = plan
            .element("service")
            .unwrap()
            .outline(vec2(960.0, 460.0), 1.0);
        let curve = connect(card, orb, 0.0);
        assert_eq!(curve.start, vec2(570.0, 300.0), "the card's right side");
        assert!((curve.end.distance(vec2(960.0, 460.0)) - 150.0).abs() < 1e-3);
    }

    #[test]
    fn packets_gather_fly_and_land_on_one_clock() {
        let mut scene = PlanBuilder::new("stage-demo", 5_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let arrival = stage.send(&mut scene, "probe", 1_000_000_000, 0.8);
        assert_eq!(
            arrival, 1_800_000_000,
            "it launches on time; the gather comes first"
        );
        let plan = scene.finish().unwrap();
        let age = plan
            .continuous_channels
            .iter()
            .find(|c| c.property == "probe.age")
            .unwrap();
        assert!(matches!(age.initial, crate::plan::ScalarPlan::Literal(value) if value == -1.0));
        assert_eq!(
            age.events[0].at_nanos(),
            660_000_000,
            "dispatched one gather early"
        );
        use packet::*;
        assert_eq!(gather(0.17), Some(0.5));
        assert_eq!((flight(0.34, 0.8), travel(0.34, 0.8)), (Some(0.0), 0.0));
        assert!(
            (travel(0.74, 0.8) - 0.5).abs() < 1e-6,
            "halfway in time is halfway along"
        );
        assert_eq!(landing(1.14, 0.8), Some(0.0));
        assert_eq!(since_crossing(0.74, 0.8, 0.9), None, "not there yet");
        let since = since_crossing(0.74, 0.8, 0.5).unwrap();
        assert!(
            since.abs() < 1e-5 && (heat(since) - 0.7).abs() < 1e-4,
            "hottest at the head"
        );
        assert_eq!(heat(COOLING), 0.0);
    }

    #[test]
    fn beats_can_be_timed_by_where_they_land() {
        let mut scene = PlanBuilder::new("stage-demo", 5_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let word = 2_000_000_000;
        assert_eq!(stage.send_arriving(&mut scene, "probe", word, 0.8), word);
        let contact = stage.connect_contacting(&mut scene, "link", word, 0.6);
        assert_eq!(contact, word, "port 0.3 s plus draw 0.6 s before the word");
        assert_eq!(
            reply_after(word),
            word + 420_000_000,
            "a 340 ms gather and an 80 ms reaction"
        );
        let plan = scene.finish().unwrap();
        let first = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|c| c.property == property)
                .unwrap()
                .events[0]
                .at_nanos()
        };
        assert_eq!(first("probe.age"), word - 800_000_000 - 340_000_000);
        assert_eq!(first("link.port"), word - 900_000_000);
    }

    /// Each event of `property` as (at, kind, target) for compact assertions.
    fn events(plan: &crate::plan::ScenePlan, property: &str) -> Vec<(u64, &'static str, f32)> {
        use crate::plan::{ScalarPlan, TrackEventPlan};
        let value = |scalar: &ScalarPlan| match scalar {
            ScalarPlan::Literal(value) => *value,
            _ => f32::NAN,
        };
        plan.continuous_channels
            .iter()
            .find(|c| c.property == property)
            .unwrap_or_else(|| panic!("missing {property}"))
            .events
            .iter()
            .map(|event| match event {
                TrackEventPlan::Set { at_nanos, value: v } => (*at_nanos, "set", value(v)),
                TrackEventPlan::Spring {
                    at_nanos, target, ..
                } => (*at_nanos, "spring", value(target)),
                TrackEventPlan::Ease {
                    at_nanos, target, ..
                } => (*at_nanos, "ease", value(target)),
            })
            .collect()
    }

    fn initial(plan: &crate::plan::ScenePlan, property: &str) -> f32 {
        match plan
            .continuous_channels
            .iter()
            .find(|c| c.property == property)
            .unwrap()
            .initial
        {
            crate::plan::ScalarPlan::Literal(value) => value,
            _ => unreachable!(),
        }
    }

    #[test]
    fn beats_declare_their_starting_poses_and_return_when_they_settle() {
        const S: u64 = 1_000_000_000;
        let mut scene = PlanBuilder::new("beats", 20 * S);
        let mut s = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        s.orb_in(&mut scene, "service", S, OrbEntrance::HERO);
        assert_eq!(
            s.glitch(&mut scene, "client", 2 * S, [7.0, 9.0, 8.0]),
            2 * S + 81_000_000
        );
        assert_eq!(s.rewind(&mut scene, 3 * S, 0.12), 3 * S + 1_400_000_000);
        assert_eq!(
            s.unburst(&mut scene, "service", 3 * S, 1.3),
            4 * S + 300_000_000
        );
        let drawn = s.resolve_spinner(&mut scene, "client", 5 * S, 6 * S);
        let handoff =
            5 * S + crate::author::seconds(f64::from(crate::effects::spinner::handoff(1.0)));
        assert!(
            handoff >= 6 * S,
            "the mark waits for the next crossing after done"
        );
        let draw = crate::author::seconds(f64::from(crate::effects::spinner::DRAW));
        assert_eq!(drawn, handoff + draw);
        s.halo(&mut scene, [("timer", 0.35), ("caption", 0.5)], 7 * S, 0.22);
        assert_eq!(
            s.ring_timer(&mut scene, "timer", 8 * S, 1.2, 0.67),
            9 * S + 200_000_000
        );
        assert_eq!(
            s.disconnect(&mut scene, "link", 10 * S, 0.35),
            10 * S + 550_000_000
        );
        let passes = s.shock_kick(&mut scene, "service", 11 * S, "client", 9.0, Some(480.0));
        assert!(
            passes > 11 * S + 120_000_000,
            "the front reaches the card after the collapse"
        );
        let plan = scene.finish().unwrap();

        assert_eq!(
            [
                initial(&plan, "service.scale"),
                initial(&plan, "service.blur"),
                initial(&plan, "service.rotation"),
                initial(&plan, "service.opacity")
            ],
            [0.58, 11.0, -1.8, 0.0]
        );
        let glitch = events(&plan, "client.glitch");
        assert_eq!(
            glitch
                .iter()
                .map(|e| (e.0 - 2 * S, e.2))
                .collect::<Vec<_>>(),
            [
                (0, 7.0),
                (27_000_000, 9.0),
                (54_000_000, 8.0),
                (81_000_000, 0.0)
            ]
        );
        assert_eq!(initial(&plan, "post.rewind"), -1.0);
        assert_eq!(events(&plan, "post.chroma")[0], (3 * S, "set", 0.12));
        assert_eq!(
            events(&plan, "service.burst"),
            [(3 * S, "ease", 0.0), (4 * S + 300_000_000, "set", -1.0)]
        );
        assert_eq!(events(&plan, "client.mark")[0], (handoff, "set", 0.0));
        assert_eq!(
            events(&plan, "caption.opacity"),
            [(7 * S + 60_000_000, "spring", 0.5)]
        );
        assert_eq!(initial(&plan, "timer.sweep"), 0.0);
        assert_eq!(events(&plan, "timer.sweep"), [(8 * S, "ease", 0.67)]);
        assert_eq!(
            events(&plan, "link.port"),
            [(10 * S + 250_000_000, "ease", 0.0)],
            "the port resolves away as the wire finishes"
        );
        let kick = events(&plan, "client.x");
        assert_eq!(kick[0].0, passes);
        assert!(
            kick[0].2 < 0.0,
            "the client sits left of the service and is pushed left"
        );
    }

    #[test]
    fn labels_swap_one_at_a_time_and_cards_step_back_together() {
        const S: u64 = 1_000_000_000;
        let mut scene = PlanBuilder::new("labels", 4 * S);
        let mut s = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let shown = s.swap_labels(&mut scene, ["caption", "timer"], S, 250_000_000);
        assert_eq!(shown, S + 250_000_000);
        s.dim(&mut scene, ["client"], 2 * S, 0.5, 0.8);
        let plan = scene.finish().unwrap();
        assert_eq!(
            initial(&plan, "caption.opacity"),
            1.0,
            "the outgoing label was showing"
        );
        assert_eq!(
            initial(&plan, "timer.opacity"),
            0.0,
            "the incoming label starts hidden"
        );
        assert_eq!(events(&plan, "timer.opacity"), [(shown, "spring", 1.0)]);
        assert_eq!(events(&plan, "client.dim"), [(2 * S, "spring", 0.5)]);
    }

    #[test]
    fn a_status_swap_skips_the_entries_between_and_then_rests() {
        const S: u64 = 1_000_000_000;
        let mut scene = PlanBuilder::new("swap", 4 * S);
        let mut s = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        assert_eq!(
            s.swap_status(&mut scene, "client", S, [1, 0], 0.4),
            S + 400_000_000
        );
        let plan = scene.finish().unwrap();
        assert_eq!(
            initial(&plan, "client.status-from"),
            -1.0,
            "old plans never swap"
        );
        assert_eq!(initial(&plan, "client.swap"), 1.0);
        assert_eq!(
            events(&plan, "client.status-from"),
            [(S, "set", 1.0), (S + 400_000_000, "set", -1.0)]
        );
        assert_eq!(events(&plan, "client.status"), [(S, "set", 0.0)]);
        assert_eq!(
            events(&plan, "client.swap"),
            [(S, "set", 0.0), (S, "ease", 1.0)]
        );
    }

    #[test]
    fn a_projected_rect_matches_the_projected_center_and_scale() {
        use crate::math::vec2;
        let camera = Camera::at(vec3(-110.0, 0.0, 60.0), vec2(1920.0, 1080.0));
        let at = vec3(430.0, 420.0, -60.0);
        let [x, y, w, h] = camera.project_rect(at, vec2(340.0, 124.0)).unwrap();
        let (center, scale) = camera.project(at).unwrap();
        assert!((x + w * 0.5 - center.x).abs() < 1e-3 && (y + h * 0.5 - center.y).abs() < 1e-3);
        assert!(
            (w - 340.0 * scale).abs() < 1e-3 && scale > 1.0,
            "the dolly magnifies it"
        );
        assert!(
            camera
                .project_rect(vec3(0.0, 0.0, -2000.0), vec2(1.0, 1.0))
                .is_none()
        );
    }

    #[test]
    fn connecting_draws_once_then_rests_without_implicit_impacts_or_flow() {
        let mut scene = PlanBuilder::new("stage-demo", 5_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let contact = stage.connect(&mut scene, "link", 1_000_000_000, 0.6);
        assert_eq!(contact, 1_900_000_000, "port 0.3 s, draw 0.6 s");
        let plan = scene.finish().unwrap();
        let channel = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|c| c.property == property)
                .unwrap_or_else(|| panic!("missing {property}"))
        };
        assert!(matches!(
            channel("link.draw").events[1],
            crate::plan::TrackEventPlan::Ease {
                at_nanos: 1_300_000_000,
                duration_nanos: 600_000_000,
                curve: DRAW_CURVE,
                ..
            }
        ));
        channel("link.port");
        for property in ["link.surge", "link.twang", "link.flow", "service.pulse"] {
            assert!(
                !plan
                    .continuous_channels
                    .iter()
                    .any(|c| c.property == property)
            );
        }
    }

    fn effects_plan() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "build", "at": [420, 540, 0], "size": [300, 110], "title": "build" },
                { "kind": "orb", "id": "deploy", "at": [1400, 540, 0], "radius": 120 },
                { "kind": "shield", "id": "guard", "around": "deploy", "radius": 190 },
                { "kind": "bolt", "id": "zap", "from": "build", "to": "deploy" },
                { "kind": "bolt", "id": "strike", "from": [960, -40, 0], "to": "guard", "strikes": 4 }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn bolts_and_shields_validate_and_round_trip() {
        let plan = effects_plan();
        plan.validate().unwrap();
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(
            json["elements"][3]["tone"], "request",
            "bolts default to the request tone"
        );
        assert_eq!(
            json["elements"][4]["from"],
            serde_json::json!([960.0, -40.0, 0.0])
        );
        assert_eq!(serde_json::from_value::<StagePlan>(json).unwrap(), plan);
        for property in [
            "zap.age",
            "zap.hum",
            "guard.up",
            "build.charge",
            "build.dissolve",
            "build.scan",
            "deploy.charge",
        ] {
            assert!(plan.accepts(property), "{property}");
        }
        assert!(!plan.accepts("guard.charge") && !plan.accepts("zap.x"));
        let mut dangling = effects_plan();
        if let StageElement::Bolt { to, .. } = &mut dangling.elements[3] {
            *to = BoltEnd::Element("zap".into());
        }
        assert!(dangling.validate().is_err(), "a bolt cannot strike a bolt");
        let mut storm = effects_plan();
        if let StageElement::Bolt { strikes, .. } = &mut storm.elements[3] {
            *strikes = 9;
        }
        assert!(storm.validate().is_err());
    }

    #[test]
    fn a_zap_starts_its_clock_with_a_fresh_seed_and_returns_contact() {
        let mut scene = PlanBuilder::new("zap", 5_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &effects_plan()).unwrap();
        let contact = stage.zap(&mut scene, "strike", 1_000_000_000);
        assert_eq!(contact, 1_075_000_000, "the leader comes first");
        stage.zap(&mut scene, "strike", 3_000_000_000);
        let gone = stage.dissolve(&mut scene, "build", 2_000_000_000);
        assert_eq!(gone, 3_100_000_000);
        let plan = scene.finish().unwrap();
        let channel = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|c| c.property == property)
                .unwrap()
        };
        let seeds = channel("strike.seed")
            .events
            .iter()
            .filter_map(|event| match event {
                crate::plan::TrackEventPlan::Set {
                    value: crate::plan::ScalarPlan::Literal(v),
                    ..
                } => Some(*v),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(seeds.len(), 2);
        assert!(seeds[0] != seeds[1] && seeds.iter().all(|s| s.fract() == 0.0));
        assert!(
            matches!(channel("strike.age").initial, crate::plan::ScalarPlan::Literal(v) if v == -1.0)
        );
        assert!(
            matches!(channel("build.dissolve").initial, crate::plan::ScalarPlan::Literal(v) if v == -1.0)
        );
    }

    fn diagram() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "form", "id": "store", "at": [1300, 540, 0], "points": 360,
                  "shapes": [{ "shape": "box", "size": [200, 200, 200] }, { "shape": "sphere", "radius": 130 }] },
                { "kind": "card", "id": "client", "at": [400, 540, 0], "size": [260, 100], "title": "client" },
                { "kind": "shape", "id": "gate", "at": [850, 540, 0], "shape": { "rect": [120, 160] },
                  "corner": 12, "fill": "surface" },
                { "kind": "shape", "id": "loop", "at": [850, 300, 0],
                  "shape": { "arc": { "radius": 50, "sweep": 0.8 } }, "arrow": "end", "stroke": "accent" },
                { "kind": "icon", "id": "lock", "at": [850, 540, -2], "size": 48, "icon": "lock" },
                { "kind": "path", "id": "route", "through": ["client", "gate", [1100, 700, 0], "store"],
                  "corner": 24, "arrow": "end" },
                { "kind": "packet", "id": "write", "beam": "route", "label": "PUT" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn new_elements_validate_round_trip_and_accept_their_channels() {
        let plan = diagram();
        plan.validate().unwrap();
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["elements"][2]["fill"], "surface");
        assert_eq!(json["elements"][3]["stroke"], "accent");
        assert!(
            json["elements"][2].get("stroke").is_none(),
            "muted is the default"
        );
        assert_eq!(
            json["elements"][5]["through"][2],
            serde_json::json!([1100.0, 700.0, 0.0])
        );
        assert_eq!(serde_json::from_value::<StagePlan>(json).unwrap(), plan);
        for property in [
            "store.morph",
            "store.pitch",
            "gate.draw",
            "gate.rotation",
            "route.trim",
            "lock.flash",
        ] {
            assert!(plan.accepts(property), "{property}");
        }
        assert!(!plan.accepts("route.x") && !plan.accepts("lock.morph"));
        let no_stroke: StageElement = serde_json::from_value(serde_json::json!({
            "kind": "shape", "id": "s", "at": [0, 0, 0], "shape": { "circle": 10 },
            "fill": "accent", "stroke": null
        }))
        .unwrap();
        assert!(matches!(
            no_stroke,
            StageElement::Shape { stroke: None, .. }
        ));
    }

    #[test]
    fn editorial_art_sizes_keep_bounded_validation_and_explicit_pigment() {
        let mut data = serde_json::json!({
            "post": StagePost::FLAT,
            "elements": [
                { "kind": "icon", "id": "art", "at": [960, 540, 0], "size": 2048,
                  "path": "M0 0H300V300H0Z", "view": 300, "ink": [0, 75, 147] },
                { "kind": "label", "id": "title", "at": [960, 540, 0], "size": 320,
                  "face": "sans-bold", "spans": [{ "text": "1.25" }] },
                { "kind": "shape", "id": "ring", "at": [960, 540, 0],
                  "shape": { "circle": 300 }, "width": 256 }
            ]
        });
        let valid = |data: &serde_json::Value| {
            serde_json::from_value::<StagePlan>(data.clone())
                .is_ok_and(|plan| plan.validate().is_ok())
        };
        assert!(valid(&data));
        let roundtrip =
            serde_json::to_value(serde_json::from_value::<StagePlan>(data.clone()).unwrap())
                .unwrap();
        assert_eq!(
            roundtrip["elements"][0]["ink"],
            serde_json::json!([0, 75, 147])
        );
        for (index, property, invalid) in [(0, "size", 2049), (1, "size", 321), (2, "width", 257)] {
            let old = data["elements"][index][property].clone();
            data["elements"][index][property] = invalid.into();
            assert!(!valid(&data));
            data["elements"][index][property] = old;
        }
        data["elements"][0]["ink"] = serde_json::json!([0, 75, 256]);
        assert!(!valid(&data), "pigment remains an sRGB byte triplet");
        assert!(
            stroke_style("connector", 25.0, None, 24.0).is_err(),
            "connector bounds do not widen with artwork"
        );
    }

    #[test]
    fn new_elements_reject_invalid_payloads() {
        let broken = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut json = serde_json::to_value(diagram()).unwrap();
            edit(&mut json["elements"]);
            serde_json::from_value::<StagePlan>(json).map_or(true, |plan| plan.validate().is_err())
        };
        assert!(!broken(&|_| {}), "the diagram itself is valid");
        assert!(broken(&|e| e[0]["shapes"] = serde_json::json!([])));
        assert!(
            broken(&|e| {
                e[0]["shapes"] = serde_json::json!([{ "shape": "plane", "size": [300, 200] }]);
                e[0]["points"] = serde_json::json!(359);
            }),
            "a prime count cannot fill a dot matrix"
        );
        assert!(broken(
            &|e| e[0]["shapes"][0]["edges"] = serde_json::json!(2.0)
        ));
        assert!(broken(
            &|e| e[0]["shapes"][0]["shape"] = serde_json::json!("cone")
        ));
        assert!(
            broken(&|e| {
                e[2]["fill"] = serde_json::Value::Null;
                e[2]["stroke"] = serde_json::Value::Null;
            }),
            "a shape needs ink"
        );
        assert!(
            broken(&|e| e[2]["arrow"] = serde_json::json!("end")),
            "closed shapes have no ends"
        );
        assert!(broken(
            &|e| e[3]["shape"]["arc"]["sweep"] = serde_json::json!(0)
        ));
        assert!(broken(&|e| e[4]["icon"] = serde_json::json!("lok")));
        assert!(
            broken(&|e| e[4]["path"] = serde_json::json!("M0 0 L10 10 Z")),
            "one source"
        );
        assert!(broken(&|e| e[5]["through"] = serde_json::json!(["client"])));
        assert!(
            broken(&|e| e[5]["through"][1] = serde_json::json!("route")),
            "not itself"
        );
        assert!(
            broken(&|e| e[5]["through"][1] = serde_json::json!("client")),
            "no repeats"
        );
        assert!(
            broken(&|e| e[5]["curve"] = serde_json::json!("smooth")),
            "smooth takes points"
        );
        assert!(
            broken(&|e| {
                e[5]["through"] = serde_json::json!([[0, 0, 0], [1, 1, 0], [2, 2, 0]]);
                e[5]["curve"] = serde_json::json!("bezier");
            }),
            "a bezier chain has 3n + 1 points"
        );
        assert!(broken(&|e| e[5]["width"] = serde_json::json!(0)));
        assert!(broken(&|e| e[6]["beam"] = serde_json::json!("gate")));
    }

    #[test]
    fn a_path_relays_its_packet_through_each_stop() {
        let plan = diagram();
        assert_eq!(plan.legs("write"), vec![Some("gate"), Some("store")]);
        let mut scene = PlanBuilder::new("relay", 8_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan).unwrap();
        let arrivals = stage.relay(&mut scene, "write", 1_000_000_000, 1.0);
        // Two legs of 0.5 s; the second gathers after the first lands and rests.
        assert_eq!(
            arrivals,
            vec![
                1_500_000_000,
                1_500_000_000 + 120_000_000 + 340_000_000 + 500_000_000
            ]
        );
        let plan = scene.finish().unwrap();
        let channel = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|c| c.property == property)
                .unwrap_or_else(|| panic!("missing {property}"))
        };
        assert!(
            matches!(
                channel("write.age").events[1],
                crate::plan::TrackEventPlan::Ease {
                    duration_nanos: 4_960_000_000,
                    ..
                }
            ),
            "the clock runs through the last leg's life"
        );
        channel("gate.flash");
        channel("store.pulse");
        assert_eq!(packet::leg_start(0, 2, 1.0), 0.0);
        assert!((packet::leg_arrival(1, 2, 1.0) - 1.8).abs() < 1e-6);
        assert_eq!(
            packet::lifetime(1, 0.8),
            packet::LIFETIME,
            "a beam is one leg"
        );
    }

    #[test]
    fn a_packet_can_be_sent_again_after_it_lands() {
        let mut scene = PlanBuilder::new("resend", 12_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let first = stage.send(&mut scene, "probe", 1_000_000_000, 0.8);
        let second = stage.send(&mut scene, "probe", 6_000_000_000, 0.5);
        assert_eq!((first, second), (1_800_000_000, 6_500_000_000));
        let plan = scene.finish().unwrap();
        let timeline = crate::plan::compile_channels(
            plan.continuous_channels
                .iter()
                .map(|c| (c, crate::timeline::PropertyId::new(&c.property))),
            plan.duration_nanos,
            |scalar| match scalar {
                crate::plan::ScalarPlan::Literal(value) => Ok(*value),
                _ => unreachable!(),
            },
        )
        .unwrap();
        let sample = |property: &str, time: f64| {
            timeline
                .sample_at(&crate::timeline::PropertyId::new(property), time)
                .unwrap()
                .position
        };
        assert!((sample("probe.age", 1.66) - 1.0).abs() < 1e-3);
        assert!(
            (sample("probe.age", 5.0) - packet::LIFETIME).abs() < 1e-3,
            "spent between sends"
        );
        assert!(
            sample("probe.age", 5.66) < 0.01,
            "the second dispatch restarts the clock"
        );
        assert_eq!(sample("probe.flight", 5.7), 0.5);
    }

    #[test]
    fn forms_match_their_shapes_and_morph_exactly_onto_each() {
        let shapes = [FormShape::cube(200.0), FormShape::Sphere { radius: 130.0 }];
        let points = form_points(&shapes, 300);
        assert_eq!(points.len(), 2);
        assert!(points.iter().all(|shape| shape.len() == 300));
        assert_eq!(points, form_points(&shapes, 300), "deterministic");
        for index in [0, 17, 299] {
            let seed = hash(index as u32, 3);
            assert_eq!(morph_point(&points, index, seed, 0.0), points[0][index]);
            assert_eq!(morph_point(&points, index, seed, 1.0), points[1][index]);
            assert_eq!(
                morph_point(&points, index, seed, 7.0),
                points[1][index],
                "held at the last"
            );
            let middle = morph_point(&points, index, seed, 0.5);
            assert!(middle.is_finite() && middle != points[0][index]);
        }
        // Early points are still on the cube while late ones have left.
        let moved = (0..300)
            .filter(|&i| morph_point(&points, i, hash(i as u32, 3), 0.2) != points[0][i])
            .count();
        assert!(moved > 0 && moved < 300, "{moved}");
        assert_eq!(shapes[0].radius(), Vec3::splat(100.0).length());
    }

    #[test]
    fn new_elements_attach_to_their_resting_outlines() {
        use crate::math::vec2;
        let plan = diagram();
        let outline = |id: &str| plan.element(id).unwrap().outline(vec2(0.0, 0.0), 1.0);
        assert_eq!(
            outline("gate"),
            Shape::Box(Box2::from_center_size(Vec2::ZERO, vec2(120.0, 160.0)))
        );
        assert!(matches!(
            outline("loop"),
            Shape::Circle(Circle { radius: 50.0, .. })
        ));
        assert_eq!(outline("lock").distance(vec2(24.0, 0.0)), 0.0);
        assert!(matches!(outline("store"), Shape::Box(_)));
        let triangle: StageElement = serde_json::from_value(serde_json::json!({
            "kind": "shape", "id": "t", "at": [0, 0, 0],
            "shape": { "polygon": [[0, -40], [40, 30], [-40, 30]] }
        }))
        .unwrap();
        let Shape::Polygon(hull) = triangle.outline(vec2(100.0, 100.0), 2.0) else {
            panic!("a polygon attaches to its hull");
        };
        assert_eq!(hull.vertices().len(), 3);
        assert_eq!(
            hull.port_toward(vec2(100.0, 1000.0)).point,
            vec2(100.0, 160.0)
        );
    }

    #[test]
    fn hits_strike_at_once_and_decay() {
        let mut scene = PlanBuilder::new("stage-demo", 5_000_000_000);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        stage.hit(&mut scene, "client.flash", 1_000_000_000, 1.0, 0.0);
        let landing = stage.settle_in(&mut scene, "client", 2_000_000_000);
        assert_eq!(landing, 2_500_000_000);
        let plan = scene.finish().unwrap();
        let flash = plan
            .continuous_channels
            .iter()
            .find(|c| c.property == "client.flash")
            .unwrap();
        assert!(matches!(
            flash.events[0],
            crate::plan::TrackEventPlan::Set {
                at_nanos: 1_000_000_000,
                ..
            }
        ));
        assert!(matches!(
            flash.events[1],
            crate::plan::TrackEventPlan::Ease {
                curve: Ease::CubicOut,
                ..
            }
        ));
    }
}
