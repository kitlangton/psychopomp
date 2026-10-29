// Requires noise.wgsl. No bindings: local pixels, projected radius, age ->
// premultiplied HDR emission and absorption coverage. Compose in any pixel recipe.
// Deterministic procedural combustion, not a frame-integrated fluid simulation.
// Domain-warped fBM shapes a rising volume; front-to-back Beer-Lambert
// absorption carries hot emission through cool smoke (GPU Gems 3, chapter 30).
fn combustion_volume(pixel: vec2<f32>, radius: f32, age: f32) -> vec4<f32> {
    let t = max(age - 0.12, 0.0);
    let ignite = smoothstep(0.0, 0.065, t);
    let fade = 1.0 - smoothstep(2.6, 5.2, age);
    if ignite * fade < 0.001 { return vec4<f32>(0.0); }
    let growth = 0.32 + 1.48 * (1.0 - exp(-t * 3.5)) + 0.10 * t;
    let lift = vec2<f32>(0.06 * t, -0.28 * t);
    let xy = pixel / max(radius, 1.0) - lift;
    if length(xy / vec2<f32>(1.12, 1.0)) > growth * 1.3 { return vec4<f32>(0.0); }
    let step_size = growth * 2.5 / 28.0;
    var transmittance = 1.0;
    var light = vec3<f32>(0.0);
    let cooling = exp(-t * 1.7);
    // Fixed quadrature avoids temporal noise crawling through the smoke.
    for (var sample = 0; sample < 28; sample++) {
        let z = -growth * 1.25 + (f32(sample) + 0.5) * step_size;
        let p = vec3<f32>(xy, z) / growth;
        let distance = length(p * vec3<f32>(0.92, 1.0, 1.0));
        // Compact support reaches zero before the ray interval and screen-space
        // rejection bounds. Otherwise positive density is cut into flat lobes.
        let support = 1.0 - smoothstep(1.03, 1.22, distance);
        if support <= 0.0 { continue; }
        let advected = p * 3.2 + vec3<f32>(0.0, t * 0.75, t * 0.16);
        let warp = vec3<f32>(fx_noise3(advected + 9.0), fx_noise3(advected.yzx + 23.0), fx_noise3(advected.zxy + 41.0)) - 0.5;
        let turbulence = fx_fbm3(advected + warp * 1.8);
        let lobes = fx_noise3(p * 2.6 + vec3<f32>(4.0, 8.0, 1.0));
        let boundary = 0.67 + 0.60 * lobes;
        let density = max(boundary - distance + (turbulence - 0.48) * 1.05, 0.0) * support;
        if density <= 0.0 { continue; }
        let absorb = (1.0 - exp(-density * step_size * 8.0)) * ignite * fade;
        // Cooler opaque folds interrupt the hot interior instead of making an
        // evenly emissive orange ball. Fine noise supplies glowing fissures.
        let heat = cooling * (0.15 + 1.7 * smoothstep(0.36, 0.68, turbulence))
            * (1.0 - smoothstep(0.25, 1.05, length(p)) * 0.72);
        let ember = mix(vec3<f32>(0.24, 0.008, 0.001), vec3<f32>(3.6, 0.38, 0.015), smoothstep(0.10, 0.52, heat));
        let fire = mix(ember, vec3<f32>(7.0, 3.2, 0.8), smoothstep(0.6, 1.1, heat));
        let smoke_key = 0.012 + 0.12 * pow(1.0 - turbulence, 3.0) + 0.026 * max(-p.y, 0.0);
        let smoke = vec3<f32>(0.65, 0.72, 0.83) * smoke_key;
        let emission = fire * smoothstep(0.035, 0.22, heat);
        light += transmittance * absorb * (smoke + emission);
        transmittance *= 1.0 - absorb;
        if transmittance < 0.015 { break; }
    }
    // A very brief ignition core; the turbulent surface remains visible after it.
    let flash = exp(-t * 19.0) * ignite * exp(-dot(xy, xy) * 5.0);
    light += vec3<f32>(7.0, 3.0, 0.9) * flash;
    return vec4<f32>(light, 1.0 - transmittance);
}
