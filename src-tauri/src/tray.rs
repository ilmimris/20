use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// Initializes and attaches the application system tray with "Settings…" and "Quit EyeBreak" menu items, a separator, and left-click popover behavior.
///
/// The tray uses the application's default window icon, shows the constructed menu on right-click, opens the settings popover on left-click, and exits the app when "Quit EyeBreak" is selected.
///
/// # Errors
///
/// Returns `Err(tauri::Error)` if building or registering the tray/menu fails.
///
/// # Examples
///
/// ```no_run
/// # use tauri::App;
/// # fn example(mut app: App) -> tauri::Result<()> {
/// setup_tray(&mut app)?;
/// # Ok(())
/// # }
/// ```
pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit EyeBreak", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&settings_item, &separator, &quit_item])?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("tray icon (icons/eye.png)".into()))?
        .clone();

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("EyeBreak")
        .menu(&menu)
        .show_menu_on_left_click(false) // left click opens popover
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => {
                app.exit(0);
            }
            "settings" => {
                open_popover(app);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                open_popover(app);
            }
        })
        .build(app)?;

    Ok(())
}

/// Toggles the settings popover window or creates and shows it if missing.
///
/// If a window with the ID "popover" already exists, hides it when visible or shows and focuses it when hidden.
/// If no such window exists, creates a non-resizable, undecorated, always-on-top popover (280×320) that loads `index.html`,
/// is skipped from the taskbar, and is visible on creation. Errors when building the window are logged.
///
/// # Examples
///
/// ```no_run
/// // Obtain a tauri::AppHandle from your application context and call:
/// // let app_handle: tauri::AppHandle = /* ... */;
/// // open_popover(&app_handle);
/// ```
fn open_popover(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("popover") {
        // Toggle: if already visible, hide it.
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
        }
        return;
    }

    // Create the popover window.
    match WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("index.html".into()))
        .title("EyeBreak")
        .inner_size(280.0, 320.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true)
        .build()
    {
        Ok(win) => {
            let _ = win.set_focus();
        }
        Err(e) => {
            log::error!("Failed to open popover: {e}");
        }
    }
}
