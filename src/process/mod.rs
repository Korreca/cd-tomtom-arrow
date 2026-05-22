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

//! Process attachment and module information management.

pub mod handle;
pub mod memory;

use crate::error::{AppError, AppResult};
use handle::ProcessHandle;
pub use memory::RemoteMemory;
use std::mem;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, Module32First, Module32Next,
    TH32CS_SNAPPROCESS, TH32CS_SNAPMODULE, PROCESSENTRY32, MODULEENTRY32,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess,
    PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, PROCESS_VM_WRITE, PROCESS_VM_OPERATION,
};

/// Information about a loaded module in a process.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub base_address: u64,
    pub size: u64,
}

/// A handle to an attached process with memory operations.
pub struct Process {
    handle: ProcessHandle,
    process_id: u32,
    module_info: ModuleInfo,
    memory: RemoteMemory,
}

fn char_array_to_string(arr: &[i8]) -> String {
    let null_pos = arr.iter().position(|&b| b == 0).unwrap_or(arr.len());
    let bytes: Vec<u8> = arr[..null_pos].iter().map(|&b| b as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

impl Process {
    /// Attach to a running process by name (e.g., "CrimsonDesert.exe").
    pub fn attach(process_name: &str) -> AppResult<Self> {
        // Find process ID
        let pid = Self::find_process_id(process_name)?;

        // Open process handle
        let handle = Self::open_process_handle(pid)?;

        // Get module info
        let module_info = Self::get_module_info(&handle, process_name, pid)?;

        // Create memory wrapper
        let memory = RemoteMemory::new(handle.raw());

        Ok(Self {
            handle,
            process_id: pid,
            module_info,
            memory,
        })
    }

    /// Get the process ID.
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    /// Get the module information.
    pub fn module(&self) -> &ModuleInfo {
        &self.module_info
    }

    /// Get the memory operations interface.
    pub fn memory(&self) -> &RemoteMemory {
        &self.memory
    }

    /// Find a process ID by executable name.
    fn find_process_id(process_name: &str) -> AppResult<u32> {
        crate::clog!("[PROCESS] Searching for process: {}", process_name);
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|_| {
                    crate::clog!("[PROCESS] ERROR: Failed to create process snapshot");
                    AppError::ProcessNotFound("Failed to create snapshot".to_string())
                })?;

            let mut entry: PROCESSENTRY32 = mem::zeroed();
            entry.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;

            if Process32First(snapshot, &raw mut entry).is_ok() {
                loop {
                    let exe_name = char_array_to_string(&entry.szExeFile);

                    if exe_name.eq_ignore_ascii_case(process_name) {
                        crate::clog!("[PROCESS] Found {} at PID={}", exe_name, entry.th32ProcessID);
                        let _ = CloseHandle(snapshot);
                        return Ok(entry.th32ProcessID);
                    }

                    if Process32Next(snapshot, &raw mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
            crate::clog!("[PROCESS] ERROR: '{}' not found", process_name);
            Err(AppError::ProcessNotFound(process_name.to_string()))
        }
    }

    /// Open a handle to a process by ID.
    fn open_process_handle(pid: u32) -> AppResult<ProcessHandle> {
        crate::clog!("[PROCESS] Opening handle for PID={}", pid);
        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
                false,
                pid,
            ).map_err(|_| {
                crate::clog!("[PROCESS] ERROR: OpenProcess failed for PID {} - handle is NULL", pid);
                AppError::AttachFailed(format!("OpenProcess failed for PID {pid}"))
            })?;

            crate::clog!("[PROCESS] Handle opened successfully for PID={}", pid);
            Ok(ProcessHandle::new(handle.0))
        }
    }

    /// Get module information by name within a process.
    fn get_module_info(_handle: &ProcessHandle, module_name: &str, pid: u32) -> AppResult<ModuleInfo> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)
                .map_err(|_| {
                    crate::clog!("[PROCESS] ERROR: Failed to create module snapshot for PID={}", pid);
                    AppError::AttachFailed("Failed to snapshot modules".to_string())
                })?;

            let mut entry: MODULEENTRY32 = mem::zeroed();
            entry.dwSize = mem::size_of::<MODULEENTRY32>() as u32;

            if Module32First(snapshot, &raw mut entry).is_ok() {
                loop {
                    let mod_name = char_array_to_string(&entry.szModule);

                    if mod_name.eq_ignore_ascii_case(module_name) {
                        crate::clog!("[PROCESS] Module '{}' at 0x{:X} (size={})", mod_name, entry.modBaseAddr as u64, entry.modBaseSize);
                        let _ = CloseHandle(snapshot);
                        return Ok(ModuleInfo {
                            name: mod_name,
                            base_address: entry.modBaseAddr as u64,
                            size: u64::from(entry.modBaseSize),
                        });
                    }

                    if Module32Next(snapshot, &raw mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
            crate::clog!("[PROCESS] ERROR: Module '{}' not found in PID={}", module_name, pid);
            Err(AppError::AttachFailed(format!(
                "Module '{module_name}' not found in process"
            )))
        }
    }

    /// Check if the process is still alive.
    /// Uses GetExitCodeProcess (fast, single kernel call) instead of
    /// CreateToolhelp32Snapshot which enumerates all processes — very expensive at 60 Hz.
    pub fn is_alive(&self) -> bool {
        const STILL_ACTIVE: u32 = 259; // STATUS_PENDING / STILL_ACTIVE
        let mut exit_code: u32 = 0;
        unsafe {
            GetExitCodeProcess(HANDLE(self.handle.raw()), &raw mut exit_code).is_ok()
                && exit_code == STILL_ACTIVE
        }
    }
}
