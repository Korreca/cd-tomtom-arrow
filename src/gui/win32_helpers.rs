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

//! Win32 and clipboard utilities shared across the GUI layer.

// ─── Clipboard ───────────────────────────────────────────────────────────────

/// CF_UNICODETEXT (13) — standard clipboard format for UTF-16 text.
const CF_UNICODETEXT: u32 = 13;

/// Parse a clipboard string of the form `"x, y, z"` or `"x,y,z"` into (x, y, z) f32 values.
/// Returns `None` if the clipboard is empty, inaccessible, or the format doesn't match.
pub fn parse_xyz_clipboard() -> Option<(f32, f32, f32)> {
    let text = get_clipboard_text()?;
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let x = parts[0].trim().parse::<f32>().ok()?;
    let y = parts[1].trim().parse::<f32>().ok()?;
    let z = parts[2].trim().parse::<f32>().ok()?;
    Some((x, y, z))
}

pub fn set_clipboard_text(text: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    let wide: Vec<u16> = OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        let len = wide.len() * 2;
        if let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, len) {
            let ptr = GlobalLock(hmem).cast::<u16>();
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                let _ = GlobalUnlock(hmem);
            }
            let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hmem.0)));
        }
        let _ = CloseClipboard();
    }
}

pub fn get_clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = match GetClipboardData(CF_UNICODETEXT) {
            Ok(hmem) => {
                let hglobal = HGLOBAL(hmem.0);
                let ptr = GlobalLock(hglobal).cast::<u16>();
                let text = if ptr.is_null() {
                    None
                } else {
                    let mut len = 0usize;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(ptr, len);
                    Some(String::from_utf16_lossy(slice))
                };
                if !ptr.is_null() {
                    let _ = GlobalUnlock(hglobal);
                }
                text
            }
            Err(_) => None,
        };
        let _ = CloseClipboard();
        result
    }
}

// ─── Window helpers ───────────────────────────────────────────────────────────

/// Convert the window's physical position to logical coordinates.
pub fn window_logical_pos(win: &slint::Window) -> (f32, f32) {
    let pos = win.position(); // PhysicalPosition { x: i32, y: i32 }
    let sf = win.scale_factor();
    (pos.x as f32 / sf, pos.y as f32 / sf)
}

/// Minimize the currently focused Win32 window.
pub fn minimize_window() {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, ShowWindow, SW_MINIMIZE,
        };
        let hwnd = GetForegroundWindow();
        if !hwnd.0.is_null() {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
}
