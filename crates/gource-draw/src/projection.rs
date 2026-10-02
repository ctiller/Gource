//! World <-> screen mapping for Gource's camera.
//!
//! The C++ camera is a perspective camera (vertical fov 90°) at `pos`, looking
//! at `(pos.x, pos.y, 0)` with up vector `(0, -1, 0)`, and everything Gource
//! draws lies on the z = 0 plane. That makes the projection of the plane an
//! exact uniform scale + translation, so the simulation can project on the CPU
//! and emit screen-space geometry:
//!
//! ```text
//! scale  = (viewport.y / 2) / (distance * tan(fov / 2)),  distance = -pos.z
//! screen = (world - pos.xy) * scale + viewport / 2
//! ```
//!
//! (world +x = screen right, world +y = screen down).

use gource_core::Bounds2D;
use gource_core::{Vec2, Vec3};

/// Vertical field of view of the Gource camera, in degrees.
pub const CAMERA_FOV_DEGREES: f32 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Camera position projected onto the plane (`pos.xy`).
    pub centre: Vec2,
    /// Distance from the camera to the z = 0 plane (`-pos.z`), > 0.
    pub distance: f32,
    /// Viewport size in pixels.
    pub viewport: Vec2,
}

impl Projection {
    /// Projection for a camera at `camera_pos` (with `camera_pos.z < 0`).
    pub fn new(camera_pos: Vec3, viewport: Vec2) -> Self {
        Self {
            centre: camera_pos.truncate(),
            distance: (-camera_pos.z).max(f32::EPSILON),
            viewport,
        }
    }

    /// Pixels per world unit.
    pub fn scale(&self) -> f32 {
        let half_fov = (CAMERA_FOV_DEGREES * 0.5).to_radians();
        (self.viewport.y * 0.5) / (self.distance * half_fov.tan())
    }

    pub fn to_screen(&self, world: Vec2) -> Vec2 {
        (world - self.centre) * self.scale() + self.viewport * 0.5
    }

    pub fn to_world(&self, screen: Vec2) -> Vec2 {
        (screen - self.viewport * 0.5) / self.scale() + self.centre
    }

    /// Scale a world-space length to pixels.
    pub fn to_screen_len(&self, len: f32) -> f32 {
        len * self.scale()
    }

    /// The part of the plane visible in the viewport (the view frustum
    /// intersected with z = 0).
    pub fn visible_bounds(&self) -> Bounds2D {
        Bounds2D::from_points(self.to_world(Vec2::ZERO), self.to_world(self.viewport))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_maps_to_middle_of_screen() {
        let p = Projection::new(Vec3::new(10.0, 20.0, -300.0), Vec2::new(1280.0, 720.0));
        assert!((p.to_screen(Vec2::new(10.0, 20.0)) - Vec2::new(640.0, 360.0)).length() < 1e-4);
    }

    #[test]
    fn scale_matches_fov() {
        // At distance d with a 90 degree fov, the visible half height is d.
        let p = Projection::new(Vec3::new(0.0, 0.0, -100.0), Vec2::new(800.0, 600.0));
        assert!((p.scale() - 3.0).abs() < 1e-5);
        assert!((p.to_screen(Vec2::new(0.0, 100.0)).y - 600.0).abs() < 1e-3);
        assert!((p.to_screen(Vec2::new(0.0, -100.0)).y - 0.0).abs() < 1e-3);
    }

    #[test]
    fn round_trip() {
        let p = Projection::new(Vec3::new(-5.0, 7.0, -250.0), Vec2::new(1024.0, 768.0));
        let w = Vec2::new(33.0, -12.5);
        assert!((p.to_world(p.to_screen(w)) - w).length() < 1e-3);
        let b = p.visible_bounds();
        assert!(b.contains(Vec2::new(-5.0, 7.0)));
        assert!((b.height() - 500.0).abs() < 1e-2);
    }

    #[test]
    fn matches_cpp_mouse_unprojection() {
        // gource.cpp: projected = (-(mx*2 - w)/h, (1 - 2*my/h)) * cam.z + cam.xy
        let cam = Vec3::new(3.0, 4.0, -120.0);
        let (w, h) = (1280.0f32, 720.0f32);
        let p = Projection::new(cam, Vec2::new(w, h));
        let (mx, my) = (100.0f32, 650.0f32);
        let cpp = Vec2::new(-(mx * 2.0 - w) / h, 1.0 - 2.0 * my / h) * cam.z + cam.truncate();
        assert!((p.to_world(Vec2::new(mx, my)) - cpp).length() < 1e-3);
    }
}
