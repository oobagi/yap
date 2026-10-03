use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::config;

/// Maximum number of history entries kept on disk.
const MAX_ENTRIES: usize = 10;

/// A single transcription history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// ISO-8601 timestamp of when the entry was created.
    pub timestamp: DateTime<Utc>,
    /// The transcribed (and optionally formatted) text.
    pub text: String,
    /// The raw transcription before formatting, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    /// The formatter output that produced `text`, when formatting was enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted_text: Option<String>,
    /// Which transcription provider produced the text.
    pub transcription_provider: String,
    /// Which transcription model was used, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription_model: Option<String>,
    /// Which formatting provider was used, if any.
    pub formatting_provider: Option<String>,
    /// Which formatting model was used, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatting_model: Option<String>,
    /// Which formatting style was used, if any.
    pub formatting_style: Option<String>,
    /// The resolved formatting instruction/prompt, if formatting was enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatting_instruction: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryAppend {
    pub text: String,
    pub raw_text: Option<String>,
    pub formatted_text: Option<String>,
    pub transcription_provider: String,
    pub transcription_model: Option<String>,
    pub formatting_provider: Option<String>,
    pub formatting_model: Option<String>,
    pub formatting_style: Option<String>,
    pub formatting_instruction: Option<String>,
}

// ---- File path ------------------------------------------------------------

/// Full path to `history.json`.
fn history_path() -> Result<PathBuf, String> {
    Ok(config::config_dir()?.join("history.json"))
}

// ---- CRUD -----------------------------------------------------------------

/// Load the history array from disk. Returns an empty vec if the file does
/// not exist or cannot be parsed.
pub fn load() -> Vec<HistoryEntry> {
    let path = match history_path() {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    if !path.exists() {
        return vec![];
    }

    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    serde_json::from_str::<Vec<HistoryEntry>>(&data).unwrap_or_default()
}

/// Append a new entry at the front of the history list. If the list exceeds
/// `MAX_ENTRIES`, the oldest entries are dropped.
///
/// This is a no-op when `historyEnabled` is `false` in config.
pub fn append(input: HistoryAppend) -> Result<HistoryEntry, String> {
    let cfg = config::get();
    if !cfg.history_enabled {
        // Still return the entry object, just don't persist.
        return Ok(HistoryEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            text: input.text,
            raw_text: input.raw_text,
            formatted_text: input.formatted_text,
            transcription_provider: input.transcription_provider,
            transcription_model: input.transcription_model,
            formatting_provider: input.formatting_provider,
            formatting_model: input.formatting_model,
            formatting_style: input.formatting_style,
            formatting_instruction: input.formatting_instruction,
        });
    }

    let entry = HistoryEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        text: input.text,
        raw_text: input.raw_text,
        formatted_text: input.formatted_text,
        transcription_provider: input.transcription_provider,
        transcription_model: input.transcription_model,
        formatting_provider: input.formatting_provider,
        formatting_model: input.formatting_model,
        formatting_style: input.formatting_style,
        formatting_instruction: input.formatting_instruction,
    };

    let mut entries = load();
    entries.insert(0, entry.clone()); // newest first
    entries.truncate(MAX_ENTRIES);
    save_entries(&entries)?;

    Ok(entry)
}

/// Remove a specific entry by ID.
pub fn remove(id: &str) -> Result<(), String> {
    let mut entries = load();
    let before = entries.len();
    entries.retain(|e| e.id != id);

    if entries.len() == before {
        return Err(format!("history entry not found: {id}"));
    }

    save_entries(&entries)
}

/// Clear all history entries (replaces file with an empty array).
pub fn clear() -> Result<(), String> {
    save_entries(&[])
}

/// Get a single entry by ID.
pub fn get(id: &str) -> Option<HistoryEntry> {
    load().into_iter().find(|e| e.id == id)
}

// ---- Internal -------------------------------------------------------------

fn save_entries(entries: &[HistoryEntry]) -> Result<(), String> {
    let path = history_path()?;
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("failed to serialize history: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("failed to write history: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_history_entries_remain_readable() {
        let entry: HistoryEntry = serde_json::from_str(
            r#"{
            "id":"old-entry", "timestamp":"2026-07-01T12:00:00Z", "text":"Existing dictation",
            "transcriptionProvider":"openai", "formattingProvider":null, "formattingStyle":null
        }"#,
        )
        .unwrap();
        assert_eq!(entry.text, "Existing dictation");
        assert!(entry.raw_text.is_none());
        assert!(entry.formatted_text.is_none());
        assert!(entry.formatting_instruction.is_none());
        let saved = serde_json::to_value(entry).unwrap();
        assert!(saved.get("rawText").is_none());
    }
}
