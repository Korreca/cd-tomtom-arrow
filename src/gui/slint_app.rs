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

//! Slint control-panel bridge: creates the UI, wires callbacks, and runs
//! the overlay window in a background thread.
#![allow(clippy::cast_possible_truncation)] // intentional: display values are small and rounded
#![allow(clippy::cast_precision_loss)] // intentional: pixel coordinates fit f32 exactly
#![allow(clippy::cast_possible_wrap)] // intentional: u32 config values fit i32 in practice

slint::include_modules!();

// ─── Map bounds (Crimson Desert world extents, rounded to nearest 5) ─────────
const MAP_X_MIN: f32 = -16395.0;
const MAP_X_MAX: f32 = 2315.0;
const MAP_Y_MIN: f32 = 0.0;
const MAP_Y_MAX: f32 = 3000.0;
const MAP_Z_MIN: f32 = -8535.0;
const MAP_Z_MAX: f32 = 5495.0;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::overlay_window::OverlayWindow;
use super::win32_helpers::{
    minimize_window, parse_xyz_clipboard, set_clipboard_text, window_logical_pos,
};
use crate::app::AppState;
use crate::config::ConfigStore;

pub fn run(config_path: &str) {
    // ── 1. Shared state ───────────────────────────────────────────────────────
    let app_state = match AppState::new(config_path) {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(e) => {
            crate::clog!("[Slint] Failed to init AppState: {e:?}");
            return;
        }
    };

    let (overlay_config, saved_window_pos) = {
        let store = ConfigStore::new(config_path);
        let oc = Arc::new(Mutex::new(store.config().overlay.clone()));
        let pos = store
            .config()
            .main_window
            .x
            .zip(store.config().main_window.y);
        (oc, pos)
    };

    // ── 2. Overlay thread (Win32 message loop) ────────────────────────────────
    {
        let as2 = app_state.clone();
        let oc2 = overlay_config.clone();
        std::thread::spawn(move || {
            let mut ov = OverlayWindow::new(as2, oc2);
            if let Err(e) = ov.initialize() {
                crate::clog!("[Overlay] Init failed: {e}");
                return;
            }
            // Win32 message loop — drives WM_TIMER at 60 Hz (game tick + rendering)
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::{
                    DispatchMessageA, GetMessageA, MSG, TranslateMessage,
                };
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageA(&raw mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&raw const msg);
                    DispatchMessageA(&raw const msg);
                }
            }
        });
    }

    // ── 3. Slint window ───────────────────────────────────────────────────────
    let ui = match ControlPanel::new() {
        Ok(u) => u,
        Err(e) => {
            crate::clog!("[Slint] Window creation failed: {e}");
            return;
        }
    };

    // ── 4. Initialise UI from saved config ────────────────────────────────────
    {
        let cfg = overlay_config.lock().unwrap();
        ui.set_ov_opacity(cfg.opacity);
        ui.set_scale(cfg.scale);
        ui.set_text_scale(cfg.text_scale);
        ui.set_hide_below(cfg.hide_below_m.cast_signed());
        ui.set_inactive_ms(cfg.inactive_ms.cast_signed());
        ui.set_text_offset_x(cfg.text_offset_x);
        ui.set_text_offset_y(cfg.text_offset_y);
        ui.set_sticky_hide(cfg.sticky_hide);
        ui.set_hide_on_inactive(cfg.hide_on_inactive);
        ui.set_locked(cfg.locked);
        ui.set_info_offset_x(cfg.info_offset_x);
        ui.set_info_offset_y(cfg.info_offset_y);
        ui.set_info_scale(cfg.info_scale);
        ui.set_info_hidden(cfg.info_hidden);
        // Formatted labels
        ui.set_ov_opacity_label(format!("{}%", (cfg.opacity * 100.0).round() as i32).into());
        ui.set_scale_label(format!("{:.2}x", cfg.scale).into());
        ui.set_text_scale_label(format!("{:.2}x", cfg.text_scale).into());
        ui.set_info_scale_label(format!("{:.2}x", cfg.info_scale).into());
    }

    // App version string
    ui.set_app_version(concat!("v ", env!("CARGO_PKG_VERSION")).into());

    // Restore main-window position if saved
    if let Some((x, y)) = saved_window_pos {
        ui.window()
            .set_position(slint::WindowPosition::Logical(slint::LogicalPosition::new(
                x as f32, y as f32,
            )));
    }

    // ── 5. Callbacks ─────────────────────────────────────────────────────────

    // Lock / unlock overlay position
    let oc = overlay_config.clone();
    let ui_weak = ui.as_weak();
    ui.on_lock_toggled(move || {
        let new_locked = {
            let mut cfg = oc.lock().unwrap();
            cfg.locked = !cfg.locked;
            cfg.locked
        };
        if let Some(u) = ui_weak.upgrade() {
            u.set_locked(new_locked);
        }
    });

    let oc = overlay_config.clone();
    let ui_weak = ui.as_weak();
    ui.on_ov_opacity_changed(move |v| {
        oc.lock().unwrap().opacity = v;
        if let Some(u) = ui_weak.upgrade() {
            u.set_ov_opacity_label(format!("{}%", (v * 100.0).round() as i32).into());
        }
    });

    let oc = overlay_config.clone();
    let ui_weak = ui.as_weak();
    ui.on_scale_changed(move |v| {
        oc.lock().unwrap().scale = v;
        if let Some(u) = ui_weak.upgrade() {
            u.set_scale_label(format!("{v:.2}x").into());
        }
    });

    let oc = overlay_config.clone();
    let ui_weak = ui.as_weak();
    ui.on_text_scale_changed(move |v| {
        oc.lock().unwrap().text_scale = v;
        if let Some(u) = ui_weak.upgrade() {
            u.set_text_scale_label(format!("{v:.2}x").into());
        }
    });

    let oc = overlay_config.clone();
    ui.on_hide_below_changed(move |v| {
        oc.lock().unwrap().hide_below_m = v.max(0).cast_unsigned();
    });

    let oc = overlay_config.clone();
    ui.on_inactive_ms_changed(move |v| {
        oc.lock().unwrap().inactive_ms = v.max(0).cast_unsigned();
    });

    let oc = overlay_config.clone();
    ui.on_text_offset_x_changed(move |v| {
        oc.lock().unwrap().text_offset_x = v;
    });

    let oc = overlay_config.clone();
    ui.on_text_offset_y_changed(move |v| {
        oc.lock().unwrap().text_offset_y = v;
    });

    let oc = overlay_config.clone();
    ui.on_sticky_hide_changed(move |v| {
        oc.lock().unwrap().sticky_hide = v;
    });

    let oc = overlay_config.clone();
    ui.on_hide_on_inactive_changed(move |v| {
        oc.lock().unwrap().hide_on_inactive = v;
    });

    let oc = overlay_config.clone();
    ui.on_info_offset_x_changed(move |v| {
        oc.lock().unwrap().info_offset_x = v;
    });

    let oc = overlay_config.clone();
    ui.on_info_offset_y_changed(move |v| {
        oc.lock().unwrap().info_offset_y = v;
    });

    let oc = overlay_config.clone();
    let ui_weak = ui.as_weak();
    ui.on_info_scale_changed(move |v| {
        oc.lock().unwrap().info_scale = v;
        if let Some(u) = ui_weak.upgrade() {
            u.set_info_scale_label(format!("{v:.2}x").into());
        }
    });

    let oc = overlay_config.clone();
    ui.on_info_hidden_changed(move |v| {
        oc.lock().unwrap().info_hidden = v;
    });

    let as_rc = app_state.clone();
    ui.on_reconnect_clicked(move || {
        if let Ok(mut state) = as_rc.try_lock() {
            state.reconnect();
        }
    });

    // ── Window management ─────────────────────────────────────────────────────

    // Generation counter: each new snackbar gets a unique id. The dismiss
    // closure only closes the bar if the id still matches — so a slow-firing
    // thread from a previous notification never hides a newer one.
    let snack_gen = Arc::new(AtomicU32::new(0));

    // Copy buttons
    let snack_gen_cp = snack_gen.clone();
    let ui_weak = ui.as_weak();
    ui.on_copy_position(move || {
        if let Some(u) = ui_weak.upgrade() {
            let text = u.get_position_copy_text().to_string();
            if !text.is_empty() {
                set_clipboard_text(&text);
                show_snack(&u, &snack_gen_cp, "Copied!", false);
            } else {
                show_snack(&u, &snack_gen_cp, "No position data", true);
            }
        }
    });

    let snack_gen_cm = snack_gen.clone();
    let ui_weak = ui.as_weak();
    ui.on_copy_marker(move || {
        if let Some(u) = ui_weak.upgrade() {
            let text = u.get_marker_copy_text().to_string();
            if !text.is_empty() {
                set_clipboard_text(&text);
                show_snack(&u, &snack_gen_cm, "Copied!", false);
            } else {
                show_snack(&u, &snack_gen_cm, "No marker data", true);
            }
        }
    });

    let as_paste = app_state.clone();
    let snack_gen_pm = snack_gen.clone();
    let ui_weak = ui.as_weak();
    ui.on_paste_marker(move || {
        let success: Option<()> = (|| {
            let (x, y, z) = parse_xyz_clipboard()?;
            if !(MAP_X_MIN..=MAP_X_MAX).contains(&x)
                || !(MAP_Y_MIN..=MAP_Y_MAX).contains(&y)
                || !(MAP_Z_MIN..=MAP_Z_MAX).contains(&z)
            {
                return None;
            }
            let mut state = as_paste.lock().ok()?;
            state.manual_marker = Some((x, y, z));
            state.marker_reset_requested = true;
            Some(())
        })();

        if let Some(u) = ui_weak.upgrade() {
            if success.is_some() {
                show_snack(&u, &snack_gen_pm, "Pasted!", false);
            } else {
                show_snack(&u, &snack_gen_pm, "Invalid format — use: x, y, z", true);
            }
        }
    });

    // Both save buttons redirect to the Markers tab (tab index 2).
    let ui_weak = ui.as_weak();
    ui.on_save_position({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(u) = ui_weak.upgrade() {
                u.set_active_tab(2);
            }
        }
    });
    ui.on_save_marker(move || {
        if let Some(u) = ui_weak.upgrade() {
            u.set_active_tab(2);
        }
    });

    // ── Input Marker dialog ───────────────────────────────────────────────────

    // Open: reset fields to 0 and open dialog
    let ui_weak = ui.as_weak();
    ui.on_input_marker(move || {
        let Some(u) = ui_weak.upgrade() else { return };
        u.set_dialog_x("0".into());
        u.set_dialog_y("0".into());
        u.set_dialog_z("0".into());
        u.set_dialog_x_error(false);
        u.set_dialog_y_error(false);
        u.set_dialog_z_error(false);
        u.set_input_dialog_open(true);
    });

    // Close
    let ui_weak = ui.as_weak();
    ui.on_input_dialog_close(move || {
        let Some(u) = ui_weak.upgrade() else { return };
        u.set_input_dialog_open(false);
        u.set_dialog_x_error(false);
        u.set_dialog_y_error(false);
        u.set_dialog_z_error(false);
    });

    // Step buttons: parse current text, add ±1, format back
    let ui_weak = ui.as_weak();
    ui.on_input_dialog_x_step(move |step| {
        let Some(u) = ui_weak.upgrade() else { return };
        let v: f64 = u.get_dialog_x().parse().unwrap_or(0.0);
        u.set_dialog_x(format!("{:.1}", v + f64::from(step)).into());
        u.set_dialog_x_error(false);
    });

    let ui_weak = ui.as_weak();
    ui.on_input_dialog_y_step(move |step| {
        let Some(u) = ui_weak.upgrade() else { return };
        let v: f64 = u.get_dialog_y().parse().unwrap_or(0.0);
        u.set_dialog_y(format!("{:.1}", v + f64::from(step)).into());
        u.set_dialog_y_error(false);
    });

    let ui_weak = ui.as_weak();
    ui.on_input_dialog_z_step(move |step| {
        let Some(u) = ui_weak.upgrade() else { return };
        let v: f64 = u.get_dialog_z().parse().unwrap_or(0.0);
        u.set_dialog_z(format!("{:.1}", v + f64::from(step)).into());
        u.set_dialog_z_error(false);
    });

    // Paste: fill all three fields from clipboard "x, y, z"
    let snack_gen_di = snack_gen.clone();
    let ui_weak = ui.as_weak();
    ui.on_input_dialog_paste(move || {
        let Some(u) = ui_weak.upgrade() else { return };
        let success: Option<()> = (|| {
            let (x, y, z) = parse_xyz_clipboard()?;
            u.set_dialog_x(format!("{:.1}", x).into());
            u.set_dialog_y(format!("{:.1}", y).into());
            u.set_dialog_z(format!("{:.1}", z).into());
            u.set_dialog_x_error(false);
            u.set_dialog_y_error(false);
            u.set_dialog_z_error(false);
            Some(())
        })();
        if success.is_none() {
            show_snack(&u, &snack_gen_di, "Invalid format — use: x, y, z", true);
        }
    });

    // Confirm: validate, set marker, close (or highlight invalid fields)
    let as_confirm = app_state.clone();
    let snack_gen_cf = snack_gen.clone();
    let ui_weak = ui.as_weak();
    ui.on_input_dialog_confirm(move || {
        let Some(u) = ui_weak.upgrade() else { return };
        let x = u.get_dialog_x().parse::<f32>();
        let y = u.get_dialog_y().parse::<f32>();
        let z = u.get_dialog_z().parse::<f32>();
        u.set_dialog_x_error(x.is_err());
        u.set_dialog_y_error(y.is_err());
        u.set_dialog_z_error(z.is_err());
        if let (Ok(xv), Ok(yv), Ok(zv)) = (x, y, z) {
            if !(MAP_X_MIN..=MAP_X_MAX).contains(&xv) {
                show_snack(
                    &u,
                    &snack_gen_cf,
                    &format!("X must be in [{}, {}]", MAP_X_MIN as i32, MAP_X_MAX as i32),
                    true,
                );
                return;
            }
            if !(MAP_Y_MIN..=MAP_Y_MAX).contains(&yv) {
                show_snack(
                    &u,
                    &snack_gen_cf,
                    &format!("Y must be in [{}, {}]", MAP_Y_MIN as i32, MAP_Y_MAX as i32),
                    true,
                );
                return;
            }
            if !(MAP_Z_MIN..=MAP_Z_MAX).contains(&zv) {
                show_snack(
                    &u,
                    &snack_gen_cf,
                    &format!("Z must be in [{}, {}]", MAP_Z_MIN as i32, MAP_Z_MAX as i32),
                    true,
                );
                return;
            }
            if let Ok(mut state) = as_confirm.try_lock() {
                state.manual_marker = Some((xv, yv, zv));
                state.marker_reset_requested = true;
            }
            u.set_input_dialog_open(false);
        }
    });

    // Close
    ui.on_open_kofi(|| {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", "https://ko-fi.com/korreca/tip"])
            .spawn()
            .ok();
    });

    ui.on_open_nexus(|| {
        std::process::Command::new("cmd")
            .args([
                "/c",
                "start",
                "",
                "https://www.nexusmods.com/crimsondesert/mods/2189",
            ])
            .spawn()
            .ok();
    });

    let as_close = app_state.clone();
    ui.on_window_close(move || {
        // Restore original game bytes BEFORE the process exits.
        // The overlay thread holds an Arc clone of app_state so AppState::Drop
        // is never reached naturally — we must clean up explicitly here.
        if let Ok(mut state) = as_close.lock() {
            state.running = false;
            state.cleanup();
        }
        // Destroy the overlay window so its Win32 message loop exits cleanly.
        unsafe {
            use std::ffi::CString;
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{FindWindowA, PostMessageA, WM_CLOSE};
            use windows::core::PCSTR;
            let class = CString::new("CrimsonDesertArrowOverlay").unwrap();
            let hwnd = FindWindowA(PCSTR(class.as_ptr().cast::<u8>()), PCSTR::null());
            if let Ok(hwnd) = hwnd {
                let _ = PostMessageA(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        slint::quit_event_loop().unwrap_or_default();
    });

    // Minimize via raw Win32 handle
    ui.on_window_minimize(|| {
        minimize_window();
    });

    // Title-bar drag — use Win32 GetCursorPos for absolute screen coords to avoid
    // the stutter caused by Slint's window-relative mouse-x/y feedback loop.
    // State: (cursor_phys_x, cursor_phys_y, win_logical_x, win_logical_y)
    let drag_init: Arc<Mutex<Option<(i32, i32, f32, f32)>>> = Arc::new(Mutex::new(None));

    let ui_weak = ui.as_weak();
    let di = drag_init.clone();
    ui.on_title_drag_start({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(u) = ui_weak.upgrade() {
                use windows::Win32::Foundation::POINT;
                use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                let mut pt = POINT { x: 0, y: 0 };
                unsafe {
                    let _ = GetCursorPos(&raw mut pt);
                }
                let (wx, wy) = window_logical_pos(u.window());
                *di.lock().unwrap() = Some((pt.x, pt.y, wx, wy));
            }
        }
    });

    let di = drag_init; // last use of drag_init — move instead of clone
    ui.on_title_drag_delta(move |_dx, _dy| {
        if let Some(u) = ui_weak.upgrade() {
            let value = *di.lock().unwrap();
            if let Some((ix, iy, wx, wy)) = value {
                use windows::Win32::Foundation::POINT;
                use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                let mut pt = POINT { x: 0, y: 0 };
                unsafe {
                    let _ = GetCursorPos(&raw mut pt);
                }
                let sf = u.window().scale_factor();
                let dx = (pt.x - ix) as f32 / sf;
                let dy = (pt.y - iy) as f32 / sf;
                u.window().set_position(slint::WindowPosition::Logical(
                    slint::LogicalPosition::new(wx + dx, wy + dy),
                ));
            }
        }
    });

    // ── 6. Refresh timer (200 ms) ─────────────────────────────────────────────
    let oc_timer = overlay_config.clone();
    let timer = slint::Timer::default();
    let ui_weak = ui.as_weak();
    let as_ref = app_state.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let snap = match as_ref.try_lock() {
                Ok(state) => state.create_snapshot(),
                Err(_) => return, // overlay holds lock briefly; skip this tick
            };

            // Status
            let (status_txt, connected) = if snap.hooks_installed {
                ("CONNECTED & READY", true)
            } else if snap.process_attached {
                ("ATTACHED — AWAITING HOOKS", false)
            } else if !snap.running {
                ("DISCONNECTED", false)
            } else {
                ("WAITING FOR GAME", false)
            };
            ui.set_status_text(status_txt.into());
            ui.set_connected(connected);
            ui.set_show_reconnect(!snap.running);

            // Position
            let (pos, pos_copy) = match (snap.player_x, snap.player_y, snap.player_z) {
                (Some(x), Some(y), Some(z)) => (
                    format!("X: {x:.1}  Y: {y:.1}  Z: {z:.1}"),
                    format!("{:.1}, {:.1}, {:.1}", x, y, z),
                ),
                _ => ("Waiting...".into(), String::new()),
            };
            ui.set_position_text(pos.into());
            ui.set_position_copy_text(pos_copy.into());

            // Heading
            let hdg = match snap.camera_heading {
                Some(h) => format!("{h:.1}°"),
                None => "—".into(),
            };
            ui.set_heading_text(hdg.into());

            // Marker
            let (mkr, mkr_copy, detected) = if snap.marker_detected {
                match (snap.marker_x, snap.marker_y, snap.marker_z) {
                    (Some(x), Some(y), Some(z)) => (
                        format!("X: {x:.1}  Y: {y:.1}  Z: {z:.1}"),
                        format!("{:.1}, {:.1}, {:.1}", x, y, z),
                        true,
                    ),
                    _ => ("Detected".into(), String::new(), true),
                }
            } else {
                ("No marker detected".into(), String::new(), false)
            };
            ui.set_marker_text(mkr.into());
            ui.set_marker_copy_text(mkr_copy.into());
            ui.set_marker_detected(detected);

            // Distance, bearing & height diff
            ui.set_distance_text(format!("{:.1} m", snap.distance).into());
            ui.set_bearing_text(format!("{:.1}°", snap.turn_angle).into());
            let marker_y_zero = snap.marker_y.is_none_or(|y| y == 0.0);
            let height_txt = if !snap.marker_detected || marker_y_zero {
                "--".to_string()
            } else if snap.height_diff > 0.5 {
                format!("▲ {:.1} m", snap.height_diff)
            } else if snap.height_diff < -0.5 {
                format!("▼ {:.1} m", snap.height_diff.abs())
            } else {
                format!("{:.1} m", snap.height_diff)
            };
            ui.set_height_diff_text(height_txt.into());

            // Keep formatted labels in sync with current config (read from
            // overlay_config so stale UI property values don't reset the labels)
            if let Ok(cfg) = oc_timer.try_lock() {
                ui.set_ov_opacity_label(
                    format!("{}%", (cfg.opacity * 100.0).round() as i32).into(),
                );
                ui.set_scale_label(format!("{:.2}x", cfg.scale).into());
                ui.set_text_scale_label(format!("{:.2}x", cfg.text_scale).into());
                ui.set_info_scale_label(format!("{:.2}x", cfg.info_scale).into());
            }
        },
    );

    // ── 7. Run Slint event loop ───────────────────────────────────────────────
    if let Err(e) = ui.run() {
        crate::clog!("[Slint] Run error: {e}");
    }

    // ── 8. Save config after window closes ───────────────────────────────────
    if let (Ok(mut state), Ok(cfg)) = (app_state.try_lock(), overlay_config.try_lock()) {
        // Persist main-window position
        let pos = ui
            .window()
            .position()
            .to_logical(ui.window().scale_factor());
        state.config.config_mut().main_window.x = Some(pos.x as i32);
        state.config.config_mut().main_window.y = Some(pos.y as i32);
        // Persist overlay config
        state.config.config_mut().overlay = cfg.clone();
        if let Err(e) = state.config.save() {
            crate::clog!("[Slint] Config save failed: {e}");
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Show a snackbar toast on the control panel.
/// Uses a Rust background thread + `invoke_from_event_loop` for the dismiss,
/// with an `AtomicU32` generation counter so a late-firing thread from a
/// previous notification never hides a newer one.
fn show_snack(ui: &ControlPanel, snack_gen: &Arc<AtomicU32>, message: &str, is_error: bool) {
    ui.set_snack_message(message.into());
    ui.set_snack_is_error(is_error);
    ui.set_snack_open(true);

    let this_gen = snack_gen.fetch_add(1, Ordering::Relaxed) + 1;
    let gen_clone = snack_gen.clone();
    let ui_weak = ui.as_weak();
    let duration = ui.get_snack_duration_ms() as u64;

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(duration));
        let _ = slint::invoke_from_event_loop(move || {
            if gen_clone.load(Ordering::Relaxed) == this_gen
                && let Some(u) = ui_weak.upgrade()
            {
                u.set_snack_open(false);
            }
        });
    });
}
