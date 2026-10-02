//! Renderer smoke test: draws a fixed scene exercising every draw list
//! feature (solid and textured quads, lines, bloom, text, alpha blending),
//! saves a screenshot and exits.
//!
//! ```sh
//! xvfb-run -a -s "-screen 0 1280x720x24" \
//!     cargo run -p gource --example drawlist_smoke -- /tmp/smoke.png
//! ```

use std::path::PathBuf;

use gource::{
    app::{self, AppConfig},
    sim::Simulation,
};
use gource_app::{InputEvent, PlatformRequest, Viewport};
use gource_core::{UVec2, Vec2, Vec4};
use gource_draw::{
    DrawList, FontId, Gfx, TextStyle, TextureId, TextureOptions, resources::FILE_PNG,
};
use gource_settings::{DisplaySettings, LogLevel};

struct Smoke {
    gfx: Gfx,
    font: FontId,
    file: TextureId,
    frame: u32,
    output: PathBuf,
    recording: bool,
    requests: Vec<PlatformRequest>,
}

impl Smoke {
    fn new(output: PathBuf) -> Self {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 24);
        let file = gfx
            .textures
            .load_bytes("file.png", FILE_PNG, TextureOptions::default())
            .expect("embedded file.png");
        Self {
            gfx,
            font,
            file,
            frame: 0,
            output,
            recording: false,
            requests: Vec::new(),
        }
    }
}

impl Simulation for Smoke {
    fn input(&mut self, event: &InputEvent) {
        if let InputEvent::KeyDown {
            key: gource_app::Key::Escape,
            ..
        } = event
        {
            self.requests.push(PlatformRequest::Quit);
        }
    }

    fn frame(&mut self, _dt: f32, viewport: Viewport, list: &mut DrawList) {
        list.reset(
            UVec2::new(viewport.width, viewport.height),
            Vec4::new(0.1, 0.1, 0.1, 1.0),
        );
        // Opaque and translucent solid rectangles (the overlap shows blending).
        list.solid_rect(
            Vec2::new(20.0, 20.0),
            Vec2::new(200.0, 100.0),
            Vec4::new(1.0, 0.0, 0.0, 1.0),
        );
        list.solid_rect(
            Vec2::new(120.0, 70.0),
            Vec2::new(200.0, 100.0),
            Vec4::new(0.0, 0.0, 1.0, 0.5),
        );
        // Textured quads, tinted.
        for i in 0..6 {
            let x = 20.0 + i as f32 * 70.0;
            let tint = Vec4::new(1.0, 1.0 - i as f32 * 0.15, i as f32 * 0.15, 1.0);
            list.rect(self.file, Vec2::new(x, 220.0), Vec2::new(64.0, 64.0), tint);
        }
        // Lines of several widths.
        for i in 0..5 {
            let y = 320.0 + i as f32 * 20.0;
            list.line(
                Vec2::new(20.0, y),
                Vec2::new(420.0, y + 40.0),
                1.0 + i as f32 * 2.0,
                Vec4::ONE,
            );
        }
        // Bloom (additive).
        list.bloom(
            Vec2::new(700.0, 200.0),
            150.0,
            Vec4::new(0.4, 0.6, 1.0, 1.0),
        );
        list.bloom(
            Vec2::new(800.0, 260.0),
            100.0,
            Vec4::new(1.0, 0.5, 0.2, 1.0),
        );
        // Text with shadow.
        let style = TextStyle::new(Vec4::ONE).with_shadow(true);
        self.gfx.draw_text(
            list,
            self.font,
            Vec2::new(20.0, 520.0),
            "Gource on Bevy: DrawList smoke test",
            &style,
        );
        let style = TextStyle::new(Vec4::new(1.0, 1.0, 0.0, 0.5));
        self.gfx.draw_text(
            list,
            self.font,
            Vec2::new(20.0, 560.0),
            "half transparent yellow",
            &style,
        );

        // A marker moving 10 px per frame, to check the order of recorded
        // frames.
        list.solid_rect(
            Vec2::new(500.0 + self.frame as f32 * 10.0, 400.0),
            Vec2::new(8.0, 8.0),
            Vec4::new(0.0, 1.0, 0.0, 1.0),
        );

        self.frame += 1;
        if self.recording && self.frame <= 40 {
            self.requests.push(PlatformRequest::CaptureFrame);
        }
        if self.frame == 30 {
            self.requests.push(PlatformRequest::Screenshot {
                path: self.output.clone(),
                with_alpha: false,
            });
        }
        if self.frame == 60 {
            self.requests.push(PlatformRequest::Quit);
        }
    }

    fn take_requests(&mut self) -> Vec<PlatformRequest> {
        std::mem::take(&mut self.requests)
    }

    fn gfx(&self) -> &Gfx {
        &self.gfx
    }
}

/// Usage: `drawlist_smoke [SCREENSHOT.png] [VIDEO.ppm]`. With a PPM path, the
/// first 40 frames are recorded.
fn main() {
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("drawlist_smoke.png"));
    let exporter: Option<Box<dyn gource_draw::VideoSink>> = args.next().map(|path| {
        Box::new(gource_draw::PpmExporter::new(&path).expect("open PPM output"))
            as Box<dyn gource_draw::VideoSink>
    });
    let display = DisplaySettings {
        display_width: 1024,
        display_height: 640,
        viewport_specified: true,
        ..DisplaySettings::default()
    };
    let mut smoke = Smoke::new(output);
    smoke.recording = exporter.is_some();
    let code = app::run(
        Box::new(smoke),
        AppConfig {
            display,
            log_level: LogLevel::Warn,
            exporter,
        },
    );
    std::process::exit(code as i32);
}
