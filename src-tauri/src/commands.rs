use crate::config::AppConfig;
use crate::strict_mode;
use crate::timer::SharedTimerState;
use serde_json::Value;
use tauri::menu::Menu;
use tauri::Wry;
use tauri::{AppHandle, Emitter, State};

/// Tauri state container.
pub struct AppState {
    pub timer: SharedTimerState,
    pub config: std::sync::Mutex<AppConfig>,
    pub tray_menu: std::sync::Mutex<Option<Menu<Wry>>>,
}

/// Lock a Mutex, recovering from poisoning gracefully.
macro_rules! lock {
    ($m:expr) => {
        $m.lock().unwrap_or_else(|e| e.into_inner())
    };
}

/// Returns overlay configuration required by the frontend overlay initializer.
#[tauri::command]
pub fn get_overlay_config(label: Option<String>, state: State<AppState>) -> Value {
    let config = lock!(state.config);
    // The primary overlay window is always labelled "overlay_0".
    let is_primary = label.as_deref() == Some("overlay_0");
    serde_json::json!({
        "break_duration": config.break_duration_seconds,
        "is_primary": is_primary,
        "is_strict_mode": config.strict_mode,
    })
}

/// Forces the current break to end immediately and resets the timer to a full work interval.
#[tauri::command]
pub fn force_skip_break(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    strict_mode::log_force_skip();
    strict_mode::disable_strict_input_suppression();
    crate::overlay::close_overlays(&app);
    // Reset timer to full interval after force-skip.
    {
        let mut ts = lock!(state.timer);
        ts.seconds_remaining = ts.work_interval_seconds;
        ts.is_paused = false;
        ts.pause_reason = None;
    }
    let _ = app.emit("break:end", serde_json::json!({ "force_skipped": true }));
    Ok(())
}

#[tauri::command]
pub fn test_sound(app: AppHandle) -> Result<(), String> {
    log::info!("Manual sound test triggered");
    crate::audio::play_break_sound(&app);
    Ok(())
}
