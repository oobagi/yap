//! Apple Foundation Models formatting helper bridge.
//!
//! macOS uses a small Swift helper because FoundationModels is a Swift-first
//! framework. Non-macOS builds return unavailable.

use std::time::Duration;

#[cfg(target_os = "macos")]
mod platform {
    use crate::log;
    use serde::Serialize;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FormatRequest<'a> {
        text: &'a str,
        prompt: &'a str,
        timeout_seconds: f64,
    }

    pub fn format(text: &str, prompt: &str, timeout: Duration) -> Result<String, String> {
        let helper = format_helper_path().ok_or_else(|| {
            "Foundation Models formatter helper not found. Rebuild the macOS sidecar with `pnpm run electron:build-sidecar`.".to_string()
        })?;

        format_with_helper(&helper, text, prompt, timeout)
    }

    fn format_with_helper(
        helper: &PathBuf,
        text: &str,
        prompt: &str,
        timeout: Duration,
    ) -> Result<String, String> {
        log::info(&format!(
            "FoundationModels formatter: starting helper timeout={timeout:?} input_len={}",
            text.trim().len()
        ));

        let request = serde_json::to_vec(&FormatRequest {
            text,
            prompt,
            timeout_seconds: timeout.as_secs_f64(),
        })
        .map_err(|error| format!("failed to encode Foundation Models request: {error}"))?;

        let mut child = Command::new(helper)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("failed to run Foundation Models helper: {error}"))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to open Foundation Models helper stdin".to_string())?;
        stdin
            .write_all(&request)
            .map_err(|error| format!("failed to write Foundation Models request: {error}"))?;
        drop(stdin);

        let started_at = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    let output = child.wait_with_output().map_err(|error| {
                        format!("failed to read Foundation Models output: {error}")
                    })?;
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if !stderr.is_empty() {
                        log::info(&format!("FoundationModels helper: {stderr}"));
                    }

                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if output.status.success() {
                        log::info(&format!(
                            "FoundationModels formatter: helper completed len={}",
                            stdout.len()
                        ));
                        return Ok(stdout);
                    }

                    let message = if stderr.is_empty() {
                        format!(
                            "Foundation Models helper failed with status {}",
                            output.status
                        )
                    } else {
                        stderr
                    };
                    return Err(message);
                }
                Ok(None) => {
                    if started_at.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait_with_output();
                        return Err(format!(
                            "Foundation Models formatter timed out after {:.1} seconds",
                            timeout.as_secs_f64()
                        ));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    let _ = child.kill();
                    return Err(format!("failed to poll Foundation Models helper: {error}"));
                }
            }
        }
    }

    fn format_helper_path() -> Option<PathBuf> {
        let helper_name = if cfg!(target_arch = "aarch64") {
            "yap-format-aarch64-apple-darwin"
        } else {
            "yap-format-x86_64-apple-darwin"
        };
        let exe = std::env::current_exe().ok()?;
        let exe_dir = exe.parent()?;

        let mut candidates = vec![exe_dir.join(helper_name)];
        if let Some(native_core_dir) = exe_dir.parent().and_then(|target_dir| target_dir.parent()) {
            candidates.push(native_core_dir.join("binaries").join(helper_name));
        }

        candidates.into_iter().find(|path| path.exists())
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::time::Duration;

    pub fn format(_text: &str, _prompt: &str, _timeout: Duration) -> Result<String, String> {
        Err("Apple On-device formatting is only available on macOS 26 or newer".to_string())
    }
}

pub fn format(text: &str, prompt: &str, timeout: Duration) -> Result<String, String> {
    platform::format(text, prompt, timeout)
}
