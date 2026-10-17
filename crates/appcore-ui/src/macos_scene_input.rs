// =============================================================================
//        #######
//     ###       ###     F: macos_scene_input.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Opt-in viewport navigation; fixed state, no raw key text or event queue.
use crate::{UiError, UiNativeWindow, UiResult};
use objc2_app_kit::{NSEvent, NSEventModifierFlags as Flags, NSEventType as Kind};
use std::time::Duration;
/// Bounded orbit/zoom deltas for one frame, independent of rendering or command authority.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiNativeSceneInput {
    /// Yaw/pitch deltas in radians.
    pub orbit: [f32; 2],
    /// Distance delta in scene units; negative approaches the target.
    pub zoom: f32,
    /// Viewport owns navigation focus; click controls or Escape to release it.
    pub active: bool,
}
#[derive(Default)]
pub(crate) struct SceneInput {
    pub(crate) top: u32,
    active: bool,
    dragging: bool,
    keys: u16,
    orbit: [f32; 2],
    zoom: f32,
}
fn key(code: u16) -> Option<u16> {
    [13, 1, 0, 2, 123, 124, 126, 125]
        .iter()
        .position(|v| *v == code)
        .map(|i| 1 << i)
}
impl SceneInput {
    fn clear(&mut self) {
        self.active = false;
        self.dragging = false;
        self.keys = 0;
        self.orbit = [0.; 2];
        self.zoom = 0.;
    }
    fn button(&mut self, inside: bool) {
        self.clear();
        self.active = inside;
        self.dragging = inside;
    }
    fn movement(&mut self, dx: f64, dy: f64) {
        if self.dragging && dx.is_finite() && dy.is_finite() {
            self.orbit[0] = (self.orbit[0] + dx.clamp(-200., 200.) as f32 * 0.006).clamp(-3., 3.);
            self.orbit[1] = (self.orbit[1] + dy.clamp(-200., 200.) as f32 * 0.006).clamp(-3., 3.);
        }
    }
    fn keyboard(&mut self, code: u16, down: bool, shortcut: bool) -> bool {
        if code == 53 || code == 48 {
            let consumed = self.active && code == 53;
            self.clear();
            return consumed;
        }
        let Some(bit) = key(code) else {
            return false;
        };
        if !down {
            self.keys &= !bit;
        }
        if shortcut {
            self.keys = 0;
            return false;
        }
        if !self.active {
            return false;
        }
        if down {
            self.keys |= bit;
        }
        true
    }
    fn take(&mut self, dt: Duration) -> UiNativeSceneInput {
        let dt = dt.as_secs_f32().min(0.05);
        let held = |mask| if (self.keys & mask) != 0 { 1. } else { 0. };
        let value = UiNativeSceneInput {
            orbit: [
                self.orbit[0] + (held(8 | 32) - held(4 | 16)) * dt * 1.5,
                self.orbit[1] + (held(128) - held(64)) * dt * 1.2,
            ],
            zoom: self.zoom + (held(2) - held(1)) * dt * 5.,
            active: self.active,
        };
        self.orbit = [0.; 2];
        self.zoom = 0.;
        value
    }
}
impl UiNativeWindow {
    /// Opt into orbit/zoom input below an overlay measured in logical points from the top.
    /// Click the viewport to focus; controls, Tab/Escape and focus loss release navigation.
    /// W/S zoom, A/D or left/right arrows orbit; up/down arrows tilt. OS shortcuts pass through.
    pub fn enable_scene_input(&self, overlay_height: u32) -> UiResult<()> {
        if !self.graphics_created.get() || overlay_height > 4096 {
            return Err(UiError::InvalidRequest);
        }
        *self.scene_input.borrow_mut() = Some(SceneInput {
            top: overlay_height,
            ..Default::default()
        });
        Ok(())
    }
    /// Take accumulated deltas and held-key movement, clamping elapsed time to 50 ms.
    /// This does not move a camera automatically or expose key text.
    pub fn take_scene_input(&self, elapsed: Duration) -> UiResult<UiNativeSceneInput> {
        let mut state = self.scene_input.borrow_mut();
        let state = state.as_mut().ok_or(UiError::InvalidRequest)?;
        if !self.window.isKeyWindow() || self.window.isMiniaturized() {
            state.clear();
        }
        Ok(state.take(elapsed))
    }
    pub(crate) fn scene_event(&self, event: &NSEvent) -> bool {
        if event.windowNumber() != self.window.windowNumber() {
            return false;
        }
        let mut state = self.scene_input.borrow_mut();
        let Some(state) = state.as_mut() else {
            return false;
        };
        let Some(view) = self.window.contentView() else {
            state.clear();
            return false;
        };
        let point = event.locationInWindow();
        let size = view.bounds().size;
        let inside = point.x >= 0.
            && point.x < size.width
            && point.y >= 0.
            && point.y < size.height - f64::from(state.top);
        let shortcut = event
            .modifierFlags()
            .intersects(Flags::Command | Flags::Control | Flags::Option);
        match event.r#type() {
            Kind::LeftMouseDown => state.button(inside),
            Kind::LeftMouseUp => state.dragging = false,
            Kind::LeftMouseDragged => state.movement(event.deltaX(), event.deltaY()),
            Kind::ScrollWheel if inside => {
                let delta = event.scrollingDeltaY();
                if delta.is_finite() {
                    state.zoom =
                        (state.zoom - delta.clamp(-50., 50.) as f32 * 0.12).clamp(-10., 10.);
                }
            }
            Kind::KeyDown | Kind::KeyUp => {
                return state.keyboard(event.keyCode(), event.r#type() == Kind::KeyDown, shortcut)
            }
            Kind::FlagsChanged if shortcut => state.keys = 0,
            _ => {}
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_held_keys_shortcuts_and_deltas_are_bounded() {
        let mut state = SceneInput::default();
        assert!(!state.keyboard(13, true, false));
        state.button(true);
        assert!(state.keyboard(13, true, false));
        assert!(state.keyboard(13, true, false));
        assert_eq!(state.take(Duration::from_secs(2)).zoom, -0.25);
        state.keyboard(13, false, false);
        assert_eq!(state.take(Duration::ZERO).zoom, 0.);
        state.movement(f64::INFINITY, 1.);
        assert_eq!(state.take(Duration::ZERO).orbit, [0.; 2]);
        state.movement(10000., 10000.);
        assert!(state.take(Duration::ZERO).orbit[0] <= 3.);
        assert_eq!(state.take(Duration::ZERO).orbit, [0.; 2]);
        state.keyboard(0, true, false);
        assert!(!state.keyboard(0, true, true));
        assert_eq!(state.keys, 0);
        state.button(false);
        assert!(!state.active);
        state.button(true);
        state.keyboard(53, true, false);
        assert!(!state.active);
    }
}
