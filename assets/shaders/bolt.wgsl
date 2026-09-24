// Lightning along a quad stretched between two points (uv.x runs along the bolt, uv.y across it).
// A fractal-displaced main channel plus a thinner branch strand, a white-hot core with an
// exponential blue glow, and a fast strobe so every strike crackles.
// params: x = seed, y = opacity, z = core width (uv units), w = segment count
#import bevy_sprite::{mesh2d_vertex_output::VertexOutput, mesh2d_view_bindings::globals}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

fn hash(n: f32) -> f32 {
    return fract(sin(n * 12.9898) * 43758.5453);
}

// Smooth value noise along one axis.
fn noise(x: f32, seed: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(hash(i + seed), hash(i + 1.0 + seed), u) - 0.5;
}

// Three octaves of jagged displacement, re-rolled several times a second.
fn displacement(x: f32, seed: f32, segments: f32) -> f32 {
    var d = 0.0;
    var amp = 0.55;
    var freq = segments;
    for (var o = 0; o < 3; o = o + 1) {
        d = d + noise(x * freq, seed + f32(o) * 31.7) * amp;
        amp = amp * 0.5;
        freq = freq * 2.3;
    }
    return d;
}

fn strand(uv: vec2<f32>, seed: f32, segments: f32, width: f32) -> vec2<f32> {
    let taper = pow(sin(uv.x * 3.14159), 0.6);
    let offset = displacement(uv.x, seed, segments) * taper * 0.8;
    let dist = abs(uv.y - 0.5 - offset * 0.5);
    let core = 1.0 - smoothstep(0.0, width, dist);
    let glow = exp(-dist / (width * 3.5));
    return vec2<f32>(core, glow);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tick = floor(globals.time * 30.0);
    let seed = params.x * 17.31 + tick * 7.13;
    let segments = params.w;
    let width = params.z * 0.5;

    let main = strand(in.uv, seed, segments, width);
    // The branch splits off a third of the way along and fades out.
    let branch_mask = smoothstep(0.25, 0.4, in.uv.x) * (1.0 - smoothstep(0.55, 0.9, in.uv.x));
    let branch = strand(in.uv, seed + 91.0, segments * 1.7, width * 0.55) * branch_mask * 0.7;

    let strobe = 0.75 + 0.25 * hash(tick + params.x);
    let core = max(main.x, branch.x);
    let glow = max(main.y, branch.y);
    let rgb = mix(color.rgb * 1.3, vec3<f32>(1.0), clamp(core * 1.2, 0.0, 1.0));
    let alpha = clamp(core + glow * 0.65, 0.0, 1.0) * params.y * strobe * color.a;
    return vec4<f32>(rgb, alpha);
}
