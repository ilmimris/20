/// Strict mode input suppression via CGEventTap (macOS only).
///
/// When strict mode is active and the overlay is visible, a CGEventTap is
/// installed at the HID level to swallow all keyboard and pointer events
/// so they do not pass through to underlying applications.
///
/// CGEventTap requires the Accessibility permission. If denied the overlay
/// still shows (preventing practical use of underlying apps) but OS-level
/// event blocking is disabled; a one-time warning is logged.
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the event tap is currently active.
static EVENT_TAP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Enables strict input suppression so keyboard and pointer events are blocked while an overlay is active.
///
/// This call is idempotent: if strict input suppression is already active it returns immediately.
/// On macOS it attempts to install a system event tap to perform OS-level blocking; if the tap cannot be created
/// (for example due to missing Accessibility permissions) the OS-level blocking may not be enabled even though
/// the active flag was set.
///
/// # Examples
///
/// ```
/// // Activate strict input suppression (safe to call multiple times).
/// enable_strict_input_suppression();
/// enable_strict_input_suppression();
/// ```
pub fn enable_strict_input_suppression() {
    if EVENT_TAP_ACTIVE.swap(true, Ordering::SeqCst) {
        return; // Already active.
    }
    #[cfg(target_os = "macos")]
    tap::install_tap();
}

/// Disables strict input suppression and removes the macOS event tap if it was active.
///
/// This clears the module's global active flag; if suppression was not active this function does nothing.
/// On macOS the installed CGEventTap (used to swallow input events) is removed, stopping OS-level input blocking.
///
/// # Examples
///
/// ```
/// // Safe to call whether or not suppression is currently active.
/// disable_strict_input_suppression();
/// ```
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
    use std::sync::{Mutex, OnceLock};

    // ---------------------------------------------------------------------------
    // Raw pointer wrappers — marked Send/Sync because access is serialised by
    // TAP_STATE's Mutex and all calls happen on the main/run-loop thread.
    // ---------------------------------------------------------------------------

    #[derive(Clone, Copy)]
    struct RawPtr(*mut std::ffi::c_void);
    // Safety: access serialised through TAP_STATE Mutex.
    unsafe impl Send for RawPtr {}
    unsafe impl Sync for RawPtr {}

    struct TapHandles {
        port: RawPtr,
        source: RawPtr,
    }

    static TAP_STATE: OnceLock<Mutex<Option<TapHandles>>> = OnceLock::new();

    fn tap_state() -> &'static Mutex<Option<TapHandles>> {
        TAP_STATE.get_or_init(|| Mutex::new(None))
    }

    // ---------------------------------------------------------------------------
    // Type aliases matching the CoreGraphics / CoreFoundation ABI.
    // ---------------------------------------------------------------------------
    type CGEventTapProxy = *mut std::ffi::c_void;
    type CGEventRef = *mut std::ffi::c_void;
    type CFMachPortRef = *mut std::ffi::c_void;
    type CFRunLoopSourceRef = *mut std::ffi::c_void;
    type CGEventMask = u64;

    // kCGEventMaskForAllEvents
    const KCG_ANY_INPUT_EVENT_TYPE: CGEventMask = !0u64;
    // kCGHIDEventTap = 0, kCGHeadInsertEventTap = 0, kCGEventTapOptionDefault = 0
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

        fn CGEventGetIntegerValueField(event: CGEventRef, field: i32) -> i64;

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
        fn CFRunLoopRemoveSource(
            rl: *mut std::ffi::c_void,
            source: CFRunLoopSourceRef,
            mode: *const std::ffi::c_void,
        );
        fn CFRunLoopGetMain() -> *mut std::ffi::c_void;

        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CFRelease(cf: *const std::ffi::c_void);

        static kCFRunLoopCommonModes: *const std::ffi::c_void;
    }

    /// CGEventTap callback that suppresses input events while the global tap is active.
    ///
    /// When the global `EVENT_TAP_ACTIVE` flag is set, this callback returns `NULL` to drop
    /// the incoming event; otherwise it forwards the original event reference.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // When the tap is active the callback returns NULL, otherwise it returns the same event.
    /// let res = unsafe { tap_callback(std::ptr::null_mut(), 0, std::ptr::null_mut(), std::ptr::null_mut()) };
    /// // `res` will be NULL if `EVENT_TAP_ACTIVE` is true, otherwise it will equal the provided event pointer.
    /// ```
    // kCGEventKeyDown = 10; kCGKeyboardEventKeycode field = 9; kVK_Escape = 53
    const KCG_EVENT_KEY_DOWN: u32 = 10;
    const KCG_KEYBOARD_EVENT_KEYCODE: i32 = 9;
    const KV_K_ESCAPE: i64 = 53;

    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        event_type: u32,
        event: CGEventRef,
        _user_info: *mut std::ffi::c_void,
    ) -> CGEventRef {
        if EVENT_TAP_ACTIVE.load(Ordering::SeqCst) {
            // Let Escape key events through so the triple-Escape escape hatch
            // in the overlay frontend can receive and count them.
            if event_type == KCG_EVENT_KEY_DOWN {
                let keycode =
                    unsafe { CGEventGetIntegerValueField(event, KCG_KEYBOARD_EVENT_KEYCODE) };
                if keycode == KV_K_ESCAPE {
                    return event;
                }
            }
            std::ptr::null_mut() // suppress all other events
        } else {
            event // pass through
        }
    }

    /// Installs a macOS CGEventTap used to suppress keyboard and pointer events while strict input suppression is active.
    ///
    /// If the event tap cannot be created (commonly because Accessibility permission is not granted), this function logs a warning and clears the global event-tap active flag so OS-level input blocking remains disabled while the overlay can still be shown.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Install the event tap to enable system-level input suppression (macOS only).
    /// // The call requires Accessibility permission in System Settings → Privacy & Security → Accessibility.
    /// crate::strict_mode::install_tap();
    /// ```
    pub fn install_tap() {
        let mut guard = tap_state().lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            return; // Already installed.
        }

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
                     Grant access in System Settings → Privacy & Security → Accessibility."
                );
                EVENT_TAP_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }

            let src = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
            if src.is_null() {
                log::warn!("CFMachPortCreateRunLoopSource returned null — releasing port");
                CFRelease(port as *const _);
                EVENT_TAP_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }

            CFRunLoopAddSource(CFRunLoopGetMain(), src, kCFRunLoopCommonModes);
            *guard = Some(TapHandles {
                port: RawPtr(port),
                source: RawPtr(src),
            });
            log::info!("CGEventTap installed for strict mode input suppression");
        }
    }

    /// Removes the installed CG event tap and its run loop source, releasing associated system resources.
    ///
    /// This is safe to call when no tap is installed; in that case the function is a no-op.
    ///
    /// # Examples
    ///
    /// ```
    /// // Remove any previously installed event tap; safe to call even if none exists.
    /// remove_tap();
    /// ```
    pub fn remove_tap() {
        let mut guard = tap_state().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(handles) = guard.take() {
            unsafe {
                // Disable the tap, remove its run-loop source, then release both.
                CGEventTapEnable(handles.port.0, false);
                CFRunLoopRemoveSource(CFRunLoopGetMain(), handles.source.0, kCFRunLoopCommonModes);
                CFRelease(handles.source.0 as *const _);
                CFRelease(handles.port.0 as *const _);
            }
            log::info!("CGEventTap removed");
        }
    }
}

/// Record a user-initiated force-skip and persist it to a local skip log.
///
/// Writes a warning to the application log and appends a timestamped `force-skip` entry
/// to `twenty20/skip_log.txt` inside the user's local data directory. If the local data
/// directory cannot be determined, the file is created relative to the current working directory.
///
/// # Examples
///
/// ```
/// // Trigger a force-skip entry (writes to log and appends to the skip log file).
/// log_force_skip();
/// ```
pub fn log_force_skip() {
    use chrono::Local;
    let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%z").to_string();
    log::warn!("Break force-skipped via 3× Escape at {timestamp}");

    let mut path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("twenty20");
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
