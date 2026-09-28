use glam::{Vec2, Vec3};
use gource_core::Bounds2D;
use gource_settings::GourceSettings;
use gource_sim::camera::{CameraCrop, CameraFocusData, ZoomCamera};

#[test]
fn test_default_constructor() {
    let cam = ZoomCamera::default();
    assert_eq!(cam.pos(), Vec3::new(0.0, 0.0, -100.0));
    assert_eq!(cam.dest(), Vec3::new(0.0, 0.0, -100.0));
    assert_eq!(cam.target(), Vec3::ZERO);
    assert_eq!(cam.up(), Vec3::new(0.0, -1.0, 0.0));
    assert_eq!(cam.min_distance(), 50.0);
    assert_eq!(cam.max_distance(), 10000.0);
    assert_eq!(cam.fov(), 90.0);
    assert_eq!(cam.znear(), 0.1);
    assert_eq!(cam.zfar(), 10001.0);
    assert_eq!(cam.padding(), 1.0);
    assert_eq!(cam.speed(), 1.0);
    assert!(!cam.is_locked_on());
}

#[test]
fn test_custom_constructor_and_accessors() {
    let start = Vec3::new(10.0, 20.0, -300.0);
    let target = Vec3::new(5.0, 6.0, 0.0);
    let mut cam = ZoomCamera::new(start, target, 20.0, 5000.0);

    assert_eq!(cam.pos(), start);
    assert_eq!(cam.dest(), start);
    assert_eq!(cam.target(), target);
    assert_eq!(cam.min_distance(), 20.0);
    assert_eq!(cam.max_distance(), 5000.0);
    assert_eq!(cam.zfar(), 5001.0);

    cam.set_min_distance(30.0);
    assert_eq!(cam.min_distance(), 30.0);

    cam.set_max_distance(8000.0);
    assert_eq!(cam.max_distance(), 8000.0);
    assert_eq!(cam.zfar(), 8001.0);

    cam.set_speed(2.5);
    assert_eq!(cam.speed(), 2.5);

    cam.set_padding(1.2);
    assert_eq!(cam.padding(), 1.2);

    cam.set_distance(600.0);
    assert_eq!(cam.dest().z, -600.0);
}

#[test]
fn test_from_settings() {
    let mut settings = GourceSettings {
        camera_zoom_min: 100.0,
        camera_zoom_max: 20000.0,
        camera_zoom_default: 400.0,
        padding: 1.15,
        ..Default::default()
    };
    settings.crop_vertical = true;

    let cam = ZoomCamera::from_settings(&settings);
    assert_eq!(cam.pos(), Vec3::new(0.0, 0.0, -400.0));
    assert_eq!(cam.dest(), Vec3::new(0.0, 0.0, -400.0));
    assert_eq!(cam.min_distance(), 100.0);
    assert_eq!(cam.max_distance(), 20000.0);
    assert_eq!(cam.padding(), 1.15);
    assert_eq!(CameraCrop::from_settings(&settings), CameraCrop::Vertical);

    settings.crop_vertical = false;
    settings.crop_horizontal = true;
    assert_eq!(CameraCrop::from_settings(&settings), CameraCrop::Horizontal);

    settings.crop_horizontal = false;
    assert_eq!(CameraCrop::from_settings(&settings), CameraCrop::None);
}

#[test]
fn test_reset_and_set_pos() {
    let start = Vec3::new(10.0, 20.0, -300.0);
    let target = Vec3::new(10.0, 20.0, 0.0);
    let mut cam = ZoomCamera::new(start, target, 50.0, 1000.0);

    // set_pos without keep_angle
    cam.set_pos(Vec3::new(50.0, 60.0, -400.0), false);
    assert_eq!(cam.pos(), Vec3::new(50.0, 60.0, -400.0));
    assert_eq!(cam.target(), target);

    // reset reverts to initial_pos and initial_target
    cam.reset();
    assert_eq!(cam.pos(), start);
    assert_eq!(cam.target(), target);

    // set_pos with keep_angle
    // dir = target - pos = (10, 20, 0) - (10, 20, -300) = (0, 0, 300)
    let new_pos = Vec3::new(100.0, 200.0, -500.0);
    cam.set_pos(new_pos, true);
    assert_eq!(cam.pos(), new_pos);
    assert_eq!(cam.target(), new_pos + Vec3::new(0.0, 0.0, 300.0));
}

#[test]
fn test_stop() {
    let mut cam = ZoomCamera::default();
    cam.set_distance(500.0);
    assert_ne!(cam.dest(), cam.pos());
    cam.stop();
    assert_eq!(cam.dest(), cam.pos());
}

#[test]
fn test_look_and_focus() {
    let cam = ZoomCamera::new(
        Vec3::new(10.0, 20.0, -100.0),
        Vec3::new(10.0, 20.0, 0.0),
        50.0,
        1000.0,
    );
    let (eye, tgt, up) = cam.look();
    assert_eq!(eye, Vec3::new(10.0, 20.0, -100.0));
    assert_eq!(tgt, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(up, Vec3::new(0.0, -1.0, 0.0));

    let custom_tgt = Vec3::new(1.0, 2.0, 3.0);
    let (eye2, tgt2, up2) = cam.look_at(custom_tgt);
    assert_eq!(eye2, cam.pos());
    assert_eq!(tgt2, custom_tgt);
    assert_eq!(up2, cam.up());

    let focus = cam.focus();
    assert_eq!(
        focus,
        CameraFocusData {
            fov: 90.0,
            znear: 0.1,
            zfar: 1001.0,
            eye: cam.pos(),
            target: cam.target(),
            up: cam.up(),
        }
    );
}

#[test]
fn test_adjust_arithmetic_golden_values() {
    // Hand-compute C++ values:
    // fov = 90 deg -> toa = tan(45 deg) * 2.0 = 1.0 * 2.0 = 2.0.
    // min_distance = 50.0, max_distance = 1000.0.
    // padding = 1.1.
    let mut cam = ZoomCamera::new(Vec3::new(0.0, 0.0, -100.0), Vec3::ZERO, 50.0, 1000.0);
    cam.set_padding(1.1);

    // Case 1: Bounds (-100, -50) to (100, 50).
    // width = 200, height = 100, centre = (0, 0).
    // bounds.width() * padding = 200 * 1.1 = 220.0.
    // bounds.height() * padding = 100 * 1.1 = 110.0.
    // Viewport: 1920 x 1080 (aspect ratio = 1920 / 1080 = 16 / 9 ~ 1.777778 >= 1.0).
    // Because aspect_ratio >= 1.0:
    // width /= aspect_ratio -> 220.0 / (1920 / 1080) = 220.0 * 1080 / 1920 = 123.75.
    // height remains 110.0.
    // width (123.75) >= height (110.0):
    // In None crop mode, width >= height -> distance = width / toa = 123.75 / 2.0 = 61.875.
    let bounds = Bounds2D::from_points(Vec2::new(-100.0, -50.0), Vec2::new(100.0, 50.0));
    let vp_wide = Vec2::new(1920.0, 1080.0);

    cam.adjust(&bounds, true, vp_wide, CameraCrop::None);
    assert_eq!(cam.dest().x, 0.0);
    assert_eq!(cam.dest().y, 0.0);
    assert!((cam.dest().z - (-61.875)).abs() < 1e-5);

    // If adjust_distance is false:
    cam.set_distance(300.0);
    cam.adjust(&bounds, false, vp_wide, CameraCrop::None);
    assert_eq!(cam.dest().x, 0.0);
    assert_eq!(cam.dest().y, 0.0);
    assert_eq!(cam.dest().z, -300.0);

    // Case 2: Portrait / tall viewport: 600 x 800 (aspect ratio = 600 / 800 = 0.75 < 1.0).
    // Same bounds (width = 220, height = 110).
    // Because aspect_ratio < 1.0:
    // height /= aspect_ratio -> 110.0 / 0.75 = 146.66667.
    // width remains 220.0.
    // width (220.0) >= height (146.66667):
    // distance = 220.0 / 2.0 = 110.0.
    cam.adjust(&bounds, true, Vec2::new(600.0, 800.0), CameraCrop::None);
    assert!((cam.dest().z - (-110.0)).abs() < 1e-5);

    // Case 3: Tall bounds in portrait viewport where height > width.
    // Bounds (-10, -200) to (10, 200).
    // width = 20 * 1.1 = 22.0.
    // height = 400 * 1.1 = 440.0.
    // In 600 x 800 (aspect 0.75):
    // height /= 0.75 -> 440.0 / 0.75 = 586.66667.
    // width remains 22.0.
    // height > width -> distance = height / toa = 586.66667 / 2.0 = 293.33333.
    let tall_bounds = Bounds2D::from_points(Vec2::new(-10.0, -200.0), Vec2::new(10.0, 200.0));
    cam.adjust(
        &tall_bounds,
        true,
        Vec2::new(600.0, 800.0),
        CameraCrop::None,
    );
    assert!((cam.dest().z - (-293.33334)).abs() < 1e-4);

    // Case 4: Crop modes on the wide bounds in 1920x1080 (width=123.75, height=110.0)
    // CropVertical: distance = width / toa = 123.75 / 2.0 = 61.875.
    cam.adjust(&bounds, true, vp_wide, CameraCrop::Vertical);
    assert!((cam.dest().z - (-61.875)).abs() < 1e-5);

    // CropHorizontal: distance = height / toa = 110.0 / 2.0 = 55.0.
    cam.adjust(&bounds, true, vp_wide, CameraCrop::Horizontal);
    assert!((cam.dest().z - (-55.0)).abs() < 1e-5);

    // Also test with GourceSettings
    let mut settings = GourceSettings {
        crop_horizontal: true,
        ..Default::default()
    };
    cam.adjust_with_settings(&bounds, true, vp_wide, &settings);
    assert!((cam.dest().z - (-55.0)).abs() < 1e-5);

    settings.crop_horizontal = false;
    settings.crop_vertical = true;
    cam.adjust_with_settings(&bounds, true, vp_wide, &settings);
    assert!((cam.dest().z - (-61.875)).abs() < 1e-5);

    // Viewport height zero fallback
    cam.adjust(&bounds, true, Vec2::new(100.0, 0.0), CameraCrop::None);
    assert!((cam.dest().z - (-110.0)).abs() < 1e-5);
}

#[test]
fn test_adjust_min_max_clamping() {
    let mut cam = ZoomCamera::new(Vec3::ZERO, Vec3::ZERO, 50.0, 500.0);
    cam.set_padding(1.0);
    let vp = Vec2::new(1000.0, 1000.0); // aspect ratio = 1.0, toa = 2.0

    // Tiny bounds: width = 10, height = 10 -> distance = 10 / 2.0 = 5.0 < min_distance (50.0)
    let tiny_bounds = Bounds2D::from_points(Vec2::new(-5.0, -5.0), Vec2::new(5.0, 5.0));
    cam.adjust(&tiny_bounds, true, vp, CameraCrop::None);
    assert_eq!(cam.dest().z, -50.0);

    // Huge bounds: width = 5000, height = 5000 -> distance = 5000 / 2.0 = 2500.0 > max_distance (500.0)
    let huge_bounds = Bounds2D::from_points(Vec2::new(-2500.0, -2500.0), Vec2::new(2500.0, 2500.0));
    cam.adjust(&huge_bounds, true, vp, CameraCrop::None);
    assert_eq!(cam.dest().z, -500.0);
}

#[test]
fn test_logic_lerp_normal() {
    // Normal logic lerp:
    // lockon = false, speed = 2.0, dt = 0.25.
    // pos = (0, 0, -100), dest = (100, 200, -300).
    // dp = dest - pos = (100, 200, -200).
    // dpt = dp * dt * speed = dp * 0.25 * 2.0 = dp * 0.5 = (50, 100, -100).
    // length2(dpt) <= length2(dp), so pos += dpt -> pos = (50, 100, -200).
    // target = (pos.x, pos.y, 0) = (50, 100, 0).
    let mut cam = ZoomCamera::new(Vec3::new(0.0, 0.0, -100.0), Vec3::ZERO, 50.0, 10000.0);
    cam.set_speed(2.0);
    cam.set_pos(Vec3::new(0.0, 0.0, -100.0), false);
    cam.set_distance(300.0);
    // adjust dest x, y manually via adjust with adjust_distance = false
    let b = Bounds2D::from_points(Vec2::new(100.0, 200.0), Vec2::new(100.0, 200.0));
    cam.adjust(&b, false, Vec2::new(800.0, 600.0), CameraCrop::None);
    assert_eq!(cam.dest(), Vec3::new(100.0, 200.0, -300.0));

    cam.logic(0.25);
    assert_eq!(cam.pos(), Vec3::new(50.0, 100.0, -200.0));
    assert_eq!(cam.target(), Vec3::new(50.0, 100.0, 0.0));

    // If dt is huge such that dpt exceeds dp:
    // dt = 10.0, dpt would be dp * 10 * 2 = dp * 20.
    // length2(dpt) > length2(dp), clamped to dp!
    // So pos reaches dest exactly in one step.
    cam.logic(10.0);
    assert_eq!(cam.pos(), Vec3::new(100.0, 200.0, -300.0));
    assert_eq!(cam.target(), Vec3::new(100.0, 200.0, 0.0));
}

#[test]
fn test_logic_lockon() {
    let mut cam = ZoomCamera::new(Vec3::ZERO, Vec3::ZERO, 50.0, 1000.0);
    cam.lock_on(true);
    assert!(cam.is_locked_on());

    // On lock_on(true), lockon_time = 1.0.
    // First step with dt = 0.5, speed = 1.0.
    // dp = (100, 0, 0).
    // dpt = dp * dt * speed = 100 * 0.5 * 1.0 = 50.
    // In lockon: dpt = dpt * lockon_time + dp * (1 - lockon_time)
    // With lockon_time = 1.0: dpt = 50 * 1.0 + 100 * 0 = 50.
    // lockon_time updated: max(0.0, 1.0 - 0.5 * 0.5) = 0.75.
    cam.set_distance(0.0); // dest.z = 0
    let b = Bounds2D::from_points(Vec2::new(100.0, 0.0), Vec2::new(100.0, 0.0));
    cam.adjust(&b, false, Vec2::new(800.0, 600.0), CameraCrop::None);

    cam.logic(0.5);
    assert_eq!(cam.pos().x, 50.0);

    // Second step: dt = 0.5, lockon_time was 0.75.
    // dp = (100 - 50, 0, 0) = 50.
    // dpt = dp * dt * speed = 50 * 0.5 * 1.0 = 25.
    // dpt = 25 * 0.75 + 50 * 0.25 = 18.75 + 12.5 = 31.25.
    // pos += 31.25 -> pos.x = 81.25.
    // lockon_time updated: max(0, 0.75 - 0.25) = 0.5.
    cam.logic(0.5);
    assert_eq!(cam.pos().x, 81.25);

    // Turning off lockon:
    cam.lock_on(false);
    assert!(!cam.is_locked_on());
}

#[test]
fn test_projection_consistency() {
    // Verify that after adjusting the camera to a bounding box, the visible bounds
    // of the resulting projection completely contain the target bounds.
    let mut cam = ZoomCamera::default();
    cam.set_padding(1.1);

    let target_bounds = Bounds2D::from_points(Vec2::new(-350.0, -120.0), Vec2::new(420.0, 250.0));
    let viewport = Vec2::new(1280.0, 720.0);

    cam.adjust(&target_bounds, true, viewport, CameraCrop::None);
    // Warp camera pos to dest as if logic() completed
    cam.set_pos(cam.dest(), false);

    let proj = cam.projection(viewport);
    let vis = proj.visible_bounds();

    // The visible bounds must contain the target bounds corners
    for corner in target_bounds.corners() {
        assert!(
            vis.contains(corner),
            "Visible bounds {:?} did not contain corner {:?}",
            vis,
            corner
        );
    }
}
