use std::sync::{Arc, Mutex};

use bevy::{
    app::AppExit,
    input::{
        ButtonState,
        keyboard::{Key as BevyKey, KeyCode, KeyboardInput},
        mouse::{
            MouseButton as BevyMouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit,
            MouseWheel,
        },
        touch::TouchPhase,
    },
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    window::{
        CursorGrabMode, CursorMoved, CursorOptions, PrimaryWindow, WindowCloseRequested,
        WindowEvent, WindowFocused, WindowMode, WindowResolution,
    },
};
use glam::{UVec2, Vec2, Vec4};
use gource::{
    app::{ExitState, install_frame_loop},
    capture::Recorder,
    render::{FrameDrawList, FrameTextures, GpuTextures},
    sim::Simulation,
};
use gource_draw::{DrawList, Gfx, PpmExporter};
use gource_sim::{InputEvent, Key, MouseButton, PlatformRequest, Viewport};

#[derive(Default, Clone)]
struct SimLog {
    inputs: Arc<Mutex<Vec<InputEvent>>>,
    frames: Arc<Mutex<Vec<(f32, Viewport)>>>,
    requests: Arc<Mutex<Vec<PlatformRequest>>>,
}

struct TestSim {
    gfx: Gfx,
    log: SimLog,
}

impl TestSim {
    fn new(log: SimLog) -> Self {
        Self {
            gfx: Gfx::new(),
            log,
        }
    }
}

impl Simulation for TestSim {
    fn input(&mut self, event: &InputEvent) {
        self.log.inputs.lock().unwrap().push(*event);
    }

    fn frame(&mut self, dt: f32, viewport: Viewport, list: &mut DrawList) {
        self.log.frames.lock().unwrap().push((dt, viewport));
        list.reset(
            UVec2::new(viewport.width, viewport.height),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
    }

    fn take_requests(&mut self) -> Vec<PlatformRequest> {
        std::mem::take(&mut self.log.requests.lock().unwrap())
    }

    fn gfx(&self) -> &Gfx {
        &self.gfx
    }
}

fn create_headless_app(
    sim: Box<dyn Simulation>,
    recorder: Recorder,
    screen: i32,
    width: u32,
    height: u32,
    scale_factor: f32,
) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<WindowEvent>();
    app.add_message::<WindowCloseRequested>();
    app.add_message::<AppExit>();

    let mut resolution = WindowResolution::new(width, height);
    resolution.set_scale_factor(scale_factor);

    app.world_mut().spawn((
        Window {
            resolution,
            ..default()
        },
        CursorOptions::default(),
        PrimaryWindow,
    ));

    app.init_resource::<FrameDrawList>();
    app.init_resource::<FrameTextures>();
    app.init_resource::<GpuTextures>();

    install_frame_loop(&mut app, sim, recorder, screen);
    app
}

#[test]
fn test_input_forwarding_ordered() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_headless_app(sim, Recorder::new(None), 0, 800, 600, 2.0);

    // Initial tick to stabilize time/frame
    app.update();
    log.inputs.lock().unwrap().clear();

    // 1. KeyDown and KeyUp
    app.world_mut()
        .write_message(WindowEvent::KeyboardInput(KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key: BevyKey::Character("a".into()),
            state: ButtonState::Pressed,
            repeat: false,
            text: None,
            window: Entity::PLACEHOLDER,
        }));
    app.world_mut()
        .write_message(WindowEvent::KeyboardInput(KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key: BevyKey::Character("a".into()),
            state: ButtonState::Released,
            repeat: false,
            text: None,
            window: Entity::PLACEHOLDER,
        }));

    // 2. CursorMoved
    app.world_mut()
        .write_message(WindowEvent::CursorMoved(CursorMoved {
            window: Entity::PLACEHOLDER,
            position: Vec2::new(10.0, 20.0),
            delta: Some(Vec2::new(1.0, 2.0)),
        }));

    // 3. MouseButtonInput
    app.world_mut()
        .write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
            window: Entity::PLACEHOLDER,
            button: BevyMouseButton::Left,
            state: ButtonState::Pressed,
        }));

    // 4. MouseWheel
    app.world_mut()
        .write_message(WindowEvent::MouseWheel(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: 1.0,
            window: Entity::PLACEHOLDER,
            phase: TouchPhase::Moved,
        }));

    // 5. WindowFocused
    app.world_mut()
        .write_message(WindowEvent::WindowFocused(WindowFocused {
            window: Entity::PLACEHOLDER,
            focused: true,
        }));

    app.update();

    let inputs = log.inputs.lock().unwrap().clone();
    assert_eq!(inputs.len(), 6);

    match &inputs[0] {
        InputEvent::KeyDown { key, repeat, .. } => {
            assert_eq!(*key, Key::Char('a'));
            assert!(!repeat);
        }
        other => panic!("expected KeyDown, got {:?}", other),
    }

    match &inputs[1] {
        InputEvent::KeyUp { key, .. } => {
            assert_eq!(*key, Key::Char('a'));
        }
        other => panic!("expected KeyUp, got {:?}", other),
    }

    match &inputs[2] {
        InputEvent::MouseMove { pos, delta } => {
            // scale_factor is 2.0, so pos should be 20.0, 40.0 and delta 2.0, 4.0
            assert_eq!(*pos, Vec2::new(20.0, 40.0));
            assert_eq!(*delta, Vec2::new(2.0, 4.0));
        }
        other => panic!("expected MouseMove, got {:?}", other),
    }

    match &inputs[3] {
        InputEvent::MouseButton {
            button,
            pressed,
            pos,
        } => {
            assert_eq!(*button, MouseButton::Left);
            assert!(pressed);
            assert_eq!(*pos, Vec2::new(20.0, 40.0));
        }
        other => panic!("expected MouseButton, got {:?}", other),
    }

    match &inputs[4] {
        InputEvent::MouseWheel { delta } => {
            assert_eq!(*delta, 1.0);
        }
        other => panic!("expected MouseWheel, got {:?}", other),
    }

    match &inputs[5] {
        InputEvent::Focus(focused) => {
            assert!(focused);
        }
        other => panic!("expected Focus, got {:?}", other),
    }
}

#[test]
fn test_grabbed_cursor_motion_only_from_mouse_motion() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_headless_app(sim, Recorder::new(None), 0, 800, 600, 1.0);

    // Grab the cursor
    let mut cursor_query = app.world_mut().query::<&mut CursorOptions>();
    let mut cursor = cursor_query.single_mut(app.world_mut()).unwrap();
    cursor.grab_mode = CursorGrabMode::Locked;

    app.update();
    log.inputs.lock().unwrap().clear();

    // CursorMoved should be ignored when grabbed
    app.world_mut()
        .write_message(WindowEvent::CursorMoved(CursorMoved {
            window: Entity::PLACEHOLDER,
            position: Vec2::new(50.0, 50.0),
            delta: Some(Vec2::new(5.0, 5.0)),
        }));

    // MouseMotion should be processed
    app.world_mut()
        .write_message(WindowEvent::MouseMotion(MouseMotion {
            delta: Vec2::new(3.0, 4.0),
        }));

    app.update();

    let inputs = log.inputs.lock().unwrap().clone();
    assert_eq!(inputs.len(), 1);
    match &inputs[0] {
        InputEvent::MouseMove { delta, .. } => {
            assert_eq!(*delta, Vec2::new(3.0, 4.0));
        }
        other => panic!("expected MouseMove from MouseMotion, got {:?}", other),
    }
}

#[test]
fn test_run_frame_conditions() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_headless_app(sim, Recorder::new(None), 0, 800, 600, 1.0);

    app.update();
    assert_eq!(log.frames.lock().unwrap().len(), 1);

    // Zero-size window skips run_frame
    let mut win_query = app.world_mut().query::<&mut Window>();
    win_query
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set_physical_resolution(0, 0);

    app.update();
    assert_eq!(log.frames.lock().unwrap().len(), 1); // Not incremented

    // Reset resolution
    win_query
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set_physical_resolution(800, 600);
    app.update();
    assert_eq!(log.frames.lock().unwrap().len(), 2);

    // ExitState quitting skips run_frame
    app.world_mut().resource_mut::<ExitState>().quit();
    app.update();
    assert_eq!(log.frames.lock().unwrap().len(), 2);
}

#[test]
fn test_window_close_request() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_headless_app(sim, Recorder::new(None), 0, 800, 600, 1.0);

    assert!(!app.world().resource::<ExitState>().is_quitting());
    app.world_mut().write_message(WindowCloseRequested {
        window: Entity::PLACEHOLDER,
    });
    app.update();
    assert!(app.world().resource::<ExitState>().is_quitting());
}

#[test]
fn test_handle_requests_execution() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let dir = tempfile::tempdir().unwrap();
    let ppm_path = dir.path().join("out.ppm");
    let exporter = PpmExporter::new(ppm_path.to_str().unwrap()).unwrap();
    let mut app = create_headless_app(
        sim,
        Recorder::new(Some(Box::new(exporter))),
        1,
        800,
        600,
        1.0,
    );

    // Queue requests in the simulation
    let snap_path = dir.path().join("snap.png");
    log.requests.lock().unwrap().extend([
        PlatformRequest::ToggleFullscreen,
        PlatformRequest::ToggleFrameless,
        PlatformRequest::SetCursorVisible(false),
        PlatformRequest::SetCursorGrab(true),
        PlatformRequest::WarpCursor(Vec2::new(100.0, 200.0)),
        PlatformRequest::CaptureFrame,
        PlatformRequest::Screenshot {
            path: snap_path.clone(),
            with_alpha: true,
        },
    ]);

    app.update();

    let mut win_query = app.world_mut().query::<(&Window, &CursorOptions)>();
    let (window, cursor) = win_query.single(app.world()).unwrap();

    // ToggleFullscreen was executed
    assert!(matches!(window.mode, WindowMode::BorderlessFullscreen(_)));
    // SetCursorVisible and SetCursorGrab executed
    assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
    assert!(!cursor.visible);
    // WarpCursor executed
    assert_eq!(
        window.physical_cursor_position(),
        Some(Vec2::new(100.0, 200.0))
    );

    // Screenshot entity was spawned
    let mut screenshot_query = app.world_mut().query::<(Entity, &Screenshot)>();
    let screenshots: Vec<Entity> = screenshot_query.iter(app.world()).map(|(e, _)| e).collect();
    assert_eq!(screenshots.len(), 1);

    // Simulate ScreenshotCaptured trigger
    let shot_entity = screenshots[0];
    let img = Image::new(
        bevy::render::render_resource::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::MAIN_WORLD,
    );
    app.world_mut().trigger(ScreenshotCaptured {
        entity: shot_entity,
        image: img,
    });

    // Check that write_frames flushes the recorder
    app.update();
    assert_eq!(app.world().resource::<Recorder>().frames_written(), 1);

    // Wait for the spawned screenshot thread to write the PNG file
    let start = std::time::Instant::now();
    while !snap_path.exists() && start.elapsed() < std::time::Duration::from_secs(2) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(snap_path.exists());

    // Test screenshot with an invalid / unwritable path (triggers save_png error logging line 302)
    let bad_path = std::path::PathBuf::from("/nonexistent_dir_123/screenshot.png");
    log.requests
        .lock()
        .unwrap()
        .push(PlatformRequest::Screenshot {
            path: bad_path,
            with_alpha: false,
        });
    app.update();
    let mut screenshot_query = app.world_mut().query::<(Entity, &Screenshot)>();
    let entities: Vec<Entity> = screenshot_query.iter(app.world()).map(|(e, _)| e).collect();
    if let Some(&bad_entity) = entities.first() {
        let dummy_img = Image::new(
            bevy::render::render_resource::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            vec![0, 0, 0, 255],
            bevy::render::render_resource::TextureFormat::Rgba8Unorm,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        );
        app.world_mut().trigger(ScreenshotCaptured {
            entity: bad_entity,
            image: dummy_img,
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Test screenshot with unsupported image format (triggers line 306 error! logging)
    let unsupp_path = dir.path().join("unsupp.png");
    log.requests
        .lock()
        .unwrap()
        .push(PlatformRequest::Screenshot {
            path: unsupp_path,
            with_alpha: false,
        });
    app.update();
    let mut screenshot_query = app.world_mut().query::<(Entity, &Screenshot)>();
    let entities: Vec<Entity> = screenshot_query.iter(app.world()).map(|(e, _)| e).collect();
    if let Some(&unsupp_entity) = entities.first() {
        // Unsupported format: Depth32Float
        let unsupp_img = Image::new(
            bevy::render::render_resource::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            vec![0, 0, 0, 0],
            bevy::render::render_resource::TextureFormat::Depth32Float,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        );
        app.world_mut().trigger(ScreenshotCaptured {
            entity: unsupp_entity,
            image: unsupp_img,
        });
    }

    // Test quit request
    log.requests.lock().unwrap().push(PlatformRequest::Quit);
    app.update();
    assert!(app.world().resource::<ExitState>().is_quitting());
}

#[test]
fn test_fatal_and_help_requests() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_headless_app(sim, Recorder::new(None), 0, 800, 600, 1.0);

    log.requests
        .lock()
        .unwrap()
        .push(PlatformRequest::Fatal("test fatal error".into()));
    app.update();
    assert_eq!(app.world().resource::<ExitState>().code(), 1);

    let log2 = SimLog::default();
    let sim2 = Box::new(TestSim::new(log2.clone()));
    let mut app2 = create_headless_app(sim2, Recorder::new(None), 0, 800, 600, 1.0);

    log2.requests
        .lock()
        .unwrap()
        .push(PlatformRequest::ShowHelpAndExit);
    app2.update();
    assert_eq!(app2.world().resource::<ExitState>().code(), 0);
    assert!(app2.world().resource::<ExitState>().is_quitting());
}

#[test]
fn test_input_edge_cases_and_all_keys() {
    use bevy::input::keyboard::NativeKey;
    use gource::input::{map_key, map_mouse_button, unshift_us, update_modifiers, wheel_delta};
    use gource_sim::Modifiers;

    // Test unshift_us for all mapped characters
    let shifted = "+_><{}?:\"~|!@#$%^&*()";
    let unshifted = "=-.,[]/;'`\\1234567890";
    for (s, u) in shifted.chars().zip(unshifted.chars()) {
        assert_eq!(unshift_us(s), u);
    }
    assert_eq!(unshift_us('z'), 'z');

    // Test map_key for all special keys
    assert_eq!(map_key(KeyCode::Tab, &BevyKey::Tab, false), Key::Tab);
    assert_eq!(map_key(KeyCode::Space, &BevyKey::Space, false), Key::Space);
    assert_eq!(map_key(KeyCode::ArrowUp, &BevyKey::ArrowUp, false), Key::Up);
    assert_eq!(
        map_key(KeyCode::ArrowDown, &BevyKey::ArrowDown, false),
        Key::Down
    );
    assert_eq!(
        map_key(KeyCode::ArrowRight, &BevyKey::ArrowRight, false),
        Key::Right
    );
    assert_eq!(map_key(KeyCode::F5, &BevyKey::F5, false), Key::F5);
    assert_eq!(map_key(KeyCode::F12, &BevyKey::F12, false), Key::F12);

    // Test logical fallbacks in map_key
    assert_eq!(map_key(KeyCode::KeyZ, &BevyKey::Space, false), Key::Space);
    assert_eq!(map_key(KeyCode::KeyZ, &BevyKey::Enter, false), Key::Return);
    assert_eq!(map_key(KeyCode::KeyZ, &BevyKey::Escape, false), Key::Escape);
    assert_eq!(map_key(KeyCode::KeyZ, &BevyKey::Tab, false), Key::Tab);
    assert_eq!(
        map_key(
            KeyCode::KeyZ,
            &BevyKey::Unidentified(NativeKey::Unidentified),
            false
        ),
        Key::Other
    );

    // Test update_modifiers for Ctrl and Super
    let mut m = Modifiers::default();
    assert!(update_modifiers(&mut m, KeyCode::ControlLeft, true));
    assert!(m.ctrl);
    assert!(update_modifiers(&mut m, KeyCode::SuperRight, true));
    assert!(m.meta);

    // Test mouse button Right and unmapped
    assert_eq!(
        map_mouse_button(BevyMouseButton::Right),
        Some(MouseButton::Right)
    );
    assert_eq!(map_mouse_button(BevyMouseButton::Other(99)), None);

    // Test wheel_delta
    assert_eq!(wheel_delta(MouseScrollUnit::Line, 2.5), 2.5);
    assert_eq!(wheel_delta(MouseScrollUnit::Pixel, 100.0), 1.0);
}

#[test]
fn test_warmup_system_headless() {
    use gource::warmup::{Warmup, run_warmup};

    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_headless_app(sim, Recorder::new(None), 0, 800, 600, 1.0);

    // Add Warmup resource and run_warmup system
    app.init_resource::<Warmup>();
    app.add_systems(Update, run_warmup);

    // Frame 1: run_warmup steps and requests probe (spawns Screenshot observer)
    app.update();
    assert_eq!(app.world().resource::<Warmup>().probes(), 1);

    // Trigger ScreenshotCaptured with probe visible
    let mut screenshot_query = app.world_mut().query::<(Entity, &Screenshot)>();
    let entities: Vec<Entity> = screenshot_query.iter(app.world()).map(|(e, _)| e).collect();
    assert_eq!(entities.len(), 1);

    // Construct image with probe visible (square at 1,1 and green glow at 16,4)
    let width = 32;
    let height = 16;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    // Set square pixel at (1, 1) to white
    let i1 = (width + 1) as usize * 4;
    rgba[i1..i1 + 3].copy_from_slice(&[255, 255, 255]);
    // Set glow pixel at (16, 4) to green
    let i2 = (4 * width + 16) as usize * 4;
    rgba[i2..i2 + 3].copy_from_slice(&[0, 200, 0]);

    let img = Image::new(
        bevy::render::render_resource::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        rgba,
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::MAIN_WORLD,
    );

    app.world_mut().trigger(ScreenshotCaptured {
        entity: entities[0],
        image: img,
    });

    // Next update: Warmup becomes ready
    app.update();
    assert!(app.world().resource::<Warmup>().is_ready());

    // Update again when already ready (tests line 155 early return)
    app.update();

    // Test run_warmup when exit is quitting (tests line 156 early return)
    app.world_mut().resource_mut::<Warmup>().probes();
    app.world_mut().resource_mut::<ExitState>().quit();
    app.update();

    // Test zero-size viewport in run_warmup (lines 173-174)
    let mut zero_app = create_headless_app(
        Box::new(TestSim::new(SimLog::default())),
        Recorder::new(None),
        0,
        0,
        0,
        1.0,
    );
    zero_app.init_resource::<Warmup>();
    zero_app.add_systems(Update, run_warmup);
    zero_app.update();
    assert_eq!(zero_app.world().resource::<Warmup>().probes(), 0);

    // Test missing window in run_warmup (lines 169-170)
    let mut no_win_app = App::new();
    no_win_app.add_plugins(MinimalPlugins);
    no_win_app.init_resource::<Warmup>();
    no_win_app.init_resource::<ExitState>();
    no_win_app.init_resource::<FrameDrawList>();
    no_win_app.init_resource::<FrameTextures>();
    no_win_app.init_resource::<GpuTextures>();
    no_win_app.insert_resource(gource::sim::SimResource::new(Box::new(TestSim::new(
        SimLog::default(),
    ))));
    no_win_app.add_systems(Update, run_warmup);
    no_win_app.update();
    assert_eq!(no_win_app.world().resource::<Warmup>().probes(), 0);

    // Test when warmup.step() returns false and warmup.is_ready() (lines 176-179)
    let mut ready_app = create_headless_app(
        Box::new(TestSim::new(SimLog::default())),
        Recorder::new(None),
        0,
        800,
        600,
        1.0,
    );
    ready_app.insert_resource(Warmup::ready());
    // Directly invoke run_warmup when Warmup::ready() is present
    ready_app.add_systems(Update, run_warmup);
    ready_app.update();
    assert!(ready_app.world().resource::<Warmup>().is_ready());
}

#[test]
fn test_finish_exit_with_error_and_timeout() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let dir = tempfile::tempdir().unwrap();
    let ppm_path = dir.path().join("out.ppm");
    let exporter = PpmExporter::new(ppm_path.to_str().unwrap()).unwrap();
    let mut app = create_headless_app(
        sim,
        Recorder::new(Some(Box::new(exporter))),
        0,
        800,
        600,
        1.0,
    );

    // Queue frame capture so recorder has in_flight > 0
    app.world_mut()
        .resource_mut::<Recorder>()
        .next_frame_index();
    assert_eq!(app.world().resource::<Recorder>().in_flight(), 1);

    // Quit with in_flight frame
    app.world_mut().resource_mut::<ExitState>().quit();

    // finish_exit will not finish while in_flight > 0 and deadline not passed
    app.update();
    assert_eq!(app.world().resource::<Recorder>().in_flight(), 1);

    // Deliver frame with mismatched dimensions to cause exporter error (lines 246-248 and 319-322)
    let sink = app.world().resource::<Recorder>().sink();
    sink.lock().unwrap().insert(
        0,
        Some(gource::capture::Frame {
            width: 10,
            height: 10,
            rgba: vec![255, 0, 0, 255], // wrong buffer size for 10x10 -> write_frame_rgba returns error!
        }),
    );
    app.update();
    assert_eq!(app.world().resource::<Recorder>().in_flight(), 0);
    // Recorder has error now, and finish_exit ran and failed
    assert_eq!(app.world().resource::<ExitState>().code(), 1);
}

#[test]
fn test_gource_app_simulation_methods() {
    use gource::cli::Outcome;
    use gource_sim::{AppOptions, GourceApp};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let outcome = gource::cli::handle_command_line(&[path]);
    let Outcome::Run(config) = outcome else {
        panic!("expected Outcome::Run, got {:?}", outcome);
    };

    let options = AppOptions::default();
    let gource_app = GourceApp::new(*config, options);
    if let Ok(mut sim_app) = gource_app {
        let sim: &mut dyn Simulation = &mut sim_app;
        sim.input(&InputEvent::Focus(true));
        let mut list = DrawList::new(UVec2::new(800, 600));
        sim.frame(
            0.016,
            Viewport {
                width: 800,
                height: 600,
                dpi_ratio: 1.0,
            },
            &mut list,
        );
        let _ = sim.take_requests();
        let _ = sim.gfx();
    }
}
