//! Hierarchical 2D/2.5D transform groups (`GroupXform`) and GPU matte masks
//! (`ClipMask`: rounded rect, circle, polygon, SVG path, or display-type silhouette).

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    face::Face,
    math::{Vec2, Vec3, vec2, vec3},
    stage::{StageElement, StagePlan},
};

pub const GROUP_DEFAULTS: [(&str, f32); 13] = [
    ("opacity", 1.0),
    ("x", 0.0),
    ("y", 0.0),
    ("z", 0.0),
    ("scale", 1.0),
    ("scale-x", 1.0),
    ("scale-y", 1.0),
    ("rotation", 0.0),
    ("skew", 0.0),
    ("blur", 0.0),
    ("clip-x", 0.0),
    ("clip-y", 0.0),
    ("clip-w", 0.0),
];

/// The largest skew angle a group takes, in radians, short of a degenerate shear.
pub const MAX_SKEW: f32 = 1.4;

/// GPU-evaluated matte applied to every primitive inside a `StageElement::Group`,
/// in the group's local space, so it moves, scales, rotates, and skews with it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ClipMask {
    /// Rounded-rectangle aperture (`clip-w` widens or narrows it).
    #[serde(rename_all = "camelCase")]
    Rect {
        size: [f32; 2],
        #[serde(default, skip_serializing_if = "is_zero")]
        corner: f32,
        #[serde(default, skip_serializing_if = "is_zero2")]
        offset: [f32; 2],
        #[serde(default, skip_serializing_if = "is_zero")]
        feather: f32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        invert: bool,
    },
    /// Circular iris (`clip-w` adds to its radius).
    #[serde(rename_all = "camelCase")]
    Circle {
        radius: f32,
        #[serde(default, skip_serializing_if = "is_zero2")]
        offset: [f32; 2],
        #[serde(default, skip_serializing_if = "is_zero")]
        feather: f32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        invert: bool,
    },
    /// A closed polygon through `points`, filled non-zero.
    #[serde(rename_all = "camelCase")]
    Polygon {
        points: Vec<[f32; 2]>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        invert: bool,
    },
    /// SVG path data (`d`) in group pixels, filled non-zero.
    #[serde(rename_all = "camelCase")]
    Path {
        d: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        invert: bool,
    },
    /// Display type's ink: every child (footage, gradients, shapes) shows inside
    /// the glyphs, centered on the group's origin.
    #[serde(rename_all = "camelCase", alias = "text-matte")]
    Type {
        text: String,
        #[serde(default, skip_serializing_if = "Face::is_mono")]
        face: Face,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        size: f32,
        #[serde(
            default = "default_matte_weight",
            skip_serializing_if = "is_default_matte_w"
        )]
        weight: u16,
        #[serde(default, skip_serializing_if = "is_zero")]
        tracking: f32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        invert: bool,
    },
}

const fn default_matte_weight() -> u16 {
    800
}
fn is_default_matte_w(w: &u16) -> bool {
    *w == 800
}
fn is_zero(v: &f32) -> bool {
    v.abs() <= f32::EPSILON
}
fn is_zero2(v: &[f32; 2]) -> bool {
    is_zero(&v[0]) && is_zero(&v[1])
}

impl ClipMask {
    pub const fn rect(size: [f32; 2]) -> Self {
        Self::Rect {
            size,
            corner: 0.0,
            offset: [0.0, 0.0],
            feather: 0.0,
            invert: false,
        }
    }

    pub const fn circle(radius: f32) -> Self {
        Self::Circle {
            radius,
            offset: [0.0, 0.0],
            feather: 0.0,
            invert: false,
        }
    }

    pub fn polygon(points: impl IntoIterator<Item = [f32; 2]>) -> Self {
        Self::Polygon {
            points: points.into_iter().collect(),
            invert: false,
        }
    }

    pub fn path(d: impl Into<String>) -> Self {
        Self::Path {
            d: d.into(),
            invert: false,
        }
    }

    pub fn r#type(text: impl Into<String>, face: Face, size: f32) -> Self {
        Self::Type {
            text: text.into(),
            face,
            family: None,
            size,
            weight: 800,
            tracking: -0.02,
            invert: false,
        }
    }

    pub const fn corner(mut self, radius: f32) -> Self {
        if let Self::Rect { corner, .. } = &mut self {
            *corner = radius;
        }
        self
    }

    pub const fn offset(mut self, xy: [f32; 2]) -> Self {
        if let Self::Rect { offset, .. } | Self::Circle { offset, .. } = &mut self {
            *offset = xy;
        }
        self
    }

    pub const fn feather(mut self, px: f32) -> Self {
        if let Self::Rect { feather, .. } | Self::Circle { feather, .. } = &mut self {
            *feather = px;
        }
        self
    }

    pub const fn inverted(mut self) -> Self {
        match &mut self {
            Self::Rect { invert, .. }
            | Self::Circle { invert, .. }
            | Self::Polygon { invert, .. }
            | Self::Path { invert, .. }
            | Self::Type { invert, .. } => *invert = true,
        }
        self
    }

    pub fn family(mut self, name: impl Into<String>) -> Self {
        if let Self::Type { family, .. } = &mut self {
            *family = Some(name.into());
        }
        self
    }

    pub const fn weight(mut self, w: u16) -> Self {
        if let Self::Type { weight, .. } = &mut self {
            *weight = w;
        }
        self
    }

    pub const fn tracking(mut self, em: f32) -> Self {
        if let Self::Type { tracking, .. } = &mut self {
            *tracking = em;
        }
        self
    }

    /// Whether the matte keeps what lies outside its shape instead of inside.
    pub const fn is_inverted(&self) -> bool {
        match self {
            Self::Rect { invert, .. }
            | Self::Circle { invert, .. }
            | Self::Polygon { invert, .. }
            | Self::Path { invert, .. }
            | Self::Type { invert, .. } => *invert,
        }
    }

    /// The group-local box a `Rect`, `Circle`, or `Polygon` matte covers at rest,
    /// as (center, half size). Paths and type are measured when rasterized.
    pub fn local_extent(&self) -> Option<(Vec2, Vec2)> {
        match self {
            Self::Rect { size, offset, .. } => Some((Vec2::from(*offset), Vec2::from(*size) * 0.5)),
            Self::Circle { radius, offset, .. } => {
                Some((Vec2::from(*offset), Vec2::splat(*radius)))
            }
            Self::Polygon { points, .. } => {
                let (low, high) = points.iter().fold(
                    (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                    |(low, high), p| (low.min(Vec2::from(*p)), high.max(Vec2::from(*p))),
                );
                Some(((low + high) * 0.5, (high - low) * 0.5))
            }
            Self::Path { .. } | Self::Type { .. } => None,
        }
    }

    pub fn validate(&self, id: &str) -> Result<()> {
        match self {
            Self::Rect {
                size,
                corner,
                offset,
                feather,
                ..
            } => {
                ensure!(
                    size.iter()
                        .all(|v| v.is_finite() && *v > 0.0 && *v <= 8192.0),
                    "group '{id}' rect clip size out of range"
                );
                ensure!(
                    corner.is_finite()
                        && *corner >= 0.0
                        && offset.iter().all(|v| v.is_finite())
                        && feather.is_finite()
                        && *feather >= 0.0,
                    "group '{id}' rect clip parameters must be finite and non-negative"
                );
            }
            Self::Circle {
                radius,
                offset,
                feather,
                ..
            } => {
                ensure!(
                    radius.is_finite()
                        && (0.5..=8192.0).contains(radius)
                        && offset.iter().all(|v| v.is_finite())
                        && feather.is_finite()
                        && *feather >= 0.0,
                    "group '{id}' circle clip parameters out of range"
                );
            }
            Self::Polygon { points, .. } => {
                ensure!(
                    (3..=512).contains(&points.len()),
                    "group '{id}' polygon clip has 3 to 512 points"
                );
                ensure!(
                    points
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && v.abs() <= 8192.0),
                    "group '{id}' polygon clip points must be finite and within 8192px"
                );
                let (_, half) = self.local_extent().expect("polygons have an extent");
                ensure!(
                    half.x > 0.25 && half.y > 0.25,
                    "group '{id}' polygon clip has no area"
                );
            }
            Self::Path { d, .. } => {
                ensure!(
                    !d.trim().is_empty() && d.len() <= 64 * 1024,
                    "group '{id}' path clip needs SVG path data under 64 KiB"
                );
                ensure!(
                    !d.contains(['"', '<', '>', '&']),
                    "group '{id}' path clip data must be plain SVG path commands"
                );
            }
            Self::Type {
                text,
                size,
                weight,
                tracking,
                ..
            } => {
                if text.trim().is_empty() || text.contains('\n') {
                    bail!("group '{id}' type matte must be a non-empty single line");
                }
                ensure!(
                    size.is_finite()
                        && (12.0..=640.0).contains(size)
                        && (100..=1000).contains(weight)
                        && tracking.is_finite()
                        && (-0.25..=1.0).contains(tracking),
                    "group '{id}' type matte metrics out of range"
                );
            }
        }
        Ok(())
    }
}

/// One group's own channels this sample, read through its Stage defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupMotion {
    pub offset: Vec3,
    pub scale: Vec2,
    pub rotation: f32,
    pub skew: f32,
    pub opacity: f32,
    pub blur: f32,
}

impl Default for GroupMotion {
    fn default() -> Self {
        Self {
            offset: Vec3::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
            skew: 0.0,
            opacity: 1.0,
            blur: 0.0,
        }
    }
}

impl GroupMotion {
    /// Read a group's channels with `read(property)`, which returns the channel
    /// or its default.
    pub fn read(read: impl Fn(&str) -> f32) -> Self {
        let scale = read("scale");
        Self {
            offset: vec3(read("x"), read("y"), read("z")),
            scale: vec2(
                (scale * read("scale-x")).max(0.01),
                (scale * read("scale-y")).max(0.01),
            ),
            rotation: read("rotation"),
            skew: read("skew").clamp(-MAX_SKEW, MAX_SKEW),
            opacity: read("opacity").clamp(0.0, 1.0),
            blur: read("blur").max(0.0),
        }
    }
}

/// A composed group transform: group-local pixels to world pixels. The 2D part
/// is a full affine map (scale, skew, and rotation compose exactly down the
/// hierarchy); depth scales with the groups' area scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupXform {
    pub origin: Vec3,
    /// Where one local pixel along +x and +y lands.
    pub x_axis: Vec2,
    pub y_axis: Vec2,
    pub depth: f32,
    pub opacity: f32,
    pub blur: f32,
}

impl Default for GroupXform {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            x_axis: Vec2::X,
            y_axis: Vec2::Y,
            depth: 1.0,
            opacity: 1.0,
            blur: 0.0,
        }
    }
}

impl GroupXform {
    /// The linear part applied to a local vector.
    pub fn linear(self, v: Vec2) -> Vec2 {
        self.x_axis * v.x + self.y_axis * v.y
    }

    pub fn transform_point(self, local: Vec3) -> Vec3 {
        let flat = self.linear(local.truncate());
        vec3(
            self.origin.x + flat.x,
            self.origin.y + flat.y,
            self.origin.z + local.z * self.depth,
        )
    }

    /// The angle local +x turns to.
    pub fn rotation(self) -> f32 {
        self.x_axis.y.atan2(self.x_axis.x)
    }

    /// The square root of the area scale: what a child drawn without
    /// distortion scales by.
    pub fn uniform_scale(self) -> f32 {
        self.x_axis.perp_dot(self.y_axis).abs().sqrt().max(0.01)
    }

    /// What is left of the linear map once a child has drawn itself at the
    /// uniform scale and turned by `drawn_turn`: the screen-space map `W` (as
    /// columns) with `W · scale · R(drawn_turn) = linear`. Identity for plain
    /// zooms, and for turns the child draws itself.
    pub fn residual(self, drawn_turn: f32) -> [Vec2; 2] {
        let s = self.uniform_scale();
        let (sin, cos) = drawn_turn.sin_cos();
        [
            self.linear(vec2(cos, -sin)) / s,
            self.linear(vec2(sin, cos)) / s,
        ]
    }

    pub fn has_residual(self, drawn_turn: f32) -> bool {
        let [x, y] = self.residual(drawn_turn);
        (x - Vec2::X).abs().max_element() > 1e-4 || (y - Vec2::Y).abs().max_element() > 1e-4
    }

    /// Compose a child group at `at` (in this group's space) with `pivot`
    /// (relative to `at`) and its own `motion`. The child scales, skews, and
    /// turns about its pivot, in that order.
    pub fn child(self, at: [f32; 3], pivot: [f32; 2], motion: GroupMotion) -> Self {
        let piv = Vec2::from(pivot);
        let turn = Vec2::from_angle(motion.rotation);
        let own_x = turn.rotate(vec2(motion.scale.x, 0.0));
        let own_y = turn.rotate(vec2(motion.skew.tan() * motion.scale.y, motion.scale.y));
        let x_axis = self.linear(own_x);
        let y_axis = self.linear(own_y);
        let world_piv = self.transform_point(Vec3::from(at) + motion.offset + piv.extend(0.0));
        let shift = x_axis * piv.x + y_axis * piv.y;
        Self {
            origin: vec3(world_piv.x - shift.x, world_piv.y - shift.y, world_piv.z),
            x_axis,
            y_axis,
            depth: self.depth * (motion.scale.x * motion.scale.y).sqrt(),
            opacity: self.opacity * motion.opacity,
            blur: self.blur + motion.blur,
        }
    }
}

impl StagePlan {
    /// The groups around `id`, outermost first; empty for a top-level element
    /// or an unknown ID.
    pub fn ancestors(&self, id: &str) -> Vec<&StageElement> {
        fn search<'a>(
            items: &'a [StageElement],
            id: &str,
            path: &mut Vec<&'a StageElement>,
        ) -> bool {
            for item in items {
                if item.id() == id {
                    return true;
                }
                if let StageElement::Group { children, .. } = item {
                    path.push(item);
                    if search(children, id, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            false
        }
        let mut path = Vec::new();
        if !search(&self.elements, id, &mut path) {
            path.clear();
        }
        path
    }

    /// The composed transform of the groups around `id` (identity at top
    /// level), reading channels with `read(element, property)`.
    pub fn parent_xform(&self, id: &str, read: impl Fn(&str, &str) -> f32) -> GroupXform {
        self.ancestors(id)
            .into_iter()
            .fold(GroupXform::default(), |parent, group| {
                group_step(parent, group, &read)
            })
    }

    /// A group's own composed transform: its parents' and then its own.
    pub fn group_xform(&self, group: &str, read: impl Fn(&str, &str) -> f32) -> Option<GroupXform> {
        let element = self.element(group)?;
        matches!(element, StageElement::Group { .. })
            .then(|| group_step(self.parent_xform(group, &read), element, &read))
    }
}

/// Compose `group` inside `parent`, reading its channels with `read`.
pub fn group_step(
    parent: GroupXform,
    group: &StageElement,
    read: impl Fn(&str, &str) -> f32,
) -> GroupXform {
    let StageElement::Group { id, at, pivot, .. } = group else {
        return parent;
    };
    parent.child(*at, *pivot, GroupMotion::read(|p| read(id, p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).abs().max_element() < 1e-3
    }

    fn motion(scale: f32, rotation: f32, opacity: f32) -> GroupMotion {
        GroupMotion {
            scale: Vec2::splat(scale),
            rotation,
            opacity,
            ..GroupMotion::default()
        }
    }

    #[test]
    fn group_xform_rotates_about_pivot() {
        let root = GroupXform::default().child(
            [500.0, 300.0, 0.0],
            [0.0, 100.0],
            motion(2.0, FRAC_PI_2, 0.8),
        );
        // The pivot stays put whatever the scale and turn.
        assert!(close(
            root.transform_point(vec3(0.0, 100.0, 0.0)),
            vec3(500.0, 400.0, 0.0)
        ));
        // The origin is (0, -100) from the pivot, doubled to (0, -200), turned to (200, 0).
        assert!(close(
            root.transform_point(Vec3::ZERO),
            vec3(700.0, 400.0, 0.0)
        ));
        assert!((root.opacity - 0.8).abs() < 1e-5);
        assert!((root.rotation() - FRAC_PI_2).abs() < 1e-5);
        assert!((root.uniform_scale() - 2.0).abs() < 1e-5);
        assert!(!root.has_residual(root.rotation()));
        // A child that cannot turn itself is turned by the residual instead.
        assert!(root.has_residual(0.0));
    }

    #[test]
    fn nested_groups_compose_like_their_matrices() {
        let outer =
            GroupXform::default().child([100.0, 0.0, 0.0], [0.0, 0.0], motion(2.0, FRAC_PI_2, 0.5));
        let inner = outer.child(
            [10.0, 0.0, 0.0],
            [0.0, 0.0],
            GroupMotion {
                offset: vec3(0.0, 5.0, 0.0),
                scale: vec2(3.0, 1.0),
                opacity: 0.5,
                blur: 1.0,
                ..GroupMotion::default()
            },
        );
        // Inner origin: outer maps (10, 5) to 100 + R90 * (20, 10) = (90, 20).
        assert!(close(
            inner.transform_point(Vec3::ZERO),
            vec3(90.0, 20.0, 0.0)
        ));
        // A point at (1, 1) in the inner group: stretched to (3, 1), then
        // doubled and turned by the outer group: (-2, 6).
        assert!(close(
            inner.transform_point(vec3(1.0, 1.0, 0.0)),
            vec3(88.0, 26.0, 0.0)
        ));
        assert!((inner.opacity - 0.25).abs() < 1e-6);
        assert!((inner.blur - 1.0).abs() < 1e-6);
        // Unequal scale leaves a residual a child must be warped by:
        // residual · scale · R(turn) reproduces the linear map.
        for turn in [0.0, inner.rotation(), 0.3] {
            assert!(inner.has_residual(turn));
            let [wx, wy] = inner.residual(turn);
            let drawn = |v: Vec2| Vec2::from_angle(turn).rotate(v) * inner.uniform_scale();
            let apply = |v: Vec2| wx * v.x + wy * v.y;
            assert!((apply(drawn(Vec2::X)) - inner.x_axis).length() < 1e-4);
            assert!((apply(drawn(Vec2::Y)) - inner.y_axis).length() < 1e-4);
        }
    }

    #[test]
    fn skew_shears_local_y_toward_x() {
        let gx = GroupXform::default().child(
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
            GroupMotion {
                skew: std::f32::consts::FRAC_PI_4,
                ..GroupMotion::default()
            },
        );
        assert!(close(
            gx.transform_point(vec3(0.0, 10.0, 0.0)),
            vec3(10.0, 10.0, 0.0)
        ));
        assert!(close(
            gx.transform_point(vec3(10.0, 0.0, 0.0)),
            vec3(10.0, 0.0, 0.0)
        ));
        assert!((gx.uniform_scale() - 1.0).abs() < 1e-5);
        assert!(gx.has_residual(gx.rotation()));
    }

    #[test]
    fn plan_parent_xform_walks_ancestors() {
        let plan: StagePlan = serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "group", "id": "outer", "at": [960, 540, 0], "children": [
                    { "kind": "group", "id": "inner", "at": [100, 0, 0], "children": [
                        { "kind": "orb", "id": "dot", "at": [10, 0, 0], "radius": 20 }
                    ] }
                ] },
                { "kind": "orb", "id": "free", "at": [0, 0, 0], "radius": 20 }
            ]
        }))
        .unwrap();
        plan.validate().unwrap();
        let read = |id: &str, p: &str| match (id, p) {
            ("outer", "scale") => 2.0,
            ("inner", "rotation") => FRAC_PI_2,
            _ => plan.channel_default(&format!("{id}.{p}")).unwrap_or(0.0),
        };
        let ids = plan
            .ancestors("dot")
            .iter()
            .map(|e| e.id())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["outer", "inner"]);
        assert!(plan.ancestors("free").is_empty());
        let parent = plan.parent_xform("dot", read);
        // inner sits at 960 + 2 * 100; dot is 10 along inner's +x, now pointing down, doubled.
        assert!(close(
            parent.transform_point(vec3(10.0, 0.0, 0.0)),
            vec3(1160.0, 560.0, 0.0)
        ));
        assert_eq!(plan.parent_xform("free", read), GroupXform::default());
        assert_eq!(plan.group_xform("inner", read), Some(parent));
        assert_eq!(plan.group_xform("dot", read), None);
    }

    #[test]
    fn clip_masks_validate_and_round_trip() {
        let masks = [
            ClipMask::r#type("MOTION", Face::Shout, 180.0).tracking(-0.04),
            ClipMask::rect([400.0, 200.0]).corner(24.0).feather(2.0),
            ClipMask::circle(90.0).inverted(),
            ClipMask::polygon([[0.0, -100.0], [90.0, 60.0], [-90.0, 60.0]]),
            ClipMask::path("M -50 -50 L 50 -50 L 0 50 Z"),
        ];
        for m in masks {
            m.validate("hero").unwrap();
            let json = serde_json::to_string(&m).unwrap();
            let back: ClipMask = serde_json::from_str(&json).unwrap();
            assert_eq!(back, m);
        }
        let legacy: ClipMask =
            serde_json::from_str(r#"{"kind":"text-matte","text":"GO","size":120}"#).unwrap();
        assert!(matches!(legacy, ClipMask::Type { .. }));
        assert!(
            ClipMask::polygon([[0.0, 0.0], [1.0, 0.0]])
                .validate("x")
                .is_err()
        );
        assert!(
            ClipMask::polygon([[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]])
                .validate("x")
                .is_err()
        );
        assert!(ClipMask::path("M 0 0\"/>").validate("x").is_err());
    }
}
