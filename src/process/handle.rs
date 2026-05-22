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

//! Safe wrapper around a Windows process HANDLE with automatic cleanup.

use core::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

/// Safe HANDLE wrapper that calls CloseHandle on drop.
pub struct ProcessHandle {
    handle: HANDLE,
}

impl ProcessHandle {
    /// Create a new handle wrapper. The handle is assumed to be valid.
    pub fn new(handle: *mut c_void) -> Self {
        Self { handle: HANDLE(handle) }
    }

    /// Get the raw pointer for Win32 calls (compatible with winapi and windows crates).
    pub fn raw(&self) -> *mut c_void {
        self.handle.0
    }

    /// Check if the handle is valid (not NULL or INVALID_HANDLE_VALUE).
    pub fn is_valid(&self) -> bool {
        !self.handle.0.is_null() && self.handle != INVALID_HANDLE_VALUE
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if self.is_valid() {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

// HANDLE is safe to send and share across threads
// (it's just a process identifier that the OS manages)
unsafe impl Send for ProcessHandle {}
unsafe impl Sync for ProcessHandle {}
