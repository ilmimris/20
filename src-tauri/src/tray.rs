use crate::commands::AppState;
use crate::timer::PauseReason;
use tauri::{
    Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager,
};

/// Lock a Mutex, recovering from a poisoned state gracefully.
macro_rules! lock {
    ($m:expr) => {
        $m.lock().unwrap_or_else(|e| e.into_inner())
    };
}

/// Initializes and attaches the application system tray with native menu items.
///
/// The menu includes:
/// - "Next break in..." (disabled, used for status)
/// - "Skip next break"
/// - "Pause for 30 min"
/// - "Pause for 1 hr"
/// - Separator
/// - "Settings…"
/// - "Quit Twenty20"
pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let next_break_item =
        MenuItem::with_id(app, "next_break", "Next break in...", false, None::<&str>)?;
    let skip_item = MenuItem::with_id(app, "skip", "Skip next break", true, None::<&str>)?;
    let pause_30_item = MenuItem::with_id(app, "pause_30", "Pause for 30 min", true, None::<&str>)?;
    let pause_1h_item = MenuItem::with_id(app, "pause_1h", "Pause for 1 hr", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Twenty20", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &next_break_item,
            &skip_item,
            &pause_30_item,
            &pause_1h_item,
            &separator,
            &settings_item,
            &quit_item,
        ],
    )?;

    {
        let state = app.state::<AppState>();
        *state.tray_menu.lock().unwrap() = Some(menu.clone());
    }

    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("tray icon (icons/eye.png)".into()))?
        .clone();

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Twenty20")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => {
                app.exit(0);
            }
            "settings" => {
                open_settings(app);
            }
            "skip" => {
                let state = app.state::<AppState>();
                let mut ts = lock!(state.timer);
                if !ts.is_strict_mode {
                    ts.seconds_remaining = ts.work_interval_seconds;
                    ts.is_paused = false;
                    ts.pause_reason = None;
                    log::info!("Break skipped via tray");
                }
            }
            "pause_30" => {
                let state = app.state::<AppState>();
                let mut ts = lock!(state.timer);
                if !ts.is_strict_mode {
                    ts.is_paused = true;
                    ts.pause_reason = Some(PauseReason::Manual);
                    ts.manual_pause_seconds_remaining = Some(30 * 60);
                    log::info!("Timer paused for 30 min via tray");
                }
            }
            "pause_1h" => {
                let state = app.state::<AppState>();
                let mut ts = lock!(state.timer);
                if !ts.is_strict_mode {
                    ts.is_paused = true;
                    ts.pause_reason = Some(PauseReason::Manual);
                    ts.manual_pause_seconds_remaining = Some(60 * 60);
                    log::info!("Timer paused for 1 hr via tray");
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                // Native menu handles this automatically with show_menu_on_left_click(true)
                // But we keep this listener if we need custom logic later.
            }
        })
        .build(app)?;

    Ok(())
}

/// Opens the settings window, creating it if it doesn't exist.
fn open_settings(app: &tauri::AppHandle) {
    crate::settings_window::show_settings(app);
}

#[derive(Debug, Clone, Copy)]
pub enum TrayIconState {
    Open,
    Blink,
    Rest,
}

pub fn update_icon(app: &tauri::AppHandle, state: TrayIconState) {
    let icon_name = match state {
        TrayIconState::Open => "eye_open.svg",
        TrayIconState::Blink => "eye_blink.svg",
        TrayIconState::Rest => "eye_rest.svg",
    };

    // Load from relative path to src-tauri
    let icon_path = std::path::Path::new("icons").join(icon_name);
    
    // In a real build, these assets are bundled. For now, we try to load them.
    // If loading fails, we log and skip to prevent crash.
    match Image::from_path(icon_path) {
        Ok(img) => {
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_icon(Some(img));
            }
        }
        Err(e) => log::warn!("Failed to load tray icon {}: {}", icon_name, e),
    }
}
