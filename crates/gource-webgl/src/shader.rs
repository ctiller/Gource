//! GLSL ES 3.00 shader sources and uniform binding definitions.
//!
//! All shaders target WebGL2 (`#version 300 es`) with highp float precision.
//!
//! Premultiplied-alpha canvas caveat:
//! WebGL canvases by default assume premultiplied alpha (`premultipliedAlpha: true`).
//! Gource colors and textures are straight (non-premultiplied) RGBA and blended in
//! gamma space with `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` (Alpha) and `ONE, ONE` (Bloom).
//! To avoid double-premultiplication or incorrect canvas compositing into the webpage,
//! the HTML canvas WebGL2 context should be created with `{ premultipliedAlpha: false }`.
//! Alternatively, if created with default canvas options, the final frame output to the
//! canvas default framebuffer would need alpha-multiplication. WebGlRenderer explicitly
//! documents and requires `{ premultipliedAlpha: false }` for accurate Gource parity.

/// Vertex shader common to both Alpha and Bloom materials.
/// Converts screen-pixel coordinates (origin top-left, +y down) into clip space:
/// `clip_x = (pos.x / viewport.x) * 2.0 - 1.0`
/// `clip_y = 1.0 - (pos.y / viewport.y) * 2.0`
pub const VERTEX_SHADER_SOURCE: &str = r#"#version 300 es
precision highp float;

layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;

uniform vec2 u_viewport;

out vec2 v_uv;
out vec4 v_color;

void main() {
    float clip_x = (a_pos.x / u_viewport.x) * 2.0 - 1.0;
    float clip_y = 1.0 - (a_pos.y / u_viewport.y) * 2.0;
    gl_Position = vec4(clip_x, clip_y, 0.0, 1.0);
    v_uv = a_uv;
    v_color = a_color;
}
"#;

/// Fragment shader for Material::Alpha.
/// Direct port of standard Gource texturing: `texture(tex, uv) * colour`.
/// Blended with `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`.
/// Colours are gamma-encoded straight RGBA: no sRGB decoding or conversion.
pub const ALPHA_FRAGMENT_SHADER_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
in vec4 v_color;

uniform sampler2D u_texture;

out vec4 fragColor;

void main() {
    fragColor = texture(u_texture, v_uv) * v_color;
}
"#;

/// Fragment shader for Material::Bloom.
/// Exact port of `data/shaders/bloom.frag` and doc comment on `Material::Bloom`.
///
/// Blended with `ONE, ONE` (additive).
/// `v_uv` spans -1..1 across the glow quad (offset from centre divided by radius).
///
/// For the pseudo-random noise `r`, we compute a cheap hash from `gl_FragCoord.xy`
/// (and modulated with `v_uv`), using the standard fract(sin(dot(...)) * 2351.3718)
/// formula from `bloom.frag`.
///
/// Smoothstep note:
/// `smoothstep(1.0, 0.67 + r * 0.33, 1.0 - intensity)` has edge0 (1.0) > edge1 (~0.85).
/// In GLSL ES 3.00, standard `smoothstep(edge0, edge1, x)` with edge0 > edge1 is undefined
/// or may fail on some drivers. We use an explicit reversed-edge smoothstep implementation
/// `smoothstep_any` matching the reference formula.
pub const BLOOM_FRAGMENT_SHADER_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
in vec4 v_color;

out vec4 fragColor;

float smoothstep_any(float edge0, float edge1, float x) {
    float t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

void main() {
    // Cheap hash based on gl_FragCoord to reproduce per-pixel dithering noise.
    // Constants 11.3713, 67.3219, and 2351.3718 are directly from data/shaders/bloom.frag.
    vec2 p = gl_FragCoord.xy + v_uv;
    float r = fract(sin(dot(p, vec2(11.3713, 67.3219))) * 2351.3718);

    float intensity = min(1.0, cos(2.0 * length(v_uv) + (0.5 - r) * 0.045));
    float gradient = intensity * smoothstep_any(0.0, 2.0, intensity);
    gradient *= smoothstep_any(1.0, 0.67 + r * 0.33, 1.0 - intensity);

    fragColor = v_color * gradient;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_sources_not_empty() {
        assert!(VERTEX_SHADER_SOURCE.starts_with("#version 300 es"));
        assert!(ALPHA_FRAGMENT_SHADER_SOURCE.starts_with("#version 300 es"));
        assert!(BLOOM_FRAGMENT_SHADER_SOURCE.starts_with("#version 300 es"));

        assert!(VERTEX_SHADER_SOURCE.contains("u_viewport"));
        assert!(ALPHA_FRAGMENT_SHADER_SOURCE.contains("u_texture"));
        assert!(BLOOM_FRAGMENT_SHADER_SOURCE.contains("smoothstep_any"));
    }
}
