// Full-screen overlay: dark vignette, red flash when hurt, heartbeat pulse at low HP,
// and a golden bloom on level-up.
// params: x = hurt flash (0..1), y = low-HP factor (0..1), z = level-up glow (0..1), w = aspect ratio
#import bevy_sprite::{mesh2d_vertex_output::VertexOutput, mesh2d_view_bindings::globals}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(params.w, 1.0) * 2.0;
    let r = length(p) / max(params.w, 1.0);
    let edge = smoothstep(0.45, 1.25, r);
    let beat = params.y * (0.5 + 0.5 * sin(globals.time * 7.0));
    let red = clamp(params.x + beat * 0.7, 0.0, 1.0);
    let gold = params.z * smoothstep(0.2, 1.1, r);
    let base = mix(color.rgb, vec3<f32>(0.75, 0.02, 0.05), red);
    let rgb = mix(base, vec3<f32>(1.0, 0.82, 0.35), clamp(gold, 0.0, 1.0));
    let alpha = clamp(edge * (color.a + red * 0.55) + gold * 0.6, 0.0, 0.95);
    return vec4<f32>(rgb, alpha);
}
