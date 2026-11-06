// blit.wgsl
// Copies the MSAA-resolved texture to the surface target. A full-screen
// NDC quad is drawn; the render pass viewport places it on the visible
// sub-rectangle of the surface, and `u_blit.uv_scale / uv_offset` crops
// the resolve texture to the corresponding portion of the full viewport
// (so a viewport that extends off the canvas still composites correctly
// from the on-surface side).

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct BlitUniforms {
    uv_offset: vec2<f32>,
    uv_scale:  vec2<f32>,
}

var<private> POSITIONS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0,  1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
);

// NDC Y is up; texture V is down — flip Y.
var<private> UVS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, 0.0),
);

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(POSITIONS[idx], 0.0, 1.0);
    out.uv = UVS[idx];
    return out;
}

@group(0) @binding(0) var t_resolved: texture_2d<f32>;
@group(0) @binding(1) var s_resolved: sampler;
@group(0) @binding(2) var<uniform> u_blit: BlitUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv * u_blit.uv_scale + u_blit.uv_offset;
    return textureSample(t_resolved, s_resolved, uv);
}
