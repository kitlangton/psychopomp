//! Hierarchical 2D/2.5D transform groups (`GroupXform`) and GPU matte masks
//! (`ClipMask`: rounded rect, circle/iris, or `TextMatte` display-glyph silhouette).

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    face::Face,
    math::{Vec2, Vec3, vec2, vec3},
};

pub const GROUP_DEFAULTS: [(&str, f32); 12] = [
    ("opacity", 1.0),
    ("x", 0.0),
    ("y", 0.0),
    ("z", 0.0),
    ("scale", 1.0),
    ("scale-x", 1.0),
    ("scale-y", 1.0),
    ("rotation", 0.0),
    ("blur", 0.0),
    ("clip-x", 0.0),
    ("clip-y", 0.0),
    ("clip-w", 0.0),
];

/// GPU-evaluated matte mask applied to every primitive inside a `StageElement::Group`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ClipMask {
    /// Rounded-rectangle aperture in the group's local space (`clip-w` widens/narrows it).
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
    /// Circular/iris aperture in the group's local space (`clip-w` adjusts radius).
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
    /// Alpha matte shaped from display typography: all children inside the `Group`
    /// (footage, gradients, shapes, procedural shaders) appear inside the glyph ink!
    #[serde(rename_all = "camelCase")]
    TextMatte {
        text: String,
        #[serde(default, skip_serializing_if = "Face::is_mono")]
        face: Face,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        size: f32,
        #[serde(default = "default_matte_weight", skip_serializing_if = "is_default_matte_w")]
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

    pub fn text_matte(text: impl Into<String>, face: Face, size: f32) -> Self {
        Self::TextMatte {
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
        match &mut self {
            Self::Rect { offset, .. } | Self::Circle { offset, .. } => *offset = xy,
            Self::TextMatte { .. } => {}
        }
        self
    }

    pub const fn feather(mut self, px: f32) -> Self {
        match &mut self {
            Self::Rect { feather, .. } | Self::Circle { feather, .. } => *feather = px,
            Self::TextMatte { .. } => {}
        }
        self
    }

    pub const fn inverted(mut self) -> Self {
        match &mut self {
            Self::Rect { invert, .. }
            | Self::Circle { invert, .. }
            | Self::TextMatte { invert, .. } => *invert = true,
        }
        self
    }

    pub fn family(mut self, name: impl Into<String>) -> Self {
        if let Self::TextMatte { family, .. } = &mut self {
            *family = Some(name.into());
        }
        self
    }

    pub const fn weight(mut self, w: u16) -> Self {
        if let Self::TextMatte { weight, .. } = &mut self {
            *weight = w;
        }
        self
    }

    pub const fn tracking(mut self, em: f32) -> Self {
        if let Self::TextMatte { tracking, .. } = &mut self {
            *tracking = em;
        }
        self
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
                    size.iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 8192.0),
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
            Self::TextMatte {
                text,
                size,
                weight,
                tracking,
                ..
            } => {
                if text.trim().is_empty() || text.contains('\n') {
                    bail!("group '{id}' text-matte must be a non-empty single line");
                }
                ensure!(
                    size.is_finite()
                        && (12.0..=640.0).contains(size)
                        && (100..=1000).contains(weight)
                        && tracking.is_finite()
                        && (-0.25..=1.0).contains(tracking),
                    "group '{id}' text-matte metrics out of range"
                );
            }
        }
        Ok(())
    }
}

/// Composed world-space group transform propagated from root groups to children.
#[derive(Clone, Copy, Debug)]
pub struct GroupXform {
    pub origin: Vec3,
    pub scale: Vec2,
    pub rotation: f32,
    pub opacity: f32,
    pub blur: f32,
}

impl Default for GroupXform {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
            opacity: 1.0,
            blur: 0.0,
        }
    }
}

impl GroupXform {
    pub fn transform_point(self, local: Vec3) -> Vec3 {
        let scaled = vec2(local.x * self.scale.x, local.y * self.scale.y);
        let turned = Vec2::from_angle(self.rotation).rotate(scaled);
        vec3(
            self.origin.x + turned.x,
            self.origin.y + turned.y,
            self.origin.z + local.z * self.uniform_scale(),
        )
    }

    pub fn uniform_scale(self) -> f32 {
        (self.scale.x.abs() * self.scale.y.abs()).sqrt().max(0.01)
    }

    /// Compose a child group (`at`, `pivot`, `offset`, `scale_xy`, `rot`, `opacity`, `blur`)
    /// inside `self`. Rotation and scaling happen around `pivot` in the child group's local space.
    #[allow(clippy::too_many_arguments)]
    pub fn child_group(
        self,
        at: [f32; 3],
        pivot: [f32; 2],
        offset: Vec3,
        scale_xy: Vec2,
        rot: f32,
        opacity: f32,
        blur: f32,
    ) -> Self {
        let piv = Vec2::from(pivot);
        let base_local = Vec3::from(at) + offset + vec3(piv.x, piv.y, 0.0);
        let world_piv = self.transform_point(base_local);
        let combined_scale = self.scale * scale_xy;
        let combined_rot = self.rotation + rot;
        // A point `p` in child space maps to `world_piv + rot(combined_rot, (p - piv) * combined_scale)`.
        let neg_piv_scaled = vec2(-piv.x * combined_scale.x, -piv.y * combined_scale.y);
        let neg_piv_turned = Vec2::from_angle(combined_rot).rotate(neg_piv_scaled);
        Self {
            origin: vec3(
                world_piv.x + neg_piv_turned.x,
                world_piv.y + neg_piv_turned.y,
                world_piv.z,
            ),
            scale: combined_scale,
            rotation: combined_rot,
            opacity: self.opacity * opacity.clamp(0.0, 1.0),
            blur: self.blur + blur.max(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn group_xform_rotates_about_pivot_and_composes_nested_children() {
        let root = GroupXform::default().child_group(
            [500.0, 300.0, 0.0],
            [0.0, 100.0],
            Vec3::ZERO,
            Vec2::splat(2.0),
            FRAC_PI_2,
            0.8,
            1.5,
        );
        // The pivot point [0, 100, 0] in group space lands at [500, 400, 0] regardless of scale/rotation!
        let at_piv = root.transform_point(vec3(0.0, 100.0, 0.0));
        assert!((at_piv.x - 500.0).abs() < 1e-3 && (at_piv.y - 400.0).abs() < 1e-3);

        // [0, 0, 0] is (0, -100) from pivot -> scaled 2x = (0, -200) -> rotated +90° = (+200, 0) -> [700, 400]
        let origin_pt = root.transform_point(Vec3::ZERO);
        assert!((origin_pt.x - 700.0).abs() < 1e-3 && (origin_pt.y - 400.0).abs() < 1e-3);
        assert!((root.opacity - 0.8).abs() < 1e-5);
    }

    #[test]
    fn clip_masks_validate_and_round_trip() {
        let m = ClipMask::text_matte("MOTION", Face::Shout, 180.0).tracking(-0.04);
        m.validate("hero").unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let back: ClipMask = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
