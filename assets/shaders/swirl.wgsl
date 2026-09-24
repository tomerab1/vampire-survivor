// Spiral vortex: tornado funnels, stage portals and blink rifts.
// params: x = arm count, y = twist, z = spin speed, w = core brightness
#import bevy_sprite::{mesh2d_vertex_output::VertexOutput, mesh2d_view_bindings::globals}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv - vec2<f32>(0.5);
    let r = length(p) * 2.0;
    let angle = atan2(p.y, p.x);
    let spiral = sin(angle * params.x + r * params.y - globals.time * params.z);
    let arms = smoothstep(0.1, 0.9, spiral) * (1.0 - r);
    let rim = smoothstep(0.75, 0.9, r) * (1.0 - smoothstep(0.9, 1.0, r));
    let core = 1.0 - smoothstep(0.0, 0.3, r);
    let fade = 1.0 - smoothstep(0.92, 1.0, r);
    let rgb = mix(color.rgb, vec3<f32>(1.0), clamp(core * params.w + arms * 0.25, 0.0, 1.0));
    let alpha = clamp(arms * 0.9 + rim * 0.6 + core * params.w * 0.8, 0.0, 1.0) * fade * color.a;
    return vec4<f32>(rgb, alpha);
}
