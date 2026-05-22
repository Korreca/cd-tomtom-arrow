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

//! Circular debug logging with timestamps.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Maximum lines to retain in the ring buffer.
const MAX_LOG_LINES: usize = 220;

/// Number of lines to display at once.
const DISPLAY_LINES: usize = 40;

/// Thread-safe circular debug logger.
#[derive(Clone)]
pub struct DebugLogger {
    buffer: Arc<Mutex<VecDeque<String>>>,
}

impl DebugLogger {
    /// Create a new debug logger.
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES))),
        }
    }

    /// Log a message with timestamp.
    pub fn log(&self, msg: impl Into<String>) {
        let msg = msg.into();
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let entry = format!("[{timestamp}] {msg}");

        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= MAX_LOG_LINES {
                buf.pop_front();
            }
            buf.push_back(entry);
        }
    }

    /// Get the last N lines as a single string.
    pub fn get_display_text(&self, lines: usize) -> String {
        let Ok(buf) = self.buffer.lock() else { return String::new(); };
        let skip = buf.len().saturating_sub(lines);
        buf.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
    }

    /// Get the current display text (last 40 lines).
    pub fn get_recent_text(&self) -> String {
        self.get_display_text(DISPLAY_LINES)
    }

    /// Clear the log buffer.
    pub fn clear(&self) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }
    }

    /// Get the total number of log lines.
    pub fn line_count(&self) -> usize {
        self.buffer.lock().ok().map_or(0, |b| b.len())
    }
}

impl Default for DebugLogger {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Global logger ────────────────────────────────────────────────────────────

use std::sync::OnceLock;

static GLOBAL_LOGGER: OnceLock<DebugLogger> = OnceLock::new();

/// Initialize the global logger. Must be called once at application startup
/// before any `clog!` invocations.
pub fn init() {
    GLOBAL_LOGGER.get_or_init(DebugLogger::new);
}

/// Returns the global logger instance, or `None` if `init()` has not been called.
pub fn global() -> Option<&'static DebugLogger> {
    GLOBAL_LOGGER.get()
}

/// Log a message through the global debug logger.
///
/// - In **debug** builds: also writes to `stderr` so it is visible in the console.
/// - In **release** builds: only stored in the in-app circular buffer (no console output).
///
/// # Example
/// ```ignore
/// clog!("[ATTACH] Attached to PID={}", pid);
/// ```
#[macro_export]
macro_rules! clog {
    ($($arg:tt)*) => {{
        let _msg = ::std::format!($($arg)*);
        #[cfg(debug_assertions)]
        ::std::eprintln!("{}", _msg);
        if let Some(__logger) = $crate::logging::global() {
            __logger.log(&_msg);
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging() {
        let logger = DebugLogger::new();
        logger.log("Test message 1");
        logger.log("Test message 2");
        let text = logger.get_recent_text();
        assert!(text.contains("Test message 1"));
        assert!(text.contains("Test message 2"));
    }

    #[test]
    fn test_max_lines() {
        let logger = DebugLogger::new();
        for i in 0..300 {
            logger.log(format!("Line {}", i));
        }
        assert_eq!(logger.line_count(), MAX_LOG_LINES);
    }
}
