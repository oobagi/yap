mod audio;
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
#[cfg(target_os = "windows")]
mod win_overlay;

use std::sync::Arc;

use tauri::{Listener, Manager};

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

/// Start recording audio from the default input device.
#[tauri::command]
fn start_recording() -> Result<String, String> {
    let path = audio::start_recording()?;
    Ok(path.to_string_lossy().to_string())
}

/// Stop the current recording session and return the WAV file path.
#[tauri::command]
fn stop_recording() -> Result<String, String> {
    let path = audio::stop_recording()?;
    Ok(path.to_string_lossy().to_string())
}

/// Get current real-time audio levels for the overlay waveform.
#[tauri::command]
fn get_audio_levels() -> audio::AudioLevels {
    audio::get_levels()
}

/// List available audio input devices.
#[tauri::command]
fn list_audio_devices() -> Vec<String> {
    audio::list_devices()
}

/// Transcribe the WAV file at the given path.
#[tauri::command]
async fn transcribe(audio_path: String) -> Result<String, String> {
    let cfg = config::get();

    let options = transcription::TranscriptionOptions {
        api_key: cfg.tx_api_key.clone(),
        model: cfg.tx_model.clone(),
        dg_smart_format: cfg.dg_smart_format,
        dg_keywords: cfg.dg_keywords.clone(),
        dg_language: cfg.dg_language.clone(),
        oai_language: cfg.oai_language.clone(),
        oai_prompt: cfg.oai_prompt.clone(),
        gemini_temperature: cfg.gemini_temperature,
        el_language_code: cfg.el_language_code.clone(),
    };

    transcription::transcribe(
        cfg.tx_provider,
        std::path::Path::new(&audio_path),
        &options,
    )
    .await
}

/// Format raw transcription text with the configured LLM.
#[tauri::command]
async fn format_text(text: String) -> Result<String, String> {
    let cfg = config::get();

    // Resolve API key: fall back to tx_api_key if fmt_api_key is empty.
    let api_key = if cfg.fmt_api_key.is_empty() {
        cfg.tx_api_key.clone()
    } else {
        cfg.fmt_api_key.clone()
    };

    let options = formatting::FormattingOptions {
        api_key,
        model: cfg.fmt_model.clone(),
        style: cfg.fmt_style,
    };

    formatting::format(cfg.fmt_provider, &text, &options).await
}

/// Paste text into the active application via clipboard + simulated keystroke.
#[tauri::command]
fn paste_text(text: String) -> Result<(), String> {
    paste::paste_text(&text)
}

/// Load all history entries from disk.
#[tauri::command]
fn get_history() -> Vec<HistoryEntry> {
    history::load()
}

/// Add a new history entry.
#[tauri::command]
fn add_history_entry(
    text: String,
    transcription_provider: String,
    formatting_provider: Option<String>,
    formatting_style: Option<String>,
) -> Result<HistoryEntry, String> {
    history::append(text, transcription_provider, formatting_provider, formatting_style)
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

// ---------------------------------------------------------------------------
// Orchestrator commands -- pipeline control from the frontend
// ---------------------------------------------------------------------------

/// Called when the user clicks the overlay pill.
#[tauri::command]
fn pill_clicked(orch: tauri::State<'_, Arc<Orchestrator>>) {
    orch.on_pill_click();
}

/// Toggle pause/resume in hands-free recording mode.
#[tauri::command]
fn pause_resume(orch: tauri::State<'_, Arc<Orchestrator>>) {
    orch.toggle_pause();
}

/// Stop hands-free recording and process the audio.
#[tauri::command]
fn stop_hands_free(orch: tauri::State<'_, Arc<Orchestrator>>) {
    orch.stop_hands_free();
}

/// Toggle the enabled state (from tray menu).
#[tauri::command]
fn toggle_enabled(orch: tauri::State<'_, Arc<Orchestrator>>) {
    orch.toggle_enabled();
}

/// Get the current pipeline state.
#[tauri::command]
fn get_pipeline_state(orch: tauri::State<'_, Arc<Orchestrator>>) -> orchestrator::AppState {
    orch.state()
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Initialize the orchestrator -- this loads config, starts
            // the hotkey listener, sets up the tray, and begins the
            // audio level poller.
            let handle = app.handle().clone();
            orchestrator::init(&handle);

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
            // Audio (direct access, still useful for frontend)
            start_recording,
            stop_recording,
            get_audio_levels,
            list_audio_devices,
            // Transcription & formatting (direct access)
            transcribe,
            format_text,
            // Paste
            paste_text,
            // History
            get_history,
            add_history_entry,
            remove_history_entry,
            clear_history,
            // Pipeline control (orchestrator)
            pill_clicked,
            pause_resume,
            stop_hands_free,
            toggle_enabled,
            get_pipeline_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
