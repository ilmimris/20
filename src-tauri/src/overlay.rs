use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Clone, Serialize)]
pub struct OverlayConfig {
    pub break_duration: u32,
    pub is_primary: bool,
    pub is_strict_mode: bool,
}

/// Open full-screen overlay windows across displays.
///
/// The primary display (index 0) shows the countdown; other displays show the dim layer.
/// On macOS this opens one overlay per connected screen; on other platforms it opens a single overlay.
///
/// # Arguments
///
/// - `break_duration`: break length in seconds shown by the primary overlay.
/// - `strict_mode`: when `true`, overlays run in strict mode (affects overlay behavior).
///
/// # Examples
///
/// ```no_run
/// // `app` is an instance of `tauri::AppHandle` available in your runtime.
/// let app: &tauri::AppHandle = unimplemented!();
/// open_overlays(app, 300, true);
/// ```
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

/// Create and open a fullscreen overlay webview for a specific display index.
///
/// The created window loads `overlay.html` and receives an initialization script that sets
/// `window.__EYEBREAK_OVERLAY_CONFIG__` with the fields `breakDuration`, `isPrimary`, and
/// `isStrictMode`.
///
/// `index` selects which display the overlay targets; an overlay with `index == 0` is treated
/// as the primary overlay. On macOS, successful creation adjusts presentation options to hide
/// the menu bar and Dock.
///
/// # Examples
///
/// ```no_run
/// // assuming `app` is a `tauri::AppHandle`
/// open_overlay_window(&app, 0, 1, 300, true);
/// ```
///
/// # Parameters
///
/// - `index`: Zero-based display index identifying this overlay (0 is primary).
/// - `break_duration`: Break duration in seconds injected into the overlay config.
/// - `strict_mode`: Whether the overlay should run in strict mode.
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

/// Closes all overlay windows named `overlay_0` through `overlay_7` and restores macOS presentation options when applicable.
///
/// On macOS this also calls the helper to restore presentation options (menu bar and Dock visibility) after closing overlays.
///
/// # Examples
///
/// ```no_run
/// // `app` is an `AppHandle` from the Tauri runtime context.
/// close_overlays(&app);
/// ```
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

/// Sends a "break:tick" event to all overlay windows with the remaining break time.
///
/// The emitted payload is a JSON object: `{ "seconds_remaining": <seconds_remaining> }`.
///
/// # Examples
///
/// ```
/// // Assuming `app` is a valid `AppHandle`:
/// emit_break_tick(&app, 10);
/// ```
pub fn emit_break_tick(app: &AppHandle, seconds_remaining: u32) {
    let _ = app.emit(
        "break:tick",
        serde_json::json!({ "seconds_remaining": seconds_remaining }),
    );
}

/// Set macOS presentation options to hide the menu bar, hide the Dock, and disable the Apple menu.
///
/// # Examples
///
/// ```no_run
/// #[cfg(target_os = "macos")]
/// fn example() {
///     // Apply presentation options appropriate for a fullscreen overlay.
///     set_presentation_options_for_overlay();
/// }
/// ```
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

/// Restore macOS presentation options to the system default.
///
/// This resets any presentation options previously applied to hide the menu bar,
/// Dock, or other system UI elements so the application returns to normal presentation mode.
///
/// # Examples
///
/// ```
/// // On macOS this will restore the default presentation options for the app.
/// restore_presentation_options();
/// ```
#[cfg(target_os = "macos")]
fn restore_presentation_options() {
    use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions};
    unsafe {
        let app = NSApplication::sharedApplication();
        app.setPresentationOptions(NSApplicationPresentationOptions::NSApplicationPresentationDefault);
    }
}