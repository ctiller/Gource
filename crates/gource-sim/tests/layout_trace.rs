use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};
use gource_sim::platform::Viewport;
use gource_sim::shell::GourceShell;

#[test]
#[ignore = "investigation harness: prints layout trajectories for comparison with an instrumented C++ build"]
fn test_layout_trace() {
    let log_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/app/gource-custom.log"
    );
    let args = vec![
        "gource".to_string(),
        "-1280x720".to_string(),
        "--seconds-per-day".to_string(),
        "0.2".to_string(),
        "--auto-skip-seconds".to_string(),
        "0.1".to_string(),
        "--stop-at-time".to_string(),
        "20".to_string(),
        "-r".to_string(),
        "60".to_string(),
        log_path.to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("failed to parse command line");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        other => panic!("expected CliAction::Run, got {:?}", other),
    };

    let opts = gource_sim::AppOptions { recording: true };
    let mut shell = GourceShell::new(config, opts).expect("failed to create GourceShell");

    let viewport = Viewport {
        width: 1280,
        height: 720,
        dpi_ratio: 1.0,
    };
    let mut list = DrawList::new(glam::UVec2::new(1280, 720));
    let dt = 1.0 / 60.0;

    // Wait until commitlog is initialized
    while shell.gource.as_ref().is_some_and(|g| g.commitlog.is_none()) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        // Run a tiny frame or pump logmill
        shell.frame(0.001, viewport, &mut list);
    }
    // Now reset gource runtime / framecount if needed, or let's check
    if let Some(ref mut gource) = shell.gource {
        gource.runtime = 0.0;
        gource.framecount = 0;
        gource.currtime = 0;
        gource.lasttime = 0;
        gource.subseconds = 0.0;
        gource.idle_time = 0.0;
        gource.stop_position_reached = false;
    }

    let mut frame = 0;
    while !shell.is_finished && frame < 1500 {
        shell.frame(dt, viewport, &mut list);

        if let Some(ref gource) = shell.gource {
            let world = &gource.world;
            let camera = &gource.camera;

            let vis_dirs = world
                .dir_map
                .values()
                .filter(|&&did| world.dirs[did].is_visible(&world.dirs))
                .count();
            let vis_files = world.files.values().filter(|f| !f.pawn.is_hidden()).count();

            let cpos = camera.pos();
            let bounds = world.dir_bounds;

            println!(
                "TRACE frame={} time={:.4} cam=({:.4},{:.4},{:.4}) bounds_min=({:.4},{:.4}) bounds_max=({:.4},{:.4}) vis_dirs={} vis_files={}",
                gource.framecount,
                gource.runtime,
                cpos.x,
                cpos.y,
                cpos.z,
                bounds.min.x,
                bounds.min.y,
                bounds.max.x,
                bounds.max.y,
                vis_dirs,
                vis_files
            );

            for (path, &did) in &world.dir_map {
                let node = &world.dirs[did];
                if node.is_visible(&world.dirs) && node.depth <= 2 {
                    let dist_p = if let Some(pid) = node.parent {
                        node.distance_to_parent(&world.dirs[pid])
                    } else {
                        0.0
                    };
                    println!(
                        "  DIR path={} pos=({:.4},{:.4}) radius={:.4} parent_radius={:.4} dist_to_parent={:.4}",
                        path, node.pos.x, node.pos.y, node.dir_radius, node.parent_radius, dist_p
                    );
                }
            }

            if gource.stop_position_reached {
                break;
            }
        }

        frame += 1;
    }
}
