// This module is a public API; many items are not yet wired into commands.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};

static RUNNING: AtomicBool = AtomicBool::new(false);
static LISTENER_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Callback type for hotkey events.
type HotkeyCallback = Box<dyn Fn() + Send + 'static>;
type CaptureCallback = Box<dyn Fn(String) + Send + 'static>;
type PermissionCallback = Box<dyn Fn(String) + Send + 'static>;

/// Stored callbacks for hotkey events (set by the caller before start).
static CALLBACKS: once_cell::sync::Lazy<Mutex<HotkeyCallbacks>> =
    once_cell::sync::Lazy::new(|| {
        Mutex::new(HotkeyCallbacks {
            on_key_down: None,
            on_key_up: None,
            on_double_tap: None,
        })
    });

static CAPTURE_CALLBACKS: once_cell::sync::Lazy<Mutex<Option<CaptureCallbacks>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

static PERMISSION_CALLBACK: once_cell::sync::Lazy<Mutex<Option<PermissionCallback>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

struct HotkeyCallbacks {
    on_key_down: Option<HotkeyCallback>,
    on_key_up: Option<HotkeyCallback>,
    on_double_tap: Option<HotkeyCallback>,
}

// Safety: the callbacks are only invoked from the hotkey thread, and we
// protect access via a Mutex.
unsafe impl Send for HotkeyCallbacks {}

struct CaptureCallbacks {
    on_preview: CaptureCallback,
    on_capture: CaptureCallback,
}

enum RuntimeEvent {
    KeyDown,
    KeyUp,
    DoubleTap,
    CapturePreview(String),
    CaptureFinish(CaptureCallback, String),
    PermissionRequired(String),
}

static DISPATCHER: once_cell::sync::Lazy<mpsc::Sender<RuntimeEvent>> =
    once_cell::sync::Lazy::new(|| {
        let (tx, rx) = mpsc::channel::<RuntimeEvent>();
        std::thread::Builder::new()
            .name("yap-hotkey-dispatch".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    match event {
                        RuntimeEvent::KeyDown => {
                            if let Ok(cb) = CALLBACKS.lock() {
                                if let Some(ref f) = cb.on_key_down {
                                    f();
                                }
                            }
                        }
                        RuntimeEvent::KeyUp => {
                            if let Ok(cb) = CALLBACKS.lock() {
                                if let Some(ref f) = cb.on_key_up {
                                    f();
                                }
                            }
                        }
                        RuntimeEvent::DoubleTap => {
                            if let Ok(cb) = CALLBACKS.lock() {
                                if let Some(ref f) = cb.on_double_tap {
                                    f();
                                }
                            }
                        }
                        RuntimeEvent::CapturePreview(shortcut) => {
                            if let Ok(capture) = CAPTURE_CALLBACKS.lock() {
                                if let Some(callbacks) = capture.as_ref() {
                                    (callbacks.on_preview)(shortcut);
                                }
                            }
                        }
                        RuntimeEvent::CaptureFinish(callback, shortcut) => {
                            callback(shortcut);
                        }
                        RuntimeEvent::PermissionRequired(label) => {
                            if let Ok(cb) = PERMISSION_CALLBACK.lock() {
                                if let Some(ref f) = *cb {
                                    f(label);
                                }
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn hotkey dispatch thread");
        tx
    });

fn dispatch(event: RuntimeEvent) {
    let _ = DISPATCHER.send(event);
}

fn begin_listener_generation() -> u64 {
    LISTENER_EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}

fn cancel_listener_generation() {
    LISTENER_EPOCH.fetch_add(1, Ordering::SeqCst);
}

fn listener_generation_matches(generation: u64) -> bool {
    RUNNING.load(Ordering::SeqCst) && LISTENER_EPOCH.load(Ordering::SeqCst) == generation
}

/// Modifier keys supported in configurable shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyModifier {
    Command,
    Control,
    Option,
    Shift,
    Fn,
}

/// Hotkey shortcut the user has configured.
///
/// `triggers` is empty for modifier-only shortcuts such as `fn`, `option`, or
/// `cmd+shift`. Non-modifier triggers use canonical names such as `space`,
/// `a`, `1`, `return`, `left`, or `f12`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeySpec {
    pub modifiers: Vec<HotkeyModifier>,
    pub triggers: Vec<String>,
}

impl HotkeySpec {
    pub fn parse(value: &str) -> Self {
        Self::try_parse(value).unwrap_or_else(|| Self {
            modifiers: vec![HotkeyModifier::Fn],
            triggers: Vec::new(),
        })
    }

    pub fn try_parse(value: &str) -> Option<Self> {
        let normalized = value.trim().replace(' ', "").to_lowercase();
        if normalized.is_empty() {
            return None;
        }

        let mut modifiers = Vec::new();
        let mut triggers: Vec<String> = Vec::new();

        for raw_part in normalized.split('+').filter(|part| !part.is_empty()) {
            if let Some(modifier) = parse_modifier(raw_part) {
                if !modifiers.contains(&modifier) {
                    modifiers.push(modifier);
                }
                continue;
            }

            let key = normalize_key(raw_part)?;
            if !triggers.contains(&key) {
                triggers.push(key);
            }
        }

        if modifiers.is_empty() && triggers.is_empty() {
            return None;
        }

        modifiers.sort_by_key(|modifier| modifier_order(*modifier));

        Some(Self {
            modifiers,
            triggers,
        })
    }

    pub fn label(&self) -> String {
        self.parts()
            .into_iter()
            .map(|part| label_part(&part))
            .collect::<Vec<_>>()
            .join("+")
    }

    pub fn canonical(&self) -> String {
        self.parts().join("+")
    }

    fn parts(&self) -> Vec<String> {
        let mut parts = self
            .modifiers
            .iter()
            .map(|modifier| match modifier {
                HotkeyModifier::Command => "cmd".to_string(),
                HotkeyModifier::Control => "ctrl".to_string(),
                HotkeyModifier::Option => "option".to_string(),
                HotkeyModifier::Shift => "shift".to_string(),
                HotkeyModifier::Fn => "fn".to_string(),
            })
            .collect::<Vec<_>>();

        for trigger in &self.triggers {
            parts.push(trigger.clone());
        }

        parts
    }
}

fn parse_modifier(value: &str) -> Option<HotkeyModifier> {
    match value {
        "cmd" | "command" | "meta" | "super" => Some(HotkeyModifier::Command),
        "ctrl" | "control" => Some(HotkeyModifier::Control),
        "alt" | "option" => Some(HotkeyModifier::Option),
        "shift" => Some(HotkeyModifier::Shift),
        "fn" | "globe" => Some(HotkeyModifier::Fn),
        _ => None,
    }
}

fn normalize_key(value: &str) -> Option<String> {
    let key = match value {
        "esc" => "escape",
        "enter" => "return",
        "backspace" => "delete",
        "del" => "forwarddelete",
        "uparrow" | "arrowup" => "up",
        "downarrow" | "arrowdown" => "down",
        "leftarrow" | "arrowleft" => "left",
        "rightarrow" | "arrowright" => "right",
        "plus" => "=",
        "minus" => "-",
        "comma" => ",",
        "period" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "semicolon" => ";",
        "quote" => "'",
        "backquote" | "grave" => "`",
        "leftbracket" => "[",
        "rightbracket" => "]",
        "space" | "tab" | "return" | "escape" | "delete" | "forwarddelete" | "capslock"
        | "home" | "end" | "pageup" | "pagedown" | "left" | "right" | "up" | "down" => value,
        _ if value
            .strip_prefix("keycode:")
            .and_then(|raw| raw.parse::<i64>().ok())
            .is_some() =>
        {
            value
        }
        _ if value
            .strip_prefix("vk:")
            .and_then(|raw| raw.parse::<u16>().ok())
            .is_some() =>
        {
            value
        }
        _ if value.len() == 1 => value,
        _ if value.starts_with('f')
            && value[1..]
                .parse::<u8>()
                .ok()
                .is_some_and(|n| (1..=24).contains(&n)) =>
        {
            value
        }
        _ => return None,
    };

    Some(key.to_string())
}

fn modifier_order(modifier: HotkeyModifier) -> u8 {
    match modifier {
        HotkeyModifier::Command => 0,
        HotkeyModifier::Control => 1,
        HotkeyModifier::Option => 2,
        HotkeyModifier::Shift => 3,
        HotkeyModifier::Fn => 4,
    }
}

fn label_part(value: &str) -> String {
    match value {
        "cmd" => "Cmd".to_string(),
        "ctrl" => "Ctrl".to_string(),
        "option" => "Option".to_string(),
        "shift" => "Shift".to_string(),
        "fn" => "fn".to_string(),
        "space" => "Space".to_string(),
        "return" => "Return".to_string(),
        "escape" => "Esc".to_string(),
        "delete" => "Delete".to_string(),
        "forwarddelete" => "Forward Delete".to_string(),
        "capslock" => "Caps Lock".to_string(),
        "pageup" => "Page Up".to_string(),
        "pagedown" => "Page Down".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "tab" => "Tab".to_string(),
        _ if value.starts_with("keycode:") => value
            .strip_prefix("keycode:")
            .map(|raw| format!("Key {raw}"))
            .unwrap_or_else(|| value.to_string()),
        _ if value.starts_with("vk:") => value
            .strip_prefix("vk:")
            .map(|raw| format!("Key {raw}"))
            .unwrap_or_else(|| value.to_string()),
        _ => value.to_string(),
    }
}

/// Double-tap detection window in seconds.
///
/// Keep this short so failed double-taps resolve quickly.
pub(crate) const DOUBLE_TAP_WINDOW: f64 = 0.3;
const MAX_TAP_DURATION: f64 = 0.5;

/// Set the callbacks before calling `start()`.
pub fn set_callbacks(
    on_key_down: impl Fn() + Send + 'static,
    on_key_up: impl Fn() + Send + 'static,
    on_double_tap: impl Fn() + Send + 'static,
) {
    if let Ok(mut cb) = CALLBACKS.lock() {
        cb.on_key_down = Some(Box::new(on_key_down));
        cb.on_key_up = Some(Box::new(on_key_up));
        cb.on_double_tap = Some(Box::new(on_double_tap));
    }
}

pub fn set_permission_required_callback(on_permission_required: impl Fn(String) + Send + 'static) {
    if let Ok(mut cb) = PERMISSION_CALLBACK.lock() {
        *cb = Some(Box::new(on_permission_required));
    }
}

pub fn begin_capture(
    on_preview: impl Fn(String) + Send + 'static,
    on_capture: impl Fn(String) + Send + 'static,
) {
    if let Ok(mut capture) = CAPTURE_CALLBACKS.lock() {
        *capture = Some(CaptureCallbacks {
            on_preview: Box::new(on_preview),
            on_capture: Box::new(on_capture),
        });
    }
    platform::reset_capture_state();
}

pub fn cancel_capture() {
    if let Ok(mut capture) = CAPTURE_CALLBACKS.lock() {
        *capture = None;
    }
    platform::reset_capture_state();
}

fn is_capturing() -> bool {
    CAPTURE_CALLBACKS
        .lock()
        .ok()
        .is_some_and(|capture| capture.is_some())
}

fn preview_capture(shortcut: String) {
    dispatch(RuntimeEvent::CapturePreview(shortcut));
}

fn finish_capture(shortcut: String) {
    let callback = CAPTURE_CALLBACKS
        .lock()
        .ok()
        .and_then(|mut capture| capture.take())
        .map(|callbacks| callbacks.on_capture);

    platform::reset_capture_state();

    if let Some(callback) = callback {
        dispatch(RuntimeEvent::CaptureFinish(callback, shortcut));
    }
}

fn notify_permission_required(label: String) {
    dispatch(RuntimeEvent::PermissionRequired(label));
}

// ---------------------------------------------------------------------------
// macOS implementation (raw CGEventTap via FFI)
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    // --- Raw CoreGraphics FFI bindings ---

    type CFDictionaryRef = *const std::ffi::c_void;
    type CGEventRef = *mut std::ffi::c_void;
    type CGEventTapProxy = *mut std::ffi::c_void;
    type CFMachPortRef = *mut std::ffi::c_void;
    type CFRunLoopSourceRef = *mut std::ffi::c_void;
    type CFRunLoopRef = *mut std::ffi::c_void;
    type CFAllocatorRef = *const std::ffi::c_void;
    type CFStringRef = *const std::ffi::c_void;

    type CGEventTapCallBack = unsafe extern "C" fn(
        proxy: CGEventTapProxy,
        event_type: u32,
        event: CGEventRef,
        user_info: *mut std::ffi::c_void,
    ) -> CGEventRef;

    // CGEventTapLocation
    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_ANNOTATED_SESSION_EVENT_TAP: u32 = 2;

    // CGEventTapPlacement
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;

    // CGEventTapOptions
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

    // CGEventType
    const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const K_CG_EVENT_KEY_UP: u32 = 11;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;
    const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFFFFFF;

    // CGEventField
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    // CGEventFlags
    const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 0x00020000;
    const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x00040000;
    const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 0x00080000;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;
    const K_CG_EVENT_FLAG_MASK_SECONDARY_FN: u64 = 0x00800000;

    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut std::ffi::c_void,
        ) -> CFMachPortRef;

        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

        fn CGEventGetFlags(event: CGEventRef) -> u64;

        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;

        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: i64,
        ) -> CFRunLoopSourceRef;

        fn CFRunLoopGetCurrent() -> CFRunLoopRef;

        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);

        fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);

        fn CFRunLoopRunInMode(
            mode: CFStringRef,
            seconds: f64,
            return_after_source_handled: u8,
        ) -> i32;

        fn CFDictionaryCreate(
            allocator: CFAllocatorRef,
            keys: *const *const std::ffi::c_void,
            values: *const *const std::ffi::c_void,
            num_values: isize,
            key_callbacks: *const std::ffi::c_void,
            value_callbacks: *const std::ffi::c_void,
        ) -> CFDictionaryRef;

        fn CFRelease(cf: *const std::ffi::c_void);

        fn AXIsProcessTrusted() -> bool;

        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;

        // kCFRunLoopCommonModes is an external C symbol
        static kCFRunLoopCommonModes: CFStringRef;
        static kCFRunLoopDefaultMode: CFStringRef;

        // kCFAllocatorDefault
        static kCFAllocatorDefault: CFAllocatorRef;

        static kCFBooleanTrue: *const std::ffi::c_void;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    /// Track key held state.
    static IS_HELD: AtomicBool = AtomicBool::new(false);

    /// Last key-up timestamp for double-tap detection (microseconds since epoch).
    static LAST_KEY_UP: AtomicU64 = AtomicU64::new(0);

    /// Timestamp when the current monitored key press began.
    static KEY_DOWN_AT: AtomicU64 = AtomicU64::new(0);

    /// Whether the current held key press already fired the double-tap callback.
    static CURRENT_PRESS_IS_DOUBLE_TAP: AtomicBool = AtomicBool::new(false);

    /// Required modifier flags for the configured shortcut.
    static REQUIRED_MODIFIER_FLAGS: AtomicU64 = AtomicU64::new(0);

    /// The configured non-modifier keycodes. Empty means modifier-only.
    static TRIGGER_KEYCODES: once_cell::sync::Lazy<Mutex<Vec<i64>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

    /// Currently held configured non-modifier keycodes.
    static PRESSED_TRIGGER_KEYCODES: once_cell::sync::Lazy<Mutex<Vec<i64>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

    /// Last modifier-only candidate observed while native shortcut capture is active.
    static CAPTURE_MODIFIER_FLAGS: AtomicU64 = AtomicU64::new(0);

    static CAPTURE_TRIGGER_KEYCODES: once_cell::sync::Lazy<Mutex<Vec<i64>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

    static CAPTURE_LAST_SHORTCUT: once_cell::sync::Lazy<Mutex<String>> =
        once_cell::sync::Lazy::new(|| Mutex::new(String::new()));

    const ALL_MODIFIER_FLAGS: u64 = K_CG_EVENT_FLAG_MASK_SHIFT
        | K_CG_EVENT_FLAG_MASK_CONTROL
        | K_CG_EVENT_FLAG_MASK_ALTERNATE
        | K_CG_EVENT_FLAG_MASK_COMMAND
        | K_CG_EVENT_FLAG_MASK_SECONDARY_FN;

    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Install a CGEventTap that listens for the configured shortcut.
    pub fn start(spec: HotkeySpec) {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return; // already running
        }

        let trigger_keycodes = spec
            .triggers
            .iter()
            .filter_map(|trigger| keycode_for_trigger(trigger))
            .collect::<Vec<_>>();
        if trigger_keycodes.len() != spec.triggers.len() {
            eprintln!(
                "[yap] HOTKEY FAILED: unsupported macOS shortcut {}",
                spec.label()
            );
            RUNNING.store(false, Ordering::SeqCst);
            return;
        }

        REQUIRED_MODIFIER_FLAGS.store(modifier_flags(&spec), Ordering::SeqCst);
        if let Ok(mut triggers) = TRIGGER_KEYCODES.lock() {
            *triggers = trigger_keycodes;
        }
        if let Ok(mut pressed) = PRESSED_TRIGGER_KEYCODES.lock() {
            pressed.clear();
        }
        IS_HELD.store(false, Ordering::SeqCst);
        LAST_KEY_UP.store(0, Ordering::SeqCst);
        KEY_DOWN_AT.store(0, Ordering::SeqCst);
        CURRENT_PRESS_IS_DOUBLE_TAP.store(false, Ordering::SeqCst);
        let generation = begin_listener_generation();

        if !accessibility_trusted(false) {
            eprintln!(
                "[yap] HOTKEY WAITING: grant Accessibility permission to enable {}",
                spec.label()
            );
            let label = spec.label();
            RUNNING.store(false, Ordering::SeqCst);
            notify_permission_required(label);
            return;
        }

        // Spawn a dedicated thread with its own CFRunLoop
        std::thread::Builder::new()
            .name("yap-hotkey".into())
            .spawn(move || {
                // Event mask: flagsChanged + keyDown + keyUp
                let event_mask: u64 = (1u64 << K_CG_EVENT_FLAGS_CHANGED)
                    | (1u64 << K_CG_EVENT_KEY_DOWN)
                    | (1u64 << K_CG_EVENT_KEY_UP);

                unsafe {
                    // Try HID-level tap first, fall back to session level
                    let mut tap = CGEventTapCreate(
                        K_CG_HID_EVENT_TAP,
                        K_CG_HEAD_INSERT_EVENT_TAP,
                        K_CG_EVENT_TAP_OPTION_DEFAULT,
                        event_mask,
                        hotkey_callback,
                        std::ptr::null_mut(),
                    );
                    if tap.is_null() {
                        tap = CGEventTapCreate(
                            K_CG_SESSION_EVENT_TAP,
                            K_CG_HEAD_INSERT_EVENT_TAP,
                            K_CG_EVENT_TAP_OPTION_DEFAULT,
                            event_mask,
                            hotkey_callback,
                            std::ptr::null_mut(),
                        );
                    }

                    if tap.is_null() {
                        eprintln!("[yap] HOTKEY FAILED: add this app to System Settings → Privacy & Security → Accessibility");
                        let label = spec.label();
                        RUNNING.store(false, Ordering::SeqCst);
                        notify_permission_required(label);
                        return;
                    }
                    eprintln!("[yap] Hotkey CGEventTap created successfully");

                    let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
                    if source.is_null() {
                        eprintln!("failed to create run loop source for event tap");
                        RUNNING.store(false, Ordering::SeqCst);
                        return;
                    }

                    let run_loop = CFRunLoopGetCurrent();
                    CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                    CGEventTapEnable(tap, true);

                    // Run the loop until stopped.
                    // MUST use kCFRunLoopDefaultMode here (not CommonModes — that's
                    // only valid for AddSource, not RunInMode).
                    while listener_generation_matches(generation) {
                        CFRunLoopRunInMode(
                            kCFRunLoopDefaultMode,
                            0.25, // check every 250ms if we should stop
                            0,    // returnAfterSourceHandled = false
                        );
                    }

                    // Cleanup
                    CGEventTapEnable(tap, false);
                    CFRunLoopRemoveSource(run_loop, source, kCFRunLoopCommonModes);
                    CFRelease(source as *const _);
                    CFRelease(tap as *const _);
                }
            })
            .expect("failed to spawn hotkey thread");
    }

    fn accessibility_trusted(prompt: bool) -> bool {
        if !prompt {
            return unsafe { AXIsProcessTrusted() };
        }

        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt as *const std::ffi::c_void];
            let values = [kCFBooleanTrue];
            let options = CFDictionaryCreate(
                kCFAllocatorDefault,
                keys.as_ptr(),
                values.as_ptr(),
                1,
                std::ptr::null(),
                std::ptr::null(),
            );

            let trusted = if options.is_null() {
                AXIsProcessTrusted()
            } else {
                AXIsProcessTrustedWithOptions(options)
            };

            if !options.is_null() {
                CFRelease(options);
            }

            trusted
        }
    }

    pub fn has_accessibility_permission() -> bool {
        accessibility_trusted(false)
    }

    pub fn request_accessibility_permission() -> bool {
        accessibility_trusted(true)
    }

    /// C callback for the CGEventTap.
    unsafe extern "C" fn hotkey_callback(
        _proxy: CGEventTapProxy,
        event_type: u32,
        event: CGEventRef,
        _user_info: *mut std::ffi::c_void,
    ) -> CGEventRef {
        // Re-enable if system disabled the tap
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            return event;
        }

        if is_capturing() {
            match capture_event(event_type, event) {
                CaptureAction::Captured(shortcut) => {
                    finish_capture(shortcut);
                    return std::ptr::null_mut();
                }
                CaptureAction::Consume => return std::ptr::null_mut(),
                CaptureAction::Ignore => {}
            }
        }

        let trigger_keycodes = TRIGGER_KEYCODES
            .lock()
            .ok()
            .map(|t| t.clone())
            .unwrap_or_default();
        let required_flags = REQUIRED_MODIFIER_FLAGS.load(Ordering::SeqCst);
        let is_modifier_only = trigger_keycodes.is_empty();

        // Suppress fn/Globe keyDown/keyUp that trigger the emoji picker.
        if event_type == K_CG_EVENT_KEY_DOWN || event_type == K_CG_EVENT_KEY_UP {
            if (required_flags & K_CG_EVENT_FLAG_MASK_SECONDARY_FN) != 0 {
                let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);
                if keycode == 63 || keycode == 179 {
                    return std::ptr::null_mut(); // consume event
                }
            }

            if is_modifier_only {
                return event;
            }

            let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);
            if !trigger_keycodes.contains(&keycode) {
                return event;
            }

            let flags = CGEventGetFlags(event);
            let modifiers_active = modifiers_match(flags, required_flags);

            if event_type == K_CG_EVENT_KEY_DOWN {
                mark_trigger_pressed(keycode);
                if modifiers_active
                    && all_triggers_pressed(&trigger_keycodes)
                    && !IS_HELD.load(Ordering::SeqCst)
                {
                    fire_key_down();
                }
                if IS_HELD.load(Ordering::SeqCst) {
                    return std::ptr::null_mut();
                }
            } else if event_type == K_CG_EVENT_KEY_UP && IS_HELD.load(Ordering::SeqCst) {
                unmark_trigger_pressed(keycode);
                fire_key_up();
                return std::ptr::null_mut();
            } else if event_type == K_CG_EVENT_KEY_UP {
                unmark_trigger_pressed(keycode);
            }

            return event;
        }

        if event_type != K_CG_EVENT_FLAGS_CHANGED {
            return event;
        }

        let flags = CGEventGetFlags(event);
        let modifiers_active = modifiers_match(flags, required_flags);

        if is_modifier_only {
            if modifiers_active && !IS_HELD.load(Ordering::SeqCst) {
                fire_key_down();
                return std::ptr::null_mut();
            } else if !modifiers_active && IS_HELD.load(Ordering::SeqCst) {
                fire_key_up();
                return std::ptr::null_mut();
            }
        } else if modifiers_active
            && all_triggers_pressed(&trigger_keycodes)
            && !IS_HELD.load(Ordering::SeqCst)
        {
            fire_key_down();
            return std::ptr::null_mut();
        } else if !modifiers_active && IS_HELD.load(Ordering::SeqCst) {
            fire_key_up();
            return std::ptr::null_mut();
        }

        event
    }

    enum CaptureAction {
        Captured(String),
        Consume,
        Ignore,
    }

    unsafe fn capture_event(event_type: u32, event: CGEventRef) -> CaptureAction {
        if event_type == K_CG_EVENT_FLAGS_CHANGED {
            let flags = CGEventGetFlags(event) & ALL_MODIFIER_FLAGS;
            let previous = CAPTURE_MODIFIER_FLAGS.swap(flags, Ordering::SeqCst);
            let has_triggers = CAPTURE_TRIGGER_KEYCODES
                .lock()
                .ok()
                .is_some_and(|triggers| !triggers.is_empty());

            if flags == 0 {
                if previous != 0 && !has_triggers {
                    return CaptureAction::Captured(
                        last_capture_shortcut()
                            .unwrap_or_else(|| shortcut_from_parts(previous, &[])),
                    );
                }
                return CaptureAction::Consume;
            }

            preview_current_capture();

            return CaptureAction::Consume;
        }

        if event_type == K_CG_EVENT_KEY_DOWN || event_type == K_CG_EVENT_KEY_UP {
            let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);

            if keycode == 63 || keycode == 179 {
                return CaptureAction::Consume;
            }

            if event_type == K_CG_EVENT_KEY_DOWN {
                if trigger_for_keycode(keycode).is_some() {
                    mark_capture_trigger_pressed(keycode);
                    preview_current_capture();
                }
                return CaptureAction::Consume;
            }

            if event_type == K_CG_EVENT_KEY_UP {
                unmark_capture_trigger_pressed(keycode);
                let flags = CGEventGetFlags(event) & ALL_MODIFIER_FLAGS;
                let has_triggers = CAPTURE_TRIGGER_KEYCODES
                    .lock()
                    .ok()
                    .is_some_and(|triggers| !triggers.is_empty());
                if flags == 0 && !has_triggers {
                    if let Some(shortcut) = last_capture_shortcut() {
                        return CaptureAction::Captured(shortcut);
                    }
                }
            }

            return CaptureAction::Consume;
        }

        CaptureAction::Ignore
    }

    fn shortcut_from_parts(flags: u64, triggers: &[String]) -> String {
        let mut parts = Vec::new();
        if (flags & K_CG_EVENT_FLAG_MASK_COMMAND) != 0 {
            parts.push("cmd");
        }
        if (flags & K_CG_EVENT_FLAG_MASK_CONTROL) != 0 {
            parts.push("ctrl");
        }
        if (flags & K_CG_EVENT_FLAG_MASK_ALTERNATE) != 0 {
            parts.push("option");
        }
        if (flags & K_CG_EVENT_FLAG_MASK_SHIFT) != 0 {
            parts.push("shift");
        }
        if (flags & K_CG_EVENT_FLAG_MASK_SECONDARY_FN) != 0 {
            parts.push("fn");
        }
        for trigger in triggers {
            parts.push(trigger);
        }

        parts.join("+")
    }

    fn preview_current_capture() {
        let shortcut = capture_shortcut();
        if !shortcut.is_empty() {
            if let Ok(mut last) = CAPTURE_LAST_SHORTCUT.lock() {
                if *last == shortcut {
                    return;
                }
                *last = shortcut.clone();
            }
            preview_capture(shortcut);
        }
    }

    fn last_capture_shortcut() -> Option<String> {
        CAPTURE_LAST_SHORTCUT
            .lock()
            .ok()
            .map(|shortcut| shortcut.clone())
            .filter(|shortcut| !shortcut.is_empty())
    }

    fn capture_shortcut() -> String {
        let flags = CAPTURE_MODIFIER_FLAGS.load(Ordering::SeqCst);
        let triggers = CAPTURE_TRIGGER_KEYCODES
            .lock()
            .ok()
            .map(|keycodes| {
                keycodes
                    .iter()
                    .filter_map(|keycode| trigger_for_keycode(*keycode))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        shortcut_from_parts(flags, &triggers)
    }

    fn mark_capture_trigger_pressed(keycode: i64) {
        if let Ok(mut triggers) = CAPTURE_TRIGGER_KEYCODES.lock() {
            if !triggers.contains(&keycode) {
                triggers.push(keycode);
            }
        }
    }

    fn unmark_capture_trigger_pressed(keycode: i64) {
        if let Ok(mut triggers) = CAPTURE_TRIGGER_KEYCODES.lock() {
            triggers.retain(|trigger| *trigger != keycode);
        }
    }

    fn fire_key_down() {
        IS_HELD.store(true, Ordering::SeqCst);

        let last_up = LAST_KEY_UP.load(Ordering::SeqCst);
        let now = now_micros();
        KEY_DOWN_AT.store(now, Ordering::SeqCst);
        CURRENT_PRESS_IS_DOUBLE_TAP.store(false, Ordering::SeqCst);
        let elapsed_secs = if last_up > 0 {
            (now.saturating_sub(last_up)) as f64 / 1_000_000.0
        } else {
            f64::MAX
        };

        if elapsed_secs < DOUBLE_TAP_WINDOW {
            LAST_KEY_UP.store(0, Ordering::SeqCst);
            CURRENT_PRESS_IS_DOUBLE_TAP.store(true, Ordering::SeqCst);
            dispatch(RuntimeEvent::DoubleTap);
        } else {
            dispatch(RuntimeEvent::KeyDown);
        }
    }

    fn fire_key_up() {
        IS_HELD.store(false, Ordering::SeqCst);
        let now = now_micros();
        let down_at = KEY_DOWN_AT.swap(0, Ordering::SeqCst);
        let held_secs = if down_at > 0 {
            (now.saturating_sub(down_at)) as f64 / 1_000_000.0
        } else {
            f64::MAX
        };
        let was_double_tap = CURRENT_PRESS_IS_DOUBLE_TAP.swap(false, Ordering::SeqCst);

        if !was_double_tap && held_secs <= MAX_TAP_DURATION {
            LAST_KEY_UP.store(now, Ordering::SeqCst);
        } else {
            LAST_KEY_UP.store(0, Ordering::SeqCst);
        }

        dispatch(RuntimeEvent::KeyUp);
    }

    fn modifiers_match(flags: u64, required: u64) -> bool {
        (flags & ALL_MODIFIER_FLAGS) == required
    }

    fn mark_trigger_pressed(keycode: i64) {
        if let Ok(mut pressed) = PRESSED_TRIGGER_KEYCODES.lock() {
            if !pressed.contains(&keycode) {
                pressed.push(keycode);
            }
        }
    }

    fn unmark_trigger_pressed(keycode: i64) {
        if let Ok(mut pressed) = PRESSED_TRIGGER_KEYCODES.lock() {
            pressed.retain(|pressed_keycode| *pressed_keycode != keycode);
        }
    }

    fn all_triggers_pressed(trigger_keycodes: &[i64]) -> bool {
        PRESSED_TRIGGER_KEYCODES.lock().ok().is_some_and(|pressed| {
            trigger_keycodes
                .iter()
                .all(|keycode| pressed.contains(keycode))
        })
    }

    fn modifier_flags(spec: &HotkeySpec) -> u64 {
        spec.modifiers.iter().fold(0, |flags, modifier| {
            flags
                | match modifier {
                    HotkeyModifier::Command => K_CG_EVENT_FLAG_MASK_COMMAND,
                    HotkeyModifier::Control => K_CG_EVENT_FLAG_MASK_CONTROL,
                    HotkeyModifier::Option => K_CG_EVENT_FLAG_MASK_ALTERNATE,
                    HotkeyModifier::Shift => K_CG_EVENT_FLAG_MASK_SHIFT,
                    HotkeyModifier::Fn => K_CG_EVENT_FLAG_MASK_SECONDARY_FN,
                }
        })
    }

    fn keycode_for_trigger(trigger: &str) -> Option<i64> {
        if let Some(raw) = trigger.strip_prefix("keycode:") {
            return raw.parse::<i64>().ok();
        }

        let keycode = match trigger {
            "a" => 0,
            "s" => 1,
            "d" => 2,
            "f" => 3,
            "h" => 4,
            "g" => 5,
            "z" => 6,
            "x" => 7,
            "c" => 8,
            "v" => 9,
            "b" => 11,
            "q" => 12,
            "w" => 13,
            "e" => 14,
            "r" => 15,
            "y" => 16,
            "t" => 17,
            "1" => 18,
            "2" => 19,
            "3" => 20,
            "4" => 21,
            "6" => 22,
            "5" => 23,
            "=" => 24,
            "9" => 25,
            "7" => 26,
            "-" => 27,
            "8" => 28,
            "0" => 29,
            "]" => 30,
            "o" => 31,
            "u" => 32,
            "[" => 33,
            "i" => 34,
            "p" => 35,
            "return" => 36,
            "l" => 37,
            "j" => 38,
            "'" => 39,
            "k" => 40,
            ";" => 41,
            "\\" => 42,
            "," => 43,
            "/" => 44,
            "n" => 45,
            "m" => 46,
            "." => 47,
            "tab" => 48,
            "space" => 49,
            "`" => 50,
            "delete" => 51,
            "escape" => 53,
            "capslock" => 57,
            "f17" => 64,
            "f18" => 79,
            "f19" => 80,
            "f20" => 90,
            "f5" => 96,
            "f6" => 97,
            "f7" => 98,
            "f3" => 99,
            "f8" => 100,
            "f9" => 101,
            "f11" => 103,
            "f13" => 105,
            "f16" => 106,
            "f14" => 107,
            "f10" => 109,
            "f12" => 111,
            "f15" => 113,
            "home" => 115,
            "pageup" => 116,
            "forwarddelete" => 117,
            "f4" => 118,
            "end" => 119,
            "f2" => 120,
            "pagedown" => 121,
            "f1" => 122,
            "left" => 123,
            "right" => 124,
            "down" => 125,
            "up" => 126,
            _ => return None,
        };

        Some(keycode)
    }

    /// Remove the event tap and clean up.
    pub fn stop() {
        cancel_listener_generation();
        if !RUNNING.swap(false, Ordering::SeqCst) {
            return; // not running
        }
        // The run loop thread will exit on the next iteration
        // since RUNNING is now false.
    }

    pub fn clear_tap_sequence() {
        LAST_KEY_UP.store(0, Ordering::SeqCst);
        KEY_DOWN_AT.store(0, Ordering::SeqCst);
        CURRENT_PRESS_IS_DOUBLE_TAP.store(false, Ordering::SeqCst);
    }

    pub fn reset_capture_state() {
        CAPTURE_MODIFIER_FLAGS.store(0, Ordering::SeqCst);
        if let Ok(mut triggers) = CAPTURE_TRIGGER_KEYCODES.lock() {
            triggers.clear();
        }
        if let Ok(mut last) = CAPTURE_LAST_SHORTCUT.lock() {
            last.clear();
        }
    }

    fn trigger_for_keycode(keycode: i64) -> Option<String> {
        let trigger = match keycode {
            0 => "a",
            1 => "s",
            2 => "d",
            3 => "f",
            4 => "h",
            5 => "g",
            6 => "z",
            7 => "x",
            8 => "c",
            9 => "v",
            11 => "b",
            12 => "q",
            13 => "w",
            14 => "e",
            15 => "r",
            16 => "y",
            17 => "t",
            18 => "1",
            19 => "2",
            20 => "3",
            21 => "4",
            22 => "6",
            23 => "5",
            24 => "=",
            25 => "9",
            26 => "7",
            27 => "-",
            28 => "8",
            29 => "0",
            30 => "]",
            31 => "o",
            32 => "u",
            33 => "[",
            34 => "i",
            35 => "p",
            36 => "return",
            37 => "l",
            38 => "j",
            39 => "'",
            40 => "k",
            41 => ";",
            42 => "\\",
            43 => ",",
            44 => "/",
            45 => "n",
            46 => "m",
            47 => ".",
            48 => "tab",
            49 => "space",
            50 => "`",
            51 => "delete",
            53 => "escape",
            57 => "capslock",
            64 => "f17",
            79 => "f18",
            80 => "f19",
            90 => "f20",
            96 => "f5",
            97 => "f6",
            98 => "f7",
            99 => "f3",
            100 => "f8",
            101 => "f9",
            103 => "f11",
            105 => "f13",
            106 => "f16",
            107 => "f14",
            109 => "f10",
            111 => "f12",
            113 => "f15",
            115 => "home",
            116 => "pageup",
            117 => "forwarddelete",
            118 => "f4",
            119 => "end",
            120 => "f2",
            121 => "pagedown",
            122 => "f1",
            123 => "left",
            124 => "right",
            125 => "down",
            126 => "up",
            _ => return Some(format!("keycode:{keycode}")),
        };

        Some(trigger.to_string())
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
        WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    /// Track key held state.
    static IS_HELD: AtomicBool = AtomicBool::new(false);

    /// Last key-up timestamp for double-tap detection.
    static LAST_KEY_UP: AtomicU64 = AtomicU64::new(0);

    /// Timestamp when the current monitored key press began.
    static KEY_DOWN_AT: AtomicU64 = AtomicU64::new(0);

    /// Whether the current held key press already fired the double-tap callback.
    static CURRENT_PRESS_IS_DOUBLE_TAP: AtomicBool = AtomicBool::new(false);

    /// Required modifiers for the configured shortcut.
    static REQUIRED_MODIFIERS: AtomicU64 = AtomicU64::new(0);

    /// Currently pressed modifiers observed by the keyboard hook.
    static PRESSED_MODIFIERS: AtomicU64 = AtomicU64::new(0);

    /// Last modifier-only candidate observed while native shortcut capture is active.
    static CAPTURE_MODIFIERS: AtomicU64 = AtomicU64::new(0);

    /// The configured non-modifier virtual key codes. Empty means modifier-only.
    static TARGET_VKS: once_cell::sync::Lazy<Mutex<Vec<u16>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

    /// Currently held configured non-modifier virtual key codes.
    static PRESSED_TARGET_VKS: once_cell::sync::Lazy<Mutex<Vec<u16>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

    static CAPTURE_TARGET_VKS: once_cell::sync::Lazy<Mutex<Vec<u16>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

    static CAPTURE_LAST_SHORTCUT: once_cell::sync::Lazy<Mutex<String>> =
        once_cell::sync::Lazy::new(|| Mutex::new(String::new()));

    /// Thread ID of the hook thread (for PostThreadMessageW).
    static HOOK_THREAD_ID: AtomicU64 = AtomicU64::new(0);

    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    const MOD_COMMAND: u64 = 1 << 0;
    const MOD_CONTROL: u64 = 1 << 1;
    const MOD_OPTION: u64 = 1 << 2;
    const MOD_SHIFT: u64 = 1 << 3;
    const WINDOWS_FN_VK: u16 = 0x87; // VK_F24; common Windows stand-in for Fn/Globe.

    /// Install a low-level keyboard hook (WH_KEYBOARD_LL).
    pub fn start(spec: HotkeySpec) {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        let mut target_vks = spec
            .triggers
            .iter()
            .filter_map(|trigger| vk_for_trigger(trigger))
            .collect::<Vec<_>>();
        if target_vks.len() != spec.triggers.len() {
            eprintln!(
                "[yap] HOTKEY FAILED: unsupported Windows shortcut {}",
                spec.label()
            );
            RUNNING.store(false, Ordering::SeqCst);
            return;
        }

        // Windows does not expose laptop Fn keys consistently. Keyboards that
        // do expose a user-space Fn/Globe-style key commonly report VK_F24.
        // Treat configured `fn` as that physical target, not as a zero-bit
        // modifier, so `fn` and `fn+key` can produce real down/up events.
        if spec.modifiers.contains(&HotkeyModifier::Fn) && !target_vks.contains(&WINDOWS_FN_VK) {
            target_vks.insert(0, WINDOWS_FN_VK);
        }

        if let Ok(mut targets) = TARGET_VKS.lock() {
            *targets = target_vks;
        }
        if let Ok(mut pressed) = PRESSED_TARGET_VKS.lock() {
            pressed.clear();
        }
        REQUIRED_MODIFIERS.store(modifier_bits(&spec), Ordering::SeqCst);
        PRESSED_MODIFIERS.store(0, Ordering::SeqCst);
        IS_HELD.store(false, Ordering::SeqCst);
        LAST_KEY_UP.store(0, Ordering::SeqCst);
        KEY_DOWN_AT.store(0, Ordering::SeqCst);
        CURRENT_PRESS_IS_DOUBLE_TAP.store(false, Ordering::SeqCst);
        let generation = begin_listener_generation();

        std::thread::Builder::new()
            .name("yap-hotkey".into())
            .spawn(move || {
                // Store our thread ID for clean shutdown
                unsafe {
                    let tid = windows::Win32::System::Threading::GetCurrentThreadId();
                    HOOK_THREAD_ID.store(tid as u64, Ordering::SeqCst);
                }

                unsafe {
                    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0);

                    match hook {
                        Ok(_h) => {}
                        Err(e) => {
                            eprintln!("failed to set keyboard hook: {e}");
                            RUNNING.store(false, Ordering::SeqCst);
                            return;
                        }
                    }

                    // Message pump
                    let mut msg = MSG::default();
                    while listener_generation_matches(generation) {
                        let ret = GetMessageW(&mut msg, None, 0, 0);
                        if ret.0 <= 0 {
                            break;
                        }
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    // Cleanup
                    if let Ok(h) = hook {
                        let _ = UnhookWindowsHookEx(h);
                    }
                }
            })
            .expect("failed to spawn hotkey thread");
    }

    /// Low-level keyboard hook procedure.
    unsafe extern "system" fn keyboard_hook_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code < 0 {
            return CallNextHookEx(None, n_code, w_param, l_param);
        }

        let kb = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        let vk_code = kb.vkCode as u16;
        let msg = w_param.0 as u32;
        let is_key_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_key_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        let targets = TARGET_VKS
            .lock()
            .ok()
            .map(|targets| targets.clone())
            .unwrap_or_default();
        let is_modifier_only = targets.is_empty();

        if is_capturing() {
            match capture_event(vk_code, is_key_down, is_key_up) {
                CaptureAction::Captured(shortcut) => {
                    finish_capture(shortcut);
                    return LRESULT(1);
                }
                CaptureAction::Consume => return LRESULT(1),
                CaptureAction::Ignore => {}
            }
        }

        if let Some(modifier) = modifier_bit_for_vk(vk_code) {
            if is_key_down {
                PRESSED_MODIFIERS.fetch_or(modifier, Ordering::SeqCst);
            } else if is_key_up {
                PRESSED_MODIFIERS.fetch_and(!modifier, Ordering::SeqCst);
            }

            let modifiers_active = modifiers_match();
            if is_modifier_only {
                if modifiers_active && !IS_HELD.load(Ordering::SeqCst) {
                    fire_key_down();
                    return LRESULT(1);
                } else if !modifiers_active && IS_HELD.load(Ordering::SeqCst) {
                    fire_key_up();
                    return LRESULT(1);
                }
            } else if modifiers_active
                && all_targets_pressed(&targets)
                && !IS_HELD.load(Ordering::SeqCst)
            {
                fire_key_down();
                return LRESULT(1);
            } else if !modifiers_active && IS_HELD.load(Ordering::SeqCst) {
                fire_key_up();
                return LRESULT(1);
            }

            return CallNextHookEx(None, n_code, w_param, l_param);
        }

        if is_modifier_only || !targets.contains(&vk_code) {
            return CallNextHookEx(None, n_code, w_param, l_param);
        }

        if is_key_down {
            mark_target_pressed(vk_code);
            if modifiers_match() && all_targets_pressed(&targets) && !IS_HELD.load(Ordering::SeqCst)
            {
                fire_key_down();
            }
            if IS_HELD.load(Ordering::SeqCst) {
                return LRESULT(1);
            }
        } else if is_key_up && IS_HELD.load(Ordering::SeqCst) {
            unmark_target_pressed(vk_code);
            fire_key_up();
            return LRESULT(1);
        } else if is_key_up {
            unmark_target_pressed(vk_code);
        }

        CallNextHookEx(None, n_code, w_param, l_param)
    }

    enum CaptureAction {
        Captured(String),
        Consume,
        Ignore,
    }

    fn capture_event(vk_code: u16, is_key_down: bool, is_key_up: bool) -> CaptureAction {
        if let Some(modifier) = modifier_bit_for_vk(vk_code) {
            if is_key_down {
                CAPTURE_MODIFIERS.fetch_or(modifier, Ordering::SeqCst);
                preview_current_capture();
                return CaptureAction::Consume;
            }

            if is_key_up {
                let previous = CAPTURE_MODIFIERS.load(Ordering::SeqCst);
                CAPTURE_MODIFIERS.fetch_and(!modifier, Ordering::SeqCst);
                let has_triggers = CAPTURE_TARGET_VKS
                    .lock()
                    .ok()
                    .is_some_and(|targets| !targets.is_empty());
                if previous != 0 && !has_triggers {
                    return CaptureAction::Captured(
                        last_capture_shortcut()
                            .unwrap_or_else(|| shortcut_from_parts(previous, &[])),
                    );
                }
                return CaptureAction::Consume;
            }
        }

        if is_key_down {
            if trigger_for_vk(vk_code).is_some() {
                mark_capture_target_pressed(vk_code);
                preview_current_capture();
            }
            return CaptureAction::Consume;
        }

        if is_key_up {
            unmark_capture_target_pressed(vk_code);
            let modifiers = CAPTURE_MODIFIERS.load(Ordering::SeqCst);
            let has_triggers = CAPTURE_TARGET_VKS
                .lock()
                .ok()
                .is_some_and(|targets| !targets.is_empty());
            if modifiers == 0 && !has_triggers {
                if let Some(shortcut) = last_capture_shortcut() {
                    return CaptureAction::Captured(shortcut);
                }
            }
            return CaptureAction::Consume;
        }

        CaptureAction::Ignore
    }

    fn shortcut_from_parts(modifiers: u64, triggers: &[String]) -> String {
        let mut parts = Vec::new();
        if (modifiers & MOD_COMMAND) != 0 {
            parts.push("cmd");
        }
        if (modifiers & MOD_CONTROL) != 0 {
            parts.push("ctrl");
        }
        if (modifiers & MOD_OPTION) != 0 {
            parts.push("option");
        }
        if (modifiers & MOD_SHIFT) != 0 {
            parts.push("shift");
        }
        for trigger in triggers {
            parts.push(trigger);
        }

        parts.join("+")
    }

    fn preview_current_capture() {
        let shortcut = capture_shortcut();
        if !shortcut.is_empty() {
            if let Ok(mut last) = CAPTURE_LAST_SHORTCUT.lock() {
                if *last == shortcut {
                    return;
                }
                *last = shortcut.clone();
            }
            preview_capture(shortcut);
        }
    }

    fn last_capture_shortcut() -> Option<String> {
        CAPTURE_LAST_SHORTCUT
            .lock()
            .ok()
            .map(|shortcut| shortcut.clone())
            .filter(|shortcut| !shortcut.is_empty())
    }

    fn capture_shortcut() -> String {
        let modifiers = CAPTURE_MODIFIERS.load(Ordering::SeqCst);
        let triggers = CAPTURE_TARGET_VKS
            .lock()
            .ok()
            .map(|targets| {
                targets
                    .iter()
                    .filter_map(|vk| trigger_for_vk(*vk))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        shortcut_from_parts(modifiers, &triggers)
    }

    fn mark_capture_target_pressed(vk: u16) {
        if let Ok(mut targets) = CAPTURE_TARGET_VKS.lock() {
            if !targets.contains(&vk) {
                targets.push(vk);
            }
        }
    }

    fn unmark_capture_target_pressed(vk: u16) {
        if let Ok(mut targets) = CAPTURE_TARGET_VKS.lock() {
            targets.retain(|target| *target != vk);
        }
    }

    fn fire_key_down() {
        IS_HELD.store(true, Ordering::SeqCst);

        let last_up = LAST_KEY_UP.load(Ordering::SeqCst);
        let now = now_micros();
        KEY_DOWN_AT.store(now, Ordering::SeqCst);
        CURRENT_PRESS_IS_DOUBLE_TAP.store(false, Ordering::SeqCst);
        let elapsed_secs = if last_up > 0 {
            (now.saturating_sub(last_up)) as f64 / 1_000_000.0
        } else {
            f64::MAX
        };

        if elapsed_secs < DOUBLE_TAP_WINDOW {
            LAST_KEY_UP.store(0, Ordering::SeqCst);
            CURRENT_PRESS_IS_DOUBLE_TAP.store(true, Ordering::SeqCst);
            dispatch(RuntimeEvent::DoubleTap);
        } else {
            dispatch(RuntimeEvent::KeyDown);
        }
    }

    fn fire_key_up() {
        IS_HELD.store(false, Ordering::SeqCst);
        let now = now_micros();
        let down_at = KEY_DOWN_AT.swap(0, Ordering::SeqCst);
        let held_secs = if down_at > 0 {
            (now.saturating_sub(down_at)) as f64 / 1_000_000.0
        } else {
            f64::MAX
        };
        let was_double_tap = CURRENT_PRESS_IS_DOUBLE_TAP.swap(false, Ordering::SeqCst);

        if !was_double_tap && held_secs <= MAX_TAP_DURATION {
            LAST_KEY_UP.store(now, Ordering::SeqCst);
        } else {
            LAST_KEY_UP.store(0, Ordering::SeqCst);
        }

        dispatch(RuntimeEvent::KeyUp);
    }

    fn modifiers_match() -> bool {
        PRESSED_MODIFIERS.load(Ordering::SeqCst) == REQUIRED_MODIFIERS.load(Ordering::SeqCst)
    }

    fn mark_target_pressed(vk: u16) {
        if let Ok(mut pressed) = PRESSED_TARGET_VKS.lock() {
            if !pressed.contains(&vk) {
                pressed.push(vk);
            }
        }
    }

    fn unmark_target_pressed(vk: u16) {
        if let Ok(mut pressed) = PRESSED_TARGET_VKS.lock() {
            pressed.retain(|pressed_vk| *pressed_vk != vk);
        }
    }

    fn all_targets_pressed(targets: &[u16]) -> bool {
        PRESSED_TARGET_VKS
            .lock()
            .ok()
            .is_some_and(|pressed| targets.iter().all(|target| pressed.contains(target)))
    }

    fn modifier_bits(spec: &HotkeySpec) -> u64 {
        spec.modifiers.iter().fold(0, |bits, modifier| {
            bits | match modifier {
                HotkeyModifier::Command => MOD_COMMAND,
                HotkeyModifier::Control => MOD_CONTROL,
                HotkeyModifier::Option => MOD_OPTION,
                HotkeyModifier::Shift => MOD_SHIFT,
                HotkeyModifier::Fn => 0,
            }
        })
    }

    fn modifier_bit_for_vk(vk: u16) -> Option<u64> {
        match vk {
            0x5B | 0x5C => Some(MOD_COMMAND), // Windows keys
            0x11 | 0xA2 | 0xA3 => Some(MOD_CONTROL),
            0x12 | 0xA4 | 0xA5 => Some(MOD_OPTION),
            0x10 | 0xA0 | 0xA1 => Some(MOD_SHIFT),
            _ => None,
        }
    }

    fn vk_for_trigger(trigger: &str) -> Option<u16> {
        if let Some(raw) = trigger.strip_prefix("vk:") {
            return raw.parse::<u16>().ok();
        }

        let vk = match trigger {
            "a" => 0x41,
            "b" => 0x42,
            "c" => 0x43,
            "d" => 0x44,
            "e" => 0x45,
            "f" => 0x46,
            "g" => 0x47,
            "h" => 0x48,
            "i" => 0x49,
            "j" => 0x4A,
            "k" => 0x4B,
            "l" => 0x4C,
            "m" => 0x4D,
            "n" => 0x4E,
            "o" => 0x4F,
            "p" => 0x50,
            "q" => 0x51,
            "r" => 0x52,
            "s" => 0x53,
            "t" => 0x54,
            "u" => 0x55,
            "v" => 0x56,
            "w" => 0x57,
            "x" => 0x58,
            "y" => 0x59,
            "z" => 0x5A,
            "0" => 0x30,
            "1" => 0x31,
            "2" => 0x32,
            "3" => 0x33,
            "4" => 0x34,
            "5" => 0x35,
            "6" => 0x36,
            "7" => 0x37,
            "8" => 0x38,
            "9" => 0x39,
            "space" => 0x20,
            "tab" => 0x09,
            "return" => 0x0D,
            "escape" => 0x1B,
            "delete" => 0x08,
            "forwarddelete" => 0x2E,
            "capslock" => 0x14,
            "left" => 0x25,
            "up" => 0x26,
            "right" => 0x27,
            "down" => 0x28,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" => 0x21,
            "pagedown" => 0x22,
            ";" => 0xBA,
            "=" => 0xBB,
            "," => 0xBC,
            "-" => 0xBD,
            "." => 0xBE,
            "/" => 0xBF,
            "`" => 0xC0,
            "[" => 0xDB,
            "\\" => 0xDC,
            "]" => 0xDD,
            "'" => 0xDE,
            _ if trigger.starts_with('f') => {
                let n = trigger[1..].parse::<u16>().ok()?;
                if (1..=24).contains(&n) {
                    0x70 + n - 1
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        Some(vk)
    }

    /// Remove the keyboard hook.
    pub fn stop() {
        cancel_listener_generation();
        if !RUNNING.swap(false, Ordering::SeqCst) {
            return;
        }
        // Post WM_QUIT to unblock GetMessageW
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid as u32, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn clear_tap_sequence() {
        LAST_KEY_UP.store(0, Ordering::SeqCst);
        KEY_DOWN_AT.store(0, Ordering::SeqCst);
        CURRENT_PRESS_IS_DOUBLE_TAP.store(false, Ordering::SeqCst);
    }

    pub fn reset_capture_state() {
        CAPTURE_MODIFIERS.store(0, Ordering::SeqCst);
        if let Ok(mut targets) = CAPTURE_TARGET_VKS.lock() {
            targets.clear();
        }
        if let Ok(mut last) = CAPTURE_LAST_SHORTCUT.lock() {
            last.clear();
        }
    }

    fn trigger_for_vk(vk: u16) -> Option<String> {
        let trigger = match vk {
            0x41 => "a",
            0x42 => "b",
            0x43 => "c",
            0x44 => "d",
            0x45 => "e",
            0x46 => "f",
            0x47 => "g",
            0x48 => "h",
            0x49 => "i",
            0x4A => "j",
            0x4B => "k",
            0x4C => "l",
            0x4D => "m",
            0x4E => "n",
            0x4F => "o",
            0x50 => "p",
            0x51 => "q",
            0x52 => "r",
            0x53 => "s",
            0x54 => "t",
            0x55 => "u",
            0x56 => "v",
            0x57 => "w",
            0x58 => "x",
            0x59 => "y",
            0x5A => "z",
            0x30 => "0",
            0x31 => "1",
            0x32 => "2",
            0x33 => "3",
            0x34 => "4",
            0x35 => "5",
            0x36 => "6",
            0x37 => "7",
            0x38 => "8",
            0x39 => "9",
            0x20 => "space",
            0x09 => "tab",
            0x0D => "return",
            0x1B => "escape",
            0x08 => "delete",
            0x2E => "forwarddelete",
            0x14 => "capslock",
            0x25 => "left",
            0x26 => "up",
            0x27 => "right",
            0x28 => "down",
            0x24 => "home",
            0x23 => "end",
            0x21 => "pageup",
            0x22 => "pagedown",
            0xBA => ";",
            0xBB => "=",
            0xBC => ",",
            0xBD => "-",
            0xBE => ".",
            0xBF => "/",
            0xC0 => "`",
            0xDB => "[",
            0xDC => "\\",
            0xDD => "]",
            0xDE => "'",
            0x70 => "f1",
            0x71 => "f2",
            0x72 => "f3",
            0x73 => "f4",
            0x74 => "f5",
            0x75 => "f6",
            0x76 => "f7",
            0x77 => "f8",
            0x78 => "f9",
            0x79 => "f10",
            0x7A => "f11",
            0x7B => "f12",
            0x7C => "f13",
            0x7D => "f14",
            0x7E => "f15",
            0x7F => "f16",
            0x80 => "f17",
            0x81 => "f18",
            0x82 => "f19",
            0x83 => "f20",
            0x84 => "f21",
            0x85 => "f22",
            0x86 => "f23",
            WINDOWS_FN_VK => "fn",
            _ => return Some(format!("vk:{vk}")),
        };

        Some(trigger.to_string())
    }
}

// ---------------------------------------------------------------------------
// Public API (delegates to platform)
// ---------------------------------------------------------------------------

/// Begin listening for the hotkey shortcut. Safe to call multiple times.
pub fn start(spec: HotkeySpec) {
    platform::start(spec);
}

/// Stop listening. Safe to call when already stopped.
pub fn stop() {
    platform::stop();
}

/// Clear any pending first-tap candidate.
pub fn clear_tap_sequence() {
    platform::clear_tap_sequence();
}

#[cfg(target_os = "macos")]
pub fn has_accessibility_permission() -> bool {
    platform::has_accessibility_permission()
}

#[cfg(not(target_os = "macos"))]
pub fn has_accessibility_permission() -> bool {
    true
}

#[cfg(target_os = "macos")]
pub fn request_accessibility_permission() -> bool {
    platform::request_accessibility_permission()
}

#[cfg(not(target_os = "macos"))]
pub fn request_accessibility_permission() -> bool {
    true
}
