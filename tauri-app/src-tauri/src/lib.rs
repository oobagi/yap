mod audio;
mod audio_ducking;
mod config;
mod formatting;
mod history;
mod hotkey;
mod orchestrator;
mod paste;
mod sidecar;
mod speech;
mod transcription;
mod tray;
mod vad;
#[cfg(target_os = "windows")]
mod win_overlay;
mod windows;

use std::sync::Arc;

use tauri::{Emitter, Listener, Manager};

use crate::config::AppConfig;
use crate::history::HistoryEntry;
use crate::orchestrator::Orchestrator;

// ---------------------------------------------------------------------------
// Tauri commands -- exposed to the frontend via invoke()
// ---------------------------------------------------------------------------

/// Load the config from disk (or return defaults).
#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    config::load()
}

/// Persist the full config to disk, then notify the orchestrator.
#[tauri::command]
fn save_config(cfg: AppConfig, orch: tauri::State<'_, Arc<Orchestrator>>) -> Result<(), String> {
    config::save(&cfg)?;
    orch.on_settings_changed();
    Ok(())
}

/// Reset onboarding and show the first onboarding prompt immediately.
#[tauri::command]
fn reset_onboarding(orch: tauri::State<'_, Arc<Orchestrator>>) -> Result<(), String> {
    orch.reset_onboarding()
}

/// List available audio input devices.
#[tauri::command]
fn list_audio_devices() -> Vec<String> {
    audio::list_devices()
}

/// Load all history entries from disk.
#[tauri::command]
fn get_history() -> Vec<HistoryEntry> {
    history::load()
}

/// Remove a history entry by ID.
#[tauri::command]
fn remove_history_entry(id: String) -> Result<(), String> {
    history::remove(&id)
}

/// Clear all history entries.
#[tauri::command]
fn clear_history() -> Result<(), String> {
    history::clear()
}

/// Show the settings window. Used by the fallback root route.
#[tauri::command]
fn show_settings(app: tauri::AppHandle) -> Result<(), String> {
    windows::show_app_window(&app, "settings")
}

/// Hide an app window without destroying it so it can be reopened from the tray.
#[tauri::command]
fn hide_app_window(app: tauri::AppHandle, label: String) -> Result<(), String> {
    windows::hide_app_window(&app, &label)
}

#[tauri::command]
fn start_hotkey_capture(app: tauri::AppHandle) {
    let preview_app = app.clone();
    hotkey::begin_capture(
        move |shortcut| {
            let _ = preview_app.emit_to("settings", "settings:hotkey-preview", shortcut);
        },
        move |shortcut| {
            let _ = app.emit_to("settings", "settings:hotkey-captured", shortcut);
        },
    );
}

#[tauri::command]
fn cancel_hotkey_capture() {
    hotkey::cancel_capture();
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(windows::handle_window_event)
        .setup(|app| {
            // Initialize the orchestrator -- this loads config, starts
            // the hotkey listener, sets up the tray, and begins the
            // audio level poller.
            let handle = app.handle().clone();
            orchestrator::init(&handle);
            windows::hide_app_if_no_windows_visible(&handle);

            // Listen for tray toggle-enabled event
            let handle2 = app.handle().clone();
            app.listen("tray:toggle-enabled", move |_event| {
                let orch: tauri::State<'_, Arc<Orchestrator>> = handle2.state();
                orch.toggle_enabled();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Config
            get_config,
            save_config,
            reset_onboarding,
            list_audio_devices,
            // History
            get_history,
            remove_history_entry,
            clear_history,
            show_settings,
            hide_app_window,
            start_hotkey_capture,
            cancel_hotkey_capture,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
