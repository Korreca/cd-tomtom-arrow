<div style="display:flex; align-items:center; gap:12px;">
  <h1 style="margin:0; flex:1;">CD TomTom Arrow</h1>
  <a href="https://ko-fi.com/B0B81YRJV7">
    <img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="ko-fi">
  </a>
</div>

A navigation overlay for **Crimson Desert** that renders a real-time directional arrow pointing to your active map marker.

| | |
|---|---|
| **Platform** | Windows 10 / 11 (64-bit) |
| **Language** | Rust |
| **License** | [GPL v3](LICENSE) |
| **Releases** | [github.com/Korreca/cd-tomtom-arrow/releases](https://github.com/Korreca/cd-tomtom-arrow/releases) |
| **Nexus Mods** | [crimsondesert/mods/2189](https://www.nexusmods.com/crimsondesert/mods/2189) |

---

## Features

- **Directional arrow overlay** -- frameless transparent window that rotates to point toward the active map marker.
- **Live distance and height data** -- displays metres remaining and elevation difference.
- **Control panel** -- tabbed UI (Navigation, Markers, Routes, Settings) to manage all options without leaving the game.
- **Configurable overlay** -- opacity, scale, position lock, auto-hide below a distance threshold, hide on inactivity, and offset adjustments.
- **Persistent settings** -- all preferences are stored in `config.json` next to the executable.

---

## How it works

CD TomTom Arrow attaches to the running Crimson Desert process and captures three pieces of live data:

| Data | Method |
|---|---|
| Player position (x, y, z) | AOB scan -> cave injection hook |
| Map marker destination (x, y, z) | AOB scan -> cave injection hook |
| Camera heading | AOB scan -> cave injection hook |

**AOB (Array of Bytes) scanning** searches the loaded game modules for known byte patterns to locate specific code sites at runtime -- no hardcoded addresses, works across game patches.

**Cave injection** temporarily patches a few bytes at each scan site to redirect execution through a small shellcode stub that copies the relevant register values into a shared memory buffer. The original bytes are restored immediately after; the hook exists only for the duration of a single instruction capture.

The overlay reads those values, computes bearing and distance, and rotates the arrow accordingly.

Nothing is written to game files on disk. The hooks touch only live process memory and are cleaned up when the tool exits.

---

## Antivirus false positives

Several AV engines flag this tool with generic heuristics. This is expected behaviour for tools of this kind and is fully documented.

**Why detections are triggered:**

- The tool enumerates running processes and opens a handle to the game (`OpenProcess`).
- It scans the game loaded modules for byte patterns (`ReadProcessMemory`).
- It uses cave injection -- patches a small number of bytes in game memory at runtime to capture register values, then restores them (`WriteProcessMemory`).
- The release binary is compiled with full LTO, symbol stripping, and `opt-level = "z"` (size optimisation), which can resemble packing or obfuscation to heuristic scanners.

These are standard techniques used by profilers, debuggers, and modding overlays. Every line of code that ends up in the binary is in this repository and can be audited or built independently.

The VirusTotal report for the current release is linked from the [Nexus Mods page](https://www.nexusmods.com/crimsondesert/mods/2189), unless the file has already been manually reviewed and approved by Nexus Mods staff following a false positive report.

---

## Building from source

### Requirements

- [Rust](https://rustup.rs/) 1.95 or later (stable toolchain)
- Windows SDK / MSVC build tools (required by `winapi` linkage -- included with [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/))

### Debug build

```
cargo build
```

### Release build

```
cargo build --release
```

The binary is written to `target\release\CD_TomTom.exe`.

---

## Usage

1. Launch **Crimson Desert** from Steam.
2. Run `CD_TomTom.exe`. In most setups no elevated privileges are required; if the process open fails, try running as Administrator.
3. Set a map marker in game. The arrow overlay appears automatically.
4. Use the control panel to adjust opacity, scale, position, and any other settings.

---

## Repository layout

```
src/
  app.rs             Main application loop and state machine
  config.rs          Settings struct and JSON serialization
  error.rs           Error types
  lib.rs             Crate root and module declarations
  logging.rs         clog! macro and logger initialization
  main.rs            Entry point
  assets/            Embedded assets
  gui/               Slint UI -- overlay window and control panel
  hooks/             Cave code generator and byte patcher
  navigation/        Bearing / distance math and runtime state
  process/           Process handle and remote memory reader
  scanner/           AOB pattern scanner
```

---

## Roadmap

Items are ordered roughly by priority and implementation dependency. Nothing here is a firm commitment -- the list reflects current intent.

### Core features

- **Marker management** -- save, edit, and delete custom markers from the Markers tab. Import and export marker sets as files to share with other players.
- **Marker info panel on the overlay** -- optional title and description visible on the overlay for each saved custom marker.
- **Route management** -- create, edit, import, and export routes (ordered or circular sequences of markers) from the Routes tab. The overlay arrow steps through waypoints automatically.
- **Settings tab** -- consolidated settings page covering all configurable options, including keybinds for toggling the GUI, toggling the overlay, and cycling to the previous / next route waypoint.

### Extensibility

- **IPC channel for third-party mods** -- expose a secure local IPC interface so external tools can register with the app and consume its capabilities (live position, active marker, overlay control, etc.). Connections require explicit one-time approval from the user through the control panel, and approval can be revoked at any time. This deliberately avoids bundling every possible use-case into this mod -- something like a QuestHelper is a prime candidate for a separate tool that hooks in over IPC rather than a built-in feature.

### Stretch goals *(only if I feel like it)*

- **Live map view** -- a second-window or second-monitor map that follows the player in real time. Strong preference is for this to live in a dedicated external tool that consumes data over the IPC channel rather than being built in here.
- **Navigable road graph** -- vectorise the road network into a weighted graph and expose shortest-path routing (A → B following roads, arrow guides along the path). Also a strong candidate for an IPC add-on rather than a built-in feature.

---

## Contributing

Bug reports, suggestions, and pull requests are welcome. For mod-related feedback or to get in touch, visit the [author profile on Nexus Mods](https://www.nexusmods.com/profile/korreca).

---

## License

GPL v3 -- see [LICENSE](LICENSE).

Copyright (C) 2026 [Korreca](https://github.com/Korreca/cd-tomtom-arrow/)

Although this project is open source under the GPL v3, wholesale copying of the implementation to ship it as part of an unrelated mod -- without meaningful contribution or attribution -- is not in the spirit of open-source collaboration and is strongly discouraged. If this project inspired your work, a credit or a link back is always appreciated.

---

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/B0B81YRJV7)
