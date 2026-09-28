//! Debug helper: run `GourceApp` headless for a number of frames and print a
//! summary of each frame's draw list (no window, no GPU).
//!
//! ```sh
//! cargo run -p gource --example dump_drawlist -- FRAMES [gource args...]
//! ```

use gource_draw::{DrawList, Material};
use gource_settings::{CliAction, parse_command_line};
use gource_sim::{AppOptions, GourceApp, Viewport};

fn main() {
    let mut args = std::env::args().skip(1);
    let frames: usize = args
        .next()
        .and_then(|n| n.parse().ok())
        .expect("usage: dump_drawlist FRAMES [gource args...]");
    let args: Vec<String> = args.collect();
    let config = match parse_command_line(&args) {
        Ok(CliAction::Run(config)) => config,
        Ok(_) => panic!("not a run command"),
        Err(e) => panic!("{}", e.0),
    };
    let viewport = Viewport {
        width: config.display.display_width.max(1) as u32,
        height: config.display.display_height.max(1) as u32,
        dpi_ratio: 1.0,
    };
    let recording = !config.display.output_ppm_filename.is_empty();
    let mut app = GourceApp::new(config, AppOptions { recording }).expect("start");
    let mut list = DrawList::default();
    for frame in 0..frames {
        app.frame(1.0 / 60.0, viewport, &mut list);
        let requests = app.take_requests();
        println!(
            "frame {frame}: clear {:?} batches {} requests {:?}",
            list.clear_colour,
            list.batches.len(),
            requests
        );
        for (i, batch) in list.batches.iter().enumerate() {
            if batch.vertices.is_empty() {
                continue;
            }
            let (mut min, mut max) = (batch.vertices[0].pos, batch.vertices[0].pos);
            let (mut cmin, mut cmax) = (batch.vertices[0].colour, batch.vertices[0].colour);
            for v in &batch.vertices {
                min = min.min(v.pos);
                max = max.max(v.pos);
                cmin = cmin.min(v.colour);
                cmax = cmax.max(v.colour);
            }
            let material = match batch.material {
                Material::Alpha => "alpha",
                Material::Bloom => "bloom",
            };
            println!(
                "  #{i} {material} tex {:?} verts {} idx {} pos {:.0?}..{:.0?} colour {:.3?}..{:.3?}",
                batch.texture,
                batch.vertices.len(),
                batch.indices.len(),
                min,
                max,
                cmin,
                cmax
            );
        }
    }
}
