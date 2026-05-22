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

pub mod cave;
pub mod inject;

use crate::error::{AppError, AppResult};
use crate::process::memory::RemoteMemory;
use crate::scanner::ScanResults;
use cave::CaveGenerator;
use inject::Patcher;
use std::collections::HashMap;

/// Hook configuration for a single hook point.
#[derive(Debug, Clone)]
struct HookConfig {
    /// Name of the hook (A, B, D, CAM)
    name: &'static str,
    /// Address to hook (from scanner)
    address: u64,
    /// Size of original instruction to replace (bytes)
    size: usize,
    /// Offset in block where cave is stored
    cave_offset: u64,
}

/// Hook installation and lifecycle management.
///
/// Manages the full process of:
/// 1. Allocating a shared memory block for data capture
/// 2. Generating code caves for each hook point
/// 3. Patching hook locations with jumps to caves
/// 4. Handling far mode (when allocation is beyond rel32 range)
/// 5. Cleanup and restoration of original bytes
pub struct HookManager {
    /// Remote memory interface
    memory: RemoteMemory,
    /// Base address of allocated code/data block
    block_base: u64,
    /// Whether far mode (trampolines) is needed
    far_mode: bool,
    /// Addresses of allocated trampolines (far mode only)
    trampolines: Vec<u64>,
    /// Original bytes at each hook location for restoration
    original_bytes: HashMap<u64, Vec<u8>>,
    /// Whether hooks are currently installed
    installed: bool,
}

impl HookManager {
    /// Create a new hook manager.
    pub fn new(memory: RemoteMemory) -> Self {
        Self {
            memory,
            block_base: 0,
            far_mode: false,
            trampolines: Vec::new(),
            original_bytes: HashMap::new(),
            installed: false,
        }
    }

    /// Get the block base address (only valid after successful install).
    pub fn block_addr(&self) -> u64 {
        self.block_base
    }

    /// Install all hooks for the scanned addresses.
    ///
    /// # Process
    /// 1. Allocate code/data block (4KB with specific offsets for caves and data)
    /// 2. Initialize data storage (travel data, map data, camera data)
    /// 3. Write code caves to block
    /// 4. Patch hook locations with jumps to caves
    /// 5. Handle far mode if needed (allocate trampolines)
    ///
    /// # Arguments
    /// * `results` - Scan results with hook addresses
    ///
    /// # Returns
    /// Success if all hooks installed, or error with partial state
    pub fn install(&mut self, results: &ScanResults) -> AppResult<()> {
        if self.installed {
            return Err(AppError::HookFailed("Hooks already installed".to_string()));
        }

        // Allocate code/data block
        self.allocate_block(results)?;

        // Initialize data storage areas
        self.init_storage()?;

        // Generate and write code caves
        self.write_caves(results)?;

        // Patch hook locations
        self.patch_hooks(results)?;

        self.installed = true;
        Ok(())
    }

    /// Remove all hooks and restore original bytes.
    pub fn uninstall(&mut self) -> AppResult<()> {
        if !self.installed {
            return Ok(());
        }

        for (addr, orig_bytes) in &self.original_bytes {
            self.memory.write_bytes(*addr, orig_bytes)?;
        }

        self.installed = false;

        // Free trampolines (far-mode only)
        for tramp in self.trampolines.drain(..) {
            let _ = self.memory.free(tramp);
        }

        // Free the code/data block; set to 0 so Drop doesn't double-free.
        if self.block_base != 0 {
            let _ = self.memory.free(self.block_base);
            self.block_base = 0;
        }

        crate::clog!("[HOOKS] Hooks uninstalled successfully");
        Ok(())
    }
}

impl HookManager {
    /// Allocate the code/data block near hook locations for rel32 jump support.
    fn allocate_block(&mut self, results: &ScanResults) -> AppResult<()> {
        let block_size = CaveGenerator::BLOCK_SIZE;
        crate::clog!(
            "[ALLOC] Attempting to allocate {} bytes for hooks",
            block_size
        );

        // Try to allocate near one of the hook locations (for rel32 jump support)
        let anchors = [
            results.entity,
            results.position,
            results.map,
            results.camera,
        ];
        for &anchor in &anchors {
            crate::clog!("[ALLOC] Trying near 0x{:X}...", anchor);
            if let Ok(block) = self.alloc_near(anchor, block_size) {
                crate::clog!("[ALLOC] Success! Allocated at 0x{:X} (rel32 mode)", block);
                self.block_base = block;
                self.far_mode = false;
                return Ok(());
            }
        }

        // Fallback: allocate anywhere (far mode, will need trampolines)
        crate::clog!("[ALLOC] Near allocation failed, falling back to any address (far mode)...");
        let block = self.memory.allocate(block_size as usize, None)?;
        crate::clog!(
            "[ALLOC] Allocated at 0x{:X} (far mode - trampolines needed)",
            block
        );
        self.block_base = block;
        self.far_mode = true;
        Ok(())
    }

    /// Allocate memory near a specific address (within rel32 range: ±2GB).
    /// Only tries a few strategic offsets to avoid hanging on millions of iterations.
    fn alloc_near(&self, near_addr: u64, size: u64) -> AppResult<u64> {
        // Try only ±128MB range with 1MB steps (much faster than ±2GB)
        const STEP: i64 = 0x100000; // 1MB step (was 64KB before, too slow)
        const MAX_OFFSET: i64 = 0x8000000; // ±128MB (was ±2GB, too slow)

        crate::clog!(
            "[ALLOC] Trying offsets near 0x{:X}, ±{}MB...",
            near_addr,
            MAX_OFFSET / 0x100000
        );

        let mut attempt_count = 0;
        // Try offsets in ±128MB range
        for offset in (STEP..MAX_OFFSET).step_by(STEP as usize) {
            attempt_count += 1;
            if attempt_count % 10 == 0 {
                crate::clog!(
                    "[ALLOC]   ...attempt {} (offset 0x{:X})...",
                    attempt_count,
                    offset
                );
            }

            // Try positive offset
            if let Ok(addr) = self
                .memory
                .allocate(size as usize, Some((near_addr as i64 + offset) as u64))
            {
                crate::clog!("[ALLOC] Success at offset +0x{:X}", offset);
                return Ok(addr);
            }

            // Try negative offset
            let neg_offset = (near_addr as i64 - offset) as u64;
            if let Ok(addr) = self.memory.allocate(size as usize, Some(neg_offset)) {
                crate::clog!("[ALLOC] Success at offset -0x{:X}", offset);
                return Ok(addr);
            }
        }

        crate::clog!(
            "[ALLOC] Could not find rel32-range allocation in {}/{} attempts",
            attempt_count,
            (MAX_OFFSET / STEP) * 2
        );
        Err(AppError::AllocFailed(
            "Could not allocate near target address".to_string(),
        ))
    }

    /// Initialize data storage areas to zero.
    fn init_storage(&self) -> AppResult<()> {
        crate::clog!(
            "[HOOKS] Initializing data storage at 0x{:X}...",
            self.block_base
        );

        // Travel data: 64 bytes at offset 0x000
        let zeros_64 = vec![0u8; 64];
        self.memory.write_bytes(self.block_base, &zeros_64)?;

        // Map data: 16 bytes at offset 0x040
        let zeros_16 = vec![0u8; 16];
        self.memory
            .write_bytes(self.block_base + CaveGenerator::OFF_MAP_DATA, &zeros_16)?;

        // Camera data: 4 bytes (float 0.0) at offset 0x090
        self.memory.write_bytes(
            self.block_base + CaveGenerator::OFF_CAM_DATA,
            &(0.0f32).to_le_bytes(),
        )?;

        crate::clog!("[HOOKS] Data storage initialized");
        Ok(())
    }

    /// Write code caves to allocated block.
    fn write_caves(&self, results: &ScanResults) -> AppResult<()> {
        crate::clog!("[HOOKS] Writing code caves...");
        let caves = vec![
            (
                CaveGenerator::OFF_CAVE_ENTITY,
                CaveGenerator::build_cave_entity(
                    self.block_base + CaveGenerator::OFF_TRAVEL_DATA,
                    results.entity + CaveGenerator::HOOK_ENTITY_SIZE as u64,
                ),
                "ENTITY",
            ),
            (
                CaveGenerator::OFF_CAVE_POSITION,
                CaveGenerator::build_cave_position(
                    self.block_base + CaveGenerator::OFF_TRAVEL_DATA,
                    results.position + CaveGenerator::HOOK_POSITION_SIZE as u64,
                ),
                "POSITION",
            ),
            (
                CaveGenerator::OFF_CAVE_MAP,
                CaveGenerator::build_cave_map(
                    self.block_base + CaveGenerator::OFF_MAP_DATA,
                    results.map + CaveGenerator::HOOK_MAP_SIZE as u64,
                ),
                "MAP",
            ),
            (
                CaveGenerator::OFF_CAVE_CAM,
                CaveGenerator::build_cave_cam(
                    self.block_base + CaveGenerator::OFF_CAM_DATA,
                    results.camera + CaveGenerator::HOOK_CAM_SIZE as u64,
                ),
                "CAM",
            ),
        ];

        for (offset, code, label) in caves {
            let addr = self.block_base + offset;
            let end = addr + code.len() as u64;

            if end > self.block_base + CaveGenerator::BLOCK_SIZE {
                return Err(AppError::HookFailed(format!(
                    "Cave exceeds allocated block: end=0x{end:X}"
                )));
            }

            crate::clog!(
                "[HOOKS] Writing cave {} at 0x{:X} ({} bytes)",
                label,
                addr,
                code.len()
            );
            self.memory.write_bytes(addr, &code)?;
        }

        crate::clog!("[HOOKS] All code caves written");
        Ok(())
    }

    /// Patch hook locations with jumps to caves.
    fn patch_hooks(&mut self, results: &ScanResults) -> AppResult<()> {
        crate::clog!("[HOOKS] Patching hook locations...");
        let hooks = vec![
            HookConfig {
                name: "ENTITY",
                address: results.entity,
                size: CaveGenerator::HOOK_ENTITY_SIZE,
                cave_offset: CaveGenerator::OFF_CAVE_ENTITY,
            },
            HookConfig {
                name: "POSITION",
                address: results.position,
                size: CaveGenerator::HOOK_POSITION_SIZE,
                cave_offset: CaveGenerator::OFF_CAVE_POSITION,
            },
            HookConfig {
                name: "MAP",
                address: results.map,
                size: CaveGenerator::HOOK_MAP_SIZE,
                cave_offset: CaveGenerator::OFF_CAVE_MAP,
            },
            HookConfig {
                name: "CAM",
                address: results.camera,
                size: CaveGenerator::HOOK_CAM_SIZE,
                cave_offset: CaveGenerator::OFF_CAVE_CAM,
            },
        ];

        for hook in hooks {
            self.patch_single_hook(&hook)?;
        }

        crate::clog!("[HOOKS] All hooks patched successfully");
        Ok(())
    }

    /// Patch a single hook location.
    fn patch_single_hook(&mut self, hook: &HookConfig) -> AppResult<()> {
        crate::clog!(
            "[HOOKS] Patching hook {} at 0x{:X} ({} bytes)",
            hook.name,
            hook.address,
            hook.size
        );
        let cave_addr = self.block_base + hook.cave_offset;

        // Save original bytes
        let orig = self.memory.read_bytes(hook.address, hook.size)?;
        self.original_bytes.insert(hook.address, orig);

        if self.far_mode {
            // Allocate trampoline for far jump
            crate::clog!(
                "[HOOKS] Using far mode - allocating trampoline for hook {}",
                hook.name
            );
            let tramp_addr = self.memory.allocate(64, Some(hook.address))?;
            self.trampolines.push(tramp_addr);
            crate::clog!("[HOOKS] Trampoline at 0x{:X}", tramp_addr);

            // Write absolute jump in trampoline
            let abs_jmp = Patcher::abs_jmp(cave_addr);
            self.memory.write_bytes(tramp_addr, &abs_jmp)?;

            // Patch hook location with jump to trampoline
            let patch = Patcher::jmp_patch(hook.address, tramp_addr, hook.size)?;
            self.memory.write_bytes(hook.address, &patch)?;
            crate::clog!("[HOOKS] Hook {} patched via trampoline", hook.name);
        } else {
            // Direct rel32 jump
            crate::clog!(
                "[HOOKS] Using direct rel32 jump to cave at 0x{:X}",
                cave_addr
            );
            let patch = Patcher::jmp_patch(hook.address, cave_addr, hook.size)?;
            self.memory.write_bytes(hook.address, &patch)?;
        }

        crate::clog!("[HOOKS] Hook {} patch complete", hook.name);
        Ok(())
    }
}

impl Drop for HookManager {
    fn drop(&mut self) {
        if !self.installed {
            return;
        }
        crate::clog!("[HOOKS] Drop: restoring hook locations...");
        for (addr, orig_bytes) in &self.original_bytes {
            match self.memory.write_bytes(*addr, orig_bytes) {
                Ok(()) => crate::clog!("[HOOKS] Drop: restored 0x{:X}", addr),
                Err(e) => crate::clog!("[HOOKS] Drop: failed to restore 0x{:X}: {}", addr, e),
            }
        }
        for tramp in &self.trampolines {
            let _ = self.memory.free(*tramp);
        }
        if self.block_base != 0 {
            let _ = self.memory.free(self.block_base);
        }
        crate::clog!("[HOOKS] Drop: cleanup complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_config_sizes() {
        assert_eq!(CaveGenerator::HOOK_ENTITY_SIZE, 8);
        assert_eq!(CaveGenerator::HOOK_POSITION_SIZE, 7);
        assert_eq!(CaveGenerator::HOOK_MAP_SIZE, 7);
        assert_eq!(CaveGenerator::HOOK_CAM_SIZE, 9);
    }

    #[test]
    fn test_block_offsets_no_overlap() {
        assert!(CaveGenerator::OFF_TRAVEL_DATA < CaveGenerator::OFF_MAP_DATA);
        assert!(CaveGenerator::OFF_MAP_DATA < CaveGenerator::OFF_CAM_DATA);
        assert!(CaveGenerator::OFF_CAM_DATA < CaveGenerator::OFF_CAVE_ENTITY);
        assert!(CaveGenerator::OFF_CAVE_ENTITY < CaveGenerator::OFF_CAVE_POSITION);
        assert!(CaveGenerator::OFF_CAVE_POSITION < CaveGenerator::OFF_CAVE_MAP);
        assert!(CaveGenerator::OFF_CAVE_MAP < CaveGenerator::OFF_CAVE_CAM);
    }
}
