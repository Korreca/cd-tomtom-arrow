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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crimson_desert_tomtom::gui::App;
use crimson_desert_tomtom::logging;
use crimson_desert_tomtom::clog;

fn main() {
    logging::init();

    clog!("═══════════════════════════");
    clog!("Crimson Desert TomTom Arrow");
    clog!("═══════════════════════════");

    // Use exe directory for config
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let config_path = exe_dir.join("config.json");
    clog!("[*] Config path: {:?}", config_path);
    clog!("[*] Starting application...");

    // Run GUI with single-thread architecture
    // App initialization and ticking happens in GUI main loop via SetTimer
    App::run(&config_path.to_string_lossy());

    clog!("[*] Application shutdown complete");
}

