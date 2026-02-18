/// Strict mode input suppression via CGEventTap (macOS only).
///
/// When strict mode is active and the overlay is visible, a CGEventTap is
/// installed at the HID level to swallow all keyboard and pointer events
/// so they do not pass through to underlying applications.
///
/// The 3× Escape emergency escape hatch is detected in the frontend
/// (BreakOverlay.svelte) and calls the `force_skip_break` Tauri command,
/// which removes the tap and closes the overlay.
///
/// CGEventTap requires the Accessibility permission. If denied, the overlay
/// still shows (preventing effective use of underlying apps via the always-
/// on-top fullscreen window) but OS-level event blocking is disabled, and
/// a one-time warning is logged.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the event tap is currently active.
static EVENT_TAP_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn enable_strict_input_suppression() {
    if EVENT_TAP_ACTIVE.swap(true, Ordering::SeqCst) {
        return; // Already active.
    }
    #[cfg(target_os = "macos")]
    tap::install_tap();
}

pub fn disable_strict_input_suppression() {
    if !EVENT_TAP_ACTIVE.swap(false, Ordering::SeqCst) {
        return; // Was not active.
    }
    #[cfg(target_os = "macos")]
    tap::remove_tap();
}

#[cfg(target_os = "macos")]
mod tap {
    use super::EVENT_TAP_ACTIVE;
    use std::sync::atomic::Ordering;

    // CGEventTap constants / types (from CoreGraphics framework).
    type CGEventTapProxy = *mut std::ffi::c_void;
    type CGEventRef = *mut std::ffi::c_void;
    type CFMachPortRef = *mut std::ffi::c_void;
    type CFRunLoopSourceRef = *mut std::ffi::c_void;
    type CGEventMask = u64;

    // kCGEventMaskForAllEvents covers all events.
    const KCG_ANY_INPUT_EVENT_TYPE: u64 = !0u64;

    // Tap locations.
    const KCG_HID_EVENT_TAP: i32 = 0;
    const KCG_HEAD_INSERT_EVENT_TAP: i32 = 0;
    const KCG_DEFAULT_TAP_OPTIONS: i32 = 0;

    extern "C" {
        fn CGEventTapCreate(
            tap: i32,
            place: i32,
            options: i32,
            events_of_interest: CGEventMask,
            callback: extern "C" fn(
                CGEventTapProxy,
                u32,
                CGEventRef,
                *mut std::ffi::c_void,
            ) -> CGEventRef,
            user_info: *mut std::ffi::c_void,
        ) -> CFMachPortRef;

        fn CFMachPortCreateRunLoopSource(
            alloc: *const std::ffi::c_void,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;

        fn CFRunLoopAddSource(
            rl: *mut std::ffi::c_void,
            source: CFRunLoopSourceRef,
            mode: *const std::ffi::c_void,
        );

        fn CFRunLoopGetMain() -> *mut std::ffi::c_void;

        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CFRelease(cf: *const std::ffi::c_void);

        static kCFRunLoopCommonModes: *const std::ffi::c_void;
    }

    // Global tap port (single overlay at a time).
    static mut TAP_PORT: CFMachPortRef = std::ptr::null_mut();
    static mut RUN_LOOP_SRC: CFRunLoopSourceRef = std::ptr::null_mut();

    /// CGEventTap callback — returns NULL to suppress the event.
    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        _event_type: u32,
        _event: CGEventRef,
        _user_info: *mut std::ffi::c_void,
    ) -> CGEventRef {
        if EVENT_TAP_ACTIVE.load(Ordering::SeqCst) {
            // Suppress by returning null.
            std::ptr::null_mut()
        } else {
            _event
        }
    }

    pub fn install_tap() {
        unsafe {
            let port = CGEventTapCreate(
                KCG_HID_EVENT_TAP,
                KCG_HEAD_INSERT_EVENT_TAP,
                KCG_DEFAULT_TAP_OPTIONS,
                KCG_ANY_INPUT_EVENT_TYPE,
                tap_callback,
                std::ptr::null_mut(),
            );
            if port.is_null() {
                log::warn!(
                    "CGEventTapCreate returned null — Accessibility permission likely denied. \
                     Strict mode overlay is shown but OS-level input blocking is disabled. \
                     Grant Accessibility access in System Settings → Privacy & Security → Accessibility."
                );
                EVENT_TAP_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
            let src = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
            CFRunLoopAddSource(CFRunLoopGetMain(), src, kCFRunLoopCommonModes);
            TAP_PORT = port;
            RUN_LOOP_SRC = src;
            log::info!("CGEventTap installed for strict mode input suppression");
        }
    }

    pub fn remove_tap() {
        unsafe {
            if !TAP_PORT.is_null() {
                CGEventTapEnable(TAP_PORT, false);
                CFRelease(TAP_PORT as *const _);
                TAP_PORT = std::ptr::null_mut();
            }
            if !RUN_LOOP_SRC.is_null() {
                CFRelease(RUN_LOOP_SRC as *const _);
                RUN_LOOP_SRC = std::ptr::null_mut();
            }
            log::info!("CGEventTap removed");
        }
    }
}

/// Log a force-skip event with timestamp.
pub fn log_force_skip() {
    use chrono::Local;
    let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%z").to_string();
    log::warn!("Break force-skipped via 3× Escape at {timestamp}");

    // Append to skip log file.
    let mut path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("eyebreak");
    path.push("skip_log.txt");

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let line = format!("{timestamp}\tforce-skip\n");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}
