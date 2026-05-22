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

//! Crimson Desert TomTom Arrow - Rust rewrite
//!
//! A Windows desktop navigation helper for Crimson Desert that:
//! - Attaches to the running game process
//! - Scans memory for game state (position, marker, camera heading)
//! - Installs runtime hooks to capture live data
//! - Displays a transparent overlay arrow pointing to the active marker
//! - Persists settings to JSON config

// ── Lint configuration ───────────────────────────────────────────────────────
// These pedantic/nursery lints produce too much noise for an application crate.
#![allow(clippy::must_use_candidate)] // #[must_use] is for library APIs
#![allow(clippy::missing_errors_doc)] // # Errors doc is for public libraries
#![allow(clippy::missing_panics_doc)] // same
#![allow(clippy::missing_const_for_fn)] // premature optimisation for app code
#![allow(clippy::doc_markdown)] // backtick style is minor
#![allow(clippy::suboptimal_flops)] // mul_add is not meaningful here
#![allow(clippy::unreadable_literal)] // GDI hex constants are fine as-is
#![allow(clippy::cast_possible_truncation)] // intentional: platform-specific ptr/u32 casts
#![allow(clippy::cast_possible_wrap)] // intentional: signed/unsigned in WinAPI
#![allow(clippy::cast_sign_loss)] // intentional: rendering/math casts
#![allow(clippy::cast_precision_loss)] // intentional: f32/u32 in GDI rendering
#![allow(clippy::too_many_lines)] // large GUI message-pump functions
#![allow(clippy::too_many_arguments)] // renderer draw helpers
#![allow(clippy::option_if_let_else)] // clearer than map_or in complex cases
#![allow(clippy::items_after_statements)] // const inside fn body is idiomatic
#![allow(clippy::struct_excessive_bools)] // snapshot/state structs use flags
#![allow(clippy::type_complexity)] // Arc<Mutex<Option<...>>> is acceptable
#![allow(clippy::manual_c_str_literals)] // PCSTR needs *const u8 not *const c_char
#![allow(clippy::significant_drop_tightening)] // drop scoping is already explicit
#![allow(clippy::struct_field_names)] // field names that mirror type names are intentional

pub mod app;
pub mod config;
pub mod error;
pub mod gui;
pub mod hooks;
pub mod logging;
pub mod navigation;
pub mod process;
pub mod scanner;
