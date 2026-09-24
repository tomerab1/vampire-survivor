// Animated energy ring. Used for the damage aura, hammer shockwaves, and weapon-drop halos.
// params: x = inner edge (0..1), y = edge softness, z = swirl speed, w = interior fill strength
#import bevy_sprite::{mesh2d_vertex_output::VertexOutput, mesh2d_view_bindings::globals}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv - vec2<f32>(0.5);
    let d = length(p) * 2.0;
    let angle = atan2(p.y, p.x);
    let soft = max(params.y, 0.001);
    let band = smoothstep(params.x - soft, params.x, d) * (1.0 - smoothstep(1.0 - soft, 1.0, d));
    let swirl = 0.55 + 0.45 * sin(angle * 7.0 + globals.time * params.z - d * 12.0);
    let fill = (1.0 - smoothstep(0.0, 1.0, d)) * params.w;
    let alpha = clamp(band * swirl + fill, 0.0, 1.0) * color.a;
    let rgb = mix(color.rgb, vec3<f32>(1.0), band * swirl * 0.35);
    return vec4<f32>(rgb, alpha);
}
