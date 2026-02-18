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
    fn path() -> PathBuf {
        let mut p = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push("eyebreak");
        p.push("timer_state.json");
        p
    }

    fn load() -> Option<Self> {
        let path = Self::path();
        let contents = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

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
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = fs::write(&path, json);
        }
    }
}

pub type SharedTimerState = Arc<Mutex<TimerState>>;

/// Initialise timer state from persisted disk state (if available).
pub fn restore_or_create(config: &AppConfig) -> TimerState {
    let interval = config.work_interval_minutes * 60;
    if let Some(persisted) = PersistedTimer::load() {
        // Don't restore if the interval has changed.
        if persisted.seconds_remaining <= interval {
            return TimerState {
                seconds_remaining: persisted.seconds_remaining,
                is_paused: false,
                pause_reason: None,
                is_strict_mode: config.strict_mode,
                work_interval_seconds: interval,
                manual_pause_seconds_remaining: None,
            };
        }
    }
    TimerState::new(config)
}

pub fn persist_state(state: &TimerState) {
    PersistedTimer::save(state.seconds_remaining);
}
