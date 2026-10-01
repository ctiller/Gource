//! WebGL2 implementation of `WebGlRenderer` compiled only for `target_arch = "wasm32"`.

use crate::buffer::FrameDrawData;
use crate::plan::{
    GL_ARRAY_BUFFER, GL_COLOR_BUFFER_BIT, GL_DYNAMIC_DRAW, GL_ELEMENT_ARRAY_BUFFER, GL_FLOAT,
    GL_RGBA, GL_RGBA8, GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_TEXTURE_MIN_FILTER,
    GL_TEXTURE_WRAP_S, GL_TEXTURE_WRAP_T, GL_TRIANGLES, GL_UNPACK_ALIGNMENT, GL_UNSIGNED_BYTE,
    GL_UNSIGNED_INT, TextureAction, TextureTracker, VERTEX_STRIDE_BYTES, gl_mag_filter,
    gl_min_filter, gl_wrap,
};
use crate::shader::{
    ALPHA_FRAGMENT_SHADER_SOURCE, BLOOM_FRAGMENT_SHADER_SOURCE, VERTEX_SHADER_SOURCE,
};
use gource_draw::{DrawList, Material, TextureId, TextureStore};
use std::collections::HashMap;
use web_sys::{
    WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

/// WebGL2 renderer for Gource `DrawList`.
pub struct WebGlRenderer {
    gl: WebGl2RenderingContext,
    alpha_program: ProgramHandle,
    bloom_program: ProgramHandle,
    vao: WebGlVertexArrayObject,
    vbo: WebGlBuffer,
    ebo: WebGlBuffer,
    textures: HashMap<TextureId, WebGlTexture>,
    tracker: TextureTracker,
}

struct ProgramHandle {
    program: WebGlProgram,
    u_viewport: Option<WebGlUniformLocation>,
    u_texture: Option<WebGlUniformLocation>,
}

impl WebGlRenderer {
    /// Initialize a new WebGL2 renderer.
    ///
    /// # Premultiplied Alpha Note
    /// To preserve exact color parity with Gource, the canvas context should be created with:
    /// `{ "premultipliedAlpha": false }`
    pub fn new(gl: WebGl2RenderingContext) -> Result<Self, String> {
        let alpha_prog = create_program(&gl, VERTEX_SHADER_SOURCE, ALPHA_FRAGMENT_SHADER_SOURCE)?;
        let u_viewport_alpha = gl.get_uniform_location(&alpha_prog, "u_viewport");
        let u_texture_alpha = gl.get_uniform_location(&alpha_prog, "u_texture");
        let alpha_program = ProgramHandle {
            program: alpha_prog,
            u_viewport: u_viewport_alpha,
            u_texture: u_texture_alpha,
        };

        let bloom_prog = create_program(&gl, VERTEX_SHADER_SOURCE, BLOOM_FRAGMENT_SHADER_SOURCE)?;
        let u_viewport_bloom = gl.get_uniform_location(&bloom_prog, "u_viewport");
        let bloom_program = ProgramHandle {
            program: bloom_prog,
            u_viewport: u_viewport_bloom,
            u_texture: None,
        };

        let vao = gl
            .create_vertex_array()
            .ok_or_else(|| "Failed to create WebGL VAO".to_string())?;
        let vbo = gl
            .create_buffer()
            .ok_or_else(|| "Failed to create VBO".to_string())?;
        let ebo = gl
            .create_buffer()
            .ok_or_else(|| "Failed to create EBO".to_string())?;

        gl.bind_vertex_array(Some(&vao));
        gl.bind_buffer(GL_ARRAY_BUFFER, Some(&vbo));
        gl.bind_buffer(GL_ELEMENT_ARRAY_BUFFER, Some(&ebo));

        // Setup vertex attributes:
        // 0: vec2 a_pos (offset 0 bytes)
        // 1: vec2 a_uv (offset 2 * 4 = 8 bytes)
        // 2: vec4 a_color (offset 4 * 4 = 16 bytes)
        let f32_bytes = std::mem::size_of::<f32>() as i32;

        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 2, GL_FLOAT, false, VERTEX_STRIDE_BYTES, 0);

        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(
            1,
            2,
            GL_FLOAT,
            false,
            VERTEX_STRIDE_BYTES,
            2 * f32_bytes,
        );

        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_with_i32(
            2,
            4,
            GL_FLOAT,
            false,
            VERTEX_STRIDE_BYTES,
            4 * f32_bytes,
        );

        gl.bind_vertex_array(None);
        gl.bind_buffer(GL_ARRAY_BUFFER, None);
        gl.bind_buffer(GL_ELEMENT_ARRAY_BUFFER, None);

        Ok(Self {
            gl,
            alpha_program,
            bloom_program,
            vao,
            vbo,
            ebo,
            textures: HashMap::new(),
            tracker: TextureTracker::new(),
        })
    }

    /// Upload new/changed textures (by `version`), delete GL textures whose ids are gone from the store.
    pub fn sync_textures(&mut self, store: &TextureStore) {
        let actions = self.tracker.plan_sync(store);

        for action in actions {
            match action {
                TextureAction::Upload { id, version } => {
                    if let Some(tex) = store.get(id) {
                        let gl_tex = self.textures.entry(id).or_insert_with(|| {
                            self.gl
                                .create_texture()
                                .expect("Failed to create WebGlTexture")
                        });

                        self.gl.bind_texture(GL_TEXTURE_2D, Some(gl_tex));

                        // Ensure 1-byte row unpack alignment for variable sized images
                        self.gl.pixel_storei(GL_UNPACK_ALIGNMENT, 1);

                        let min_filter = gl_min_filter(tex.options.filter, tex.options.mipmaps);
                        let mag_filter = gl_mag_filter(tex.options.filter);
                        let wrap_s = gl_wrap(tex.options.wrap);
                        let wrap_t = gl_wrap(tex.options.wrap);

                        self.gl.tex_parameteri(
                            GL_TEXTURE_2D,
                            GL_TEXTURE_MIN_FILTER,
                            min_filter as i32,
                        );
                        self.gl.tex_parameteri(
                            GL_TEXTURE_2D,
                            GL_TEXTURE_MAG_FILTER,
                            mag_filter as i32,
                        );
                        self.gl
                            .tex_parameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, wrap_s as i32);
                        self.gl
                            .tex_parameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, wrap_t as i32);

                        // Upload mip levels
                        for (level, data) in tex.levels.iter().enumerate() {
                            let level_w = (tex.width >> level).max(1);
                            let level_h = (tex.height >> level).max(1);

                            let res = self.gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                                GL_TEXTURE_2D,
                                level as i32,
                                GL_RGBA8 as i32,
                                level_w as i32,
                                level_h as i32,
                                0,
                                GL_RGBA,
                                GL_UNSIGNED_BYTE,
                                Some(data),
                            );
                            if let Err(e) = res {
                                log::error!(
                                    "Failed to upload mip level {level} for texture {:?}: {:?}",
                                    id,
                                    e
                                );
                            }
                        }

                        self.gl.bind_texture(GL_TEXTURE_2D, None);
                        self.tracker.record_upload(id, version);
                    }
                }
                TextureAction::Delete { id } => {
                    if let Some(gl_tex) = self.textures.remove(&id) {
                        self.gl.delete_texture(Some(&gl_tex));
                    }
                    self.tracker.record_delete(id);
                }
            }
        }
    }

    /// Clear to list.clear_colour, set viewport, draw batches in order.
    pub fn render(&mut self, list: &DrawList) {
        let vp_w = list.viewport.x as i32;
        let vp_h = list.viewport.y as i32;

        self.gl.viewport(0, 0, vp_w, vp_h);

        self.gl.clear_color(
            list.clear_colour.x,
            list.clear_colour.y,
            list.clear_colour.z,
            list.clear_colour.w,
        );
        self.gl.clear(GL_COLOR_BUFFER_BIT);

        if list.is_empty() {
            return;
        }

        let frame_data = FrameDrawData::from_draw_list(list);
        if frame_data.is_empty() {
            return;
        }

        // Upload vertex & index data
        self.gl.bind_buffer(GL_ARRAY_BUFFER, Some(&self.vbo));
        unsafe {
            let vert_view = js_sys::Float32Array::view(&frame_data.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                GL_ARRAY_BUFFER,
                &vert_view,
                GL_DYNAMIC_DRAW,
            );
        }

        self.gl
            .bind_buffer(GL_ELEMENT_ARRAY_BUFFER, Some(&self.ebo));
        unsafe {
            let idx_view = js_sys::Uint32Array::view(&frame_data.indices);
            self.gl.buffer_data_with_array_buffer_view(
                GL_ELEMENT_ARRAY_BUFFER,
                &idx_view,
                GL_DYNAMIC_DRAW,
            );
        }

        // Enable blending
        self.gl.enable(WebGl2RenderingContext::BLEND);

        // Bind VAO
        self.gl.bind_vertex_array(Some(&self.vao));

        let vp_size = [list.width(), list.height()];

        let mut current_program = None;
        let mut current_texture = None;
        let mut current_blend = None;

        for batch in &frame_data.batches {
            let prog_handle = match batch.material {
                Material::Alpha => &self.alpha_program,
                Material::Bloom => &self.bloom_program,
            };

            let prog_ptr = prog_handle as *const ProgramHandle;
            if current_program != Some(prog_ptr) {
                self.gl.use_program(Some(&prog_handle.program));
                if let Some(ref loc) = prog_handle.u_viewport {
                    self.gl.uniform2f(Some(loc), vp_size[0], vp_size[1]);
                }
                if let Some(ref loc) = prog_handle.u_texture {
                    // Texture unit 0
                    self.gl.uniform1i(Some(loc), 0);
                }
                current_program = Some(prog_ptr);
            }

            if current_blend != Some(batch.blend) {
                self.gl.blend_func(batch.blend.sfactor, batch.blend.dfactor);
                current_blend = Some(batch.blend);
            }

            if batch.material == Material::Alpha && current_texture != Some(batch.texture) {
                if let Some(tex) = self.textures.get(&batch.texture) {
                    self.gl.active_texture(WebGl2RenderingContext::TEXTURE0);
                    self.gl.bind_texture(GL_TEXTURE_2D, Some(tex));
                }
                current_texture = Some(batch.texture);
            }

            // WebGL2 supports UNSIGNED_INT for glDrawElements
            let byte_offset = (batch.index_offset * std::mem::size_of::<u32>()) as i32;
            self.gl.draw_elements_with_i32(
                GL_TRIANGLES,
                batch.index_count as i32,
                GL_UNSIGNED_INT,
                byte_offset,
            );
        }

        self.gl.bind_vertex_array(None);
        self.gl.use_program(None);
    }
}

impl Drop for WebGlRenderer {
    fn drop(&mut self) {
        for (_, tex) in self.textures.drain() {
            self.gl.delete_texture(Some(&tex));
        }
        self.gl.delete_buffer(Some(&self.vbo));
        self.gl.delete_buffer(Some(&self.ebo));
        self.gl.delete_vertex_array(Some(&self.vao));
        self.gl.delete_program(Some(&self.alpha_program.program));
        self.gl.delete_program(Some(&self.bloom_program.program));
    }
}

fn create_program(
    gl: &WebGl2RenderingContext,
    vert_src: &str,
    frag_src: &str,
) -> Result<WebGlProgram, String> {
    let vert_shader = compile_shader(gl, WebGl2RenderingContext::VERTEX_SHADER, vert_src)?;
    let frag_shader = compile_shader(gl, WebGl2RenderingContext::FRAGMENT_SHADER, frag_src)?;

    let program = gl
        .create_program()
        .ok_or_else(|| "Failed to create GL program".to_string())?;

    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);

    let linked = gl
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);

    if !linked {
        let info = gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "Unknown error linking program".to_string());
        gl.delete_shader(Some(&vert_shader));
        gl.delete_shader(Some(&frag_shader));
        gl.delete_program(Some(&program));
        return Err(format!("Program link error: {info}"));
    }

    gl.delete_shader(Some(&vert_shader));
    gl.delete_shader(Some(&frag_shader));

    Ok(program)
}

fn compile_shader(
    gl: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| "Failed to create GL shader".to_string())?;

    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    let compiled = gl
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false);

    if !compiled {
        let info = gl
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "Unknown error compiling shader".to_string());
        gl.delete_shader(Some(&shader));
        return Err(format!("Shader compile error: {info}"));
    }

    Ok(shader)
}
