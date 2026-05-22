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

//! Isometric arrow renderer — GDI+ drawing pipeline.
//!
//! Provides [`render_layered_frame`] (main per-pixel alpha render via
//! `UpdateLayeredWindow`) and [`draw_isometric_arrow`] (GDI+ 3-D arrow +
//! GDI distance text, shared with the legacy WM_PAINT fallback path).
#![allow(unsafe_code)]
#![allow(clippy::cast_possible_truncation)] // intentional: rendering coordinates & opacity are bounded
#![allow(clippy::cast_precision_loss)] // intentional: OVERLAY_SIZE=600 fits f32 exactly
#![allow(clippy::cast_sign_loss)] // intentional: opacity is clamped 0-1, pixel count is positive
#![allow(clippy::cast_possible_wrap)] // intentional: polygon vertex counts are small
#![allow(clippy::too_many_arguments)] // render_layered_frame has many params by design
#![allow(clippy::too_many_lines)] // rendering pipeline is long by nature

use core::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::OnceLock;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CLIP_DEFAULT_PRECIS, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_QUALITY,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, ETO_OPTIONS, ExtTextOutW, FillRect, GRAPHICS_MODE,
    GetDC, GetTextExtentPoint32W, HDC, HGDIOBJ, OUT_DEFAULT_PRECIS, RGBQUAD, ReleaseDC,
    SelectObject, SetBkMode, SetGraphicsMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows::core::PCWSTR;

/// Window and bitmap edge length in pixels.
pub(crate) const OVERLAY_SIZE: i32 = 600;

/// Info panel data passed to the renderer.
/// When `None` is passed to the draw functions the panel is not rendered.
pub(crate) struct InfoOverlay {
    pub title: String,
    pub description: String,
    pub offset_x: i32,
    pub offset_y: i32,
    pub scale: f32,
}

/// Color in RGB (0-255 each).
#[derive(Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    fn lerp(c1: Self, c2: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (f32::from(c1.r) + (f32::from(c2.r) - f32::from(c1.r)) * t) as u8,
            g: (f32::from(c1.g) + (f32::from(c2.g) - f32::from(c1.g)) * t) as u8,
            b: (f32::from(c1.b) + (f32::from(c2.b) - f32::from(c1.b)) * t) as u8,
        }
    }

    fn brighten(self, factor: f32) -> Self {
        Self {
            r: ((f32::from(self.r) + (255.0 - f32::from(self.r)) * factor) as u8),
            g: ((f32::from(self.g) + (255.0 - f32::from(self.g)) * factor) as u8),
            b: ((f32::from(self.b) + (255.0 - f32::from(self.b)) * factor) as u8),
        }
    }

    fn darken(self, factor: f32) -> Self {
        Self {
            r: ((f32::from(self.r) * (1.0 - factor)) as u8),
            g: ((f32::from(self.g) * (1.0 - factor)) as u8),
            b: ((f32::from(self.b) * (1.0 - factor)) as u8),
        }
    }
}

/// 2D point.
#[derive(Clone, Copy, Debug)]
struct Point2D {
    x: f32,
    y: f32,
}

/// 3D point.
#[derive(Clone, Copy)]
struct Point3D {
    x: f32,
    y: f32,
    z: f32,
}

impl Point3D {
    /// Rotate around X, Y, Z axes (pitch, yaw, roll).
    fn rotate_xyz(&self, pitch: f32, yaw: f32, roll: f32) -> Self {
        let mut p = *self;

        // Yaw (Z axis)
        let cos_y = yaw.cos();
        let sin_y = yaw.sin();
        p = Self {
            x: p.x * cos_y - p.y * sin_y,
            y: p.x * sin_y + p.y * cos_y,
            z: p.z,
        };

        // Pitch (X axis)
        let cos_p = pitch.cos();
        let sin_p = pitch.sin();
        p = Self {
            x: p.x,
            y: p.y * cos_p - p.z * sin_p,
            z: p.y * sin_p + p.z * cos_p,
        };

        // Roll (Y axis)
        let cos_r = roll.cos();
        let sin_r = roll.sin();
        p = Self {
            x: p.x * cos_r + p.z * sin_r,
            y: p.y,
            z: -p.x * sin_r + p.z * cos_r,
        };

        p
    }

    fn project_isometric(&self, cx: f32, cy: f32, scale: f32, squash_y: f32) -> Point2D {
        Point2D {
            x: cx + self.x * scale,
            y: cy + self.y * squash_y * scale,
        }
    }
}

// ── GDI+ flat API (gdiplus.dll) ────────────────────────────────────────────
// winapi 0.3 does not ship GDI+ bindings, so we declare them manually.

/// Opaque GDI+ handle types.
#[allow(non_camel_case_types)]
enum GpGraphics {}
#[allow(non_camel_case_types)]
enum GpBrush {}
#[allow(non_camel_case_types)]
enum GpPen {}

/// GDI+ integer-coordinate point (X/Y capital to match the Windows header).
#[repr(C)]
#[allow(non_snake_case)]
struct GpPoint {
    X: i32,
    Y: i32,
}

/// Startup input — version 1, everything else zero.
#[repr(C)]
#[allow(non_snake_case)]
struct GdiplusStartupInput {
    GdiplusVersion: u32,
    DebugEventCallback: usize, // function pointer — 0 = null
    SuppressBackgroundThread: i32,
    SuppressExternalCodecs: i32,
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const GdiplusStartupInput, output: *mut u8)
    -> u32;
    fn GdipCreateFromHDC(hdc: HDC, graphics: *mut *mut GpGraphics) -> u32;
    fn GdipSetSmoothingMode(graphics: *mut GpGraphics, mode: i32) -> u32;
    fn GdipDeleteGraphics(graphics: *mut GpGraphics) -> u32;
    fn GdipCreateSolidFill(color: u32, brush: *mut *mut GpBrush) -> u32;
    fn GdipFillPolygonI(
        graphics: *mut GpGraphics,
        brush: *mut GpBrush,
        points: *const GpPoint,
        count: i32,
        fill_mode: i32,
    ) -> u32;
    fn GdipDeleteBrush(brush: *mut GpBrush) -> u32;
    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut GpPen) -> u32;
    fn GdipDrawPolygonI(
        graphics: *mut GpGraphics,
        pen: *mut GpPen,
        points: *const GpPoint,
        count: i32,
    ) -> u32;
    fn GdipDeletePen(pen: *mut GpPen) -> u32;
}

/// GDI+ is initialized once per process (token is never needed again).
static GDIPLUS_TOKEN: OnceLock<usize> = OnceLock::new();

fn init_gdiplus() {
    GDIPLUS_TOKEN.get_or_init(|| unsafe {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        let mut token: usize = 0;
        GdiplusStartup(&raw mut token, &raw const input, ptr::null_mut());
        token
    });
}

/// Draw the isometric navigation arrow using GDI+ for native anti-aliased edges.
/// Polygons are filled with GdipFillPolygonI + SmoothingModeAntiAlias (4).
/// Text is still rendered with GDI (ExtTextOutW) which has its own hinting AA.
pub(crate) fn draw_isometric_arrow(
    hdc: HDC,
    width: i32,
    height: i32,
    turn_deg: f32,
    scale: f32,
    text_scale: f32,
    distance: f32,
    height_diff: f32,
    text_offset_x: i32,
    text_offset_y: i32,
    info: Option<&InfoOverlay>,
) {
    init_gdiplus();
    unsafe {
        // ── Geometry ────────────────────────────────────────────────────────────
        let cx = (width / 2) as f32;
        let cy = (height as f32) * 0.35;
        let iso_scale = 0.7 * scale.clamp(0.5, 2.5);

        //  Arrow shape ↑: wide triangle head + narrow rectangle body.
        //  Head base y = body top y (no gap, no shelf face).
        //  12 vertices — front z=22, back z=0
        //
        //  idx   x       y      z    description
        let raw: &[(f32, f32, f32)] = &[
            (0.0, -88.0, 22.0),   //  0  tip              front
            (-38.0, -25.0, 22.0), //  1  head-left         front
            (38.0, -25.0, 22.0),  //  2  head-right        front
            (-14.0, -25.0, 22.0), //  3  body-top-left     front  (same y as head base)
            (14.0, -25.0, 22.0),  //  4  body-top-right    front
            (-14.0, 60.0, 22.0),  //  5  body-bot-left     front
            (14.0, 60.0, 22.0),   //  6  body-bot-right    front
            (0.0, -88.0, 0.0),    //  7  tip              back
            (-38.0, -25.0, 0.0),  //  8  head-left         back
            (38.0, -25.0, 0.0),   //  9  head-right        back
            (-14.0, -25.0, 0.0),  // 10  body-top-left     back
            (14.0, -25.0, 0.0),   // 11  body-top-right    back
            (-14.0, 60.0, 0.0),   // 12  body-bot-left     back
            (14.0, 60.0, 0.0),    // 13  body-bot-right    back
        ];

        let iso_pitch = 0.9599f32; // 55° elevation
        let yaw = turn_deg.to_radians();
        let rot: Vec<Point3D> = raw
            .iter()
            .map(|&(x, y, z)| Point3D { x, y, z }.rotate_xyz(iso_pitch, yaw, 0.0))
            .collect();
        let sp: Vec<(Point2D, f32)> = rot
            .iter()
            .map(|p| (p.project_isometric(cx, cy, iso_scale, 0.65), p.z))
            .collect();

        let pt = |i: usize| GpPoint {
            X: sp[i].0.x as i32,
            Y: sp[i].0.y as i32,
        };
        let dz = |idx: &[usize]| idx.iter().map(|&i| sp[i].1).sum::<f32>() / idx.len() as f32;

        // ── Colors ──────────────────────────────────────────────────────────────
        let align = (turn_deg.abs() / 180.0).min(1.0);
        let c_good = Color {
            r: 74,
            g: 222,
            b: 128,
        };
        let c_bad = Color {
            r: 220,
            g: 68,
            b: 68,
        };
        let base = Color::lerp(c_good, c_bad, align);
        let c_out = base.darken(0.68); // outline pen
        let c_front = base.brighten(0.28); // front face   (most lit)
        let c_right = base; // right sides  (medium)
        let c_left = base.darken(0.40); // left sides   (shadow)
        let c_bot = base.darken(0.60); // bottom / back

        // ── Face table — 11 faces, no horizontal shelf ───────────────────────────
        // Head and body share the same junction y=-25.
        // Inner Z-walls fill the width gap (head±38 → body±14); they face inward
        // so they are not visible as a step from the outside.
        let mut faces: Vec<(Vec<GpPoint>, Color, f32)> = vec![
            // Front
            (vec![pt(0), pt(1), pt(2)], c_front, dz(&[0, 1, 2])),
            (vec![pt(3), pt(4), pt(6), pt(5)], c_front, dz(&[3, 4, 5, 6])),
            // Left outer wall (head side)
            (vec![pt(1), pt(0), pt(7), pt(8)], c_left, dz(&[0, 1, 7, 8])),
            // Left inner Z-wall (head-left → body-top-left, inward-facing)
            (
                vec![pt(1), pt(8), pt(10), pt(3)],
                c_left,
                dz(&[1, 3, 8, 10]),
            ),
            // Left body wall
            (
                vec![pt(3), pt(5), pt(12), pt(10)],
                c_left,
                dz(&[3, 5, 10, 12]),
            ),
            // Right outer wall (head side)
            (vec![pt(0), pt(2), pt(9), pt(7)], c_right, dz(&[0, 2, 7, 9])),
            // Right inner Z-wall
            (
                vec![pt(2), pt(9), pt(11), pt(4)],
                c_right,
                dz(&[2, 4, 9, 11]),
            ),
            // Right body wall
            (
                vec![pt(4), pt(11), pt(13), pt(6)],
                c_right,
                dz(&[4, 6, 11, 13]),
            ),
            // Bottom
            (
                vec![pt(5), pt(6), pt(13), pt(12)],
                c_bot,
                dz(&[5, 6, 12, 13]),
            ),
            // Back head
            (vec![pt(7), pt(9), pt(8)], c_bot, dz(&[7, 8, 9])),
            // Back body
            (
                vec![pt(10), pt(12), pt(13), pt(11)],
                c_bot,
                dz(&[10, 11, 12, 13]),
            ),
        ];

        // Painter's algorithm: back-to-front (ascending rotated Z)
        faces.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // ── GDI+ rendering ───────────────────────────────────────────────────────
        let argb_out = 0xFF000000u32
            | (u32::from(c_out.r) << 16)
            | (u32::from(c_out.g) << 8)
            | u32::from(c_out.b);

        let mut graphics: *mut GpGraphics = ptr::null_mut();
        GdipCreateFromHDC(hdc, &raw mut graphics);
        GdipSetSmoothingMode(graphics, 4); // SmoothingModeAntiAlias

        for (pts, color, _) in &faces {
            let argb = 0xFF000000u32
                | (u32::from(color.r) << 16)
                | (u32::from(color.g) << 8)
                | u32::from(color.b);
            let mut brush: *mut GpBrush = ptr::null_mut();
            GdipCreateSolidFill(argb, &raw mut brush);
            GdipFillPolygonI(graphics, brush, pts.as_ptr(), pts.len() as i32, 1);
            GdipDeleteBrush(brush);

            let mut pen: *mut GpPen = ptr::null_mut();
            GdipCreatePen1(argb_out, 1.0f32, 2, &raw mut pen); // 1px, UnitPixel
            GdipDrawPolygonI(graphics, pen, pts.as_ptr(), pts.len() as i32);
            GdipDeletePen(pen);
        }

        GdipDeleteGraphics(graphics);

        // ── Distance text — drawn with GDI (font rasteriser has its own AA) ─────
        let cx1 = (width / 2) as f32;
        let cy1 = (height as f32) * 0.35;
        let font_size =
            (22.0 * scale.clamp(0.5, 2.5) * text_scale.clamp(0.5, 2.5)).max(14.0) as i32;
        let font_name_wide: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let hfont = CreateFontW(
            font_size,
            0,
            0,
            0,
            700,
            0u32,
            0u32,
            0u32,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            0x42u32, // FF_SWISS | VARIABLE_PITCH
            PCWSTR(font_name_wide.as_ptr()),
        );
        let old_font = SelectObject(hdc, HGDIOBJ(hfont.0));
        SetBkMode(hdc, TRANSPARENT);

        let base_x = cx1 as i32 + text_offset_x;
        let base_y = (cy1 + 62.0 * scale.clamp(0.5, 2.5)) as i32 + text_offset_y - 25;
        // Thin 1px halo on all 8 sides — looks clean, not blocky like 2px
        let ofs: [(i32, i32); 8] = [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ];

        // Line 1: distance (always)
        let dist_str = format!("{distance:.0}m");
        let dw: Vec<u16> = dist_str.encode_utf16().chain(std::iter::once(0)).collect();
        let l1 = (dw.len() - 1) as u32;
        let mut sz1: SIZE = mem::zeroed();
        let _ = GetTextExtentPoint32W(hdc, &dw[..l1 as usize], &raw mut sz1);
        let x1 = base_x - sz1.cx / 2;
        SetTextColor(hdc, COLORREF(0x00111111));
        for (dx, dy) in &ofs {
            let _ = ExtTextOutW(
                hdc,
                x1 + dx,
                base_y + dy,
                ETO_OPTIONS(0),
                None,
                PCWSTR(dw.as_ptr()),
                l1,
                None,
            );
        }
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let _ = ExtTextOutW(
            hdc,
            x1,
            base_y,
            ETO_OPTIONS(0),
            None,
            PCWSTR(dw.as_ptr()),
            l1,
            None,
        );

        // Line 2: height diff (only when meaningful)
        if height_diff.abs() > 0.5 {
            let sym = if height_diff > 0.0 {
                "\u{25B2}"
            } else {
                "\u{25BC}"
            };
            let h_str = format!("{} {:.0}m", sym, height_diff.abs());
            let hw: Vec<u16> = h_str.encode_utf16().chain(std::iter::once(0)).collect();
            let l2 = (hw.len() - 1) as u32;
            let mut sz2: SIZE = mem::zeroed();
            let _ = GetTextExtentPoint32W(hdc, &hw[..l2 as usize], &raw mut sz2);
            let x2 = base_x - sz2.cx / 2;
            let y2 = base_y + sz1.cy + 3;
            SetTextColor(hdc, COLORREF(0x00111111));
            for (dx, dy) in &ofs {
                let _ = ExtTextOutW(
                    hdc,
                    x2 + dx,
                    y2 + dy,
                    ETO_OPTIONS(0),
                    None,
                    PCWSTR(hw.as_ptr()),
                    l2,
                    None,
                );
            }
            SetTextColor(hdc, COLORREF(0x00FFFFFF));
            let _ = ExtTextOutW(
                hdc,
                x2,
                y2,
                ETO_OPTIONS(0),
                None,
                PCWSTR(hw.as_ptr()),
                l2,
                None,
            );
        }

        // ── Info panel — title (bold) + description (normal) to the right ────
        if let Some(info) = info {
            let info_scale = info.scale.clamp(0.5, 2.5);
            let title_font_size = (20.0 * info_scale).max(10.0) as i32;
            let desc_font_size = (15.0 * info_scale).max(8.0) as i32;

            // Position: to the right of the arrow center, aligned with upper arrow area
            let info_x = cx1 + 70.0 * scale.clamp(0.5, 2.5) + info.offset_x as f32;
            let info_x = info_x as i32;
            let info_y = (cy1 - 30.0 * scale.clamp(0.5, 2.5) + info.offset_y as f32) as i32;

            // Column width: scales with info_scale but capped to available window space
            let available_w = (width - info_x - 4).max(40);
            let col_width = ((180.0 * info_scale) as i32).min(available_w);

            // ── Title font (bold) ────────────────────────────────────────────
            let title_font_wide: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
            let hfont_title = CreateFontW(
                title_font_size,
                0,
                0,
                0,
                700,
                0u32,
                0u32,
                0u32,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                0x42u32,
                PCWSTR(title_font_wide.as_ptr()),
            );
            SelectObject(hdc, HGDIOBJ(hfont_title.0));

            // Truncate title to fit column width
            let raw_title = &info.title[..info.title.len().min(40)];
            let title_str = {
                let wide: Vec<u16> = raw_title.encode_utf16().chain(std::iter::once(0)).collect();
                let wl = (wide.len() - 1) as u32;
                let mut sz: SIZE = mem::zeroed();
                let _ = GetTextExtentPoint32W(hdc, &wide[..wl as usize], &raw mut sz);
                if sz.cx <= col_width {
                    raw_title.to_string()
                } else {
                    let chars: Vec<char> = raw_title.chars().collect();
                    let mut hi = chars.len();
                    loop {
                        if hi == 0 {
                            break "...".to_string();
                        }
                        let candidate: String = chars[..hi].iter().collect::<String>() + "...";
                        let cw: Vec<u16> =
                            candidate.encode_utf16().chain(std::iter::once(0)).collect();
                        let cl = (cw.len() - 1) as u32;
                        let mut cs: SIZE = mem::zeroed();
                        let _ = GetTextExtentPoint32W(hdc, &cw[..cl as usize], &raw mut cs);
                        if cs.cx <= col_width {
                            break candidate;
                        }
                        hi -= 1;
                    }
                }
            };
            let tw: Vec<u16> = title_str.encode_utf16().chain(std::iter::once(0)).collect();
            let tl = (tw.len() - 1) as u32;
            let mut tsz: SIZE = mem::zeroed();
            let _ = GetTextExtentPoint32W(hdc, &tw[..tl as usize], &raw mut tsz);

            SetTextColor(hdc, COLORREF(0x00111111));
            for (dx, dy) in &ofs {
                let _ = ExtTextOutW(
                    hdc,
                    info_x + dx,
                    info_y + dy,
                    ETO_OPTIONS(0),
                    None,
                    PCWSTR(tw.as_ptr()),
                    tl,
                    None,
                );
            }
            SetTextColor(hdc, COLORREF(0x00FFFFFF));
            let _ = ExtTextOutW(
                hdc,
                info_x,
                info_y,
                ETO_OPTIONS(0),
                None,
                PCWSTR(tw.as_ptr()),
                tl,
                None,
            );

            // ── Description font (normal weight) ────────────────────────────
            let desc_font_wide: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
            let hfont_desc = CreateFontW(
                desc_font_size,
                0,
                0,
                0,
                400,
                0u32,
                0u32,
                0u32,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                0x42u32,
                PCWSTR(desc_font_wide.as_ptr()),
            );
            // Select desc font (deselects hfont_title — now safe to delete)
            SelectObject(hdc, HGDIOBJ(hfont_desc.0));
            let _ = DeleteObject(HGDIOBJ(hfont_title.0));

            // Word-wrap description (capped at 140 chars) into lines
            let raw_desc = &info.description[..info.description.len().min(140)];
            let mut desc_lines: Vec<String> = Vec::new();
            {
                let mut line = String::new();
                for word in raw_desc.split_whitespace() {
                    let candidate = if line.is_empty() {
                        word.to_string()
                    } else {
                        format!("{line} {word}")
                    };
                    let cw: Vec<u16> = candidate.encode_utf16().chain(std::iter::once(0)).collect();
                    let cl = (cw.len() - 1) as u32;
                    let mut cs: SIZE = mem::zeroed();
                    let _ = GetTextExtentPoint32W(hdc, &cw[..cl as usize], &raw mut cs);
                    if !line.is_empty() && cs.cx > col_width {
                        desc_lines.push(line.clone());
                        line = word.to_string();
                    } else {
                        line = candidate;
                    }
                }
                if !line.is_empty() {
                    desc_lines.push(line);
                }
            }

            let line_h = desc_font_size + (desc_font_size / 4).max(3);
            let desc_y_start = info_y + tsz.cy + (title_font_size / 4).max(3);
            for (i, dline) in desc_lines.iter().enumerate() {
                let ly = desc_y_start + i as i32 * line_h;
                let lw: Vec<u16> = dline.encode_utf16().chain(std::iter::once(0)).collect();
                let ll = (lw.len() - 1) as u32;
                SetTextColor(hdc, COLORREF(0x00111111));
                for (dx, dy) in &ofs {
                    let _ = ExtTextOutW(
                        hdc,
                        info_x + dx,
                        ly + dy,
                        ETO_OPTIONS(0),
                        None,
                        PCWSTR(lw.as_ptr()),
                        ll,
                        None,
                    );
                }
                SetTextColor(hdc, COLORREF(0x00FFFFFF));
                let _ = ExtTextOutW(
                    hdc,
                    info_x,
                    ly,
                    ETO_OPTIONS(0),
                    None,
                    PCWSTR(lw.as_ptr()),
                    ll,
                    None,
                );
            }

            // Restore old_font (deselects hfont_desc — now safe to delete)
            SelectObject(hdc, old_font);
            let _ = DeleteObject(HGDIOBJ(hfont_desc.0));
        }

        SelectObject(hdc, old_font);
        let _ = DeleteObject(HGDIOBJ(hfont.0));
    }
}

/// Render the arrow into a 32-bit ARGB DIB and push it to the layered window
/// via `UpdateLayeredWindow` (per-pixel alpha, no `SetLayeredWindowAttributes`).
///
/// Background pixels (pure black = 0x000000) remain fully transparent (A=0).
/// All other pixels (arrow geometry) become fully opaque (`A=opacity_byte`).
pub(crate) unsafe fn render_layered_frame(
    hwnd: HWND,
    opacity: f32,
    scale: f32,
    text_scale: f32,
    turn_deg: f32,
    draw_arrow: bool,
    distance: f32,
    height_diff: f32,
    text_offset_x: i32,
    text_offset_y: i32,
    info: Option<&InfoOverlay>,
) {
    unsafe {
        let width = OVERLAY_SIZE;
        let height = OVERLAY_SIZE;
        let opacity_byte = (opacity.clamp(0.0, 1.0) * 255.0) as u8;

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));

        // 32-bit top-down DIB (BGRA in memory)
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0u32, // BI_RGB
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };

        let mut bits: *mut c_void = ptr::null_mut();
        let Ok(hbitmap) = CreateDIBSection(
            Some(hdc_screen),
            &raw const bmi,
            DIB_RGB_COLORS,
            &raw mut bits,
            None,
            0,
        ) else {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);
            return;
        };
        if bits.is_null() {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);
            return;
        }
        // Save the stock bitmap so we can restore it before DeleteObject (GDI requires deselect-before-delete
        // to avoid leaking the DIB section memory; ~1.4 MB/tick * 60 fps = GDI exhaustion in ~40 s).
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ(hbitmap.0));

        // Black background (BGRA = 0x00000000) — will become transparent
        let bg_brush = CreateSolidBrush(COLORREF(0x000000));
        let rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        FillRect(hdc_mem, &raw const rect, bg_brush);
        let _ = DeleteObject(HGDIOBJ(bg_brush.0));

        SetGraphicsMode(hdc_mem, GRAPHICS_MODE(2)); // GM_ADVANCED
        if draw_arrow {
            draw_isometric_arrow(
                hdc_mem,
                width,
                height,
                turn_deg,
                scale,
                text_scale,
                distance,
                height_diff,
                text_offset_x,
                text_offset_y,
                info,
            );
        }

        // Fix alpha channel: GDI leaves A=0 on all pixels.
        // UpdateLayeredWindow with AC_SRC_ALPHA expects premultiplied pixels.
        // Strategy: set A=255 for all visible (non-black) pixels so they are
        // already "premultiplied" (premult_R = R * 255/255 = R).
        // The overall opacity is applied via SourceConstantAlpha in BLENDFUNCTION,
        // which multiplies every pixel's effective alpha by opacity_byte/255.
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (width * height) as usize);
        for pixel in pixels.iter_mut() {
            let bgr = *pixel & 0x00FF_FFFF;
            if bgr != 0 {
                *pixel = 0xFF000000 | bgr; // full alpha; compositor scales by SourceConstantAlpha
            }
            // pure black stays 0x00000000 (fully transparent)
        }

        // Push to compositor via UpdateLayeredWindow
        let src_pt = POINT { x: 0, y: 0 };
        let wnd_size = SIZE {
            cx: width,
            cy: height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: 0, // AC_SRC_OVER
            BlendFlags: 0,
            SourceConstantAlpha: opacity_byte, // overall window opacity (scales per-pixel alpha)
            AlphaFormat: 1,                    // AC_SRC_ALPHA (per-pixel alpha)
        };
        // Pass null for position — keep the window's current screen position
        if UpdateLayeredWindow(
            hwnd,
            Some(hdc_screen),
            None, // keep current position
            Some(&raw const wnd_size),
            Some(hdc_mem),
            Some(&raw const src_pt),
            COLORREF(0),
            Some(&raw const blend),
            ULW_ALPHA,
        )
        .is_err()
        {
            crate::clog!("[GDI] UpdateLayeredWindow FAILED");
        }

        // Deselect hbitmap before deleting (GDI requires this to actually free the DIB memory).
        SelectObject(hdc_mem, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);
    }
}
