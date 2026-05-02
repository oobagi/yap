use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::formatting::{FormattingProvider, FormattingStyle};
use crate::transcription::TranscriptionProvider;

/// All 21 config keys from the Yap spec, plus serde defaults for each.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// Hotkey modifier: "fn" or "option"
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Preferred input device name. Empty string means "system default".
    #[serde(default)]
    pub audio_device: String,

    /// Transcription provider
    #[serde(default)]
    pub tx_provider: TranscriptionProvider,

    /// Transcription API key
    #[serde(default)]
    pub tx_api_key: String,

    /// Transcription model (empty = provider default)
    #[serde(default)]
    pub tx_model: String,

    /// Formatting provider
    #[serde(default)]
    pub fmt_provider: FormattingProvider,

    /// Formatting API key (empty = reuse tx_api_key if same provider type)
    #[serde(default)]
    pub fmt_api_key: String,

    /// Formatting model (empty = provider default)
    #[serde(default)]
    pub fmt_model: String,

    /// Formatting style
    #[serde(default)]
    pub fmt_style: FormattingStyle,

    /// Whether the user has completed onboarding
    #[serde(default)]
    pub onboarding_complete: bool,

    /// Deepgram: enable smart formatting
    #[serde(default = "default_true")]
    pub dg_smart_format: bool,

    /// Deepgram: comma-separated boost keywords
    #[serde(default)]
    pub dg_keywords: String,

    /// Deepgram: ISO 639-1 language code
    #[serde(default)]
    pub dg_language: String,

    /// OpenAI: ISO 639-1 language code
    #[serde(default)]
    pub oai_language: String,

    /// OpenAI: context prompt for Whisper
    #[serde(default)]
    pub oai_prompt: String,

    /// Gemini: temperature (0.0 - 1.0)
    #[serde(default)]
    pub gemini_temperature: f64,

    /// ElevenLabs: ISO 639-1 language code
    #[serde(default)]
    pub el_language_code: String,

    /// Play sound effects on start/stop/error
    #[serde(default = "default_true")]
    pub sounds_enabled: bool,

    /// Show the animated gradient background on the overlay
    #[serde(default = "default_true")]
    pub gradient_enabled: bool,

    /// Keep the pill visible even when idle
    #[serde(default = "default_true")]
    pub always_visible_pill: bool,

    /// Persist transcription history to disk
    #[serde(default = "default_true")]
    pub history_enabled: bool,

    /// BCP 47 locale for on-device speech recognition (e.g. "en-US", "ja-JP").
    /// Empty string defaults to "en-US".
    #[serde(default)]
    pub speech_locale: String,
}

impl AppConfig {
    /// Resolved speech recognition locale (defaults to "en-US" if empty).
    pub fn speech_recognition_locale(&self) -> String {
        if self.speech_locale.is_empty() {
            "en-US".to_string()
        } else {
            self.speech_locale.clone()
        }
    }
}

// ---- serde default helpers ------------------------------------------------

fn default_hotkey() -> String {
    "fn".to_string()
}

fn default_true() -> bool {
    true
}

// ---- Default trait --------------------------------------------------------

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            audio_device: String::new(),
            tx_provider: TranscriptionProvider::default(),
            tx_api_key: String::new(),
            tx_model: String::new(),
            fmt_provider: FormattingProvider::default(),
            fmt_api_key: String::new(),
            fmt_model: String::new(),
            fmt_style: FormattingStyle::default(),
            onboarding_complete: false,
            dg_smart_format: true,
            dg_keywords: String::new(),
            dg_language: String::new(),
            oai_language: String::new(),
            oai_prompt: String::new(),
            gemini_temperature: 0.0,
            el_language_code: String::new(),
            sounds_enabled: true,
            gradient_enabled: true,
            always_visible_pill: true,
            history_enabled: true,
            speech_locale: String::new(),
        }
    }
}

// ---- Global config state --------------------------------------------------

static CONFIG: once_cell::sync::Lazy<Mutex<AppConfig>> =
    once_cell::sync::Lazy::new(|| Mutex::new(AppConfig::default()));

// ---- File path ------------------------------------------------------------

/// Return the platform-appropriate config directory, creating it if needed.
///
/// - macOS / Linux: `~/.config/yap/`
/// - Windows: `%APPDATA%\yap\`
pub fn config_dir() -> Result<PathBuf, String> {
    let base = if cfg!(target_os = "windows") {
        dirs::data_dir() // %APPDATA% on Windows
    } else {
        dirs::home_dir().map(|h| h.join(".config"))
    };

    let dir = base
        .ok_or_else(|| "could not determine config directory".to_string())?
        .join("yap");

    fs::create_dir_all(&dir).map_err(|e| format!("failed to create config dir: {e}"))?;
    Ok(dir)
}

/// Full path to `config.json`.
pub fn config_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.json"))
}

// ---- Load / Save ----------------------------------------------------------

/// Load config from disk. If the file does not exist or is invalid, returns
/// the default config and writes it to disk.
pub fn load() -> Result<AppConfig, String> {
    let path = config_path()?;

    let config = if path.exists() {
        let data = fs::read_to_string(&path).map_err(|e| format!("failed to read config: {e}"))?;
        serde_json::from_str::<AppConfig>(&data).unwrap_or_else(|_| {
            // File exists but is malformed -- fall back to defaults.
            AppConfig::default()
        })
    } else {
        let config = AppConfig::default();
        // Write defaults so the file exists for the user to inspect.
        let _ = save_to_disk(&config);
        config
    };

    // Update global state.
    if let Ok(mut guard) = CONFIG.lock() {
        *guard = config.clone();
    }

    Ok(config)
}

/// Persist the given config to disk and update global state.
pub fn save(config: &AppConfig) -> Result<(), String> {
    save_to_disk(config)?;

    if let Ok(mut guard) = CONFIG.lock() {
        *guard = config.clone();
    }

    Ok(())
}

/// Get a snapshot of the current in-memory config.
pub fn get() -> AppConfig {
    CONFIG.lock().map(|g| g.clone()).unwrap_or_default()
}

/// Update a single field via a closure, persist, and return the new config.
pub fn update<F: FnOnce(&mut AppConfig)>(f: F) -> Result<AppConfig, String> {
    let mut config = get();
    f(&mut config);
    save(&config)?;
    Ok(config)
}

// ---- Internal -------------------------------------------------------------

fn save_to_disk(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("failed to serialize config: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("failed to write config: {e}"))?;
    Ok(())
}
