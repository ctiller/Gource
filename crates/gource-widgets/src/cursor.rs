//! MouseCursor: cursor state, button tracking, idle hiding, and custom cursor rendering
//! (port of `core/mousecursor.h`, `core/mousecursor.cpp`).
//!
//! Unlike the SDL-dependent C++ implementation, this struct is decoupled from windowing:
//! it consumes input state/events and exposes query methods and the desired platform cursor
//! visibility (`system_cursor_visible()`) for the platform layer to fulfill.

use glam::Vec2;
use gource_draw::{DrawList, TextureId};

/// Default idle timeout in seconds before hiding the cursor (from C++ `timeout = 3.0f`).
pub const DEFAULT_CURSOR_TIMEOUT: f32 = 3.0;

/// Mouse cursor tracking state and custom cursor rendering.
///
/// Port of C++ `MouseCursor`.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseCursor {
    mousepos: Vec2,
    rel: Vec2,
    hidden: bool,
    system_cursor: bool,
    idle: f32,
    timeout: f32,
    scrollwheel: i32,
    left_click: bool,
    right_click: bool,
    middle_click: bool,
    left_pressed: bool,
    right_pressed: bool,
    middle_pressed: bool,
    has_focus: bool,
}

impl Default for MouseCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseCursor {
    /// Create a new `MouseCursor`.
    ///
    /// Port of `MouseCursor::MouseCursor()`.
    pub fn new() -> Self {
        Self {
            mousepos: Vec2::ZERO,
            rel: Vec2::ZERO,
            hidden: false,
            system_cursor: true,
            timeout: DEFAULT_CURSOR_TIMEOUT,
            idle: DEFAULT_CURSOR_TIMEOUT,
            scrollwheel: 0,
            left_click: false,
            right_click: false,
            middle_click: false,
            left_pressed: false,
            right_pressed: false,
            middle_pressed: false,
            has_focus: true,
        }
    }

    /// Current cursor position in window coordinates.
    ///
    /// Port of `MouseCursor::getPos()`.
    pub fn pos(&self) -> Vec2 {
        self.mousepos
    }

    /// Relative motion vector of the cursor.
    ///
    /// Port of `MouseCursor::getRelativePos()`.
    pub fn relative_pos(&self) -> Vec2 {
        self.rel
    }

    /// Set relative motion vector.
    ///
    /// Port of `MouseCursor::updateRelativePos(const vec2& rel)`.
    pub fn update_relative_pos(&mut self, rel: Vec2) {
        self.rel = rel;
    }

    /// Update cursor position and reset idle timer.
    ///
    /// Port of `MouseCursor::updatePos(const vec2& pos)`.
    pub fn update_pos(&mut self, pos: Vec2) {
        self.mousepos = pos;
        self.idle = 0.0;
    }

    /// Set left click state.
    ///
    /// Port of `MouseCursor::leftClick(bool click)`.
    pub fn set_left_click(&mut self, click: bool) {
        self.left_click = click;
    }

    /// Set right click state.
    ///
    /// Port of `MouseCursor::rightClick(bool click)`.
    pub fn set_right_click(&mut self, click: bool) {
        self.right_click = click;
    }

    /// Set middle click state.
    ///
    /// Port of `MouseCursor::middleClick(bool click)`.
    pub fn set_middle_click(&mut self, click: bool) {
        self.middle_click = click;
    }

    /// Whether left click was triggered.
    ///
    /// Port of `MouseCursor::leftClick()`.
    pub fn left_click(&self) -> bool {
        self.left_click
    }

    /// Whether right click was triggered.
    ///
    /// Port of `MouseCursor::rightClick()`.
    pub fn right_click(&self) -> bool {
        self.right_click
    }

    /// Whether middle click was triggered.
    ///
    /// Port of `MouseCursor::middleClick()`.
    pub fn middle_click(&self) -> bool {
        self.middle_click
    }

    /// Set continuous button pressed state.
    pub fn set_button_down(&mut self, button: MouseButtonKind, down: bool) {
        match button {
            MouseButtonKind::Left => self.left_pressed = down,
            MouseButtonKind::Right => self.right_pressed = down,
            MouseButtonKind::Middle => self.middle_pressed = down,
        }
    }

    /// Whether left button is held down.
    ///
    /// Port of `MouseCursor::leftButtonPressed()`.
    pub fn left_button_pressed(&self) -> bool {
        self.left_pressed
    }

    /// Whether right button is held down.
    ///
    /// Port of `MouseCursor::rightButtonPressed()`.
    pub fn right_button_pressed(&self) -> bool {
        self.right_pressed
    }

    /// Whether middle button is held down.
    pub fn middle_button_pressed(&self) -> bool {
        self.middle_pressed
    }

    /// Whether both left and right buttons are held down.
    ///
    /// Port of `MouseCursor::bothPressed()`.
    pub fn both_pressed(&self) -> bool {
        self.left_pressed && self.right_pressed
    }

    /// Whether any button (left or right) is held down.
    ///
    /// Port of `MouseCursor::buttonPressed()`.
    pub fn button_pressed(&self) -> bool {
        self.left_pressed || self.right_pressed
    }

    /// Scroll wheel accumulated delta.
    ///
    /// Port of `MouseCursor::scrollWheel()`.
    pub fn scroll_wheel(&self) -> i32 {
        self.scrollwheel
    }

    /// Scroll event (true = up/positive, false = down/negative).
    ///
    /// Port of `MouseCursor::scroll(bool dir)`.
    pub fn scroll(&mut self, dir: bool) {
        self.scrollwheel += if dir { 1 } else { -1 };
    }

    /// Set window focus state.
    ///
    /// Port of `MouseCursor::hasFocus()`.
    pub fn set_focus(&mut self, focus: bool) {
        self.has_focus = focus;
    }

    /// Whether window currently has mouse focus.
    ///
    /// Port of `MouseCursor::hasFocus()`.
    pub fn has_focus(&self) -> bool {
        self.has_focus
    }

    /// Whether cursor is explicitly hidden.
    ///
    /// Port of `MouseCursor::isHidden()`.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Whether system cursor mode is enabled (true) or custom cursor texture is used (false).
    ///
    /// Port of `MouseCursor::isSystemCursor()`.
    pub fn is_system_cursor(&self) -> bool {
        self.system_cursor
    }

    /// Configure whether to use system cursor.
    ///
    /// Port of `MouseCursor::useSystemCursor(bool system_cursor)`.
    pub fn use_system_cursor(&mut self, system_cursor: bool) {
        self.system_cursor = system_cursor;
    }

    /// Show or hide cursor.
    ///
    /// Port of `MouseCursor::showCursor(bool show)`.
    pub fn show_cursor(&mut self, show: bool) {
        if self.hidden == !show {
            return;
        }
        self.hidden = !show;
        if show {
            self.idle = 0.0;
        }
    }

    /// Whether cursor is currently visible on screen.
    ///
    /// Port of `MouseCursor::isVisible()`: `(!hidden && idle < timeout && hasFocus())`.
    pub fn is_visible(&self) -> bool {
        !self.hidden && self.idle < self.timeout && self.has_focus
    }

    /// Query what the platform system cursor visibility should be set to.
    ///
    /// In C++:
    /// - If `system_cursor` is true: visible when `!hidden && idle < timeout && hasFocus()`.
    /// - If `system_cursor` is false: system cursor is always hidden (replaced by custom texture).
    pub fn system_cursor_visible(&self) -> bool {
        self.system_cursor && self.is_visible()
    }

    /// Reset transient click and scroll state for next frame/event.
    ///
    /// Port of `MouseCursor::resetButtonState()`.
    pub fn reset_button_state(&mut self) {
        self.scrollwheel = 0;
        self.right_click = false;
        self.left_click = false;
        self.middle_click = false;
    }

    /// Advance idle timer.
    ///
    /// Port of `MouseCursor::logic(float dt)`.
    pub fn logic(&mut self, dt: f32) {
        self.idle += dt;
    }

    /// Current idle time in seconds.
    pub fn idle_time(&self) -> f32 {
        self.idle
    }

    /// Timeout in seconds before hiding due to inactivity.
    pub fn timeout(&self) -> f32 {
        self.timeout
    }

    /// Set idle timeout.
    pub fn set_timeout(&mut self, timeout: f32) {
        self.timeout = timeout;
    }

    /// Render custom cursor into draw list when `!system_cursor` and visible.
    ///
    /// Port of `MouseCursor::draw()`.
    /// In C++:
    /// ```text
    /// if(system_cursor || cursortex == 0) return;
    /// if(!isVisible()) return;
    /// glTexCoord2f(0,0); glVertex2i(0, 0);
    /// glTexCoord2f(1,0); glVertex2i(cursortex->w, 0);
    /// glTexCoord2f(1,1); glVertex2i(cursortex->w, cursortex->h);
    /// glTexCoord2f(0,1); glVertex2i(0, cursortex->h);
    /// ```
    pub fn draw(&self, list: &mut DrawList, texture: TextureId, size: Vec2) {
        if self.system_cursor || !self.is_visible() {
            return;
        }
        list.rect(texture, self.mousepos, size, glam::Vec4::ONE);
    }
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButtonKind {
    Left,
    Right,
    Middle,
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec2;

    #[test]
    fn initial_state() {
        let cursor = MouseCursor::new();
        assert_eq!(cursor.pos(), Vec2::ZERO);
        assert_eq!(cursor.relative_pos(), Vec2::ZERO);
        assert!(!cursor.is_hidden());
        assert!(cursor.is_system_cursor());
        assert!(cursor.has_focus());
        // Initially idle == timeout, so is_visible is false!
        assert!(!cursor.is_visible());
        assert!(!cursor.system_cursor_visible());
    }

    #[test]
    fn update_pos_resets_idle() {
        let mut cursor = MouseCursor::new();
        cursor.update_pos(Vec2::new(100.0, 200.0));
        assert_eq!(cursor.pos(), Vec2::new(100.0, 200.0));
        assert_eq!(cursor.idle_time(), 0.0);
        assert!(cursor.is_visible());
        assert!(cursor.system_cursor_visible());

        cursor.logic(1.5);
        assert_eq!(cursor.idle_time(), 1.5);
        assert!(cursor.is_visible());

        cursor.logic(1.6); // Total idle = 3.1 >= 3.0
        assert!(!cursor.is_visible());
        assert!(!cursor.system_cursor_visible());
    }

    #[test]
    fn show_cursor_and_hidden() {
        let mut cursor = MouseCursor::new();
        cursor.update_pos(Vec2::new(50.0, 50.0));
        assert!(cursor.is_visible());

        cursor.show_cursor(false);
        assert!(cursor.is_hidden());
        assert!(!cursor.is_visible());

        // Calling show_cursor(false) again is a no-op
        cursor.show_cursor(false);
        assert!(cursor.is_hidden());

        cursor.show_cursor(true);
        assert!(!cursor.is_hidden());
        assert_eq!(cursor.idle_time(), 0.0);
        assert!(cursor.is_visible());
    }

    #[test]
    fn focus_affects_visibility() {
        let mut cursor = MouseCursor::new();
        cursor.update_pos(Vec2::new(10.0, 10.0));
        assert!(cursor.is_visible());

        cursor.set_focus(false);
        assert!(!cursor.has_focus());
        assert!(!cursor.is_visible());
        assert!(!cursor.system_cursor_visible());

        cursor.set_focus(true);
        assert!(cursor.is_visible());
    }

    #[test]
    fn system_vs_custom_cursor() {
        let mut cursor = MouseCursor::new();
        cursor.update_pos(Vec2::new(10.0, 10.0));
        cursor.use_system_cursor(false);
        assert!(!cursor.is_system_cursor());
        assert!(cursor.is_visible());
        // When custom cursor is active, system cursor is hidden
        assert!(!cursor.system_cursor_visible());
    }

    #[test]
    fn buttons_and_clicks() {
        let mut cursor = MouseCursor::new();
        assert!(!cursor.left_click());
        assert!(!cursor.right_click());
        assert!(!cursor.middle_click());
        assert!(!cursor.left_button_pressed());
        assert!(!cursor.right_button_pressed());
        assert!(!cursor.middle_button_pressed());
        assert!(!cursor.button_pressed());
        assert!(!cursor.both_pressed());

        cursor.set_left_click(true);
        cursor.set_right_click(true);
        cursor.set_middle_click(true);
        cursor.set_button_down(MouseButtonKind::Left, true);
        assert!(cursor.left_click());
        assert!(cursor.right_click());
        assert!(cursor.middle_click());
        assert!(cursor.left_button_pressed());
        assert!(cursor.button_pressed());
        assert!(!cursor.both_pressed());

        cursor.set_button_down(MouseButtonKind::Right, true);
        assert!(cursor.right_button_pressed());
        assert!(cursor.both_pressed());

        cursor.set_button_down(MouseButtonKind::Middle, true);
        assert!(cursor.middle_button_pressed());

        cursor.scroll(true);
        cursor.scroll(true);
        cursor.scroll(false);
        assert_eq!(cursor.scroll_wheel(), 1);

        cursor.reset_button_state();
        assert_eq!(cursor.scroll_wheel(), 0);
        assert!(!cursor.left_click());
        assert!(!cursor.right_click());
        assert!(!cursor.middle_click());
        // Continuous button state is retained across reset_button_state
        assert!(cursor.left_button_pressed());
    }

    #[test]
    fn relative_pos() {
        let mut cursor = MouseCursor::new();
        cursor.update_relative_pos(Vec2::new(5.0, -3.0));
        assert_eq!(cursor.relative_pos(), Vec2::new(5.0, -3.0));
    }

    #[test]
    fn custom_cursor_draw() {
        let mut cursor = MouseCursor::new();
        let mut list = DrawList::new(UVec2::new(800, 600));

        // System cursor = true: should NOT draw custom cursor
        cursor.update_pos(Vec2::new(100.0, 150.0));
        cursor.draw(&mut list, TextureId(5), Vec2::new(32.0, 32.0));
        assert!(list.is_empty());

        // Custom cursor, visible
        cursor.use_system_cursor(false);
        cursor.draw(&mut list, TextureId(5), Vec2::new(32.0, 32.0));
        assert_eq!(list.batches.len(), 1);
        assert_eq!(list.batches[0].texture, TextureId(5));
        let verts = &list.batches[0].vertices;
        assert_eq!(verts.len(), 4);
        assert_eq!(verts[0].pos, Vec2::new(100.0, 150.0));
        assert_eq!(verts[2].pos, Vec2::new(132.0, 182.0));

        // When not visible: should NOT draw
        list.reset(UVec2::new(800, 600), glam::Vec4::ZERO);
        cursor.show_cursor(false);
        cursor.draw(&mut list, TextureId(5), Vec2::new(32.0, 32.0));
        assert!(list.is_empty());
    }

    #[test]
    fn cursor_timeout_setting() {
        let mut cursor = MouseCursor::new();
        assert_eq!(cursor.timeout(), DEFAULT_CURSOR_TIMEOUT);
        cursor.set_timeout(5.0);
        assert_eq!(cursor.timeout(), 5.0);
    }

    #[test]
    fn cursor_default_and_exact_quad_geometry() {
        let default_cursor = MouseCursor::default();
        assert_eq!(default_cursor.pos(), Vec2::ZERO);

        let mut cursor = MouseCursor::new();
        cursor.use_system_cursor(false);
        cursor.update_pos(Vec2::new(50.0, 75.0));

        let mut list = DrawList::new(UVec2::new(800, 600));
        cursor.draw(&mut list, TextureId(3), Vec2::new(16.0, 24.0));

        assert_eq!(list.batches.len(), 1);
        let b = &list.batches[0];
        assert_eq!(b.texture, TextureId(3));
        assert_eq!(b.vertices.len(), 4);

        // Quad corners: top-left (50, 75), top-right (66, 75), bottom-right (66, 99), bottom-left (50, 99)
        assert_eq!(b.vertices[0].pos, Vec2::new(50.0, 75.0));
        assert_eq!(b.vertices[0].uv, Vec2::new(0.0, 0.0));
        assert_eq!(b.vertices[0].colour, glam::Vec4::ONE);

        assert_eq!(b.vertices[1].pos, Vec2::new(66.0, 75.0));
        assert_eq!(b.vertices[1].uv, Vec2::new(1.0, 0.0));

        assert_eq!(b.vertices[2].pos, Vec2::new(66.0, 99.0));
        assert_eq!(b.vertices[2].uv, Vec2::new(1.0, 1.0));

        assert_eq!(b.vertices[3].pos, Vec2::new(50.0, 99.0));
        assert_eq!(b.vertices[3].uv, Vec2::new(0.0, 1.0));
    }
}
