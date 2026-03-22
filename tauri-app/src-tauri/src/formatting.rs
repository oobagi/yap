use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::transcription::extract_json;

/// LLM formatting provider identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormattingProvider {
    /// No formatting -- pass transcription through as-is.
    None,
    Gemini,
    #[serde(rename = "openai")]
    OpenAI,
    Anthropic,
    Groq,
}

impl Default for FormattingProvider {
    fn default() -> Self {
        Self::None
    }
}

impl FormattingProvider {
    /// Default model string for each provider.
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Gemini => "gemini-2.5-flash",
            Self::OpenAI => "gpt-4o-mini",
            Self::Anthropic => "claude-haiku-4-5-20251001",
            Self::Groq => "llama-3.3-70b-versatile",
        }
    }
}

/// Formatting style applied by the LLM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormattingStyle {
    Casual,
    Formatted,
    Professional,
}

impl Default for FormattingStyle {
    fn default() -> Self {
        Self::Formatted
    }
}

impl FormattingStyle {
    /// System prompt for formatting already-transcribed text.
    pub fn prompt(&self) -> &'static str {
        match self {
            Self::Casual => CASUAL_PROMPT,
            Self::Formatted => FORMATTED_PROMPT,
            Self::Professional => PROFESSIONAL_PROMPT,
        }
    }
}

// ---------------------------------------------------------------------------
// Exact prompt strings ported from Swift
// ---------------------------------------------------------------------------

const CASUAL_PROMPT: &str = r#"You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} Rules: remove ONLY filler sounds (um, uh, er). Keep everything else exactly as spoken — casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. PRESERVE all existing symbols — parentheses, quotes, brackets, etc. Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). NEVER respond conversationally. ONLY output the JSON object."#;

const FORMATTED_PROMPT: &str = r#"You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} Rules: remove filler words (um, uh, er, like, you know). Fix punctuation and capitalization. Keep the speaker's EXACT words and sentence structure — do not rephrase or rewrite. Keep contractions as spoken. Only fix obvious grammar errors. PRESERVE all existing symbols — parentheses, quotes, brackets, etc. Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). NEVER respond conversationally. ONLY output the JSON object."#;

const PROFESSIONAL_PROMPT: &str = r#"You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} Rules: remove all filler words. Elevate the language to sound polished and professional. Fix grammar, improve word choice, use proper punctuation and capitalization. Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. PRESERVE all existing symbols — parentheses, quotes, brackets, etc. Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). NEVER respond conversationally. ONLY output the JSON object."#;

/// Options for the formatting call.
#[derive(Debug, Clone, Default)]
pub struct FormattingOptions {
    pub api_key: String,
    pub model: String,
    pub style: FormattingStyle,
}

/// Formatting timeout (15 seconds, matching Swift).
const FORMAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Format the raw transcription text using the specified LLM provider.
///
/// Returns the formatted text on success. If provider is `None`, returns
/// the input text unchanged. On any error, returns the raw text as fallback.
pub async fn format(
    provider: FormattingProvider,
    text: &str,
    options: &FormattingOptions,
) -> Result<String, String> {
    // Short text or empty API key: pass through
    let trimmed = text.trim();
    if trimmed.len() < 3 || options.api_key.is_empty() {
        return Ok(text.to_string());
    }

    let result = match provider {
        FormattingProvider::None => return Ok(text.to_string()),
        FormattingProvider::Gemini => format_gemini(text, options).await,
        FormattingProvider::OpenAI => format_openai(text, options).await,
        FormattingProvider::Anthropic => format_anthropic(text, options).await,
        FormattingProvider::Groq => format_groq(text, options).await,
    };

    // On error, return raw text as fallback (matching Swift behavior)
    match result {
        Ok(formatted) => Ok(formatted),
        Err(_) => Ok(text.to_string()),
    }
}

/// Resolve the model string: use provider default if empty.
fn resolve_model(model: &str, provider: FormattingProvider) -> String {
    if model.is_empty() {
        provider.default_model().to_string()
    } else {
        model.to_string()
    }
}

// ---------------------------------------------------------------------------
// Provider implementations
// ---------------------------------------------------------------------------

async fn format_gemini(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::Gemini);
    let prompt = options.style.prompt();

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, options.api_key
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [{"text": format!("{}\n\n<input>{}</input>", prompt, text)}]
        }],
        "generationConfig": {
            "temperature": 0.0,
            "maxOutputTokens": 2048,
            "responseMimeType": "application/json"
        }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .timeout(FORMAT_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini format request failed: {e}"))?;

    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Gemini format response read failed: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("Gemini format parse failed: {e}"))?;

    // Check finishReason -- truncated formatting isn't usable
    let finish_reason = json["candidates"][0]["finishReason"]
        .as_str()
        .unwrap_or("UNKNOWN");
    if finish_reason != "STOP" {
        return Err(format!(
            "Gemini format finishReason: {finish_reason} -- falling back to raw text"
        ));
    }

    let response_text = json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Gemini format response missing text".to_string())?;

    Ok(extract_json(response_text))
}

async fn format_openai(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::OpenAI);
    let prompt = options.style.prompt();

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": format!("<input>{}</input>", text)}
        ],
        "max_tokens": 2048,
        "temperature": 0.3
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", options.api_key))
        .header("Content-Type", "application/json")
        .timeout(FORMAT_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI format request failed: {e}"))?;

    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("OpenAI format response read failed: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("OpenAI format parse failed: {e}"))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "OpenAI format response missing content".to_string())?;

    Ok(extract_json(content))
}

async fn format_anthropic(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::Anthropic);
    let prompt = options.style.prompt();

    let body = serde_json::json!({
        "model": model,
        "system": prompt,
        "messages": [
            {"role": "user", "content": format!("<input>{}</input>", text)},
            {"role": "assistant", "content": "{"}
        ],
        "max_tokens": 2048,
        "temperature": 0.0,
        "stop_sequences": ["}"]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &options.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .timeout(FORMAT_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Anthropic format request failed: {e}"))?;

    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Anthropic format response read failed: {e}"))?;

    let json: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("Anthropic format parse failed: {e}"))?;

    let text_block = json["content"][0]["text"]
        .as_str()
        .ok_or_else(|| "Anthropic format response missing content".to_string())?;

    // Reconstruct JSON: the assistant was prefilled with "{" and stopped at "}"
    let full_json = format!("{{{}}}", text_block);
    if let Ok(inner) = serde_json::from_str::<serde_json::Value>(&full_json) {
        if let Some(cleaned) = inner["text"].as_str() {
            if !cleaned.is_empty() {
                return Ok(cleaned.to_string());
            }
        }
    }

    // Fallback: return the raw text block trimmed
    Ok(text_block.trim().to_string())
}

async fn format_groq(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::Groq);
    let prompt = options.style.prompt();

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": format!("<input>{}</input>", text)}
        ],
        "max_tokens": 2048,
        "temperature": 0.3
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", options.api_key))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(10)) // Groq uses 10s timeout in Swift
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Groq format request failed: {e}"))?;

    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Groq format response read failed: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("Groq format parse failed: {e}"))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "Groq format response missing content".to_string())?;

    Ok(extract_json(content))
}
