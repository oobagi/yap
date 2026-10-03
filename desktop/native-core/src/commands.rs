use crate::audio;
use crate::cache::CacheCleanupSummary;
use crate::config::{self, AppConfig};
use crate::history::{self, HistoryEntry};
use crate::model_manager::{
    self, WhisperDownloadRequest, WhisperModelList, WhisperModelSearchRequest, WhisperModelSummary,
};
use crate::ollama::{
    self, OllamaModelList, OllamaModelSearchRequest, OllamaModelSummary, OllamaPullRequest,
};
use serde_json::Value;

pub trait CommandHost: Send + Sync + 'static {
    fn on_settings_changed(&self) {}
    fn emit(&self, _event: &'static str, _payload: Value) {}
}

pub fn store_config(cfg: AppConfig, host: &dyn CommandHost) -> Result<(), String> {
    config::save(&cfg)?;
    host.on_settings_changed();
    Ok(())
}

pub fn audio_device_names() -> Vec<String> {
    audio::list_devices()
}

pub fn load_history() -> Vec<HistoryEntry> {
    history::load()
}

pub fn delete_history_entry(id: &str) -> Result<(), String> {
    history::remove(id)
}

pub fn delete_all_history() -> Result<(), String> {
    history::clear()
}

pub fn clear_runtime_cache() -> Result<CacheCleanupSummary, String> {
    crate::dictation::clear_runtime_cache()
}

pub fn list_whisper_models() -> Result<WhisperModelList, String> {
    model_manager::list_whisper_models()
}

pub fn search_whisper_models(
    request: WhisperModelSearchRequest,
) -> Result<Vec<WhisperModelSummary>, String> {
    model_manager::search_whisper_models(request)
}

pub fn download_whisper_model(
    request: WhisperDownloadRequest,
    host: &dyn CommandHost,
) -> Result<WhisperModelSummary, String> {
    model_manager::download_whisper_model(request, |payload| {
        host.emit("models:download", payload);
    })
}

pub fn delete_whisper_model(file_name: &str) -> Result<(), String> {
    model_manager::delete_whisper_model(file_name)
}

pub fn reveal_whisper_models() -> Result<(), String> {
    model_manager::reveal_whisper_models()
}

pub fn list_ollama_models() -> Result<OllamaModelList, String> {
    ollama::list_ollama_models()
}

pub fn search_ollama_models(
    request: OllamaModelSearchRequest,
) -> Result<Vec<OllamaModelSummary>, String> {
    ollama::search_ollama_models(request)
}

pub fn pull_ollama_model(request: OllamaPullRequest, host: &dyn CommandHost) -> Result<(), String> {
    ollama::pull_ollama_model(request, |payload| {
        host.emit("models:ollama-download", payload);
    })
}

pub fn delete_ollama_model(id: &str) -> Result<(), String> {
    ollama::delete_ollama_model(id)
}
