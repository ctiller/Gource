//! WGSL shaders for `Material::Alpha` (scene) and `Material::Bloom` (directory glow).
//!
//! Matches the fixed-function GL / GLSL shaders of the C++ renderer and the Bevy
//! shaders in `crates/gource/src/render/{scene,bloom}.wgsl`.

/// Textured, vertex-coloured triangles in screen pixel coordinates (`Material::Alpha`).
pub const SCENE_WGSL: &str = r#"
struct ViewportUniform {
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(1) @binding(0) var scene_texture: texture_2d<f32>;
@group(1) @binding(1) var scene_sampler: sampler;

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let size = viewport.size.xy;
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
"#;

/// Additive radial glow around directories (`Material::Bloom`, port of `data/shaders/bloom.{vert,frag}`).
pub const BLOOM_WGSL: &str = r#"
struct ViewportUniform {
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let size = viewport.size.xy;
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

fn smoothstep_any(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * 512.0;
    let r = fract(sin(dot(p, vec2<f32>(11.3713, 67.3219))) * 2351.3718);

    let intensity = min(1.0, cos(length(in.uv * 2.0) + (0.5 - r) * 0.045));
    var gradient = intensity * smoothstep_any(0.0, 2.0, intensity);
    gradient *= smoothstep_any(1.0, 0.67 + r * 0.33, 1.0 - intensity);

    return in.color * gradient;
}
"#;
