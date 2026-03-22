mod audio;
mod config;
mod formatting;
mod history;
mod hotkey;
mod paste;
mod transcription;
mod tray;

use crate::config::AppConfig;
use crate::history::HistoryEntry;

// ---------------------------------------------------------------------------
// Tauri commands -- exposed to the frontend via invoke()
// ---------------------------------------------------------------------------

/// Load the config from disk (or return defaults).
#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    config::load()
}

/// Persist the full config to disk.
#[tauri::command]
fn save_config(cfg: AppConfig) -> Result<(), String> {
    config::save(&cfg)
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
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Load config at startup.
            let _ = config::load();

            // Set up system tray.
            let handle = app.handle().clone();
            let _ = tray::setup_tray(&handle);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            start_recording,
            stop_recording,
            get_audio_levels,
            list_audio_devices,
            transcribe,
            format_text,
            paste_text,
            get_history,
            add_history_entry,
            remove_history_entry,
            clear_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
