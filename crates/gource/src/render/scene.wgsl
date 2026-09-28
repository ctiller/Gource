// Gource scene material: textured, vertex-coloured triangles given in
// screen pixels (origin top-left, +y down).
//
// Equivalent of the fixed-function GL path used by the C++ renderer:
// `out = texture(uv) * colour`, blended with SRC_ALPHA / ONE_MINUS_SRC_ALPHA
// (the blend state is set in `SceneMaterial::specialize`). Colours and texels
// are used as-is (gamma space), so no sRGB conversion happens here.

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

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var scene_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var scene_sampler: sampler;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let size = view.viewport.zw;
    // Pixel -> clip space. Depth is irrelevant (painter's order comes from
    // the entity sort key); any value in [0, 1] passes the 2D depth test.
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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(scene_texture, scene_sampler, in.uv) * in.color;
}
