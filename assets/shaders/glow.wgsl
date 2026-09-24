// Soft radial glow with a hot white core. Used for magic bolts, gems, and pickups.
// params: x = pulse speed, y = halo falloff exponent, z = core radius (0..1), w = unused
#import bevy_sprite::{mesh2d_vertex_output::VertexOutput, mesh2d_view_bindings::globals}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = length(in.uv - vec2<f32>(0.5)) * 2.0;
    let pulse = 1.0 + 0.25 * sin(globals.time * params.x);
    let halo = pow(clamp(1.0 - d, 0.0, 1.0), params.y) * pulse;
    let core = 1.0 - smoothstep(0.0, max(params.z, 0.001), d);
    let rgb = mix(color.rgb, vec3<f32>(1.0), core * 0.85);
    return vec4<f32>(rgb, clamp(halo + core, 0.0, 1.0) * color.a);
}
