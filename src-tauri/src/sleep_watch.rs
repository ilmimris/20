//! macOS sleep/wake awareness via IOKit power management notifications.
//!
//! Registers a system power callback and bridges the Cocoa main-thread events
//! into the async Tokio timer loop via a `tokio::sync::watch` channel.
//! No new Cargo dependencies — only IOKit and CoreFoundation, which macOS
//! links automatically for Tauri apps.

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use tokio::sync::watch;

    /// IOKit message sent just before the system sleeps.
    /// The callback *must* call `IOAllowPowerChange` or the OS will hang.
    const K_IO_MESSAGE_SYSTEM_WILL_SLEEP: u32 = 0xe000_0280;

    /// IOKit message sent after the system has fully woken.
    const K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON: u32 = 0xe000_0300;

    // Opaque types matching the IOKit / CoreFoundation C ABI on macOS.
    type IONotificationPortRef = *mut c_void;
    type IoObjectT = u32; // mach_port_t
    type IoConnectT = u32; // mach_port_t

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        /// Register for system power state change notifications.
        ///
        /// Returns an `io_connect_t` root port (0 on failure) that must be
        /// passed to `IOAllowPowerChange` on every `kIOMessageSystemWillSleep`
        /// notification.
        fn IORegisterForSystemPower(
            refcon: *mut c_void,
            notify_port: *mut IONotificationPortRef,
            callback: unsafe extern "C" fn(*mut c_void, u32, u32, *mut c_void),
            notifier: *mut IoObjectT,
        ) -> IoConnectT;

        /// Returns a `CFRunLoopSourceRef` for the notification port.
        fn IONotificationPortGetRunLoopSource(notify_port: IONotificationPortRef) -> *mut c_void;

        /// Acknowledge a sleep notification; must be called for
        /// `kIOMessageSystemWillSleep` or the system will hang.
        fn IOAllowPowerChange(root_port: IoConnectT, notif_id: isize);
    }

    extern "C" {
        fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopRun();
        static kCFRunLoopDefaultMode: *const c_void;
    }

    /// Channel sender written once at startup, read from the C callback.
    static SLEEP_SENDER: OnceLock<watch::Sender<bool>> = OnceLock::new();

    /// IOKit root port stored after `IORegisterForSystemPower` succeeds;
    /// used by the callback to call `IOAllowPowerChange`.
    static ROOT_PORT: OnceLock<IoConnectT> = OnceLock::new();

    /// C-compatible IOKit power-state callback.
    ///
    /// Runs on the dedicated `sleep-watch` thread's `CFRunLoop`.
    /// Sends `true` on sleep and `false` on wake via `SLEEP_SENDER`.
    unsafe extern "C" fn power_callback(
        _refcon: *mut c_void,
        _service: u32,
        message_type: u32,
        message_argument: *mut c_void,
    ) {
        match message_type {
            K_IO_MESSAGE_SYSTEM_WILL_SLEEP => {
                log::info!("System will sleep — signalling timer loop");
                if let Some(tx) = SLEEP_SENDER.get() {
                    let _ = tx.send(true);
                }
                // Acknowledge sleep; without this the system hangs for ~30 s.
                if let Some(&root_port) = ROOT_PORT.get() {
                    IOAllowPowerChange(root_port, message_argument as isize);
                }
            }
            K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON => {
                log::info!("System woke — signalling timer loop");
                if let Some(tx) = SLEEP_SENDER.get() {
                    let _ = tx.send(false);
                }
            }
            _ => {}
        }
    }

    /// Registers IOKit sleep/wake observers and bridges events to `sender`.
    ///
    /// Spawns a background thread that runs a `CFRunLoop` to receive IOKit
    /// power-management notifications for the lifetime of the application.
    /// Sends `true` when the system is about to sleep and `false` on wake.
    pub fn setup(sender: watch::Sender<bool>) {
        SLEEP_SENDER.set(sender).ok();

        std::thread::Builder::new()
            .name("sleep-watch".into())
            .spawn(|| unsafe {
                let mut notify_port: IONotificationPortRef = std::ptr::null_mut();
                let mut notifier: IoObjectT = 0;

                let root_port = IORegisterForSystemPower(
                    std::ptr::null_mut(),
                    &mut notify_port,
                    power_callback,
                    &mut notifier,
                );

                if root_port == 0 {
                    log::error!("IORegisterForSystemPower failed — sleep/wake awareness disabled");
                    return;
                }

                ROOT_PORT.set(root_port).ok();

                let source = IONotificationPortGetRunLoopSource(notify_port);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);

                log::info!("Sleep/wake watcher running");
                CFRunLoopRun(); // Blocks this thread; processes IOKit power notifications.
            })
            .expect("failed to spawn sleep-watch thread");
    }
}

#[cfg(not(target_os = "macos"))]
mod macos {
    use tokio::sync::watch;

    /// No-op on non-macOS platforms.
    pub fn setup(_sender: watch::Sender<bool>) {}
}

pub use macos::setup;
