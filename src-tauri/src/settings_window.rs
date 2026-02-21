use crate::commands::AppState;
use objc2::runtime::{AnyClass, ClassBuilder, Sel};
use objc2::{msg_send, rc::Retained, sel, ClassType};
use objc2_app_kit::{
    NSBackingStoreType, NSBezelStyle, NSBox, NSButton, NSColor, NSFont, NSGridView,
    NSLayoutConstraint, NSPanel, NSPopUpButton, NSStackView, NSSwitch, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSEdgeInsets, NSObject, NSPoint, NSRect, NSSize, NSString,
};
use std::sync::{Mutex, Once, OnceLock};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

struct SettingsControls {
    app_handle: AppHandle,
    work_field: Retained<NSTextField>,
    break_field: Retained<NSTextField>,
    strict_switch: Retained<NSSwitch>,
    launch_switch: Retained<NSSwitch>,
    meet_switch: Retained<NSSwitch>,
    theme_popup: Retained<NSPopUpButton>,
    sound_popup: Retained<NSPopUpButton>,
    warning_popup: Retained<NSPopUpButton>,
}

struct SettingsControlsWrapper(SettingsControls);
unsafe impl Send for SettingsControlsWrapper {}
unsafe impl Sync for SettingsControlsWrapper {}

static SETTINGS_CONTROLS: OnceLock<Mutex<Option<SettingsControlsWrapper>>> = OnceLock::new();

struct PanelWrapper(Retained<NSPanel>);
unsafe impl Send for PanelWrapper {}
unsafe impl Sync for PanelWrapper {}

static SETTINGS_WINDOW: OnceLock<Mutex<Option<PanelWrapper>>> = OnceLock::new();

fn create_settings_delegate(_mtm: MainThreadMarker) -> Retained<NSObject> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let superclass = NSObject::class();
        let name = c"Twenty20SettingsDelegate";
        let mut builder =
            ClassBuilder::new(name, superclass).expect("failed to create class builder");

        unsafe {
            builder.add_method(sel!(save:), save_action as extern "C" fn(_, _, _));
        }

        builder.register();
    });

    let name = c"Twenty20SettingsDelegate";
    let cls = AnyClass::get(name).expect("class not registered");
    let obj: Option<Retained<NSObject>> = unsafe { msg_send![cls, new] };
    obj.expect("failed to create delegate")
}

extern "C" fn save_action(_this: &NSObject, _cmd: Sel, _sender: Option<&NSObject>) {
    log::info!("Save action triggered");
    let guard = SETTINGS_CONTROLS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap();
    if let Some(wrapper) = &*guard {
        let controls = &wrapper.0;
        let app = &controls.app_handle;
        let state = app.state::<AppState>();

        // Read values safely
        let work_mins = controls.work_field.integerValue() as u32;
        let break_secs = controls.break_field.integerValue() as u32;
        let strict = controls.strict_switch.state() == 1;
        let launch = controls.launch_switch.state() == 1;
        let meet = controls.meet_switch.state() == 1;

        let theme = controls
            .theme_popup
            .titleOfSelectedItem()
            .map(|s| s.to_string())
            .unwrap_or("dark".to_string());
        let sound = controls
            .sound_popup
            .titleOfSelectedItem()
            .map(|s| s.to_string())
            .unwrap_or("off".to_string());
        let warn_val = controls
            .warning_popup
            .titleOfSelectedItem()
            .map(|s| s.to_string())
            .unwrap_or("Off".to_string());
        let pre_warn = if warn_val == "Off" {
            0
        } else {
            warn_val.trim_end_matches('s').parse().unwrap_or(60)
        };

        log::info!(
            "Saving: Work={}, Break={}, Strict={}, Launch={}, Meet={}, Theme={}, Sound={}, Warn={}",
            work_mins,
            break_secs,
            strict,
            launch,
            meet,
            theme,
            sound,
            pre_warn
        );

        // Update config and Timer
        {
            let mut config = state.config.lock().unwrap();
            config.work_interval_minutes = work_mins;
            config.break_duration_seconds = break_secs;
            config.strict_mode = strict;
            config.launch_at_login = launch;
            config.meeting_detection = meet;
            config.overlay_theme = theme;
            config.sound = sound;
            config.pre_warning_seconds = pre_warn;

            // Validate and save
            let validated = config.clone().validated();
            *config = validated.clone();

            if let Err(e) = config.save() {
                log::error!("Failed to save config: {}", e);
            } else {
                log::info!("Config saved to disk");
            }

            // Update Timer state
            let mut ts = state.timer.lock().unwrap_or_else(|e| e.into_inner());
            ts.is_strict_mode = validated.strict_mode;
            ts.work_interval_seconds = validated.work_interval_minutes * 60;
        }

        // Update autolaunch via plugin
        if launch {
            let _ = app.autolaunch().enable();
        } else {
            let _ = app.autolaunch().disable();
        }

        close_settings_window();
    }
}

fn close_settings_window() {
    let guard = SETTINGS_WINDOW
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap();
    if let Some(wrapper) = &*guard {
        wrapper.0.close();
    }
}

pub fn show_settings(app: &AppHandle) {
    let mtm = MainThreadMarker::new().expect("must be on main thread");

    // Read config
    let state = app.state::<AppState>();
    let config = state.config.lock().unwrap();
    let work_mins = config.work_interval_minutes;
    let break_secs = config.break_duration_seconds;
    let strict = config.strict_mode;
    let launch = config.launch_at_login;
    let meet = config.meeting_detection;
    let theme = config.overlay_theme.clone();
    let sound = config.sound.clone();
    let pre_warn = config.pre_warning_seconds;
    drop(config);

    // Check if window already exists
    let mut guard = SETTINGS_WINDOW
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap();
    if let Some(wrapper) = &*guard {
        #[allow(deprecated)]
        objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        wrapper.0.makeKeyAndOrderFront(None);
        return;
    }

    // Create new window
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 500.0));
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable; // Maybe fixed size?

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        mtm.alloc(),
        rect,
        style,
        NSBackingStoreType::Buffered,
        false,
    );

    panel.setTitle(&NSString::from_str("Twenty20 Settings"));
    unsafe {
        panel.setReleasedWhenClosed(false);
    }
    panel.center();

    // Create Layout
    let content_view = panel.contentView().expect("content view");

    let main_stack = NSStackView::new(mtm);
    main_stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    main_stack.setSpacing(16.0);
    main_stack.setEdgeInsets(NSEdgeInsets {
        top: 20.0,
        left: 20.0,
        bottom: 20.0,
        right: 20.0,
    });
    main_stack.setTranslatesAutoresizingMaskIntoConstraints(false);

    content_view.addSubview(&main_stack);

    // Constraints
    let c1 = main_stack
        .topAnchor()
        .constraintEqualToAnchor(&content_view.topAnchor());
    let c2 = main_stack
        .leadingAnchor()
        .constraintEqualToAnchor(&content_view.leadingAnchor());
    let c3 = main_stack
        .trailingAnchor()
        .constraintEqualToAnchor(&content_view.trailingAnchor());

    let constraints: [&NSLayoutConstraint; 3] = [&c1, &c2, &c3];
    NSLayoutConstraint::activateConstraints(&NSArray::from_slice(&constraints));

    // --- Timer Section ---
    add_section_header(&main_stack, "TIMER", mtm);

    let grid_timer: Option<Retained<NSGridView>> =
        unsafe { msg_send![mtm.alloc::<NSGridView>(), initWithFrame: NSRect::ZERO] };
    let grid_timer = grid_timer.expect("timer grid init failed");

    grid_timer.setRowSpacing(8.0);
    grid_timer.setColumnSpacing(12.0);
    grid_timer.setXPlacement(objc2_app_kit::NSGridCellPlacement::Leading);

    // Work Interval
    let (lbl_work, input_work) =
        create_number_row("Work interval (minutes)", work_mins as i32, mtm);
    let views_work: [&NSView; 2] = [&lbl_work, &input_work];
    grid_timer.addRowWithViews(&NSArray::from_slice(&views_work));

    // Break Duration
    let (lbl_break, input_break) =
        create_number_row("Break duration (seconds)", break_secs as i32, mtm);
    let views_break: [&NSView; 2] = [&lbl_break, &input_break];
    grid_timer.addRowWithViews(&NSArray::from_slice(&views_break));

    main_stack.addArrangedSubview(&grid_timer);

    // --- Behavior Section ---
    add_section_header(&main_stack, "BEHAVIOR", mtm);

    let grid_behavior: Option<Retained<NSGridView>> =
        unsafe { msg_send![mtm.alloc::<NSGridView>(), initWithFrame: NSRect::ZERO] };
    let grid_behavior = grid_behavior.expect("behavior grid init failed");

    grid_behavior.setRowSpacing(8.0);
    grid_behavior.setColumnSpacing(12.0);
    grid_behavior.setXPlacement(objc2_app_kit::NSGridCellPlacement::Leading);

    let (lbl_strict, switch_strict) = create_switch_row("Strict mode", strict, mtm);
    let desc_strict = create_small_text("Disable skip/pause. Press Esc × 3 to exit.", mtm);
    let views_strict: [&NSView; 2] = [&lbl_strict, &switch_strict];
    grid_behavior.addRowWithViews(&NSArray::from_slice(&views_strict));

    let (lbl_login, switch_login) = create_switch_row("Launch at login", launch, mtm);
    let views_login: [&NSView; 2] = [&lbl_login, &switch_login];
    grid_behavior.addRowWithViews(&NSArray::from_slice(&views_login));

    let (lbl_meet, switch_meet) = create_switch_row("Meeting detection", meet, mtm);
    let views_meet: [&NSView; 2] = [&lbl_meet, &switch_meet];
    grid_behavior.addRowWithViews(&NSArray::from_slice(&views_meet));

    main_stack.addArrangedSubview(&grid_behavior);
    main_stack.addArrangedSubview(&desc_strict); // Place description below strict row group

    // --- Appearance Section ---
    add_section_header(&main_stack, "APPEARANCE", mtm);
    let grid_appearance: Option<Retained<NSGridView>> =
        unsafe { msg_send![mtm.alloc::<NSGridView>(), initWithFrame: NSRect::ZERO] };
    let grid_appearance = grid_appearance.expect("appearance grid init failed");
    grid_appearance.setRowSpacing(8.0);
    grid_appearance.setColumnSpacing(12.0);
    grid_appearance.setXPlacement(objc2_app_kit::NSGridCellPlacement::Leading);

    let (lbl_theme, popup_theme) =
        create_dropdown_row("Overlay Theme", &["dark", "light", "nature"], &theme, mtm);
    let views_theme: [&NSView; 2] = [&lbl_theme, &popup_theme];
    grid_appearance.addRowWithViews(&NSArray::from_slice(&views_theme));

    let (lbl_sound, popup_sound) =
        create_dropdown_row("Timer Sound", &["off", "chime", "whitenoise"], &sound, mtm);
    let views_sound: [&NSView; 2] = [&lbl_sound, &popup_sound];
    grid_appearance.addRowWithViews(&NSArray::from_slice(&views_sound));

    let warn_str = if pre_warn == 0 {
        "Off".to_string()
    } else {
        format!("{}s", pre_warn)
    };
    let (lbl_warn, popup_warn) = create_dropdown_row(
        "Pre-break Warning",
        &["Off", "30s", "60s", "90s", "120s"],
        &warn_str,
        mtm,
    );
    let views_warn: [&NSView; 2] = [&lbl_warn, &popup_warn];
    grid_appearance.addRowWithViews(&NSArray::from_slice(&views_warn));

    main_stack.addArrangedSubview(&grid_appearance);

    // --- Footer (Save) ---
    let spacer = NSBox::new(mtm);
    main_stack.addArrangedSubview(&spacer);

    let save_btn = NSButton::new(mtm);
    save_btn.setTitle(&NSString::from_str("Save Settings"));
    #[allow(deprecated)]
    save_btn.setBezelStyle(NSBezelStyle::Rounded);
    save_btn.setKeyEquivalent(&NSString::from_str("\r"));

    let delegate = create_settings_delegate(mtm);
    unsafe {
        save_btn.setTarget(Some(&delegate));
        save_btn.setAction(Some(sel!(save:)));
    }
    main_stack.addArrangedSubview(&save_btn);

    // Store controls for delegate access
    let controls = SettingsControls {
        app_handle: app.clone(),
        work_field: input_work,
        break_field: input_break,
        strict_switch: switch_strict,
        launch_switch: switch_login,
        meet_switch: switch_meet,
        theme_popup: popup_theme,
        sound_popup: popup_sound,
        warning_popup: popup_warn,
    };
    *SETTINGS_CONTROLS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = Some(SettingsControlsWrapper(controls));

    // Keep delegate alive
    *SETTINGS_DELEGATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = Some(DelegateWrapper(delegate));

    // Bring the app to the foreground so the panel appears on top.
    // Required because the app runs with Accessory activation policy and is not
    // automatically active when the user clicks a tray menu item.
    #[allow(deprecated)]
    objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);

    panel.makeKeyAndOrderFront(None);
    *guard = Some(PanelWrapper(panel));

    log::info!("Native settings window opened");
}

struct DelegateWrapper(#[allow(dead_code)] Retained<NSObject>);
unsafe impl Send for DelegateWrapper {}
unsafe impl Sync for DelegateWrapper {}

static SETTINGS_DELEGATE: OnceLock<Mutex<Option<DelegateWrapper>>> = OnceLock::new();

fn add_section_header(stack: &NSStackView, title: &str, mtm: MainThreadMarker) {
    let label = NSTextField::new(mtm);
    label.setStringValue(&NSString::from_str(title));
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setFont(Some(&NSFont::boldSystemFontOfSize(10.0)));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    stack.addArrangedSubview(&label);
}

fn create_number_row(
    label_text: &str,
    default_val: i32,
    mtm: MainThreadMarker,
) -> (Retained<NSTextField>, Retained<NSTextField>) {
    let label = NSTextField::new(mtm);
    label.setStringValue(&NSString::from_str(label_text));
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);

    let input = NSTextField::new(mtm);
    input.setStringValue(&NSString::from_str(&default_val.to_string()));
    input.setBezeled(true);
    input.setDrawsBackground(true);
    input
        .widthAnchor()
        .constraintEqualToConstant(60.0)
        .setActive(true);
    (label, input)
}

fn create_switch_row(
    label_text: &str,
    checked: bool,
    mtm: MainThreadMarker,
) -> (Retained<NSTextField>, Retained<NSSwitch>) {
    let label = NSTextField::new(mtm);
    label.setStringValue(&NSString::from_str(label_text));
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);

    let toggle = NSSwitch::new(mtm);
    let state = if checked { 1 } else { 0 };
    toggle.setState(state);
    (label, toggle)
}

fn create_small_text(text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::new(mtm);
    label.setStringValue(&NSString::from_str(text));
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    label.setTextColor(Some(&NSColor::tertiaryLabelColor()));
    label.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    label.setPreferredMaxLayoutWidth(300.0);
    label
}

fn create_dropdown_row(
    label_text: &str,
    options: &[&str],
    selected: &str,
    mtm: MainThreadMarker,
) -> (Retained<NSTextField>, Retained<NSPopUpButton>) {
    let label = NSTextField::new(mtm);
    label.setStringValue(&NSString::from_str(label_text));
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);

    let popup = mtm.alloc::<NSPopUpButton>();
    let popup: Option<Retained<NSPopUpButton>> =
        unsafe { msg_send![popup, initWithFrame: NSRect::ZERO, pullsDown: false] };
    let popup = popup.expect("failed to init popup");

    for option in options {
        popup.addItemWithTitle(&NSString::from_str(option));
    }
    popup.selectItemWithTitle(&NSString::from_str(selected));

    (label, popup)
}
