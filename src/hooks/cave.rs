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

/// Code cave assembly generation for runtime hooks.
///
/// Each cave captures game state into a shared memory block and returns control.
/// Caves are responsible for:
/// 1. Saving necessary registers
/// 2. Loading the shared data block address
/// 3. Executing the original/modified instruction
/// 4. Writing captured data to the block
/// 5. Restoring registers
/// 6. Jumping back to the hooked instruction's next address
pub struct CaveGenerator;

impl CaveGenerator {
    /// Memory offsets within the shared block (total 0x1000 = 4096 bytes).
    pub const BLOCK_SIZE: u64 = 0x1000;
    pub const OFF_TRAVEL_DATA: u64 = 0x000; // 64 bytes for position data
    pub const OFF_MAP_DATA: u64 = 0x040; // 16 bytes for marker data
    pub const OFF_CAM_DATA: u64 = 0x090; // 4 bytes for camera heading
    pub const OFF_CAVE_ENTITY: u64 = 0x100; // Code cave entity
    pub const OFF_CAVE_POSITION: u64 = 0x180; // Code cave position
    pub const OFF_CAVE_MAP: u64 = 0x200; // Code cave map
    pub const OFF_CAVE_CAM: u64 = 0x280; // Code cave CAM

    /// Sizes of original hooked instructions (bytes to replace with jumps).
    pub const HOOK_ENTITY_SIZE: usize = 8;
    pub const HOOK_POSITION_SIZE: usize = 7;
    pub const HOOK_MAP_SIZE: usize = 7;
    pub const HOOK_CAM_SIZE: usize = 9;

    /// Generate cave entity assembly.
    ///
    /// Captures position data from XMM registers into shared memory.
    /// Original instruction: `movaps xmm0, [rax + 0x190]`
    ///
    /// # Arguments
    /// * `block_addr` - Base address of shared data block
    /// * `return_addr` - Address to jump back to after execution
    ///
    /// # Returns
    /// Assembled cave code (machine bytes)
    pub fn build_cave_entity(block_addr: u64, return_addr: u64) -> Vec<u8> {
        let mut code = Vec::new();

        // Save RCX register
        code.extend_from_slice(b"\x51");

        // Load block address into RCX
        code.extend_from_slice(b"\x48\xB9");
        code.extend_from_slice(&block_addr.to_le_bytes());

        // Store position data (RAX + 0x90 to RCX + 0x18)
        code.extend_from_slice(b"\x48\x89\x41\x18");

        // Load and store XMM register data to block
        // movaps xmm0, [rax + 0x190]
        code.extend_from_slice(b"\xC5\xFA\x10\x80\x90\x00\x00\x00");
        // movaps [rcx + 0x20], xmm0
        code.extend_from_slice(b"\xC5\xFA\x11\x41\x20");

        // movaps xmm0, [rax + 0x194]
        code.extend_from_slice(b"\xC5\xFA\x10\x80\x94\x00\x00\x00");
        // movaps [rcx + 0x24], xmm0
        code.extend_from_slice(b"\xC5\xFA\x11\x41\x24");

        // movaps xmm0, [rax + 0x198]
        code.extend_from_slice(b"\xC5\xFA\x10\x80\x98\x00\x00\x00");
        // movaps [rcx + 0x28], xmm0
        code.extend_from_slice(b"\xC5\xFA\x11\x41\x28");

        // Restore RCX
        code.extend_from_slice(b"\x59");

        // Original hooked instruction (or modified version)
        // movaps [rax + 0x1B0], xmm0
        code.extend_from_slice(b"\xC5\xF8\x11\x88\xB0\x01\x00\x00");

        // Jump back to return address (absolute 64-bit jump)
        code.extend_from_slice(b"\xFF\x25\x00\x00\x00\x00");
        code.extend_from_slice(&return_addr.to_le_bytes());

        code
    }

    /// Generate cave position assembly.
    ///
    /// Captures position data conditionally.
    ///
    /// # Arguments
    /// * `block_addr` - Base address of shared data block
    /// * `return_addr` - Address to jump back to after execution
    ///
    /// # Returns
    /// Assembled cave code
    pub fn build_cave_position(block_addr: u64, return_addr: u64) -> Vec<u8> {
        let mut code = Vec::new();

        // Save RAX
        code.extend_from_slice(b"\x50");

        // Load block address into RAX
        code.extend_from_slice(b"\x48\xB8");
        code.extend_from_slice(&block_addr.to_le_bytes());

        // Check if RAX + 0x10 == 0 (memory offset)
        code.extend_from_slice(b"\x48\x83\x78\x10\x00");

        // Jump if not equal (skip write)
        code.extend_from_slice(b"\x7E\x15");

        // Compare memory at RAX + 0x18 with RCX
        code.extend_from_slice(b"\x48\x3B\x48\x18");

        // Jump if not equal
        code.extend_from_slice(b"\x75\x0F");

        // Restore RAX and return
        code.extend_from_slice(b"\x58");
        code.extend_from_slice(b"\xFF\x25\x00\x00\x00\x00");
        code.extend_from_slice(&return_addr.to_le_bytes());

        // Restore RAX (alternative path)
        code.extend_from_slice(b"\x58");

        // Original hooked instruction
        // movaps [rcx + 0x90], xmm2
        code.extend_from_slice(b"\x0F\x11\x99\x90\x00\x00\x00");

        // Jump back to return address
        code.extend_from_slice(b"\xFF\x25\x00\x00\x00\x00");
        code.extend_from_slice(&return_addr.to_le_bytes());

        code
    }

    /// Generate cave map assembly.
    ///
    /// Captures map/marker destination data.
    ///
    /// # Arguments
    /// * `block_addr` - Base address of shared data block
    /// * `return_addr` - Address to jump back to after execution
    ///
    /// # Returns
    /// Assembled cave code
    pub fn build_cave_map(block_addr: u64, return_addr: u64) -> Vec<u8> {
        let mut code = Vec::new();

        // Original instruction 1: movaps [rdx], xmm0
        code.extend_from_slice(b"\xC5\xFB\x11\x02");

        // Original instruction 2: mov eax, [rdi + 0x8]
        code.extend_from_slice(b"\x8B\x47\x08");

        // Save RCX
        code.extend_from_slice(b"\x51");

        // Load block address into RCX
        code.extend_from_slice(b"\x48\xB9");
        code.extend_from_slice(&block_addr.to_le_bytes());

        // Store XMM0 to RCX (map data)
        code.extend_from_slice(b"\xC5\xFB\x11\x01");

        // Store EAX to RCX + 0x8
        code.extend_from_slice(b"\x89\x41\x08");

        // Set flag at RCX + 0xC = 1
        code.extend_from_slice(b"\xC7\x41\x0C\x01\x00\x00\x00");

        // Restore RCX
        code.extend_from_slice(b"\x59");

        // Jump back
        code.extend_from_slice(b"\xFF\x25\x00\x00\x00\x00");
        code.extend_from_slice(&return_addr.to_le_bytes());

        code
    }

    /// Generate cave CAM assembly.
    ///
    /// Captures camera heading data.
    ///
    /// # Arguments
    /// * `block_addr` - Base address of shared data block
    /// * `return_addr` - Address to jump back to after execution
    ///
    /// # Returns
    /// Assembled cave code
    pub fn build_cave_cam(block_addr: u64, return_addr: u64) -> Vec<u8> {
        let mut code = Vec::new();

        // Save RCX
        code.extend_from_slice(b"\x51");

        // Load block address into RCX
        code.extend_from_slice(b"\x48\xB9");
        code.extend_from_slice(&block_addr.to_le_bytes());

        // Original hooked instruction: movaps [r23 + 0x4A4], xmm18
        code.extend_from_slice(b"\xC4\xC1\x7A\x11\x97\xA4\x04\x00\x00");

        // Store camera heading (float) to block
        code.extend_from_slice(b"\xC5\xFA\x11\x11");

        // Restore RCX
        code.extend_from_slice(b"\x59");

        // Jump back
        code.extend_from_slice(b"\xFF\x25\x00\x00\x00\x00");
        code.extend_from_slice(&return_addr.to_le_bytes());

        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cave_entity_generation() {
        let cave = CaveGenerator::build_cave_entity(0x400000, 0x500000);
        // Just verify it generates some code
        assert!(!cave.is_empty());
        assert!(cave.len() > 50); // Should have substantial code
    }

    #[test]
    fn test_cave_sizes_reasonable() {
        let cave_entity = CaveGenerator::build_cave_entity(0x400000, 0x500000);
        let cave_position = CaveGenerator::build_cave_position(0x400000, 0x500000);
        let cave_map = CaveGenerator::build_cave_map(0x400000, 0x500000);
        let cave_cam = CaveGenerator::build_cave_cam(0x400000, 0x500000);

        // All caves should fit in their allocated regions
        assert!(cave_entity.len() <= 128); // OFF_CAVE_ENTITY to OFF_CAVE_POSITION is 0x80 bytes
        assert!(cave_position.len() <= 128); // OFF_CAVE_POSITION to OFF_CAVE_MAP is 0x80 bytes
        assert!(cave_map.len() <= 128); // OFF_CAVE_MAP to OFF_CAVE_CAM is 0x80 bytes
        assert!(cave_cam.len() <= 256); // OFF_CAVE_CAM to end of block
    }
}
