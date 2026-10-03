use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, Instant};

use reqwest::Url;

const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const OLLAMA_LIBRARY_SEARCH_URL: &str = "https://ollama.com/search";
const OLLAMA_LIBRARY_API_URL: &str = "https://ollamadb.dev/api/v1/models";
const RECOMMENDED_OLLAMA_MODEL_ID: &str = "qwen3.5:4b";
const OLLAMA_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const OLLAMA_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const OLLAMA_SEARCH_TIMEOUT: Duration = Duration::from_secs(12);
const OLLAMA_PULL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModelList {
    pub recommended_id: String,
    pub service_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_error: Option<String>,
    pub models: Vec<OllamaModelSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModelSummary {
    pub id: String,
    pub name: String,
    pub source: OllamaModelSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_hint: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OllamaModelSource {
    Curated,
    Library,
    Installed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModelSearchRequest {
    pub query: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaPullRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagModel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    details: Option<OllamaTagDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagDetails {
    #[serde(default)]
    parameter_size: Option<String>,
    #[serde(default)]
    quantization_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaPullChunk {
    #[serde(default)]
    status: String,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaLibraryResponse {
    #[serde(default)]
    models: Vec<OllamaLibraryModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaLibraryModel {
    #[serde(default)]
    model_identifier: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    model_name: String,
    #[serde(default)]
    model_type: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    capability: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    pulls: Option<u64>,
    #[serde(default)]
    tags: Option<u64>,
    #[serde(default)]
    last_updated_str: Option<String>,
}

pub async fn format_text(
    text: &str,
    prompt: &str,
    model: &str,
    timeout: Duration,
) -> Result<String, String> {
    let model = sanitize_model_id(model)?;
    let client = ollama_client(timeout)?;
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": prompt },
            { "role": "user", "content": format!("<input>{text}</input>") }
        ],
        "format": {
            "type": "object",
            "properties": {
                "text": { "type": "string" }
            },
            "required": ["text"],
            "additionalProperties": false
        },
        "options": {
            "temperature": 0.0,
            "num_predict": 2048
        },
        "think": false,
        "stream": false,
        "keep_alive": "5m"
    });

    let resp = client
        .post(format!("{}/api/chat", ollama_base_url()))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Ollama format request failed: {e}"))?;

    let status = resp.status();
    let resp_text = resp
        .text()
        .await
        .map_err(|e| format!("Ollama format response read failed: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&resp_text).map_err(|e| format!("Ollama format parse failed: {e}"))?;

    if !status.is_success() {
        let message = json["error"]
            .as_str()
            .or_else(|| json["message"].as_str())
            .unwrap_or("unknown Ollama API error");
        return Err(format!(
            "Ollama format API error (HTTP {status}): {message}"
        ));
    }

    parse_format_response(&json)
}

fn parse_format_response(json: &serde_json::Value) -> Result<String, String> {
    if json["done_reason"].as_str() == Some("length") || json["done"].as_bool() == Some(false) {
        return Err("Ollama formatter returned incomplete text".to_string());
    }
    let content = json["message"]["content"]
        .as_str()
        .ok_or_else(|| "Ollama format response missing content".to_string())?;
    let output: serde_json::Value = serde_json::from_str(content)
        .map_err(|error| format!("Ollama formatter returned invalid JSON: {error}"))?;
    output["text"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Ollama formatter returned empty or missing text".to_string())
}

pub fn list_ollama_models() -> Result<OllamaModelList, String> {
    let installed = match run_async(fetch_installed_models()) {
        Ok(models) => models,
        Err(error) => {
            return Ok(OllamaModelList {
                recommended_id: RECOMMENDED_OLLAMA_MODEL_ID.to_string(),
                service_available: false,
                service_error: Some(error),
                models: curated_models(),
            });
        }
    };

    Ok(OllamaModelList {
        recommended_id: RECOMMENDED_OLLAMA_MODEL_ID.to_string(),
        service_available: true,
        service_error: None,
        models: merge_models(curated_models(), installed),
    })
}

pub fn search_ollama_models(
    request: OllamaModelSearchRequest,
) -> Result<Vec<OllamaModelSummary>, String> {
    let query = request.query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let installed = run_async(fetch_installed_models()).unwrap_or_default();
    let mut results = curated_models()
        .into_iter()
        .filter(|model| model_matches_query(model, query))
        .collect::<Vec<_>>();

    let remote_models = run_async(search_ollama_library_page(query)).unwrap_or_default();
    let remote_models = if remote_models.is_empty() {
        run_async(search_ollama_library_api(query)).unwrap_or_default()
    } else {
        remote_models
    };
    append_unique_models(&mut results, remote_models);

    if is_valid_model_id(query) && !results.iter().any(|model| same_model(&model.id, query)) {
        results.insert(
            0,
            OllamaModelSummary {
                id: query.to_string(),
                name: model_name(query),
                source: OllamaModelSource::Library,
                size_bytes: None,
                size_label: None,
                speed_hint: None,
                quality_hint: Some("Ollama library model".to_string()),
                installed: false,
            },
        );
    }

    Ok(merge_models(results, installed))
}

async fn search_ollama_library_page(query: &str) -> Result<Vec<OllamaModelSummary>, String> {
    let url = Url::parse_with_params(OLLAMA_LIBRARY_SEARCH_URL, &[("q", query.to_string())])
        .map_err(|e| format!("failed to build Ollama library search URL: {e}"))?;

    let client = reqwest::Client::builder()
        .connect_timeout(OLLAMA_CONNECT_TIMEOUT)
        .timeout(OLLAMA_SEARCH_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create Ollama library search client: {e}"))?;

    let html = client
        .get(url)
        .header("User-Agent", "Yap Ollama model search")
        .send()
        .await
        .map_err(|e| format!("Ollama library search failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Ollama library search failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("failed to read Ollama library search response: {e}"))?;

    Ok(parse_ollama_search_html(&html))
}

async fn search_ollama_library_api(query: &str) -> Result<Vec<OllamaModelSummary>, String> {
    let url = Url::parse_with_params(
        OLLAMA_LIBRARY_API_URL,
        &[
            ("search", query.to_string()),
            ("sort_by", "pulls".to_string()),
            ("order", "desc".to_string()),
            ("limit", "25".to_string()),
        ],
    )
    .map_err(|e| format!("failed to build Ollama library search URL: {e}"))?;

    let client = reqwest::Client::builder()
        .connect_timeout(OLLAMA_CONNECT_TIMEOUT)
        .timeout(OLLAMA_SEARCH_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create Ollama library search client: {e}"))?;

    let response = client
        .get(url)
        .header("User-Agent", "Yap Ollama model search")
        .send()
        .await
        .map_err(|e| format!("Ollama library search failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Ollama library search failed: {e}"))?
        .json::<OllamaLibraryResponse>()
        .await
        .map_err(|e| format!("failed to parse Ollama library search response: {e}"))?;

    let mut models = response
        .models
        .into_iter()
        .filter_map(library_model_summary)
        .collect::<Vec<_>>();
    models.dedup_by(|a, b| same_model(&a.id, &b.id));
    Ok(models)
}

pub fn pull_ollama_model(
    request: OllamaPullRequest,
    emit: impl Fn(serde_json::Value),
) -> Result<(), String> {
    let id = sanitize_model_id(&request.id)?;
    emit(json!({
        "id": id,
        "model": id,
        "status": "started",
    }));

    let result = run_async(pull_ollama_model_async(&id, &emit));
    if result.is_ok() {
        emit(json!({
            "id": id,
            "model": id,
            "status": "finished",
            "percent": 100,
        }));
    }
    result
}

pub fn delete_ollama_model(id: &str) -> Result<(), String> {
    let id = sanitize_model_id(id)?;
    run_async(delete_ollama_model_async(&id))
}

async fn fetch_installed_models() -> Result<Vec<OllamaModelSummary>, String> {
    let client = ollama_client(OLLAMA_REQUEST_TIMEOUT)?;
    let resp = client
        .get(format!("{}/api/tags", ollama_base_url()))
        .send()
        .await
        .map_err(|e| format!("Ollama is not reachable at {}: {e}", ollama_base_url()))?
        .error_for_status()
        .map_err(|e| format!("Ollama model list failed: {e}"))?;

    let tags = resp
        .json::<OllamaTagsResponse>()
        .await
        .map_err(|e| format!("failed to parse Ollama model list: {e}"))?;

    Ok(tags
        .models
        .into_iter()
        .filter_map(installed_model_summary)
        .collect())
}

async fn pull_ollama_model_async(
    id: &str,
    emit: &impl Fn(serde_json::Value),
) -> Result<(), String> {
    let client = ollama_client(OLLAMA_PULL_TIMEOUT)?;
    let mut resp = client
        .post(format!("{}/api/pull", ollama_base_url()))
        .json(&json!({ "model": id, "stream": true }))
        .send()
        .await
        .map_err(|e| format!("Ollama model pull failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Ollama model pull failed: {e}"))?;

    let mut buffer = String::new();
    let mut completed = false;
    let mut last_progress = Instant::now() - PROGRESS_INTERVAL;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("failed to read Ollama pull stream: {e}"))?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(index) = buffer.find('\n') {
            let line = buffer[..index].trim().to_string();
            buffer = buffer[index + 1..].to_string();
            completed |= handle_pull_line(id, &line, emit, &mut last_progress)?;
        }
    }

    completed |= handle_pull_line(id, buffer.trim(), emit, &mut last_progress)?;
    if completed {
        Ok(())
    } else {
        Err("Ollama download ended before reporting success. Try the download again.".to_string())
    }
}

async fn delete_ollama_model_async(id: &str) -> Result<(), String> {
    let client = ollama_client(OLLAMA_REQUEST_TIMEOUT)?;
    client
        .delete(format!("{}/api/delete", ollama_base_url()))
        .json(&json!({ "model": id }))
        .send()
        .await
        .map_err(|e| format!("Ollama model delete failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Ollama model delete failed: {e}"))?;
    Ok(())
}

fn handle_pull_line(
    id: &str,
    line: &str,
    emit: &impl Fn(serde_json::Value),
    last_progress: &mut Instant,
) -> Result<bool, String> {
    if line.is_empty() {
        return Ok(false);
    }
    let chunk: OllamaPullChunk = serde_json::from_str(line)
        .map_err(|e| format!("failed to parse Ollama pull progress: {e}"))?;
    if let Some(error) = chunk.error {
        return Err(error);
    }

    if last_progress.elapsed() < PROGRESS_INTERVAL && chunk.status != "success" {
        return Ok(false);
    }
    *last_progress = Instant::now();

    let percent = chunk
        .completed
        .zip(chunk.total)
        .filter(|(_, total)| *total > 0)
        .map(|(completed, total)| ((completed as f64 / total as f64) * 100.0).clamp(0.0, 100.0));

    emit(json!({
        "id": id,
        "model": id,
        "status": "progress",
        "message": chunk.status,
        "transferred": chunk.completed,
        "total": chunk.total,
        "percent": percent,
    }));
    Ok(chunk.status == "success")
}

fn installed_model_summary(model: OllamaTagModel) -> Option<OllamaModelSummary> {
    let id = if model.model.is_empty() {
        model.name
    } else {
        model.model
    };
    if id.is_empty() {
        return None;
    }

    let quality_hint = model.details.as_ref().map(|details| {
        [
            details.parameter_size.as_deref(),
            details.quantization_level.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
    });

    Some(OllamaModelSummary {
        id: id.clone(),
        name: model_name(&id),
        source: OllamaModelSource::Installed,
        size_bytes: model.size,
        size_label: model.size.map(format_size),
        speed_hint: None,
        quality_hint: quality_hint.filter(|hint| !hint.is_empty()),
        installed: true,
    })
}

fn library_model_summary(model: OllamaLibraryModel) -> Option<OllamaModelSummary> {
    let id = library_model_id(&model)?;
    if !is_valid_model_id(&id) {
        return None;
    }

    let mut meta = Vec::new();
    if let Some(pulls) = model.pulls {
        meta.push(format!("{} pulls", format_count(pulls)));
    }
    if let Some(tags) = model.tags.filter(|tags| *tags > 0) {
        meta.push(format!("{tags} tags"));
    }
    if let Some(updated) = model
        .last_updated_str
        .as_deref()
        .map(str::trim)
        .filter(|updated| !updated.is_empty())
    {
        meta.push(format!("updated {updated}"));
    }

    let mut descriptors = model
        .labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .take(4)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if let Some(capability) = model
        .capability
        .as_deref()
        .map(str::trim)
        .filter(|capability| !capability.is_empty())
    {
        descriptors.push(capability.to_string());
    }
    if !model.model_type.trim().is_empty() {
        descriptors.push(capitalize(model.model_type.trim()));
    }
    if descriptors.is_empty() {
        if let Some(description) = model
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty())
        {
            descriptors.push(truncate_description(description, 96));
        }
    }

    let name = model.model_name.trim();
    Some(OllamaModelSummary {
        id: id.clone(),
        name: if name.is_empty() {
            model_name(&id)
        } else {
            name.to_string()
        },
        source: OllamaModelSource::Library,
        size_bytes: None,
        size_label: None,
        speed_hint: join_nonempty(meta),
        quality_hint: join_nonempty(descriptors),
        installed: false,
    })
}

fn library_model_id(model: &OllamaLibraryModel) -> Option<String> {
    let model_identifier = model.model_identifier.trim();
    if model_identifier.is_empty() {
        return None;
    }

    let namespace = model
        .namespace
        .as_deref()
        .map(str::trim)
        .filter(|namespace| !namespace.is_empty());

    Some(match namespace {
        Some(namespace) if !model_identifier.contains('/') => {
            format!("{namespace}/{model_identifier}")
        }
        _ => model_identifier.to_string(),
    })
}

fn parse_ollama_search_html(html: &str) -> Vec<OllamaModelSummary> {
    let mut models = Vec::new();
    for block in html.split("<li x-test-model").skip(1) {
        let Some(id) = extract_library_href(block) else {
            continue;
        };
        if !is_valid_model_id(&id) {
            continue;
        }

        let name = extract_marker_texts(block, "x-test-search-response-title")
            .into_iter()
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| model_name(&id));
        let description = extract_first_paragraph(block);
        let mut descriptors = extract_marker_texts(block, "x-test-size")
            .into_iter()
            .chain(extract_marker_texts(block, "x-test-capability"))
            .take(6)
            .collect::<Vec<_>>();
        if descriptors.is_empty() {
            if let Some(description) = description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
            {
                descriptors.push(truncate_description(description, 96));
            }
        }

        let mut meta = Vec::new();
        if let Some(pulls) = extract_marker_texts(block, "x-test-pull-count")
            .into_iter()
            .next()
        {
            meta.push(format!("{pulls} pulls"));
        }
        if let Some(tags) = extract_marker_texts(block, "x-test-tag-count")
            .into_iter()
            .next()
            .filter(|tags| !tags.is_empty())
        {
            meta.push(format!("{tags} tags"));
        }
        if let Some(updated) = extract_marker_texts(block, "x-test-updated")
            .into_iter()
            .next()
            .filter(|updated| !updated.is_empty())
        {
            meta.push(format!("updated {updated}"));
        }

        append_unique_models(
            &mut models,
            vec![OllamaModelSummary {
                id,
                name,
                source: OllamaModelSource::Library,
                size_bytes: None,
                size_label: None,
                speed_hint: join_nonempty(meta),
                quality_hint: join_nonempty(descriptors),
                installed: false,
            }],
        );
    }
    models
}

fn extract_library_href(block: &str) -> Option<String> {
    let start = block.find("href=\"/library/")? + "href=\"/library/".len();
    let rest = &block[start..];
    let end = rest.find('"')?;
    Some(decode_html_text(&rest[..end]))
}

fn extract_marker_texts(block: &str, marker: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = block;
    while let Some(marker_index) = rest.find(marker) {
        rest = &rest[marker_index + marker.len()..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let after_open = &rest[open_end + 1..];
        let Some(close_start) = after_open.find("</") else {
            break;
        };
        let value = strip_html_tags(&after_open[..close_start]);
        if !value.is_empty() {
            values.push(value);
        }
        rest = &after_open[close_start..];
    }
    values
}

fn extract_first_paragraph(block: &str) -> Option<String> {
    let paragraph_start = block.find("<p ")?;
    let after_start = &block[paragraph_start..];
    let open_end = after_start.find('>')?;
    let after_open = &after_start[open_end + 1..];
    let close_start = after_open.find("</p>")?;
    let text = strip_html_tags(&after_open[..close_start]);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn strip_html_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    decode_html_text(output.trim())
}

fn decode_html_text(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

fn curated_models() -> Vec<OllamaModelSummary> {
    vec![
        curated_model("qwen3.5:4b", "Qwen 3.5 4B", "Fast", "Recommended"),
        curated_model("gemma3:4b", "Gemma 3 4B", "Fast", "Strong multilingual"),
        curated_model("llama3.2:3b", "Llama 3.2 3B", "Fastest", "Small"),
        curated_model("qwen3:8b", "Qwen 3 8B", "Balanced", "Higher quality"),
        curated_model(
            "mistral-small3.2:24b",
            "Mistral Small 3.2 24B",
            "Slower",
            "Best local quality",
        ),
    ]
}

fn curated_model(id: &str, name: &str, speed_hint: &str, quality_hint: &str) -> OllamaModelSummary {
    OllamaModelSummary {
        id: id.to_string(),
        name: name.to_string(),
        source: OllamaModelSource::Curated,
        size_bytes: None,
        size_label: None,
        speed_hint: Some(speed_hint.to_string()),
        quality_hint: Some(quality_hint.to_string()),
        installed: false,
    }
}

fn merge_models(
    mut models: Vec<OllamaModelSummary>,
    installed: Vec<OllamaModelSummary>,
) -> Vec<OllamaModelSummary> {
    for installed_model in installed {
        if let Some(model) = models
            .iter_mut()
            .find(|model| same_model(&model.id, &installed_model.id))
        {
            model.installed = true;
            model.size_bytes = installed_model.size_bytes;
            model.size_label = installed_model.size_label;
            if installed_model.quality_hint.is_some() {
                model.quality_hint = installed_model.quality_hint;
            }
            continue;
        }
        models.push(installed_model);
    }
    models
}

fn append_unique_models(models: &mut Vec<OllamaModelSummary>, extra: Vec<OllamaModelSummary>) {
    for model in extra {
        if models
            .iter()
            .any(|existing| same_model(&existing.id, &model.id))
        {
            continue;
        }
        models.push(model);
    }
}

fn join_nonempty(values: Vec<String>) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

fn model_matches_query(model: &OllamaModelSummary, query: &str) -> bool {
    let query = query.to_lowercase();
    [
        model.id.as_str(),
        model.name.as_str(),
        model.speed_hint.as_deref().unwrap_or_default(),
        model.quality_hint.as_deref().unwrap_or_default(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(&query))
}

fn sanitize_model_id(id: &str) -> Result<String, String> {
    let id = id.trim();
    if !is_valid_model_id(id) {
        return Err("invalid Ollama model name".to_string());
    }
    Ok(id.to_string())
}

fn is_valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && !id.chars().any(|ch| ch.is_control() || ch.is_whitespace())
        && !id.contains("..")
}

fn same_model(a: &str, b: &str) -> bool {
    normalize_model_id(a) == normalize_model_id(b)
}

fn normalize_model_id(id: &str) -> String {
    id.trim().trim_end_matches(":latest").to_lowercase()
}

fn model_name(id: &str) -> String {
    id.replace(':', " ")
        .replace(['-', '_', '/'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn truncate_description(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

fn format_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    if mb < 1024.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{:.1} GB", mb / 1024.0)
    }
}

fn ollama_base_url() -> String {
    std::env::var("OLLAMA_HOST")
        .ok()
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
        .unwrap_or_else(|| OLLAMA_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn ollama_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(OLLAMA_CONNECT_TIMEOUT)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("failed to create Ollama client: {e}"))
}

fn run_async<T>(future: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to start async runtime: {e}"))?
        .block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_rejects_truncated_or_malformed_output() {
        let response = json!({"done": true, "done_reason": "stop", "message": {"content": "{\"text\":\"Hello.\"}"}});
        assert_eq!(parse_format_response(&response).unwrap(), "Hello.");
        for response in [
            json!({"done_reason": "length", "message": {"content": "{\"text\":\"Partial\"}"}}),
            json!({"done": false, "message": {"content": "{\"text\":\"Partial\"}"}}),
            json!({"message": {"content": "{\"text\":\"unfinished"}}),
            json!({"message": {"content": "{\"text\":\"\"}"}}),
        ] {
            assert!(parse_format_response(&response).is_err());
        }
    }

    #[test]
    fn pull_requires_an_explicit_success_event() {
        let mut last_progress = Instant::now() - PROGRESS_INTERVAL;
        assert!(!handle_pull_line("test", "", &|_| {}, &mut last_progress).unwrap());
        assert!(!handle_pull_line(
            "test",
            r#"{"status":"pulling manifest"}"#,
            &|_| {},
            &mut last_progress
        )
        .unwrap());
        assert!(handle_pull_line(
            "test",
            r#"{"status":"success"}"#,
            &|_| {},
            &mut last_progress
        )
        .unwrap());
        assert!(handle_pull_line(
            "test",
            r#"{"error":"not found"}"#,
            &|_| {},
            &mut last_progress
        )
        .is_err());
    }

    #[test]
    fn invalid_model_names_are_rejected() {
        assert!(sanitize_model_id("").is_err());
        assert!(sanitize_model_id("llama 3").is_err());
        assert!(sanitize_model_id("../llama3").is_err());
        assert_eq!(sanitize_model_id("qwen3.5:4b").unwrap(), "qwen3.5:4b");
    }

    #[test]
    fn latest_tag_matches_untagged_curated_id() {
        assert!(same_model("gemma3", "gemma3:latest"));
    }

    #[test]
    fn maps_library_models_to_pullable_ids() {
        let model = OllamaLibraryModel {
            model_identifier: "model".to_string(),
            namespace: Some("someone".to_string()),
            model_name: "Model".to_string(),
            model_type: "community".to_string(),
            description: Some("Useful local model".to_string()),
            capability: Some("Tools".to_string()),
            labels: vec!["7B".to_string(), "Q4".to_string()],
            pulls: Some(12_400),
            tags: Some(3),
            last_updated_str: Some("2 days ago".to_string()),
        };

        let summary = library_model_summary(model).unwrap();
        assert_eq!(summary.id, "someone/model");
        assert_eq!(summary.source, OllamaModelSource::Library);
        assert_eq!(
            summary.speed_hint.unwrap(),
            "12.4K pulls, 3 tags, updated 2 days ago"
        );
        assert_eq!(summary.quality_hint.unwrap(), "7B, Q4, Tools, Community");
    }

    #[test]
    fn parses_ollamadb_response_shape() {
        let parsed: OllamaLibraryResponse = serde_json::from_str(
            r#"{
                "models": [
                    {
                        "model_identifier": "llama3.2",
                        "namespace": null,
                        "model_name": "llama3.2",
                        "model_type": "official",
                        "description": "Meta Llama model",
                        "capability": null,
                        "labels": ["1B", "3B"],
                        "pulls": 6300000,
                        "tags": 68,
                        "last_updated_str": "4 months ago"
                    }
                ],
                "total_count": 1,
                "limit": 1,
                "skip": 0
            }"#,
        )
        .unwrap();

        let summary = library_model_summary(parsed.models.into_iter().next().unwrap()).unwrap();
        assert_eq!(summary.id, "llama3.2");
        assert_eq!(summary.name, "llama3.2");
        assert_eq!(
            summary.speed_hint.unwrap(),
            "6.3M pulls, 68 tags, updated 4 months ago"
        );
    }

    #[test]
    fn parses_ollama_search_html() {
        let models = parse_ollama_search_html(
            r#"
            <li x-test-model class="flex items-baseline border-b border-neutral-200 py-6">
              <a href="/library/gemma4" class="group w-full">
                <span x-test-search-response-title>gemma4</span>
                <p class="max-w-lg">Gemma 4 models are designed for multimodal understanding.</p>
                <span x-test-capability>vision</span>
                <span x-test-capability>tools</span>
                <span x-test-size>12b</span>
                <span x-test-size>26b</span>
                <span x-test-pull-count>16.8M</span>
                <span x-test-tag-count>49</span>
                <span x-test-updated>4 days ago</span>
              </a>
            </li>
            "#,
        );

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gemma4");
        assert_eq!(models[0].name, "gemma4");
        assert_eq!(
            models[0].speed_hint.as_deref(),
            Some("16.8M pulls, 49 tags, updated 4 days ago")
        );
        assert_eq!(
            models[0].quality_hint.as_deref(),
            Some("12b, 26b, vision, tools")
        );
    }
}
