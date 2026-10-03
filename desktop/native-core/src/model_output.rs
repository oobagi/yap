//! Shared contract for generated dictation: only a complete JSON text result
//! may enter the paste pipeline. Never salvage prose or partial JSON.

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextOutput {
    text: String,
}

pub fn text_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "text": { "type": "string" } },
        "required": ["text"],
        "additionalProperties": false
    })
}

/// Empty text is valid for silent audio; formatters reject it separately.
pub fn parse_text(content: &str) -> Result<String, String> {
    // Serde also accepts positional arrays for structs; our contract requires
    // an object so an array such as ["Hello."] must not become pasteable text.
    if !content.trim_start().starts_with('{') {
        return Err("Model returned invalid output: expected a JSON object".into());
    }
    serde_json::from_str::<TextOutput>(content)
        .map(|output| output.text)
        .map_err(|_| {
            "Model returned invalid output: expected a JSON object with only a text string".into()
        })
}

/// Gemini may include thought parts before the final text. Only consume the
/// final answer, and require a completed candidate even if partial JSON parses.
pub fn parse_gemini_response(response: &Value) -> Result<String, String> {
    let candidate = &response["candidates"][0];
    if candidate["finishReason"].as_str() != Some("STOP") {
        return Err("Gemini returned incomplete or blocked output".into());
    }
    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or("Gemini response missing content parts")?;
    let mut content = String::new();
    for part in parts {
        if part["thought"].as_bool() == Some(true) {
            continue;
        }
        content.push_str(
            part["text"]
                .as_str()
                .ok_or("Gemini response contained non-text output")?,
        );
    }
    parse_text(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_text_preserves_unicode_markup_and_silence() {
        for text in [
            "Hello.",
            "",
            "Café\nRésumé",
            "<input>Hello.</input>",
            "Use {braces}.",
        ] {
            assert_eq!(
                parse_text(&json!({"text": text}).to_string()).unwrap(),
                text
            );
        }
    }

    #[test]
    fn malformed_or_unstructured_output_is_never_pasteable() {
        for content in [
            "<input>Hello.</input>",
            "Here is your cleaned text: Hello.",
            "Here is the result: {\"text\":\"Hello.\"}",
            "```json\n{\"text\":\"Hello.\"}\n```",
            "{\"text\":\"Partial",
            "{\"text\":\"Hello.\"} trailing explanation",
            "{\"text\":\"Hello.\"}{\"text\":\"Again.\"}",
            "{\"text\":\"Hello.\",\"reasoning\":\"My thoughts\"}",
            "{\"text\":\"Hello.\",\"text\":\"Again.\"}",
            "{\"text\":null}",
            "{\"text\":123}",
            "{}",
            "[]",
            "[\"Hello.\"]",
        ] {
            assert!(parse_text(content).is_err(), "accepted {content}");
        }
    }

    #[test]
    fn gemini_uses_only_completed_final_output() {
        let silence = json!({"candidates": [{"finishReason": "STOP", "content": {
            "parts": [{"text": "{\"text\":\"\"}"}]
        }}]});
        assert_eq!(parse_gemini_response(&silence).unwrap(), "");
        let mut response = json!({"candidates": [{
            "finishReason": "STOP",
            "content": {"parts": [
                {"thought": true, "text": "Internal reasoning"},
                {"text": "{\"text\":\"Hello.\"}"}
            ]}
        }]});
        assert_eq!(parse_gemini_response(&response).unwrap(), "Hello.");
        for reason in ["MAX_TOKENS", "SAFETY", "OTHER"] {
            response["candidates"][0]["finishReason"] = json!(reason);
            assert!(parse_gemini_response(&response).is_err());
        }
        assert!(parse_gemini_response(&json!({"candidates": []})).is_err());
        assert!(parse_gemini_response(&json!({"candidates": [{
            "finishReason": "STOP", "content": {"parts": [
                {"thought": true, "text": "{\"text\":\"Thinking only\"}"}
            ]}
        }]}))
        .is_err());
    }
}
