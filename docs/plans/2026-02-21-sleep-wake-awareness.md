# Sleep/Wake Awareness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Detect macOS sleep/wake events and ensure Twenty20 never prevents system sleep; on wake, reset the 20-minute timer to a fresh cycle.

**Architecture:** Register block-based `NSWorkspaceWillSleepNotification` / `NSWorkspaceDidWakeNotification` observers in a new `sleep_watch.rs` module. Bridge the Cocoa main-thread callbacks to the async Tokio timer loop via a `tokio::sync::watch` channel. The timer loop skips all countdown logic while sleeping and resets state on wake.

**Tech Stack:** Rust, Tauri 2, objc2 0.6, objc2-foundation 0.3, objc2-app-kit 0.3, block2 0.6, tokio::sync::watch

---

## Context You Must Read First

Before touching any code, read these files in full so you understand what you're modifying:

- `src-tauri/src/lib.rs` — contains `run()` (app setup) and `run_timer_loop()` (the async 1-second tick loop)
- `src-tauri/src/timer.rs` — `TimerState` struct, `SharedTimerState` type alias
- `src-tauri/src/overlay.rs` — `close_overlays()` dispatches to main thread via `run_on_main_thread`
- `src-tauri/src/strict_mode.rs` — example of raw FFI for macOS; `disable_strict_input_suppression()` is safe to call from any thread
- `src-tauri/src/tray.rs` — `update_icon()` and `TrayIconState`

**Key facts:**
- `run_timer_loop()` is an async Tokio task that ticks once per second
- `break_active` is a **local variable** inside `run_timer_loop` — not in `TimerState`
- `overlay::close_overlays()` and `strict_mode::disable_strict_input_suppression()` are safe to call from any thread
- `tokio::sync::watch` sender is `Send + Sync` — safe to call from Cocoa main-thread callbacks
- `objc2-foundation` currently only has `NSArray` and `NSString` features — we must add more
- The `setup()` callback in `lib.rs` runs on the main thread — safe for NSWorkspace calls

---

## Task 1: Add Cargo.toml Features

**Files:**
- Modify: `src-tauri/Cargo.toml`

### Step 1: Open and read Cargo.toml

Read `src-tauri/Cargo.toml` and confirm the current `objc2-foundation` features section.

Expected current state:
```toml
objc2-foundation = { version = "0.3", features = [
  "NSArray",
  "NSString",
] }
```

### Step 2: Add NSNotificationCenter and NSNotification features

In `src-tauri/Cargo.toml`, update the `objc2-foundation` features and add `block2`:

```toml
objc2-foundation = { version = "0.3", features = [
  "NSArray",
  "NSString",
  "NSNotificationCenter",
  "NSNotification",
] }
```

Also add `block2` to the macOS-specific dependencies section (right after the `objc2-foundation` entry):

```toml
block2 = "0.6"
```

The full macOS deps block should then look like:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = [
  "NSWorkspace",
  "NSRunningApplication",
  "NSScreen",
  "NSApplication",
  "NSWindow",
  "NSPanel",
  "NSButton",
  "NSTextField",
  "NSSwitch",
  "NSPopUpButton",
  "NSMenu",
  "NSMenuItem",
  "NSSound",
  "NSLayoutConstraint",
  "NSStackView",
  "NSColor",
  "NSFont",
  "NSAlert",
  "NSControl",
  "NSView",
  "NSText",
] }
objc2-foundation = { version = "0.3", features = [
  "NSArray",
  "NSString",
  "NSNotificationCenter",
  "NSNotification",
] }
block2 = "0.6"
```

### Step 3: Verify it compiles

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

Expected: no errors. If `block2 = "0.6"` causes a version conflict, check what version `objc2` depends on:

```bash
cargo tree -p block2 2>&1 | head -10
```

Use the version shown there.

### Step 4: Commit

```bash
cd ..
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add NSNotificationCenter, NSNotification, block2 deps for sleep/wake support"
```

---

## Task 2: Create `sleep_watch.rs`

**Files:**
- Create: `src-tauri/src/sleep_watch.rs`

This module registers macOS sleep/wake notification observers and signals the timer loop via a `tokio::sync::watch` channel.

### Step 1: Create the file

Create `src-tauri/src/sleep_watch.rs` with this content:

```rust
//! macOS sleep/wake notification observer.
//!
//! Registers `NSWorkspaceWillSleepNotification` and `NSWorkspaceDidWakeNotification`
//! observers on `NSWorkspace.sharedWorkspace().notificationCenter()`.
//! Signals the timer loop via a `tokio::sync::watch` channel:
//!   - `true`  → system is going to sleep
//!   - `false` → system just woke up

#[cfg(target_os = "macos")]
pub fn setup(sleep_tx: tokio::sync::watch::Sender<bool>) {
    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{MainThreadMarker, NSNotificationCenter, NSString};
    use std::ptr::NonNull;

    // Safety: setup() is called from the Tauri `setup` callback which runs on the main thread.
    let mtm = MainThreadMarker::new().expect("sleep_watch::setup must be called on the main thread");

    let nc: objc2::rc::Retained<NSNotificationCenter> = unsafe {
        NSWorkspace::sharedWorkspace(mtm).notificationCenter()
    };

    // --- Will Sleep ---
    let tx_sleep = sleep_tx.clone();
    let sleep_block = RcBlock::new(move |_notif: NonNull<objc2_foundation::NSNotification>| {
        log::info!("System going to sleep — pausing Twenty20");
        let _ = tx_sleep.send(true);
    });

    // --- Did Wake ---
    let tx_wake = sleep_tx;
    let wake_block = RcBlock::new(move |_notif: NonNull<objc2_foundation::NSNotification>| {
        log::info!("System woke from sleep — resetting Twenty20 timer");
        let _ = tx_wake.send(false);
    });

    unsafe {
        let will_sleep = NSString::from_str("NSWorkspaceWillSleepNotification");
        let did_wake = NSString::from_str("NSWorkspaceDidWakeNotification");

        // Observers must stay alive for the entire app lifetime — leak intentionally.
        let sleep_obs = nc.addObserverForName_object_queue_usingBlock(
            Some(&will_sleep),
            None,
            None,
            &sleep_block,
        );
        let wake_obs = nc.addObserverForName_object_queue_usingBlock(
            Some(&did_wake),
            None,
            None,
            &wake_block,
        );

        // Prevent drop — these observers must live as long as the app runs.
        std::mem::forget(sleep_obs);
        std::mem::forget(wake_obs);
    }

    log::info!("sleep_watch: NSWorkspace sleep/wake observers registered");
}

#[cfg(not(target_os = "macos"))]
pub fn setup(_sleep_tx: tokio::sync::watch::Sender<bool>) {
    // No-op on non-macOS platforms.
}
```

### Step 2: Verify it compiles

```bash
cd src-tauri && cargo check 2>&1
```

**If you see errors about `RcBlock::new` signature or block parameter types:**

The exact block type signature depends on objc2-foundation's generated bindings. Check the actual method signature:

```bash
cd src-tauri && cargo doc --open 2>/dev/null; grep -r "addObserverForName" ~/.cargo/registry/src/*/objc2-foundation-*/src/ 2>/dev/null | head -20
```

Common variations to try if the above doesn't compile:
- Parameter might be `&NSNotification` instead of `NonNull<NSNotification>`
- Block might need to be `&Block<dyn Fn(NonNull<NSNotification>)>` instead of `RcBlock`
- `nc.notificationCenter()` might return by value not reference — adjust accordingly

### Step 3: Register the module in `lib.rs`

Add `mod sleep_watch;` to `src-tauri/src/lib.rs` near the top where other modules are declared:

```rust
mod sleep_watch;
```

### Step 4: Check compilation again

```bash
cd src-tauri && cargo check 2>&1
```

Expected: no errors.

### Step 5: Commit

```bash
cd ..
git add src-tauri/src/sleep_watch.rs src-tauri/src/lib.rs
git commit -m "feat: add sleep_watch module with NSWorkspace sleep/wake observers"
```

---

## Task 3: Wire the Watch Channel in `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs`

The `run()` function sets up the channel and passes it to both `sleep_watch::setup()` and `run_timer_loop()`.

### Step 1: Read `lib.rs` fully before editing

Locate the `setup` closure in `lib.rs`. It currently ends with:
```rust
tauri::async_runtime::spawn(run_timer_loop(app_handle, timer_ref));
```

### Step 2: Add the watch channel and wire it up

Replace this block in the `setup` closure:

```rust
// Start the main timer loop in a background task using Tauri's async runtime.
let app_handle = app.handle().clone();
let timer_ref = Arc::clone(&timer_state);
tauri::async_runtime::spawn(run_timer_loop(app_handle, timer_ref));
```

With:

```rust
// Create a watch channel to signal sleep/wake transitions to the timer loop.
// false = awake (initial state), true = sleeping.
let (sleep_tx, sleep_rx) = tokio::sync::watch::channel(false);

// Register macOS sleep/wake notification observers.
sleep_watch::setup(sleep_tx);

// Start the main timer loop in a background task using Tauri's async runtime.
let app_handle = app.handle().clone();
let timer_ref = Arc::clone(&timer_state);
tauri::async_runtime::spawn(run_timer_loop(app_handle, timer_ref, sleep_rx));
```

### Step 3: Update `run_timer_loop` signature

Find the function signature:
```rust
async fn run_timer_loop(app: tauri::AppHandle, timer: SharedTimerState) {
```

Change it to:
```rust
async fn run_timer_loop(
    app: tauri::AppHandle,
    timer: SharedTimerState,
    mut sleep_rx: tokio::sync::watch::Receiver<bool>,
) {
```

### Step 4: Verify it compiles

```bash
cd src-tauri && cargo check 2>&1
```

Expected: errors about `run_timer_loop` body not using `sleep_rx` yet — that's fine, continue.

### Step 5: Commit (partial — compiles with warnings)

```bash
cd ..
git add src-tauri/src/lib.rs
git commit -m "feat: wire tokio watch channel for sleep/wake signalling"
```

---

## Task 4: Handle Sleep/Wake in the Timer Loop

**Files:**
- Modify: `src-tauri/src/lib.rs` — `run_timer_loop` function body

### Step 1: Read the full `run_timer_loop` body

Understand all local variables at the top of the loop:
- `meeting_poll_counter: u32`
- `break_active: bool`
- `break_seconds_left: u32`
- `notified_pre_warning: bool`
- `persist_counter: u32`

### Step 2: Add sleep tracking variable

After the existing local variable declarations at the top of `run_timer_loop` (after `tray::update_icon(&app, tray::TrayIconState::Open);`), add:

```rust
let mut is_sleeping = false;
```

### Step 3: Add sleep/wake handling at the top of the loop

Inside the `loop { ... }` body, **after** `sleep(Duration::from_secs(1)).await;` and **before** the config read, insert this block:

```rust
// --- Sleep/wake handling ---
let sleeping_now = *sleep_rx.borrow();
if sleeping_now && !is_sleeping {
    // Transition: awake → sleeping
    is_sleeping = true;
    log::info!("Handling sleep: closing overlays and cancelling active break");
    overlay::close_overlays(&app);
    strict_mode::disable_strict_input_suppression();
    break_active = false;
    notified_pre_warning = false;
    meeting_poll_counter = 0;
}
if !sleeping_now && is_sleeping {
    // Transition: sleeping → awake
    is_sleeping = false;
    log::info!("Handling wake: resetting timer to full work interval");
    {
        let app_state = app.state::<AppState>();
        let cfg = lock!(app_state.config);
        let interval = cfg.work_interval_minutes * 60;
        let mut ts = lock!(timer);
        ts.seconds_remaining = interval;
        ts.is_paused = false;
        ts.pause_reason = None;
        ts.manual_pause_seconds_remaining = None;
        timer::persist_state(&ts);
    }
    tray::update_icon(&app, tray::TrayIconState::Open);
    {
        let ts = lock!(timer);
        let _ = app.emit(
            "timer:tick",
            serde_json::json!({
                "seconds_remaining": ts.seconds_remaining,
                "is_paused": false,
                "pause_reason": null,
            }),
        );
    }
    meeting_poll_counter = 0;
}
if is_sleeping {
    continue; // Skip all countdown logic while sleeping
}
```

This block must be placed **before** the config destructuring block that reads `config_interval`, `config_break_dur`, etc.

### Step 4: Verify the final structure

After your edit, the top of `run_timer_loop`'s loop body should look like this (in order):

1. `sleep(Duration::from_secs(1)).await;`
2. Sleep/wake handling block (new)
3. Config read block: `let (config_interval, ...) = { ... };`
4. Meeting detection block
5. Break countdown block
6. Work timer countdown block

### Step 5: Run quality checks

```bash
cd src-tauri
cargo fmt
cargo clippy --all-targets -- -D warnings 2>&1
```

Fix any clippy warnings before continuing.

### Step 6: Build frontend to check for TS errors

```bash
cd ..
npm run build 2>&1
```

Expected: successful build with no TypeScript errors.

### Step 7: Commit

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: handle sleep/wake in timer loop — close overlays on sleep, reset timer on wake"
```

---

## Task 5: Manual Testing

There are no automated tests for this project. Test manually.

### Test 1: Normal timer operation still works

```bash
npm run tauri dev
```

1. App launches → tray icon appears (open eye)
2. Wait for a break to trigger (or temporarily set `work_interval_minutes = 1` in `~/.config/twenty20/config.toml`)
3. Break overlay appears → countdown runs → overlay closes automatically
4. Timer resets to 20 min → tray menu shows "Next break in 20:00"

### Test 2: Sleep → Wake resets timer

1. Run the app
2. Note the current timer value from tray menu (e.g., "Next break in 18:30")
3. Close the MacBook lid (or System Settings → Battery → Sleep Now, or `sudo pmset sleepnow`)
4. Wait ~10 seconds
5. Open the lid
6. Click the tray icon → menu should show "Next break in 20:00" (full reset)
7. Check app logs for: `"Handling sleep: ..."` and `"Handling wake: ..."`

To see logs during dev:
```bash
RUST_LOG=info npm run tauri dev
```

### Test 3: Sleep during active break

1. Set `work_interval_minutes = 1` in `~/.config/twenty20/config.toml`
2. Run the app and wait for the break overlay to appear
3. While the overlay is showing, sleep the Mac
4. Wake the Mac
5. Verify: overlay is gone, timer shows "Next break in 20:00", no crash

### Test 4: Verify Mac can sleep

1. Run the app
2. Leave the Mac idle for the duration of your display sleep setting (e.g., 5 min in System Settings → Battery)
3. Verify the display turns off normally
4. If display still stays on, run `pmset -g assertions` and check for any Twenty20-related assertions

---

## Task 6: Final Quality Checks and Commit

### Step 1: Run all quality checks

```bash
cd src-tauri
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cd ..
npm run build
```

All must pass with no warnings or errors.

### Step 2: Final commit if needed

If any fmt/clippy fixes were needed:

```bash
git add -p
git commit -m "fix: resolve fmt/clippy issues in sleep/wake implementation"
```

---

## Troubleshooting

**`block2::RcBlock` doesn't exist or has different API:**
Check the block2 v0.6 docs: `cargo doc -p block2 --open`. The type may be `block2::Block` or `block2::StackBlock`. Look for the equivalent of "create a heap-allocated block from a Rust closure."

**`addObserverForName_object_queue_usingBlock` doesn't exist:**
The method may not be generated for `NSNotificationCenter` in this version. Alternative: use `NSWorkspace.addObserver_selector_name_object:` with a custom selector. This requires defining an Objective-C class in Rust with `define_class!` macro — more complex but documented in objc2 examples.

**Observers fire but `sleep_rx.borrow()` doesn't see changes:**
Ensure `sleep_tx.send(true/false)` is actually called by adding a `println!` or `log::info!` inside the blocks. If callbacks never fire, check that `nc` is `NSWorkspace.sharedWorkspace().notificationCenter()` (not `NSNotificationCenter.defaultCenter()`).

**`MainThreadMarker::new()` panics:**
`sleep_watch::setup()` must be called from the Tauri `setup` closure, which runs on the main thread. Confirm the call site is inside `.setup(move |app| { ... })`.
