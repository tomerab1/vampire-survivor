// Stylized procedural fire for pixel art. Motion comes from the usual 2D-fire recipe:
//   1. domain-warped fBm (two fBm samples displace a third), scrolled upward, so flames curl and lick;
//   2. a shape mask (teardrop / orb / ground tongues) eroded by that noise, swaying more near the tip;
//   3. a blackbody-style temperature ramp: dark red -> orange -> yellow -> white core;
//   4. flicker, with a per-instance seed from world position so shared-material flames never sync up;
// then it's stylized to match the sprites: UVs snap to chunky pixels and the color is posterized into bands.
// params: x = rise speed, y = noise scale, z = shape (0 teardrop, 1 ground patch, 2 orb), w = seed
#import bevy_sprite::{mesh2d_vertex_output::VertexOutput, mesh2d_view_bindings::globals}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    return fract((q.x + 45.32) * (q.y + 45.32) * 7.13);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    // Rotate between octaves to hide grid artifacts.
    let rot = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 5; i = i + 1) {
        v = v + noise(q) * amp;
        q = rot * q * 2.02 + vec2<f32>(1.7, 9.2);
        amp = amp * 0.5;
    }
    return v;
}

// Approximate blackbody glow for a normalized temperature in [0, 1].
fn blackbody(t: f32) -> vec3<f32> {
    let r = smoothstep(0.0, 0.3, t);
    let g = smoothstep(0.2, 0.7, t) * 0.9;
    let b = smoothstep(0.78, 1.0, t) * 0.7;
    return vec3<f32>(r, g, b) + vec3<f32>(0.25, 0.02, 0.0) * (1.0 - r);
}

const PIXELS: f32 = 22.0;
const BANDS: f32 = 5.0;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let seed = in.world_position.xy * 0.011 + vec2<f32>(params.w, params.w * 1.7);
    let t = globals.time * params.x;
    let uv = floor(in.uv * PIXELS) / PIXELS;
    let h = 1.0 - uv.y;             // 0 at the base, 1 at the tip
    let x = uv.x - 0.5;

    // 1. Domain-warped fBm, scrolling upward.
    let p = vec2<f32>(x * 2.2, h * 1.4) * params.y + seed;
    let q = vec2<f32>(
        fbm(p + vec2<f32>(0.0, -t)),
        fbm(p + vec2<f32>(5.2, 1.3 - t * 1.3)),
    );
    let n = fbm(p + (q - 0.5) * 1.8 + vec2<f32>(0.0, -t * 1.6));

    // 2. Shape mask, eroded by the noise.
    var heat = 0.0;
    if (params.z < 0.5) {
        // Teardrop: wide at the base, narrowing and swaying toward the tip.
        let sway = (q.x - 0.5) * 0.35 * h;
        let width = mix(0.36, 0.03, pow(h, 0.9));
        let body = 1.0 - smoothstep(width * 0.4, width, abs(x + sway));
        let base = smoothstep(0.0, 0.1, h);
        heat = body * base * (1.15 - h) - n * 0.55 * (0.3 + h);
    } else if (params.z < 1.5) {
        // Ground patch: a low bed of embers with irregular tongues licking up.
        let tongues = 0.3 + 0.7 * fbm(vec2<f32>(x * 7.0, -t * 0.8) + seed);
        let sides = 1.0 - smoothstep(0.22, 0.48, abs(x));
        let bottom = smoothstep(0.0, 0.2, h);
        heat = sides * bottom * (tongues - h) * 1.2 - n * 0.3;
    } else {
        // Orb: a ball of fire (meteors) with a turbulent rim.
        let d = length(vec2<f32>(x, h - 0.5)) * 2.0;
        heat = (1.0 - d) * 1.3 - n * 0.6;
    }

    // 3 + 4. Temperature ramp with flicker.
    let flicker = 0.85 + 0.3 * noise(vec2<f32>(t * 3.0, seed.x));
    let temp = clamp(heat * 1.35 * flicker, 0.0, 1.0);
    let banded = ceil(temp * BANDS) / BANDS;
    let rgb = blackbody(banded) * color.rgb * 1.1;
    let alpha = step(0.08, temp) * color.a;
    return vec4<f32>(rgb, alpha);
}
