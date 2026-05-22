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

/// Lock-free snapshot of application state, populated each refresh via `AppState::create_snapshot()`.
#[derive(Clone, Debug)]
pub struct AppSnapshot {
    // Connection status
    pub process_attached: bool,
    pub hooks_installed: bool,
    
    // Position data
    pub player_x: Option<f32>,
    pub player_y: Option<f32>,
    pub player_z: Option<f32>,
    pub camera_heading: Option<f32>,
    
    // Marker data
    pub marker_x: Option<f32>,
    pub marker_y: Option<f32>,
    pub marker_z: Option<f32>,
    pub marker_detected: bool,
    
    // Overlay display
    pub overlay_visible: bool,
    pub distance: f32,
    pub turn_angle: f32,
    pub height_diff: f32,
    
    // Debug logs
    pub debug_text: String,

    // Whether the app loop is still running (false = max retries hit)
    pub running: bool,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            process_attached: false,
            hooks_installed: false,
            player_x: None,
            player_y: None,
            player_z: None,
            camera_heading: None,
            marker_x: None,
            marker_y: None,
            marker_z: None,
            marker_detected: false,
            overlay_visible: false,
            distance: 0.0,
            turn_angle: 0.0,
            height_diff: 0.0,
            debug_text: String::new(),
            running: true,
        }
    }
}
