use crate::config::AppConfig;
use crate::strict_mode;
use crate::timer::{PauseReason, SharedTimerState};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

/// Tauri state container.
pub struct AppState {
    pub timer: SharedTimerState,
    pub config: std::sync::Mutex<AppConfig>,
}

// ---------------------------------------------------------------------------
// Timer / tray commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_timer_state(state: State<AppState>) -> Value {
    let ts = state.timer.lock().unwrap();
    serde_json::to_value(&*ts).unwrap_or_default()
}

#[tauri::command]
pub fn skip_break(state: State<AppState>) -> Result<(), String> {
    let mut ts = state.timer.lock().unwrap();
    if ts.is_strict_mode {
        return Err("Strict mode: cannot skip breaks".into());
    }
    ts.seconds_remaining = ts.work_interval_seconds;
    ts.is_paused = false;
    ts.pause_reason = None;
    log::info!("Break skipped by user");
    Ok(())
}

#[tauri::command]
pub fn pause_timer(minutes: u32, state: State<AppState>) -> Result<(), String> {
    let mut ts = state.timer.lock().unwrap();
    if ts.is_strict_mode {
        return Err("Strict mode: cannot pause timer".into());
    }
    ts.is_paused = true;
    ts.pause_reason = Some(PauseReason::Manual);
    ts.manual_pause_seconds_remaining = Some(minutes * 60);
    log::info!("Timer paused for {minutes} min by user");
    Ok(())
}

#[tauri::command]
pub fn resume_timer(state: State<AppState>) -> Result<(), String> {
    let mut ts = state.timer.lock().unwrap();
    if matches!(ts.pause_reason, Some(PauseReason::Meeting)) {
        return Err("Cannot manually resume — meeting in progress".into());
    }
    ts.is_paused = false;
    ts.pause_reason = None;
    ts.manual_pause_seconds_remaining = None;
    log::info!("Timer resumed by user");
    Ok(())
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Value {
    let config = state.config.lock().unwrap();
    serde_json::to_value(&*config).unwrap_or_default()
}

#[tauri::command]
pub fn save_config(config: AppConfig, state: State<AppState>, app: AppHandle) -> Result<(), String> {
    let validated = config.validated();
    validated.save()?;
    {
        let mut current = state.config.lock().unwrap();
        *current = validated.clone();
    }
    {
        let mut ts = state.timer.lock().unwrap();
        ts.is_strict_mode = validated.strict_mode;
        ts.work_interval_seconds = validated.work_interval_minutes * 60;
    }

    // Update launch at login via autostart plugin.
    {
        use tauri_plugin_autostart::ManagerExt;
        if validated.launch_at_login {
            let _ = app.autolaunch().enable();
        } else {
            let _ = app.autolaunch().disable();
        }
    }

    log::info!("Config saved: {:?}", validated);
    Ok(())
}

#[tauri::command]
pub fn get_overlay_config(
    state: State<AppState>,
    // label is passed via JS window.__EYEBREAK_OVERLAY_CONFIG__ init script instead
) -> Value {
    let config = state.config.lock().unwrap();
    serde_json::json!({
        "break_duration": config.break_duration_seconds,
        "is_primary": true,
        "is_strict_mode": config.strict_mode,
    })
}

#[tauri::command]
pub fn force_skip_break(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    strict_mode::log_force_skip();
    strict_mode::disable_strict_input_suppression();
    crate::overlay::close_overlays(&app);
    // Reset timer to full interval after force-skip.
    {
        let mut ts = state.timer.lock().unwrap();
        ts.seconds_remaining = ts.work_interval_seconds;
        ts.is_paused = false;
        ts.pause_reason = None;
    }
    let _ = app.emit("break:end", serde_json::json!({ "force_skipped": true }));
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
