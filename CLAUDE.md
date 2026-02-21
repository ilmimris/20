# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Twenty20** is a native macOS menu bar application built with Tauri that enforces the 20-20-20 eye health rule. Every 20 minutes, it shows a non-dismissible full-screen overlay reminding users to look 20 feet away for 20 seconds. It lives in the system tray (no Dock icon) and uses native macOS APIs extensively.

**Target**: macOS 13+. This is macOS-only; no cross-platform support planned.

## Commands

### Development
```bash
npm install                      # Install frontend deps (run once)
npm run tauri dev                # Start dev server + Tauri app (hot-reload)
```

### Quality Checks (run before committing)
```bash
cd src-tauri
cargo fmt -- --check             # Check Rust formatting
cargo clippy --all-targets -- -D warnings  # Lint (warnings are errors)
cd ..
npm run build                    # Frontend build check (catches TS/Svelte errors)
```

### Build
```bash
npm run tauri build              # Full release DMG build
```

There are no automated tests. CI runs the quality checks above.

## Architecture

### Layer Separation
- **`src/`** — Svelte 5 frontend (runes mode). Only one window: `BreakOverlay.svelte` for the full-screen break overlay. The settings UI is NOT web-based.
- **`src-tauri/src/`** — Rust backend. All timer logic, system integration, and macOS APIs live here.

### Key Rust Modules
| File | Responsibility |
|------|---------------|
| `lib.rs` | App setup, Tauri builder, spawns `run_timer_loop()` |
| `timer.rs` | Timer state machine, persists to `~/.local/share/twenty20/timer_state.json` |
| `config.rs` | User config (TOML at `~/.config/twenty20/config.toml`), validation |
| `commands.rs` | Tauri `#[command]` handlers invoked from frontend |
| `tray.rs` | System tray icon + menu, icon state animation |
| `overlay.rs` | Creates/destroys full-screen webview windows on all displays |
| `settings_window.rs` | **Native NSPanel** settings UI built with objc2 (not a webview) |
| `strict_mode.rs` | CGEventTap input suppression during breaks; Escape×3 in 5s to force-skip |
| `meeting.rs` | Polls every 30s for active video calls (NSWorkspace + window title scan) |
| `audio.rs` | Native NSSound playback |

### Timer Loop (`lib.rs` + `timer.rs`)
The core loop ticks every 1 second:
1. Work phase countdown → emits `timer:tick` events to tray
2. Pre-break warning at 60s remaining → notification
3. Break trigger → `overlay.rs` opens windows, `audio.rs` plays sound, `strict_mode.rs` enables CGEventTap if needed
4. Break phase countdown → emits `break:tick` to overlay frontend
5. Break end → closes overlays, disables strict mode, resets timer

Meeting detection pauses the timer; manual pause has a timeout. State persists every 30 ticks.

### Frontend ↔ Backend Communication
- **Events** (backend→frontend): `break:tick`, `break:end`, `timer:tick`
- **Commands** (frontend→backend): `get_overlay_config`, `force_skip_break`, `test_sound`
- **Config injection**: Backend sets `window.__TWENTY20_OVERLAY_CONFIG__` before overlay window loads (contains `breakDuration`, `isPrimary`, `isStrictMode`)

### macOS Private APIs
The app uses `macOS-private-api` Tauri feature and direct objc2 bindings for:
- `NSPanel` (settings window, non-activating)
- `NSApplication` activation policy → Accessory (no Dock icon)
- `NSSound` (audio)
- `CGEventTap` (strict mode input blocking)
- `NSWorkspace` (meeting detection via running app bundle IDs)

These require macOS-specific entitlements (`src-tauri/entitlements.plist`) and Accessibility permission for CGEventTap.

## Configuration

User-editable settings in `~/.config/twenty20/config.toml`:
- `work_interval_minutes` (1–60, default 20)
- `break_duration_seconds` (5–60, default 20)
- `strict_mode` (bool, default false) — blocks all input via CGEventTap
- `overlay_theme`: `"dark"` | `"light"` | `"nature"` (default `"dark"`)
- `sound`: `"off"` | `"chime"` | `"whitenoise"` (default `"off"`)
- `pre_warning_seconds` (0 or 30–120, default 60)
- `meeting_detection` (bool, default true)
- `launch_at_login` (bool, default true)

## Important Constraints

- **Strict mode breaks cannot be dismissed** except via the triple-Escape escape hatch (3 presses in 5 seconds). This is intentional per the PRD.
- The settings window is a **native NSPanel** (objc2), not a Tauri webview — changes to settings UI require Rust/objc2 code, not Svelte.
- The tray icon uses embedded PNG bytes (not file paths) and has three states: open eye (normal), blink (pre-warning), rest (break active).
- `rodio` is listed in Cargo.toml but unused — audio uses NSSound exclusively.
