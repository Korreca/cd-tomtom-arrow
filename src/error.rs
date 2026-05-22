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

//! Error types and Result alias for the application.

use std::io;
use thiserror::Error;

/// Main error type for the application.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Process not found: {0}")]
    ProcessNotFound(String),

    #[error("Failed to attach to process: {0}")]
    AttachFailed(String),

    #[error("Failed to read memory: {0}")]
    ReadMemoryFailed(String),

    #[error("Failed to write memory: {0}")]
    WriteMemoryFailed(String),

    #[error("Failed to allocate memory: {0}")]
    AllocFailed(String),

    #[error("AOB pattern not found: {0}")]
    PatternNotFound(String),

    #[error("Hook installation failed: {0}")]
    HookFailed(String),

    #[error("Invalid hook configuration: {0}")]
    InvalidHookConfig(String),

    #[error("Trampoline too far: from 0x{from:X} to 0x{to:X}")]
    RelJmpOutOfRange { from: u64, to: u64 },

    #[error("Failed to free memory: {0}")]
    FreeFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience result type for the application.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    // ── Display messages ────────────────────────────────────────────────────

    #[test]
    fn display_process_not_found() {
        let e = AppError::ProcessNotFound("CD.exe".to_string());
        assert_eq!(e.to_string(), "Process not found: CD.exe");
    }

    #[test]
    fn display_pattern_not_found() {
        let e = AppError::PatternNotFound("entity".to_string());
        assert_eq!(e.to_string(), "AOB pattern not found: entity");
    }

    #[test]
    fn display_rel_jmp_out_of_range_includes_hex_addresses() {
        let e = AppError::RelJmpOutOfRange { from: 0x400000, to: 0x90000000 };
        let msg = e.to_string();
        assert!(msg.contains("400000"), "from address missing: {msg}");
        assert!(msg.contains("90000000"), "to address missing: {msg}");
    }

    #[test]
    fn display_hook_failed() {
        let e = AppError::HookFailed("trampoline write".to_string());
        assert_eq!(e.to_string(), "Hook installation failed: trampoline write");
    }

    // ── From conversions ────────────────────────────────────────────────────

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let app_err = AppError::from(io_err);
        assert!(app_err.to_string().contains("file missing"));
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let app_err = AppError::from(json_err);
        assert!(matches!(app_err, AppError::Json(_)));
    }
}
