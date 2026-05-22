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

//! Overlay window for navigation arrow using isometric projection in Win32.
//!
//! This module contains numerous unsafe Win32 API calls required for
//! window creation, message handling, and GDI rendering.
#![allow(unsafe_code)]

use super::renderer::{render_layered_frame, draw_isometric_arrow, InfoOverlay, OVERLAY_SIZE};
use crate::app::AppState;
use crate::config::OverlayConfig;
use std::sync::{Arc, Mutex};
use core::ffi::c_void;
use windows::core::PCSTR;
use windows::Win32::Foundation::{HWND, RECT, COLORREF, HINSTANCE, WPARAM, LPARAM, LRESULT};
use windows::Win32::Graphics::Gdi::{
    HBRUSH, HGDIOBJ, GRAPHICS_MODE,
    BeginPaint, EndPaint, FillRect, PAINTSTRUCT,
    CreateCompatibleDC, CreateCompatibleBitmap, SelectObject, CreateSolidBrush,
    SetGraphicsMode, BitBlt, DeleteObject, DeleteDC, SRCCOPY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    WNDCLASSA, CREATESTRUCTA, HICON, HCURSOR,
    WINDOW_LONG_PTR_INDEX,
    RegisterClassA, CreateWindowExA, SetTimer, ShowWindow, DestroyWindow,
    GetClientRect, SetWindowLongPtrA, GetWindowLongPtrA,
    GetWindowRect, SetWindowPos, PostQuitMessage, DefWindowProcA,
    FindWindowA, GetWindowThreadProcessId, PostMessageA,
    CS_HREDRAW, CS_VREDRAW,
    WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
    WS_POPUP,
    WM_CREATE, WM_PAINT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_DESTROY, WM_TIMER, WM_CLOSE,
    SW_SHOW, SW_HIDE,
    GWL_EXSTYLE, SWP_NOSIZE, SWP_NOZORDER,
    HWND_TOP,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use std::mem;
use std::ptr;
use std::ffi::CString;
use std::time::Instant;

/// Overlay window for displaying navigation arrow.
pub struct OverlayWindow {
    hwnd: HWND,
    app_state: Option<Arc<Mutex<AppState>>>,
    turn_deg: f32,
    distance: f32,
    height_diff: f32,
    visible: bool,
    config: Arc<Mutex<OverlayConfig>>,
    last_activity: Instant,
    is_hidden: bool,
    sticky_hidden: bool,
    /// Last known marker position for detecting marker changes (sticky-hide reset)
    last_marker: Option<(f32, f32, f32)>,
    /// Previous (turn_deg, distance, height_diff) for inactivity detection
    last_nav_signature: (f32, f32, f32),
    drag_start_x: i32,
    drag_start_y: i32,
    is_dragging: bool,
    /// Last values pushed to UpdateLayeredWindow — used for dirty-check to skip identical frames
    last_rendered_turn:        f32,
    last_rendered_dist:        f32,
    last_rendered_height:      f32,
    last_rendered_draw:        bool,
    last_rendered_opacity:     f32,
    last_rendered_scale:       f32,
    last_rendered_text_scale:  f32,
    last_rendered_offset_x:    i32,
    last_rendered_offset_y:    i32,
    /// Info panel dirty-check fields
    last_rendered_info_hidden:   bool,
    last_rendered_info_offset_x: i32,
    last_rendered_info_offset_y: i32,
    last_rendered_info_scale:    f32,
    /// Cached locked state — used to update WS_EX_TRANSPARENT only on change
    last_locked:               Option<bool>,
}

const TIMER_MS: u32 = 16;

impl OverlayWindow {
    /// Create a new overlay window.
    pub fn new(app_state: Arc<Mutex<AppState>>, config: Arc<Mutex<OverlayConfig>>) -> Self {
        Self {
            hwnd: HWND(ptr::null_mut()),
            app_state: Some(app_state),
            turn_deg: 0.0,
            distance: 0.0,
            height_diff: 0.0,
            visible: false,
            config,
            last_activity: Instant::now(),
            is_hidden: false,
            sticky_hidden: false,
            last_marker: None,
            last_nav_signature: (0.0, 0.0, 0.0),
            drag_start_x: 0,
            drag_start_y: 0,
            is_dragging: false,
            last_rendered_turn:        f32::NAN,
            last_rendered_dist:        f32::NAN,
            last_rendered_height:      f32::NAN,
            last_rendered_draw:        false,
            last_rendered_opacity:     f32::NAN,
            last_rendered_scale:       f32::NAN,
            last_rendered_text_scale:  f32::NAN,
            last_rendered_offset_x:    i32::MIN,
            last_rendered_offset_y:    i32::MIN,
            last_rendered_info_hidden:   true,
            last_rendered_info_offset_x: i32::MIN,
            last_rendered_info_offset_y: i32::MIN,
            last_rendered_info_scale:    f32::NAN,
            last_locked:               None,
        }
    }

    /// Initialize the overlay window.
    pub fn initialize(&mut self) -> Result<(), String> {
        unsafe {
            let class_name = CString::new("CrimsonDesertArrowOverlay").map_err(|_| "Class name error")?;

            // Destroy any zombie window from a previous run (cross-process FindWindowA)
            if let Ok(existing_hwnd) = FindWindowA(PCSTR(class_name.as_ptr().cast::<u8>()), PCSTR::null()) {
                // Check if it belongs to our process; if not, we can't reuse it safely
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(existing_hwnd, Some(&raw mut pid));
                if pid != GetCurrentProcessId() {
                    // Foreign process window - post close but don't reuse it
                    let _ = PostMessageA(Some(existing_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                // Either way, fall through and create a fresh window for this process
            }

            let wnd_class = WNDCLASSA {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(overlay_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: std::mem::size_of::<*mut Self>() as i32,
                hInstance: HINSTANCE(ptr::null_mut()),
                hIcon: HICON(ptr::null_mut()),
                hCursor: HCURSOR(ptr::null_mut()),
                hbrBackground: HBRUSH(ptr::null_mut()),
                lpszMenuName: PCSTR::null(),
                lpszClassName: PCSTR(class_name.as_ptr().cast::<u8>()),
            };

            RegisterClassA(&raw const wnd_class);

            // Extract config values before passing self
            let (x, y, _opacity) = {
                let cfg = self.config.lock().unwrap();
                (cfg.x, cfg.y, cfg.opacity)
            };

            let hwnd = CreateWindowExA(
                // WS_EX_LAYERED for UpdateLayeredWindow (per-pixel alpha).
                // WS_EX_NOACTIVATE prevents stealing focus.
                // Do NOT call SetLayeredWindowAttributes — use UpdateLayeredWindow instead.
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PCSTR(class_name.as_ptr().cast::<u8>()),
                PCSTR(b"Arrow\0".as_ptr()),
                WS_POPUP,
                x,
                y,
                OVERLAY_SIZE,
                OVERLAY_SIZE,
                None,
                None,
                None,
                Some(std::ptr::from_mut::<Self>(self) as *const c_void),
            ).map_err(|_| "Failed to create overlay window".to_string())?;

            self.hwnd = hwnd;

            // Set 60 FPS timer (16.67ms ≈ 16ms)
            SetTimer(Some(hwnd), 1, TIMER_MS, None);

            let _ = ShowWindow(hwnd, SW_SHOW);
            // Initial frame via UpdateLayeredWindow
            render_layered_frame(hwnd, 1.0, 1.0, 1.0, 0.0, false, 0.0, 0.0, 0, 0, None);

            Ok(())
        }
    }

}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            // Store the OverlayWindow pointer in window extra data
            let create_struct = lparam.0 as *const CREATESTRUCTA;
            if !create_struct.is_null() {
                let overlay_ptr = (*create_struct).lpCreateParams.cast::<OverlayWindow>();
                SetWindowLongPtrA(hwnd, WINDOW_LONG_PTR_INDEX(0), overlay_ptr as isize);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { mem::zeroed() };
            let hdc = unsafe { BeginPaint(hwnd, &raw mut ps) };

            if !hdc.0.is_null() {
                // Use full client rect for drawing, NOT ps.rcPaint.
                // On layered/transparent windows rcPaint can be empty (0x0),
                // which would produce a null bitmap and draw nothing.
                let mut client_rect: RECT = unsafe { mem::zeroed() };
                unsafe { let _ = GetClientRect(hwnd, &raw mut client_rect); }
                let width = client_rect.right;
                let height = client_rect.bottom;

                // Double buffering: create memory DC
                let hdc_mem = unsafe { CreateCompatibleDC(Some(hdc)) };
                let hbitmap = unsafe { CreateCompatibleBitmap(hdc, width, height) };
                let hbitmap_old = unsafe { SelectObject(hdc_mem, HGDIOBJ(hbitmap.0)) };

                // Fill with magenta key color — these pixels become transparent via LWA_COLORKEY.
                // COLORREF is 0x00BBGGRR: magenta (R=255,G=0,B=255) = 0x00FF00FF
                let hbrush = unsafe { CreateSolidBrush(COLORREF(0x00FF00FF)) };
                let rect_mem = RECT { left: 0, top: 0, right: width, bottom: height };
                unsafe { FillRect(hdc_mem, &raw const rect_mem, hbrush); }
                unsafe { let _ = DeleteObject(HGDIOBJ(hbrush.0)); }
                
                // Enable advanced graphics mode for better rendering quality
                unsafe { SetGraphicsMode(hdc_mem, GRAPHICS_MODE(2)); } // GM_ADVANCED = 2

                // Get overlay window state
                let overlay_ptr = unsafe { GetWindowLongPtrA(hwnd, WINDOW_LONG_PTR_INDEX(0)) } as *mut OverlayWindow;
                let turn_deg = if overlay_ptr.is_null() {
                    0.0
                } else {
                    unsafe { &*overlay_ptr }.turn_deg
                };

                // Draw isometric arrow to memory buffer
                let (scale, text_scale) = if overlay_ptr.is_null() { (1.0, 1.0) } else {
                    unsafe { &*overlay_ptr }.config.lock()
                        .map_or((1.0, 1.0), |c| (c.scale, c.text_scale))
                };
                let (distance, height_diff, text_offset_x, text_offset_y) = if overlay_ptr.is_null() { (0.0, 0.0, 0, 0) } else {
                    let ov = unsafe { &*overlay_ptr };
                    let (tx, ty) = ov.config.lock().map_or((0, 0), |c| (c.text_offset_x, c.text_offset_y));
                    (ov.distance, ov.height_diff, tx, ty)
                };
                draw_isometric_arrow(hdc_mem, width, height, turn_deg, scale, text_scale, distance, height_diff, text_offset_x, text_offset_y, None);

                // Copy memory buffer to screen at once (eliminates flicker)
                unsafe {
                    let _ = BitBlt(hdc, 0, 0, width, height, Some(hdc_mem), 0, 0, SRCCOPY);
                }

                // Cleanup
                unsafe {
                    SelectObject(hdc_mem, hbitmap_old);
                    let _ = DeleteObject(HGDIOBJ(hbitmap.0));
                    let _ = DeleteDC(hdc_mem);
                }
            }

            unsafe { let _ = EndPaint(hwnd, &raw const ps); }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 as u16) as i16 as i32; // GET_X_LPARAM: sign-extend
            let y = ((lparam.0 >> 16) as u16) as i16 as i32; // GET_Y_LPARAM: sign-extend
            
            // Store drag start position
            let overlay_ptr = unsafe { GetWindowLongPtrA(hwnd, WINDOW_LONG_PTR_INDEX(0)) } as *mut OverlayWindow;
            if !overlay_ptr.is_null() {
                let overlay = unsafe { &mut *overlay_ptr };
                let cfg = overlay.config.lock().unwrap();
                if !cfg.locked {
                    overlay.is_dragging = true;
                    overlay.drag_start_x = x;
                    overlay.drag_start_y = y;
                    unsafe { SetCapture(hwnd); }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let overlay_ptr = unsafe { GetWindowLongPtrA(hwnd, WINDOW_LONG_PTR_INDEX(0)) } as *mut OverlayWindow;
            if !overlay_ptr.is_null() {
                let overlay = unsafe { &mut *overlay_ptr };
                if overlay.is_dragging {
                    overlay.is_dragging = false;
                    unsafe { let _ = ReleaseCapture(); }

                    // Get new window position
                    let mut rect: RECT = unsafe { mem::zeroed() };
                    unsafe { let _ = GetWindowRect(hwnd, &raw mut rect); }
                    let new_x = rect.left;
                    let new_y = rect.top;

                    // Update overlay config
                    {
                        let mut cfg = overlay.config.lock().unwrap();
                        cfg.x = new_x;
                        cfg.y = new_y;
                    }

                    // Persist position via AppState
                    if let Some(ref app_state_arc) = overlay.app_state
                        && let Ok(mut state) = app_state_arc.try_lock() {
                            state.config.config_mut().overlay.x = new_x;
                            state.config.config_mut().overlay.y = new_y;
                            let _ = state.config.save();
                        }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 as u16) as i16 as i32; // GET_X_LPARAM: sign-extend
            let y = ((lparam.0 >> 16) as u16) as i16 as i32; // GET_Y_LPARAM: sign-extend
            
            let overlay_ptr = unsafe { GetWindowLongPtrA(hwnd, WINDOW_LONG_PTR_INDEX(0)) } as *mut OverlayWindow;
            if !overlay_ptr.is_null() {
                let overlay = unsafe { &*overlay_ptr };
                if overlay.is_dragging {
                    let mut rect: RECT = unsafe { mem::zeroed() };
                    unsafe { let _ = GetWindowRect(hwnd, &raw mut rect); }
                    
                    let dx = x - overlay.drag_start_x;
                    let dy = y - overlay.drag_start_y;
                    
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOP),
                            rect.left + dx,
                            rect.top + dy,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER,
                        );
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0); }
            LRESULT(0)
        }
        WM_TIMER => {
            // 60 FPS loop — read memory every frame (same as Python's poll() at 16 ms).
            // Rendering is skipped when display values have not changed, which keeps
            // CPU near zero while the player is standing still.
            let overlay_ptr = unsafe { GetWindowLongPtrA(hwnd, WINDOW_LONG_PTR_INDEX(0)) } as *mut OverlayWindow;
            if !overlay_ptr.is_null() {
                let overlay = unsafe { &mut *overlay_ptr };

                // ── Read game memory (every frame, ~60 Hz) ──────────────────────────
                if let Some(ref app_state_arc) = overlay.app_state
                    && let Ok(mut app_state) = app_state_arc.lock() {
                        let _ = app_state.tick();
                        let display = app_state.state.get_overlay_display();

                        overlay.turn_deg    = display.turn_angle;
                        overlay.distance    = display.distance;
                        overlay.height_diff = display.height_diff;
                        overlay.visible     = display.visible;

                        // Detect marker changes for sticky-hide reset
                        let nav = app_state.state.get_navigation();
                        let current_marker = nav.marker_dest.map(|m| (m.0, m.1, m.2));
                        let user_reset = app_state.marker_reset_requested;
                        if user_reset {
                            app_state.marker_reset_requested = false;
                        }
                        let marker_changed = match (overlay.last_marker, current_marker) {
                            (Some(old), Some(new)) => {
                                (old.0 - new.0).abs() > 1.0
                                    || (old.1 - new.1).abs() > 1.0
                                    || (old.2 - new.2).abs() > 1.0
                            }
                            (None, Some(_)) => true,
                            (Some(_), None) => { overlay.last_marker = None; false }
                            _ => false,
                        };
                        if marker_changed || user_reset {
                            overlay.last_marker = current_marker;
                            overlay.sticky_hidden = false;
                            overlay.last_activity = Instant::now();
                            overlay.last_nav_signature = (overlay.turn_deg, overlay.distance, overlay.height_diff);
                        }

                        // Reset inactivity timer on player or camera movement.
                        // turn_deg changes with camera rotation; distance/height_diff
                        // change with player movement. Use a 0.5° / 0.5 m threshold
                        // to filter floating-point noise without missing real movement.
                        if overlay.visible {
                            let sig = (overlay.turn_deg, overlay.distance, overlay.height_diff);
                            let (pt, pd, ph) = overlay.last_nav_signature;
                            if (sig.0 - pt).abs() > 0.5
                                || (sig.1 - pd).abs() > 0.5
                                || (sig.2 - ph).abs() > 0.5
                            {
                                overlay.last_activity = Instant::now();
                                overlay.last_nav_signature = sig;
                            }
                        }
                    }

                // Sync WS_EX_TRANSPARENT with locked state so the window is
                // 100% pass-through (no hover cursor change) when locked.
                {
                    let locked = overlay.config.lock().map_or(true, |c| c.locked);
                    if overlay.last_locked != Some(locked) {
                        overlay.last_locked = Some(locked);
                        unsafe {
                            let ex = GetWindowLongPtrA(hwnd, GWL_EXSTYLE);
                            let new_ex = if locked {
                                ex | WS_EX_TRANSPARENT.0 as isize
                            } else {
                                ex & !(WS_EX_TRANSPARENT.0 as isize)
                            };
                            SetWindowLongPtrA(hwnd, GWL_EXSTYLE, new_ex);
                        }
                    }
                }

                // Evaluate hide/show rules
                {
                    let cfg = overlay.config.lock().unwrap();
                    let distance = overlay.distance;

                    if overlay.visible {
                        // Has nav data: apply distance-hide and inactivity
                        let below_threshold = cfg.hide_below_m > 0
                            && distance < cfg.hide_below_m as f32;
                        if below_threshold {
                            overlay.is_hidden = true;
                            if cfg.sticky_hide {
                                overlay.sticky_hidden = true;
                            }
                        } else if !overlay.sticky_hidden {
                            overlay.is_hidden = false;
                        }

                        if !overlay.is_hidden && cfg.hide_on_inactive && cfg.inactive_ms > 0 {
                            let elapsed_ms = overlay.last_activity.elapsed().as_millis() as u32;
                            if elapsed_ms >= cfg.inactive_ms {
                                overlay.is_hidden = true;
                            }
                        }
                    } else {
                        // No nav data: stay shown (transparent — no arrow drawn)
                        overlay.is_hidden = false;
                    }
                }
            }

            // ── Dirty-check render ───────────────────────────────────────────────────
            // Only rebuild and push the DIBSection when something actually changed.
            // When the player stands still, this skips CreateDIBSection + the 360 k-pixel
            // alpha loop + UpdateLayeredWindow entirely, dropping CPU to near zero.
            let is_hidden = if overlay_ptr.is_null() { true } else {
                unsafe { &*overlay_ptr }.is_hidden
            };

            if !overlay_ptr.is_null() {
                let overlay_ref = unsafe { &mut *(overlay_ptr) };
                let draw_arrow = !is_hidden && overlay_ref.visible;
                let turn_deg   = overlay_ref.turn_deg;
                let distance   = overlay_ref.distance;
                let height_diff = overlay_ref.height_diff;

                let (opacity, scale, text_scale, text_offset_x, text_offset_y, info_hidden, info_offset_x, info_offset_y, info_scale) = if let Ok(cfg) = overlay_ref.config.lock() {
                    (cfg.opacity, cfg.scale, cfg.text_scale, cfg.text_offset_x, cfg.text_offset_y,
                     cfg.info_hidden, cfg.info_offset_x, cfg.info_offset_y, cfg.info_scale)
                } else { (1.0, 1.0, 1.0, 0, 0, true, 0, 0, 1.0) };

                let needs_redraw = draw_arrow != overlay_ref.last_rendered_draw
                    || (turn_deg    - overlay_ref.last_rendered_turn       ).abs() > 0.5
                    || (distance    - overlay_ref.last_rendered_dist       ).abs() > 0.5
                    || (height_diff - overlay_ref.last_rendered_height     ).abs() > 0.5
                    || (opacity     - overlay_ref.last_rendered_opacity    ).abs() > 0.005
                    || (scale       - overlay_ref.last_rendered_scale      ).abs() > 0.005
                    || (text_scale  - overlay_ref.last_rendered_text_scale ).abs() > 0.005
                    || text_offset_x != overlay_ref.last_rendered_offset_x
                    || text_offset_y != overlay_ref.last_rendered_offset_y
                    || info_hidden   != overlay_ref.last_rendered_info_hidden
                    || info_offset_x != overlay_ref.last_rendered_info_offset_x
                    || info_offset_y != overlay_ref.last_rendered_info_offset_y
                    || (info_scale   - overlay_ref.last_rendered_info_scale).abs() > 0.005;

                if needs_redraw {
                    let info_overlay = (!info_hidden).then(|| InfoOverlay {
                        title:       "Coming soon...".to_string(),
                        description: "Marker info coming soon. Once markers are implemented, this panel will display the name and notes of the currently active navigation target.".to_string(),
                        offset_x:    info_offset_x,
                        offset_y:    info_offset_y,
                        scale:       info_scale,
                    });
                    unsafe { render_layered_frame(hwnd, opacity, scale, text_scale, turn_deg, draw_arrow, distance, height_diff, text_offset_x, text_offset_y, info_overlay.as_ref()); }
                    overlay_ref.last_rendered_turn        = turn_deg;
                    overlay_ref.last_rendered_dist        = distance;
                    overlay_ref.last_rendered_height      = height_diff;
                    overlay_ref.last_rendered_draw        = draw_arrow;
                    overlay_ref.last_rendered_opacity     = opacity;
                    overlay_ref.last_rendered_scale       = scale;
                    overlay_ref.last_rendered_text_scale  = text_scale;
                    overlay_ref.last_rendered_offset_x    = text_offset_x;
                    overlay_ref.last_rendered_offset_y    = text_offset_y;
                    overlay_ref.last_rendered_info_hidden   = info_hidden;
                    overlay_ref.last_rendered_info_offset_x = info_offset_x;
                    overlay_ref.last_rendered_info_offset_y = info_offset_y;
                    overlay_ref.last_rendered_info_scale    = info_scale;
                }
            }

            // Show/hide AFTER rendering so the window reveals the freshly-rendered frame.
            unsafe { let _ = ShowWindow(hwnd, if is_hidden { SW_HIDE } else { SW_SHOW }); }
            LRESULT(0)
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}