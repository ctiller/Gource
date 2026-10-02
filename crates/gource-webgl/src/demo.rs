//! Demo rendering function for WebGL2.

use crate::gl::WebGlRenderer;
use gource_core::{UVec2, Vec2, Vec4};
use gource_draw::{DrawList, TextureOptions, TextureStore};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// Render a tiny demo onto the specified HTML canvas ID.
///
/// Draws a few colored rectangles and a bloom glow effect.
#[wasm_bindgen]
pub fn demo(canvas_id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window found"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("No document found"))?;
    let canvas_el = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str("Canvas element not found"))?;
    let canvas = canvas_el
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("Element is not a canvas"))?;

    // Premultiplied alpha note: set premultipliedAlpha to false
    let context_options = js_sys::Object::new();
    js_sys::Reflect::set(
        &context_options,
        &JsValue::from_str("premultipliedAlpha"),
        &JsValue::from_bool(false),
    )?;

    let gl_obj = canvas
        .get_context_with_context_options("webgl2", &context_options)?
        .ok_or_else(|| JsValue::from_str("WebGL2 not supported"))?;
    let gl = gl_obj
        .dyn_into::<web_sys::WebGl2RenderingContext>()
        .map_err(|_| JsValue::from_str("Not a WebGl2RenderingContext"))?;

    let width = canvas.width();
    let height = canvas.height();

    let mut renderer = WebGlRenderer::new(gl).map_err(|e| JsValue::from_str(&e))?;
    let mut store = TextureStore::new();

    // Create a 2x2 test texture (check pattern)
    let check_pixels = vec![
        255, 100, 100, 255, 100, 255, 100, 255, 100, 100, 255, 255, 255, 255, 100, 255,
    ];
    let custom_tex = store.create_rgba("demo_check", 2, 2, check_pixels, TextureOptions::plain());

    renderer.sync_textures(&store);

    let mut list = DrawList::new(UVec2::new(width, height));
    list.clear_colour = Vec4::new(0.05, 0.05, 0.08, 1.0);

    // Solid background rect
    list.solid_rect(
        Vec2::new(20.0, 20.0),
        Vec2::new(200.0, 150.0),
        Vec4::new(0.2, 0.3, 0.6, 0.8),
    );

    // Textured rect
    list.rect(
        custom_tex,
        Vec2::new(260.0, 20.0),
        Vec2::new(150.0, 150.0),
        Vec4::ONE,
    );

    // Line
    list.line(
        Vec2::new(50.0, 250.0),
        Vec2::new(400.0, 300.0),
        6.0,
        Vec4::new(1.0, 0.8, 0.2, 1.0),
    );

    // Bloom glow
    list.bloom(Vec2::new(300.0, 200.0), 80.0, Vec4::new(1.0, 0.5, 0.2, 1.0));

    renderer.render(&list);

    Ok(())
}
