//! File representation (port of file.cpp).

use crate::pawn::Pawn;
use glam::{Vec2, Vec3};
use gource_core::Bounds2D;
use slotmap::new_key_type;

new_key_type! {
    /// File identifier in [`crate::world::World`].
    pub struct FileId;
    /// Directory identifier in [`crate::world::World`].
    pub struct DirId;
}

/// A file in the visualizer repository tree.
/// Port of `RFile` in `file.h` / `file.cpp`.
#[derive(Debug, Clone)]
pub struct File {
    pub pawn: Pawn,

    pub file_colour: Vec3,
    pub touch_colour: Vec3,

    pub dir: Option<DirId>,

    pub removed_timestamp: i64,
    pub forced_removal: bool,
    pub expired: bool,
    pub removing: bool,

    pub fade_start: f32,
    pub last_action: f32,

    pub radius: f32,
    pub dest: Vec2,
    pub distance: f32,
    pub vel: Vec2,

    pub path: String,
    pub fullpath: String,
    pub ext: String,

    pub lines: u32,
    pub byte_size: u64,
    pub touch_count: u32,
    pub target_size: f32,
    pub pulse_timer: f32,
    pub pulse_max_time: f32,
    pub pulse_delta: i32,
    pub pulse_scale: f32,
    pub total_added: u64,
    pub total_removed: u64,
    pub created_timestamp: i64,
    pub dominant_cohort_colour: Option<Vec3>,
    pub weighted: bool,

    pub is_shadow: bool,
    pub shadow_alpha: f32,
    pub solidifying: bool,
    pub solidify_timer: f32,
}

impl File {
    /// Port of `RFile::RFile(const std::string & name, const vec3 & colour, const vec2 & pos, int tagid)`.
    pub fn new(
        fullpath: &str,
        colour: Vec3,
        pos: Vec2,
        tagid: i32,
        file_diameter: f32,
        filename_time: f32,
        file_extension_fallback: bool,
    ) -> Self {
        let mut pawn = Pawn::new(fullpath.to_string(), pos, tagid);
        pawn.hidden = true;
        // C++: `gGourceFileDiameter * 1.05` (a double literal).
        pawn.size = (file_diameter as f64 * 1.05) as f32;
        let radius = pawn.size * 0.5;

        pawn.speed = 5.0;
        pawn.nametime = filename_time;
        pawn.name_interval = pawn.nametime;
        pawn.namecol = Vec3::ONE;
        pawn.shadow = true;

        let (path, name, ext) = Self::parse_path(fullpath, file_extension_fallback);
        pawn.name = name;
        let target_size = pawn.size;

        Self {
            pawn,
            file_colour: colour,
            touch_colour: Vec3::ONE,
            dir: None,
            removed_timestamp: 0,
            forced_removal: false,
            expired: false,
            removing: false,
            fade_start: -1.0,
            last_action: 0.0,
            radius,
            dest: Vec2::ZERO,
            distance: 0.0,
            vel: Vec2::ZERO,
            path,
            fullpath: fullpath.to_string(),
            ext,
            lines: 0,
            byte_size: 0,
            touch_count: 0,
            target_size,
            pulse_timer: 0.0,
            pulse_max_time: 0.4,
            pulse_delta: 0,
            pulse_scale: 0.0,
            total_added: 0,
            total_removed: 0,
            created_timestamp: 0,
            dominant_cohort_colour: None,
            weighted: false,
            is_shadow: false,
            shadow_alpha: 0.45,
            solidifying: false,
            solidify_timer: 0.0,
        }
    }

    /// Parse fullpath into `(path, filename, ext)`.
    /// Port of `RFile::setFilename`.
    pub fn parse_path(abs_file_path: &str, extension_fallback: bool) -> (String, String, String) {
        let (path, name) = match abs_file_path.rfind('/') {
            Some(pos) => (
                abs_file_path[..=pos].to_string(),
                abs_file_path[pos + 1..].to_string(),
            ),
            None => (String::new(), abs_file_path.to_string()),
        };

        let ext = match name.rfind('.') {
            Some(dot) if dot != name.len() - 1 => name[dot + 1..].to_string(),
            _ if extension_fallback => name.clone(),
            _ => String::new(),
        };

        (path, name, ext)
    }

    /// Port of `RFile::remove(time_t removed_timestamp)`.
    pub fn remove_with_timestamp(&mut self, removed_timestamp: i64) {
        self.last_action = self.pawn.elapsed;
        self.fade_start = self.pawn.elapsed;
        self.removing = true;
        self.removed_timestamp = removed_timestamp;
    }

    /// Port of `RFile::remove()`.
    pub fn remove_forced(&mut self) {
        self.forced_removal = true;
        self.remove_with_timestamp(0);
    }

    /// Port of `RFile::getAbsolutePos()`.
    pub fn absolute_pos(&self, dir_pos: Vec2) -> Vec2 {
        self.pawn.pos + dir_pos
    }

    /// Port of `RFile::overlaps(const vec2& pos)`.
    pub fn overlaps(&self, dir_pos: Vec2, point: Vec2) -> bool {
        let abs_pos = self.absolute_pos(dir_pos);
        let halfsize_x = self.pawn.size * 0.5;
        let halfsize = Vec2::new(halfsize_x, halfsize_x * self.pawn.graphic_ratio);
        let bounds = Bounds2D::from_points(abs_pos - halfsize, abs_pos + halfsize);
        bounds.contains(point)
    }

    /// Port of `RFile::colourize()`.
    pub fn colourize(&mut self, hasher: &gource_core::StringHasher) {
        self.file_colour = if !self.ext.is_empty() {
            hasher.colour_hash(&self.ext)
        } else {
            Vec3::ONE
        };
    }

    /// Port of `RFile::getNameColour()`.
    pub fn name_colour(&self, selection_colour: Vec3) -> Vec3 {
        if self.pawn.selected {
            selection_colour
        } else {
            self.pawn.namecol
        }
    }

    /// Port of `RFile::getColour()`.
    pub fn colour(&self) -> Vec3 {
        if self.pawn.selected {
            return Vec3::ONE;
        }
        let lc = self.pawn.elapsed - self.last_action;
        if lc < 1.0 {
            self.touch_colour * (1.0 - lc) + self.file_colour * lc
        } else {
            self.file_colour
        }
    }

    /// Port of `RFile::getAlpha()`.
    pub fn alpha(&self) -> f32 {
        let mut alpha = self.pawn.alpha();
        if self.fade_start > 0.0 {
            let fade_elapsed = (self.pawn.elapsed - self.fade_start).clamp(0.0, 1.0);
            alpha = 1.0 - fade_elapsed;
        }
        if self.is_shadow {
            alpha *= self.shadow_alpha;
        } else if self.solidifying && self.solidify_timer > 0.0 {
            // Smoothly ramp up from shadow_alpha to 1.0 over solidify_timer (0.5s)
            let progress = 1.0 - (self.solidify_timer / 0.5).clamp(0.0, 1.0);
            let blend = self.shadow_alpha + (1.0 - self.shadow_alpha) * progress;
            alpha *= blend;
        }
        alpha
    }

    /// Solidify a shadow file into a permanent committed file.
    pub fn solidify(&mut self) {
        if self.is_shadow {
            self.is_shadow = false;
            self.solidifying = true;
            self.solidify_timer = 0.5;
            // Trigger visual pulse ring
            self.pulse_timer = self.pulse_max_time;
            self.pulse_scale = 1.5;
            self.pulse_delta = 1;
        }
    }

    /// Port of `RFile::touch(time_t touched_timestamp, const vec3 & colour)`.
    /// Returns true if this un-expired the file.
    pub fn touch(&mut self, touched_timestamp: i64, colour: Vec3) -> bool {
        if self.forced_removal || (self.removing && touched_timestamp < self.removed_timestamp) {
            return false;
        }

        self.fade_start = -1.0;
        self.removing = false;
        self.removed_timestamp = 0;
        self.last_action = self.pawn.elapsed;
        self.touch_colour = colour;

        let was_expired = self.expired;
        self.expired = false;
        self.touch_count = self.touch_count.saturating_add(1);

        self.pawn.show_name();
        self.pawn.set_hidden(false);

        was_expired
    }

    /// Port of `RFile::logic(float dt)`.
    /// Returns `true` if the file has completely faded out and just transitioned to expired.
    pub fn logic(&mut self, dt: f32, file_idle_time: f32) -> bool {
        self.pawn.logic(dt);

        let dest_pos = self.dest * self.distance;
        self.pawn.accel = dest_pos - self.pawn.pos;

        let mut accel2 = self.pawn.accel * self.pawn.speed * dt;
        if accel2.length_squared() > self.pawn.accel.length_squared() {
            accel2 = self.pawn.accel;
        }
        self.pawn.pos += accel2;
        self.pawn.accel = Vec2::ZERO;

        if self.solidifying {
            self.solidify_timer = (self.solidify_timer - dt).max(0.0);
            if self.solidify_timer == 0.0 {
                self.solidifying = false;
            }
        }

        if self.weighted {
            let target = if self.pawn.is_hidden() || self.removing || self.fade_start > 0.0 {
                0.0
            } else {
                self.target_size
            };
            if (self.pawn.size - target).abs() > 1e-4 {
                self.pawn.size += (target - self.pawn.size) * (dt * 4.0).min(1.0);
                self.radius = (self.pawn.size * 0.5).max(0.05);
                self.pawn.dims = Vec2::splat(self.pawn.size);
            }
        }
        if self.pulse_timer > 0.0 {
            self.pulse_timer = (self.pulse_timer - dt).max(0.0);
        }

        if self.fade_start < 0.0
            && file_idle_time > 0.0
            && (self.pawn.elapsed - self.last_action) > file_idle_time
        {
            self.fade_start = self.pawn.elapsed;
        }

        let mut just_expired = false;
        if self.fade_start > 0.0 && !self.expired && (self.pawn.elapsed - self.fade_start) >= 1.0 {
            self.expired = true;
            just_expired = true;
        }

        if self.pawn.is_hidden() && !self.forced_removal {
            self.pawn.elapsed = 0.0;
        }

        just_expired
    }

    /// Update lines count, lifetime added/removed, and trigger pulse animation if configured.
    pub fn apply_line_delta(
        &mut self,
        added: Option<u32>,
        removed: Option<u32>,
        pulse_setting: f32,
    ) {
        let add = added.unwrap_or(0);
        let rem = removed.unwrap_or(0);
        self.lines = (self.lines as i64 + add as i64 - rem as i64).max(0) as u32;
        self.total_added = self.total_added.saturating_add(add as u64);
        self.total_removed = self.total_removed.saturating_add(rem as u64);
        if pulse_setting > 0.0 && (add > 0 || rem > 0) {
            self.pulse_delta = add as i32 - rem as i32;
            self.pulse_timer = self.pulse_max_time;
            self.pulse_scale = (1.0 + (add + rem) as f32).ln() * pulse_setting;
        }
    }

    /// Set target diameter based on weight relative to a reference weight.
    pub fn set_weight_target(&mut self, weight: f32, ref_weight: f32, base_diameter: f32) {
        self.weighted = true;
        let factor = (weight.max(1.0) / ref_weight.max(1.0))
            .sqrt()
            .clamp(0.5, 4.0);
        self.target_size = (base_diameter as f64 * 1.05) as f32 * factor;
    }

    /// Compute active pulse ring size and color (if currently pulsing).
    pub fn pulse_visual(&self) -> Option<(f32, glam::Vec4)> {
        if self.pulse_timer > 0.0 && self.pulse_scale > 0.0 {
            let progress = 1.0 - (self.pulse_timer / self.pulse_max_time).clamp(0.0, 1.0);
            let ring_size =
                self.pawn.size * (1.0 + progress * (0.6 + 0.3 * self.pulse_scale.min(4.0)));
            let alpha = (1.0 - progress) * 0.65 * self.alpha();
            let col = if self.pulse_delta >= 0 {
                glam::Vec4::new(0.25, 0.95, 0.45, alpha)
            } else {
                glam::Vec4::new(0.95, 0.3, 0.3, alpha)
            };
            Some((ring_size, col))
        } else {
            None
        }
    }

    /// Ratio of lines removed to total lifetime touched lines (0.0 to 1.0).
    pub fn churn_ratio(&self) -> f32 {
        let total = self.total_added + self.total_removed;
        if total == 0 {
            0.0
        } else {
            self.total_removed as f32 / total as f32
        }
    }

    /// Compute file display colour depending on active colour mode.
    pub fn display_colour(
        &self,
        mode: gource_settings::FileColourMode,
        current_timestamp: i64,
    ) -> Vec3 {
        let base_col = match mode {
            gource_settings::FileColourMode::Extension => self.file_colour,
            gource_settings::FileColourMode::Churn => {
                let churn = self.churn_ratio();
                let cool = Vec3::new(0.2, 0.65, 0.95);
                let hot = Vec3::new(1.0, 0.25, 0.15);
                cool.lerp(hot, churn)
            }
            gource_settings::FileColourMode::Age => {
                let age_days =
                    ((current_timestamp - self.created_timestamp).max(0) as f32) / 86400.0;
                let t = (age_days / 365.0).clamp(0.0, 1.0);
                let fresh = Vec3::new(0.1, 0.9, 0.7);
                let ancient = Vec3::new(0.85, 0.4, 0.7);
                fresh.lerp(ancient, t)
            }
            gource_settings::FileColourMode::Cohort => {
                self.dominant_cohort_colour.unwrap_or(self.file_colour)
            }
        };

        if self.pawn.selected {
            return Vec3::ONE;
        }
        let lc = self.pawn.elapsed - self.last_action;
        if lc < 1.0 {
            self.touch_colour * (1.0 - lc) + base_col * lc
        } else {
            base_col
        }
    }

    /// Label display text (extension or full name).
    pub fn display_name(&self, file_extensions: bool) -> &str {
        if file_extensions && !self.ext.is_empty() {
            &self.ext
        } else {
            &self.pawn.name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::StringHasher;

    #[test]
    fn parse_path_cases() {
        let (p1, n1, e1) = File::parse_path("/a/b/c.rs", false);
        assert_eq!(p1, "/a/b/");
        assert_eq!(n1, "c.rs");
        assert_eq!(e1, "rs");

        let (p2, n2, e2) = File::parse_path("Makefile", false);
        assert_eq!(p2, "");
        assert_eq!(n2, "Makefile");
        assert_eq!(e2, "");

        let (p3, n3, e3) = File::parse_path("Makefile", true);
        assert_eq!(p3, "");
        assert_eq!(n3, "Makefile");
        assert_eq!(e3, "Makefile");

        let (p4, n4, e4) = File::parse_path("/a/b/file.", true);
        assert_eq!(p4, "/a/b/");
        assert_eq!(n4, "file.");
        assert_eq!(e4, "file.");
    }

    #[test]
    fn file_lifecycle_and_fade() {
        let mut f = File::new("/foo/bar.txt", Vec3::ONE, Vec2::ZERO, 1, 8.0, 4.0, false);
        assert!(f.pawn.is_hidden());
        assert_eq!(f.ext, "txt");
        assert_eq!(f.pawn.name, "bar.txt");

        let unexp = f.touch(100, Vec3::new(1.0, 0.0, 0.0));
        assert!(!unexp);
        assert!(!f.pawn.is_hidden());
        assert_eq!(f.touch_colour, Vec3::new(1.0, 0.0, 0.0));

        // Logic step
        f.dest = Vec2::new(1.0, 0.0);
        f.distance = 10.0;
        f.logic(0.1, 5.0);
        assert!(f.pawn.pos.x > 0.0);

        // Colour blending within 1s of touch
        let c = f.colour();
        assert!(c.x > 0.5);

        // Expire file
        f.remove_with_timestamp(200);
        assert!(f.removing);
        let expired = f.logic(1.5, 0.0);
        assert!(expired);
        assert!(f.expired);
        assert_eq!(f.alpha(), 0.0);

        // Re-touch unexpires
        let unexp2 = f.touch(201, Vec3::ONE);
        assert!(unexp2);
        assert!(!f.expired);
        assert!(!f.removing);

        // Colourize
        let hasher = StringHasher::default();
        f.colourize(&hasher);
        assert_eq!(f.file_colour, hasher.colour_hash("txt"));

        // Forced removal
        f.remove_forced();
        assert!(f.forced_removal);
        // touch should be ignored
        assert!(!f.touch(300, Vec3::ZERO));

        // Display name
        assert_eq!(f.display_name(true), "txt");
        assert_eq!(f.display_name(false), "bar.txt");

        // Overlaps
        assert!(f.overlaps(Vec2::ZERO, f.pawn.pos));
    }
}
