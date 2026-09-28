// Gource bloom (port of data/shaders/bloom.{vert,frag}), blended additively
// (ONE, ONE; set in `BloomMaterial::specialize`).
//
// `uv` is the offset from the glow centre divided by the glow radius (-1..1
// across the quad). The C++ shader works on the world-space offset `pos` and
// the radius `r`; `(length(pos * 2) + offset) / r` equals
// `length(uv * 2) + (0.5 - noise) * 0.045`, which is what is computed here.

#import bevy_sprite::mesh2d_view_bindings::view

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// Unused; keeps the material bind group non-empty.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> bloom_params: vec4<f32>;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let size = view.viewport.zw;
    out.position = vec4<f32>(
        v.position.x / size.x * 2.0 - 1.0,
        1.0 - v.position.y / size.y * 2.0,
        0.5,
        1.0,
    );
    out.uv = v.uv;
    out.color = v.color;
    return out;
}

// GLSL smoothstep evaluated with reversed edges (edge0 > edge1), which the
// original shader relies on; WGSL/SPIR-V leave that case undefined.
fn smoothstep_any(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Per-fragment noise. The C++ hashes the world-space offset; any input
    // that varies per fragment gives the same dithering character.
    let p = in.uv * 512.0;
    let r = fract(sin(dot(p, vec2<f32>(11.3713, 67.3219))) * 2351.3718);

    let intensity = min(1.0, cos(length(in.uv * 2.0) + (0.5 - r) * 0.045));
    var gradient = intensity * smoothstep_any(0.0, 2.0, intensity);
    gradient *= smoothstep_any(1.0, 0.67 + r * 0.33, 1.0 - intensity);

    return in.color * gradient + bloom_params * 0.0;
}
