use base64::Engine;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use crate::formatting::FormattingStyle;

/// Transcription provider identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionProvider {
    /// On-device speech recognition (Apple Speech / Windows Speech).
    None,
    Gemini,
    #[serde(rename = "openai")]
    OpenAI,
    Deepgram,
    #[serde(rename = "elevenlabs")]
    ElevenLabs,
}

impl Default for TranscriptionProvider {
    fn default() -> Self {
        Self::None
    }
}

impl TranscriptionProvider {
    /// Default model string for each provider (used when user leaves model empty).
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Gemini => "gemini-2.5-flash",
            Self::OpenAI => "gpt-4o-transcribe",
            Self::Deepgram => "nova-3",
            Self::ElevenLabs => "scribe_v1",
        }
    }

    /// Whether this provider can also handle formatting (it's an LLM).
    pub fn can_also_format(&self) -> bool {
        matches!(self, Self::Gemini)
    }
}

/// Options that influence transcription behavior per-provider.
#[derive(Debug, Clone, Default)]
pub struct TranscriptionOptions {
    pub api_key: String,
    pub model: String,
    /// Deepgram smart formatting.
    pub dg_smart_format: bool,
    /// Deepgram keyword boosting (comma-separated).
    pub dg_keywords: String,
    /// Deepgram language code.
    pub dg_language: String,
    /// OpenAI language code.
    pub oai_language: String,
    /// OpenAI context prompt.
    pub oai_prompt: String,
    /// Gemini temperature (0.0 - 1.0).
    pub gemini_temperature: f64,
    /// ElevenLabs language code.
    pub el_language_code: String,
}

/// Maximum number of retries for transient failures.
const MAX_RETRIES: u32 = 3;

/// Transcribe the WAV file at `audio_path` using the specified provider.
///
/// Returns the transcribed text on success.
pub async fn transcribe(
    provider: TranscriptionProvider,
    audio_path: &Path,
    options: &TranscriptionOptions,
) -> Result<String, String> {
    match provider {
        TranscriptionProvider::None => transcribe_on_device(audio_path).await,
        TranscriptionProvider::Gemini => transcribe_gemini(audio_path, options, None).await,
        TranscriptionProvider::OpenAI => transcribe_openai(audio_path, options).await,
        TranscriptionProvider::Deepgram => transcribe_deepgram(audio_path, options).await,
        TranscriptionProvider::ElevenLabs => transcribe_elevenlabs(audio_path, options).await,
    }
}

/// Gemini one-shot: transcribe + format in a single API call.
/// Only works when both transcription and formatting use Gemini.
pub async fn transcribe_gemini_oneshot(
    audio_path: &Path,
    options: &TranscriptionOptions,
    style: FormattingStyle,
) -> Result<String, String> {
    transcribe_gemini(audio_path, options, Some(style)).await
}

/// Compute timeout based on audio file size.
/// 16-bit PCM WAV at typical sample rates ~ 64KB/s (conservative middle estimate).
fn compute_timeout(audio_len: usize) -> Duration {
    let estimated_seconds = audio_len as f64 / 64_000.0;
    let timeout_secs = f64::max(30.0, 30.0 + estimated_seconds);
    Duration::from_secs_f64(timeout_secs)
}

/// Resolve the model string: use provider default if empty.
fn resolve_model(model: &str, provider: TranscriptionProvider) -> String {
    if model.is_empty() {
        provider.default_model().to_string()
    } else {
        model.to_string()
    }
}

// ---------------------------------------------------------------------------
// Provider implementations
// ---------------------------------------------------------------------------

async fn transcribe_on_device(_audio_path: &Path) -> Result<String, String> {
    // On-device transcription is handled natively by the frontend/OS layer.
    // This Rust backend cannot directly invoke SFSpeechRecognizer or System.Speech.
    Err("on-device transcription not yet implemented".into())
}

async fn transcribe_gemini(
    audio_path: &Path,
    options: &TranscriptionOptions,
    style: Option<FormattingStyle>,
) -> Result<String, String> {
    let audio_data =
        std::fs::read(audio_path).map_err(|e| format!("failed to read audio file: {e}"))?;
    let timeout = compute_timeout(audio_data.len());
    let model = resolve_model(&options.model, TranscriptionProvider::Gemini);

    let base64_audio = base64::engine::general_purpose::STANDARD.encode(&audio_data);

    // Use style-specific audio prompt for one-shot, or plain transcription prompt
    let prompt = match style {
        Some(s) => audio_prompt_for_style(s),
        None => plain_transcription_prompt(),
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, options.api_key
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [
                {"inline_data": {"mime_type": "audio/wav", "data": base64_audio}},
                {"text": prompt}
            ]
        }],
        "generationConfig": {
            "temperature": options.gemini_temperature,
            "maxOutputTokens": 2048,
            "responseMimeType": "application/json"
        }
    });

    with_retry(MAX_RETRIES, || async {
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| (format!("Gemini request failed: {e}"), is_retryable_error(&e)))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| (format!("Gemini response read failed: {e}"), true))?;

        if !status.is_success() {
            // Try to extract error message from JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(msg) = json["error"]["message"].as_str() {
                    return Err((format!("Gemini API error: {msg}"), false));
                }
            }
            return Err((format!("Gemini API error (HTTP {status}): {text}"), false));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| (format!("Gemini parse failed: {e}"), true))?;

        // Check finishReason -- anything other than STOP means truncated/blocked
        let finish_reason = json["candidates"][0]["finishReason"]
            .as_str()
            .unwrap_or("UNKNOWN");
        if finish_reason != "STOP" {
            return Err((
                format!("Gemini finishReason: {finish_reason} (expected STOP)"),
                true,
            ));
        }

        let response_text = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| ("Gemini response missing text content".to_string(), true))?;

        Ok(extract_json(response_text))
    })
    .await
}

async fn transcribe_openai(
    audio_path: &Path,
    options: &TranscriptionOptions,
) -> Result<String, String> {
    let audio_data =
        std::fs::read(audio_path).map_err(|e| format!("failed to read audio file: {e}"))?;
    let timeout = compute_timeout(audio_data.len());
    let model = resolve_model(&options.model, TranscriptionProvider::OpenAI);

    with_retry(MAX_RETRIES, || {
        let audio_data = audio_data.clone();
        let model = model.clone();
        let api_key = options.api_key.clone();
        let oai_language = options.oai_language.clone();
        let oai_prompt = options.oai_prompt.clone();

        async move {
            let file_part = multipart::Part::bytes(audio_data)
                .file_name("recording.wav")
                .mime_str("audio/wav")
                .map_err(|e| (format!("multipart error: {e}"), false))?;

            let mut form = multipart::Form::new()
                .part("file", file_part)
                .text("model", model);

            if !oai_language.is_empty() {
                form = form.text("language", oai_language);
            }
            if !oai_prompt.is_empty() {
                form = form.text("prompt", oai_prompt);
            }

            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.openai.com/v1/audio/transcriptions")
                .header("Authorization", format!("Bearer {}", api_key))
                .timeout(timeout)
                .multipart(form)
                .send()
                .await
                .map_err(|e| (format!("OpenAI request failed: {e}"), is_retryable_error(&e)))?;

            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| (format!("OpenAI response read failed: {e}"), true))?;

            if !status.is_success() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(msg) = json["error"]["message"].as_str() {
                        return Err((format!("OpenAI API error: {msg}"), false));
                    }
                }
                return Err((format!("OpenAI API error (HTTP {status}): {text}"), false));
            }

            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| (format!("OpenAI parse failed: {e}"), true))?;

            json["text"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ("OpenAI response missing 'text' field".to_string(), true))
        }
    })
    .await
}

async fn transcribe_deepgram(
    audio_path: &Path,
    options: &TranscriptionOptions,
) -> Result<String, String> {
    let audio_data =
        std::fs::read(audio_path).map_err(|e| format!("failed to read audio file: {e}"))?;
    let timeout = compute_timeout(audio_data.len());
    let model = resolve_model(&options.model, TranscriptionProvider::Deepgram);

    // Build query parameters
    let mut params = vec![format!("model={}", model)];
    if options.dg_smart_format {
        params.push("smart_format=true".to_string());
    }
    if !options.dg_language.is_empty() {
        params.push(format!("language={}", options.dg_language));
    }
    // Keyword boosting
    for kw in options
        .dg_keywords
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        params.push(format!(
            "keywords={}",
            urlencoding_simple(kw)
        ));
    }

    let url = format!(
        "https://api.deepgram.com/v1/listen?{}",
        params.join("&")
    );

    with_retry(MAX_RETRIES, || {
        let audio_data = audio_data.clone();
        let url = url.clone();
        let api_key = options.api_key.clone();

        async move {
            let client = reqwest::Client::new();
            let resp = client
                .post(&url)
                .header("Authorization", format!("Token {}", api_key))
                .header("Content-Type", "audio/wav")
                .timeout(timeout)
                .body(audio_data)
                .send()
                .await
                .map_err(|e| (format!("Deepgram request failed: {e}"), is_retryable_error(&e)))?;

            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| (format!("Deepgram response read failed: {e}"), true))?;

            if !status.is_success() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(msg) = json["err_msg"].as_str() {
                        return Err((format!("Deepgram API error: {msg}"), false));
                    }
                }
                return Err((format!("Deepgram API error (HTTP {status}): {text}"), false));
            }

            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| (format!("Deepgram parse failed: {e}"), true))?;

            json["results"]["channels"][0]["alternatives"][0]["transcript"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ("Deepgram response missing transcript".to_string(), true))
        }
    })
    .await
}

async fn transcribe_elevenlabs(
    audio_path: &Path,
    options: &TranscriptionOptions,
) -> Result<String, String> {
    let audio_data =
        std::fs::read(audio_path).map_err(|e| format!("failed to read audio file: {e}"))?;
    let timeout = compute_timeout(audio_data.len());
    let model = resolve_model(&options.model, TranscriptionProvider::ElevenLabs);

    with_retry(MAX_RETRIES, || {
        let audio_data = audio_data.clone();
        let model = model.clone();
        let api_key = options.api_key.clone();
        let el_language_code = options.el_language_code.clone();

        async move {
            let file_part = multipart::Part::bytes(audio_data)
                .file_name("recording.wav")
                .mime_str("audio/wav")
                .map_err(|e| (format!("multipart error: {e}"), false))?;

            let mut form = multipart::Form::new()
                .part("file", file_part)
                .text("model_id", model);

            if !el_language_code.is_empty() {
                form = form.text("language_code", el_language_code);
            }

            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.elevenlabs.io/v1/speech-to-text")
                .header("xi-api-key", api_key)
                .timeout(timeout)
                .multipart(form)
                .send()
                .await
                .map_err(|e| {
                    (
                        format!("ElevenLabs request failed: {e}"),
                        is_retryable_error(&e),
                    )
                })?;

            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| (format!("ElevenLabs response read failed: {e}"), true))?;

            if !status.is_success() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(msg) = json["detail"]["message"].as_str() {
                        return Err((format!("ElevenLabs API error: {msg}"), false));
                    }
                }
                return Err((
                    format!("ElevenLabs API error (HTTP {status}): {text}"),
                    false,
                ));
            }

            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| (format!("ElevenLabs parse failed: {e}"), true))?;

            json["text"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ("ElevenLabs response missing 'text' field".to_string(), true))
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// Retry helper
// ---------------------------------------------------------------------------

/// Retry an async operation up to `max_retries` times.
/// The closure returns `Result<T, (String, bool)>` where the bool indicates
/// whether the error is retryable.
async fn with_retry<F, Fut, T>(max_retries: u32, f: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, (String, bool)>>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match f().await {
            Ok(val) => return Ok(val),
            Err((msg, retryable)) => {
                if retryable && attempt < max_retries {
                    // Backoff: 0.5s * attempt
                    let backoff = Duration::from_millis(500 * attempt as u64);
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                return Err(msg);
            }
        }
    }
}

/// Check if a reqwest error is retryable (timeout or connection issue).
fn is_retryable_error(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request()
}

// ---------------------------------------------------------------------------
// JSON extraction helper
// ---------------------------------------------------------------------------

/// Extract the "text" field from a JSON response string.
/// Handles markdown code fences and searches for `{"text": "..."}` patterns.
pub fn extract_json(text: &str) -> String {
    let mut s = text.trim().to_string();

    // Strip markdown code fences
    if s.starts_with("```json") {
        s = s[7..].to_string();
    } else if s.starts_with("```") {
        s = s[3..].to_string();
    }
    if s.ends_with("```") {
        s = s[..s.len() - 3].to_string();
    }
    s = s.trim().to_string();

    // Try direct JSON parse
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&s) {
        if let Some(text_val) = json["text"].as_str() {
            return text_val.to_string();
        }
    }

    // Try to find JSON object anywhere in the string
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            if end > start {
                let json_slice = &s[start..=end];
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_slice) {
                    if let Some(text_val) = json["text"].as_str() {
                        return text_val.to_string();
                    }
                }
            }
        }
    }

    s
}

// ---------------------------------------------------------------------------
// Prompt strings (ported from Swift)
// ---------------------------------------------------------------------------

const DICTATION_COMMANDS: &str = r#"DICTATION COMMANDS — when the speaker says any of these, insert the symbol instead of the words: "period" or "full stop" → . | "comma" → , | "question mark" → ? | "exclamation mark" or "exclamation point" → ! "colon" → : | "semicolon" → ; | "open parenthesis" or "open paren" → ( | "close parenthesis" or "close paren" → ) "open bracket" → [ | "close bracket" → ] | "open brace" or "open curly" → { | "close brace" or "close curly" → } "open quote" or "open quotes" → " | "close quote" or "close quotes" or "end quote" → " "dash" or "em dash" → — | "hyphen" → - | "ellipsis" or "dot dot dot" → … "new line" or "newline" → insert a line break | "new paragraph" → insert two line breaks "ampersand" → & | "at sign" → @ | "hashtag" or "hash" → # | "dollar sign" → $ | "percent" or "percent sign" → % "asterisk" or "star" → * | "slash" or "forward slash" → / | "backslash" → \ "underscore" → _ | "pipe" → | | "tilde" → ~ | "caret" → ^ Only convert these when the speaker clearly intends them as punctuation commands, not when used naturally in speech."#;

const NOISE_RULE: &str = "IGNORE all background noise, sound effects, music, and non-speech sounds. Only transcribe human speech. If there is no speech, respond with {\"text\":\"\"}.";

/// Audio prompt for one-shot transcribe+format (Gemini).
pub fn audio_prompt_for_style(style: FormattingStyle) -> String {
    match style {
        FormattingStyle::Casual => format!(
            "Transcribe this audio. Remove filler sounds (um, uh, er) but keep everything else exactly as spoken — \
            casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. \
            {} \
            {} \
            You MUST respond with ONLY a JSON object: {{\"text\":\"transcription here\"}}",
            DICTATION_COMMANDS, NOISE_RULE
        ),
        FormattingStyle::Formatted => format!(
            "Transcribe this audio. Remove filler words (um, uh, er, like, you know). \
            Fix punctuation and capitalization. Keep the speaker's EXACT words and sentence structure — \
            do not rephrase or rewrite. Keep contractions as spoken. Only fix obvious grammar errors. \
            {} \
            {} \
            You MUST respond with ONLY a JSON object: {{\"text\":\"transcription here\"}}",
            DICTATION_COMMANDS, NOISE_RULE
        ),
        FormattingStyle::Professional => format!(
            "Transcribe this audio. Remove all filler words. Elevate the language to sound polished and professional. \
            Fix grammar, improve word choice, use proper punctuation and capitalization. \
            Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. \
            {} \
            {} \
            You MUST respond with ONLY a JSON object: {{\"text\":\"transcription here\"}}",
            DICTATION_COMMANDS, NOISE_RULE
        ),
    }
}

/// Plain transcription prompt (no formatting, for when formatting is handled separately).
pub fn plain_transcription_prompt() -> String {
    format!(
        "Transcribe this audio exactly as spoken, with proper punctuation and capitalization. \
        {} \
        {} \
        You MUST respond with ONLY a JSON object: {{\"text\":\"transcription here\"}}",
        DICTATION_COMMANDS, NOISE_RULE
    )
}

// ---------------------------------------------------------------------------
// Simple URL encoding for keywords
// ---------------------------------------------------------------------------

fn urlencoding_simple(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}
