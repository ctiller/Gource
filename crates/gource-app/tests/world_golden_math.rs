//! Golden values are printed from the C++ reference at full precision.
#![allow(clippy::excessive_precision)]

use glam::{Vec2, Vec4};
use gource_app::pawn::Pawn;
use gource_app::spline::SplineEdge;

#[test]
fn test_golden_world_math_spline() {
    let mut spline1 = SplineEdge::new();
    let p1 = Vec2::new(0.0, 100.0);
    let c1 = Vec4::new(1.0, 0.0, 0.0, 1.0);
    let p2 = Vec2::new(100.0, 0.0);
    let c2 = Vec4::new(0.0, 1.0, 0.0, 1.0);
    let spos = Vec2::new(10.0, 10.0);

    spline1.update(p1, c1, p2, c2, spos, 0.5);

    // C++ golden values from tests/data/world/golden_world_math.txt:
    // SPLINE 10 30.000000 30.000000
    // PT 0 100.000000 0.000000 0.000000 1.000000 0.000000 1.000000
    // PT 5 30.000000 30.000000 0.500000 0.500000 0.000000 1.000000
    // PT 10 0.000000 100.000000 1.000000 0.000000 0.000000 1.000000
    assert_eq!(spline1.spline_point.len(), 11);
    assert!((spline1.label_pos().x - 30.0).abs() < 1e-4);
    assert!((spline1.label_pos().y - 30.0).abs() < 1e-4);

    assert!((spline1.spline_point[0].x - 100.0).abs() < 1e-4);
    assert!((spline1.spline_point[0].y - 0.0).abs() < 1e-4);
    assert!((spline1.spline_point[5].x - 30.0).abs() < 1e-4);
    assert!((spline1.spline_point[5].y - 30.0).abs() < 1e-4);
    assert!((spline1.spline_point[10].x - 0.0).abs() < 1e-4);
    assert!((spline1.spline_point[10].y - 100.0).abs() < 1e-4);

    assert!((spline1.spline_colour[0] - c2).length() < 1e-4);
    assert!((spline1.spline_colour[10] - c1).length() < 1e-4);

    // Spline test 2
    let mut spline2 = SplineEdge::new();
    let p1_2 = Vec2::new(50.0, -20.0);
    let c1_2 = Vec4::new(0.2, 0.4, 0.6, 0.8);
    let p2_2 = Vec2::new(-10.0, 80.0);
    let c2_2 = Vec4::new(0.8, 0.6, 0.4, 0.2);
    let spos_2 = Vec2::new(0.0, 0.0);

    spline2.update(p1_2, c1_2, p2_2, c2_2, spos_2, 0.3);
    // SPLINE 10 22.399998 7.000002
    assert_eq!(spline2.spline_point.len(), 11);
    assert!((spline2.label_pos().x - 22.4).abs() < 1e-3);
    assert!((spline2.label_pos().y - 7.0).abs() < 1e-3);
    assert!((spline2.spline_point[0].x - (-10.0)).abs() < 1e-4);
    assert!((spline2.spline_point[0].y - 80.0).abs() < 1e-4);
    assert!((spline2.spline_point[10].x - 50.0).abs() < 1e-4);
    assert!((spline2.spline_point[10].y - (-20.0)).abs() < 1e-4);
}

#[test]
fn test_golden_world_math_name_alpha() {
    let mut p = Pawn::new("user".to_string(), Vec2::ZERO, 1);
    p.nametime = 5.0;

    // NAME_ALPHA 5.000000 4.500000 0.500000
    p.name_interval = 4.5;
    assert!((p.name_alpha() - 0.5).abs() < 1e-4);

    // NAME_ALPHA 5.000000 3.000000 1.000000
    p.name_interval = 3.0;
    assert!((p.name_alpha() - 1.0).abs() < 1e-4);

    // NAME_ALPHA 5.000000 0.500000 0.500000
    p.name_interval = 0.5;
    assert!((p.name_alpha() - 0.5).abs() < 1e-4);
}
