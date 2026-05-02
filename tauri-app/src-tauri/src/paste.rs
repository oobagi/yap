use std::thread;
use std::time::Duration;

/// Write `text` to the system clipboard, simulate Cmd+V (macOS) or Ctrl+V
/// (Windows), then restore the previous clipboard contents.
///
/// Sequence (from spec):
///   1. Save current clipboard string
///   2. Clear clipboard, set `text`
///   3. Wait 50 ms
///   4. Simulate paste keystroke
///   5. Wait 300 ms
///   6. Restore previous clipboard (or leave empty)
pub fn paste_text(text: &str) -> Result<(), String> {
    // --- Step 1: Save previous clipboard ---
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = clipboard.get_text().ok();

    // --- Step 2: Set new text ---
    clipboard
        .set_text(text.to_string())
        .map_err(|e| e.to_string())?;

    // --- Step 3: Wait for clipboard readiness ---
    thread::sleep(Duration::from_millis(50));

    // --- Step 4: Simulate paste keystroke ---
    simulate_paste()?;

    // --- Step 5: Wait for paste to complete ---
    thread::sleep(Duration::from_millis(300));

    // --- Step 6: Restore previous clipboard ---
    match previous {
        Some(prev) => clipboard.set_text(prev).map_err(|e| e.to_string())?,
        None => clipboard.clear().map_err(|e| e.to_string())?,
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Platform-specific keystroke simulation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn simulate_paste() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "failed to create CGEventSource".to_string())?;

    // Virtual key 0x09 = 'V' on US keyboard layout
    let v_keycode: CGKeyCode = 0x09;

    let key_down = CGEvent::new_keyboard_event(source.clone(), v_keycode, true)
        .map_err(|_| "failed to create key down event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(source, v_keycode, false)
        .map_err(|_| "failed to create key up event".to_string())?;

    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);

    key_down.post(CGEventTapLocation::AnnotatedSession);
    key_up.post(CGEventTapLocation::AnnotatedSession);

    Ok(())
}

#[cfg(target_os = "windows")]
fn simulate_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    // Press Ctrl+V
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| e.to_string())?;

    Ok(())
}
