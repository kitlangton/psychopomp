//! Custom WGSL `Shader` layers. Each layer compiles once into its own
//! pipeline: the Stage primitive shader (its vertex stage, matte, and
//! helpers such as `fx_noise3`), a generated `Params` struct, the user's
//! module, and a fragment entry that calls `shade` inside the layer's rounded
//! quad. The layer's primitive carries its placement, sample time, and packed
//! parameters, so it is sorted, warped, and clipped like any other.
use super::stage::{PRIMITIVE_SHADER, compose};
use anyhow::{Context, Result, anyhow, ensure};
use psychopomp::stage::{ShaderParam, shader::SHADER_PARAM_FLOATS};
use wgpu::naga::{
    front::wgsl,
    valid::{Capabilities, ValidationFlags, Validator},
};

/// A shader layer's primitive kind is this plus its pipeline slot; the
/// ordinary primitive shader draws nothing for it.
pub(crate) const SHADER_KIND: f32 = 100.0;

/// The pipeline slot of a primitive of kind `kind`, if it is a shader layer.
pub(crate) fn slot(kind: f32) -> Option<usize> {
    (kind >= SHADER_KIND - 0.5).then(|| (kind - SHADER_KIND).round() as usize)
}

/// The `Prim` fields that carry a layer's packed parameters, four floats each.
const PARAM_FIELDS: [&str; SHADER_PARAM_FLOATS / 4] = [
    "fill",
    "stroke",
    "glow",
    "uv",
    "light",
    "light_color",
    "pool",
    "pool_color",
];

const PRELUDE: &str = r#"
// What `shade` sees of one pixel of its layer.
struct ShaderIn {
    uv: vec2<f32>,    // 0..1 across the layer, y down
    local: vec2<f32>, // world pixels from the layer's center, before it turns
    size: vec2<f32>,  // the layer's size in world pixels
    px: vec2<f32>,    // the frame pixel
    time: f32,        // seconds: this shutter sample's time
    scale: f32,       // frame pixels per world pixel
};
"#;

// a = (kind, cx, cy, turn), b = (half w, half h, corner, blur),
// grad_a = (time, scale, opacity, 0); parameters in `PARAM_FIELDS`.
const ENTRY: &str = r#"
@fragment
fn fs_shader(vin: VOut) -> @location(0) vec4<f32> {
    let prim = prims[vin.index];
    let clip_cov = eval_clip(prim, vin.px);
    if clip_cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    let offset = vin.src - prim.a.yz;
    let c = cos(prim.a.w);
    let s = sin(prim.a.w);
    let local = vec2<f32>(offset.x * c + offset.y * s, -offset.x * s + offset.y * c);
    let half = max(prim.b.xy, vec2<f32>(0.5));
    let cover = coverage(sd_round_box(local, half, prim.b.z), prim.b.w);
    if cover <= 0.0 {
        return vec4<f32>(0.0);
    }
    let scale = max(prim.grad_a.y, 1.0e-4);
    var input: ShaderIn;
    input.uv = local / (2.0 * half) + vec2<f32>(0.5);
    input.local = local / scale;
    input.size = 2.0 * half / scale;
    input.px = vin.px;
    input.time = prim.grad_a.x;
    input.scale = scale;
    let color = shade(input, shader_params(prim));
    let alpha = clamp(color.a, 0.0, 1.0) * cover * prim.grad_a.z * clip_cov;
    return vec4<f32>(max(color.rgb, vec3<f32>(0.0)) * alpha, alpha);
}
"#;

fn field(index: usize) -> String {
    format!(
        "prim.{}.{}",
        PARAM_FIELDS[index / 4],
        ["x", "y", "z", "w"][index % 4]
    )
}

/// The `Params` struct and the function that unpacks it from a primitive.
fn params_wgsl(params: &[ShaderParam]) -> String {
    let mut fields = String::new();
    let mut unpack = String::new();
    let mut at = 0;
    for param in params {
        let name = param.name();
        match param {
            ShaderParam::Scalar { .. } => {
                fields.push_str(&format!("    {name}: f32,\n"));
                unpack.push_str(&format!("    p.{name} = {};\n", field(at)));
            }
            ShaderParam::Color { .. } => {
                fields.push_str(&format!("    {name}: vec3<f32>,\n"));
                unpack.push_str(&format!(
                    "    p.{name} = vec3<f32>({}, {}, {});\n",
                    field(at),
                    field(at + 1),
                    field(at + 2)
                ));
            }
        }
        at += param.floats();
    }
    if params.is_empty() {
        fields.push_str("    unused: f32,\n");
    }
    format!(
        "struct Params {{\n{fields}}};\n\nfn shader_params(prim: Prim) -> Params {{\n    var p: Params;\n{unpack}    return p;\n}}\n"
    )
}

/// The whole WGSL of one layer's pipeline around `primitive` (the composed
/// Stage primitive shader).
pub(crate) fn layer_source(primitive: &str, wgsl: &str, params: &[ShaderParam]) -> String {
    format!(
        "{primitive}\n{PRELUDE}\n{}\n// ---- user module ----\n{wgsl}\n// ---- end user module ----\n{ENTRY}",
        params_wgsl(params)
    )
}

/// Parse and validate a layer's WGSL without a GPU, so a typo fails preflight
/// with the user module's line instead of the first render.
pub(crate) fn validate_layer(id: &str, wgsl: &str, params: &[ShaderParam]) -> Result<()> {
    let source = layer_source(&compose(&PRIMITIVE_SHADER), wgsl, params);
    let module = wgsl::parse_str(&source)
        .map_err(|err| anyhow!("{}", err.emit_to_string_with_path(&source, id)))
        .with_context(|| format!("stage shader '{id}' WGSL does not parse"))?;
    Validator::new(ValidationFlags::all(), Capabilities::default())
        .validate(&module)
        .map_err(|err| anyhow!("{}", err.emit_to_string(&source)))
        .with_context(|| format!("stage shader '{id}' WGSL does not validate"))?;
    let shade = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("shade"))
        .map(|(_, f)| f);
    ensure!(
        shade.is_some_and(|f| f.arguments.len() == 2 && f.result.is_some()),
        "stage shader '{id}' needs `fn shade(s: ShaderIn, p: Params) -> vec4<f32>`"
    );
    Ok(())
}

/// The packed parameter fields of a layer's primitive, from each parameter
/// channel's sampled value (`value(property)`, colors as sRGB components),
/// colors converted to linear light.
pub(crate) fn pack_params(
    params: &[ShaderParam],
    value: impl Fn(&str) -> f32,
) -> [[f32; 4]; SHADER_PARAM_FLOATS / 4] {
    let mut floats = [0.0_f32; SHADER_PARAM_FLOATS];
    let mut at = 0;
    for param in params {
        for (property, ..) in param.channels() {
            let v = value(&property);
            floats[at] = match param {
                ShaderParam::Scalar { .. } => v,
                ShaderParam::Color { .. } => srgb_to_linear(v.clamp(0.0, 1.0)),
            };
            at += 1;
        }
    }
    let mut out = [[0.0; 4]; SHADER_PARAM_FLOATS / 4];
    for (i, v) in floats.into_iter().enumerate() {
        out[i / 4][i % 4] = v;
    }
    out
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAIN: &str = include_str!("../../../../scenes/shaders/src/grain_field.wgsl");

    fn grain_params() -> Vec<ShaderParam> {
        vec![
            ShaderParam::scalar("speed", 0.2, [0.0, 2.0]),
            ShaderParam::scalar("grain", 0.12, [0.0, 0.6]),
            ShaderParam::scalar("warp", 0.8, [0.0, 3.0]),
            ShaderParam::color("deep", [16, 24, 64]),
            ShaderParam::color("glow", [255, 120, 60]),
            ShaderParam::color("haze", [120, 210, 255]),
        ]
    }

    #[test]
    fn example_layer_validates_without_a_gpu() {
        validate_layer("field", GRAIN, &grain_params()).unwrap();
    }

    #[test]
    fn a_layer_with_no_params_still_has_a_params_struct() {
        let wgsl =
            "fn shade(s: ShaderIn, p: Params) -> vec4<f32> { return vec4<f32>(s.uv, 0.0, 1.0); }";
        validate_layer("plain", wgsl, &[]).unwrap();
    }

    #[test]
    fn errors_name_the_layer_and_the_wgsl_problem() {
        let wgsl = "fn shade(s: ShaderIn, p: Params) -> vec4<f32> { return p.missing; }";
        let params = [ShaderParam::scalar("amount", 1.0, [0.0, 1.0])];
        let err = format!("{:#}", validate_layer("broken", wgsl, &params).unwrap_err());
        assert!(err.contains("broken"), "{err}");
        assert!(err.contains("missing"), "{err}");
        let wrong = "fn shade(s: ShaderIn) -> vec4<f32> { return vec4<f32>(1.0); }";
        assert!(validate_layer("arity", wrong, &[]).is_err());
    }

    #[test]
    fn params_pack_in_declaration_order_with_linear_colors() {
        let params = [
            ShaderParam::scalar("a", 0.0, [0.0, 1.0]),
            ShaderParam::color("c", [0, 0, 0]),
            ShaderParam::scalar("b", 0.0, [0.0, 1.0]),
        ];
        let packed = pack_params(&params, |property| match property {
            "a" => 2.0,
            "c.r" => 1.0,
            "c.g" => 0.5,
            "c.b" => 0.0,
            "b" => -3.0,
            _ => panic!("{property}"),
        });
        assert_eq!(packed[0][0], 2.0);
        assert_eq!(packed[0][1], 1.0);
        assert!((packed[0][2] - 0.214).abs() < 1e-3, "{}", packed[0][2]);
        assert_eq!(packed[0][3], 0.0);
        assert_eq!(packed[1][0], -3.0);
        assert!(params_wgsl(&params).contains("p.b = prim.stroke.x;"));
    }
}
