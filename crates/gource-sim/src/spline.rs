//! SplineEdge: curved tree edges (port of spline.cpp).

use glam::{Vec2, Vec4};
use std::f32::consts::PI;

/// A curved edge connecting directory nodes.
/// Port of `SplineEdge` in `spline.h` / `spline.cpp`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplineEdge {
    pub spline_point: Vec<Vec2>,
    pub spline_colour: Vec<Vec4>,
    pub label_pos: Vec2,
}

impl SplineEdge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Label position along the spline.
    /// Port of `SplineEdge::getLabelPos()`.
    pub fn label_pos(&self) -> Vec2 {
        self.label_pos
    }

    /// Calculate spline points and label position.
    /// Port of `SplineEdge::update(pos1, col1, pos2, col2, spos)`.
    /// `dir_name_position` comes from `gGourceSettings.dir_name_position` (default 0.5).
    pub fn update(
        &mut self,
        pos1: Vec2,
        col1: Vec4,
        pos2: Vec2,
        col2: Vec4,
        spos: Vec2,
        dir_name_position: f32,
    ) {
        let mid = (pos1 - pos2) * 0.5;
        let to = pos1 - spos;

        let to_len = to.length();
        let mid_len = mid.length();

        let to_norm = if to_len > 0.0 { to / to_len } else { to };
        let mid_norm = if mid_len > 0.0 { mid / mid_len } else { mid };

        let dp = (to_norm.dot(mid_norm)).clamp(-1.0, 1.0);
        let ang = dp.acos() / PI;

        let edge_detail = ((ang * 100.0) as i32).clamp(1, 10);

        self.spline_point.clear();
        self.spline_colour.clear();

        let count = (edge_detail + 1) as usize;
        self.spline_point.reserve(count);
        self.spline_colour.reserve(count);

        for i in 0..=edge_detail {
            let t = i as f32 / edge_detail as f32;
            let tt = 1.0 - t;

            let p0 = pos1 * t + spos * tt;
            let p1 = spos * t + pos2 * tt;

            let pt = p0 * t + p1 * tt;
            let coln = col1 * t + col2 * tt;

            self.spline_point.push(pt);
            self.spline_colour.push(coln);
        }

        let pos = dir_name_position;
        let s_quota = 0.5 - (pos - 0.5).abs();
        let p_quota = 1.0 - s_quota;

        self.label_pos = pos1 * (p_quota * (1.0 - pos)) + pos2 * (p_quota * pos) + spos * s_quota;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_straight_line() {
        let mut spline = SplineEdge::new();
        let p1 = Vec2::new(0.0, 0.0);
        let p2 = Vec2::new(100.0, 0.0);
        let spos = Vec2::new(50.0, 0.0);
        let c1 = Vec4::new(1.0, 0.0, 0.0, 1.0);
        let c2 = Vec4::new(0.0, 1.0, 0.0, 1.0);

        spline.update(p1, c1, p2, c2, spos, 0.5);

        assert!(!spline.spline_point.is_empty());
        assert_eq!(spline.spline_point[0], p2);
        assert_eq!(*spline.spline_point.last().unwrap(), p1);

        // label pos with pos = 0.5:
        // s_quota = 0.5 - 0 = 0.5
        // p_quota = 0.5
        // label_pos = pos1 * (0.5 * 0.5) + pos2 * (0.5 * 0.5) + spos * 0.5 = 0.25 pos1 + 0.25 pos2 + 0.5 spos
        assert_eq!(spline.label_pos(), Vec2::new(50.0, 0.0));
    }

    #[test]
    fn spline_curved_detail() {
        let mut spline = SplineEdge::new();
        let p1 = Vec2::new(0.0, 100.0);
        let p2 = Vec2::new(100.0, 0.0);
        let spos = Vec2::new(0.0, 0.0);

        spline.update(p1, Vec4::ONE, p2, Vec4::ONE, spos, 0.3);
        assert!(spline.spline_point.len() > 2);
        assert!(spline.label_pos().x > 0.0);
    }
}
