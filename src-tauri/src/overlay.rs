use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Clone, Serialize)]
pub struct OverlayConfig {
    pub break_duration: u32,
    pub is_primary: bool,
    pub is_strict_mode: bool,
}

/// Open full-screen overlay windows on all connected displays.
/// The primary display (index 0) shows the countdown; others show the dim layer.
pub fn open_overlays(app: &AppHandle, break_duration: u32, strict_mode: bool) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSScreen;
        let screens = unsafe { NSScreen::screens() };
        let screen_count = unsafe { screens.count() };
        for i in 0..screen_count {
            open_overlay_window(app, i as usize, screen_count as usize, break_duration, strict_mode);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        open_overlay_window(app, 0, 1, break_duration, strict_mode);
    }
}

fn open_overlay_window(
    app: &AppHandle,
    index: usize,
    _total: usize,
    break_duration: u32,
    strict_mode: bool,
) {
    let label = format!("overlay_{index}");
    // Close existing if any.
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.close();
    }

    let is_primary = index == 0;

    match WebviewWindowBuilder::new(app, &label, WebviewUrl::App("overlay.html".into()))
        .fullscreen(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .decorations(false)
        .transparent(true)
        .visible(true)
        .initialization_script(&format!(
            r#"
            window.__EYEBREAK_OVERLAY_CONFIG__ = {{
                breakDuration: {break_duration},
                isPrimary: {is_primary},
                isStrictMode: {strict_mode},
            }};
            "#
        ))
        .build()
    {
        Ok(_win) => {
            log::info!("Opened overlay window {label} (primary={is_primary})");

            // On macOS, set presentation options to hide menu bar and Dock.
            #[cfg(target_os = "macos")]
            set_presentation_options_for_overlay();
        }
        Err(e) => {
            log::error!("Failed to open overlay window {label}: {e}");
        }
    }
}

/// Close all overlay windows.
pub fn close_overlays(app: &AppHandle) {
    for i in 0..8 {
        let label = format!("overlay_{i}");
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.close();
        }
    }

    #[cfg(target_os = "macos")]
    restore_presentation_options();

    log::info!("All overlay windows closed");
}

/// Emit break:tick events to all overlay windows.
pub fn emit_break_tick(app: &AppHandle, seconds_remaining: u32) {
    let _ = app.emit(
        "break:tick",
        serde_json::json!({ "seconds_remaining": seconds_remaining }),
    );
}

#[cfg(target_os = "macos")]
fn set_presentation_options_for_overlay() {
    use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions};
    unsafe {
        let app = NSApplication::sharedApplication();
        app.setPresentationOptions(
            NSApplicationPresentationOptions::NSApplicationPresentationHideMenuBar
                | NSApplicationPresentationOptions::NSApplicationPresentationHideDock
                | NSApplicationPresentationOptions::NSApplicationPresentationDisableAppleMenu,
        );
    }
}

#[cfg(target_os = "macos")]
fn restore_presentation_options() {
    use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions};
    unsafe {
        let app = NSApplication::sharedApplication();
        app.setPresentationOptions(NSApplicationPresentationOptions::NSApplicationPresentationDefault);
    }
}
