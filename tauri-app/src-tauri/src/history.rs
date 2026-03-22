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
    /// Which transcription provider produced the text.
    pub transcription_provider: String,
    /// Which formatting provider was used, if any.
    pub formatting_provider: Option<String>,
    /// Which formatting style was used, if any.
    pub formatting_style: Option<String>,
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
pub fn append(
    text: String,
    transcription_provider: String,
    formatting_provider: Option<String>,
    formatting_style: Option<String>,
) -> Result<HistoryEntry, String> {
    let cfg = config::get();
    if !cfg.history_enabled {
        // Still return the entry object, just don't persist.
        return Ok(HistoryEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            text,
            transcription_provider,
            formatting_provider,
            formatting_style,
        });
    }

    let entry = HistoryEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        text,
        transcription_provider,
        formatting_provider,
        formatting_style,
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
