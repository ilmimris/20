use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
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
    let app_handle = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSScreen;
            use objc2_foundation::MainThreadMarker;
            // Now safe because we are on main thread
            let mtm = MainThreadMarker::new().expect("must run on main thread");
            let screens = NSScreen::screens(mtm);
            let screen_count = screens.count();
            for i in 0..screen_count {
                open_overlay_window(&app_handle, i, screen_count, break_duration, strict_mode);
            }
            // Set presentation options once after all windows are built.
            set_presentation_options_for_overlay();
        }

        #[cfg(not(target_os = "macos"))]
        {
            open_overlay_window(&app_handle, 0, 1, break_duration, strict_mode);
        }
    });
}

/// Create and open a fullscreen overlay webview for a specific display index.
///
/// The created window loads `overlay.html` and receives an initialization script that sets
/// `window.__TWENTY20_OVERLAY_CONFIG__` with the fields `breakDuration`, `isPrimary`, and
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

    // Create the window hidden; we configure its level and frame before showing it.
    // Do NOT use .fullscreen(true) — on macOS that triggers the native fullscreen
    // animation which slides the window into its own separate Mission Control Space,
    // letting the user swipe/Ctrl+← away from it or exit it with Cmd+Ctrl+F.
    let win_result = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("overlay.html".into()))
        .always_on_top(true)
        .skip_taskbar(true)
        .decorations(false)
        .transparent(true)
        .visible(false)
        .initialization_script(format!(
            r#"
            window.__TWENTY20_OVERLAY_CONFIG__ = {{
                breakDuration: {break_duration},
                isPrimary: {is_primary},
                isStrictMode: {strict_mode},
            }};
            "#
        ))
        .build();

    match win_result {
        Ok(win) => {
            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::{
                    NSScreen, NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior,
                };
                use objc2_foundation::MainThreadMarker;

                // Safe: open_overlay_window is always called from run_on_main_thread.
                let mtm = MainThreadMarker::new().expect("must be on main thread");
                let screens = NSScreen::screens(mtm);

                if let Ok(raw_ptr) = win.ns_window() {
                    unsafe {
                        let ns_win = &*(raw_ptr as *const NSWindow);

                        // Cover the exact screen frame for this display index.
                        if index < screens.count() {
                            let screen = screens.objectAtIndex(index);
                            let frame = screen.frame();
                            // Place the window over the screen without entering
                            // macOS fullscreen mode (no new Space is created).
                            ns_win.setFrame_display(frame, false);
                        }

                        // Raise to NSScreenSaverWindowLevel (1000) so the overlay
                        // floats above every other window, including fullscreen apps.
                        ns_win.setLevel(NSScreenSaverWindowLevel);

                        // Appear on ALL Spaces — including any fullscreen-app Space
                        // the user might try to switch to.
                        ns_win.setCollectionBehavior(
                            NSWindowCollectionBehavior::CanJoinAllSpaces
                                | NSWindowCollectionBehavior::Stationary
                                | NSWindowCollectionBehavior::FullScreenAuxiliary,
                        );
                    }
                }
            }

            let _ = win.show();
            log::info!("Opened overlay window {label} (primary={is_primary})");
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
    let app_handle = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSScreen;
            use objc2_foundation::MainThreadMarker;
            let mtm = MainThreadMarker::new().expect("must run on main thread");
            let count = NSScreen::screens(mtm).count();
            for i in 0..count {
                let label = format!("overlay_{i}");
                if let Some(win) = app_handle.get_webview_window(&label) {
                    let _ = win.close();
                }
            }
            restore_presentation_options();
        }

        #[cfg(not(target_os = "macos"))]
        {
            if let Some(win) = app_handle.get_webview_window("overlay_0") {
                let _ = win.close();
            }
        }

        log::info!("All overlay windows closed");
    });
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
    if let Err(e) = app.emit(
        "break:tick",
        serde_json::json!({ "seconds_remaining": seconds_remaining }),
    ) {
        log::warn!("Failed to emit break:tick: {e}");
    }
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
    use objc2_foundation::MainThreadMarker;
    let mtm = MainThreadMarker::new().expect("must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setPresentationOptions(
        NSApplicationPresentationOptions::HideMenuBar
            | NSApplicationPresentationOptions::HideDock
            | NSApplicationPresentationOptions::DisableAppleMenu
            // Prevent Cmd+Tab from switching away from the overlay.
            // Requires HideDock to also be set (Apple requirement).
            | NSApplicationPresentationOptions::DisableProcessSwitching,
    );
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
    use objc2_foundation::MainThreadMarker;
    let mtm = MainThreadMarker::new().expect("must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setPresentationOptions(NSApplicationPresentationOptions::Default);
}
