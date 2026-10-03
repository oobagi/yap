use std::path::{Path, PathBuf};

#[cfg(feature = "local-whisper-native")]
use hound::{SampleFormat, WavReader};
#[cfg(feature = "local-whisper-native")]
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[cfg(feature = "local-whisper-native")]
const WHISPER_SAMPLE_RATE: u32 = 16_000;

pub fn transcribe(
    audio_path: &Path,
    model: &str,
    language: Option<&str>,
) -> Result<String, String> {
    transcribe_impl(audio_path, model, language)
}

#[cfg(feature = "local-whisper-native")]
fn transcribe_impl(
    audio_path: &Path,
    model: &str,
    language: Option<&str>,
) -> Result<String, String> {
    let model_path = resolve_model_path(model)?;
    let audio = load_whisper_audio(audio_path)?;

    whisper_rs::install_logging_hooks();

    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .map_err(|e| {
            format!(
                "failed to load local Whisper model at {}: {e}",
                model_path.display()
            )
        })?;
    let mut state = ctx
        .create_state()
        .map_err(|e| format!("failed to create local Whisper state: {e}"))?;

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    });
    params.set_n_threads(whisper_thread_count());
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    let language = language.and_then(normalize_language);
    if let Some(language) = language.as_deref() {
        params.set_language(Some(language));
    }

    state
        .full(params, &audio)
        .map_err(|e| format!("local Whisper inference failed: {e}"))?;

    let text = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("");

    Ok(text.trim().to_string())
}

#[cfg(not(feature = "local-whisper-native"))]
fn transcribe_impl(
    _audio_path: &Path,
    model: &str,
    _language: Option<&str>,
) -> Result<String, String> {
    let _ = resolve_model_path(model)?;
    Err(
        "Local Whisper is not enabled in this build. Rebuild yap-core with the `local-whisper-native` Cargo feature and ensure cmake is installed.".to_string(),
    )
}

#[cfg(feature = "local-whisper-native")]
fn whisper_thread_count() -> i32 {
    std::thread::available_parallelism()
        .map(|count| count.get().clamp(1, 4) as i32)
        .unwrap_or(2)
}

#[cfg(feature = "local-whisper-native")]
fn normalize_language(language: &str) -> Option<String> {
    let language = language.trim();
    if language.is_empty() || language.eq_ignore_ascii_case("auto") {
        return None;
    }
    let primary = language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .trim()
        .to_lowercase();
    (!primary.is_empty()).then_some(primary)
}

fn resolve_model_path(model: &str) -> Result<PathBuf, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Choose a Local Whisper model in Transcription Settings.".to_string());
    }

    let candidates = model_candidates(model);
    if let Some(path) = candidates.iter().find(|path| path.is_file()) {
        return Ok(path.clone());
    }

    let searched = candidates
        .iter()
        .map(|path| format!("  - {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "Local Whisper model not found. Set the Model path to an existing whisper.cpp GGML .bin file. Searched:\n{searched}"
    ))
}

fn model_candidates(model: &str) -> Vec<PathBuf> {
    let expanded = expand_home(model);
    let mut candidates = vec![expanded.clone()];

    if looks_like_identifier(model) {
        let file_name = if model.ends_with(".bin") {
            model.to_string()
        } else {
            format!("{model}.bin")
        };

        if let Ok(config_dir) = crate::config::config_dir() {
            candidates.push(config_dir.join("whisper").join(&file_name));
            candidates.push(config_dir.join("models").join("whisper").join(&file_name));
        }

        candidates.push(crate::model_manager::whisper_cache_dir().join(&file_name));
        for cache_dir in legacy_whisper_cache_dirs() {
            candidates.push(cache_dir.join(&file_name));
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn legacy_whisper_cache_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(cache_dir) = dirs::cache_dir() {
        dirs.push(cache_dir.join("yap").join("whisper"));
    }
    if let Some(home_dir) = dirs::home_dir() {
        dirs.push(home_dir.join(".cache").join("yap").join("whisper"));
    }
    dirs
}

fn looks_like_identifier(model: &str) -> bool {
    !model.contains('/')
        && !model.contains('\\')
        && !model.starts_with('.')
        && !model.starts_with('~')
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

#[cfg(feature = "local-whisper-native")]
fn load_whisper_audio(audio_path: &Path) -> Result<Vec<f32>, String> {
    let mut reader =
        WavReader::open(audio_path).map_err(|e| format!("failed to open WAV file: {e}"))?;
    let spec = reader.spec();

    if spec.channels == 0 {
        return Err("WAV file has zero channels".to_string());
    }

    let samples = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| value as f32 / 32768.0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("failed to read WAV samples: {e}"))?,
        (SampleFormat::Int, bits) if bits <= 32 => {
            let scale = 2_f32.powi(bits as i32 - 1);
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("failed to read WAV samples: {e}"))?
        }
        (SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("failed to read WAV samples: {e}"))?,
        _ => {
            return Err(format!(
                "unsupported WAV format: {:?} {}-bit",
                spec.sample_format, spec.bits_per_sample
            ));
        }
    };

    let mono = downmix_to_mono(&samples, spec.channels as usize)?;
    Ok(resample_linear(
        &mono,
        spec.sample_rate,
        WHISPER_SAMPLE_RATE,
    ))
}

#[cfg(feature = "local-whisper-native")]
fn downmix_to_mono(samples: &[f32], channels: usize) -> Result<Vec<f32>, String> {
    if channels == 0 {
        return Err("channel count must be greater than zero".to_string());
    }
    if channels == 1 {
        return Ok(samples.to_vec());
    }

    Ok(samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect())
}

#[cfg(feature = "local-whisper-native")]
fn resample_linear(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if samples.is_empty() || source_rate == 0 || source_rate == target_rate {
        return samples.to_vec();
    }

    let output_len =
        ((samples.len() as u64 * target_rate as u64) / source_rate as u64).max(1) as usize;
    let step = source_rate as f64 / target_rate as f64;

    (0..output_len)
        .map(|index| {
            let source_pos = index as f64 * step;
            let left = source_pos.floor() as usize;
            let right = (left + 1).min(samples.len() - 1);
            let fraction = (source_pos - left as f64) as f32;
            samples[left] + (samples[right] - samples[left]) * fraction
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transcription_requires_an_explicit_model() {
        use crate::transcription::{transcribe, TranscriptionOptions, TranscriptionProvider};

        assert_eq!(TranscriptionProvider::LocalWhisper.default_model(), "");
        for model in ["", "   "] {
            let options = TranscriptionOptions {
                model: model.into(),
                ..Default::default()
            };
            let error = transcribe(
                TranscriptionProvider::LocalWhisper,
                Path::new("unused.wav"),
                &options,
            )
            .await
            .unwrap_err();
            assert!(error.contains("Choose a Local Whisper model"));
        }
    }

    #[cfg(feature = "local-whisper-native")]
    #[test]
    fn normalizes_bcp47_language_to_primary_code() {
        assert_eq!(normalize_language("en-US").as_deref(), Some("en"));
        assert_eq!(normalize_language("JA_jp").as_deref(), Some("ja"));
        assert_eq!(normalize_language("").as_deref(), None);
        assert_eq!(normalize_language("auto").as_deref(), None);
    }

    #[test]
    fn expands_short_model_identifier_to_standard_model_dirs() {
        let candidates = model_candidates("base.en");
        assert!(candidates.iter().any(|path| path.ends_with("base.en")));
        assert!(candidates
            .iter()
            .any(|path| path.ends_with("whisper/base.en.bin")));
    }

    #[test]
    fn includes_platform_cache_fallback_for_existing_models() {
        let candidates = model_candidates("base.en");
        if let Some(cache_dir) = dirs::cache_dir() {
            let legacy_path = cache_dir.join("yap").join("whisper").join("base.en.bin");
            assert!(candidates.iter().any(|path| path == &legacy_path));
        }
    }

    #[cfg(feature = "local-whisper-native")]
    #[test]
    fn downsamples_forty_eight_khz_to_sixteen_khz() {
        let samples = vec![0.0; 48_000];
        let resampled = resample_linear(&samples, 48_000, 16_000);
        assert_eq!(resampled.len(), 16_000);
    }

    #[cfg(feature = "local-whisper-native")]
    #[test]
    fn downmixes_stereo_frames_to_mono() {
        let samples = vec![1.0, -1.0, 0.5, 0.25];
        assert_eq!(downmix_to_mono(&samples, 2).unwrap(), vec![0.0, 0.375]);
    }

    #[cfg(feature = "local-whisper-native")]
    #[test]
    fn transcribes_fixture_when_model_and_audio_are_provided() {
        let (Ok(model), Ok(audio)) = (
            std::env::var("YAP_TEST_LOCAL_WHISPER_MODEL"),
            std::env::var("YAP_TEST_LOCAL_WHISPER_WAV"),
        ) else {
            return;
        };

        let text = transcribe(Path::new(&audio), &model, Some("en"))
            .expect("fixture transcription should complete");
        assert!(
            text.to_lowercase().contains("hello"),
            "expected transcription to contain fixture word, got {text:?}"
        );
    }
}
