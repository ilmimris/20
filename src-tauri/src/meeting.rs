/// Meeting detection for macOS.
///
/// Three layers polled every 30 seconds, fully local (no network):
///   1. Native app bundle IDs via NSWorkspace.
///   2. Window title matching via `lsappinfo` / AppleScript (requires Accessibility).
///   3. Camera/microphone in-use indicator (best-effort, MVP stub).

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::NSWorkspace;

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

    /// Layer 1: Is a known native conferencing app running?
    pub fn is_native_conferencing_app_running() -> bool {
        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let apps = workspace.runningApplications();
            for app in apps.iter() {
                if let Some(bundle_id) = app.bundleIdentifier() {
                    let s = bundle_id.to_string();
                    if CONFERENCING_BUNDLE_IDS.contains(&s.as_str()) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Layer 2: Do any browser/app windows have call-related titles?
    ///
    /// Uses `lsappinfo` (available without special permissions for app names)
    /// and an AppleScript fallback to query window titles of front browsers.
    /// Returns false gracefully if permission is denied or the helper is missing.
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

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                for pattern in BROWSER_CALL_PATTERNS {
                    if text.contains(pattern) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Layer 3: Camera/microphone in use (best-effort MVP stub).
    ///
    /// A full implementation uses CMIOHardware or IOKit. For v1.0, layers 1 and 2
    /// cover the majority of conferencing scenarios. This returns false to keep
    /// the dependency surface minimal; can be enabled in a polish iteration.
    pub fn is_av_device_in_use() -> bool {
        false
    }

    /// Combined: returns true if any layer detects an active meeting.
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
    pub fn is_meeting_active() -> bool {
        false
    }
}

pub use macos::is_meeting_active;
