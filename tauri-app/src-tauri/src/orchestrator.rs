//! Central pipeline orchestrator — equivalent to AppDelegate.swift.
//!
//! Owns the state machine and coordinates the full pipeline:
//! hotkey → audio → transcription → formatting → paste.
//!
//! All public methods are safe to call from any thread. Internal state is
//! guarded by a `Mutex` behind an `Arc` stored in Tauri's managed state.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, AudioLevels};
use crate::config::{self, AppConfig};
use crate::formatting::{self, FormattingOptions, FormattingProvider};
use crate::history;
use crate::hotkey::{self, HotkeyModifier};
use crate::paste;
use crate::transcription::{self, TranscriptionOptions, TranscriptionProvider};
use crate::tray;

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppState {
    Idle,
    Recording,
    HandsFreeRecording,
    HandsFreePaused,
    Processing,
}

// ---------------------------------------------------------------------------
// Events emitted to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateChangePayload {
    state: AppState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload {
    message: String,
}

// ---------------------------------------------------------------------------
// Orchestrator inner (lives behind Arc<Mutex<_>>)
// ---------------------------------------------------------------------------

struct OrchestratorInner {
    state: AppState,
    /// Whether the hotkey listener is enabled (tray toggle).
    enabled: bool,
    /// Peak audio level observed during the current recording session.
    peak_level: f32,
    /// Timestamp when the current recording started.
    recording_start: Option<Instant>,
    /// Whether we should ignore the next key-up event (hands-free entered
    /// while the hotkey was still held from the initial key-down).
    ignore_pending_key_up: bool,
    /// Handle to emit events and manage windows.
    app: AppHandle,
}

impl OrchestratorInner {
    /// Emit a state change event to the frontend and update the tray icon.
    fn emit_state(&self) {
        let _ = self.app.emit("state:change", StateChangePayload { state: self.state });
        let state_str = match self.state {
            AppState::Idle => "idle",
            AppState::Recording => "recording",
            AppState::HandsFreeRecording => "recording",
            AppState::HandsFreePaused => "recording",
            AppState::Processing => "processing",
        };
        tray::update_icon(&self.app, state_str);
    }

    /// Emit an error event to the frontend.
    fn emit_error(&self, message: &str) {
        let _ = self.app.emit("pipeline:error", ErrorPayload { message: message.to_string() });
    }

    /// Emit audio levels to the frontend.
    fn emit_levels(&self, levels: &AudioLevels) {
        let _ = self.app.emit("audio:levels", levels);
    }
}

// ---------------------------------------------------------------------------
// Thread-safe wrapper
// ---------------------------------------------------------------------------

/// `Orchestrator` is the single owner of pipeline state. It is stored in
/// Tauri's managed state as `Arc<Orchestrator>` so that commands, hotkey
/// callbacks, and audio callbacks can all reach it.
pub struct Orchestrator {
    inner: Mutex<OrchestratorInner>,
}

impl Orchestrator {
    /// Create a new orchestrator and bind it to the given `AppHandle`.
    fn new(app: AppHandle) -> Self {
        Self {
            inner: Mutex::new(OrchestratorInner {
                state: AppState::Idle,
                enabled: true,
                peak_level: 0.0,
                recording_start: None,
                ignore_pending_key_up: false,
                app,
            }),
        }
    }

    // -- Snapshot helpers (lock briefly, copy, release) --------------------

    pub fn state(&self) -> AppState {
        self.inner.lock().unwrap().state
    }

    fn app_handle(&self) -> AppHandle {
        self.inner.lock().unwrap().app.clone()
    }

    // -- Key event handlers -----------------------------------------------

    /// Called by the hotkey module when the modifier key is pressed.
    pub fn on_key_down(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled {
            return;
        }
        if inner.state != AppState::Idle {
            return;
        }
        inner.state = AppState::Recording;
        inner.recording_start = Some(Instant::now());
        inner.peak_level = 0.0;
        inner.emit_state();
        drop(inner);

        // Start audio capture
        match audio::start_recording() {
            Ok(_path) => {
                log::info("Recording started");
                // Play start sound delayed 100ms
                let app = self.app_handle();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    play_sound(&app, "Blow");
                });
            }
            Err(e) => {
                log::info(&format!("Failed to start recording: {e}"));
                let mut inner = self.inner.lock().unwrap();
                inner.state = AppState::Idle;
                inner.emit_error(&format!("Recording failed: {e}"));
                inner.emit_state();
            }
        }
    }

    /// Called by the hotkey module when the modifier key is released.
    pub fn on_key_up(&self) {
        let mut inner = self.inner.lock().unwrap();

        // In hands-free mode, ignore a single key-up if we entered hands-free
        // while the key was still held.
        if inner.state == AppState::HandsFreeRecording || inner.state == AppState::HandsFreePaused {
            if inner.ignore_pending_key_up {
                inner.ignore_pending_key_up = false;
                return;
            }
            drop(inner);
            self.stop_hands_free_internal();
            return;
        }

        if inner.state != AppState::Recording {
            return;
        }

        let duration = inner
            .recording_start
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let peak = inner.peak_level;
        drop(inner);

        log::info(&format!(
            "Key up -- duration: {:.1}s, peak: {:.3}",
            duration, peak
        ));

        // Too-short tap with low peak: cancel
        if duration < 0.5 && peak < 0.15 {
            log::info("Too short / quiet -- cancelling");
            let _ = audio::stop_recording(); // discard
            let mut inner = self.inner.lock().unwrap();
            inner.state = AppState::Idle;
            inner.emit_state();
            inner.emit_error("hold_tip");
            return;
        }

        self.stop_and_process();
    }

    /// Called by the hotkey module on a double-tap of the modifier key.
    pub fn on_double_tap(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled {
            return;
        }

        match inner.state {
            AppState::Recording => {
                // Convert current hold-to-record into hands-free
                inner.state = AppState::HandsFreeRecording;
                inner.ignore_pending_key_up = true;
                inner.emit_state();
                log::info("Converted to hands-free recording");
            }
            AppState::Idle => {
                // Start fresh hands-free recording
                inner.state = AppState::Recording;
                inner.recording_start = Some(Instant::now());
                inner.peak_level = 0.0;
                inner.emit_state();
                drop(inner);

                match audio::start_recording() {
                    Ok(_) => {
                        play_sound(&self.app_handle(), "Blow");
                        let mut inner = self.inner.lock().unwrap();
                        inner.state = AppState::HandsFreeRecording;
                        inner.ignore_pending_key_up = true;
                        inner.emit_state();
                        log::info("Hands-free recording started");
                    }
                    Err(e) => {
                        log::info(&format!("Failed to start recording: {e}"));
                        let mut inner = self.inner.lock().unwrap();
                        inner.state = AppState::Idle;
                        inner.emit_error(&format!("Recording failed: {e}"));
                        inner.emit_state();
                    }
                }
            }
            _ => {}
        }
    }

    // -- Hands-free controls -----------------------------------------------

    /// Toggle pause/resume in hands-free mode.
    pub fn toggle_pause(&self) {
        let mut inner = self.inner.lock().unwrap();
        match inner.state {
            AppState::HandsFreeRecording => {
                audio::pause_recording();
                inner.state = AppState::HandsFreePaused;
                inner.emit_state();
                log::info("Hands-free: paused");
            }
            AppState::HandsFreePaused => {
                audio::resume_recording();
                inner.state = AppState::HandsFreeRecording;
                inner.emit_state();
                log::info("Hands-free: resumed");
            }
            _ => {}
        }
    }

    /// Stop hands-free recording and process the audio.
    pub fn stop_hands_free(&self) {
        self.stop_hands_free_internal();
    }

    fn stop_hands_free_internal(&self) {
        let state = self.state();
        if state != AppState::HandsFreeRecording && state != AppState::HandsFreePaused {
            return;
        }
        log::info("Hands-free: stopping");
        self.stop_and_process();
    }

    // -- Pill click handler (from frontend IPC) ----------------------------

    /// Called when the user clicks the overlay pill.
    pub fn on_pill_click(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled {
            return;
        }

        match inner.state {
            AppState::Idle => {
                // Start click-to-record (hands-free)
                inner.state = AppState::Recording;
                inner.recording_start = Some(Instant::now());
                inner.peak_level = 0.0;
                inner.emit_state();
                drop(inner);

                match audio::start_recording() {
                    Ok(_) => {
                        play_sound(&self.app_handle(), "Blow");
                        let mut inner = self.inner.lock().unwrap();
                        inner.state = AppState::HandsFreeRecording;
                        inner.ignore_pending_key_up = false;
                        inner.emit_state();
                        log::info("Pill click: hands-free recording started");
                    }
                    Err(e) => {
                        log::info(&format!("Pill click: failed to start recording: {e}"));
                        let mut inner = self.inner.lock().unwrap();
                        inner.state = AppState::Idle;
                        inner.emit_error(&format!("Recording failed: {e}"));
                        inner.emit_state();
                    }
                }
            }
            AppState::Recording => {
                // Convert hold-to-record into hands-free
                inner.state = AppState::HandsFreeRecording;
                inner.ignore_pending_key_up = true;
                inner.emit_state();
                log::info("Pill click: converted to hands-free");
            }
            AppState::HandsFreeRecording | AppState::HandsFreePaused => {
                // Stop current hands-free session
                drop(inner);
                self.stop_hands_free_internal();
            }
            AppState::Processing => {
                // Ignore clicks during processing
            }
        }
    }

    // -- Settings change handler ------------------------------------------

    /// Called when settings are saved. Reloads config and restarts the hotkey
    /// listener with the new modifier.
    pub fn on_settings_changed(&self) {
        log::info("Settings changed -- reloading");
        let cfg = config::get();

        // Restart hotkey with new modifier
        hotkey::stop();
        let modifier = parse_hotkey_modifier(&cfg.hotkey);
        let app = self.app_handle();
        let orch = app.state::<Arc<Orchestrator>>();
        start_hotkey_listener(Arc::clone(&orch), modifier);

        // Notify frontend
        let inner = self.inner.lock().unwrap();
        let _ = inner.app.emit("settings:changed", ());
    }

    // -- Enable/disable toggle (from tray) --------------------------------

    pub fn toggle_enabled(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.enabled = !inner.enabled;
        let enabled = inner.enabled;
        log::info(&format!("Enabled: {enabled}"));
        let _ = inner.app.emit("enabled:changed", enabled);
    }

    // -- Audio level callback ---------------------------------------------

    /// Called from the audio level polling loop with fresh levels.
    pub fn on_audio_levels(&self, levels: AudioLevels) {
        let mut inner = self.inner.lock().unwrap();
        if levels.level > inner.peak_level {
            inner.peak_level = levels.level;
        }
        inner.emit_levels(&levels);
    }

    // -- Core pipeline (stop recording, transcribe, format, paste) --------

    fn stop_and_process(&self) {
        // Stop the audio recorder and get the WAV path
        let wav_path = match audio::stop_recording() {
            Ok(p) => p,
            Err(e) => {
                log::info(&format!("Stop recording failed: {e}"));
                let mut inner = self.inner.lock().unwrap();
                inner.state = AppState::Idle;
                inner.emit_state();
                return;
            }
        };

        let peak = {
            let inner = self.inner.lock().unwrap();
            inner.peak_level
        };

        play_sound(&self.app_handle(), "Pop");

        // Silence check
        if peak < 0.15 {
            log::info(&format!("Silence detected (peak {:.3}) -- skipping", peak));
            let mut inner = self.inner.lock().unwrap();
            inner.state = AppState::Idle;
            inner.emit_state();
            inner.emit_error("speak_tip");
            return;
        }

        // Transition to processing
        {
            let mut inner = self.inner.lock().unwrap();
            inner.state = AppState::Processing;
            inner.emit_state();
        }

        // Spawn the async transcription/formatting pipeline
        let app = self.app_handle();
        let orch = app.state::<Arc<Orchestrator>>();
        let orch = Arc::clone(&orch);
        let cfg = config::get();

        tauri::async_runtime::spawn(async move {
            let result = process_audio_pipeline(&wav_path, &cfg).await;
            match result {
                Ok(text) => {
                    if !text.is_empty() {
                        // Add to history
                        let tx_provider = format!("{:?}", cfg.tx_provider).to_lowercase();
                        let fmt_provider = if cfg.fmt_provider != FormattingProvider::None {
                            Some(format!("{:?}", cfg.fmt_provider).to_lowercase())
                        } else {
                            None
                        };
                        let fmt_style = if cfg.fmt_provider != FormattingProvider::None {
                            Some(format!("{:?}", cfg.fmt_style).to_lowercase())
                        } else {
                            None
                        };
                        let _ = history::append(
                            text.clone(),
                            tx_provider,
                            fmt_provider,
                            fmt_style,
                        );

                        // Refresh tray history menu
                        let app = orch.app_handle();
                        tray::refresh_history_menu(&app);

                        // Paste the result
                        if let Err(e) = paste::paste_text(&text) {
                            log::info(&format!("Paste failed: {e}"));
                        }
                    }
                }
                Err(e) => {
                    log::info(&format!("Pipeline error: {e}"));
                    let inner = orch.inner.lock().unwrap();
                    inner.emit_error(&classify_error(&e));
                }
            }

            // Return to idle
            let mut inner = orch.inner.lock().unwrap();
            inner.state = AppState::Idle;
            inner.emit_state();
        });
    }
}

// ---------------------------------------------------------------------------
// Async pipeline: transcribe -> format -> return text
// ---------------------------------------------------------------------------

async fn process_audio_pipeline(wav_path: &PathBuf, cfg: &AppConfig) -> Result<String, String> {
    let tx_options = TranscriptionOptions {
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

    // Resolve formatting API key (fall back to tx_api_key if empty)
    let fmt_api_key = if cfg.fmt_api_key.is_empty() {
        cfg.tx_api_key.clone()
    } else {
        cfg.fmt_api_key.clone()
    };

    // Check for Gemini one-shot optimization: when both transcription and
    // formatting use Gemini, a single API call handles both.
    let use_oneshot = cfg.tx_provider == TranscriptionProvider::Gemini
        && cfg.fmt_provider == FormattingProvider::Gemini
        && cfg.tx_provider.can_also_format();

    let raw_text = if use_oneshot {
        log::info("One-shot: Gemini transcribe+format");
        transcription::transcribe_gemini_oneshot(wav_path, &tx_options, cfg.fmt_style).await?
    } else {
        // Standard two-step: transcribe, then optionally format
        log::info(&format!("Transcribing with {:?}", cfg.tx_provider));
        transcription::transcribe(cfg.tx_provider, wav_path, &tx_options).await?
    };

    let trimmed = raw_text.trim().to_string();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // Prompt regurgitation guard
    let lower = trimmed.to_lowercase();
    if lower.contains("transcribe this audio")
        || lower.contains("respond with only a json")
        || lower.contains("dictation commands")
    {
        log::info("Discarded -- model regurgitated the prompt");
        return Ok(String::new());
    }

    // If we used one-shot, text is already formatted
    if use_oneshot {
        return Ok(trimmed);
    }

    // Format if a provider is configured
    if cfg.fmt_provider != FormattingProvider::None && !fmt_api_key.is_empty() {
        log::info(&format!("Formatting with {:?}", cfg.fmt_provider));
        let fmt_options = FormattingOptions {
            api_key: fmt_api_key,
            model: cfg.fmt_model.clone(),
            style: cfg.fmt_style,
        };
        let formatted = formatting::format(cfg.fmt_provider, &trimmed, &fmt_options).await?;

        // Check formatted text for prompt regurgitation too
        let fl = formatted.to_lowercase();
        if fl.contains("transcribe this audio")
            || fl.contains("respond with only a json")
            || fl.contains("dictation commands")
        {
            log::info("Discarded formatted text -- prompt regurgitation");
            return Ok(trimmed);
        }

        Ok(formatted)
    } else {
        Ok(trimmed)
    }
}

// ---------------------------------------------------------------------------
// Error classification (maps API errors to user-friendly messages)
// ---------------------------------------------------------------------------

fn classify_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("quota") || lower.contains("rate") || lower.contains("429") {
        "Rate limited -- try again".to_string()
    } else if lower.contains("auth") || lower.contains("key") || lower.contains("401") || lower.contains("403") {
        "Invalid API key".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "Request timed out".to_string()
    } else if lower.contains("offline") || lower.contains("network") || lower.contains("internet") {
        "No internet connection".to_string()
    } else {
        "Something went wrong".to_string()
    }
}

// ---------------------------------------------------------------------------
// Sound effects
// ---------------------------------------------------------------------------

fn play_sound(app: &AppHandle, name: &str) {
    let cfg = config::get();
    if !cfg.sounds_enabled {
        return;
    }

    // Attempt to load the sound from the app's resource directory
    let resource_path = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("sounds").join(format!("{name}.aiff")));

    if let Some(path) = resource_path {
        if path.exists() {
            std::thread::spawn(move || {
                if let Ok((_stream, stream_handle)) = rodio::OutputStream::try_default() {
                    if let Ok(file) = std::fs::File::open(&path) {
                        let source = rodio::Decoder::new(std::io::BufReader::new(file));
                        if let Ok(source) = source {
                            let _ = stream_handle.play_raw(rodio::source::Source::convert_samples(source));
                            // Keep thread alive while audio plays
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Hotkey modifier parsing
// ---------------------------------------------------------------------------

fn parse_hotkey_modifier(hotkey: &str) -> HotkeyModifier {
    match hotkey {
        "option" => HotkeyModifier::Option,
        _ => HotkeyModifier::Fn,
    }
}

// ---------------------------------------------------------------------------
// Hotkey listener setup (connects hotkey callbacks to orchestrator)
// ---------------------------------------------------------------------------

fn start_hotkey_listener(orch: Arc<Orchestrator>, modifier: HotkeyModifier) {
    let orch_down = Arc::clone(&orch);
    let orch_up = Arc::clone(&orch);
    let orch_double = Arc::clone(&orch);

    hotkey::set_callbacks(
        move || orch_down.on_key_down(),
        move || orch_up.on_key_up(),
        move || orch_double.on_double_tap(),
    );

    hotkey::start(modifier);
}

// ---------------------------------------------------------------------------
// Audio level polling loop
// ---------------------------------------------------------------------------

/// Spawn a background thread that polls audio levels every ~33ms and
/// forwards them to the orchestrator (which emits to the frontend).
fn start_level_poller(orch: Arc<Orchestrator>) {
    std::thread::Builder::new()
        .name("yap-level-poller".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(33));
                let state = orch.state();
                if state == AppState::Recording
                    || state == AppState::HandsFreeRecording
                    || state == AppState::HandsFreePaused
                {
                    let levels = audio::get_levels();
                    orch.on_audio_levels(levels);
                }
            }
        })
        .expect("failed to spawn level poller thread");
}

// ---------------------------------------------------------------------------
// Initialization (called from lib.rs setup)
// ---------------------------------------------------------------------------

/// Initialize the orchestrator and wire up all modules.
///
/// This is the main entry point called from `lib.rs` during app setup.
/// It loads configuration, starts the hotkey listener, sets up the tray,
/// and begins the audio level polling loop.
pub fn init(app: &AppHandle) {
    // Load config
    let _ = config::load();
    let cfg = config::get();

    // Create orchestrator and store as managed state
    let orch = Arc::new(Orchestrator::new(app.clone()));
    app.manage(Arc::clone(&orch));

    // Set up system tray
    let _ = tray::setup_tray(app);

    // Start hotkey listener
    let modifier = parse_hotkey_modifier(&cfg.hotkey);
    log::info(&format!("Starting hotkey: {:?}", modifier));
    start_hotkey_listener(Arc::clone(&orch), modifier);

    // Start audio level poller
    start_level_poller(Arc::clone(&orch));

    // Show overlay window (always-visible pill in idle state)
    if let Some(overlay) = app.get_webview_window("overlay") {
        // Position at bottom-center of primary monitor
        if let Ok(monitor) = overlay.primary_monitor() {
            if let Some(monitor) = monitor {
                let screen = monitor.size();
                let scale = monitor.scale_factor();
                let win_w = 400.0;
                let win_h = 200.0;
                let x = ((screen.width as f64 / scale) - win_w) / 2.0;
                let y = (screen.height as f64 / scale) - win_h;
                let _ = overlay.set_position(tauri::PhysicalPosition::new(
                    (x * scale) as i32,
                    (y * scale) as i32,
                ));
                log::info(&format!("Overlay positioned at {},{} (screen {}x{}, scale {})", x as i32, y as i32, screen.width, screen.height, scale));
            }
        }
        // Set macOS-specific window behaviors so the overlay:
        // - Appears on ALL desktops/Spaces (canJoinAllSpaces)
        // - Doesn't animate with Mission Control (stationary)
        // - Can appear alongside full-screen apps (fullScreenAuxiliary)
        #[cfg(target_os = "macos")]
        {
            use tauri::Emitter;
            if let Ok(ns_win) = overlay.ns_window() {
                unsafe {
                    use objc2::msg_send;
                    use objc2::runtime::AnyObject;
                    let win = ns_win as *mut AnyObject;
                    // NSWindowCollectionBehavior:
                    //   canJoinAllSpaces = 1 << 0 = 1
                    //   fullScreenAuxiliary = 1 << 8 = 256
                    //   stationary = 1 << 4 = 16
                    let behavior: u64 = 1 | 16 | 256;
                    let _: () = msg_send![win, setCollectionBehavior: behavior];
                    // Also set window level higher (floating = 3, or status = 25)
                    let _: () = msg_send![win, setLevel: 25_i64];
                }
                log::info("Overlay: set canJoinAllSpaces + stationary + fullScreenAuxiliary");
            }
        }

        let _ = overlay.show();
        log::info("Overlay window shown");
    } else {
        log::info("WARNING: overlay window not found");
    }

    log::info("Orchestrator initialized -- ready");
}

// ---------------------------------------------------------------------------
// Simple logging helper (writes to ~/.config/yap/debug.log)
// ---------------------------------------------------------------------------

pub(crate) mod log {
    use std::io::Write;

    pub fn info(message: &str) {
        eprintln!("[yap] {message}");

        // Also append to the debug log file
        if let Ok(dir) = crate::config::config_dir() {
            let path = dir.join("debug.log");
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let timestamp = chrono::Utc::now().to_rfc3339();
                let _ = writeln!(file, "[{timestamp}] {message}");
            }
        }
    }
}
