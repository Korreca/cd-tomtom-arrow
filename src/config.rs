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

//! Configuration management for overlays and main window.

use serde::{Deserialize, Serialize, Serializer};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;

/// Overlay display and behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// X position of overlay window
    pub x: i32,
    /// Y position of overlay window
    pub y: i32,
    /// Whether overlay is locked (no dragging)
    pub locked: bool,
    /// Opacity from 0.10 to 1.0
    #[serde(default = "default_opacity", serialize_with = "serialize_f32_2dp")]
    pub opacity: f32,
    /// Scale multiplier from 0.5 to 2.5
    #[serde(default = "default_scale", serialize_with = "serialize_f32_2dp")]
    pub scale: f32,
    /// Auto-hide overlay when distance is below N meters (0 = disabled)
    pub hide_below_m: u32,
    /// Offset X for distance text rendering
    pub text_offset_x: i32,
    /// Offset Y for distance text rendering
    pub text_offset_y: i32,
    /// Scale multiplier for text size
    #[serde(default = "default_text_scale", serialize_with = "serialize_f32_2dp")]
    pub text_scale: f32,
    /// Keep overlay hidden after going below threshold until marker changes
    #[serde(default = "default_true")]
    pub sticky_hide: bool,
    /// Hide overlay when inactive (no motion detected)
    #[serde(default = "default_true")]
    pub hide_on_inactive: bool,
    /// Inactivity timeout in milliseconds before hiding
    #[serde(default = "default_inactive_ms")]
    pub inactive_ms: u32,
    /// Offset X for info panel rendering
    #[serde(default)]
    pub info_offset_x: i32,
    /// Offset Y for info panel rendering
    #[serde(default)]
    pub info_offset_y: i32,
    /// Scale multiplier for info panel text
    #[serde(default = "default_scale", serialize_with = "serialize_f32_2dp")]
    pub info_scale: f32,
    /// Hide info panel
    #[serde(default = "default_true")]
    pub info_hidden: bool,
}

// ── Validation bounds ────────────────────────────────────────────────────────
const OPACITY_MIN: f32     = 0.10;
const OPACITY_MAX: f32     = 1.0;
const SCALE_MIN: f32       = 0.5;
const SCALE_MAX: f32       = 2.5;
const OFFSET_MIN: i32      = -250;
const OFFSET_MAX: i32      = 250;
const HIDE_BELOW_MAX: u32  = 500;
const INACTIVE_MS_MIN: u32 = 500;
const INACTIVE_MS_MAX: u32 = 10_000;
const WIN_WIDTH_MIN: u32   = 920;
const WIN_WIDTH_MAX: u32   = 3_840;
const WIN_HEIGHT_MIN: u32  = 680;
const WIN_HEIGHT_MAX: u32  = 2_160;

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            x: 90,
            y: 90,
            locked: true,
            opacity: 1.0,
            scale: 1.0,
            hide_below_m: 10,
            text_offset_x: 0,
            text_offset_y: 0,
            text_scale: 1.0,
            sticky_hide: true,
            hide_on_inactive: true,
            inactive_ms: 1200,
            info_offset_x: 0,
            info_offset_y: 0,
            info_scale: 1.0,
            info_hidden: true,
        }
    }
}

/// Main control window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainWindowConfig {
    /// Window width
    pub width: u32,
    /// Window height
    pub height: u32,
    /// X position (None = centered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    /// Y position (None = centered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
}

/// Root configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub main_window: MainWindowConfig,
    pub overlay: OverlayConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            main_window: MainWindowConfig {
                width: 920,
                height: 680,
                x: None,
                y: None,
            },
            overlay: OverlayConfig::default(),
        }
    }
}

fn default_opacity() -> f32 {
    1.0
}

fn default_scale() -> f32 {
    1.0
}

fn default_text_scale() -> f32 {
    1.0
}

fn default_inactive_ms() -> u32 {
    1200
}

fn default_true() -> bool {
    true
}


#[allow(clippy::trivially_copy_pass_by_ref)] // serde serialize_with requires &T
fn serialize_f32_2dp<S: Serializer>(val: &f32, s: S) -> Result<S::Ok, S::Error> {
    let rounded = (f64::from(*val) * 100.0).round() / 100.0;
    s.serialize_f64(rounded)
}

/// Manages config persistence.
pub struct ConfigStore {
    path: PathBuf,
    config: Config,
}

impl ConfigStore {
    /// Create a new config store tied to a specific file path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let path_exists = path.exists();
        let config = Config::load_from_file(&path).unwrap_or_default();
        
        let store = Self { path, config };
        
        // Create config file with defaults if it didn't exist
        if !path_exists
            && let Err(e) = store.save() {
                crate::clog!("Warning: Could not create config.json: {}", e);
            }
        
        store
    }

    /// Reload config from the associated file, falling back to defaults on error.
    pub fn load(&mut self) {
        self.config = Config::load_from_file(&self.path).unwrap_or_default();
    }

    /// Save the current config to the associated file.
    pub fn save(&self) -> AppResult<()> {
        self.config.save_to_file(&self.path)
    }

    /// Get a reference to the config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get a mutable reference to the config.
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Replace the entire config.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }
}

impl Config {
    /// Load config from a JSON file. Returns Ok(default) if file does not exist.
    pub fn load_from_file(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let json = fs::read_to_string(path)?;
        let config = serde_json::from_str::<Self>(&json)?;
        Ok(config)
    }

    /// Save config to a JSON file with nice formatting.
    pub fn save_to_file(&self, path: &Path) -> AppResult<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Validate config values are in safe ranges.
    pub fn validate(&mut self) {
        self.overlay.opacity     = self.overlay.opacity.clamp(OPACITY_MIN, OPACITY_MAX);
        self.overlay.scale       = self.overlay.scale.clamp(SCALE_MIN, SCALE_MAX);
        self.overlay.text_scale  = self.overlay.text_scale.clamp(SCALE_MIN, SCALE_MAX);
        self.overlay.text_offset_x = self.overlay.text_offset_x.clamp(OFFSET_MIN, OFFSET_MAX);
        self.overlay.text_offset_y = self.overlay.text_offset_y.clamp(OFFSET_MIN, OFFSET_MAX);
        self.overlay.hide_below_m  = self.overlay.hide_below_m.min(HIDE_BELOW_MAX);
        self.overlay.inactive_ms   = self.overlay.inactive_ms.clamp(INACTIVE_MS_MIN, INACTIVE_MS_MAX);
        self.overlay.info_scale    = self.overlay.info_scale.clamp(SCALE_MIN, SCALE_MAX);
        self.overlay.info_offset_x = self.overlay.info_offset_x.clamp(OFFSET_MIN, OFFSET_MAX);
        self.overlay.info_offset_y = self.overlay.info_offset_y.clamp(OFFSET_MIN, OFFSET_MAX);
        self.main_window.width  = self.main_window.width.clamp(WIN_WIDTH_MIN, WIN_WIDTH_MAX);
        self.main_window.height = self.main_window.height.clamp(WIN_HEIGHT_MIN, WIN_HEIGHT_MAX);
    }
}

/// Helper function to save overlay config to a file, merging with existing config
pub fn save_overlay_config_to_file(path: &Path, overlay_config: &OverlayConfig) -> AppResult<()> {
    // Load existing config or create default
    let mut config = if path.exists() {
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json).unwrap_or_default()
    } else {
        Config::default()
    };
    
    // Update only the overlay section
    config.overlay = overlay_config.clone();
    
    // Save back
    config.save_to_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── validate() clamping ─────────────────────────────────────────────────

    #[test]
    fn validate_clamps_opacity_to_minimum() {
        let mut cfg = Config::default();
        cfg.overlay.opacity = 0.0; // below OPACITY_MIN (0.10)
        cfg.validate();
        assert_eq!(cfg.overlay.opacity, OPACITY_MIN);
    }

    #[test]
    fn validate_clamps_opacity_to_maximum() {
        let mut cfg = Config::default();
        cfg.overlay.opacity = 2.0; // above OPACITY_MAX (1.0)
        cfg.validate();
        assert_eq!(cfg.overlay.opacity, OPACITY_MAX);
    }

    #[test]
    fn validate_clamps_scale_bounds() {
        let mut cfg = Config::default();
        cfg.overlay.scale = 0.1; // below SCALE_MIN (0.5)
        cfg.validate();
        assert_eq!(cfg.overlay.scale, SCALE_MIN);

        cfg.overlay.scale = 99.0; // above SCALE_MAX (2.5)
        cfg.validate();
        assert_eq!(cfg.overlay.scale, SCALE_MAX);
    }

    #[test]
    fn validate_clamps_text_offsets() {
        let mut cfg = Config::default();
        cfg.overlay.text_offset_x = -999;
        cfg.overlay.text_offset_y = 999;
        cfg.validate();
        assert_eq!(cfg.overlay.text_offset_x, OFFSET_MIN);
        assert_eq!(cfg.overlay.text_offset_y, OFFSET_MAX);
    }

    #[test]
    fn validate_clamps_hide_below_m() {
        let mut cfg = Config::default();
        cfg.overlay.hide_below_m = 9999;
        cfg.validate();
        assert_eq!(cfg.overlay.hide_below_m, HIDE_BELOW_MAX);
    }

    #[test]
    fn validate_clamps_inactive_ms() {
        let mut cfg = Config::default();
        cfg.overlay.inactive_ms = 100; // below INACTIVE_MS_MIN (500)
        cfg.validate();
        assert_eq!(cfg.overlay.inactive_ms, INACTIVE_MS_MIN);

        cfg.overlay.inactive_ms = 999_999; // above INACTIVE_MS_MAX (10_000)
        cfg.validate();
        assert_eq!(cfg.overlay.inactive_ms, INACTIVE_MS_MAX);
    }

    #[test]
    fn validate_clamps_window_dimensions() {
        let mut cfg = Config::default();
        cfg.main_window.width = 1;
        cfg.main_window.height = 9999;
        cfg.validate();
        assert_eq!(cfg.main_window.width, WIN_WIDTH_MIN);
        assert_eq!(cfg.main_window.height, WIN_HEIGHT_MAX);
    }

    #[test]
    fn validate_leaves_valid_values_unchanged() {
        let mut cfg = Config::default(); // all defaults are within bounds
        let before = cfg.overlay.opacity;
        cfg.validate();
        assert_eq!(cfg.overlay.opacity, before);
    }

    // ── save / load roundtrip ───────────────────────────────────────────────

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir();
        let path = dir.join("cd_tomtom_test_config_roundtrip.json");
        let _ = fs::remove_file(&path); // clean up any leftover

        let mut cfg = Config::default();
        cfg.overlay.opacity = 0.75;
        cfg.overlay.scale = 1.5;
        cfg.overlay.hide_below_m = 25;
        cfg.overlay.sticky_hide = false;

        cfg.save_to_file(&path).expect("save failed");
        let loaded = Config::load_from_file(&path).expect("load failed");

        assert!((loaded.overlay.opacity - 0.75).abs() < 0.001);
        assert!((loaded.overlay.scale - 1.5).abs() < 0.001);
        assert_eq!(loaded.overlay.hide_below_m, 25);
        assert!(!loaded.overlay.sticky_hide);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_from_nonexistent_file_returns_default() {
        let path = std::env::temp_dir().join("cd_tomtom_this_does_not_exist_xyz.json");
        let _ = fs::remove_file(&path);
        let cfg = Config::load_from_file(&path).expect("should return default, not error");
        // Spot-check a couple of default values
        assert_eq!(cfg.main_window.width, 920);
        assert!((cfg.overlay.opacity - 1.0).abs() < 0.001);
    }

    #[test]
    fn save_overlay_config_merges_without_losing_main_window() {
        let dir = std::env::temp_dir();
        let path = dir.join("cd_tomtom_test_merge.json");
        let _ = fs::remove_file(&path);

        // Write initial full config
        let mut initial = Config::default();
        initial.main_window.width = 1280;
        initial.main_window.height = 720;
        initial.save_to_file(&path).expect("save failed");

        // Merge only the overlay section
        let mut overlay = OverlayConfig::default();
        overlay.opacity = 0.5;
        save_overlay_config_to_file(&path, &overlay).expect("merge save failed");

        let loaded = Config::load_from_file(&path).expect("load failed");
        assert_eq!(loaded.main_window.width, 1280, "main_window.width must survive merge");
        assert!((loaded.overlay.opacity - 0.5).abs() < 0.001);

        let _ = fs::remove_file(&path);
    }

    // ── f32 serialization precision ─────────────────────────────────────────

    #[test]
    fn opacity_serialized_with_two_decimal_places() {
        let mut cfg = Config::default();
        cfg.overlay.opacity = 0.123_456_789; // more precision than 2dp
        let json = serde_json::to_string(&cfg).expect("serialize failed");
        // The value should be rounded to 2dp → 0.12
        assert!(json.contains("\"opacity\":0.12"), "got: {json}");
    }
}
