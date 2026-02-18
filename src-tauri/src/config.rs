use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Work interval in minutes (1–60). Default: 20.
    pub work_interval_minutes: u32,
    /// Break duration in seconds (5–60). Default: 20.
    pub break_duration_seconds: u32,
    /// Strict mode — disables skip/pause controls.
    pub strict_mode: bool,
    /// Overlay theme: "dark" | "light" | "nature".
    pub overlay_theme: String,
    /// Sound: "off" | "chime" | "whitenoise".
    pub sound: String,
    /// Launch at login.
    pub launch_at_login: bool,
    /// Pre-break warning lead time in seconds. 0 = off.
    pub pre_warning_seconds: u32,
    /// Meeting detection auto-pause.
    pub meeting_detection: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            work_interval_minutes: 20,
            break_duration_seconds: 20,
            strict_mode: false,
            overlay_theme: "dark".into(),
            sound: "off".into(),
            launch_at_login: true,
            pre_warning_seconds: 60,
            meeting_detection: true,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("eyebreak");
        path.push("config.toml");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(config) = toml::from_str::<AppConfig>(&contents) {
                return config;
            }
        }
        let default_config = Self::default();
        // Save defaults on first run
        let _ = default_config.save();
        default_config
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let contents = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, contents).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Clamp values to valid ranges.
    pub fn validated(mut self) -> Self {
        self.work_interval_minutes = self.work_interval_minutes.clamp(1, 60);
        self.break_duration_seconds = self.break_duration_seconds.clamp(5, 60);
        self
    }
}
