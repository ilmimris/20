# Sleep/Wake Awareness Design

**Date:** 2026-02-21
**Status:** Approved

## Problem

The Mac screen stays on even after an hour of inactivity when Twenty20 is running. The user wants the Mac to sleep normally when there is no user activity. On wake, the 20-minute work timer should reset to a fresh cycle.

## Approach

Add macOS sleep/wake notification handling via `NSWorkspace` notification observers. Bridge the Cocoa main-thread callbacks into the async Tokio timer loop using a `tokio::sync::watch` channel. No new Cargo dependencies; no changes to `TimerState`.

## Architecture

Three components:

1. **`src-tauri/src/sleep_watch.rs`** (new)
   - Registers block-based observers on `NSWorkspace.sharedWorkspace().notificationCenter()` for:
     - `NSWorkspaceWillSleepNotification` → sends `true` on the watch channel
     - `NSWorkspaceDidWakeNotification` → sends `false` on the watch channel
   - Uses `objc2-foundation` and `objc2-app-kit` (already in Cargo.toml).
   - Returns nothing; the caller owns the watch `Sender`.

2. **`src-tauri/src/lib.rs`** changes
   - Create `tokio::sync::watch::channel::<bool>(false)` in `setup()`.
   - Pass the `Sender` to `sleep_watch::setup()`.
   - Pass the `Receiver` to `run_timer_loop()`.

3. **`run_timer_loop()` changes** (in `lib.rs`)
   - Track `was_sleeping: bool` local state.
   - At the top of each tick, read the watch receiver.
   - **On `false → true` (sleep transition):**
     - Call `overlay::close_overlays(&app)`
     - Call `strict_mode::disable_strict_input_suppression()`
     - Set local `break_active = false`
     - Set `notified_pre_warning = false`
     - Log the sleep event
   - **While sleeping:** `continue` — skip all countdown, meeting detection, and tray updates.
   - **On `true → false` (wake transition):**
     - Lock timer state; set `seconds_remaining = work_interval_seconds`, `is_paused = false`, `pause_reason = None`, `manual_pause_seconds_remaining = None`
     - Call `timer::persist_state(&ts)`
     - Call `tray::update_icon(&app, TrayIconState::Open)`
     - Emit `timer:tick` with updated state
     - Reset `meeting_poll_counter = 0`
     - Log the wake event

## Data Flow

```
macOS kernel
  │  NSWorkspaceWillSleepNotification / NSWorkspaceDidWakeNotification
  ▼
sleep_watch::setup() — NSNotificationCenter block observer (main thread)
  │  watch::Sender<bool>.send(true / false)
  ▼
tokio::sync::watch channel
  │  watch::Receiver<bool> checked at top of each 1-second tick
  ▼
run_timer_loop()
  ├─ on sleep:  close_overlays(), disable_strict_input_suppression(), break_active=false
  └─ on wake:   reset timer to full cycle, persist, update tray, emit tick
```

## Edge Cases

| Scenario | Behaviour |
|----------|-----------|
| Sleep during an active break | Overlays close; `break_active` set to false; no `break:end` event; timer resets on wake |
| Sleep while manually paused | Manual pause cleared on wake; fresh 20-minute cycle starts |
| Sleep during strict mode | CGEventTap disabled on sleep signal; no input suppression on wake |
| Rapid sleep/wake (lid bounce) | Watch channel is level-triggered; timer loop resets on first tick after final stable wake |
| Pre-warning notification during sleep | Not possible — loop skips all logic while `is_sleeping` |

## Files Changed

| File | Change |
|------|--------|
| `src-tauri/src/sleep_watch.rs` | New — NSWorkspace observer setup |
| `src-tauri/src/lib.rs` | Add watch channel creation, wire `sleep_watch::setup()`, pass receiver to timer loop |
| `src-tauri/src/lib.rs` (`run_timer_loop`) | Add sleep/wake transition handling |

## Not Changed

- `src-tauri/src/timer.rs` — `TimerState` struct unchanged
- `src-tauri/Cargo.toml` — no new dependencies
- `src/` (Svelte frontend) — no changes
