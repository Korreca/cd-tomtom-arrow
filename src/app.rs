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

use crate::clog;
use crate::config::ConfigStore;
use crate::error::AppResult;
use crate::gui::app_snapshot::AppSnapshot;
use crate::hooks::HookManager;
use crate::navigation::state::RuntimeState;
use crate::process::Process;
use crate::scanner::Scanner;

const MAX_ATTACH_ATTEMPTS: u32 = 5;
const ATTACH_RETRY_TICKS: u32 = 300;
const HEIGHT_DIFF_THRESHOLD: f32 = 0.1;

/// Main application state coordinating all subsystems.
pub struct AppState {
    /// Configuration store
    pub config: ConfigStore,
    /// Runtime state
    pub state: RuntimeState,
    /// Currently attached process (if any)
    pub process: Option<Process>,
    /// Hook manager (if hooks installed)
    pub hooks: Option<HookManager>,
    /// Block address for hooked data (only valid if hooks installed)
    pub block_addr: u64,
    /// World offset address (read directly from game memory, not hooked)
    pub world_offset_addr: u64,
    /// Process attachment retry counter (ticks between attempts)
    pub attach_retries: u32,
    /// Number of failed attach attempts (stops after 5)
    pub attach_failure_count: u32,
    /// Whether the application is running
    pub running: bool,
    /// Manually pasted marker destination (fallback when game has no active marker)
    pub manual_marker: Option<(f32, f32, f32)>,
    /// Set to true when the user explicitly sets a manual marker; cleared by the overlay thread.
    /// Allows sticky-hide to reset even when re-setting the same coordinates.
    pub marker_reset_requested: bool,
    /// Last game marker coordinates — used to detect any change (new or moved marker)
    last_game_marker: Option<(i32, i32, i32)>,
}

impl AppState {
    /// Create a new application state.
    pub fn new(config_path: &str) -> AppResult<Self> {
        let config = ConfigStore::new(config_path);
        let state = RuntimeState::new();

        clog!("Application initialized");

        Ok(Self {
            config,
            state,
            process: None,
            hooks: None,
            block_addr: 0,
            world_offset_addr: 0,
            attach_retries: ATTACH_RETRY_TICKS - 1,
            attach_failure_count: 0,
            running: true,
            manual_marker: None,
            marker_reset_requested: false,
            last_game_marker: None,
        })
    }

    /// Main application loop (16ms tick rate).
    /// Execute a single tick of the application loop.
    /// This is called periodically by the GUI event loop.
    pub fn tick(&mut self) -> AppResult<()> {
        // Check if we should exit
        if !self.running {
            return Ok(());
        }

        // Try to attach if not already attached
        if self.process.is_none() {
            self.attach_retries += 1;

            // Only retry up to MAX_ATTACH_ATTEMPTS times (roughly every 5 seconds = ATTACH_RETRY_TICKS)
            if self.attach_retries >= ATTACH_RETRY_TICKS {
                self.attach_retries = 0;

                // Check if we've exceeded max retry count
                if self.attach_failure_count >= MAX_ATTACH_ATTEMPTS {
                    clog!("[ATTACH] Max retry attempts exceeded. Reconnect manually.");
                    self.running = false;
                    return Ok(());
                }

                clog!(
                    "[ATTACH] Retrying attach... (attempt {} of {})",
                    self.attach_failure_count + 1,
                    MAX_ATTACH_ATTEMPTS
                );
                match self.try_attach() {
                    Ok(()) => {
                        self.attach_failure_count = 0; // Reset on success
                    }
                    Err(e) => {
                        self.attach_failure_count += 1;
                        clog!("Attach failed ({}): {}", self.attach_failure_count, e);

                        // If it's a pattern not found error, stop completely
                        if matches!(e, crate::error::AppError::PatternNotFound(_)) {
                            clog!(
                                "[ATTACH] AOB pattern not found - game executable may have changed. Stopping."
                            );
                            self.running = false;
                            return Ok(());
                        }
                    }
                }
            }
        }

        // If attached, check process health
        let is_alive = self
            .process
            .as_ref()
            .is_some_and(super::process::Process::is_alive);
        let has_process = self.process.is_some();

        if !is_alive && has_process {
            clog!("[PROCESS DEAD] Process died, detaching.");
            self.detach();
        } else if has_process {
            // Update from memory if still attached
            if let Some(process) = &self.process {
                let memory = process.memory();

                // ── Single block read (0x20..0x94 = 116 bytes, 1 kernel crossing) ──────
                // Offsets relative to block_addr:
                //   player_pos  @ 0x20 (12 bytes, 3×f32)
                //   marker      @ 0x40 (16 bytes, 3×f32 + u32)
                //   camera_hdg  @ 0x90 ( 4 bytes, 1×f32)
                const BLOCK_START: u64 = 0x20;
                const BLOCK_SIZE: usize = 0x74; // 0x94 - 0x20

                let (pos, marker, heading) = if self.block_addr != 0 {
                    match memory.read_bytes(self.block_addr + BLOCK_START, BLOCK_SIZE) {
                        Ok(buf) => {
                            let f32_at = |off: usize| -> f32 {
                                f32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
                            };
                            let u32_at = |off: usize| -> u32 {
                                u32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
                            };
                            // player_pos: relative offset 0x00 (block 0x20)
                            let (px, py, pz) = (f32_at(0x00), f32_at(0x04), f32_at(0x08));
                            let pos = if px == 0.0 && py == 0.0 && pz == 0.0 {
                                None
                            } else {
                                Some((px, py, pz))
                            };
                            // marker: relative offset 0x20 (block 0x40)
                            let (mx, my, mz) = (f32_at(0x20), f32_at(0x24), f32_at(0x28));
                            let mflag = u32_at(0x2C);
                            let marker = if mflag == 1 {
                                Some((mx, my, mz, mflag))
                            } else {
                                None
                            };
                            // camera heading: relative offset 0x70 (block 0x90)
                            let heading = Some(f32_at(0x70));
                            (pos, marker, heading)
                        }
                        Err(_) => (None, None, None),
                    }
                } else {
                    (None, None, None)
                };

                // World offset: separate address, single read (16 bytes)
                let world_off = if self.world_offset_addr != 0 {
                    memory
                        .read_bytes(self.world_offset_addr, 16)
                        .ok()
                        .map(|buf| {
                            let f32_at = |o: usize| {
                                f32::from_le_bytes(buf[o..o + 4].try_into().unwrap_or([0; 4]))
                            };
                            (f32_at(0), f32_at(4), f32_at(8))
                        })
                } else {
                    None
                };

                // Calculate absolute position (local + world offsets)
                let abs_pos = if let (Some((px, py, pz)), Some((ox, _oy, oz))) = (pos, world_off) {
                    Some((px + ox, py, pz + oz))
                } else {
                    pos
                };

                // Update navigation state
                let mut nav = self.state.get_navigation();
                nav.player_pos = abs_pos;
                // Last-set wins: if the game sets a NEW or DIFFERENT marker it takes over.
                // Quantize to integers (1-unit grid) so tiny float noise doesn't trigger.
                let current_game_key = marker.map(|(x, y, z, _)| (x as i32, y as i32, z as i32));
                if current_game_key.is_some() && current_game_key != self.last_game_marker {
                    // Game marker changed — clear manual so game wins
                    self.manual_marker = None;
                }
                // If the game explicitly cleared its marker and manual_marker was
                // shadowing the same destination, clear manual too.  This lets the
                // overlay detect the None → Some transition when the player
                // re-selects the same destination (2-click: remove then re-add).
                if current_game_key.is_none()
                    && let (Some((mx, my, mz)), Some((gx, gy, gz))) =
                        (self.manual_marker, self.last_game_marker)
                    && (mx as i32 - gx).abs() <= 1
                    && (my as i32 - gy).abs() <= 1
                    && (mz as i32 - gz).abs() <= 1
                {
                    self.manual_marker = None;
                }
                self.last_game_marker = current_game_key;

                // Last-set wins: manual overrides game until game sets a different marker
                let effective_marker = self
                    .manual_marker
                    .map(|(x, y, z)| (x, y, z, 1u32))
                    .or(marker);
                nav.marker_dest = effective_marker;
                nav.camera_heading = heading;
                self.state.set_navigation(nav);

                // Calculate bearing and turn_angle every tick if we have heading and marker
                let mut display = self.state.get_overlay_display();

                if let (Some((px, py, pz)), Some((mx, my, mz, _flag))) = (abs_pos, effective_marker)
                {
                    let dx = mx - px;
                    let dz = mz - pz;
                    let bearing = crate::navigation::math::bearing_to_marker(dx, dz);
                    let distance = crate::navigation::math::distance_2d(px, pz, mx, mz);

                    // Calculate turn angle relative to camera heading (or 0.0 if no heading)
                    let turn_angle = if let Some(chead) = heading {
                        crate::navigation::math::normalize_signed(bearing - chead)
                    } else {
                        0.0
                    };

                    // Calculate height difference only for markers with non-zero y (my != 0)
                    let height_diff = if my.abs() > HEIGHT_DIFF_THRESHOLD {
                        my - py
                    } else {
                        0.0
                    };

                    // Update display state
                    display.turn_angle = turn_angle;
                    display.distance = distance;
                    display.height_diff = height_diff;
                    display.visible = true;
                } else {
                    // Reset display when no marker
                    display.visible = false;
                    display.turn_angle = 0.0;
                    display.distance = 0.0;
                    display.height_diff = 0.0;
                }

                self.state.set_overlay_display(display);
            }
        }

        Ok(())
    }

    /// Attempt to attach to the game process.
    fn try_attach(&mut self) -> AppResult<()> {
        clog!("[ATTACH] Starting attachment process...");
        let process = Process::attach("CrimsonDesert.exe")?;
        clog!("[ATTACH] Attached to process PID={}", process.process_id());

        // Try to scan and install hooks. If anything fails, detach and propagate error.
        match self.attach_with_scanning(process) {
            Ok(()) => {
                clog!("[ATTACH] Attachment successful");
                Ok(())
            }
            Err(e) => {
                clog!("[ATTACH] Scan/hook failed, detaching: {}", e);
                self.detach(); // Clean up on failure
                Err(e)
            }
        }
    }

    /// Internal helper for attachment scanning. Separate function so we can
    /// catch errors and clean up before returning to try_attach.
    fn attach_with_scanning(&mut self, process: Process) -> AppResult<()> {
        // Scan for hook points
        let module = process.module();
        clog!(
            "[SCAN] Module: base=0x{:X}, size={} bytes",
            module.base_address,
            module.size
        );

        // CRITICAL FIX: Only scan first 100MB (AOB patterns are always in code section at start)
        // This matches Python behavior which also only scanned beginning of module
        let scan_limit = std::cmp::min(module.size as usize, 100 * 1024 * 1024); // 100MB max
        clog!(
            "[SCAN] Only reading first {} MB for patterns (limit from {} bytes total)",
            scan_limit / (1024 * 1024),
            module.size
        );

        // Read in 5MB chunks to avoid massive allocations
        const CHUNK_SIZE: usize = 5 * 1024 * 1024; // 5MB chunks
        clog!("[SCAN] Reading in {} byte chunks...", CHUNK_SIZE);

        let mut chunks: Vec<Vec<u8>> = Vec::new();
        let mut offset: usize = 0;
        let mut chunk_count = 0;

        while offset < scan_limit {
            let to_read = std::cmp::min(CHUNK_SIZE, scan_limit - offset);
            let chunk = process
                .memory()
                .read_bytes(module.base_address + offset as u64, to_read)?;
            clog!("[SCAN] Chunk #{}: {} bytes", chunk_count + 1, chunk.len());
            chunks.push(chunk);
            offset += to_read;
            chunk_count += 1;
        }

        clog!("[SCAN] Concatenating {} chunks...", chunks.len());
        let module_data: Vec<u8> = chunks.into_iter().flatten().collect();
        clog!("[SCAN] Total scanned: {} bytes", module_data.len());

        clog!("[SCAN] Starting pattern scanning...");
        let scan_results = Scanner::scan(&module_data, module.base_address)?;
        drop(module_data); // Explicitly free the allocation immediately
        clog!("[SCAN] AOB patterns scanned successfully");

        // Install hooks
        let mut hook_manager = HookManager::new(process.memory().clone());
        hook_manager.install(&scan_results)?;
        clog!("[HOOKS] Hooks installed successfully");

        // Store block address for memory reads
        self.block_addr = hook_manager.block_addr();
        self.world_offset_addr = scan_results.world_offset;
        clog!(
            "[ATTACH] Block=0x{:X}, WorldOffset=0x{:X}",
            self.block_addr,
            self.world_offset_addr
        );

        self.process = Some(process);
        self.hooks = Some(hook_manager);
        self.attach_retries = 0;

        Ok(())
    }

    /// Detach from the game process and uninstall hooks.
    fn detach(&mut self) {
        clog!("[DETACH] Starting detach procedure...");
        // Uninstall hooks first (critical!)
        if let Some(mut hooks) = self.hooks.take() {
            match hooks.uninstall() {
                Ok(()) => clog!("[DETACH] Hooks uninstalled successfully"),
                Err(e) => clog!("[DETACH] Error uninstalling hooks: {}", e),
            }
            drop(hooks);
        }

        self.process = None;
        self.block_addr = 0;
        self.world_offset_addr = 0;
        clog!("[DETACH] Detached from process");
    }

    /// Attempt manual reconnection (called by UI button).
    /// Resets failure count and resumes running state.
    pub fn reconnect(&mut self) {
        clog!("[RECONNECT] Manual reconnection attempt");
        self.attach_failure_count = 0;
        self.attach_retries = ATTACH_RETRY_TICKS - 1;
        self.running = true;
    }

    /// Signal the application to stop.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Cleanup and shutdown - ALWAYS CALLED on exit or drop.
    pub fn cleanup(&mut self) {
        clog!("[APP] Cleaning up resources");
        self.detach(); // Uninstall hooks and close process
        clog!("[APP] Shutdown complete");
    }

    /// Create a snapshot of current app state for GUI rendering (no locks needed).
    pub fn create_snapshot(&self) -> AppSnapshot {
        let nav = self.state.get_navigation();
        let display = self.state.get_overlay_display();

        let mut snapshot = AppSnapshot {
            process_attached: self.process.is_some(),
            hooks_installed: self.hooks.is_some(),
            ..Default::default()
        };

        // Player position
        if let Some((x, y, z)) = nav.absolute_player_pos() {
            snapshot.player_x = Some(x);
            snapshot.player_y = Some(y);
            snapshot.player_z = Some(z);
        }

        // Camera heading
        snapshot.camera_heading = nav.camera_heading;

        // Marker
        if let Some((x, y, z, _)) = nav.marker_dest {
            snapshot.marker_x = Some(x);
            snapshot.marker_y = Some(y);
            snapshot.marker_z = Some(z);
            snapshot.marker_detected = true;
        }

        // Overlay display
        snapshot.overlay_visible = display.visible;
        snapshot.distance = display.distance;
        snapshot.turn_angle = display.turn_angle;
        snapshot.height_diff = display.height_diff;

        // Debug logs
        snapshot.debug_text = crate::logging::global()
            .map_or_else(String::new, super::logging::DebugLogger::get_recent_text);

        // Running state
        snapshot.running = self.running;

        snapshot
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// SAFETY: AppState contains raw pointers from RemoteMemory, but all access is
/// protected by Mutex in Arc<Mutex<AppState>>. The raw pointers are only used
/// within the bounds of a locked mutex, making thread-safe access safe.
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation() {
        let app = AppState::new("config.json");
        assert!(app.is_ok());
    }

    #[test]
    fn test_app_state_logger() {
        crate::logging::init();
        let _app = AppState::new("config.json").unwrap();
        assert!(crate::logging::global().unwrap().line_count() > 0);
    }

    #[test]
    fn test_app_state_running() {
        let app = AppState::new("config.json").unwrap();
        // Just verify the app starts in running state
        assert!(app.running);
    }
}
