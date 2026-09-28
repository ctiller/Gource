//! Golden values are printed from the C++ reference at full precision.
#![allow(clippy::excessive_precision)]

use glam::{Vec2, Vec3};
use gource_sim::file::File;
use gource_sim::user::User;

#[test]
fn test_golden_file_sim_lifecycle_and_fade() {
    let mut file = File::new(
        "/test/file.txt",
        Vec3::new(0.2, 0.4, 0.6),
        Vec2::ZERO,
        1,
        8.0,
        5.0,
        false,
    );
    file.pawn.fadetime = 0.0;
    file.pawn.set_hidden(false);
    file.touch_colour = Vec3::ONE;
    file.dest = Vec2::new(1.0, 0.0);
    file.distance = 100.0;
    file.last_action = 0.0;

    let dt = 0.1;
    let file_idle_time = 3.0;

    // Simulate 50 steps
    for step in 0..50 {
        let just_expired = file.logic(dt, file_idle_time);

        // Check values against C++ golden log:
        // FSTEP 0 t=0.100000 pos=(50.000000,0.000000) alpha=1.000000 col=(0.920000,0.940000,0.960000) expired=0 just_expired=0
        if step == 0 {
            assert!((file.pawn.pos.x - 50.0).abs() < 1e-4);
            assert_eq!(file.alpha(), 1.0);
            let c = file.colour();
            assert!((c.x - 0.92).abs() < 1e-3);
            assert!((c.y - 0.94).abs() < 1e-3);
            assert!((c.z - 0.96).abs() < 1e-3);
            assert!(!file.expired);
            assert!(!just_expired);
        }

        // FSTEP 5 t=0.600000 pos=(98.437500,0.000000) alpha=1.000000 col=(0.520000,0.640000,0.760000)
        if step == 5 {
            assert!((file.pawn.pos.x - 98.4375).abs() < 1e-3);
            assert_eq!(file.alpha(), 1.0);
            let c = file.colour();
            assert!((c.x - 0.52).abs() < 1e-3);
            assert!((c.y - 0.64).abs() < 1e-3);
            assert!((c.z - 0.76).abs() < 1e-3);
        }

        // FSTEP 10 t=1.100000 pos=(99.951172,0.000000) alpha=1.000000 col=(0.200000,0.400000,0.600000)
        if step == 10 {
            assert!((file.pawn.pos.x - 99.951172).abs() < 1e-3);
            assert_eq!(file.alpha(), 1.0);
            let c = file.colour();
            assert!((c.x - 0.20).abs() < 1e-4);
            assert!((c.y - 0.40).abs() < 1e-4);
            assert!((c.z - 0.60).abs() < 1e-4);
        }

        // FSTEP 35 t=3.599999 pos=(100.000000,0.000000) alpha=0.500000
        if step == 35 {
            assert!((file.pawn.pos.x - 100.0).abs() < 1e-3);
            assert!((file.alpha() - 0.5).abs() < 1e-3);
        }

        // FSTEP 41 t=4.199998 pos=(100.000000,0.000000) alpha=0.000000 expired=1 just_expired=1
        if step == 41 {
            assert!((file.pawn.pos.x - 100.0).abs() < 1e-3);
            assert_eq!(file.alpha(), 0.0);
            assert!(file.expired);
            assert!(just_expired);
        }
    }
}

#[test]
fn test_golden_user_physics_steps() {
    let hasher = gource_core::StringHasher::default();
    let mut user = User::new("test", Vec2::ZERO, 1, 250.0, 1.0, &hasher);
    user.pawn.accel = Vec2::new(300.0, 400.0); // length = 500

    let dt = 0.05;
    let friction = 0.5;

    // Step 0:
    // USTEP 0 pos=(7.500000,10.000000) accel=(146.250000,195.000000)
    let _ = user.logic(0.0, dt, 5.0, 100.0, friction, |_| None);
    assert!((user.pawn.pos.x - 7.5).abs() < 1e-3);
    assert!((user.pawn.pos.y - 10.0).abs() < 1e-3);
    assert!((user.pawn.accel.x - 146.25).abs() < 1e-3);
    assert!((user.pawn.accel.y - 195.0).abs() < 1e-3);

    // Step 1:
    // USTEP 1 pos=(14.812500,19.750000) accel=(142.593750,190.125000)
    let _ = user.logic(0.0, dt, 5.0, 100.0, friction, |_| None);
    assert!((user.pawn.pos.x - 14.8125).abs() < 1e-3);
    assert!((user.pawn.pos.y - 19.75).abs() < 1e-3);
    assert!((user.pawn.accel.x - 142.59375).abs() < 1e-3);
    assert!((user.pawn.accel.y - 190.125).abs() < 1e-3);
}
