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

//! Navigation math: bearing, distance, angle normalization.

/// Normalize a signed angle to the range [-180, 180).
pub fn normalize_signed(degrees: f32) -> f32 {
    let result = (degrees + 180.0) % 360.0;
    if result < 0.0 {
        result + 180.0
    } else {
        result - 180.0
    }
}

/// Compute bearing angle to a marker from an observer using dx, dz coordinates.
/// Exactly like Python: math.degrees(math.atan2(dx, dz))
pub fn bearing_to_marker(dx: f32, dz: f32) -> f32 {
    // dx.atan2(dz) is identical to Python's math.atan2(dx, dz) — same argument order.
    let rad = dx.atan2(dz);
    let deg = rad.to_degrees();
    (deg + 360.0) % 360.0
}

/// Compute 2D distance between two XZ positions.
pub fn distance_2d(x1: f32, z1: f32, x2: f32, z2: f32) -> f32 {
    let dx = x2 - x1;
    let dz = z2 - z1;
    dx.hypot(dz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_signed() {
        assert_eq!(normalize_signed(0.0), 0.0);
        assert_eq!(normalize_signed(180.0), -180.0); // 180 wraps to -180
        assert_eq!(normalize_signed(181.0), -179.0);
        assert_eq!(normalize_signed(360.0), 0.0);
        assert_eq!(normalize_signed(-180.0), -180.0);
        assert_eq!(normalize_signed(-181.0), 179.0);
    }

    #[test]
    fn test_bearing() {
        // North (positive Z): angle should be ~0°
        assert!((bearing_to_marker(0.0, 1.0) - 0.0).abs() < 0.1);
        // East (positive X): angle should be ~90°
        assert!((bearing_to_marker(1.0, 0.0) - 90.0).abs() < 0.1);
        // South (negative Z): angle should be ~180°
        assert!((bearing_to_marker(0.0, -1.0) - 180.0).abs() < 0.1);
        // West (negative X): angle should be ~270°
        assert!((bearing_to_marker(-1.0, 0.0) - 270.0).abs() < 0.1);
    }

    #[test]
    fn test_distance_2d() {
        assert_eq!(distance_2d(0.0, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(distance_2d(0.0, 0.0, 3.0, 4.0), 5.0);
        assert_eq!(distance_2d(1.0, 1.0, 2.0, 2.0), (2.0_f32).sqrt());
    }
}
