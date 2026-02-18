use crate::config::AppConfig;
use crate::strict_mode;
use crate::timer::{PauseReason, SharedTimerState};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tauri::menu::Menu;
use tauri::Wry;

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

// ---------------------------------------------------------------------------
// Timer / tray commands
// ---------------------------------------------------------------------------

/// Get the current timer state as a JSON value.
///
/// The returned `serde_json::Value` is a serialized representation of the shared timer state.
/// If serialization fails, an empty/default JSON value is returned.
///
/// # Examples
///
/// ```no_run
/// use serde_json::Value;
/// // `state` is a Tauri `State<AppState>` provided by the runtime.
/// let json: Value = get_timer_state(state);
/// assert!(json.is_object() || json.is_null());
/// ```
#[tauri::command]
pub fn get_timer_state(state: State<AppState>) -> Value {
    let ts = lock!(state.timer);
    serde_json::to_value(&*ts).unwrap_or_default()
}

/// Skip the current break and reset the timer to the full work interval.
///
/// If strict mode is enabled, the timer is not modified and an error is returned.
///
/// # Returns
///
/// `Ok(())` when the break was skipped and the timer reset; `Err(String)` when strict mode prevents skipping.
///
/// # Examples
///
/// ```no_run
/// // In a real Tauri command context `state` is provided by the runtime.
/// // Here we illustrate expected usage.
/// // let state: tauri::State<AppState> = /* provided by Tauri */ ;
/// // let result = skip_break(state);
/// // assert!(result.is_ok());
/// ```
#[tauri::command]
pub fn skip_break(state: State<AppState>) -> Result<(), String> {
    let mut ts = lock!(state.timer);
    if ts.is_strict_mode {
        return Err("Strict mode: cannot skip breaks".into());
    }
    ts.seconds_remaining = ts.work_interval_seconds;
    ts.is_paused = false;
    ts.pause_reason = None;
    log::info!("Break skipped by user");
    Ok(())
}

/// Pauses the application's timer for the given number of minutes unless strict mode is enabled.
///
/// When successful, the timer is marked paused, the pause reason is set to `Manual`, and the remaining
/// manual pause duration is recorded in seconds. If strict mode is enabled, the function returns an error
/// and makes no state changes.
///
/// # Parameters
///
/// - `minutes`: Number of minutes to pause the timer.
///
/// # Returns
///
/// `Ok(())` on success, `Err` with a descriptive message if pausing is disallowed (e.g., strict mode).
///
/// # Examples
///
/// ```no_run
/// # use tauri::State;
/// # use my_crate::AppState;
/// // `state` would be provided by the Tauri runtime in real usage.
/// // pause_timer(5, state).unwrap();
/// ```
#[tauri::command]
pub fn pause_timer(minutes: u32, state: State<AppState>) -> Result<(), String> {
    let mut ts = lock!(state.timer);
    if ts.is_strict_mode {
        return Err("Strict mode: cannot pause timer".into());
    }
    ts.is_paused = true;
    ts.pause_reason = Some(PauseReason::Manual);
    ts.manual_pause_seconds_remaining = Some(minutes * 60);
    log::info!("Timer paused for {minutes} min by user");
    Ok(())
}

/// Resumes the timer and clears any manual pause metadata.
///
/// Clears the paused flag, the pause reason, and any manual pause remaining seconds.
///
/// # Returns
///
/// `Ok(())` on success, `Err(String)` if the timer is paused for a meeting.
///
/// # Examples
///
/// ```no_run
/// // `state` is provided by Tauri when called as a command.
/// let _ = resume_timer(state);
/// ```
#[tauri::command]
pub fn resume_timer(state: State<AppState>) -> Result<(), String> {
    let mut ts = lock!(state.timer);
    if matches!(ts.pause_reason, Some(PauseReason::Meeting)) {
        return Err("Cannot manually resume — meeting in progress".into());
    }
    ts.is_paused = false;
    ts.pause_reason = None;
    ts.manual_pause_seconds_remaining = None;
    log::info!("Timer resumed by user");
    Ok(())
}

/// Get the current application configuration as a JSON value.
///
/// # Returns
///
/// A `serde_json::Value` containing the serialized `AppConfig`; `null` if serialization fails.
///
/// # Examples
///
/// ```rust,no_run
/// // In a Tauri command context obtain `State<AppState>` and call from the JS side.
/// let json = crate::commands::get_config(state);
/// assert!(json.is_object() || json.is_null());
/// ```
#[tauri::command]
pub fn get_config(state: State<AppState>) -> Value {
    let config = lock!(state.config);
    serde_json::to_value(&*config).unwrap_or_default()
}

/// Saves and applies a validated application configuration.
///
/// Validates and persists the provided `config`, updates the in-memory configuration,
/// applies timer-related settings (strict mode and work interval), and enables or
/// disables launch-at-login according to the saved config.
///
/// # Returns
///
/// `Ok(())` on success, `Err(String)` if validation or saving fails.
///
/// # Examples
///
/// ```
/// // Assume `cfg`, `state`, and `app` are available and correctly typed:
/// // let cfg: AppConfig = ...;
/// // let state: State<AppState> = ...;
/// // let app: AppHandle = ...;
/// save_config(cfg, state, app).expect("failed to save config");
/// ```
#[tauri::command]
pub fn save_config(
    config: AppConfig,
    state: State<AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let validated = config.validated();
    validated.save()?;
    {
        let mut current = lock!(state.config);
        *current = validated.clone();
    }
    {
        let mut ts = lock!(state.timer);
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

/// Returns overlay configuration required by the frontend overlay initializer.
///
/// The returned JSON object contains:
/// - `break_duration`: number of seconds for a break,
/// - `is_primary`: `true` for the primary overlay,
/// - `is_strict_mode`: whether strict mode is enabled.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// let cfg = json!({
///     "break_duration": 300,
///     "is_primary": true,
///     "is_strict_mode": false,
/// });
/// assert!(cfg.get("break_duration").is_some());
/// assert_eq!(cfg["is_primary"], true);
/// ```
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
///
/// This disables strict input suppression, closes any visible overlays, resets the timer's
/// remaining seconds to the configured work interval, clears pause state and reason, and
/// emits a `break:end` event with `{ "force_skipped": true }`.
///
/// # Returns
///
/// `Ok(())` when the force-skip has been processed.
///
/// # Examples
///
/// ```no_run
/// // Called from a Tauri command handler with access to `AppHandle` and `State<AppState>`.
/// // let result = force_skip_break(app_handle, app_state);
/// ```
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

/// Terminate the application with exit code 0.
///
/// # Examples
///
/// ```no_run
/// # use tauri::AppHandle;
/// # fn example(app: AppHandle) {
/// quit_app(app);
/// # }
/// ```
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
