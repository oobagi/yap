//! Native overlay sidecar (macOS only).
//!
//! Spawns the Swift helper binary that renders the overlay using NSPanel +
//! SwiftUI.  Communication is newline-delimited JSON over stdin/stdout.

#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::sync::Mutex;

#[cfg(target_os = "macos")]
use serde::Serialize;
#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
use crate::orchestrator;

// ---------------------------------------------------------------------------
// Child process handle
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
static CHILD_STDIN: Mutex<Option<std::process::ChildStdin>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Messages: Rust → Sidecar (stdin)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OutMessage {
    State {
        state: String,
        #[serde(rename = "handsFree")]
        hands_free: bool,
        paused: bool,
        elapsed: f64,
    },
    Levels {
        level: f32,
        bars: Vec<f32>,
    },
    Error {
        message: String,
    },
    Permission {
        title: String,
        message: String,
        #[serde(rename = "actionLabel")]
        action_label: String,
        visible: bool,
    },
    Onboarding {
        step: String,
        text: String,
        #[serde(rename = "hotkeyLabel")]
        hotkey_label: String,
    },
    OnboardingPress {
        pressed: bool,
    },
    Config {
        #[serde(rename = "gradientEnabled")]
        gradient_enabled: bool,
        #[serde(rename = "alwaysVisible")]
        always_visible: bool,
        #[serde(rename = "hotkeyLabel")]
        hotkey_label: String,
    },
    #[allow(dead_code)]
    Celebrating,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Send a message to the sidecar overlay.  No-op if sidecar isn't running.
#[cfg(target_os = "macos")]
pub fn send(msg: &OutMessage) {
    let mut guard = match CHILD_STDIN.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let stdin = match guard.as_mut() {
        Some(s) => s,
        None => return,
    };
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = writeln!(stdin, "{}", json);
        let _ = stdin.flush();
    }
}

/// Spawn the sidecar overlay process.
#[cfg(target_os = "macos")]
pub fn spawn(app: &tauri::AppHandle) {
    use std::process::{Command, Stdio};

    // Resolve the sidecar binary path.
    // In a Tauri bundle it lives at Contents/MacOS/yap-overlay.
    // During development, try the swift build output first.
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecar-overlay/.build/debug/yap-overlay");

    let bundled_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("yap-overlay")));

    let bin = if dev_path.exists() {
        dev_path
    } else if let Some(ref bp) = bundled_path {
        if bp.exists() {
            bp.clone()
        } else {
            orchestrator::log::info(&format!(
                "Sidecar binary not found at {:?} or {:?}",
                dev_path, bundled_path
            ));
            return;
        }
    } else {
        orchestrator::log::info("Sidecar: cannot determine binary path");
        return;
    };

    orchestrator::log::info(&format!("Sidecar: launching {:?}", bin));

    let mut child = match Command::new(&bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            orchestrator::log::info(&format!("Sidecar: failed to spawn: {e}"));
            return;
        }
    };

    // Take ownership of stdin
    let stdin = child.stdin.take().unwrap();
    *CHILD_STDIN.lock().unwrap() = Some(stdin);

    // Read stdout on a background thread
    let stdout = child.stdout.take().unwrap();
    let app_handle = app.clone();

    std::thread::Builder::new()
        .name("yap-sidecar-reader".into())
        .spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stdout);

            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                if line.is_empty() {
                    continue;
                }

                let parsed: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let event = match parsed["event"].as_str() {
                    Some(e) => e.to_string(),
                    None => continue,
                };

                match event.as_str() {
                    "ready" => {
                        orchestrator::log::info("Sidecar: overlay ready");
                    }
                    "pill_click" => {
                        let orch: tauri::State<'_, std::sync::Arc<orchestrator::Orchestrator>> =
                            app_handle.state();
                        orch.on_pill_click();
                    }
                    "permission_action" => {
                        let orch: tauri::State<'_, std::sync::Arc<orchestrator::Orchestrator>> =
                            app_handle.state();
                        orch.on_permission_action();
                    }
                    "pause" => {
                        let orch: tauri::State<'_, std::sync::Arc<orchestrator::Orchestrator>> =
                            app_handle.state();
                        orch.toggle_pause();
                    }
                    "stop" => {
                        let orch: tauri::State<'_, std::sync::Arc<orchestrator::Orchestrator>> =
                            app_handle.state();
                        orch.stop_hands_free();
                    }
                    _ => {}
                }
            }

            orchestrator::log::info("Sidecar: process exited");
            *CHILD_STDIN.lock().unwrap() = None;
        })
        .expect("failed to spawn sidecar reader thread");
}

// Stub for non-macOS
#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub fn spawn(_app: &tauri::AppHandle) {}
