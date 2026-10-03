use std::io::{self, BufRead, Write};
use std::sync::{mpsc, Arc};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use yap_core_lib::commands::{self, CommandHost};
use yap_core_lib::config::AppConfig;
use yap_core_lib::dictation::{self, DictationHost};
use yap_core_lib::model_manager::WhisperDownloadRequest;
use yap_core_lib::ollama::OllamaPullRequest;

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum RpcOutput {
    Response {
        id: Option<Value>,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Event {
        event: &'static str,
        payload: Value,
    },
}

#[derive(Clone)]
struct SidecarHost {
    output: mpsc::Sender<RpcOutput>,
}

impl CommandHost for SidecarHost {
    fn on_settings_changed(&self) {
        if dictation::is_running() {
            let host: Arc<dyn DictationHost> = Arc::new(self.clone());
            if let Err(error) = dictation::start(host) {
                let _ = self.output.send(RpcOutput::Event {
                    event: "dictation:error",
                    payload: json!({
                        "title": "Settings update failed",
                        "message": error,
                    }),
                });
            }
        }
    }

    fn emit(&self, event: &'static str, payload: Value) {
        let _ = self.output.send(RpcOutput::Event { event, payload });
    }
}

impl DictationHost for SidecarHost {
    fn emit(&self, event: &'static str, payload: Value) {
        let _ = self.output.send(RpcOutput::Event { event, payload });
    }
}

fn main() {
    let (output_tx, output_rx) = mpsc::channel::<RpcOutput>();
    let writer = thread::spawn(move || {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        while let Ok(output) = output_rx.recv() {
            match serde_json::to_writer(&mut stdout, &output) {
                Ok(()) => {
                    let _ = stdout.write_all(b"\n");
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });

    let host = Arc::new(SidecarHost {
        output: output_tx.clone(),
    });

    for line in io::stdin().lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                let _ = output_tx.send(error_response(
                    None,
                    format!("failed to read request line: {err}"),
                ));
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(err) => {
                let _ =
                    output_tx.send(error_response(None, format!("invalid request JSON: {err}")));
                continue;
            }
        };

        let id = request.id.clone();
        let response = match handle_request(request, Arc::clone(&host)) {
            Ok(result) => success_response(id, result),
            Err(err) => error_response(id, err),
        };
        let _ = output_tx.send(response);
    }

    let _ = dictation::stop();
    drop(host);
    drop(output_tx);
    let _ = writer.join();
}

fn handle_request(request: RpcRequest, host: Arc<SidecarHost>) -> Result<Option<Value>, String> {
    match request.method.as_str() {
        "config.save" => {
            let config = parse_config_params(request.params)?;
            commands::store_config(config, host.as_ref())?;
            Ok(Some(json!({ "saved": true })))
        }
        "history.get" => to_value(commands::load_history()).map(Some),
        "history.remove" => {
            let id = required_string_param(request.params, "id")?;
            commands::delete_history_entry(&id)?;
            Ok(Some(json!({ "removed": true })))
        }
        "history.clear" => {
            commands::delete_all_history()?;
            Ok(Some(json!({ "cleared": true })))
        }
        "cache.clear" => to_value(commands::clear_runtime_cache()?).map(Some),
        "models.whisper.list" => to_value(commands::list_whisper_models()?).map(Some),
        "models.whisper.search" => {
            let request = parse_params(request.params)?;
            to_value(commands::search_whisper_models(request)?).map(Some)
        }
        "models.whisper.download" => {
            let download: WhisperDownloadRequest = parse_params(request.params)?;
            let download_id = download.id.clone();
            let download_file_name = download.file_name.clone();
            let worker_download_id = download_id.clone();
            let worker_download_file_name = download_file_name.clone();
            let host = Arc::clone(&host);
            std::thread::spawn(move || {
                if let Err(error) = commands::download_whisper_model(download, host.as_ref()) {
                    CommandHost::emit(
                        host.as_ref(),
                        "models:download",
                        json!({
                            "id": worker_download_id,
                            "fileName": worker_download_file_name,
                            "status": "error",
                            "error": error,
                        }),
                    );
                }
            });
            Ok(Some(json!({
                "started": true,
                "id": download_id,
                "fileName": download_file_name,
            })))
        }
        "models.whisper.delete" => {
            let file_name = required_string_param(request.params, "fileName")?;
            commands::delete_whisper_model(&file_name)?;
            Ok(Some(json!({ "deleted": true })))
        }
        "models.whisper.reveal" => {
            commands::reveal_whisper_models()?;
            Ok(Some(json!({ "revealed": true })))
        }
        "models.ollama.list" => to_value(commands::list_ollama_models()?).map(Some),
        "models.ollama.search" => {
            let request = parse_params(request.params)?;
            to_value(commands::search_ollama_models(request)?).map(Some)
        }
        "models.ollama.pull" => {
            let pull: OllamaPullRequest = parse_params(request.params)?;
            let pull_id = pull.id.clone();
            let worker_pull_id = pull_id.clone();
            let host = Arc::clone(&host);
            std::thread::spawn(move || {
                if let Err(error) = commands::pull_ollama_model(pull, host.as_ref()) {
                    CommandHost::emit(
                        host.as_ref(),
                        "models:ollama-download",
                        json!({
                            "id": worker_pull_id,
                            "model": worker_pull_id,
                            "status": "error",
                            "error": error,
                        }),
                    );
                }
            });
            Ok(Some(json!({
                "started": true,
                "id": pull_id,
            })))
        }
        "models.ollama.delete" => {
            let id = required_string_param(request.params, "id")?;
            commands::delete_ollama_model(&id)?;
            Ok(Some(json!({ "deleted": true })))
        }
        "audio.list_devices" => to_value(commands::audio_device_names()).map(Some),
        "runtime.start" => {
            let host: Arc<dyn DictationHost> = host;
            dictation::start(host)?;
            Ok(Some(json!({ "started": true })))
        }
        "runtime.stop" => {
            dictation::stop()?;
            Ok(Some(json!({ "stopped": true })))
        }
        other => Err(format!("unknown method: {other}")),
    }
}

fn parse_params<T: for<'de> Deserialize<'de>>(params: Option<Value>) -> Result<T, String> {
    serde_json::from_value(params.unwrap_or_else(|| json!({})))
        .map_err(|err| format!("invalid params: {err}"))
}

fn parse_config_params(params: Option<Value>) -> Result<AppConfig, String> {
    let params = params.ok_or_else(|| "missing params".to_string())?;
    let config_value = params
        .get("config")
        .cloned()
        .ok_or_else(|| "missing config param".to_string())?;
    serde_json::from_value::<AppConfig>(config_value)
        .map_err(|err| format!("invalid config params: {err}"))
}

fn required_string_param(params: Option<Value>, key: &str) -> Result<String, String> {
    let params = params.ok_or_else(|| "missing params".to_string())?;
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing string param: {key}"))
}

fn to_value<T: Serialize>(result: T) -> Result<Value, String> {
    serde_json::to_value(result).map_err(|err| format!("failed to serialize response: {err}"))
}

fn success_response(id: Option<Value>, result: Option<Value>) -> RpcOutput {
    RpcOutput::Response {
        id,
        ok: true,
        result,
        error: None,
    }
}

fn error_response(id: Option<Value>, error: String) -> RpcOutput {
    RpcOutput::Response {
        id,
        ok: false,
        result: None,
        error: Some(error),
    }
}
