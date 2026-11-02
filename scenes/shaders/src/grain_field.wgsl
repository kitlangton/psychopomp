// A drifting gradient field with film grain. `fx_fbm3` and `fx_hash3` come
// from the Stage's noise module; `ShaderIn` and `Params` are generated.
fn shade(s: ShaderIn, p: Params) -> vec4<f32> {
    let t = s.time * p.speed;
    // Domain-warped fBM in world pixels, so the field keeps its grain as the
    // layer scales.
    let q = s.local / 420.0;
    let w = vec2<f32>(
        fx_fbm3(vec3<f32>(q * 1.3, t)),
        fx_fbm3(vec3<f32>(q * 1.3 + vec2<f32>(5.2, 1.3), t + 3.7)),
    );
    let f = fx_fbm3(vec3<f32>(q + w * p.warp, t * 0.7));
    let ramp = smoothstep(0.25, 0.75, f);
    var color = mix(p.deep, p.glow, ramp);
    // A cool haze rises toward the top edge, a little brighter than white
    // where it peaks so it blooms.
    color = mix(color, p.haze * 1.4, smoothstep(0.55, 0.95, f) * (1.0 - s.uv.y) * 0.8);
    // Grain per frame pixel per sample, centred on zero.
    let n = fx_hash3(vec3<f32>(floor(s.px), floor(s.time * 24.0))) - 0.5;
    color = max(color * (1.0 + n * p.grain * 2.0), vec3<f32>(0.0));
    return vec4<f32>(color, 1.0);
}
