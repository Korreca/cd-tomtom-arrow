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
use std::collections::HashMap;

/// Represents a single AOB (Array of Bytes) pattern to search for in module memory.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Human-readable name (e.g., "entity", "position", "map")
    pub name: &'static str,
    /// Byte pattern to search for
    pub pattern: &'static [u8],
    /// Offset from pattern match to actual hook address
    pub offset: u64,
}

/// Scanned addresses from all patterns.
#[derive(Debug, Clone)]
pub struct ScanResults {
    pub entity: u64,      // AOB_ENTITY + 3
    pub position: u64,    // AOB_POS + 0
    pub map: u64,         // AOB_MAP + 4
    pub camera: u64,      // AOB_CAM + 0
    pub world_offset: u64, // AOB_WORLD resolved with rel32
}

impl ScanResults {
    /// Verify all scanned addresses are valid (non-zero).
    pub fn is_valid(&self) -> bool {
        self.entity != 0
            && self.position != 0
            && self.map != 0
            && self.camera != 0
            && self.world_offset != 0
    }
}

/// Pattern definitions from CrimsonDesert game engine.
pub struct Scanner;

impl Scanner {
    /// AOB for entity read: stores entity reference
    /// Offset +3 points to the actual hook location
    const ENTITY: Pattern = Pattern {
        name: "entity",
        pattern: b"\x48\x8B\x06\xC5\xF8\x11\x88\xB0\x01\x00\x00",
        offset: 3,
    };

    /// AOB for position write: stores player position
    /// Offset +0 points to the hook location
    const POSITION: Pattern = Pattern {
        name: "position",
        pattern: b"\x0F\x11\x99\x90\x00\x00\x00",
        offset: 0,
    };

    /// AOB for map destination write: stores target marker
    /// Offset +4 points to the hook location
    const MAP: Pattern = Pattern {
        name: "map",
        pattern: b"\xC5\xFB\x10\x07\xC5\xFB\x11\x02\x8B\x47\x08\x89\x42\x08",
        offset: 4,
    };

    /// AOB for camera heading read: stores camera rotation
    /// Offset +0 points to the hook location
    const CAMERA: Pattern = Pattern {
        name: "camera",
        pattern: b"\xC4\xC1\x7A\x11\x97\xA4\x04\x00\x00\xC5\x78\x2F\xCE",
        offset: 0,
    };

    /// AOB for world offset resolution: uses rel32 displacement
    /// The actual address is calculated from a RIP-relative reference
    const WORLD: Pattern = Pattern {
        name: "world",
        pattern: b"\xC5\xF8\x5C\x05",
        offset: 0,
    };

    /// Scan module memory for all patterns.
    ///
    /// # Arguments
    /// * `module_data` - Entire module memory as bytes
    /// * `module_base` - Base address of the module in target process
    ///
    /// # Returns
    /// All resolved hook addresses, or error if any pattern not found
    pub fn scan(module_data: &[u8], module_base: u64) -> AppResult<ScanResults> {
        crate::clog!("[SCANNER] Starting search for 5 patterns in {} bytes", module_data.len());

        crate::clog!("[SCANNER] Searching for ENTITY pattern...");
        let entity = Self::find_pattern(&Self::ENTITY, module_data, module_base)?;
        crate::clog!("[SCANNER] ENTITY found at 0x{:X}", entity);

        crate::clog!("[SCANNER] Searching for POSITION pattern...");
        let position = Self::find_pattern(&Self::POSITION, module_data, module_base)?;
        crate::clog!("[SCANNER] POSITION found at 0x{:X}", position);

        crate::clog!("[SCANNER] Searching for MAP pattern...");
        let map = Self::find_pattern(&Self::MAP, module_data, module_base)?;
        crate::clog!("[SCANNER] MAP found at 0x{:X}", map);

        crate::clog!("[SCANNER] Searching for CAMERA pattern...");
        let camera = Self::find_pattern(&Self::CAMERA, module_data, module_base)?;
        crate::clog!("[SCANNER] CAMERA found at 0x{:X}", camera);

        crate::clog!("[SCANNER] Searching for WORLD pattern (rel32 resolution)...");
        let world_offset = Self::find_world_offset(module_data, module_base)?;
        crate::clog!("[SCANNER] WORLD offset resolved to 0x{:X}", world_offset);

        crate::clog!("[SCANNER] All patterns found successfully!");

        Ok(ScanResults {
            entity,
            position,
            map,
            camera,
            world_offset,
        })
    }

    /// Find a single pattern by name and offset in module memory.
    fn find_pattern(
        pattern: &Pattern,
        module_data: &[u8],
        module_base: u64,
    ) -> AppResult<u64> {
        let idx = module_data
            .windows(pattern.pattern.len())
            .position(|window| window == pattern.pattern)
            .ok_or_else(|| {
                AppError::PatternNotFound(format!(
                    "AOB pattern '{}' not found in module",
                    pattern.name
                ))
            })?;

        Ok(module_base + idx as u64 + pattern.offset)
    }

    /// Find the world offset address using rel32 resolution.
    ///
    /// The AOB_WORLD pattern contains a RIP-relative reference. We must:
    /// 1. Find all occurrences of the pattern
    /// 2. Read the rel32 displacement at offset +4 from each match
    /// 3. Calculate target = base + match_offset + 8 + displacement
    /// 4. Return the most frequently occurring target (heuristic for correctness)
    fn find_world_offset(module_data: &[u8], module_base: u64) -> AppResult<u64> {
        let pattern = &Self::WORLD;
        let mut targets: HashMap<u64, u32> = HashMap::new();
        let mut pos = 0;

        // Find all matches
        while pos < module_data.len().saturating_sub(8) {
            if let Some(idx) = Self::find_in_range(module_data, pattern.pattern, pos) {
                // Read rel32 displacement at offset +4
                let disp_offset = idx + 4;
                if disp_offset + 4 <= module_data.len() {
                    // Read little-endian i32 displacement
                    let disp_bytes = &module_data[disp_offset..disp_offset + 4];
                    let disp = i32::from_le_bytes([
                        disp_bytes[0],
                        disp_bytes[1],
                        disp_bytes[2],
                        disp_bytes[3],
                    ]);

                    // Calculate target address: RIP = base + idx + 8 + disp
                    let target = (module_base as i64 + idx as i64 + 8 + i64::from(disp)) as u64;
                    *targets.entry(target).or_insert(0) += 1;
                }
                pos = idx + 1;
            } else {
                break;
            }
        }

        if targets.is_empty() {
            return Err(AppError::PatternNotFound(
                "AOB pattern 'world' not found or no valid rel32 targets".to_string(),
            ));
        }

        // Return the most frequently occurring target
        targets
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(addr, _)| addr)
            .ok_or_else(|| {
                AppError::PatternNotFound("Failed to resolve world offset target".to_string())
            })
    }

    /// Helper: find pattern starting from position pos in data.
    fn find_in_range(data: &[u8], pattern: &[u8], start: usize) -> Option<usize> {
        data[start..]
            .windows(pattern.len())
            .position(|window| window == pattern)
            .map(|idx| idx + start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_find_simple() {
        let data = b"hello world hello";
        let pattern = Pattern {
            name: "test",
            pattern: b"world",
            offset: 0,
        };
        let base = 0x400000;
        let result = Scanner::find_pattern(&pattern, data, base).unwrap();
        assert_eq!(result, base + 6);
    }

    #[test]
    fn test_pattern_find_with_offset() {
        let data = b"XXXABC";
        let pattern = Pattern {
            name: "test",
            pattern: b"ABC",
            offset: 2,
        };
        let base = 0x400000;
        let result = Scanner::find_pattern(&pattern, data, base).unwrap();
        assert_eq!(result, base + 5); // pos 3 + offset 2
    }

    #[test]
    fn test_pattern_not_found() {
        let data = b"hello world";
        let pattern = Pattern {
            name: "test",
            pattern: b"xyz",
            offset: 0,
        };
        let base = 0x400000;
        let result = Scanner::find_pattern(&pattern, data, base);
        assert!(result.is_err());
    }

    #[test]
    fn test_world_offset_resolution() {
        // Create synthetic module data with world pattern
        let mut data = vec![0u8; 100];
        // Place pattern at offset 10
        let pattern_offset = 10;
        data[pattern_offset..pattern_offset + 4].copy_from_slice(b"\xC5\xF8\x5C\x05");
        // Place rel32 at offset 14 (pattern offset + 4), value 0x1000 (LE)
        let rel32: i32 = 0x1000;
        let rel32_bytes = rel32.to_le_bytes();
        data[pattern_offset + 4..pattern_offset + 8].copy_from_slice(&rel32_bytes);

        let base = 0x400000;
        let result = Scanner::find_world_offset(&data, base).unwrap();
        // target = base + pattern_offset + 8 + rel32 = 0x400000 + 10 + 8 + 0x1000 = 0x401012
        assert_eq!(result, 0x401012);
    }
}
