mod audio;
mod commands;
mod config;
mod meeting;
mod overlay;
mod settings_window;
mod sleep_watch;
mod strict_mode;
mod timer;
mod tray;

use commands::AppState;
use config::AppConfig;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use timer::SharedTimerState;

/// Lock a Mutex, recovering from a poisoned state gracefully.
macro_rules! lock {
    ($m:expr) => {
        $m.lock().unwrap_or_else(|e| e.into_inner())
    };
}

/// Initializes logging, application state, and runs the Tauri application.
///
/// This function loads the application configuration, restores or creates the persistent timer
/// state, registers application-wide state and plugins (autostart and notifications), wires the
/// command invoke handlers, sets up the system tray, spawns the background timer loop, and starts
/// the Tauri event loop that runs the Twenty20 application.
///
/// # Examples
///
/// ```no_run
/// // Starts the Twenty20 Tauri application; this call does not return until the app exits.
/// twenty20_lib::run();
/// ```
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = AppConfig::load();
    let timer_state = Arc::new(Mutex::new(timer::restore_or_create(&config)));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_notification::init())
        .manage(AppState {
            timer: Arc::clone(&timer_state),
            config: Mutex::new(config),
            tray_menu: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overlay_config,
            commands::force_skip_break,
            commands::test_sound,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Build the system tray.
            tray::setup_tray(app)?;

            // Wire sleep/wake awareness: bridge IOKit notifications into the timer loop.
            let (sleep_tx, sleep_rx) = tokio::sync::watch::channel::<bool>(false);
            sleep_watch::setup(sleep_tx);

            // Start the main timer loop in a background task using Tauri's async runtime.
            let app_handle = app.handle().clone();
            let timer_ref = Arc::clone(&timer_state);
            tauri::async_runtime::spawn(run_timer_loop(app_handle, timer_ref, sleep_rx));

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // Only prevent exit when it was triggered by a window close (no exit code).
            // Explicit app.exit(0) calls (e.g. from the quit menu) carry a code and must proceed.
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// Runs the main timer loop that drives work/break countdowns, emits UI events, manages overlays and strict input suppression, and polls for meetings.
///
/// The loop ticks once per second and:
/// - Decrements the work timer and emits `timer:tick` events for UI updates.
/// - Sends a pre-break notification when configured lead time is reached.
/// - Transitions to a break phase when the work timer reaches zero, opens overlays, enables strict mode if configured, emits `break:start`, counts down the break, then emits `break:end` and resets the work timer.
/// - Detects meetings periodically and pauses/resumes the timer with a `Meeting` pause reason; if a meeting starts during a break, it will close overlays and reset the break state.
/// - Handles manual pauses with an optional auto-resume timeout.
/// - Persists timer state after updates.
///
/// Note: This function runs indefinitely until the application exits. It uses the managed `AppState` config and the shared `SharedTimerState` to drive behavior and emits events on the provided `app` handle.
///
/// # Examples
///
/// ```no_run
/// # use tokio::spawn;
/// # // `app` and `timer` are provided by the application environment.
/// // spawn(async move { run_timer_loop(app, timer).await });
/// ```
async fn run_timer_loop(
    app: tauri::AppHandle,
    timer: SharedTimerState,
    sleep_rx: tokio::sync::watch::Receiver<bool>,
) {
    use std::time::Duration;
    use tokio::time::sleep;

    let mut meeting_poll_counter = 0u32;
    // Track the break phase locally (not in shared state to avoid extra locking).
    let mut break_active = false;
    let mut break_seconds_left: u32 = 0;
    let mut notified_pre_warning = false;
    // Throttle disk persistence: only write every 30 ticks (≈ 30 s).
    let mut persist_counter: u32 = 0;
    // Track sleep state to detect transitions.
    let mut was_sleeping = false;
    tray::update_icon(&app, tray::TrayIconState::Open);

    loop {
        sleep(Duration::from_secs(1)).await;

        // --- Sleep/wake awareness (checked at the top of every tick) ---
        let is_sleeping = *sleep_rx.borrow();

        if !was_sleeping && is_sleeping {
            // Transition: awake → sleeping.
            overlay::close_overlays(&app);
            strict_mode::disable_strict_input_suppression();
            break_active = false;
            notified_pre_warning = false;
            log::info!("System sleeping — timer loop suspended");
            was_sleeping = true;
            continue;
        }

        if was_sleeping && !is_sleeping {
            // Transition: sleeping → awake — reset work timer to a fresh cycle.
            {
                let mut ts = lock!(timer);
                ts.seconds_remaining = ts.work_interval_seconds;
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
            log::info!("System woke — timer reset to full cycle");
            was_sleeping = false;
            continue;
        }

        if is_sleeping {
            // Mid-sleep tick (was_sleeping == true, is_sleeping == true).
            continue;
        }

        let (config_interval, config_break_dur, is_strict, meeting_detection, pre_warning_secs) = {
            let app_state = app.state::<AppState>();
            let cfg = lock!(app_state.config);
            (
                cfg.work_interval_minutes * 60,
                cfg.break_duration_seconds,
                cfg.strict_mode,
                cfg.meeting_detection,
                cfg.pre_warning_seconds,
            )
        };

        // --- Meeting detection (every 30 seconds, offloaded to a blocking thread) ---
        meeting_poll_counter += 1;
        if meeting_detection && meeting_poll_counter >= 30 {
            meeting_poll_counter = 0;

            let meeting_now = tokio::task::spawn_blocking(meeting::is_meeting_active)
                .await
                .unwrap_or(false);

            let currently_meeting_paused = {
                let ts = lock!(timer);
                matches!(ts.pause_reason, Some(timer::PauseReason::Meeting))
            };

            if meeting_now && !currently_meeting_paused {
                log::info!("Meeting detected — pausing timer");
                if break_active {
                    overlay::close_overlays(&app);
                    strict_mode::disable_strict_input_suppression();
                    break_active = false;
                    let mut ts = lock!(timer);
                    ts.seconds_remaining = config_interval;
                    ts.is_paused = true;
                    ts.pause_reason = Some(timer::PauseReason::Meeting);
                } else {
                    let mut ts = lock!(timer);
                    ts.is_paused = true;
                    ts.pause_reason = Some(timer::PauseReason::Meeting);
                }
            } else if !meeting_now && currently_meeting_paused {
                log::info!("Meeting ended — resuming timer");
                let mut ts = lock!(timer);
                ts.is_paused = false;
                ts.pause_reason = None;
            }
        }

        // --- Break countdown phase ---
        if break_active {
            // Decrement first, then check for completion (fixes off-by-one so the
            // break lasts exactly config_break_dur seconds).
            break_seconds_left = break_seconds_left.saturating_sub(1);
            if break_seconds_left == 0 {
                break_active = false;
                notified_pre_warning = false;
                overlay::close_overlays(&app);
                strict_mode::disable_strict_input_suppression();
                let _ = app.emit("break:end", serde_json::json!({ "force_skipped": false }));
                let mut ts = lock!(timer);
                ts.seconds_remaining = config_interval;
                ts.is_paused = false;
                ts.pause_reason = None;
                timer::persist_state(&ts);
                log::info!("Break complete — restarting work timer");
                tray::update_icon(&app, tray::TrayIconState::Open);
            } else {
                overlay::emit_break_tick(&app, break_seconds_left);
            }
            continue;
        }

        // --- Work timer countdown ---
        let paused = lock!(timer).is_paused;

        if paused {
            // Handle manual pause auto-resume.
            {
                let mut ts = lock!(timer);
                match ts.manual_pause_seconds_remaining {
                    Some(0) => {
                        ts.manual_pause_seconds_remaining = None;
                        if matches!(ts.pause_reason, Some(timer::PauseReason::Manual)) {
                            ts.is_paused = false;
                            ts.pause_reason = None;
                        }
                    }
                    Some(ref mut r) => {
                        *r -= 1;
                    }
                    None => {}
                }
            }

            // Emit tick so the tray popover keeps updating while paused.
            let ts = lock!(timer);
            let _ = app.emit(
                "timer:tick",
                serde_json::json!({
                    "seconds_remaining": ts.seconds_remaining,
                    "is_paused": ts.is_paused,
                    "pause_reason": ts.pause_reason,
                }),
            );

            // Update native tray menu
            let (is_strict, _) = {
                let app_state = app.state::<AppState>();
                let cfg = lock!(app_state.config);
                (cfg.strict_mode, cfg.break_duration_seconds)
            };
            update_tray_menu(&app, ts.seconds_remaining, true, is_strict);

            maybe_persist(&timer, &mut persist_counter);
            continue;
        }

        // Tick the work timer.
        let seconds_remaining = {
            let mut ts = lock!(timer);
            if ts.seconds_remaining > 0 {
                ts.seconds_remaining -= 1;
            }
            ts.seconds_remaining
        };

        // Pre-break notification.
        if !notified_pre_warning && pre_warning_secs > 0 && seconds_remaining == pre_warning_secs {
            notified_pre_warning = true;
            send_pre_break_notification(&app, pre_warning_secs);
            tray::update_icon(&app, tray::TrayIconState::Blink);
        }

        // Emit tick.
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

        // Update native tray
        update_tray_menu(&app, seconds_remaining, false, is_strict);

        maybe_persist(&timer, &mut persist_counter);

        // Trigger break.
        if seconds_remaining == 0 {
            log::info!("Break time! Opening overlays.");
            break_active = true;
            break_seconds_left = config_break_dur;

            if is_strict {
                strict_mode::enable_strict_input_suppression();
            }

            overlay::open_overlays(&app, config_break_dur, is_strict);
            audio::play_break_sound(&app);
            tray::update_icon(&app, tray::TrayIconState::Rest);
            let _ = app.emit(
                "break:start",
                serde_json::json!({ "duration": config_break_dur }),
            );
        }
    }
}

/// Persist timer state at most once every 30 seconds.
fn maybe_persist(timer: &SharedTimerState, counter: &mut u32) {
    *counter += 1;
    if *counter >= 30 {
        *counter = 0;
        let ts = lock!(timer);
        timer::persist_state(&ts);
    }
}

fn send_pre_break_notification(app: &tauri::AppHandle, lead_seconds: u32) {
    let minutes = lead_seconds / 60;
    let secs = lead_seconds % 60;

    let label = match (minutes, secs) {
        (m, 0) if m > 0 => format!("{m} minute{}", if m == 1 { "" } else { "s" }),
        (0, s) => format!("{s} second{}", if s == 1 { "" } else { "s" }),
        (m, s) => format!(
            "{m} minute{} {} second{}",
            if m == 1 { "" } else { "s" },
            s,
            if s == 1 { "" } else { "s" }
        ),
    };

    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("Twenty20")
        .body(format!("Eye break in {label} — get ready to look away"))
        .show();
    log::info!("Pre-break notification: break in {label}");
}

/// Updates the system tray menu items (timer label, enabled states) based on current state.
fn update_tray_menu(
    app: &tauri::AppHandle,
    seconds_remaining: u32,
    is_paused: bool,
    is_strict_mode: bool,
) {
    use tauri::menu::MenuItemKind;
    // We access the menu via AppState since TrayIcon doesn't expose it safely in v2
    let state = app.state::<AppState>();

    // We need to lock the mutex to get the menu handle
    // Note: The menu handle itself methods don't require lock on the menu,
    // but we need to get it from the Option inside Mutex.
    // However, we can't hold the lock while updating if updating triggers something that locks?
    // But menu updates are usually safe.
    // Let's scope the lock or clone the menu handle (it's a resource handle, cheap to clone).

    let menu = {
        let guard = match state.tray_menu.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match &*guard {
            Some(m) => m.clone(),
            None => return,
        }
    };

    // Helper to format time
    let label = if is_paused {
        "Paused".to_string()
    } else {
        let m = seconds_remaining / 60;
        let s = seconds_remaining % 60;
        format!("Next break in {:02}:{:02}", m, s)
    };

    let items = match menu.items() {
        Ok(i) => i,
        Err(_) => return,
    };

    for item in items {
        if let MenuItemKind::MenuItem(i) = item {
            let id = i.id();
            if id == "next_break" {
                let _ = i.set_text(&label);
            } else if id == "skip" || id == "pause_30" || id == "pause_1h" {
                // Disable skip/pause if strict mode is on.
                // Also if already paused, "pause" buttons could be disabled or changed to resume,
                // but for now let's just respect strict mode for enabling/disabling.
                // Logic: If strict mode -> disabled.
                // If not strict mode -> enabled.
                // (Simplification: even if paused, we might allow clicking pause to extend, or skip to reset)
                let _ = i.set_enabled(!is_strict_mode);
            }
        }
    }
}
