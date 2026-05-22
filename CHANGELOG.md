# Changelog

Only published releases are listed here. Intermediate work-in-progress builds are not
documented. Pre-1.0 versions do not follow a strict consecutive numbering scheme.

## [0.2.46] - 2026-05-22

Complete rewrite in Rust. The Python script and its runtime dependencies are no longer required.

### Added
- **Native executable** — self-contained `.exe` compiled with Rust; no Python installation or `pip` dependencies needed.
- **Slint tabbed control panel** — replaces the flat tkinter window. Tabs: Navigation, Markers *(stub)*, Routes *(stub)*, Settings *(stub)*.
- **Info panel overlay** — text panel rendered alongside the arrow, intended to display the active marker's name and notes once marker management is implemented. Configurable offset, scale, and visibility. Currently shows a placeholder.
- **Manual marker input** — coordinate dialog in the Navigation tab lets you type or paste X/Y/Z destination coordinates as a fallback when the game has no active marker.
- **Clipboard copy/paste** — copy buttons on the position and marker rows write coordinates to the clipboard in `X, Y, Z` format. A quick Paste button sets clipboard coordinates as the manual destination without opening the dialog; the input dialog also has its own Paste button to fill all three fields at once.
- **Snackbar notifications** — non-blocking status toasts in the control panel (config saved, attach errors, etc.).
- **UAC manifest** — elevation is declared in the application manifest; Windows handles the UAC prompt cleanly at launch instead of an in-app re-launch dialog.
- **Config validation** — all values are clamped to defined bounds on load; malformed or out-of-range entries are silently corrected rather than crashing.
- **Open source** — source code published on GitHub under GPL v3. The prototype script was never publicly linked; this is the first public release of the codebase.

### Changed
- **Overlay renderer** — arrow is now drawn with GDI+ via `UpdateLayeredWindow` (per-pixel alpha) instead of a tkinter canvas with a chroma-key background.
- **Config schema** — `sticky_hide_below_until_marker_change` key renamed to `sticky_hide`. Existing `config.json` files from 0.1.x are not automatically migrated; the relevant settings will reset to defaults on first run.
- **Control panel layout** — settings are grouped into labelled panels within each tab rather than a flat vertical list.

### Removed
- **Arrow shadow toggle** — drop-shadow option removed.
- **Debug log panel** — the raw engine log text box is no longer shown in the main window. Structured data panels in the Navigation tab cover the relevant information.

### Performance
- **Binary size** — release build is ~6.8 MB, down from ~12 MB for the frozen Python executable.
- **CPU usage** — sits at a constant ~0 % at idle; the Python version held a steady 0.2–0.3 % due to the tkinter polling loop.
- **Memory footprint** — working set under 7 MB, down from ~18 MB. Of the current 7 MB, over 5 MB is the Slint GUI runtime and cannot be reduced further.

## [0.1.35] - 2025-05-01

Initial public release. Rapid proof-of-concept prototype written as a Python script with a tkinter UI. Not intended as a long-term codebase — published to validate the hook approach and gather early feedback before committing to a full implementation.

### Added
- AOB scan + cave injection to capture live player position, map marker destination, and camera heading from the running game process.
- Frameless transparent arrow overlay (tkinter canvas, chroma-key transparency) that rotates toward the active map marker.
- Distance text displayed below the arrow; colour fades green → red with turn angle.
- Optional drop shadow on the arrow.
- Overlay controls: opacity, scale, position lock, auto-hide below a distance threshold, sticky-hide until marker changes, inactivity hide with configurable timeout, distance-text offset X/Y, text scale.
- Persistent settings saved to `config.json` next to the script.
- Debug log panel in the main window showing engine events.
- In-app re-launch as Administrator if the process open fails.
- Reconnect button for manual re-attach after a game restart.
