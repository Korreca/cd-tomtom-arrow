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

//! GUI — Slint control panel + Win32 overlay.

pub mod app_snapshot;
pub mod overlay_window;
pub mod renderer;
pub mod slint_app;
pub mod win32_helpers;

/// Application entry point.
pub struct App;

impl App {
    /// Run the Slint control panel on the main thread.
    /// The Win32 overlay is spawned as a background thread inside `slint_app::run`.
    pub fn run(config_path: &str) {
        slint_app::run(config_path);
    }
}

