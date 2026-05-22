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

//! Safe wrappers for remote process memory operations.

use crate::error::{AppError, AppResult};
use core::ffi::c_void;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, VirtualAllocEx, VirtualFreeEx,
};

const REL32_RANGE: u64 = 0x7FFF_FFFF;
const MIN_USER_ADDRESS: u64 = 0x10000;

/// Encapsulates remote memory read/write/alloc/free operations for a process.
#[derive(Clone)]
pub struct RemoteMemory {
    process_handle: HANDLE,
}

impl RemoteMemory {
    /// Create a new RemoteMemory wrapper.
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            process_handle: HANDLE(handle),
        }
    }

    /// Read a buffer of bytes from remote memory.
    pub fn read_bytes(&self, address: u64, size: usize) -> AppResult<Vec<u8>> {
        let mut buffer = vec![0u8; size];
        let mut bytes_read: usize = 0;

        unsafe {
            ReadProcessMemory(
                self.process_handle,
                address as *const c_void,
                buffer.as_mut_ptr().cast::<c_void>(),
                size,
                Some(&raw mut bytes_read),
            )
            .map_err(|e| {
                AppError::ReadMemoryFailed(format!(
                    "ReadProcessMemory failed at 0x{address:X} (size {size}): {e}"
                ))
            })?;
        }

        if bytes_read != size {
            return Err(AppError::ReadMemoryFailed(format!(
                "ReadProcessMemory returned {bytes_read} bytes, expected {size}"
            )));
        }

        Ok(buffer)
    }

    /// Write a buffer of bytes to remote memory.
    pub fn write_bytes(&self, address: u64, data: &[u8]) -> AppResult<()> {
        let mut bytes_written: usize = 0;

        unsafe {
            WriteProcessMemory(
                self.process_handle,
                address as *mut c_void,
                data.as_ptr().cast::<c_void>(),
                data.len(),
                Some(&raw mut bytes_written),
            )
            .map_err(|e| {
                AppError::WriteMemoryFailed(format!(
                    "WriteProcessMemory failed at 0x{:X} (size {}): {}",
                    address,
                    data.len(),
                    e
                ))
            })?;
        }

        if bytes_written != data.len() {
            return Err(AppError::WriteMemoryFailed(format!(
                "WriteProcessMemory returned {} bytes, expected {}",
                bytes_written,
                data.len()
            )));
        }

        Ok(())
    }

    /// Read a single f32 from remote memory.
    pub fn read_float(&self, address: u64) -> AppResult<f32> {
        let bytes = self.read_bytes(address, 4)?;
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes);
        Ok(f32::from_le_bytes(array))
    }

    /// Read a single u32 from remote memory.
    pub fn read_u32(&self, address: u64) -> AppResult<u32> {
        let bytes = self.read_bytes(address, 4)?;
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes);
        Ok(u32::from_le_bytes(array))
    }

    /// Read a single u64 from remote memory.
    pub fn read_u64(&self, address: u64) -> AppResult<u64> {
        let bytes = self.read_bytes(address, 8)?;
        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes);
        Ok(u64::from_le_bytes(array))
    }

    /// Write a single f32 to remote memory.
    pub fn write_float(&self, address: u64, value: f32) -> AppResult<()> {
        self.write_bytes(address, &value.to_le_bytes())
    }

    /// Write a single u32 to remote memory.
    pub fn write_u32(&self, address: u64, value: u32) -> AppResult<()> {
        self.write_bytes(address, &value.to_le_bytes())
    }

    /// Write a single u64 to remote memory.
    pub fn write_u64(&self, address: u64, value: u64) -> AppResult<()> {
        self.write_bytes(address, &value.to_le_bytes())
    }

    /// Allocate memory in the remote process.
    /// If `near_address` is Some, try to allocate near that address (for rel32 jumps).
    /// Otherwise, allocate anywhere.
    pub fn allocate(&self, size: usize, near_address: Option<u64>) -> AppResult<u64> {
        crate::clog!(
            "[ALLOC] Allocating {} bytes (near: {:?})",
            size,
            near_address.map(|a| format!("0x{a:X}"))
        );
        if let Some(near) = near_address {
            // Try to allocate near the given address for rel32 jumps (±2GB range)
            // Same as Python: iterate with 64KB steps, same as alloc_near()
            const STEP: u64 = 0x10000; // 64KB steps (matches Python)

            // Try offsets from 64KB up to ~2GB
            for offset in (STEP..REL32_RANGE).step_by(STEP as usize) {
                for addr in [near.saturating_add(offset), near.saturating_sub(offset)] {
                    if addr < MIN_USER_ADDRESS {
                        continue;
                    }
                    // Try to allocate at this address (Windows may allocate elsewhere)
                    unsafe {
                        let ptr = VirtualAllocEx(
                            self.process_handle,
                            Some(addr as *const c_void),
                            size,
                            MEM_COMMIT | MEM_RESERVE,
                            PAGE_EXECUTE_READWRITE,
                        );

                        if !ptr.is_null() {
                            let allocated_addr = ptr as u64;
                            // Check if allocated address is within rel32 range of original hook
                            let dist = (allocated_addr as i64 - near as i64).unsigned_abs();
                            if dist < REL32_RANGE {
                                crate::clog!(
                                    "[ALLOC] Success: allocated {} bytes at 0x{:X}",
                                    size,
                                    allocated_addr
                                );
                                return Ok(allocated_addr);
                            }
                            // Too far away, free and continue searching
                            let _ = VirtualFreeEx(self.process_handle, ptr, 0, MEM_RELEASE);
                        }
                    }
                }
            }
        }

        // Fall back to unrestricted allocation
        let result = self.allocate_at(0, size)?;
        crate::clog!(
            "[ALLOC] Fallback allocation: {} bytes at 0x{:X}",
            size,
            result
        );
        Ok(result)
    }

    /// Allocate memory at a specific address (or 0 for any address).
    fn allocate_at(&self, address: u64, size: usize) -> AppResult<u64> {
        unsafe {
            let ptr = VirtualAllocEx(
                self.process_handle,
                if address == 0 {
                    None
                } else {
                    Some(address as *const c_void)
                },
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            );

            if ptr.is_null() {
                return Err(AppError::AllocFailed(format!(
                    "VirtualAllocEx failed at 0x{address:X} (size {size})"
                )));
            }

            Ok(ptr as u64)
        }
    }

    /// Free allocated memory in the remote process.
    pub fn free(&self, address: u64) -> AppResult<()> {
        crate::clog!("[FREE] Freeing memory at 0x{:X}", address);
        unsafe {
            VirtualFreeEx(self.process_handle, address as *mut c_void, 0, MEM_RELEASE).map_err(
                |e| AppError::FreeFailed(format!("VirtualFreeEx failed at 0x{address:X}: {e}")),
            )?;
        }

        crate::clog!("[FREE] Free successful at 0x{:X}", address);
        Ok(())
    }
}
