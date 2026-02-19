use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    /// Create an `AppConfig` populated with the application's default settings.
    ///
    /// The defaults are:
    /// - `work_interval_minutes = 20`
    /// - `break_duration_seconds = 20`
    /// - `strict_mode = false`
    /// - `overlay_theme = "dark"`
    /// - `sound = "off"`
    /// - `launch_at_login = true`
    /// - `pre_warning_seconds = 60`
    /// - `meeting_detection = true`
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = AppConfig::default();
    /// assert_eq!(cfg.work_interval_minutes, 20);
    /// assert_eq!(cfg.overlay_theme, "dark");
    /// assert!(cfg.launch_at_login);
    /// ```
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
    /// Constructs the filesystem path to the application's configuration file.
    ///
    /// The path points to "<user_config_dir>/twenty20/config.toml" when a user config
    /// directory is available; otherwise it falls back to "./twenty20/config.toml".
    ///
    /// # Examples
    ///
    /// ```
    /// let p = config_path();
    /// assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("config.toml"));
    /// let parent = p.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str());
    /// assert_eq!(parent, Some("twenty20"));
    /// ```
    pub fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("twenty20");
        path.push("config.toml");
        path
    }

    /// Loads the application configuration from the platform-specific config file, falling back to the default configuration if the file is missing or invalid.
    ///
    /// If the file is missing or cannot be parsed as TOML, the default `AppConfig` is saved to the config path before being returned.
    ///
    /// # Returns
    ///
    /// The loaded `AppConfig`, or the default `AppConfig` when loading/parsing fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = AppConfig::load();
    /// // use cfg, e.g. ensure values are within expected ranges
    /// let cfg = cfg.validated();
    /// ```
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

    /// Persists the configuration to the platform-specific config file.
    ///
    /// Attempts to create the parent directory if necessary, serializes `self` to
    /// pretty TOML, and writes it to the path returned by `AppConfig::config_path()`.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the file was written successfully, `Err(String)` with an error
    /// message if directory creation, serialization, or file write fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = AppConfig::default();
    /// // Save the default config to the config path; assert that it succeeds.
    /// assert!(cfg.save().is_ok());
    /// ```
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let contents = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, contents).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Return a copy of the config with numeric fields clamped to their valid ranges.
    ///
    /// Ensures `work_interval_minutes` is between 1 and 60 (inclusive) and
    /// `break_duration_seconds` is between 5 and 60 (inclusive).
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = AppConfig { work_interval_minutes: 0, break_duration_seconds: 120, ..Default::default() };
    /// let valid = cfg.validated();
    /// assert_eq!(valid.work_interval_minutes, 1);
    /// assert_eq!(valid.break_duration_seconds, 60);
    /// ```
    pub fn validated(mut self) -> Self {
        self.work_interval_minutes = self.work_interval_minutes.clamp(1, 60);
        self.break_duration_seconds = self.break_duration_seconds.clamp(5, 60);
        // pre_warning_seconds: 0 (off) or 30–120.
        if self.pre_warning_seconds != 0 {
            self.pre_warning_seconds = self.pre_warning_seconds.clamp(30, 120);
        }
        // Normalise string enums to known values; fall back to default.
        if !["dark", "light", "nature"].contains(&self.overlay_theme.as_str()) {
            self.overlay_theme = "dark".into();
        }
        if !["off", "chime", "whitenoise"].contains(&self.sound.as_str()) {
            self.sound = "off".into();
        }
        self
    }
}
