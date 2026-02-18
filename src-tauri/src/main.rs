// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Program entry point that delegates execution to `eyebreak_lib::run`.
///
/// # Examples
///
/// ```
/// # // Calling `main()` starts the application; this example demonstrates invocation.
/// # fn _call_main() { main(); }
/// ```
fn main() {
    eyebreak_lib::run()
}