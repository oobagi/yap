use serde::Serialize;
use std::fs;
use std::io;
use std::path::PathBuf;

const TEMP_RECORDING_FILE: &str = "yap_recording.wav";
const DEBUG_LOG_FILE: &str = "debug.log";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheCleanupSummary {
    pub removed: Vec<String>,
    pub missing: Vec<String>,
}

pub fn clear_runtime_cache() -> Result<CacheCleanupSummary, String> {
    if crate::audio::is_recording() {
        return Err("Stop the current recording before cleaning the cache.".to_string());
    }

    let mut removed = Vec::new();
    let mut missing = Vec::new();

    for target in cleanup_targets()? {
        remove_target(target, &mut removed, &mut missing)?;
    }

    Ok(CacheCleanupSummary { removed, missing })
}

fn cleanup_targets() -> Result<Vec<CleanupTarget>, String> {
    Ok(vec![
        CleanupTarget {
            label: "temporary recording".to_string(),
            path: std::env::temp_dir().join(TEMP_RECORDING_FILE),
        },
        CleanupTarget {
            label: "debug log".to_string(),
            path: crate::config::config_dir()?.join(DEBUG_LOG_FILE),
        },
    ])
}

fn remove_target(
    target: CleanupTarget,
    removed: &mut Vec<String>,
    missing: &mut Vec<String>,
) -> Result<(), String> {
    match fs::remove_file(&target.path) {
        Ok(()) => {
            removed.push(target.label);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            missing.push(target.label);
            Ok(())
        }
        Err(error) => Err(format!(
            "failed to remove {} at {}: {error}",
            target.label,
            target.path.display()
        )),
    }
}

struct CleanupTarget {
    label: String,
    path: PathBuf,
}
