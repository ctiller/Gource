//! Browser glue (wasm32 only): canvas, fetch streaming, the frame loop and
//! DOM input.

use crate::remote::{RemoteStream, stream_url};
use gource_app::{AppOptions, GourceApp, InputEvent, Viewport};
use gource_core::StringHasher;
use gource_draw::DrawList;
use gource_model::wire::FilterSpec;
use gource_settings::{CliAction, parse_command_line};
use gource_vcs::CommitFeed;
use gource_webgl::WebGlRenderer;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    HtmlCanvasElement, KeyboardEvent, MouseEvent, ReadableStreamDefaultReader, Response,
    WebGl2RenderingContext, WheelEvent,
};

fn log(msg: &str) {
    web_sys::console::log_1(&msg.into());
}

fn error(msg: &str) {
    web_sys::console::error_1(&msg.into());
}

struct State {
    app: GourceApp,
    renderer: WebGlRenderer,
    list: DrawList,
    canvas: HtmlCanvasElement,
    input: Vec<InputEvent>,
    last_ms: Option<f64>,
    last_mouse: gource_core::Vec2,
    frames: u64,
    /// If non-zero, each animation frame advances this many fixed 1/60 s
    /// steps instead of the wall-clock time (deterministic captures).
    fixed_steps: u32,
}

impl State {
    fn viewport(&self) -> Viewport {
        let dpr = web_sys::window().unwrap().device_pixel_ratio().max(1.0);
        let w = (self.canvas.client_width() as f64 * dpr).round().max(1.0) as u32;
        let h = (self.canvas.client_height() as f64 * dpr).round().max(1.0) as u32;
        if self.canvas.width() != w || self.canvas.height() != h {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
        }
        let mut v = Viewport::new(w, h);
        v.dpi_ratio = dpr as f32;
        v
    }

    fn frame(&mut self, now_ms: f64) {
        let dt = self.last_ms.map_or(1.0 / 60.0, |last| {
            ((now_ms - last) / 1000.0).clamp(0.0, 0.25)
        }) as f32;
        self.last_ms = Some(now_ms);
        for event in std::mem::take(&mut self.input) {
            self.app.input(&event);
        }
        let viewport = self.viewport();
        if self.fixed_steps == 0 {
            self.app.frame(dt, viewport, &mut self.list);
        } else {
            for _ in 0..self.fixed_steps {
                self.app.frame(1.0 / 60.0, viewport, &mut self.list);
            }
        }
        let _ = self.app.take_requests();
        self.renderer.sync_textures(&self.app.gfx().textures);
        self.renderer.render(&self.list);
        self.frames += 1;
    }

    /// Canvas-relative position in physical pixels.
    fn mouse_pos(&self, e: &MouseEvent) -> gource_core::Vec2 {
        let dpr = web_sys::window().unwrap().device_pixel_ratio() as f32;
        gource_core::Vec2::new(e.offset_x() as f32, e.offset_y() as f32) * dpr
    }
}

/// Start Gource on the canvas with id `canvas_id`, streaming from the
/// `gource-serve` at `server` ("" = this page's origin). `args` are gource
/// command line options (whitespace separated); `filter` is a FilterSpec
/// query string (`ff=..&us=..`). `fixed_steps` > 0 replaces wall-clock
/// time with that many 1/60 s steps per animation frame (for captures).
#[wasm_bindgen]
pub async fn start(
    canvas_id: String,
    server: String,
    args: String,
    filter: String,
    fixed_steps: u32,
) -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(|info| error(&format!("gource panicked: {info}"))));

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas: HtmlCanvasElement = document
        .get_element_by_id(&canvas_id)
        .ok_or_else(|| format!("no element #{canvas_id}"))?
        .dyn_into()?;

    let attrs = js_sys::Object::new();
    js_sys::Reflect::set(&attrs, &"premultipliedAlpha".into(), &false.into())?;
    js_sys::Reflect::set(&attrs, &"antialias".into(), &true.into())?;
    let gl: WebGl2RenderingContext = canvas
        .get_context_with_context_options("webgl2", &attrs)?
        .ok_or("WebGL2 is not available")?
        .dyn_into()?;
    let renderer = WebGlRenderer::new(gl)?;

    // No path: the default (".") is not checked, and the feed replaces it.
    let argv: Vec<String> = args.split_whitespace().map(String::from).collect();
    let config = match parse_command_line(&argv) {
        Ok(CliAction::Run(config)) => config,
        other => return Err(format!("unusable gource arguments: {other:?}").into()),
    };

    let feed = CommitFeed::new();
    let options = AppOptions {
        feed: Some(feed.clone()),
        ..Default::default()
    };
    let app = GourceApp::new(config, options).map_err(|e| e.to_string())?;

    let filter = FilterSpec::from_query(&filter).map_err(|e| e.to_string())?;
    let server = if server.is_empty() {
        window.location().origin()?
    } else {
        server
    };
    let url = stream_url(&server, None, &filter);
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = pump(&url, feed).await {
            error(&format!("stream {url}: {e}"));
        }
    });

    let state = Rc::new(RefCell::new(State {
        app,
        renderer,
        list: DrawList::default(),
        canvas: canvas.clone(),
        input: Vec::new(),
        last_ms: None,
        last_mouse: gource_core::Vec2::ZERO,
        frames: 0,
        fixed_steps,
    }));
    install_input(&canvas, &state)?;
    run_frames(state);
    Ok(())
}

/// Stream the response body into the feed.
async fn pump(url: &str, feed: CommitFeed) -> Result<(), String> {
    let js = |e: JsValue| format!("{e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let resp: Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(js)?
        .dyn_into()
        .map_err(js)?;
    if !resp.ok() {
        let text = JsFuture::from(resp.text().map_err(js)?).await.map_err(js)?;
        return Err(format!(
            "HTTP {}: {}",
            resp.status(),
            text.as_string().unwrap_or_default()
        ));
    }
    let body = resp.body().ok_or("response has no body")?;
    let reader: ReadableStreamDefaultReader = body.get_reader().unchecked_into();
    let mut stream = RemoteStream::new(feed, StringHasher::default());
    let mut bytes = 0usize;
    loop {
        let chunk = JsFuture::from(reader.read()).await.map_err(js)?;
        let done = js_sys::Reflect::get(&chunk, &"done".into()).map_err(js)?;
        if done.is_truthy() {
            break;
        }
        let value = js_sys::Reflect::get(&chunk, &"value".into()).map_err(js)?;
        let data = js_sys::Uint8Array::new(&value).to_vec();
        bytes += data.len();
        stream.push(&data)?;
    }
    stream.finish()?;
    log(&format!(
        "gource: received {} commits ({bytes} bytes)",
        stream.commits()
    ));
    Ok(())
}

fn run_frames(state: Rc<RefCell<State>>) {
    type Tick = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;
    let tick: Tick = Rc::new(RefCell::new(None));
    let again = tick.clone();
    *tick.borrow_mut() = Some(Closure::new(move |now: f64| {
        state.borrow_mut().frame(now);
        request_frame(again.borrow().as_ref().unwrap());
    }));
    request_frame(tick.borrow().as_ref().unwrap());
}

fn request_frame(f: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("requestAnimationFrame");
}

fn install_input(canvas: &HtmlCanvasElement, state: &Rc<RefCell<State>>) -> Result<(), JsValue> {
    use crate::input::{key_with_code, modifiers, mouse_button, wheel_steps};
    canvas.set_tab_index(0);
    let _ = canvas.focus();

    let listen = |target: &web_sys::EventTarget,
                  name: &str,
                  f: Box<dyn FnMut(web_sys::Event)>|
     -> Result<(), JsValue> {
        let closure = Closure::wrap(f);
        target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
        closure.forget();
        Ok(())
    };

    for (name, down) in [("keydown", true), ("keyup", false)] {
        let s = state.clone();
        listen(
            canvas,
            name,
            Box::new(move |e| {
                let e: KeyboardEvent = e.unchecked_into();
                let key = key_with_code(&e.key(), &e.code());
                let mods = modifiers(e.shift_key(), e.ctrl_key(), e.alt_key(), e.meta_key());
                e.prevent_default();
                s.borrow_mut().input.push(if down {
                    InputEvent::KeyDown {
                        key,
                        modifiers: mods,
                        repeat: e.repeat(),
                    }
                } else {
                    InputEvent::KeyUp {
                        key,
                        modifiers: mods,
                    }
                });
            }),
        )?;
    }
    for (name, pressed) in [("mousedown", true), ("mouseup", false)] {
        let s = state.clone();
        listen(
            canvas,
            name,
            Box::new(move |e| {
                let e: MouseEvent = e.unchecked_into();
                if let Some(button) = mouse_button(e.button()) {
                    let mut st = s.borrow_mut();
                    let pos = st.mouse_pos(&e);
                    st.input.push(InputEvent::MouseButton {
                        button,
                        pressed,
                        pos,
                    });
                }
            }),
        )?;
    }
    let s = state.clone();
    listen(
        canvas,
        "mousemove",
        Box::new(move |e| {
            let e: MouseEvent = e.unchecked_into();
            let mut st = s.borrow_mut();
            let pos = st.mouse_pos(&e);
            let delta = pos - st.last_mouse;
            st.last_mouse = pos;
            st.input.push(InputEvent::MouseMove { pos, delta });
        }),
    )?;
    let s = state.clone();
    listen(
        canvas,
        "wheel",
        Box::new(move |e| {
            let e: WheelEvent = e.unchecked_into();
            e.prevent_default();
            let delta = wheel_steps(e.delta_y());
            if delta != 0.0 {
                s.borrow_mut().input.push(InputEvent::MouseWheel { delta });
            }
        }),
    )?;
    listen(canvas, "contextmenu", Box::new(|e| e.prevent_default()))?;
    Ok(())
}

/// The gource-web version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
