mod commands;
mod config;
mod meeting;
mod overlay;
mod strict_mode;
mod timer;
mod tray;

use commands::AppState;
use config::AppConfig;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use timer::SharedTimerState;

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
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_timer_state,
            commands::skip_break,
            commands::pause_timer,
            commands::resume_timer,
            commands::get_config,
            commands::save_config,
            commands::get_overlay_config,
            commands::force_skip_break,
            commands::quit_app,
        ])
        .setup(move |app| {
            // Build the system tray.
            tray::setup_tray(app)?;

            // Start the main timer loop in a background thread.
            let app_handle = app.handle().clone();
            let timer_ref = Arc::clone(&timer_state);
            tokio::spawn(run_timer_loop(app_handle, timer_ref));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running EyeBreak");
}

/// Main timer loop. Ticks every second, fires overlays, polls meetings.
async fn run_timer_loop(app: tauri::AppHandle, timer: SharedTimerState) {
    use std::time::Duration;
    use tokio::time::sleep;

    let mut meeting_poll_counter = 0u32;

    // Track the break phase
    let mut break_active = false;
    let mut break_seconds_left: u32 = 0;
    let mut notified_pre_warning = false;

    loop {
        sleep(Duration::from_secs(1)).await;

        let (config_interval, config_break_dur, is_strict, meeting_detection, pre_warning_secs) = {
            let app_state = app.state::<AppState>();
            let cfg = app_state.config.lock().unwrap();
            (
                cfg.work_interval_minutes * 60,
                cfg.break_duration_seconds,
                cfg.strict_mode,
                cfg.meeting_detection,
                cfg.pre_warning_seconds,
            )
        };

        // --- Meeting detection (every 30 seconds) ---
        meeting_poll_counter += 1;
        if meeting_detection && meeting_poll_counter >= 30 {
            meeting_poll_counter = 0;
            let meeting_now = meeting::is_meeting_active();

            // Read current pause reason without holding lock during the decisions below.
            let currently_meeting_paused = {
                let ts = timer.lock().unwrap();
                matches!(ts.pause_reason, Some(timer::PauseReason::Meeting))
            };

            if meeting_now && !currently_meeting_paused {
                log::info!("Meeting detected — pausing timer");
                // If overlay is open, close it and reset timer.
                if break_active {
                    overlay::close_overlays(&app);
                    strict_mode::disable_strict_input_suppression();
                    break_active = false;
                    let mut ts = timer.lock().unwrap();
                    ts.seconds_remaining = config_interval;
                    ts.is_paused = true;
                    ts.pause_reason = Some(timer::PauseReason::Meeting);
                } else {
                    let mut ts = timer.lock().unwrap();
                    ts.is_paused = true;
                    ts.pause_reason = Some(timer::PauseReason::Meeting);
                }
            } else if !meeting_now && currently_meeting_paused {
                log::info!("Meeting ended — resuming timer");
                let mut ts = timer.lock().unwrap();
                ts.is_paused = false;
                ts.pause_reason = None;
            }
        }

        // --- Break countdown phase ---
        if break_active {
            if break_seconds_left == 0 {
                // Break complete.
                break_active = false;
                notified_pre_warning = false;
                overlay::close_overlays(&app);
                strict_mode::disable_strict_input_suppression();
                let _ = app.emit("break:end", serde_json::json!({ "force_skipped": false }));
                let mut ts = timer.lock().unwrap();
                ts.seconds_remaining = config_interval;
                ts.is_paused = false;
                ts.pause_reason = None;
                timer::persist_state(&ts);
                log::info!("Break complete — restarting work timer");
                continue;
            }
            break_seconds_left -= 1;
            overlay::emit_break_tick(&app, break_seconds_left);
            continue;
        }

        // --- Work timer countdown ---
        let paused = {
            let ts = timer.lock().unwrap();
            ts.is_paused
        };

        if paused {
            // Handle manual pause timeout: decrement and auto-resume when expired.
            {
                let mut ts = timer.lock().unwrap();
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

            // Emit tick for UI update even while paused.
            let ts = timer.lock().unwrap();
            let _ = app.emit(
                "timer:tick",
                serde_json::json!({
                    "seconds_remaining": ts.seconds_remaining,
                    "is_paused": ts.is_paused,
                    "pause_reason": ts.pause_reason,
                }),
            );
            timer::persist_state(&ts);
            continue;
        }

        // Tick the work timer.
        let seconds_remaining = {
            let mut ts = timer.lock().unwrap();
            if ts.seconds_remaining > 0 {
                ts.seconds_remaining -= 1;
            }
            ts.seconds_remaining
        };

        // Pre-break notification.
        if !notified_pre_warning
            && pre_warning_secs > 0
            && seconds_remaining == pre_warning_secs
        {
            notified_pre_warning = true;
            send_pre_break_notification(&app, pre_warning_secs);
        }

        // Emit tick.
        {
            let ts = timer.lock().unwrap();
            let _ = app.emit(
                "timer:tick",
                serde_json::json!({
                    "seconds_remaining": ts.seconds_remaining,
                    "is_paused": false,
                    "pause_reason": null,
                }),
            );
            timer::persist_state(&ts);
        }

        // Trigger break.
        if seconds_remaining == 0 {
            log::info!("Break time! Opening overlays.");
            break_active = true;
            break_seconds_left = config_break_dur;

            if is_strict {
                strict_mode::enable_strict_input_suppression();
            }

            overlay::open_overlays(&app, config_break_dur, is_strict);
            let _ = app.emit("break:start", serde_json::json!({ "duration": config_break_dur }));
        }
    }
}

fn send_pre_break_notification(app: &tauri::AppHandle, lead_seconds: u32) {
    let minutes = lead_seconds / 60;
    let label = if minutes > 0 {
        format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
    } else {
        format!("{lead_seconds} seconds")
    };

    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("EyeBreak")
        .body(format!("Eye break in {label} — get ready to look away"))
        .show();
    log::info!("Pre-break notification: break in {label}");
}
