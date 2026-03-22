// This module is a public API; many items are not yet wired into commands.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Callback type for hotkey events.
type HotkeyCallback = Box<dyn Fn() + Send + 'static>;

/// Stored callbacks for hotkey events (set by the caller before start).
static CALLBACKS: once_cell::sync::Lazy<Mutex<HotkeyCallbacks>> =
    once_cell::sync::Lazy::new(|| {
        Mutex::new(HotkeyCallbacks {
            on_key_down: None,
            on_key_up: None,
            on_double_tap: None,
        })
    });

struct HotkeyCallbacks {
    on_key_down: Option<HotkeyCallback>,
    on_key_up: Option<HotkeyCallback>,
    on_double_tap: Option<HotkeyCallback>,
}

// Safety: the callbacks are only invoked from the hotkey thread, and we
// protect access via a Mutex.
unsafe impl Send for HotkeyCallbacks {}

/// Hotkey modifier the user has configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyModifier {
    Fn,
    Option,
}

/// Double-tap detection window in seconds.
const DOUBLE_TAP_WINDOW: f64 = 0.35;

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

// ---------------------------------------------------------------------------
// macOS implementation (raw CGEventTap via FFI)
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    // --- Raw CoreGraphics FFI bindings ---

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

        fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: u8) -> i32;

        fn CFRelease(cf: *const std::ffi::c_void);

        // kCFRunLoopCommonModes is an external C symbol
        static kCFRunLoopCommonModes: CFStringRef;

        // kCFAllocatorDefault
        static kCFAllocatorDefault: CFAllocatorRef;
    }

    /// Track key held state.
    static IS_HELD: AtomicBool = AtomicBool::new(false);

    /// Last key-up timestamp for double-tap detection (microseconds since epoch).
    static LAST_KEY_UP: AtomicU64 = AtomicU64::new(0);

    /// The modifier mask we're monitoring (raw u64 flags).
    static MODIFIER_MASK: AtomicU64 = AtomicU64::new(0);

    /// Whether we're monitoring the fn/Globe key.
    static IS_FN_KEY: AtomicBool = AtomicBool::new(false);

    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Install a CGEventTap that listens for the configured modifier key.
    pub fn start(modifier: HotkeyModifier) {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return; // already running
        }

        let (mask, is_fn) = match modifier {
            HotkeyModifier::Fn => (K_CG_EVENT_FLAG_MASK_SECONDARY_FN, true),
            HotkeyModifier::Option => (K_CG_EVENT_FLAG_MASK_ALTERNATE, false),
        };

        MODIFIER_MASK.store(mask, Ordering::SeqCst);
        IS_FN_KEY.store(is_fn, Ordering::SeqCst);
        IS_HELD.store(false, Ordering::SeqCst);
        LAST_KEY_UP.store(0, Ordering::SeqCst);

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
                        RUNNING.store(false, Ordering::SeqCst);
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

                    // Run the loop until stopped
                    while RUNNING.load(Ordering::SeqCst) {
                        CFRunLoopRunInMode(
                            kCFRunLoopCommonModes,
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

        // Suppress fn/Globe keyDown/keyUp that trigger the emoji picker
        if event_type == K_CG_EVENT_KEY_DOWN || event_type == K_CG_EVENT_KEY_UP {
            if IS_FN_KEY.load(Ordering::SeqCst) {
                let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);
                if keycode == 63 || keycode == 179 {
                    return std::ptr::null_mut(); // consume event
                }
            }
            return event;
        }

        if event_type != K_CG_EVENT_FLAGS_CHANGED {
            return event;
        }

        let mask = MODIFIER_MASK.load(Ordering::SeqCst);
        let flags = CGEventGetFlags(event);
        let trigger_active = (flags & mask) != 0;

        // Only trigger if no other modifiers are held
        let other_modifiers = K_CG_EVENT_FLAG_MASK_SHIFT
            | K_CG_EVENT_FLAG_MASK_CONTROL
            | K_CG_EVENT_FLAG_MASK_ALTERNATE
            | K_CG_EVENT_FLAG_MASK_COMMAND;
        let relevant_others = flags & other_modifiers & !mask;
        let has_other_modifiers = relevant_others != 0;

        if trigger_active && !has_other_modifiers && !IS_HELD.load(Ordering::SeqCst) {
            IS_HELD.store(true, Ordering::SeqCst);

            let last_up = LAST_KEY_UP.load(Ordering::SeqCst);
            let now = now_micros();
            let elapsed_secs = if last_up > 0 {
                (now.saturating_sub(last_up)) as f64 / 1_000_000.0
            } else {
                f64::MAX
            };

            if elapsed_secs < DOUBLE_TAP_WINDOW {
                LAST_KEY_UP.store(0, Ordering::SeqCst);
                if let Ok(cb) = CALLBACKS.lock() {
                    if let Some(ref f) = cb.on_double_tap {
                        f();
                    }
                }
            } else {
                if let Ok(cb) = CALLBACKS.lock() {
                    if let Some(ref f) = cb.on_key_down {
                        f();
                    }
                }
            }

            return std::ptr::null_mut(); // consume event
        } else if !trigger_active && IS_HELD.load(Ordering::SeqCst) {
            IS_HELD.store(false, Ordering::SeqCst);
            LAST_KEY_UP.store(now_micros(), Ordering::SeqCst);

            if let Ok(cb) = CALLBACKS.lock() {
                if let Some(ref f) = cb.on_key_up {
                    f();
                }
            }

            return std::ptr::null_mut(); // consume release too
        }

        event
    }

    /// Remove the event tap and clean up.
    pub fn stop() {
        if !RUNNING.swap(false, Ordering::SeqCst) {
            return; // not running
        }
        // The run loop thread will exit on the next iteration
        // since RUNNING is now false.
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
        TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    /// Track key held state.
    static IS_HELD: AtomicBool = AtomicBool::new(false);

    /// Last key-up timestamp for double-tap detection.
    static LAST_KEY_UP: AtomicU64 = AtomicU64::new(0);

    /// The virtual key code we're monitoring (default: CapsLock = 0x14).
    static TARGET_VK: Mutex<u16> = Mutex::new(0x14);

    /// Thread ID of the hook thread (for PostThreadMessageW).
    static HOOK_THREAD_ID: AtomicU64 = AtomicU64::new(0);

    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Install a low-level keyboard hook (WH_KEYBOARD_LL).
    pub fn start(modifier: HotkeyModifier) {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        // Map modifier to VK code
        let vk = match modifier {
            HotkeyModifier::Fn => 0x14u16,    // CapsLock as fn equivalent on Windows
            HotkeyModifier::Option => 0xA4u16, // VK_LMENU (Left Alt)
        };
        if let Ok(mut target) = TARGET_VK.lock() {
            *target = vk;
        }
        IS_HELD.store(false, Ordering::SeqCst);
        LAST_KEY_UP.store(0, Ordering::SeqCst);

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
                    while RUNNING.load(Ordering::SeqCst) {
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
        let target = TARGET_VK.lock().ok().map(|t| *t).unwrap_or(0x14);

        if vk_code != target {
            return CallNextHookEx(None, n_code, w_param, l_param);
        }

        let msg = w_param.0 as u32;
        let is_key_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_key_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if is_key_down && !IS_HELD.load(Ordering::SeqCst) {
            IS_HELD.store(true, Ordering::SeqCst);

            let last_up = LAST_KEY_UP.load(Ordering::SeqCst);
            let now = now_micros();
            let elapsed_secs = if last_up > 0 {
                (now.saturating_sub(last_up)) as f64 / 1_000_000.0
            } else {
                f64::MAX
            };

            if elapsed_secs < DOUBLE_TAP_WINDOW {
                LAST_KEY_UP.store(0, Ordering::SeqCst);
                if let Ok(cb) = CALLBACKS.lock() {
                    if let Some(ref f) = cb.on_double_tap {
                        f();
                    }
                }
            } else {
                if let Ok(cb) = CALLBACKS.lock() {
                    if let Some(ref f) = cb.on_key_down {
                        f();
                    }
                }
            }

            return LRESULT(1); // consume event
        } else if is_key_up && IS_HELD.load(Ordering::SeqCst) {
            IS_HELD.store(false, Ordering::SeqCst);
            LAST_KEY_UP.store(now_micros(), Ordering::SeqCst);

            if let Ok(cb) = CALLBACKS.lock() {
                if let Some(ref f) = cb.on_key_up {
                    f();
                }
            }

            return LRESULT(1); // consume release too
        }

        CallNextHookEx(None, n_code, w_param, l_param)
    }

    /// Remove the keyboard hook.
    pub fn stop() {
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
}

// ---------------------------------------------------------------------------
// Public API (delegates to platform)
// ---------------------------------------------------------------------------

/// Begin listening for the hotkey modifier. Safe to call multiple times.
pub fn start(modifier: HotkeyModifier) {
    platform::start(modifier);
}

/// Stop listening. Safe to call when already stopped.
pub fn stop() {
    platform::stop();
}
