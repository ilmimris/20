use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    Manual,
    Meeting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerState {
    /// Seconds remaining until next break.
    pub seconds_remaining: u32,
    pub is_paused: bool,
    pub pause_reason: Option<PauseReason>,
    pub is_strict_mode: bool,
    /// Total work interval seconds (for resets).
    pub work_interval_seconds: u32,
    /// Countdown for a manual pause (seconds remaining before auto-resume).
    /// Set by the pause_timer command; decremented by the timer loop.
    pub manual_pause_seconds_remaining: Option<u32>,
}

impl TimerState {
    /// Creates a new `TimerState` initialized from application configuration.
    ///
    /// The returned state uses `config.work_interval_minutes` to set both `seconds_remaining`
    /// and `work_interval_seconds`, sets `is_paused` to `false`, `pause_reason` to `None`,
    /// `is_strict_mode` from `config.strict_mode`, and leaves `manual_pause_seconds_remaining` as `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// // Construct a minimal AppConfig. Replace with your application's config constructor.
    /// let config = AppConfig { work_interval_minutes: 25, strict_mode: true, ..Default::default() };
    /// let state = TimerState::new(&config);
    /// assert_eq!(state.seconds_remaining, 25 * 60);
    /// assert!(!state.is_paused);
    /// assert!(state.pause_reason.is_none());
    /// assert!(state.manual_pause_seconds_remaining.is_none());
    /// assert!(state.is_strict_mode);
    /// ```
    pub fn new(config: &AppConfig) -> Self {
        Self {
            seconds_remaining: config.work_interval_minutes * 60,
            is_paused: false,
            pause_reason: None,
            is_strict_mode: config.strict_mode,
            work_interval_seconds: config.work_interval_minutes * 60,
            manual_pause_seconds_remaining: None,
        }
    }
}

/// Persistent state saved to disk across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedTimer {
    seconds_remaining: u32,
    /// Unix timestamp in seconds when state was saved.
    saved_at: u64,
}

impl PersistedTimer {
    /// Builds the filesystem path for the timer state JSON file.
    ///
    /// Uses the platform-specific data-local directory when available; otherwise falls back to the current directory.
    /// The resulting path ends with `twenty20/timer_state.json`.
    ///
    /// # Examples
    ///
    /// ```
    /// let p = path();
    /// assert!(p.ends_with(std::path::Path::new("twenty20/timer_state.json")));
    /// ```
    fn path() -> PathBuf {
        let mut p = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push("twenty20");
        p.push("timer_state.json");
        p
    }

    /// Loads the persisted timer state from disk if it exists and is valid JSON.
    ///
    /// Attempts to read the timer state file at the configured path and deserialize it into a
    /// `PersistedTimer`.
    ///
    /// # Returns
    ///
    /// `Some(PersistedTimer)` if the file exists and contains valid JSON, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// // Attempt to restore persisted timer state; handle absence gracefully.
    /// if let Some(persisted) = crate::timer::PersistedTimer::load() {
    ///     println!("restored {} seconds", persisted.seconds_remaining);
    /// } else {
    ///     println!("no persisted timer state found");
    /// }
    /// ```
    fn load() -> Option<Self> {
        let path = Self::path();
        let contents = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Persists the provided remaining seconds to the timer state file with the current Unix timestamp.
    ///
    /// The function writes a JSON object containing `seconds_remaining` and the save time (`saved_at`)
    /// to the module's timer state path, creating parent directories if necessary. I/O failures are
    /// ignored.
    ///
    /// # Examples
    ///
    /// ```
    /// // Persist 90 seconds remaining to the timer state file.
    /// PersistedTimer::save(90);
    /// ```
    fn save(seconds_remaining: u32) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let saved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let state = PersistedTimer {
            seconds_remaining,
            saved_at,
        };
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                log::warn!(
                    "Failed to create timer state directory {}: {e}",
                    parent.display()
                );
                return;
            }
        }
        match serde_json::to_string(&state) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    log::warn!("Failed to write timer state to {}: {e}", path.display());
                }
            }
            Err(e) => log::warn!("Failed to serialise timer state: {e}"),
        }
    }
}

pub type SharedTimerState = Arc<Mutex<TimerState>>;

/// Initialise timer state from persisted disk state (if available).
///
/// Subtracts the time elapsed since the state was saved so that the countdown
/// continues correctly across restarts and sleep/wake cycles.
///
/// If a persisted state exists and its `seconds_remaining` is less than or equal to `config.work_interval_minutes * 60`, the persisted remaining seconds are used while other runtime fields are initialized from `config`. If no compatible persisted state is found, a new `TimerState` is returned via `TimerState::new`.
///
/// # Examples
///
/// ```no_run
/// let config = crate::AppConfig {
///     work_interval_minutes: 25,
///     strict_mode: false,
///     // fill other fields as required by AppConfig...
/// };
/// let state = crate::timer::restore_or_create(&config);
/// ```
pub fn restore_or_create(config: &AppConfig) -> TimerState {
    use std::time::{SystemTime, UNIX_EPOCH};

    let interval = config.work_interval_minutes * 60;

    if let Some(persisted) = PersistedTimer::load() {
        // Skip restore if the work interval setting changed.
        if persisted.seconds_remaining > interval {
            return TimerState::new(config);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(persisted.saved_at);

        let elapsed = now.saturating_sub(persisted.saved_at);
        let adjusted = persisted.seconds_remaining.saturating_sub(elapsed as u32);

        // If the timer would have expired while the app was away, start fresh.
        if adjusted == 0 {
            return TimerState::new(config);
        }

        return TimerState {
            seconds_remaining: adjusted,
            is_paused: false,
            pause_reason: None,
            is_strict_mode: config.strict_mode,
            work_interval_seconds: interval,
            manual_pause_seconds_remaining: None,
        };
    }
    TimerState::new(config)
}

/// Persists the timer's current seconds remaining to durable storage.
///
/// Saves the state's `seconds_remaining` value so it can be restored on a later run.
///
/// # Examples
///
/// ```
/// use crate::timer::{TimerState, persist_state};
///
/// let state = TimerState {
///     seconds_remaining: 120,
///     is_paused: false,
///     pause_reason: None,
///     is_strict_mode: false,
///     work_interval_seconds: 1500,
///     manual_pause_seconds_remaining: None,
/// };
///
/// persist_state(&state);
/// ```
pub fn persist_state(state: &TimerState) {
    PersistedTimer::save(state.seconds_remaining);
}
