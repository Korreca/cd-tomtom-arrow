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

use std::{collections::HashMap, path::PathBuf};

fn main() {
    #[cfg(target_os = "windows")]
    {
        winres::WindowsResource::new()
            .set_icon("src/assets/icon.ico")
            .set_manifest_file("CD_TomTom.exe.manifest")
            .compile()
            .expect("Failed to compile resource script");
    }

    let library_paths = HashMap::from([("lucide".to_string(), PathBuf::from(lucide_slint::lib()))]);

    let config = slint_build::CompilerConfiguration::new().with_library_paths(library_paths);

    slint_build::compile_with_config("src/gui/slint/control_panel.slint", config)
        .expect("Slint build failed");
}
