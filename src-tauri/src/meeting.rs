/// Meeting detection for macOS.
///
/// Three layers polled every 30 seconds, fully local (no network):
///   1. Native app bundle IDs via NSWorkspace.
///   2. Window title matching via `lsappinfo` / AppleScript (requires Accessibility).
///   3. Camera/microphone in-use indicator (best-effort, MVP stub).
#[cfg(target_os = "macos")]
mod macos {

    /// Bundle IDs for known native conferencing apps.
    const CONFERENCING_BUNDLE_IDS: &[&str] = &[
        "us.zoom.xos",
        "com.microsoft.teams2",
        "Cisco-Systems.Spark",
        "com.apple.FaceTime",
        "com.hnc.Discord",
    ];

    /// Window title fragments that indicate a browser-based call.
    const BROWSER_CALL_PATTERNS: &[&str] = &[
        "Meet \u{2013}",
        "Meet - ",
        "Zoom Meeting",
        "Microsoft Teams",
        "On a call",
        "Google Meet",
    ];

    /// Layer 1: Is a known native conferencing app running and not hidden?
    ///
    /// Checks the system's running applications for bundle identifiers that match the internal
    /// list of known conferencing apps.
    ///
    /// # Returns
    ///
    /// `true` if a known native conferencing application is running, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let active = is_native_conferencing_app_running();
    /// println!("Native conferencing app running: {}", active);
    /// ```
    pub fn is_native_conferencing_app_running() -> bool {
        use objc2_app_kit::NSWorkspace;

        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        for app in apps.iter() {
            if let Some(bundle_id) = app.bundleIdentifier() {
                let s = bundle_id.to_string();
                if CONFERENCING_BUNDLE_IDS.contains(&s.as_str()) && !app.isHidden() {
                    return true;
                }
            }
        }
        false
    }

    /// Detects whether any frontmost browser window appears to be in a call based on its title.
    ///
    /// This checks common browser processes' front-window titles via an AppleScript and scans
    /// them for known call-related fragments. The check is best-effort: it returns `false` if
    /// the helper command is unavailable, if execution fails, or if permissions prevent reading
    /// window titles for the inspected windows.
    ///
    /// # Returns
    ///
    /// `true` if any front browser window title contains a known call-related pattern, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// let in_call = is_browser_call_active();
    /// // `in_call` will be `true` when a matching call title is detected, otherwise `false`.
    /// ```
    pub fn is_browser_call_active() -> bool {
        // AppleScript to get frontmost browser window title.
        // This approach requires Accessibility permission for non-frontmost windows
        // but works for the active window without it.
        let script = r#"
            tell application "System Events"
                set windowTitles to {}
                set browserBundles to {"com.google.Chrome", "org.mozilla.firefox", "com.apple.Safari", "com.microsoft.edgemac"}
                repeat with proc in (processes whose background only is false)
                    if bundle identifier of proc is in browserBundles then
                        try
                            set end of windowTitles to name of front window of proc
                        end try
                    end if
                end repeat
                return windowTitles as text
            end tell
        "#;

        // Spawn with a 5-second timeout to avoid blocking the detection loop
        // when Accessibility permission is denied or the script hangs.
        let mut child = match std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return false,
        };

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Child exited — collect output.
                    use std::io::Read;
                    let mut buf = Vec::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        let _ = stdout.read_to_end(&mut buf);
                    }
                    let text = String::from_utf8_lossy(&buf);
                    return BROWSER_CALL_PATTERNS.iter().any(|p| text.contains(p));
                }
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        log::warn!("osascript timed out — killing child process");
                        let _ = child.kill();
                        let _ = child.wait(); // reap zombie
                        return false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => return false,
            }
        }
    }

    /// Reports whether a camera or microphone appears to be in use (best-effort stub).
    ///
    /// This is a placeholder implementation for v1.0 that always returns `false`.
    /// A complete implementation would query system APIs (e.g., CMIO/IOKit) to detect
    /// active AV devices; keeping this as a stub avoids adding heavy native dependencies
    /// in the initial release.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(!is_av_device_in_use());
    /// ```
    pub fn is_av_device_in_use() -> bool {
        false
    }

    /// Determines whether a meeting is currently active.
    ///
    /// Checks multiple detection layers (native conferencing apps, browser-based calls, and AV device usage)
    /// and returns `true` if any layer indicates an active meeting.
    ///
    /// # Examples
    ///
    /// ```
    /// let active = is_meeting_active();
    /// // `active` is `true` when a meeting is detected, `false` otherwise
    /// let _ = active;
    /// ```
    ///
    /// # Returns
    ///
    /// `true` if a meeting is detected by any detection layer, `false` otherwise.
    pub fn is_meeting_active() -> bool {
        if is_native_conferencing_app_running() {
            return true;
        }
        if is_browser_call_active() {
            return true;
        }
        is_av_device_in_use()
    }
}

#[cfg(not(target_os = "macos"))]
mod macos {
    /// Reports whether a meeting is active on the host system; on non-macOS builds this stub always reports no meeting.
    ///
    /// # Examples
    ///
    /// ```
    /// // On non-macOS targets this will always be false.
    /// assert_eq!(is_meeting_active(), false);
    /// ```
    ///
    /// # Returns
    ///
    /// `true` if a meeting is detected, `false` otherwise. On non-macOS builds this always returns `false`.
    pub fn is_meeting_active() -> bool {
        false
    }
}

pub use macos::is_meeting_active;
