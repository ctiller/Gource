//! ZoomCamera (port of `zoomcamera.{h,cpp}`).
//!
//! Controls the camera position, target, zoom and smooth movement towards
//! a target bounding box or destination. The camera points at `(pos.x, pos.y, 0)`
//! with up vector `(0, -1, 0)` on the z = 0 plane.
//!
//! # C++ Call Mapping (src/gource.cpp -> crates/gource-sim/src/camera.rs)
//!
//! | C++ call in `gource.cpp` | Rust method in [`ZoomCamera`] |
//! |---|---|
//! | `camera.setPadding(padding)` | [`ZoomCamera::set_padding`] |
//! | `camera.getDest()` | [`ZoomCamera::dest`] |
//! | `camera.setDistance(distance)` | [`ZoomCamera::set_distance`] |
//! | `camera.lockOn(bool)` | [`ZoomCamera::lock_on`] |
//! | `camera.reset()` | [`ZoomCamera::reset`] |
//! | `camera.getPos()` | [`ZoomCamera::pos`] |
//! | `camera.setPos(pos, keep_angle)` | [`ZoomCamera::set_pos`] |
//! | `camera.stop()` | [`ZoomCamera::stop`] |
//! | `camera.adjust(bounds, adjust_distance)` | [`ZoomCamera::adjust`] or [`ZoomCamera::adjust_with_settings`] |
//! | `camera.logic(dt)` | [`ZoomCamera::logic`] |
//! | `camera.getTarget()` | [`ZoomCamera::target`] |
//! | `camera.getUp()` | [`ZoomCamera::up`] |
//! | `camera.getFOV()` | [`ZoomCamera::fov`] |
//! | `camera.getZNear()` | [`ZoomCamera::znear`] |
//! | `camera.getZFar()` | [`ZoomCamera::zfar`] |
//! | `camera.focus()` | [`ZoomCamera::focus`] (returns [`CameraFocusData`]) |
//! | (display unproject / project) | [`ZoomCamera::projection`] (CPU projection) |

use glam::{Vec2, Vec3};
use gource_core::Bounds2D;
use gource_draw::Projection;
use gource_settings::GourceSettings;

/// Initial camera z (C++ `Gource::starting_z`).
pub const STARTING_Z: f32 = -300.0;

/// Camera mode cropping for [`ZoomCamera::adjust`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraCrop {
    #[default]
    None,
    Vertical,
    Horizontal,
}

impl CameraCrop {
    /// Determine crop mode from `GourceSettings`.
    pub fn from_settings(settings: &GourceSettings) -> Self {
        if settings.crop_vertical {
            CameraCrop::Vertical
        } else if settings.crop_horizontal {
            CameraCrop::Horizontal
        } else {
            CameraCrop::None
        }
    }
}

/// Data returned by [`ZoomCamera::focus`] describing the 3D perspective and view matrices.
///
/// In the C++ version, `ZoomCamera::focus()` directly issued OpenGL calls:
/// `display.mode3D(fov, znear, zfar)` and `gluLookAt(...)`.
/// In Rust, rendering is decoupled from OpenGL, so `focus()` returns the camera view/perspective
/// parameters as data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraFocusData {
    pub fov: f32,
    pub znear: f32,
    pub zfar: f32,
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
}

/// Smooth zoom camera (port of `ZoomCamera`).
#[derive(Debug, Clone, PartialEq)]
pub struct ZoomCamera {
    pos: Vec3,
    dest: Vec3,
    target: Vec3,
    up: Vec3,
    initial_pos: Vec3,
    initial_target: Vec3,

    lockon: bool,
    speed: f32,
    lockon_time: f32,

    padding: f32,

    min_distance: f32,
    max_distance: f32,

    fov: f32,
    znear: f32,
    zfar: f32,
}

impl Default for ZoomCamera {
    /// Port of `ZoomCamera::ZoomCamera()`.
    ///
    /// Constructs a camera with default settings (fov = 90.0, znear = 0.1, zfar = 10001.0,
    /// min_distance = 50.0, max_distance = 10000.0, up = (0, -1, 0)).
    fn default() -> Self {
        Self::new(Vec3::new(0.0, 0.0, -100.0), Vec3::ZERO, 50.0, 10000.0)
    }
}

impl ZoomCamera {
    /// Port of `ZoomCamera::ZoomCamera(vec3 pos, vec3 target, float min_distance, float max_distance)`.
    pub fn new(pos: Vec3, target: Vec3, min_distance: f32, max_distance: f32) -> Self {
        let mut cam = Self {
            pos,
            dest: pos,
            target,
            up: Vec3::new(0.0, -1.0, 0.0),
            initial_pos: pos,
            initial_target: target,

            lockon: false,
            speed: 1.0,
            lockon_time: 0.0,

            padding: 1.0,

            min_distance: 0.0,
            max_distance: 0.0,

            fov: 90.0,
            znear: 0.1,
            zfar: 0.0,
        };

        cam.set_min_distance(min_distance);
        cam.set_max_distance(max_distance);
        cam.reset();
        cam
    }

    /// The camera `Gource` starts with: C++
    /// `ZoomCamera(vec3(0, 0, starting_z), vec3(0), camera_zoom_default, camera_zoom_max)`.
    /// `camera_zoom_default` is the closest the automatic framing zooms in;
    /// `camera_zoom_min` only limits manual zooming.
    pub fn from_settings(settings: &GourceSettings) -> Self {
        let mut cam = Self::new(
            Vec3::new(0.0, 0.0, STARTING_Z),
            Vec3::ZERO,
            settings.camera_zoom_default,
            settings.camera_zoom_max,
        );
        cam.set_padding(settings.padding);
        cam
    }

    /// Port of `ZoomCamera::reset()`.
    pub fn reset(&mut self) {
        self.pos = self.initial_pos;
        self.target = self.initial_target;
    }

    /// Port of `ZoomCamera::getPos()`.
    #[inline]
    pub fn pos(&self) -> Vec3 {
        self.pos
    }

    /// Port of `ZoomCamera::getDest()`.
    #[inline]
    pub fn dest(&self) -> Vec3 {
        self.dest
    }

    /// Port of `ZoomCamera::getTarget()`.
    #[inline]
    pub fn target(&self) -> Vec3 {
        self.target
    }

    /// Port of `ZoomCamera::getUp()`.
    #[inline]
    pub fn up(&self) -> Vec3 {
        self.up
    }

    /// Port of `ZoomCamera::getFOV()`.
    #[inline]
    pub fn fov(&self) -> f32 {
        self.fov
    }

    /// Port of `ZoomCamera::getZNear()`.
    #[inline]
    pub fn znear(&self) -> f32 {
        self.znear
    }

    /// Port of `ZoomCamera::getZFar()`.
    #[inline]
    pub fn zfar(&self) -> f32 {
        self.zfar
    }

    /// Port of `ZoomCamera::getMinDistance()`.
    #[inline]
    pub fn min_distance(&self) -> f32 {
        self.min_distance
    }

    /// Port of `ZoomCamera::getMaxDistance()`.
    #[inline]
    pub fn max_distance(&self) -> f32 {
        self.max_distance
    }

    /// Port of `ZoomCamera::padding`.
    #[inline]
    pub fn padding(&self) -> f32 {
        self.padding
    }

    /// Port of `ZoomCamera::speed`.
    #[inline]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Whether lockOn mode is enabled.
    #[inline]
    pub fn is_locked_on(&self) -> bool {
        self.lockon
    }

    /// Port of `ZoomCamera::setSpeed(float speed)`.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    /// Port of `ZoomCamera::setPadding(float padding)`.
    pub fn set_padding(&mut self, padding: f32) {
        self.padding = padding;
    }

    /// Port of `ZoomCamera::setMinDistance(float min)`.
    pub fn set_min_distance(&mut self, min: f32) {
        self.min_distance = min;
    }

    /// Port of `ZoomCamera::setMaxDistance(float max)`.
    pub fn set_max_distance(&mut self, max: f32) {
        self.max_distance = max;
        self.zfar = max + 1.0;
    }

    /// Port of `ZoomCamera::setDistance(float distance)`.
    pub fn set_distance(&mut self, distance: f32) {
        self.dest.z = -distance;
    }

    /// Port of `ZoomCamera::setPos(const vec3& pos, bool keep_angle)`.
    pub fn set_pos(&mut self, pos: Vec3, keep_angle: bool) {
        if keep_angle {
            let dir = self.target - self.pos;
            self.pos = pos;
            self.target = pos + dir;
        } else {
            self.pos = pos;
        }
    }

    /// Port of `ZoomCamera::lockOn(bool lockon)`.
    pub fn lock_on(&mut self, lockon: bool) {
        if lockon {
            self.lockon_time = 1.0;
        }
        self.lockon = lockon;
    }

    /// Port of `ZoomCamera::stop()`.
    pub fn stop(&mut self) {
        self.dest = self.pos;
    }

    /// Port of `ZoomCamera::lookAt(const vec3& target)`.
    ///
    /// In C++ this called `gluLookAt(pos, target, up)`. In Rust we return `(pos, target, up)`.
    pub fn look_at(&self, target: Vec3) -> (Vec3, Vec3, Vec3) {
        (self.pos, target, self.up)
    }

    /// Port of `ZoomCamera::look()`.
    ///
    /// Calls `lookAt(self.target)`.
    pub fn look(&self) -> (Vec3, Vec3, Vec3) {
        self.look_at(self.target)
    }

    /// Port of `ZoomCamera::focus()`.
    ///
    /// In C++ this called `display.mode3D(fov, znear, zfar); look();`.
    /// Returns [`CameraFocusData`].
    pub fn focus(&self) -> CameraFocusData {
        CameraFocusData {
            fov: self.fov,
            znear: self.znear,
            zfar: self.zfar,
            eye: self.pos,
            target: self.target,
            up: self.up,
        }
    }

    /// Create a [`Projection`] at the current camera position for screen/world mapping.
    pub fn projection(&self, viewport: Vec2) -> Projection {
        Projection::new(self.pos, viewport)
    }

    /// Port of `ZoomCamera::adjust(const Bounds2D& bounds, bool adjust_distance)`.
    ///
    /// Centers the camera destination on `bounds.centre()`. If `adjust_distance` is true,
    /// computes the camera destination z based on padding, aspect ratio, fov, crop mode,
    /// and clamped to `[min_distance, max_distance]`.
    pub fn adjust(
        &mut self,
        bounds: &Bounds2D,
        adjust_distance: bool,
        viewport: Vec2,
        crop: CameraCrop,
    ) {
        let centre = bounds.centre();
        self.dest.x = centre.x;
        self.dest.y = centre.y;

        if !adjust_distance {
            return;
        }

        // scale by padding
        let mut width = bounds.width() * self.padding;
        let mut height = bounds.height() * self.padding;

        let aspect_ratio = if viewport.y != 0.0 {
            viewport.x / viewport.y
        } else {
            1.0
        };

        if aspect_ratio < 1.0 {
            height /= aspect_ratio;
        } else {
            width /= aspect_ratio;
        }

        // calc visible width of the opposite wall at a distance of 1 this fov
        // C++ `tan( fov * 0.5f * DEGREES_TO_RADIANS ) * 2.0`: double math.
        let toa = ((((self.fov * 0.5) as f64) * gource_core::math::CPP_DEGREES_TO_RADIANS).tan()
            * 2.0) as f32;

        // TOA = tan = opposite/adjacent (distance = adjacent)
        // use the larger side of the box
        let distance = match crop {
            CameraCrop::Vertical => width / toa,
            CameraCrop::Horizontal => height / toa,
            CameraCrop::None => {
                if width >= height {
                    width / toa
                } else {
                    height / toa
                }
            }
        };

        // C++ order: max wins if min > max (f32::clamp would panic).
        let distance = distance.max(self.min_distance).min(self.max_distance);
        self.dest.z = -distance;
    }

    /// Port of `ZoomCamera::adjust(bounds, adjust_distance)` taking settings and viewport.
    pub fn adjust_with_settings(
        &mut self,
        bounds: &Bounds2D,
        adjust_distance: bool,
        viewport: Vec2,
        settings: &GourceSettings,
    ) {
        self.adjust(
            bounds,
            adjust_distance,
            viewport,
            CameraCrop::from_settings(settings),
        );
    }

    /// Port of `ZoomCamera::logic(float dt)`.
    ///
    /// Moves the camera towards `dest`. If locked on, smoothly interpolates with `lockon_time`.
    /// Resets `target` to `(pos.x, pos.y, 0.0)` every frame so the camera never tilts.
    pub fn logic(&mut self, dt: f32) {
        let dp = self.dest - self.pos;

        let mut dpt = dp * dt * self.speed;

        if self.lockon {
            dpt = dpt * self.lockon_time + dp * (1.0 - self.lockon_time);

            if self.lockon_time > 0.0 {
                self.lockon_time = (self.lockon_time - dt * 0.5).max(0.0);
            }
        }

        if dpt.length_squared() > dp.length_squared() {
            dpt = dp;
        }

        self.pos += dpt;

        self.target = Vec3::new(self.pos.x, self.pos.y, 0.0);
    }
}
