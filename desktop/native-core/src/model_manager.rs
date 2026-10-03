use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HUGGING_FACE_MODEL_API: &str = "https://huggingface.co/api/models";
const WHISPER_CPP_REPO: &str = "ggerganov/whisper.cpp";
const MAX_MODEL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelList {
    pub cache_dir: String,
    pub models: Vec<WhisperModelSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelSummary {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub source: WhisperModelSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_label: Option<String>,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WhisperModelSource {
    HuggingFace,
    Installed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelSearchRequest {
    pub query: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperDownloadRequest {
    pub id: String,
    pub url: String,
    pub file_name: String,
    #[serde(default)]
    pub expected_size: Option<u64>,
}

pub fn whisper_cache_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        return dirs::cache_dir()
            .unwrap_or_else(|| dirs::data_dir().unwrap_or_else(std::env::temp_dir))
            .join("yap")
            .join("whisper");
    }

    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".cache")
        .join("yap")
        .join("whisper")
}

pub fn list_whisper_models() -> Result<WhisperModelList, String> {
    let cache_dir = whisper_cache_dir();
    fs::create_dir_all(&cache_dir).map_err(|e| format!("failed to create Whisper cache: {e}"))?;

    let models = installed_models(&cache_dir)?;

    Ok(WhisperModelList {
        cache_dir: cache_dir.display().to_string(),
        models,
    })
}

pub fn search_whisper_models(
    request: WhisperModelSearchRequest,
) -> Result<Vec<WhisperModelSummary>, String> {
    let query = request.query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let results = run_async(search_hugging_face(query))?;
    let cache_dir = whisper_cache_dir();
    Ok(results
        .into_iter()
        .map(|model| with_install_state(model, &cache_dir))
        .collect())
}

pub fn download_whisper_model(
    request: WhisperDownloadRequest,
    emit: impl Fn(serde_json::Value),
) -> Result<WhisperModelSummary, String> {
    let cache_dir = whisper_cache_dir();
    fs::create_dir_all(&cache_dir).map_err(|e| format!("failed to create Whisper cache: {e}"))?;

    let file_name = safe_model_file_name(&request.file_name)?;
    let url = validate_download_url(&request.url, &file_name)?;
    let final_path = cache_dir.join(&file_name);
    let temp_path = cache_dir.join(format!(
        ".{}.{}.download",
        file_name,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));

    emit(json!({
        "id": request.id,
        "fileName": file_name,
        "status": "started",
        "total": request.expected_size,
    }));

    let result = run_async(download_to_temp(
        &request.id,
        url,
        &file_name,
        request.expected_size,
        &temp_path,
        &emit,
    ));

    if let Err(error) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    replace_model_file(&temp_path, &final_path)?;

    let size_bytes = fs::metadata(&final_path)
        .ok()
        .map(|metadata| metadata.len());
    emit(json!({
        "id": request.id,
        "fileName": file_name,
        "status": "finished",
        "transferred": size_bytes,
        "total": size_bytes.or(request.expected_size),
        "percent": 100,
    }));

    Ok(WhisperModelSummary {
        id: request.id,
        name: model_name_from_file(&file_name),
        file_name,
        source: WhisperModelSource::Installed,
        url: Some(request.url),
        size_bytes,
        size_label: size_bytes.map(format_size),
        installed: true,
        path: Some(final_path.display().to_string()),
    })
}

pub fn delete_whisper_model(file_name: &str) -> Result<(), String> {
    let cache_dir = whisper_cache_dir();
    let file_name = safe_model_file_name(file_name)?;
    let path = cache_dir.join(file_name);

    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("failed to delete Whisper model: {e}"))?;
    }
    Ok(())
}

pub fn reveal_whisper_models() -> Result<(), String> {
    let cache_dir = whisper_cache_dir();
    fs::create_dir_all(&cache_dir).map_err(|e| format!("failed to create Whisper cache: {e}"))?;

    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(&cache_dir).status()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&cache_dir).status()
    } else {
        Command::new("xdg-open").arg(&cache_dir).status()
    }
    .map_err(|e| format!("failed to reveal Whisper model folder: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("failed to reveal Whisper model folder".to_string())
    }
}

async fn search_hugging_face(query: &str) -> Result<Vec<WhisperModelSummary>, String> {
    let url = Url::parse_with_params(
        HUGGING_FACE_MODEL_API,
        &[
            ("search", format!("whisper.cpp {query}")),
            ("full", "true".to_string()),
            ("blobs", "true".to_string()),
            ("sort", "likes".to_string()),
            ("direction", "-1".to_string()),
            ("limit", "50".to_string()),
        ],
    )
    .map_err(|e| format!("failed to build Hugging Face search URL: {e}"))?;

    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .timeout(SEARCH_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create Hugging Face search client: {e}"))?;

    // Repository search does not search filenames. Include the live whisper.cpp
    // file listing so queries such as "large-v3-turbo" find the standard models.
    let catalog_url = format!("{HUGGING_FACE_MODEL_API}/{WHISPER_CPP_REPO}?blobs=true");
    let (models, catalog) = tokio::join!(
        fetch_hugging_face_models(&client, url.as_str()),
        fetch_hugging_face_models(&client, &catalog_url),
    );
    let models = models?;
    let catalog = catalog?;

    let models = models
        .as_array()
        .ok_or_else(|| "Hugging Face search returned an unexpected response".to_string())?;
    let mut candidates = whisper_models_from_repo(&catalog, query);
    for model in models {
        candidates.extend(whisper_models_from_repo(model, query));
    }

    candidates.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    candidates.dedup_by(|a, b| a.file_name == b.file_name);
    Ok(candidates.into_iter().take(25).collect())
}

async fn fetch_hugging_face_models(
    client: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, String> {
    client
        .get(url)
        .header("User-Agent", "Yap model manager")
        .send()
        .await
        .map_err(|e| format!("Hugging Face search failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Hugging Face search failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("failed to parse Hugging Face search response: {e}"))
}

fn whisper_models_from_repo(model: &serde_json::Value, query: &str) -> Vec<WhisperModelSummary> {
    let mut candidates = Vec::new();
    let Some(repo_id) = model
        .get("id")
        .or_else(|| model.get("modelId"))
        .and_then(|value| value.as_str())
    else {
        return candidates;
    };
    let Some(siblings) = model.get("siblings").and_then(|value| value.as_array()) else {
        return candidates;
    };

    for sibling in siblings {
        let Some(remote_file_name) = sibling.get("rfilename").and_then(|value| value.as_str())
        else {
            continue;
        };
        if !is_compatible_whisper_ggml_bin(remote_file_name) {
            continue;
        }
        let id = model_id_from_remote_file(remote_file_name);
        let file_name = format!("{id}.bin");
        let name = model_name_from_file(remote_file_name);
        if ![repo_id, remote_file_name, &name]
            .iter()
            .any(|value| value.to_lowercase().contains(&query.to_lowercase()))
        {
            continue;
        }
        let size_bytes = sibling.get("size").and_then(|value| value.as_u64());
        candidates.push(WhisperModelSummary {
            id,
            name,
            file_name,
            source: WhisperModelSource::HuggingFace,
            url: Some(resolve_url(repo_id, remote_file_name)),
            size_bytes,
            size_label: size_bytes.map(format_size),
            installed: false,
            path: None,
        });
    }
    candidates
}

async fn download_to_temp(
    id: &str,
    url: Url,
    file_name: &str,
    expected_size: Option<u64>,
    temp_path: &Path,
    emit: &impl Fn(serde_json::Value),
) -> Result<(), String> {
    if expected_size.is_some_and(|size| size > MAX_MODEL_BYTES) {
        return Err(format!(
            "model is too large for Yap's managed cache limit of {}",
            format_size(MAX_MODEL_BYTES)
        ));
    }

    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to create download client: {e}"))?;

    let mut response = client
        .get(url)
        .header("User-Agent", "Yap model manager")
        .send()
        .await
        .map_err(|e| format!("model download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("model download failed: {e}"))?;

    let total = response
        .content_length()
        .or(expected_size)
        .ok_or_else(|| "model download did not report a file size".to_string())?;
    if total > MAX_MODEL_BYTES {
        return Err(format!(
            "model is too large for Yap's managed cache limit of {}",
            format_size(MAX_MODEL_BYTES)
        ));
    }

    let mut file =
        File::create(temp_path).map_err(|e| format!("failed to create temp model file: {e}"))?;
    let mut transferred = 0_u64;
    let mut last_progress = Instant::now() - PROGRESS_INTERVAL;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("failed to read model download: {e}"))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("failed to write model download: {e}"))?;
        transferred += chunk.len() as u64;
        if transferred > MAX_MODEL_BYTES {
            return Err(format!(
                "model exceeded Yap's managed cache limit of {}",
                format_size(MAX_MODEL_BYTES)
            ));
        }
        if last_progress.elapsed() >= PROGRESS_INTERVAL {
            emit_download_progress(emit, id, file_name, transferred, total);
            last_progress = Instant::now();
        }
    }

    file.sync_all()
        .map_err(|e| format!("failed to flush model download: {e}"))?;

    if transferred != total {
        return Err(format!(
            "model download was incomplete: expected {} but received {}",
            format_size(total),
            format_size(transferred)
        ));
    }
    emit_download_progress(emit, id, file_name, transferred, total);

    Ok(())
}

fn installed_models(cache_dir: &Path) -> Result<Vec<WhisperModelSummary>, String> {
    if !cache_dir.exists() {
        return Ok(Vec::new());
    }

    let mut models = Vec::new();
    for entry in
        fs::read_dir(cache_dir).map_err(|e| format!("failed to inspect Whisper cache: {e}"))?
    {
        let entry = entry.map_err(|e| format!("failed to inspect Whisper model: {e}"))?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        let id = file_name.trim_end_matches(".bin").to_string();
        let size_bytes = entry.metadata().ok().map(|metadata| metadata.len());
        models.push(WhisperModelSummary {
            id,
            name: model_name_from_file(&file_name),
            file_name,
            source: WhisperModelSource::Installed,
            url: None,
            size_bytes,
            size_label: size_bytes.map(format_size),
            installed: true,
            path: Some(path.display().to_string()),
        });
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

fn with_install_state(mut model: WhisperModelSummary, cache_dir: &Path) -> WhisperModelSummary {
    let path = cache_dir.join(&model.file_name);
    if path.is_file() {
        model.installed = true;
        model.path = Some(path.display().to_string());
        if let Ok(metadata) = path.metadata() {
            model.size_bytes = Some(metadata.len());
            model.size_label = Some(format_size(metadata.len()));
        }
    }
    model
}

fn safe_model_file_name(file_name: &str) -> Result<String, String> {
    let file_name = file_name.trim();
    if file_name.is_empty()
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name == "."
        || file_name == ".."
        || !file_name.ends_with(".bin")
    {
        return Err("invalid Whisper model file name".to_string());
    }
    Ok(file_name.to_string())
}

fn validate_download_url(raw_url: &str, file_name: &str) -> Result<Url, String> {
    let url = Url::parse(raw_url).map_err(|e| format!("invalid model download URL: {e}"))?;
    if url.scheme() != "https" || url.host_str() != Some("huggingface.co") {
        return Err("model downloads must use a Hugging Face HTTPS URL".to_string());
    }

    let segments = url
        .path_segments()
        .ok_or_else(|| "model download URL has no path".to_string())?
        .collect::<Vec<_>>();
    if segments.iter().any(|segment| {
        segment.is_empty() || *segment == "." || *segment == ".." || segment.contains('\\')
    }) {
        return Err("model download URL contains an invalid path".to_string());
    }

    let Some(resolve_index) = segments.iter().position(|segment| *segment == "resolve") else {
        return Err("model download URL must be a Hugging Face resolve URL".to_string());
    };
    if resolve_index < 2 || segments.get(resolve_index + 1) != Some(&"main") {
        return Err("model download URL must resolve from the main branch".to_string());
    }

    let remote_file_name = segments
        .get(resolve_index + 2..)
        .filter(|segments| !segments.is_empty())
        .map(|segments| segments.join("/"))
        .ok_or_else(|| "model download URL is missing a model file".to_string())?;
    if !is_compatible_whisper_ggml_bin(&remote_file_name) {
        return Err(
            "model download URL does not point to a compatible Whisper GGML .bin file".to_string(),
        );
    }

    let expected_file_name = format!("{}.bin", model_id_from_remote_file(&remote_file_name));
    if expected_file_name != file_name {
        return Err("model download URL does not match the requested model file".to_string());
    }

    Ok(url)
}

fn replace_model_file(temp_path: &Path, final_path: &Path) -> Result<(), String> {
    if !final_path.exists() {
        return fs::rename(temp_path, final_path)
            .map_err(|e| format!("failed to move model into cache: {e}"));
    }

    let backup_path = final_path.with_extension("bin.previous");
    let _ = fs::remove_file(&backup_path);
    fs::rename(final_path, &backup_path)
        .map_err(|e| format!("failed to prepare existing model replacement: {e}"))?;

    if let Err(error) = fs::rename(temp_path, final_path) {
        let _ = fs::rename(&backup_path, final_path);
        return Err(format!("failed to move model into cache: {error}"));
    }

    let _ = fs::remove_file(&backup_path);
    Ok(())
}

fn emit_download_progress(
    emit: &impl Fn(serde_json::Value),
    id: &str,
    file_name: &str,
    transferred: u64,
    total: u64,
) {
    emit(json!({
        "id": id,
        "fileName": file_name,
        "status": "progress",
        "transferred": transferred,
        "total": total,
        "percent": ((transferred as f64 / total as f64) * 100.0).clamp(0.0, 100.0),
    }));
}

fn is_compatible_whisper_ggml_bin(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    let base_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&lower);
    lower.ends_with(".bin")
        && !lower.contains("encoder")
        && [
            "ggml-tiny",
            "ggml-base",
            "ggml-small",
            "ggml-medium",
            "ggml-large",
            "ggml-distil",
        ]
        .iter()
        .any(|prefix| base_name.starts_with(prefix))
}

fn model_id_from_remote_file(file_name: &str) -> String {
    let name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name)
        .trim_end_matches(".bin");
    name.strip_prefix("ggml-").unwrap_or(name).to_string()
}

fn model_name_from_file(file_name: &str) -> String {
    model_id_from_remote_file(file_name)
        .replace('.', " ")
        .replace('-', " ")
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

fn resolve_url(repo: &str, file_name: &str) -> String {
    format!(
        "https://huggingface.co/{repo}/resolve/main/{}",
        file_name.replace(' ', "%20")
    )
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    if mb < 1024.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{:.1} GB", mb / 1024.0)
    }
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
    fn whisper_cache_uses_yap_whisper_folder() {
        let cache_dir = whisper_cache_dir();
        assert!(cache_dir.ends_with(Path::new("yap").join("whisper")));
    }

    #[test]
    fn local_list_contains_only_installed_model_files() {
        let cache = std::env::temp_dir().join(format!("yap-whisper-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&cache).unwrap();
        assert!(installed_models(&cache).unwrap().is_empty());
        fs::write(cache.join("example.bin"), b"model").unwrap();
        fs::write(cache.join(".unfinished.download"), b"partial").unwrap();
        let models = installed_models(&cache).unwrap();
        fs::remove_dir_all(&cache).unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "example");
        assert!(models[0].installed);
        assert_eq!(models[0].size_bytes, Some(5));
    }

    #[test]
    fn search_matches_actual_files_without_inventing_results() {
        let repo = json!({
            "id": "ggerganov/whisper.cpp",
            "siblings": [
                {"rfilename": "ggml-large-v3-turbo-q5_0.bin"},
                {"rfilename": "ggml-base.en.bin"},
                {"rfilename": "README.md"}
            ]
        });
        let models = whisper_models_from_repo(&repo, "large-v3-turbo");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "large-v3-turbo-q5_0");
        assert_eq!(models[0].url.as_deref(), Some("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin"));
        assert!(whisper_models_from_repo(&repo, "not-a-real-model").is_empty());
    }

    #[test]
    fn safe_delete_input_rejects_paths_outside_cache() {
        assert!(safe_model_file_name("../base.en.bin").is_err());
        assert!(safe_model_file_name("/tmp/base.en.bin").is_err());
        assert!(safe_model_file_name("base.en.txt").is_err());
        assert_eq!(safe_model_file_name("base.en.bin").unwrap(), "base.en.bin");
    }

    #[test]
    fn download_url_must_match_safe_hugging_face_model_file() {
        assert!(validate_download_url(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
            "base.en.bin"
        )
        .is_ok());
        assert!(
            validate_download_url("https://example.com/ggml-base.en.bin", "base.en.bin").is_err()
        );
        assert!(validate_download_url(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
            "base.en.bin"
        )
        .is_err());
        assert!(validate_download_url(
            "https://huggingface.co/ggerganov/whisper.cpp/blob/main/ggml-base.en.bin",
            "base.en.bin"
        )
        .is_err());
    }

    #[test]
    fn download_url_allows_nested_hugging_face_model_paths() {
        assert!(validate_download_url(
            "https://huggingface.co/mychen76/whisper_cpp_models/resolve/main/ggml/ggml-base.en-q5_0.bin",
            "base.en-q5_0.bin"
        )
        .is_ok());
    }

    #[test]
    fn remote_file_names_normalize_to_local_ids() {
        assert_eq!(model_id_from_remote_file("ggml-base.en.bin"), "base.en");
        assert_eq!(
            model_id_from_remote_file("nested/ggml-large-v3-turbo-q5_0.bin"),
            "large-v3-turbo-q5_0"
        );
    }
}
