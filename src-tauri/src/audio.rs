use std::sync::Mutex;
use objc2_app_kit::NSSound;
use objc2_foundation::{MainThreadMarker, NSString};
use objc2::rc::Retained;
use tauri::{AppHandle, Manager};
use crate::commands::AppState;

#[allow(dead_code)]
struct SoundWrapper(Retained<NSSound>);
unsafe impl Send for SoundWrapper {}
unsafe impl Sync for SoundWrapper {}

static CURRENT_SOUND: Mutex<Option<SoundWrapper>> = Mutex::new(None);

pub fn play_break_sound(app: &AppHandle) {
    let app_state = app.state::<AppState>();
    let sound_name = {
        let cfg = app_state.config.lock().unwrap_or_else(|e| e.into_inner());
        cfg.sound.clone()
    };

    if sound_name == "off" {
        return;
    }

    let system_sound = match sound_name.as_str() {
        "chime" => "Glass",
        "whitenoise" => "Blow",
        _ => {
            log::warn!("Unknown sound name: '{}'", sound_name);
            return;
        }
    };

    let _ = app.run_on_main_thread(move || {
        let _mtm = MainThreadMarker::new().expect("must run on main thread");
        let name_str = NSString::from_str(system_sound);
        
        let sound = NSSound::soundNamed(&name_str);
        if let Some(s) = sound {
            s.play();
            // Keep it alive
            let mut guard = CURRENT_SOUND.lock().unwrap();
            *guard = Some(SoundWrapper(s));
        } else {
            log::warn!("System sound '{}' not found", system_sound);
        }
    });
}
