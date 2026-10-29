// Stage primitives: instanced quads whose fragments evaluate a signed distance.
// Output is premultiplied HDR color; glow adds light without occluding (alpha 0),
// so bloom later turns bright emissive color into soft halos.
//
// Live editing: set PSYCHOPOMP_SHADER_DIR to this directory and re-render; the
// renderer reads the file at startup instead of the compiled-in copy.

struct Globals {
    viewport: vec4<f32>, // width, height, atlas width, atlas height
};

struct Prim {
    bbox: vec4<f32>,   // screen rectangle to rasterize: x0, y0, x1, y1
    a: vec4<f32>,      // kind, then kind-specific
    b: vec4<f32>,      // kind-specific
    fill: vec4<f32>,   // straight linear RGBA
    stroke: vec4<f32>, // straight linear RGBA
    glow: vec4<f32>,   // linear RGB intensity, radius in pixels
    uv: vec4<f32>,     // text atlas rectangle, or polyline point range
    light: vec4<f32>,  // rounded rects: a reflection's x, y, radius, strength
    light_color: vec4<f32>,
    pool: vec4<f32>,   // rounded rects: a pool of light in the glass, same form
    pool_color: vec4<f32>,
    mask: vec4<f32>,       // dissolve: age (0 for none), card center x, y, scale
    mask_shape: vec4<f32>, // dissolve: card half size in world pixels, field seed, 0
    mask_color: vec4<f32>, // dissolve: rim tone, linear RGB
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> prims: array<Prim>;
@group(0) @binding(2) var<storage, read> points: array<vec4<f32>>;
@group(0) @binding(3) var atlas: texture_2d<f32>;
@group(0) @binding(4) var atlas_sampler: sampler;
// Every footage element's current frame: straight-alpha sRGB, read linear.
@group(0) @binding(5) var footage: texture_2d<f32>;

struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) px: vec2<f32>,
    @location(1) @interpolate(flat) index: u32,
};

@vertex
fn vs(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VOut {
    let prim = prims[instance];
    let corner = vec2<f32>(f32(vertex & 1u), f32((vertex >> 1u) & 1u));
    let px = mix(prim.bbox.xy, prim.bbox.zw, corner);
    var out: VOut;
    out.position = vec4<f32>(
        px.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - px.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
    out.px = px;
    out.index = instance;
    return out;
}

fn sd_round_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - half_size + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Coverage of the inside of a distance field, softened by blur for defocus.
fn coverage(d: f32, blur: f32) -> f32 {
    let width = 0.85 + blur;
    return clamp(0.5 - d / width, 0.0, 1.0);
}

// Light that falls off with distance outside a shape.
fn halo(d: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    let outside = max(d, 0.0);
    return exp(-outside / radius) * exp(-outside / (radius * 3.0)) * 0.9;
}

// The diagrams' reflection falloff: full at the light, 0.65 at 0.3 of its
// radius, 0.16 at 0.7, gone at the radius (REFLECTION in stage.rs).
fn reflection(r: f32) -> f32 {
    if r < 0.3 {
        return mix(1.0, 0.65, r / 0.3);
    }
    if r < 0.7 {
        return mix(0.65, 0.16, (r - 0.3) / 0.4);
    }
    return max(mix(0.16, 0.0, (r - 0.7) / 0.3), 0.0);
}

// Light from things that move. A reflection lights only the border near it;
// a pool also enters the glass, and the border catches some of it.
fn card_light(prim: Prim, px: vec2<f32>, band: f32, inside: f32) -> vec3<f32> {
    var light = vec3<f32>(0.0);
    if prim.light.w > 0.0 {
        let r = length(px - prim.light.xy) / max(prim.light.z, 1.0);
        light += prim.light_color.rgb * prim.light.w * reflection(r) * band * 0.6;
    }
    if prim.pool.w > 0.0 {
        let r = length(px - prim.pool.xy) / max(prim.pool.z, 1.0);
        let field = prim.pool.w * exp(-2.0 * r * r);
        light += prim.pool_color.rgb * field * (inside * 0.12 + band * 0.4);
    }
    return light;
}

fn direction(angle: f32) -> vec2<f32> {
    return vec2<f32>(cos(angle), sin(angle));
}

// Nearest point of the first `drawn` pixels of a polyline: (distance, length
// along it, interpolated point energy). Points are (x, y, length so far, energy).
fn path_nearest(px: vec2<f32>, first: u32, count: u32, drawn: f32, reach: f32) -> vec3<f32> {
    var best = reach;
    var along = 0.0;
    var energy = 0.0;
    for (var k = 0u; k + 1u < count; k = k + 1u) {
        let a = points[first + k];
        if a.z >= drawn {
            break;
        }
        let b = points[first + k + 1u];
        if px.x < min(a.x, b.x) - best || px.x > max(a.x, b.x) + best ||
           px.y < min(a.y, b.y) - best || px.y > max(a.y, b.y) + best {
            continue;
        }
        let t_end = clamp((drawn - a.z) / max(b.z - a.z, 1.0e-4), 0.0, 1.0);
        let end = mix(a.xy, b.xy, t_end);
        let pa = px - a.xy;
        let ba = end - a.xy;
        let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1.0e-4), 0.0, 1.0);
        let distance = length(pa - ba * h);
        if distance < best {
            best = distance;
            along = mix(a.z, b.z, h * t_end);
            energy = mix(a.w, b.w, h * t_end);
        }
    }
    return vec3<f32>(best, along, energy);
}

// Signed distance to a closed polygon of `count` points from `first`:
// negative inside, by counting edge crossings (Quilez's sdPolygon).
fn sd_polygon(p: vec2<f32>, first: u32, count: u32) -> f32 {
    var d = 1.0e12;
    var s = 1.0;
    var j = count - 1u;
    for (var i = 0u; i < count; i = i + 1u) {
        let vi = points[first + i].xy;
        let vj = points[first + j].xy;
        let e = vj - vi;
        let w = p - vi;
        let b = w - e * clamp(dot(w, e) / max(dot(e, e), 1.0e-6), 0.0, 1.0);
        d = min(d, dot(b, b));
        let c = vec3<bool>((p.y >= vi.y), (p.y < vj.y), (e.x * w.y > e.y * w.x));
        if all(c) || !any(c) {
            s = -s;
        }
        j = i;
    }
    return s * sqrt(d);
}

// One footage texel as premultiplied linear light, held inside its slot
// (x0, y0, x1, y1) so neighbors in the atlas never bleed in.
fn footage_texel(p: vec2<i32>, slot: vec4<f32>) -> vec4<f32> {
    let q = clamp(p, vec2<i32>(slot.xy), vec2<i32>(slot.zw) - vec2<i32>(1));
    let t = textureLoad(footage, q, 0);
    return vec4<f32>(t.rgb * t.a, t.a);
}

// Bilinear in premultiplied light, so transparent texels add no fringe.
fn footage_bilinear(at: vec2<f32>, slot: vec4<f32>) -> vec4<f32> {
    let p = at - vec2<f32>(0.5);
    let base = floor(p);
    let f = p - base;
    let i = vec2<i32>(base);
    let a = footage_texel(i, slot);
    let b = footage_texel(i + vec2<i32>(1, 0), slot);
    let c = footage_texel(i + vec2<i32>(0, 1), slot);
    let d = footage_texel(i + vec2<i32>(1, 1), slot);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Footage spread over a disc `radius` texels across: 16 golden-angle taps,
// round where a box shows copies. It defocuses and it antialiases
// minification alike.
fn footage_sample(at: vec2<f32>, slot: vec4<f32>, radius: f32) -> vec4<f32> {
    if radius < 0.35 {
        return footage_bilinear(at, slot);
    }
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < 16; k = k + 1) {
        let r = sqrt((f32(k) + 0.5) / 16.0) * radius;
        let angle = f32(k) * 2.3999632;
        sum = sum + footage_bilinear(at + vec2<f32>(cos(angle), sin(angle)) * r, slot);
    }
    return sum / 16.0;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let prim = prims[in.index];
    let kind = u32(prim.a.x + 0.5);
    let px = in.px;
    var color = vec3<f32>(0.0);
    var alpha = 0.0;

    switch kind {
        // Rounded rectangle: a = (kind, cx, cy, corner), b = (half w, half h, border, blur)
        case 0u: {
            let d = sd_round_box(px - prim.a.yz, prim.b.xy, prim.a.w);
            let outer = coverage(d, prim.b.w);
            let inner = coverage(d + prim.b.z, prim.b.w);
            let fill_a = prim.fill.a * inner;
            let stroke_a = prim.stroke.a * max(outer - inner, 0.0);
            alpha = fill_a + stroke_a * (1.0 - fill_a);
            // A restrained overhead key gives the panel thickness without an
            // emissive outline. Socket light is added separately below.
            let height = clamp((px.y - prim.a.z) / max(prim.b.y, 1.0), -1.0, 1.0);
            let key = mix(1.08, 0.92, height * 0.5 + 0.5);
            let rim = mix(1.2, 0.7, height * 0.5 + 0.5);
            color = prim.fill.rgb * fill_a * key + prim.stroke.rgb * stroke_a * (1.0 - fill_a) * rim;
            color += prim.glow.rgb * halo(d, prim.glow.w) * smoothstep(-1.0, 1.5, d);
            color += card_light(prim, px, max(outer - inner, 0.0), inner);
        }
        // Circle: a = (kind, cx, cy, radius), b = (border, 0, 0, blur)
        case 1u: {
            let d = length(px - prim.a.yz) - prim.a.w;
            let outer = coverage(d, prim.b.w);
            let inner = coverage(d + prim.b.x, prim.b.w);
            let fill_a = prim.fill.a * inner;
            let stroke_a = prim.stroke.a * max(outer - inner, 0.0);
            alpha = fill_a + stroke_a * (1.0 - fill_a);
            color = prim.fill.rgb * fill_a + prim.stroke.rgb * stroke_a * (1.0 - fill_a);
            color += prim.glow.rgb * halo(d, prim.glow.w);
        }
        // Arc: a = (kind, cx, cy, radius), b = (thickness, start angle, sweep, blur)
        case 2u: {
            let q = px - prim.a.yz;
            let radius = prim.a.w;
            let half_thickness = prim.b.x * 0.5;
            let sweep = prim.b.z;
            var d = abs(length(q) - radius) - half_thickness;
            if sweep < 6.2831 {
                var angle = atan2(q.y, q.x) - prim.b.y;
                angle = angle - floor(angle / 6.2831853) * 6.2831853;
                if angle > sweep {
                    let start_cap = length(q - direction(prim.b.y) * radius);
                    let end_cap = length(q - direction(prim.b.y + sweep) * radius);
                    d = min(start_cap, end_cap) - half_thickness;
                }
            }
            alpha = prim.stroke.a * coverage(d, prim.b.w);
            color = prim.stroke.rgb * alpha + prim.glow.rgb * halo(d, prim.glow.w);
        }
        // Polyline: a = (kind, width, drawn length, blur), b = (dash, gap, phase, fade)
        // uv = (first point, point count). Points are (x, y, length so far, heat).
        case 3u: {
            let first = u32(prim.uv.x + 0.5);
            let count = u32(prim.uv.y + 0.5);
            let drawn = prim.a.z;
            var best = 1.0e9;
            var along = 0.0;
            var heat = 1.0;
            for (var k = 0u; k + 1u < count; k = k + 1u) {
                let a = points[first + k];
                if a.z >= drawn {
                    break;
                }
                let b = points[first + k + 1u];
                if px.x < min(a.x, b.x) - best || px.x > max(a.x, b.x) + best ||
                   px.y < min(a.y, b.y) - best || px.y > max(a.y, b.y) + best {
                    continue;
                }
                var end = b.xy;
                var end_length = b.z;
                if b.z > drawn {
                    let t = (drawn - a.z) / max(b.z - a.z, 1.0e-4);
                    end = mix(a.xy, b.xy, t);
                    end_length = drawn;
                }
                let pa = px - a.xy;
                let ba = end - a.xy;
                let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1.0e-4), 0.0, 1.0);
                let distance = length(pa - ba * h);
                if distance < best {
                    best = distance;
                    along = mix(a.z, end_length, h);
                    // Points carry a heat (1 for ordinary lines): a cooling trail.
                    heat = mix(a.w, b.w, h * (end_length - a.z) / max(b.z - a.z, 1.0e-4));
                }
            }
            var d = best - prim.a.y * 0.5;
            let dash = prim.b.x;
            if dash > 0.0 {
                // Each dash is a capsule: measure along the path to the nearest dash
                // too, so its glow is round rather than a bar across the line.
                let period = dash + prim.b.y;
                let m = (along + prim.b.z) - floor((along + prim.b.z) / period) * period;
                let gap = select(0.0, min(m - dash, period - m), m > dash);
                d = length(vec2<f32>(best, gap)) - prim.a.y * 0.5;
            }
            // Fade toward the start: a comet trail brightest at its head.
            let strength = mix(1.0, clamp(along / max(drawn, 1.0), 0.0, 1.0), prim.b.w) * heat;
            alpha = prim.stroke.a * coverage(d, prim.a.w) * strength;
            color = prim.stroke.rgb * alpha + prim.glow.rgb * halo(d, prim.glow.w) * strength;
            // A shape's outline catches a passing packet's reflection.
            if prim.light.w > 0.0 {
                let r = length(px - prim.light.xy) / max(prim.light.z, 1.0);
                color += prim.light_color.rgb * prim.light.w * reflection(r) * coverage(d, prim.a.w) * strength * 0.6;
            }
        }
        // Text: a = (kind, left, top, blur), b = (width, height, revealed width, 0)
        // uv = atlas rectangle in texels. light = (shimmer phase, strength, 0, 0).
        case 4u: {
            let local = (px - prim.a.yz) / prim.b.xy;
            if all(local >= vec2<f32>(0.0)) && all(local <= vec2<f32>(1.0)) && px.x <= prim.a.y + prim.b.z {
                let uv = mix(prim.uv.xy, prim.uv.zw, local) / globals.viewport.zw;
                var cov = textureSampleLevel(atlas, atlas_sampler, uv, 0.0).r;
                let blur = prim.a.w;
                if blur > 0.25 {
                    // A 3x3 tent (4-2-1 weights): round where a cross goes boxy.
                    let step = vec2<f32>(blur, blur) / globals.viewport.zw * 1.5;
                    var sum = cov * 4.0;
                    for (var i = 0; i < 8; i = i + 1) {
                        let angle = f32(i) * 0.7853982;
                        let offset = vec2<f32>(round(cos(angle)), round(sin(angle)));
                        let weight = select(1.0, 2.0, abs(offset.x) + abs(offset.y) < 1.5);
                        sum = sum + weight * textureSampleLevel(atlas, atlas_sampler, uv + offset * step, 0.0).r;
                    }
                    cov = sum / 16.0;
                }
                alpha = prim.fill.a * cov;
                color = prim.fill.rgb * alpha;
                if prim.light.y > 0.0 {
                    // The blog's "broad" ShimmerText: the ink dims by 40% and a
                    // raised-cosine sheen 0.84 text widths wide sweeps left to
                    // right once per phase unit.
                    let strength = prim.light.y;
                    let band = (fract(local.x - prim.light.x) - 0.5) / 0.84 + 0.5;
                    let sheen = select(0.0, 0.5 - 0.5 * cos(band * 6.2831853), band > 0.0 && band < 1.0);
                    let base = 1.0 - 0.4 * strength;
                    let lit = 0.85 * strength * sheen;
                    color = alpha * (prim.fill.rgb * base * (1.0 - lit) + vec3<f32>(0.8) * lit);
                    alpha = alpha * (base + lit * (1.0 - base));
                }
            }
        }
        // Backdrop: a radial gradient from fill (center) to stroke (edge).
        // a = (kind, cx, cy, radius)
        case 5u: {
            let t = clamp(length(px - prim.a.yz) / prim.a.w, 0.0, 1.0);
            color = mix(prim.fill.rgb, prim.stroke.rgb, smoothstep(0.0, 1.0, t));
            alpha = 1.0;
        }
        // Raymarched fire and smoke: a = (kind, cx, cy, radius), b = (age, opacity, 0, 0)
        case 6u: {
            return combustion_volume(px - prim.a.yz, prim.a.w, prim.b.x) * prim.b.y;
        }
        // Polygon: a = (kind, border, 0, blur), uv = (first point, count).
        // Points are (x, y, 0, 0), any simple polygon; inside by crossings.
        case 7u: {
            let d = sd_polygon(px, u32(prim.uv.x + 0.5), u32(prim.uv.y + 0.5));
            let outer = coverage(d, prim.a.w);
            let inner = coverage(d + prim.a.y, prim.a.w);
            let fill_a = prim.fill.a * select(outer, inner, prim.a.y > 0.0);
            let stroke_a = prim.stroke.a * max(outer - inner, 0.0) * select(0.0, 1.0, prim.a.y > 0.0);
            alpha = fill_a + stroke_a * (1.0 - fill_a);
            color = prim.fill.rgb * fill_a + prim.stroke.rgb * stroke_a * (1.0 - fill_a);
            color += prim.glow.rgb * halo(d, prim.glow.w);
            // An arrival's flood enters the filled glass, as on a card.
            if prim.pool.w > 0.0 {
                let r = length(px - prim.pool.xy) / max(prim.pool.z, 1.0);
                color += prim.pool_color.rgb * prim.pool.w * exp(-2.0 * r * r) * fill_a * 0.12;
            }
        }
        // Lightning channel: a = (kind, core half width, drawn length, taper toward
        // the end), b = (blur, corona radius, reach, 0); fill.rgb = core light,
        // stroke.rgb = corona light; uv = (first point, count). Pure emission.
        case 10u: {
            let perp_d = dot(px - prim.light.xy, prim.light.zw);
            if perp_d < prim.light_color.x || perp_d > prim.light_color.y {
                return vec4<f32>(0.0);
            }
            let hit = path_nearest(px, u32(prim.uv.x + 0.5), u32(prim.uv.y + 0.5), prim.a.z, prim.b.z + 2.0);
            if hit.x >= prim.b.z + 2.0 {
                return vec4<f32>(0.0);
            }
            let taper = 1.0 - prim.a.w * clamp(hit.y / max(prim.a.z, 1.0), 0.0, 1.0);
            let light = lightning_channel(hit.x, prim.a.y * taper, prim.b.x, prim.fill.rgb, prim.stroke.rgb, prim.b.y, prim.b.z);
            return vec4<f32>(light * hit.z, 0.0);
        }
        // Forcefield bubble: a = (kind, cx, cy, radius), b = (up, time, opacity, blur),
        // fill.rgb = tone; light, pool, uv, light_color = four contacts. Pure emission.
        case 11u: {
            let light = shield_bubble((px - prim.a.yz) / max(prim.a.w, 1.0), prim.a.w, prim.b.x, prim.b.y, prim.b.w, prim.fill.rgb, prim.light, prim.pool, prim.uv, prim.light_color);
            return vec4<f32>(light * prim.b.z, 0.0);
        }
        // Scan line: a = (kind, cx, cy, corner), b = (half w, half h, line y offset, heading),
        // fill = tone and strength, stroke = (wake length, line width, blur, 0). Pure emission.
        case 12u: {
            let s = scan_light(px - prim.a.yz, prim.b.xy, prim.a.w, prim.b.z, prim.b.w, prim.stroke.x, prim.stroke.y, prim.stroke.z);
            let light = prim.fill.rgb * (s.x * 1.2 + s.y * 0.07 + s.z * 0.8) + vec3<f32>(0.7) * (s.x * 0.8);
            return vec4<f32>(light * prim.fill.a, 0.0);
        }
        // Footage: a = (kind, cx, cy, corner), b = (half w, half h, blur, turn),
        // uv = the source window in atlas texels (x0, y0, x1, y1), light = the
        // frame's slot, fill = (opacity, saturation, dim, tint), glow = (tint
        // tone, rim width), stroke = rim, light_color = (mask: 0 rounded box,
        // 1 circle, 2 polygon; polygon first point, count, 0).
        case 13u: {
            let offset = px - prim.a.yz;
            let c = cos(prim.b.w);
            let s = sin(prim.b.w);
            let local = vec2<f32>(offset.x * c + offset.y * s, -offset.x * s + offset.y * c);
            let half = prim.b.xy;
            let mask = u32(prim.light_color.x + 0.5);
            var d = sd_round_box(local, half, prim.a.w);
            if mask == 1u {
                d = length(local) - min(half.x, half.y);
            } else if mask == 2u {
                d = sd_polygon(px, u32(prim.light_color.y + 0.5), u32(prim.light_color.z + 0.5));
            }
            let blur = prim.b.z;
            let cover = coverage(d, blur);
            if cover <= 0.0 {
                return vec4<f32>(0.0);
            }
            let window = prim.uv;
            let f = clamp(local / (2.0 * half) + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
            let at = mix(window.xy, window.zw, f);
            // Texels per pixel: minified footage spreads its taps to match.
            let footprint = max((window.z - window.x) / (2.0 * half.x), (window.w - window.y) / (2.0 * half.y));
            var radius = blur * footprint;
            if footprint > 1.2 {
                radius = max(radius, footprint * 0.5);
            }
            let texel = footage_sample(at, prim.light, radius);
            let luma = dot(texel.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
            var rgb = mix(vec3<f32>(luma), texel.rgb, prim.fill.y);
            rgb = mix(rgb, prim.glow.rgb * luma * 1.6, prim.fill.w);
            rgb = rgb * (1.0 - prim.fill.z);
            let opacity = prim.fill.x * cover;
            alpha = texel.a * opacity;
            color = rgb * opacity;
            if prim.glow.w > 0.0 {
                let band = max(cover - coverage(d + prim.glow.w, blur), 0.0) * prim.stroke.a * prim.fill.x;
                color = prim.stroke.rgb * band + color * (1.0 - band);
                alpha = band + alpha * (1.0 - band);
            }
        }
        default: {}
    }
    if prim.mask.x > 0.0 {
        // Dissolve: the material burns away along the card's noise field,
        // scorched just ahead of the front and glowing just behind it.
        let scale = max(prim.mask.w, 0.05);
        let field = dissolve_field((px - prim.mask.yz) / scale, prim.mask_shape.xy, u32(prim.mask_shape.z + 0.5));
        let burn = dissolve_burn(field, prim.mask.x, 0.012 / scale);
        color = color * burn.x * (1.0 - 0.6 * (burn.z - burn.y)) + dissolve_glow(burn.y, prim.mask_color.rgb) * alpha;
        alpha = alpha * burn.x;
    }
    return vec4<f32>(color, alpha);
}
