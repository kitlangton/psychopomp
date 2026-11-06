//! Custom WGSL `Shader` layers: a user fragment module painted on a
//! camera-facing quad, with typed scalar and color parameters that are
//! ordinary Stage channels (`<id>.<param>`, `<id>.<param>.r|g|b`).

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// The transform channels every shader layer reads, before its parameters.
/// `blur` softens the layer's edge (with depth of field), not its pixels.
pub const SHADER_BASE_DEFAULTS: [(&str, f32); 7] = [
    ("opacity", 1.0),
    ("x", 0.0),
    ("y", 0.0),
    ("z", 0.0),
    ("scale", 1.0),
    ("rotation", 0.0),
    ("blur", 0.0),
];

/// How many floats a layer's parameters may take in all (a scalar is one, a
/// color three): the renderer packs them into eight `vec4`s of its primitive.
pub const SHADER_PARAM_FLOATS: usize = 32;

/// The color components a color parameter's channels name, in sRGB 0..1.
pub const COLOR_COMPONENTS: [&str; 3] = ["r", "g", "b"];

/// One typed parameter of a shader layer. Its name is a WGSL field of the
/// generated `Params` struct and the Stage channel that animates it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ShaderParam {
    /// An `f32`; `range` bounds Studio's knob and nothing else.
    Scalar {
        name: String,
        default: f32,
        #[serde(default = "unit_range")]
        range: [f32; 2],
    },
    /// A `vec3<f32>` of linear light, authored as sRGB: its channels
    /// `<param>.r`, `.g`, `.b` are sRGB components 0..1.
    Color { name: String, default: [u8; 3] },
}

fn unit_range() -> [f32; 2] {
    [0.0, 1.0]
}

impl ShaderParam {
    pub fn scalar(name: &str, default: f32, range: [f32; 2]) -> Self {
        Self::Scalar {
            name: name.into(),
            default,
            range,
        }
    }

    pub fn color(name: &str, default: [u8; 3]) -> Self {
        Self::Color {
            name: name.into(),
            default,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Scalar { name, .. } | Self::Color { name, .. } => name,
        }
    }

    /// Floats this parameter takes in the packed parameter block.
    pub fn floats(&self) -> usize {
        match self {
            Self::Scalar { .. } => 1,
            Self::Color { .. } => 3,
        }
    }

    /// Its channel properties (after the element's `<id>.`), their defaults,
    /// and the knob range of each, in packing order.
    pub fn channels(&self) -> Vec<(String, f32, [f32; 2])> {
        match self {
            Self::Scalar {
                name,
                default,
                range,
            } => vec![(name.clone(), *default, *range)],
            Self::Color { name, default } => COLOR_COMPONENTS
                .iter()
                .zip(default)
                .map(|(c, v)| (format!("{name}.{c}"), f32::from(*v) / 255.0, [0.0, 1.0]))
                .collect(),
        }
    }
}

/// What `property` of a shader layer with `params` reads before anything
/// writes it, if the layer reads it.
pub fn shader_property_default(params: &[ShaderParam], property: &str) -> Option<f32> {
    if let Some(&(_, v)) = SHADER_BASE_DEFAULTS.iter().find(|(k, _)| *k == property) {
        return Some(v);
    }
    params
        .iter()
        .flat_map(ShaderParam::channels)
        .find(|(name, ..)| name == property)
        .map(|(_, v, _)| v)
}

/// Validate a shader layer's geometry, parameters, and that its module
/// declares the `shade` entry function. WGSL itself compiles at preflight.
pub fn validate_shader(
    id: &str,
    size: [f32; 2],
    corner: f32,
    wgsl: &str,
    params: &[ShaderParam],
) -> Result<()> {
    ensure!(
        size.iter().all(|v| (1.0..=8192.0).contains(v)),
        "shader '{id}' size is out of range"
    );
    ensure!(
        corner.is_finite() && (0.0..=size[0].min(size[1]) * 0.5).contains(&corner),
        "shader '{id}' corner is out of range"
    );
    ensure!(
        wgsl.len() <= 64_000 && wgsl.contains("fn shade("),
        "shader '{id}' WGSL must define `fn shade(s: ShaderIn, p: Params) -> vec4<f32>` in at most 64000 bytes"
    );
    let mut seen = std::collections::HashSet::new();
    for param in params {
        let name = param.name();
        let ident = name.len() <= 24
            && name.starts_with(|c: char| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        ensure!(
            ident,
            "shader '{id}' parameter '{name}' must be a snake_case WGSL name of at most 24 characters"
        );
        ensure!(
            !SHADER_BASE_DEFAULTS.iter().any(|(k, _)| *k == name),
            "shader '{id}' parameter '{name}' shadows a transform channel"
        );
        ensure!(
            seen.insert(name),
            "shader '{id}' declares parameter '{name}' twice"
        );
        if let ShaderParam::Scalar { default, range, .. } = param {
            ensure!(
                default.is_finite() && range.iter().all(|v| v.is_finite()) && range[0] < range[1],
                "shader '{id}' parameter '{name}' needs a finite default and an increasing range"
            );
        }
    }
    let floats = params.iter().map(ShaderParam::floats).sum::<usize>();
    ensure!(
        floats <= SHADER_PARAM_FLOATS,
        "shader '{id}' parameters take {floats} floats; the limit is {SHADER_PARAM_FLOATS} (a color takes 3)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADE: &str = "fn shade(s: ShaderIn, p: Params) -> vec4<f32> { return vec4<f32>(1.0); }";

    #[test]
    fn color_params_expand_to_srgb_component_channels() {
        let params = [
            ShaderParam::scalar("amount", 0.4, [0.0, 2.0]),
            ShaderParam::color("tint", [255, 0, 51]),
        ];
        assert_eq!(shader_property_default(&params, "amount"), Some(0.4));
        assert_eq!(shader_property_default(&params, "tint.r"), Some(1.0));
        assert_eq!(shader_property_default(&params, "tint.b"), Some(0.2));
        assert_eq!(shader_property_default(&params, "tint"), None);
        assert_eq!(shader_property_default(&params, "opacity"), Some(1.0));
        assert_eq!(shader_property_default(&params, "missing"), None);
    }

    #[test]
    fn validation_rejects_bad_names_overflow_and_missing_entry() {
        let ok = [ShaderParam::scalar("speed", 1.0, [0.0, 4.0])];
        validate_shader("fx", [400.0, 300.0], 12.0, SHADE, &ok).unwrap();
        let bad =
            |params: &[ShaderParam]| validate_shader("fx", [400.0, 300.0], 0.0, SHADE, params);
        assert!(bad(&[ShaderParam::scalar("Speed", 1.0, [0.0, 1.0])]).is_err());
        assert!(bad(&[ShaderParam::scalar("scale-x", 1.0, [0.0, 1.0])]).is_err());
        assert!(bad(&[ShaderParam::scalar("opacity", 1.0, [0.0, 1.0])]).is_err());
        assert!(bad(&[ShaderParam::scalar("a", 1.0, [1.0, 1.0])]).is_err());
        assert!(
            bad(&[
                ShaderParam::scalar("a", 1.0, [0.0, 1.0]),
                ShaderParam::scalar("a", 1.0, [0.0, 1.0]),
            ])
            .is_err()
        );
        let many = (0..11)
            .map(|i| ShaderParam::color(&format!("c{i}"), [0, 0, 0]))
            .collect::<Vec<_>>();
        assert!(bad(&many).is_err(), "33 floats overflow the block");
        assert!(validate_shader("fx", [400.0, 300.0], 0.0, "fn main() {}", &ok).is_err());
        assert!(validate_shader("fx", [400.0, 300.0], 300.0, SHADE, &ok).is_err());
    }

    #[test]
    fn params_round_trip_through_json() {
        let params = vec![
            ShaderParam::scalar("amount", 0.4, [0.0, 2.0]),
            ShaderParam::color("tint", [10, 20, 30]),
        ];
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json[1]["type"], "color");
        let back: Vec<ShaderParam> = serde_json::from_value(json).unwrap();
        assert_eq!(back, params);
    }
}
