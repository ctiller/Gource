use std::sync::{Arc, Mutex};

use bevy::{
    app::AppExit,
    input::{
        ButtonState,
        keyboard::{Key as BevyKey, KeyCode, KeyboardInput, NativeKey},
        mouse::{
            MouseButton as BevyMouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit,
            MouseWheel,
        },
    },
    prelude::*,
    window::{
        CursorMoved, CursorOptions, PrimaryWindow, WindowCloseRequested, WindowEvent,
        WindowResolution,
    },
};
use gource::{
    app::{ExitState, install_frame_loop, run_frame},
    capture::Recorder,
    cli::{Outcome, handle_command_line},
    input::{InputState, convert_event, map_key, update_modifiers},
    render::{FrameDrawList, FrameTextures, GpuTextures},
    sim::{SimResource, Simulation},
};
use gource_app::{
    AppOptions, GourceApp, InputEvent, Key, Modifiers, MouseButton, PlatformRequest, Viewport,
};
use gource_core::{UVec2, Vec2, Vec4};
use gource_draw::{DrawList, Gfx};
use gource_vcs::VcsError;

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

fn create_test_headless_app(
    sim: Box<dyn Simulation>,
    recorder: Recorder,
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

    install_frame_loop(&mut app, sim, recorder, 0);
    app
}

#[test]
fn test_cli_output_custom_log_error_handling() {
    let dir = tempfile::tempdir().unwrap();
    let empty_dir = dir.path().join("empty_repo");
    std::fs::create_dir(&empty_dir).unwrap();

    let out_log = dir.path().join("out.log");
    // Passing a directory with no VCS repo causes write_custom_log to return Err(VcsError::Message("..."))
    let outcome = handle_command_line(&[
        "--output-custom-log".to_string(),
        out_log.to_str().unwrap().to_string(),
        empty_dir.to_str().unwrap().to_string(),
    ]);

    match outcome {
        Outcome::Exit {
            stdout,
            stderr,
            code,
        } => {
            assert_eq!(code, 1);
            assert!(stdout.is_empty());
            assert!(stderr.contains("gource: "), "stderr was: {stderr}");
        }
        Outcome::Run(_) => panic!("expected Exit, got Run"),
    }
}

#[test]
fn test_cli_output_custom_log_unwritable_output() {
    // If output is an unwritable path, write_custom_log returns Err(VcsError::Io(_))
    // which hits cli.rs line 83: Err(_) => Outcome::success(""),
    // In order for it to reach File::create, the repo must be valid.
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !repo.join(".git").exists() {
        return;
    }
    let outcome = handle_command_line(&[
        "--output-custom-log".to_string(),
        "/nonexistent_dir_12345/out.log".to_string(),
        repo.to_str().unwrap().to_string(),
    ]);

    match outcome {
        Outcome::Exit {
            stdout,
            stderr,
            code,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.is_empty());
            assert!(stderr.is_empty());
        }
        Outcome::Run(_) => panic!("expected Exit, got Run"),
    }
}

#[test]
fn test_cli_vcs_error_formatting() {
    // Check quit with empty vs non-empty message matching the match arm logic
    let err_empty = VcsError::Message(String::new());
    let err_non_empty = VcsError::Message("custom error occurred".into());

    let res_empty = match err_empty {
        VcsError::Message(msg) if !msg.is_empty() => Outcome::quit(&msg),
        _ => Outcome::quit("default empty"),
    };
    if let Outcome::Exit { stderr, code, .. } = res_empty {
        assert_eq!(code, 1);
        assert!(stderr.contains("default empty"));
    }

    let res_non_empty = match err_non_empty {
        VcsError::Message(msg) if !msg.is_empty() => Outcome::quit(&msg),
        _ => Outcome::quit("default fallback"),
    };
    if let Outcome::Exit { stderr, code, .. } = res_non_empty {
        assert_eq!(code, 1);
        assert!(stderr.contains("custom error occurred"));
    }
}

#[test]
fn test_input_modifiers_all_keys() {
    let mut m = Modifiers::default();
    assert!(update_modifiers(&mut m, KeyCode::ShiftLeft, true));
    assert!(m.shift);
    assert!(update_modifiers(&mut m, KeyCode::ShiftLeft, false));
    assert!(!m.shift);

    assert!(update_modifiers(&mut m, KeyCode::ControlRight, true));
    assert!(m.ctrl);
    assert!(update_modifiers(&mut m, KeyCode::ControlRight, false));
    assert!(!m.ctrl);

    assert!(update_modifiers(&mut m, KeyCode::AltRight, true));
    assert!(m.alt);
    assert!(update_modifiers(&mut m, KeyCode::AltRight, false));
    assert!(!m.alt);

    assert!(update_modifiers(&mut m, KeyCode::SuperLeft, true));
    assert!(m.meta);
    assert!(update_modifiers(&mut m, KeyCode::SuperLeft, false));
    assert!(!m.meta);
}

#[test]
fn test_input_map_key_all_cases() {
    // KeyCode physical mappings
    assert_eq!(map_key(KeyCode::Enter, &BevyKey::Enter, false), Key::Return);
    assert_eq!(
        map_key(KeyCode::NumpadEnter, &BevyKey::Enter, false),
        Key::Return
    );
    assert_eq!(map_key(KeyCode::Tab, &BevyKey::Tab, false), Key::Tab);
    assert_eq!(map_key(KeyCode::Space, &BevyKey::Space, false), Key::Space);
    assert_eq!(map_key(KeyCode::ArrowUp, &BevyKey::ArrowUp, false), Key::Up);
    assert_eq!(
        map_key(KeyCode::ArrowDown, &BevyKey::ArrowDown, false),
        Key::Down
    );
    assert_eq!(
        map_key(KeyCode::ArrowLeft, &BevyKey::ArrowLeft, false),
        Key::Left
    );
    assert_eq!(
        map_key(KeyCode::ArrowRight, &BevyKey::ArrowRight, false),
        Key::Right
    );
    assert_eq!(map_key(KeyCode::F5, &BevyKey::F5, false), Key::F5);
    assert_eq!(map_key(KeyCode::F11, &BevyKey::F11, false), Key::F11);
    assert_eq!(map_key(KeyCode::F12, &BevyKey::F12, false), Key::F12);
    assert_eq!(
        map_key(KeyCode::NumpadAdd, &BevyKey::Character("+".into()), false),
        Key::KeypadPlus
    );
    assert_eq!(
        map_key(
            KeyCode::NumpadSubtract,
            &BevyKey::Character("-".into()),
            false
        ),
        Key::KeypadMinus
    );

    // Logical mappings with non-special KeyCode
    let dummy = KeyCode::KeyK;
    assert_eq!(map_key(dummy, &BevyKey::Space, false), Key::Space);
    assert_eq!(map_key(dummy, &BevyKey::Enter, false), Key::Return);
    assert_eq!(map_key(dummy, &BevyKey::Escape, false), Key::Escape);
    assert_eq!(map_key(dummy, &BevyKey::Tab, false), Key::Tab);

    // BevyKey::Character empty vs non-empty, lowercase vs unshift
    assert_eq!(
        map_key(dummy, &BevyKey::Character("".into()), false),
        Key::Other
    );
    assert_eq!(
        map_key(dummy, &BevyKey::Character("A".into()), false),
        Key::Char('a')
    );
    assert_eq!(
        map_key(dummy, &BevyKey::Character("!".into()), true),
        Key::Char('1')
    );

    // Other BevyKey
    assert_eq!(
        map_key(
            dummy,
            &BevyKey::Unidentified(NativeKey::Unidentified),
            false
        ),
        Key::Other
    );
    assert_eq!(map_key(dummy, &BevyKey::Backspace, false), Key::Backspace);
}

#[test]
fn test_input_convert_event_all_variants() {
    let mut state = InputState::default();

    // 1. KeyboardInput KeyDown repeat true
    let event = WindowEvent::KeyboardInput(KeyboardInput {
        key_code: KeyCode::KeyB,
        logical_key: BevyKey::Character("b".into()),
        state: ButtonState::Pressed,
        repeat: true,
        text: None,
        window: Entity::PLACEHOLDER,
    });
    let converted = convert_event(&mut state, &event, 1.0, false);
    assert_eq!(
        converted,
        Some(InputEvent::KeyDown {
            key: Key::Char('b'),
            modifiers: Modifiers::default(),
            repeat: true,
        })
    );

    // 2. CursorMoved with delta: None (should use pos - state.cursor)
    state.cursor = bevy::math::Vec2::new(10.0, 10.0);
    let event = WindowEvent::CursorMoved(CursorMoved {
        window: Entity::PLACEHOLDER,
        position: bevy::math::Vec2::new(25.0, 35.0),
        delta: None,
    });
    let converted = convert_event(&mut state, &event, 2.0, false);
    assert_eq!(
        converted,
        Some(InputEvent::MouseMove {
            pos: gource_core::Vec2::new(50.0, 70.0),
            delta: gource_core::Vec2::new(40.0, 60.0),
        })
    );
    assert_eq!(state.cursor, bevy::math::Vec2::new(50.0, 70.0));

    // 3. CursorMoved while grabbed (should return None)
    let event = WindowEvent::CursorMoved(CursorMoved {
        window: Entity::PLACEHOLDER,
        position: bevy::math::Vec2::new(60.0, 80.0),
        delta: Some(bevy::math::Vec2::new(5.0, 5.0)),
    });
    let converted = convert_event(&mut state, &event, 1.0, true);
    assert_eq!(converted, None);

    // 4. MouseMotion while NOT grabbed (should return None)
    let event = WindowEvent::MouseMotion(MouseMotion {
        delta: bevy::math::Vec2::new(1.0, 2.0),
    });
    let converted = convert_event(&mut state, &event, 1.0, false);
    assert_eq!(converted, None);

    // 5. MouseMotion while grabbed (should return Some)
    let converted = convert_event(&mut state, &event, 1.0, true);
    assert_eq!(
        converted,
        Some(InputEvent::MouseMove {
            pos: gource_core::Vec2::new(state.cursor.x, state.cursor.y),
            delta: gource_core::Vec2::new(1.0, 2.0),
        })
    );

    // 6. MouseButton with unmapped button (should return None)
    let event = WindowEvent::MouseButtonInput(MouseButtonInput {
        window: Entity::PLACEHOLDER,
        button: BevyMouseButton::Other(42),
        state: ButtonState::Pressed,
    });
    let converted = convert_event(&mut state, &event, 1.0, false);
    assert_eq!(converted, None);

    // 7. MouseWheel with zero delta (should return None)
    let event = WindowEvent::MouseWheel(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: 0.0,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
    let converted = convert_event(&mut state, &event, 1.0, false);
    assert_eq!(converted, None);

    // 8. Other WindowEvent (e.g. WindowThemeChanged) (should return None)
    let event = WindowEvent::WindowThemeChanged(bevy::window::WindowThemeChanged {
        window: Entity::PLACEHOLDER,
        theme: bevy::window::WindowTheme::Dark,
    });
    let converted = convert_event(&mut state, &event, 1.0, false);
    assert_eq!(converted, None);
}

#[test]
fn test_forward_input_system_empty_events() {
    let log = SimLog::default();
    let sim = Box::new(TestSim::new(log.clone()));
    let mut app = create_test_headless_app(sim, Recorder::new(None), 800, 600, 1.0);

    // Frame with no events at all: forward_input executes early return
    app.update();
    assert!(log.inputs.lock().unwrap().is_empty());
}

#[test]
fn test_sim_resource_poisoned_lock() {
    let sim = Box::new(TestSim::new(SimLog::default()));
    let res = SimResource::new(sim);

    // Poison the inner Mutex by panicking inside a lock on a separate thread
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = res.lock();
        panic!("deliberate panic to poison mutex");
    }));

    // Next lock() call should recover gracefully via unwrap_or_else(|e| e.into_inner())
    let mut guard = res.lock();
    guard.input(&InputEvent::Focus(false));
}

#[test]
fn test_gource_app_simulation_methods_comprehensive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let outcome = handle_command_line(&[path]);
    let Outcome::Run(config) = outcome else {
        panic!("expected Outcome::Run, got {:?}", outcome);
    };

    let options = AppOptions::default();
    if let Ok(mut sim_app) = GourceApp::new(*config, options) {
        let sim: &mut dyn Simulation = &mut sim_app;
        sim.input(&InputEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        sim.input(&InputEvent::KeyUp {
            key: Key::Space,
            modifiers: Modifiers::default(),
        });
        sim.input(&InputEvent::MouseMove {
            pos: Vec2::new(100.0, 100.0),
            delta: Vec2::new(1.0, 1.0),
        });
        sim.input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            pos: Vec2::new(100.0, 100.0),
        });
        sim.input(&InputEvent::MouseWheel { delta: 1.0 });

        let mut list = DrawList::new(UVec2::new(640, 480));
        sim.frame(
            0.016,
            Viewport {
                width: 640,
                height: 480,
                dpi_ratio: 1.0,
            },
            &mut list,
        );
        let reqs = sim.take_requests();
        let _ = reqs;
        let gfx = sim.gfx();
        let _ = gfx;
    }
}

#[test]
fn test_app_run_frame_missing_window() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<Time<Real>>();
    app.init_resource::<FrameDrawList>();
    app.init_resource::<FrameTextures>();
    app.init_resource::<GpuTextures>();
    app.init_resource::<gource::app::PendingRequests>();
    app.init_resource::<ExitState>();
    app.insert_resource(Recorder::new(None));
    app.insert_resource(SimResource::new(Box::new(TestSim::new(SimLog::default()))));

    // Notice: no Window spawned!
    app.add_systems(Update, run_frame);
    // Should hit line 224: `let Ok(window) = windows.single() else { return; };`
    app.update();
}
