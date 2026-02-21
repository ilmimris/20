# Twenty20

> Protect your eyes. Every 20 minutes, look 20 feet away for 20 seconds.

[![CI](https://github.com/ilmimris/Twenty20/actions/workflows/ci.yml/badge.svg)](https://github.com/ilmimris/Twenty20/actions/workflows/ci.yml)
[![macOS 13+](https://img.shields.io/badge/macOS-13%2B-blue?logo=apple)](https://github.com/ilmimris/Twenty20/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

<p align="center">
  <img src="assets/screenshot-overlay.png" alt="Twenty20 break overlay" width="600" />
</p>

Twenty20 is a lightweight macOS menu bar app that enforces the [20-20-20 rule](https://www.aoa.org/healthy-eyes/eye-and-vision-conditions/computer-vision-syndrome) to reduce digital eye strain. It runs silently in your system tray and shows a full-screen overlay every 20 minutes — no buttons to dismiss, no excuses to skip.

---

## Features

- **Automatic full-screen breaks** — overlay appears on all connected displays every 20 minutes
- **Strict mode** — blocks all keyboard and mouse input during breaks so you actually rest your eyes
- **Smart meeting detection** — pauses the timer automatically when Zoom, Teams, Google Meet, FaceTime, or Discord is active
- **Configurable intervals** — adjust work time (1–60 min) and break duration (5–60 sec)
- **Three overlay themes** — dark, light, and nature
- **Pre-break warning** — optional notification before the break hits
- **Launch at login** — runs silently in the background from startup
- **Tiny footprint** — ~10 MB, built with [Tauri](https://tauri.app/) (not Electron)

---

## Requirements

- macOS 13 Ventura or later
- For strict mode: grant Accessibility permission when prompted (`System Settings → Privacy & Security → Accessibility`)

---

## Installation

1. Download the latest `.dmg` from [Releases](https://github.com/ilmimris/Twenty20/releases)
2. Open the DMG and drag **Twenty20** to your Applications folder
3. Launch it — the eye icon appears in your menu bar
4. Grant Accessibility permission if you plan to use Strict Mode

> **Gatekeeper note:** If macOS blocks the app on first launch, right-click → Open to bypass the warning.

---

## Usage

Twenty20 requires almost no ongoing interaction.

| Action | How |
|--------|-----|
| See time until next break | Click the eye icon in the menu bar |
| Skip the next break | Menu bar → *Skip next break* |
| Pause temporarily | Menu bar → *Pause for 30 min* or *Pause for 1 hr* |
| Open settings | Menu bar → *Settings…* |
| Quit | Menu bar → *Quit Twenty20* |

During a break, the overlay counts down from 20 seconds and closes automatically. In strict mode, all input is blocked — pressing Escape three times within 5 seconds will force-dismiss the overlay as an emergency escape.

<p align="center">
  <img src="assets/screenshot-tray.png" alt="Twenty20 menu bar tray" width="300" />
</p>

---

## Configuration

Settings are available via the native Settings window (menu bar → *Settings…*) or by editing the config file directly:

```
~/.config/twenty20/config.toml
```

| Setting | Default | Description |
|---------|---------|-------------|
| `work_interval_minutes` | `20` | Minutes between breaks (1–60) |
| `break_duration_seconds` | `20` | Break length in seconds (5–60) |
| `strict_mode` | `false` | Block all input during breaks |
| `overlay_theme` | `"dark"` | `"dark"` \| `"light"` \| `"nature"` |
| `sound` | `"off"` | `"off"` \| `"chime"` \| `"whitenoise"` |
| `pre_warning_seconds` | `60` | Notification lead time before break (0 to disable) |
| `meeting_detection` | `true` | Auto-pause during video calls |
| `launch_at_login` | `true` | Start automatically on login |

---

## Building from Source

**Prerequisites:** [Rust](https://rustup.rs/) (1.77.2+), [Node.js](https://nodejs.org/) (18+), Xcode Command Line Tools

```bash
# Clone the repository
git clone https://github.com/ilmimris/Twenty20.git
cd Twenty20

# Install frontend dependencies
npm install

# Start in development mode (hot-reload)
npm run tauri dev

# Build a release DMG
npm run tauri build
```

**Quality checks** (required before submitting a PR):

```bash
cd src-tauri
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cd ..
npm run build
```

---

## Contributing

Contributions are welcome. Please open an issue first to discuss any significant change.

1. Fork the repository and create a feature branch
2. Make your changes and run the quality checks above
3. Open a pull request — CI will verify formatting and linting automatically

---

## License

MIT © [Twenty20](https://github.com/ilmimris/Twenty20)
