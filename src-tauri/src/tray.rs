use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, WebviewUrl, WebviewWindowBuilder,
};

pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit EyeBreak", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&settings_item, &separator, &quit_item])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("EyeBreak")
        .menu(&menu)
        .menu_on_left_click(false) // left click opens popover
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
