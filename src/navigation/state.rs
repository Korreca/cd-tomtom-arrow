// CD_TomTom - Navigation overlay tool for Crimson Desert.
// Copyright (C) 2026 Korreca <https://github.com/Korreca/cd-tomtom-arrow/>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Shared runtime state: position, marker, camera, visibility.

use std::sync::{Arc, Mutex};

/// Navigation state snapshot from memory.
#[derive(Debug, Clone, Copy, Default)]
pub struct NavigationState {
    /// Player absolute position (x, y, z) — already world-offset applied by the backend.
    pub player_pos: Option<(f32, f32, f32)>,
    /// Map marker destination (x, y, z, flag)
    pub marker_dest: Option<(f32, f32, f32, u32)>,
    /// Camera heading in degrees
    pub camera_heading: Option<f32>,
}

impl NavigationState {
    /// Get the player's absolute position.
    /// The world offset is applied by the backend before storing, so this
    /// returns `player_pos` directly.
    pub fn absolute_player_pos(&self) -> Option<(f32, f32, f32)> {
        self.player_pos
    }
}

/// Display and interaction state for the overlay.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayDisplayState {
    /// Is overlay currently visible?
    pub visible: bool,
    /// Current arrow rotation in degrees
    pub turn_angle: f32,
    /// Distance to marker in metres
    pub distance: f32,
    /// Height difference (marker_y - player_y)
    pub height_diff: f32,
}

/// Encapsulates all mutable runtime state.
#[derive(Debug)]
pub struct RuntimeState {
    nav: Arc<Mutex<NavigationState>>,
    overlay: Arc<Mutex<OverlayDisplayState>>,
}

impl RuntimeState {
    /// Create a new runtime state.
    pub fn new() -> Self {
        Self {
            nav: Arc::new(Mutex::new(NavigationState::default())),
            overlay: Arc::new(Mutex::new(OverlayDisplayState::default())),
        }
    }

    /// Update navigation state.
    pub fn set_navigation(&self, state: NavigationState) {
        if let Ok(mut nav) = self.nav.lock() {
            *nav = state;
        }
    }

    /// Get current navigation state snapshot.
    pub fn get_navigation(&self) -> NavigationState {
        self.nav.lock().ok().map(|n| *n).unwrap_or_default()
    }

    /// Update overlay display state.
    pub fn set_overlay_display(&self, state: OverlayDisplayState) {
        if let Ok(mut ovr) = self.overlay.lock() {
            *ovr = state;
        }
    }

    /// Get current overlay display state snapshot.
    pub fn get_overlay_display(&self) -> OverlayDisplayState {
        self.overlay.lock().ok().map(|o| *o).unwrap_or_default()
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RuntimeState {
    fn clone(&self) -> Self {
        Self {
            nav: Arc::clone(&self.nav),
            overlay: Arc::clone(&self.overlay),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── NavigationState::absolute_player_pos ────────────────────────────────

    #[test]
    fn absolute_pos_returns_player_pos() {
        let state = NavigationState {
            player_pos: Some((3.0, 1.0, 7.0)),
            ..Default::default()
        };
        assert_eq!(state.absolute_player_pos(), Some((3.0, 1.0, 7.0)));
    }

    #[test]
    fn absolute_pos_none_when_no_player_pos() {
        let state = NavigationState {
            player_pos: None,
            ..Default::default()
        };
        assert!(state.absolute_player_pos().is_none());
    }

    // ── RuntimeState getters / setters ──────────────────────────────────────

    #[test]
    fn runtime_state_nav_roundtrip() {
        let rs = RuntimeState::new();
        let nav = NavigationState {
            player_pos: Some((1.0, 2.0, 3.0)),
            camera_heading: Some(45.0),
            ..Default::default()
        };
        rs.set_navigation(nav);
        let got = rs.get_navigation();
        assert_eq!(got.player_pos, Some((1.0, 2.0, 3.0)));
        assert_eq!(got.camera_heading, Some(45.0));
    }

    #[test]
    fn runtime_state_clone_shares_state() {
        // Clone shares the same Arcs — a write on one is visible on the other.
        let rs = RuntimeState::new();
        let rs2 = rs.clone();
        let nav = NavigationState {
            player_pos: Some((1.0, 2.0, 3.0)),
            ..Default::default()
        };
        rs.set_navigation(nav);
        assert_eq!(
            rs2.get_navigation().player_pos,
            Some((1.0, 2.0, 3.0)),
            "clone sees same Arc"
        );
    }
}
