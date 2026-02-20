# Product Requirements Document: Twenty20 — 20-20-20 Eye Care App for macOS

**Version:** 1.0
**Date:** 2026-02-18
**Status:** Draft

---

## 1. Overview

### 1.1 Product Summary

Twenty20 is a macOS menu bar app built with Tauri that enforces the **20-20-20 rule** to reduce digital eye strain. Every 20 minutes, it overlays the screen with a full-screen break prompt, instructing the user to look at something 20 feet away for 20 seconds. After the 20-second break completes, the overlay dismisses automatically and the next cycle begins.

### 1.2 Problem Statement

Prolonged screen use causes Computer Vision Syndrome (CVS), including eye fatigue, dryness, headaches, and blurred vision. The 20-20-20 rule, recommended by optometrists, significantly reduces these symptoms when practiced consistently. Existing solutions are either too easy to dismiss, lack enforcement, or are heavy Electron-based apps with excessive resource usage.

### 1.3 Goals

- Deliver a lightweight, low-overhead native-feeling macOS app.
- Enforce screen breaks with a non-dismissible (or minimally dismissible) overlay.
- Require zero ongoing user interaction after initial setup.
- Reside quietly in the system tray until a break is needed.

### 1.4 Non-Goals

- iOS / Windows / Linux support (v1.0).
- Mac App Store distribution (v1.0); initial release is direct DMG via GitHub Releases.
- Syncing break history across devices.
- Calendar integration — Outlook 365, Google Calendar, Apple iCal (v1.1).
- Writing to or modifying any calendar data.
- Custom break exercises (eye rolls, palming, etc.) — post-v1.0.

---

## 2. Target Users

| Persona | Description |
|---|---|
| Knowledge workers | Engineers, designers, writers spending 6+ hours/day at a screen |
| Students | Laptop-heavy study sessions |
| Remote workers | Lack structured office breaks |

---

## 3. Tech Stack

| Layer | Technology |
|---|---|
| App framework | [Tauri v2](https://tauri.app) |
| Frontend (overlay UI) | Svelte + TailwindCSS |
| Backend / system integration | Rust |
| Target OS | macOS 13 Ventura and later |
| Distribution | GitHub Releases (DMG, notarized) |

**Rationale for Tauri:** Significantly smaller binary than Electron (~10 MB vs ~150 MB), native Rust backend for OS integration, lower memory footprint, built-in system tray API.

---

## 4. Core Features

### 4.1 System Tray Icon

- App lives exclusively in the macOS menu bar (no Dock icon by default).
- Tray icon shows a simple eye glyph.
- Tray icon animates or changes color 60 seconds before an upcoming break.
- Clicking the tray icon opens a popover with:
  - Time until next break (countdown).
  - Meeting status badge: "Meeting detected — paused" when a call is active.
  - "Skip next break" button (logs skip, resets timer). Hidden in strict mode.
  - "Pause for 30 min / 1 hr" option. Hidden in strict mode.
  - "Settings" link.
  - "Quit" option.

### 4.2 Break Overlay

- A full-screen, topmost overlay window appears on **all connected displays** simultaneously.
- Overlay is rendered as a Tauri `WebviewWindow` set to `always_on_top`, `fullscreen`, and `skip_taskbar`.
- macOS `NSApplication.presentationOptions` set to hide menu bar and Dock during overlay.
- Overlay content:
  - Dim background (80% opacity dark layer) with a calming visual (breathing animation or nature image).
  - Large countdown timer (20 → 0 seconds).
  - Instruction text: *"Look at something 20 feet away"*.
  - Optional: ambient sound (soft chime or white noise) via `rodio` crate.
- After 20 seconds the overlay closes automatically. No manual close button in strict mode.
- **Strict mode** (user opt-in):
  - "Skip" and "Pause" controls are hidden from the tray popover.
  - The overlay consumes all keyboard and mouse input — nothing passes through to underlying apps.
  - macOS `CGEventTap` (or `NSEvent.addGlobalMonitorForEvents`) suppresses input events for the overlay duration.
  - **Emergency escape hatch:** pressing `Escape` three times within five seconds force-closes the overlay. The break is logged as "force-skipped" with a timestamp. This requires no password — the 3-tap friction is sufficient to prevent accidents while allowing genuine emergencies.
  - Strict mode can only be toggled in Settings between breaks, never mid-overlay.

### 4.3 Timer Engine (Rust backend)

- Uses `tokio` async runtime with a 20-minute interval timer.
- Persists timer state to disk so breaks survive app restarts without resetting the full 20-minute window.
- Emits Tauri events to the frontend: `break:start`, `break:end`, `break:skip`, `timer:tick`.
- Handles system sleep/wake: resumes countdown from where it left off after wake.

### 4.4 Settings

Accessible from the tray popover. Stored in `~/.config/twenty20/config.toml`.

| Setting | Default | Options |
|---|---|---|
| Work interval | 20 min | 1–60 min |
| Break duration | 20 sec | 5–60 sec |
| Strict mode (no skip) | Off | On / Off |
| Overlay theme | Dark | Dark / Light / Nature |
| Sound | Off | Off / Chime / White noise |
| Launch at login | On | On / Off |
| Show break notification pre-warning | On (60 sec) | Off / 30 sec / 60 sec / 2 min |
| Meeting detection (auto-pause) | On | On / Off |

### 4.5 Notifications

- macOS native notification sent before break (configurable lead time).
- Notification is informational only; the overlay is the enforcer.

### 4.6 Launch at Login

- Uses macOS `SMAppService` (Tauri plugin or direct Rust FFI) to register as a login item.
- Enabled by default on first launch after user confirmation.

### 4.7 Meeting Detection

Automatically pauses the break timer when the user is in an active video call, then resumes once the call ends.

**Detection strategy (Rust, all layers polled every 30 seconds, fully local):**

1. **Native app detection** — query `NSWorkspace.sharedWorkspace.runningApplications` for known conferencing apps:
   - Zoom (`us.zoom.xos`)
   - Microsoft Teams (`com.microsoft.teams2`)
   - Webex (`Cisco-Systems.Spark`)
   - FaceTime (`com.apple.FaceTime`)
   - Discord (`com.hnc.Discord`) — only when in a voice/video channel (detected via window title)

2. **Window title detection** — for browser-based calls (Google Meet, Teams Web), query `CGWindowListCopyWindowInfo` for browser windows whose title matches patterns: `"Meet – "`, `"Zoom Meeting"`, `"Microsoft Teams"`, `"On a call"`.

3. **Microphone / camera in-use indicator** — as a fallback, check macOS privacy indicators: if the camera or microphone usage indicator is active (`IOKit` or `AXUIElement` introspection), treat the session as a meeting.

**Behavior:**

- When a meeting is detected: timer pauses silently; tray icon updates to show "Paused — meeting in progress"; no break overlay is shown.
- When the meeting ends: timer resumes from where it left off (does not reset to 20 min).
- If a break was already triggered (overlay open) when a meeting starts: overlay closes immediately and timer resets to 20 min for the next cycle.
- All detection is local-only; no network calls are made; no meeting content is read.

**Permissions required:**

- Accessibility permission (`AXUIElement`) is needed for layer 2 (browser window title matching). It is requested on first run with a plain-language explanation.
- **If the user denies Accessibility permission:** layer 2 is silently disabled; layers 1 (native app) and 3 (mic/camera) continue to operate. A one-time warning notification is shown: *"Browser-based call detection disabled — grant Accessibility access in System Settings → Privacy & Security → Accessibility to enable it."* A persistent warning badge appears in the tray popover until access is granted. The app never re-prompts automatically.
- No screen recording permission needed; only window metadata (title, app bundle ID) is accessed.

---

## 5. UX Flow

```
App Launch
    │
    ▼
Menu bar icon appears
    │
    ▼
20-min countdown starts ──────────────────────────────────────┐
    │                                                          │
    │  [meeting detected?]                                     │
    ├──Yes──► Timer pauses; tray shows "Meeting in progress"   │
    │         Poll every 30s; when meeting ends → resume timer─┤
    │                                                          │
    │ (60 sec before break, no meeting)                        │
    ▼                                                          │
Native notification: "Break in 60 seconds"                    │
    │                                                          │
    ▼                                                          │
Full-screen overlay appears on all displays                   │
    │                                                          │
    │  [strict mode ON]        [strict mode OFF]               │
    │  Input consumed          User may skip via tray          │
    │  Esc×3 = force-skip      or wait out the timer           │
    │                                                          │
    ▼                                                          │
20-second countdown plays                                     │
    │                                                          │
    ▼                                                          │
Overlay auto-dismisses ───────────────────────────────────────┘
(cycle repeats)
```

**Skip flow:**
- User clicks "Skip next break" in tray popover → break is skipped → timer resets to 20 min → skip is logged.
- In strict mode, "Skip" option is hidden; the only escape is the 3× Escape key sequence.

**Pause flow:**
- User selects "Pause for 30 min" → timer pauses → tray icon shows pause indicator → timer resumes after selected duration.
- Not available in strict mode.

**Meeting flow:**
- Meeting detected mid-countdown → timer pauses, no overlay shown.
- Meeting detected while overlay is open → overlay closes immediately, timer resets to 20 min.
- Meeting ends → timer resumes from remaining time (does not restart from 20 min).

---

## 6. Multi-Monitor Support

- On break trigger, Tauri opens a separate `WebviewWindow` per display using `NSScreen.screens`.
- All overlay windows open simultaneously and close simultaneously.
- Primary display shows the countdown; secondary displays show the dim overlay without the timer.

---

## 7. Accessibility

- Overlay text uses system font at minimum 36pt for readability during a break.
- Sufficient contrast ratio (WCAG AA) on overlay text.
- VoiceOver announces the break start and remaining time every 5 seconds.
- Keyboard shortcut to open tray popover (configurable, default `⌥⌘E`).

---

## 8. Performance Requirements

| Metric | Target |
|---|---|
| App binary size | < 15 MB |
| Idle memory usage | < 30 MB |
| CPU usage (idle) | < 0.5% |
| Overlay render time | < 200 ms from trigger to visible |
| Startup time | < 1 second to tray ready |

---

## 9. Security & Privacy

- No network requests; app is fully offline.
- No telemetry or analytics in v1.0.
- Config file stored in user-space; no admin privileges required.
- Hardened Runtime enabled for notarization (required for direct DMG distribution on macOS). App Sandbox is intentionally not used in v1.0, as it would block `CGEventTap` and `CGWindowListCopyWindowInfo`.

---

## 10. Tauri-Specific Implementation Notes

### 10.1 Window Configuration (`tauri.conf.json`)

```json
{
  "app": {
    "windows": [],
    "trayIcon": {
      "iconPath": "icons/eye.png",
      "tooltip": "Twenty20"
    }
  }
}
```

Overlay windows are created programmatically at runtime:

```rust
tauri::WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
    .fullscreen(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .decorations(false)
    .transparent(true)
    .build()?;
```

### 10.2 Tray Setup

```rust
let tray = TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .menu(&menu)
    .on_menu_event(handle_menu_event)
    .build(app)?;
```

### 10.3 Rust Crates

| Crate | Purpose |
|---|---|
| `tokio` | Async timer and event loop |
| `serde` / `toml` | Config serialization |
| `tauri-plugin-autostart` | Launch at login |
| `rodio` | Optional audio playback |
| `objc2` / `objc2-app-kit` | macOS NSScreen, presentation options, NSWorkspace |
| `core-graphics` | `CGWindowListCopyWindowInfo` for browser window titles |
| `accessibility` (or raw `AXUIElement` via `objc2`) | Browser window title access for web-based meeting detection |

---

## 11. Milestones

| Milestone | Deliverables |
|---|---|
| M1 — Foundation | Tauri project scaffold, tray icon, settings file read/write |
| M2 — Timer Engine | Rust countdown, persist state, sleep/wake handling |
| M3 — Overlay | Full-screen overlay window, multi-monitor, countdown UI |
| M4 — Strict Mode | Input consumption via `CGEventTap`, 3× Escape escape hatch, force-skip logging |
| M5 — Meeting Detection | NSWorkspace app polling, CGWindowList title matching, mic/camera fallback, auto-pause/resume |
| M6 — Polish | Notifications, animations, sound, accessibility |
| M7 — Distribution | DMG packaging, code signing, notarization, GitHub Release workflow (CI uploads signed DMG as release asset) |

---

## 12. Success Metrics (Post-Launch)

- Daily Active Users maintaining ≥ 80% break compliance rate.
- < 1% crash rate.
- Average idle memory footprint under 30 MB across user devices.
- App Store rating ≥ 4.5 stars within 90 days of launch.
