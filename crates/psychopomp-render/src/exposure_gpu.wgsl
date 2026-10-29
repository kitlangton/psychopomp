struct ShutterUniform {
    weight: f32,
    opacity: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> u: ShutterUniform;

@vertex
fn vs(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    let pts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pts[idx], 0.0, 1.0);
}

@fragment
fn fs_add(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(pos.xy), 0);
    return c * u.weight;
}

@fragment
fn fs_resolve_opaque(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(pos.xy), 0);
    return vec4<f32>(c.rgb * u.opacity, c.a * u.opacity);
}

@fragment
fn fs_resolve_alpha(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(pos.xy), 0);
    if c.a > 1.0e-5 {
        return vec4<f32>((c.rgb / c.a) * u.opacity, clamp(c.a * u.opacity, 0.0, 1.0));
    }
    return vec4<f32>(0.0);
}

@fragment
fn fs_composite_over(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(pos.xy), 0);
    return c * u.opacity;
}
