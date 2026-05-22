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

use crate::error::{AppError, AppResult};

/// Creates and applies memory patches for hook installation.
pub struct Patcher;

impl Patcher {
    /// Calculate a rel32 (RIP-relative 32-bit) displacement between two addresses.
    ///
    /// In x64, rel32 displacement = target - source - 5 (size of rel32 jump instruction)
    ///
    /// # Arguments
    /// * `from_addr` - Source address (where the jump instruction starts)
    /// * `to_addr` - Target address to jump to
    ///
    /// # Returns
    /// The 32-bit signed displacement, or error if out of range (±2GB)
    pub fn rel32(from_addr: u64, to_addr: u64) -> AppResult<i32> {
        let from_i64 = from_addr as i64;
        let to_i64 = to_addr as i64;
        let displacement = to_i64 - from_i64 - 5;

        if !(-0x80000000..=0x7FFFFFFF).contains(&displacement) {
            return Err(AppError::RelJmpOutOfRange {
                from: from_addr,
                to: to_addr,
            });
        }

        Ok(displacement as i32)
    }

    /// Create a rel32 jump patch with NOP padding.
    ///
    /// Format: 0xE9 (jmp rel32) + 4-byte displacement + NOP padding
    ///
    /// # Arguments
    /// * `from_addr` - Address where jump is installed
    /// * `to_addr` - Target address
    /// * `total_len` - Total patch length (must be >= 5 for 0xE9 jmp)
    ///
    /// # Returns
    /// Patch bytes: E9 <rel32_le> <NOPs>
    pub fn jmp_patch(from_addr: u64, to_addr: u64, total_len: usize) -> AppResult<Vec<u8>> {
        if total_len < 5 {
            return Err(AppError::InvalidHookConfig(
                "patch length must be >= 5 bytes".to_string(),
            ));
        }

        let rel32 = Self::rel32(from_addr, to_addr)?;
        let mut patch = Vec::with_capacity(total_len);

        // 0xE9 = jmp rel32
        patch.push(0xE9);
        // 4-byte little-endian displacement
        patch.extend_from_slice(&rel32.to_le_bytes());
        // Pad remaining bytes with NOPs (0x90)
        patch.resize(total_len, 0x90);

        Ok(patch)
    }

    /// Create a 64-bit absolute jump (for far jumps outside rel32 range).
    ///
    /// Format: 0xFF25 <6 zero bytes> + 8-byte target address
    /// This is the x64 indirect absolute jump: jmp [rip+0]
    ///
    /// # Arguments
    /// * `target` - 64-bit absolute target address
    ///
    /// # Returns
    /// 14-byte patch: FF25 00000000 <target_le_u64>
    pub fn abs_jmp(target: u64) -> Vec<u8> {
        let mut patch = Vec::with_capacity(14);
        patch.extend_from_slice(b"\xFF\x25\x00\x00\x00\x00");
        patch.extend_from_slice(&target.to_le_bytes());
        patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rel32_near() {
        // From 0x400000, jump to 0x400010
        let disp = Patcher::rel32(0x400000, 0x400010).unwrap();
        // rel32 = 0x400010 - 0x400000 - 5 = 16 - 5 = 11 (0x0B)
        assert_eq!(disp, 11);
    }

    #[test]
    fn test_rel32_backward() {
        // From 0x400010, jump back to 0x400000
        let disp = Patcher::rel32(0x400010, 0x400000).unwrap();
        // rel32 = 0x400000 - 0x400010 - 5 = -16 - 5 = -21
        assert_eq!(disp, -21);
    }

    #[test]
    fn test_rel32_out_of_range() {
        // Try to jump beyond 2GB
        let from = 0x0;
        let to = 0x100000000u64; // 4GB away
        let result = Patcher::rel32(from, to);
        assert!(result.is_err());
    }

    #[test]
    fn test_jmp_patch_exact() {
        let patch = Patcher::jmp_patch(0x400000, 0x400010, 5).unwrap();
        assert_eq!(patch.len(), 5);
        assert_eq!(patch[0], 0xE9); // jmp opcode
        // Next 4 bytes are rel32 (little-endian)
        let rel32 = i32::from_le_bytes([patch[1], patch[2], patch[3], patch[4]]);
        assert_eq!(rel32, 11);
    }

    #[test]
    fn test_jmp_patch_with_nops() {
        let patch = Patcher::jmp_patch(0x400000, 0x400010, 8).unwrap();
        assert_eq!(patch.len(), 8);
        assert_eq!(patch[0], 0xE9);
        assert_eq!(patch[5], 0x90); // NOP padding
        assert_eq!(patch[6], 0x90);
        assert_eq!(patch[7], 0x90);
    }

    #[test]
    fn test_abs_jmp() {
        let patch = Patcher::abs_jmp(0x123456789ABCDEF0);
        assert_eq!(patch.len(), 14);
        assert_eq!(&patch[0..6], b"\xFF\x25\x00\x00\x00\x00");
        let target = u64::from_le_bytes([
            patch[6], patch[7], patch[8], patch[9], patch[10], patch[11], patch[12], patch[13],
        ]);
        assert_eq!(target, 0x123456789ABCDEF0);
    }

    #[test]
    fn test_jmp_patch_too_small() {
        let result = Patcher::jmp_patch(0x400000, 0x400010, 4);
        assert!(result.is_err());
    }
}
