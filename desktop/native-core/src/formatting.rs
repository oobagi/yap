use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::apple_format;
use crate::ollama;
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
    Apple,
    Ollama,
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
            Self::Apple => "",
            Self::Ollama => "qwen3.5:4b",
        }
    }

    pub fn requires_api_key(&self) -> bool {
        matches!(
            self,
            Self::Gemini | Self::OpenAI | Self::Anthropic | Self::Groq
        )
    }

    fn falls_back_to_raw_text(&self) -> bool {
        matches!(self, Self::Apple | Self::Ollama)
    }
}

/// Formatting style applied by the LLM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormattingStyle {
    Casual,
    Formatted,
    Professional,
    Custom,
}

impl Default for FormattingStyle {
    fn default() -> Self {
        Self::Formatted
    }
}

impl FormattingStyle {
    /// System prompt for formatting already-transcribed text.
    pub fn base_prompt(&self) -> &'static str {
        match self {
            Self::Casual => CASUAL_PROMPT,
            Self::Formatted => FORMATTED_PROMPT,
            Self::Professional => PROFESSIONAL_PROMPT,
            Self::Custom => FORMATTED_PROMPT,
        }
    }

    pub fn prompt(&self, custom_prompt: &str) -> String {
        if *self != Self::Custom {
            return self.base_prompt().to_string();
        }

        let custom_prompt = custom_prompt.trim();
        if custom_prompt.is_empty() {
            FORMATTED_PROMPT.to_string()
        } else {
            custom_formatter_prompt(custom_prompt)
        }
    }
}

pub fn resolved_instruction(style: FormattingStyle, custom_prompt: &str) -> String {
    style.prompt(custom_prompt)
}

// ---------------------------------------------------------------------------
// Prompt strings
// ---------------------------------------------------------------------------

const CASUAL_PROMPT: &str = r#"You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} Rules: remove ONLY filler sounds (um, uh, er). Keep the speaker's words, casual phrasing, slang, contractions, and meaning. All lowercase. Minimal punctuation. PRESERVE all existing symbols — parentheses, quotes, brackets, etc. Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). If the speaker dictates an ordered list, format it as separate numbered lines using digits and periods: "1. item", "2. item", "3. item". Do not spell list numbers as words when they are being used as list markers. NEVER respond conversationally. ONLY output the JSON object."#;

const FORMATTED_PROMPT: &str = r#"You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} Rules: remove filler words (um, uh, er, like, you know). Fix punctuation and capitalization. Keep the speaker's words and meaning — do not rephrase or rewrite the substance. Keep contractions as spoken. Only fix obvious grammar errors. PRESERVE all existing symbols — parentheses, quotes, brackets, etc. Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). If the speaker dictates an ordered list, format it as separate numbered lines using digits and periods: "1. item", "2. item", "3. item". Do not spell list numbers as words when they are being used as list markers. NEVER respond conversationally. ONLY output the JSON object."#;

const PROFESSIONAL_PROMPT: &str = r#"You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} Rules: remove all filler words. Elevate the language to sound polished and professional. Fix grammar, improve word choice, use proper punctuation and capitalization. Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. PRESERVE all existing symbols — parentheses, quotes, brackets, etc. Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). If the speaker dictates an ordered list, format it as separate numbered lines using digits and periods: "1. item", "2. item", "3. item". Do not spell list numbers as words when they are being used as list markers. NEVER respond conversationally. ONLY output the JSON object."#;

const CUSTOM_FORMATTER_PROMPT: &str = r#"You transform spoken transcription text. You MUST respond with ONLY a JSON object: {"text":"transformed version here"}

The user's custom instructions are the PRIMARY TASK. Apply them literally and strongly. Do not treat them as optional style preferences.
If the custom instructions conflict with any default cleanup behavior, follow the custom instructions.
Return exactly one final transformed text. Do not include both the original input and a transformed version.
Do not add headings, labels, examples, placeholder items, or extra sections unless the user explicitly asks for them.
If the custom instructions include examples like "Period" => ".", treat those examples as conversion rules.
Unless the custom instructions require changing the words, preserve the speaker's meaning and wording. Remove filler words (um, uh, er, like, you know), fix obvious punctuation/capitalization, and convert spoken punctuation commands to symbols.
If the speaker dictates an ordered list and the custom instructions do not say otherwise, format it as separate numbered lines using digits and periods: "1. item", "2. item", "3. item". Do not spell list numbers as words when they are being used as list markers.
NEVER explain, apologize, refuse, or respond conversationally. ONLY output the JSON object.

User custom instructions:"#;

pub fn custom_formatter_prompt(custom_prompt: &str) -> String {
    format!("{CUSTOM_FORMATTER_PROMPT}\n{}", custom_prompt.trim())
}

/// Options for the formatting call.
#[derive(Debug, Clone, Default)]
pub struct FormattingOptions {
    pub api_key: String,
    pub model: String,
    pub style: FormattingStyle,
    pub custom_prompt: String,
}

/// Formatting timeout.
const FORMAT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub struct FormattingResult {
    pub text: String,
    pub applied: bool,
}

impl FormattingResult {
    fn unchanged(text: &str) -> Self {
        Self {
            text: text.to_string(),
            applied: false,
        }
    }
}

/// Format the raw transcription text using the specified LLM provider.
///
/// Returns the formatted text on success. If provider is `None`, returns
/// the input text unchanged. Provider errors are returned to the caller so the
/// runtime can surface them through the normal error state.
pub async fn format(
    provider: FormattingProvider,
    text: &str,
    options: &FormattingOptions,
) -> Result<FormattingResult, String> {
    // Short text: pass through
    let trimmed = text.trim();
    if trimmed.len() < 3 {
        return Ok(FormattingResult::unchanged(text));
    }

    if provider.requires_api_key() && options.api_key.is_empty() {
        return Ok(FormattingResult::unchanged(text));
    }

    let result = match provider {
        FormattingProvider::None => return Ok(FormattingResult::unchanged(text)),
        FormattingProvider::Gemini => format_gemini(text, options).await,
        FormattingProvider::OpenAI => format_openai(text, options).await,
        FormattingProvider::Anthropic => format_anthropic(text, options).await,
        FormattingProvider::Groq => format_groq(text, options).await,
        FormattingProvider::Apple => format_apple(text, options).await,
        FormattingProvider::Ollama => format_ollama(text, options).await,
    };

    finish_formatting(provider, text, options, result)
}

fn finish_formatting(
    provider: FormattingProvider,
    text: &str,
    options: &FormattingOptions,
    result: Result<String, String>,
) -> Result<FormattingResult, String> {
    let result = result.and_then(|formatted| {
        if formatted.trim().is_empty() {
            Err("Formatter returned empty text".to_string())
        } else {
            Ok(formatted)
        }
    });
    match result {
        Ok(formatted) => {
            let formatted = render_formatted_text_for_paste(&formatted);
            if options.style == FormattingStyle::Custom
                && !options.custom_prompt.trim().is_empty()
                && formatted.trim() == text.trim()
            {
                crate::log::info(&format!(
                    "Custom formatting returned unchanged text for {}",
                    provider_label(provider)
                ));
            }
            Ok(FormattingResult {
                text: formatted,
                applied: true,
            })
        }
        Err(error) => {
            let error = sanitize_error(&error);
            crate::log::info(&format!(
                "Formatting failed for {}: {}",
                provider_label(provider),
                error
            ));
            if provider.falls_back_to_raw_text() {
                return Ok(FormattingResult::unchanged(text));
            }
            Err(error)
        }
    }
}

pub fn render_formatted_text_for_paste(text: &str) -> String {
    let marker_positions = ordered_list_marker_positions(text);
    if marker_positions.len() < 2 {
        return text.to_string();
    }

    let marker_positions = marker_positions
        .into_iter()
        .map(|(index, _)| index)
        .collect::<std::collections::HashSet<_>>();
    let mut rendered = String::with_capacity(text.len() + marker_positions.len());

    for (index, ch) in text.char_indices() {
        if marker_positions.contains(&index) {
            trim_horizontal_space(&mut rendered);
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                if rendered.ends_with(',') || rendered.ends_with(';') {
                    rendered.pop();
                    trim_horizontal_space(&mut rendered);
                }
                rendered.push('\n');
            }
        }
        rendered.push(ch);
    }

    rendered
}

fn trim_horizontal_space(value: &mut String) {
    while value.ends_with(' ') || value.ends_with('\t') {
        value.pop();
    }
}

fn ordered_list_marker_positions(text: &str) -> Vec<(usize, u16)> {
    let mut markers = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some((end, number)) = ordered_list_marker_at(bytes, index) {
            markers.push((index, number));
            index = end;
        } else {
            index += 1;
        }
    }

    if !has_sequential_markers(&markers) {
        return Vec::new();
    }
    markers
}

fn ordered_list_marker_at(bytes: &[u8], index: usize) -> Option<(usize, u16)> {
    if index > 0 && !bytes[index - 1].is_ascii_whitespace() {
        return None;
    }

    let mut cursor = index;
    let mut number = 0u16;
    let mut digits = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && digits < 3 {
        number = number * 10 + u16::from(bytes[cursor] - b'0');
        cursor += 1;
        digits += 1;
    }

    if digits == 0 || number == 0 || cursor >= bytes.len() {
        return None;
    }

    if bytes[cursor] != b'.' && bytes[cursor] != b')' {
        return None;
    }
    cursor += 1;

    if cursor >= bytes.len() || !bytes[cursor].is_ascii_whitespace() {
        return None;
    }

    Some((cursor, number))
}

fn has_sequential_markers(markers: &[(usize, u16)]) -> bool {
    markers
        .windows(2)
        .any(|pair| pair[0].1.saturating_add(1) == pair[1].1)
}

/// Resolve the model string: use provider default if empty.
fn resolve_model(model: &str, provider: FormattingProvider) -> String {
    if model.is_empty() {
        provider.default_model().to_string()
    } else {
        model.to_string()
    }
}

fn provider_label(provider: FormattingProvider) -> &'static str {
    match provider {
        FormattingProvider::None => "none",
        FormattingProvider::Gemini => "gemini",
        FormattingProvider::OpenAI => "openai",
        FormattingProvider::Anthropic => "anthropic",
        FormattingProvider::Groq => "groq",
        FormattingProvider::Apple => "apple",
        FormattingProvider::Ollama => "ollama",
    }
}

fn sanitize_error(error: &str) -> String {
    let Some(start) = error.find("key=") else {
        return error.to_string();
    };
    let suffix = &error[start..];
    let end = suffix.find(['&', ' ', ')']).unwrap_or(suffix.len());
    error.replace(&suffix[..end], "key=<redacted>")
}

fn api_error_message(json: &serde_json::Value) -> Option<&str> {
    json["error"]["message"]
        .as_str()
        .or_else(|| json["error"].as_str())
        .or_else(|| json["message"].as_str())
}

// ---------------------------------------------------------------------------
// Provider implementations
// ---------------------------------------------------------------------------

async fn format_apple(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let text = text.to_string();
    let prompt = options.style.prompt(&options.custom_prompt);
    tokio::task::spawn_blocking(move || apple_format::format(&text, &prompt, FORMAT_TIMEOUT))
        .await
        .map_err(|error| format!("Foundation Models formatter task failed: {error}"))?
}

async fn format_ollama(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::Ollama);
    let prompt = options.style.prompt(&options.custom_prompt);
    ollama::format_text(text, &prompt, &model, FORMAT_TIMEOUT).await
}

async fn format_gemini(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::Gemini);
    let prompt = options.style.prompt(&options.custom_prompt);

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

    let status = resp.status();
    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Gemini format response read failed: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("Gemini format parse failed: {e}"))?;

    if !status.is_success() {
        let message = json["error"]["message"]
            .as_str()
            .unwrap_or("unknown Gemini API error");
        return Err(format!(
            "Gemini format API error (HTTP {status}): {message}"
        ));
    }

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
    let prompt = options.style.prompt(&options.custom_prompt);

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

    let status = resp.status();
    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("OpenAI format response read failed: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("OpenAI format parse failed: {e}"))?;

    if !status.is_success() {
        let message = api_error_message(&json).unwrap_or("unknown OpenAI API error");
        return Err(format!(
            "OpenAI format API error (HTTP {status}): {message}"
        ));
    }

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "OpenAI format response missing content".to_string())?;

    Ok(extract_json(content))
}

async fn format_anthropic(text: &str, options: &FormattingOptions) -> Result<String, String> {
    let model = resolve_model(&options.model, FormattingProvider::Anthropic);
    let prompt = options.style.prompt(&options.custom_prompt);

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

    let status = resp.status();
    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Anthropic format response read failed: {e}"))?;

    let json: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("Anthropic format parse failed: {e}"))?;

    if !status.is_success() {
        let message = api_error_message(&json).unwrap_or("unknown Anthropic API error");
        return Err(format!(
            "Anthropic format API error (HTTP {status}): {message}"
        ));
    }

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
    let prompt = options.style.prompt(&options.custom_prompt);

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

    let status = resp.status();
    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Groq format response read failed: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("Groq format parse failed: {e}"))?;

    if !status.is_success() {
        let message = api_error_message(&json).unwrap_or("unknown Groq API error");
        return Err(format!("Groq format API error (HTTP {status}): {message}"));
    }

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "Groq format response missing content".to_string())?;

    Ok(extract_json(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_provider_failure_preserves_dictation_without_claiming_formatting() {
        for provider in [FormattingProvider::Apple, FormattingProvider::Ollama] {
            for output in [Err("provider unavailable".into()), Ok("  ".into())] {
                let result = finish_formatting(
                    provider,
                    "keep my words",
                    &FormattingOptions::default(),
                    output,
                )
                .unwrap();
                assert_eq!(result.text, "keep my words");
                assert!(!result.applied);
            }
        }
    }

    #[test]
    fn successful_unchanged_output_is_still_a_completed_formatting_request() {
        let result = finish_formatting(
            FormattingProvider::Ollama,
            "Hello.",
            &FormattingOptions::default(),
            Ok("Hello.".into()),
        )
        .unwrap();
        assert_eq!(result.text, "Hello.");
        assert!(result.applied);
    }

    #[test]
    fn custom_prompt_is_primary_task() {
        let prompt = FormattingStyle::Custom.prompt("replace every word with duck");

        assert!(prompt.contains("PRIMARY TASK"));
        assert!(prompt.contains("replace every word with duck"));
        assert!(!prompt.contains("Keep the speaker's EXACT words"));
        assert!(!prompt.contains("EXACT words and sentence structure"));
    }

    #[test]
    fn prompts_request_digit_numbered_lists_without_literal_newline_rule() {
        let prompt = FormattingStyle::Formatted.prompt("");
        let custom_prompt = FormattingStyle::Custom.prompt("remove corrections and filler");

        assert!(prompt.contains("separate numbered lines"));
        assert!(prompt.contains("Do not spell list numbers as words"));
        assert!(!prompt.contains("Do not write literal"));
        assert!(custom_prompt.contains("separate numbered lines"));
    }

    #[test]
    fn render_pass_moves_inline_numbered_list_markers_to_lines() {
        let input =
            "Here are my reasons: 1. this feature works, 2. it is decent, 3. I want to keep it.";
        let rendered = render_formatted_text_for_paste(input);

        assert_eq!(
            rendered,
            "Here are my reasons:\n1. this feature works\n2. it is decent\n3. I want to keep it."
        );
    }

    #[test]
    fn render_pass_handles_parenthesized_markers_and_semicolons() {
        let input = "Plan: 1) test it; 2) ship it; 3) watch logs.";
        let rendered = render_formatted_text_for_paste(input);

        assert_eq!(rendered, "Plan:\n1) test it\n2) ship it\n3) watch logs.");
    }

    #[test]
    fn render_pass_preserves_existing_numbered_list_lines() {
        let input = "1. this feature works\n2. it is decent\n3. I want to keep it";
        let rendered = render_formatted_text_for_paste(input);

        assert_eq!(rendered, input);
    }

    #[test]
    fn render_pass_ignores_single_marker_and_decimals() {
        assert_eq!(
            render_formatted_text_for_paste("Use option 1. if you want it."),
            "Use option 1. if you want it."
        );
        assert_eq!(
            render_formatted_text_for_paste("Version 1.2 is faster than version 1.1."),
            "Version 1.2 is faster than version 1.1."
        );
    }

    #[test]
    #[ignore = "requires a running local Ollama service and an installed formatter model"]
    fn ollama_actual_model_output_survives_render_pass() {
        let model =
            std::env::var("YAP_OLLAMA_TEST_MODEL").unwrap_or_else(|_| "qwen3.5:4b".to_string());
        let input = "This is a test of one if this feature works two if it's decent and three if I want to keep it.";
        let options = FormattingOptions {
            model,
            style: FormattingStyle::Formatted,
            ..Default::default()
        };

        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let raw_model_output = runtime
            .block_on(super::format_ollama(input, &options))
            .expect("Ollama model output");
        let rendered = render_formatted_text_for_paste(&raw_model_output);

        assert!(
            !has_inline_ordered_list_sequence(&rendered),
            "raw model output:\n{raw_model_output}\n\nrendered output:\n{rendered}"
        );
    }

    fn has_inline_ordered_list_sequence(text: &str) -> bool {
        text.lines().any(|line| {
            let markers = ordered_list_marker_positions(line);
            markers.len() > 1
        })
    }
}
