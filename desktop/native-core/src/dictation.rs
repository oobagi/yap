use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::audio;
use crate::audio_ducking;
use crate::config::{self, AppConfig};
use crate::formatting::{self, FormattingOptions, FormattingProvider};
use crate::history::{self, HistoryAppend};
use crate::hotkey::{self, HotkeySpec};
use crate::paste;
use crate::transcription::{self, TranscriptionOptions, TranscriptionProvider};
use crate::vad;

const SOUND_START_PRESS: &str = "Blow";
const SOUND_HANDS_FREE: &str = "HandsFree";
const SOUND_NEXT: &str = "Pop";
const SOUND_SKIP: &str = "Skip";
const SHORT_TAP_TIP_GRACE: Duration =
    Duration::from_millis((hotkey::DOUBLE_TAP_WINDOW * 1000.0) as u64 + 100);
const HOLD_TO_RECORD_DELAY: Duration = Duration::from_millis(250);
const PRE_RECORDING_CUE_DELAY: Duration = Duration::from_millis(220);
const SILENCE_PEAK_THRESHOLD: f32 = 0.15;
const ONBOARDING_HOLD_CONFIRM_DELAY: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeState {
    Idle,
    PressPending,
    Recording,
    TapPending,
    Paused,
    Processing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnboardingStep {
    TryIt,
    DoubleTapTip,
    ClickTip,
    ApiTip,
    FormattingTip,
    Welcome,
}

impl OnboardingStep {
    fn to_str(self) -> &'static str {
        match self {
            Self::TryIt => "tryIt",
            Self::DoubleTapTip => "doubleTapTip",
            Self::ClickTip => "clickTip",
            Self::ApiTip => "apiTip",
            Self::FormattingTip => "formattingTip",
            Self::Welcome => "welcome",
        }
    }
}

pub trait DictationHost: Send + Sync + 'static {
    fn emit(&self, event: &'static str, payload: Value);
}

static RUNTIME: once_cell::sync::Lazy<Mutex<RuntimeState>> =
    once_cell::sync::Lazy::new(|| Mutex::new(RuntimeState::Idle));
static RECORDING_STARTED_AT: once_cell::sync::Lazy<Mutex<Option<Instant>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));
static RECORDING_PAUSED_AT: once_cell::sync::Lazy<Mutex<Option<Instant>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));
static RECORDING_PAUSED_DURATION: once_cell::sync::Lazy<Mutex<Duration>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Duration::ZERO));
static PEAK_LEVEL: once_cell::sync::Lazy<Mutex<f32>> =
    once_cell::sync::Lazy::new(|| Mutex::new(0.0));
static ONBOARDING_STEP: once_cell::sync::Lazy<Mutex<Option<OnboardingStep>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));
static ONBOARDING_HOLD_STARTED_AT: once_cell::sync::Lazy<Mutex<Option<Instant>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));
static HANDS_FREE_RECORDING: once_cell::sync::Lazy<Mutex<bool>> =
    once_cell::sync::Lazy::new(|| Mutex::new(false));
static IGNORE_PENDING_KEY_UP: AtomicBool = AtomicBool::new(false);
static RECORDING_GENERATION: AtomicU64 = AtomicU64::new(0);
static HOST: once_cell::sync::Lazy<Mutex<Option<Arc<dyn DictationHost>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));
static LEVEL_POLLER_STARTED: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn clear_runtime_cache() -> Result<crate::cache::CacheCleanupSummary, String> {
    // Keep the state lock until cleanup finishes so a new recording cannot start
    // while its temporary audio file is being removed.
    let current = RUNTIME
        .lock()
        .map_err(|_| "Dictation state is unavailable".to_string())?;
    if *current != RuntimeState::Idle {
        return Err("Finish the current dictation before cleaning the cache.".to_string());
    }
    crate::cache::clear_runtime_cache()
}

pub fn start(host: Arc<dyn DictationHost>) -> Result<(), String> {
    let _ = config::load();
    if let Ok(mut active_host) = HOST.lock() {
        *active_host = Some(Arc::clone(&host));
    }
    reset_active_runtime();

    hotkey::set_callbacks(on_key_down, on_key_up, on_double_tap);
    hotkey::set_permission_required_callback(on_accessibility_permission_required);

    let cfg = config::get();
    let spec = HotkeySpec::parse(&cfg.hotkey);
    RUNNING.store(true, Ordering::SeqCst);
    hotkey::start(spec);
    start_level_poller();
    spawn_overlay(&cfg);
    set_state(RuntimeState::Idle);
    clear_permission_prompt_if_available();
    start_onboarding_if_needed(&cfg);
    emit(
        "dictation:runtime-started",
        json!({
            "hotkey": cfg.hotkey,
        }),
    );
    Ok(())
}

fn on_accessibility_permission_required(label: String) {
    emit_accessibility_permission_required(&label);
}

fn emit_accessibility_permission_required(label: &str) {
    emit_overlay_permission(
        "Accessibility Required",
        &format!("Allow Accessibility access to use the {label} hotkey anywhere."),
        "Open Settings",
        true,
    );
    emit(
        "dictation:permission-required",
        json!({
            "permission": "accessibility",
            "hotkeyLabel": label,
        }),
    );
}

#[cfg(target_os = "macos")]
fn clear_permission_prompt_if_available() {
    if hotkey::has_accessibility_permission() {
        emit_overlay_permission("", "", "", false);
    }
}

#[cfg(not(target_os = "macos"))]
fn clear_permission_prompt_if_available() {
    emit_overlay_permission("", "", "", false);
}

#[cfg(target_os = "macos")]
fn on_accessibility_permission_action() {
    let cfg = config::get();
    let label = HotkeySpec::parse(&cfg.hotkey).label();
    emit_accessibility_permission_required(&label);
}

pub fn stop() -> Result<(), String> {
    RUNNING.store(false, Ordering::SeqCst);
    reset_active_runtime();
    set_state(RuntimeState::Idle);
    set_onboarding_step(None);
    stop_overlay();
    emit("dictation:runtime-stopped", Value::Null);
    if let Ok(mut host) = HOST.lock() {
        *host = None;
    }
    Ok(())
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

fn reset_active_runtime() {
    hotkey::stop();
    if matches!(state(), RuntimeState::Recording | RuntimeState::Paused) {
        let _ = audio::stop_recording();
        audio_ducking::end();
    }
    clear_recording_clock();
    if let Ok(mut peak) = PEAK_LEVEL.lock() {
        *peak = 0.0;
    }
    if let Ok(mut hold_started_at) = ONBOARDING_HOLD_STARTED_AT.lock() {
        *hold_started_at = None;
    }
    if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
        *active_hands_free = false;
    }
    IGNORE_PENDING_KEY_UP.store(false, Ordering::SeqCst);
    RECORDING_GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn on_key_down() {
    if matches!(state(), RuntimeState::Recording | RuntimeState::Paused)
        && is_hands_free_recording()
    {
        stop_and_process();
        return;
    }

    if let Some(step) = onboarding_step() {
        match step {
            OnboardingStep::TryIt => {
                let _ = begin_press_to_record();
            }
            OnboardingStep::ApiTip | OnboardingStep::FormattingTip | OnboardingStep::Welcome => {
                start_onboarding_hold(step);
            }
            OnboardingStep::DoubleTapTip | OnboardingStep::ClickTip => {}
        }
        return;
    }

    if matches!(state(), RuntimeState::Idle) {
        let _ = begin_press_to_record();
    }
}

fn begin_press_to_record() -> bool {
    if !matches!(state(), RuntimeState::Idle) {
        return false;
    }

    let generation = next_recording_generation();
    if let Ok(mut peak) = PEAK_LEVEL.lock() {
        *peak = 0.0;
    }
    start_recording_clock();
    if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
        *active_hands_free = false;
    }
    IGNORE_PENDING_KEY_UP.store(false, Ordering::SeqCst);
    set_state(RuntimeState::PressPending);
    play_sound(SOUND_START_PRESS);

    std::thread::Builder::new()
        .name("yap-core-hold-to-record-delay".into())
        .spawn(move || {
            std::thread::sleep(HOLD_TO_RECORD_DELAY);
            if state() != RuntimeState::PressPending || current_recording_generation() != generation
            {
                return;
            }
            let _ = start_recording_for_generation(generation, false);
        })
        .ok();

    true
}

fn begin_hands_free_recording(ignore_pending_key_up: bool) -> bool {
    if !matches!(
        state(),
        RuntimeState::Idle | RuntimeState::PressPending | RuntimeState::TapPending
    ) {
        return false;
    }

    let generation = next_recording_generation();
    start_recording_clock();
    if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
        *active_hands_free = true;
    }
    if let Ok(mut peak) = PEAK_LEVEL.lock() {
        *peak = 0.0;
    }
    IGNORE_PENDING_KEY_UP.store(ignore_pending_key_up, Ordering::SeqCst);
    set_state(RuntimeState::Recording);
    play_recording_cue(SOUND_HANDS_FREE);
    start_recording_for_generation(generation, true)
}

fn start_recording_for_generation(generation: u64, hands_free: bool) -> bool {
    if current_recording_generation() != generation {
        return false;
    }

    let expected_state = if hands_free {
        RuntimeState::Recording
    } else {
        RuntimeState::PressPending
    };
    if state() != expected_state {
        return false;
    }

    let cfg = config::get();
    let device = cfg.audio_device.trim();
    audio_ducking::begin(cfg.background_audio_mode());

    match audio::start_recording((!device.is_empty()).then_some(device)) {
        Ok(_) => {
            start_recording_clock();
            if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
                *active_hands_free = hands_free;
            }
            if let Ok(mut peak) = PEAK_LEVEL.lock() {
                *peak = 0.0;
            }
            set_state(RuntimeState::Recording);
            complete_onboarding_recording_action(
                if hands_free {
                    OnboardingStep::DoubleTapTip
                } else {
                    OnboardingStep::TryIt
                },
                if hands_free {
                    OnboardingStep::ClickTip
                } else {
                    OnboardingStep::DoubleTapTip
                },
            );
            true
        }
        Err(error) => {
            audio_ducking::end();
            emit_error("Recording failed", error);
            set_state(RuntimeState::Idle);
            false
        }
    }
}

fn on_key_up() {
    if ONBOARDING_HOLD_STARTED_AT
        .lock()
        .map(|hold| hold.is_some())
        .unwrap_or(false)
    {
        clear_onboarding_hold();
        return;
    }

    if state() == RuntimeState::PressPending {
        begin_tap_pending("quick tap", true);
        return;
    }

    if is_hands_free_recording() {
        IGNORE_PENDING_KEY_UP.store(false, Ordering::SeqCst);
        return;
    }

    if !matches!(state(), RuntimeState::Recording) {
        return;
    }

    let peak = current_peak_level();
    if recording_elapsed_duration().as_millis() < 500 && peak < SILENCE_PEAK_THRESHOLD {
        clear_recording_clock();
        let _ = audio::stop_recording();
        audio_ducking::end();
        begin_tap_pending("too short / quiet", false);
        return;
    }

    stop_and_process();
}

fn on_double_tap() {
    if let Some(step) = onboarding_step() {
        if step != OnboardingStep::DoubleTapTip {
            return;
        }
    }

    match state() {
        RuntimeState::Recording if !is_hands_free_recording() => {
            if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
                *active_hands_free = true;
            }
            IGNORE_PENDING_KEY_UP.store(true, Ordering::SeqCst);
            set_state(RuntimeState::Recording);
            play_sound(SOUND_HANDS_FREE);
        }
        RuntimeState::Idle | RuntimeState::PressPending | RuntimeState::TapPending => {
            let _ = begin_hands_free_recording(true);
        }
        _ => {}
    }
}

fn on_overlay_pill_click() {
    match state() {
        RuntimeState::Idle => {
            if let Some(step) = onboarding_step() {
                if step == OnboardingStep::ClickTip && begin_hands_free_recording(false) {
                    advance_onboarding_step(OnboardingStep::ApiTip);
                }
            } else {
                let _ = begin_hands_free_recording(false);
            }
        }
        RuntimeState::Recording if !is_hands_free_recording() => {
            if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
                *active_hands_free = true;
            }
            IGNORE_PENDING_KEY_UP.store(true, Ordering::SeqCst);
            set_state(RuntimeState::Recording);
            play_sound(SOUND_HANDS_FREE);
        }
        RuntimeState::PressPending
        | RuntimeState::TapPending
        | RuntimeState::Recording
        | RuntimeState::Paused
        | RuntimeState::Processing => {}
    }
}

fn on_overlay_pause() {
    match state() {
        RuntimeState::Recording if is_hands_free_recording() => {
            audio::pause_recording();
            pause_recording_clock();
            set_state(RuntimeState::Paused);
        }
        RuntimeState::Paused => {
            audio::resume_recording();
            resume_recording_clock();
            set_state(RuntimeState::Recording);
        }
        _ => {}
    }
}

fn on_overlay_stop() {
    if matches!(state(), RuntimeState::Recording | RuntimeState::Paused) {
        stop_and_process();
    }
}

fn next_recording_generation() -> u64 {
    RECORDING_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

fn current_recording_generation() -> u64 {
    RECORDING_GENERATION.load(Ordering::SeqCst)
}

fn begin_tap_pending(reason: &'static str, play_skip_sound: bool) {
    let generation = current_recording_generation();
    if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
        *active_hands_free = false;
    }
    IGNORE_PENDING_KEY_UP.store(false, Ordering::SeqCst);
    set_state(RuntimeState::TapPending);

    std::thread::Builder::new()
        .name("yap-core-tap-pending-grace".into())
        .spawn(move || {
            std::thread::sleep(SHORT_TAP_TIP_GRACE);
            if state() != RuntimeState::TapPending || current_recording_generation() != generation {
                return;
            }
            if play_skip_sound {
                play_sound(SOUND_SKIP);
            }
            emit("dictation:skipped", json!({ "reason": reason }));
            set_state(RuntimeState::Idle);
        })
        .ok();
}

fn start_onboarding_if_needed(cfg: &AppConfig) {
    if cfg.onboarding_complete {
        set_onboarding_step(None);
    } else {
        set_onboarding_step(Some(OnboardingStep::TryIt));
    }
}

fn onboarding_step() -> Option<OnboardingStep> {
    ONBOARDING_STEP.lock().map(|step| *step).unwrap_or(None)
}

fn set_onboarding_step(step: Option<OnboardingStep>) {
    if let Ok(mut active_step) = ONBOARDING_STEP.lock() {
        *active_step = step;
    }
    emit_overlay_onboarding(step);
}

fn advance_onboarding_step(next: OnboardingStep) {
    play_sound(SOUND_NEXT);
    set_onboarding_step(Some(next));
}

fn complete_onboarding_recording_action(expected: OnboardingStep, next: OnboardingStep) {
    if onboarding_step() == Some(expected) {
        advance_onboarding_step(next);
    }
}

fn start_onboarding_hold(step: OnboardingStep) {
    if !matches!(
        step,
        OnboardingStep::ApiTip | OnboardingStep::FormattingTip | OnboardingStep::Welcome
    ) {
        return;
    }

    let started_at = Instant::now();
    if let Ok(mut hold_started_at) = ONBOARDING_HOLD_STARTED_AT.lock() {
        *hold_started_at = Some(started_at);
    }
    emit_overlay_onboarding_press(true);

    std::thread::spawn(move || {
        std::thread::sleep(ONBOARDING_HOLD_CONFIRM_DELAY);
        let should_advance = ONBOARDING_HOLD_STARTED_AT
            .lock()
            .map(|hold_started_at| hold_started_at.is_some_and(|active| active == started_at))
            .unwrap_or(false)
            && onboarding_step() == Some(step);

        if !should_advance {
            return;
        }

        clear_onboarding_hold();
        match step {
            OnboardingStep::ApiTip => advance_onboarding_step(OnboardingStep::FormattingTip),
            OnboardingStep::FormattingTip => advance_onboarding_step(OnboardingStep::Welcome),
            OnboardingStep::Welcome => finalize_onboarding(),
            _ => {}
        }
    });
}

fn clear_onboarding_hold() {
    if let Ok(mut hold_started_at) = ONBOARDING_HOLD_STARTED_AT.lock() {
        *hold_started_at = None;
    }
    emit_overlay_onboarding_press(false);
}

fn finalize_onboarding() {
    let _ = config::update(|cfg| {
        cfg.onboarding_complete = true;
    });
    set_onboarding_step(None);
}

fn stop_and_process() {
    set_state(RuntimeState::Processing);
    clear_recording_clock();
    let wav_path = match audio::stop_recording() {
        Ok(path) => path,
        Err(error) => {
            audio_ducking::end();
            emit_error("Recording failed", error);
            set_state(RuntimeState::Idle);
            return;
        }
    };
    audio_ducking::end();
    play_sound(SOUND_SKIP);
    let peak = current_peak_level();

    if peak < SILENCE_PEAK_THRESHOLD {
        emit(
            "dictation:skipped",
            json!({
                "reason": "silence",
                "peak": peak,
            }),
        );
        set_state(RuntimeState::Idle);
        return;
    }

    std::thread::Builder::new()
        .name("yap-core-process-audio".into())
        .spawn(move || {
            let cfg = config::get();
            if !vad::pre_check(&wav_path) {
                emit("dictation:skipped", json!({ "reason": "no speech" }));
                set_state(RuntimeState::Idle);
                return;
            }

            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("failed to start async runtime: {error}"))
                .and_then(|runtime| runtime.block_on(process_audio_pipeline(&wav_path, &cfg)));

            let keep_overlay_error = match result {
                Ok(result) if !result.text.trim().is_empty() => {
                    let text = result.text.trim().to_string();
                    if cfg.history_enabled {
                        let _ = history::append(result.into_history_append(text.clone()));
                        emit("history:changed", Value::Null);
                    }
                    if let Err(error) = paste::paste_text(&text, cfg.press_enter_after_paste) {
                        emit_error("Paste failed", error);
                    } else {
                        emit("dictation:pasted", json!({ "text": text }));
                    }
                    false
                }
                Ok(_) => {
                    emit(
                        "dictation:skipped",
                        json!({ "reason": "empty transcription" }),
                    );
                    false
                }
                Err(error) => {
                    let settings_section = provider_settings_section_for_error(&error);
                    emit_processing_error(error);
                    if let Some(section) = settings_section {
                        emit("settings:show-section", json!(section));
                    }
                    true
                }
            };

            set_state_with_overlay(RuntimeState::Idle, !keep_overlay_error);
        })
        .ok();
}

#[derive(Debug, Clone)]
struct ProcessedDictation {
    text: String,
    raw_text: Option<String>,
    formatted_text: Option<String>,
    transcription_provider: String,
    transcription_model: Option<String>,
    formatting_provider: Option<String>,
    formatting_model: Option<String>,
    formatting_style: Option<String>,
    formatting_instruction: Option<String>,
}

impl ProcessedDictation {
    fn unformatted(text: String, cfg: &AppConfig) -> Self {
        Self {
            raw_text: Some(text.clone()),
            text,
            formatted_text: None,
            transcription_provider: provider_name(&cfg.tx_provider),
            transcription_model: transcription_model_for_history(cfg),
            formatting_provider: None,
            formatting_model: None,
            formatting_style: None,
            formatting_instruction: None,
        }
    }

    fn formatted(text: String, raw_text: Option<String>, cfg: &AppConfig) -> Self {
        let formatting_model = if cfg.fmt_provider == FormattingProvider::None {
            None
        } else {
            let model = if cfg.fmt_model.trim().is_empty() {
                cfg.fmt_provider.default_model().to_string()
            } else {
                cfg.fmt_model.clone()
            };
            (!model.trim().is_empty()).then_some(model)
        };

        Self {
            raw_text,
            formatted_text: Some(text.clone()),
            text,
            transcription_provider: provider_name(&cfg.tx_provider),
            transcription_model: transcription_model_for_history(cfg),
            formatting_provider: (cfg.fmt_provider != FormattingProvider::None)
                .then(|| provider_name(&cfg.fmt_provider)),
            formatting_model,
            formatting_style: (cfg.fmt_provider != FormattingProvider::None)
                .then(|| format!("{:?}", cfg.fmt_style).to_lowercase()),
            formatting_instruction: (cfg.fmt_provider != FormattingProvider::None)
                .then(|| formatting::resolved_instruction(cfg.fmt_style, &cfg.fmt_custom_prompt)),
        }
    }

    fn into_history_append(self, text: String) -> HistoryAppend {
        HistoryAppend {
            text,
            raw_text: self.raw_text,
            formatted_text: self.formatted_text,
            transcription_provider: self.transcription_provider,
            transcription_model: self.transcription_model,
            formatting_provider: self.formatting_provider,
            formatting_model: self.formatting_model,
            formatting_style: self.formatting_style,
            formatting_instruction: self.formatting_instruction,
        }
    }
}

async fn process_audio_pipeline(
    wav_path: &PathBuf,
    cfg: &AppConfig,
) -> Result<ProcessedDictation, String> {
    if cfg!(not(target_os = "macos")) && cfg.tx_provider == TranscriptionProvider::None {
        return Err("Choose an API provider in Settings".to_string());
    }

    if cfg.tx_provider.requires_api_key() && cfg.tx_api_key.is_empty() {
        return Err("Set up an API key in Settings".to_string());
    }

    let tx_options = TranscriptionOptions {
        api_key: cfg.tx_api_key.clone(),
        model: cfg.tx_model.clone(),
        dg_smart_format: cfg.dg_smart_format,
        dg_keywords: cfg.dg_keywords.clone(),
        dg_language: cfg.dg_language.clone(),
        oai_language: cfg.oai_language.clone(),
        oai_prompt: cfg.oai_prompt.clone(),
        gemini_temperature: cfg.gemini_temperature,
        el_language_code: cfg.el_language_code.clone(),
    };

    let use_oneshot = cfg.tx_provider == TranscriptionProvider::Gemini
        && cfg.fmt_provider == FormattingProvider::Gemini
        && cfg.tx_provider.can_also_format();

    let raw_text = if use_oneshot {
        transcription::transcribe_gemini_oneshot(
            wav_path,
            &tx_options,
            cfg.fmt_style,
            &cfg.fmt_custom_prompt,
        )
        .await
        .map_err(|error| format!("Transcription failed: {error}"))?
    } else {
        transcription::transcribe(cfg.tx_provider, wav_path, &tx_options)
            .await
            .map_err(|error| format!("Transcription failed: {error}"))?
    };

    let trimmed = raw_text.trim().to_string();
    if trimmed.is_empty() {
        return Ok(ProcessedDictation::unformatted(trimmed, cfg));
    }

    if is_prompt_regurgitation(&trimmed) {
        return Ok(ProcessedDictation::unformatted(String::new(), cfg));
    }

    if use_oneshot {
        let text = formatting::render_formatted_text_for_paste(&trimmed);
        let mut result = ProcessedDictation::formatted(text, None, cfg);
        result.formatting_model = result.transcription_model.clone();
        result.formatting_instruction = Some(transcription::audio_prompt_for_style(
            cfg.fmt_style,
            &cfg.fmt_custom_prompt,
        ));
        return Ok(result);
    }

    let fmt_api_key = if cfg.fmt_api_key.is_empty() {
        cfg.tx_api_key.clone()
    } else {
        cfg.fmt_api_key.clone()
    };

    if cfg.fmt_provider == FormattingProvider::None {
        return Ok(ProcessedDictation::unformatted(trimmed, cfg));
    }
    if cfg.fmt_provider.requires_api_key() && fmt_api_key.is_empty() {
        return Err("Set up a formatting API key in Settings".to_string());
    }

    let fmt_options = FormattingOptions {
        api_key: fmt_api_key,
        model: cfg.fmt_model.clone(),
        style: cfg.fmt_style,
        custom_prompt: cfg.fmt_custom_prompt.clone(),
    };
    let formatted = formatting::format(cfg.fmt_provider, &trimmed, &fmt_options)
        .await
        .map_err(|error| format!("Formatting failed: {error}"))?;
    if !formatted.applied || is_prompt_regurgitation(&formatted.text) {
        return Ok(ProcessedDictation::unformatted(trimmed, cfg));
    }
    Ok(ProcessedDictation::formatted(
        formatted.text,
        Some(trimmed),
        cfg,
    ))
}

fn provider_name<T: std::fmt::Debug>(provider: &T) -> String {
    format!("{provider:?}").to_lowercase()
}

fn transcription_model_for_history(cfg: &AppConfig) -> Option<String> {
    let model = if cfg.tx_model.trim().is_empty() {
        cfg.tx_provider.default_model()
    } else {
        cfg.tx_model.trim()
    };
    (!model.is_empty()).then(|| model.to_string())
}

fn classify_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("choose an api provider") {
        "Choose an API provider in Settings".to_string()
    } else if lower.contains("set up a formatting api key") {
        "Set up a formatting API key in Settings".to_string()
    } else if lower.contains("set up an api key") {
        "Set up an API key in Settings".to_string()
    } else if lower.contains("quota") || lower.contains("rate") || lower.contains("429") {
        "Rate limited -- try again".to_string()
    } else if lower.contains("appleintelligencenotenabled")
        || lower.contains("foundation models formatter is unavailable")
    {
        "Apple Intelligence unavailable".to_string()
    } else if is_provider_settings_error(&lower) {
        "Invalid API key".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "Request timed out".to_string()
    } else if lower.contains("offline") || lower.contains("network") || lower.contains("internet") {
        "No internet connection".to_string()
    } else {
        "Something went wrong".to_string()
    }
}

fn provider_settings_section_for_error(error: &str) -> Option<&'static str> {
    let lower = error.to_lowercase();
    if lower.contains("set up a formatting api key") {
        return Some("formatting");
    }
    if lower.contains("appleintelligencenotenabled")
        || lower.contains("foundation models formatter is unavailable")
        || lower.contains("choose an ollama model")
    {
        return Some("formatting");
    }
    if lower.contains("choose an api provider")
        || lower.contains("set up an api key")
        || lower.contains("choose a local whisper model")
    {
        return Some("transcription");
    }
    if !is_provider_settings_error(&lower) {
        return None;
    }
    if lower.contains("format") {
        Some("formatting")
    } else {
        Some("transcription")
    }
}

fn is_provider_settings_error(lower: &str) -> bool {
    lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("authentication")
        || lower.contains("authorization")
        || lower.contains("invalid api key")
        || lower.contains("api key invalid")
        || lower.contains("api key not valid")
        || lower.contains("missing api key")
        || lower.contains("invalid token")
}

fn is_prompt_regurgitation(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("transcribe this audio")
        || lower.contains("respond with only a json")
        || lower.contains("dictation commands")
}

fn state() -> RuntimeState {
    RUNTIME
        .lock()
        .map(|state| *state)
        .unwrap_or(RuntimeState::Idle)
}

fn set_state(next: RuntimeState) {
    set_state_with_overlay(next, true);
}

fn set_state_with_overlay(next: RuntimeState, update_overlay: bool) {
    if matches!(next, RuntimeState::Idle) {
        if let Ok(mut active_hands_free) = HANDS_FREE_RECORDING.lock() {
            *active_hands_free = false;
        }
        if let Ok(mut started_at) = RECORDING_STARTED_AT.lock() {
            *started_at = None;
        }
        if let Ok(mut paused_at) = RECORDING_PAUSED_AT.lock() {
            *paused_at = None;
        }
        if let Ok(mut paused_duration) = RECORDING_PAUSED_DURATION.lock() {
            *paused_duration = Duration::ZERO;
        }
        if let Ok(mut peak) = PEAK_LEVEL.lock() {
            *peak = 0.0;
        }
        IGNORE_PENDING_KEY_UP.store(false, Ordering::SeqCst);
    }

    if let Ok(mut state) = RUNTIME.lock() {
        *state = next;
    }

    let state = match next {
        RuntimeState::Idle => "idle",
        RuntimeState::PressPending | RuntimeState::TapPending => "pending",
        RuntimeState::Recording => "recording",
        RuntimeState::Paused => "paused",
        RuntimeState::Processing => "processing",
    };
    let hands_free = is_hands_free_recording();
    let paused = matches!(next, RuntimeState::Paused);
    emit(
        "dictation:state",
        json!({
            "state": state,
            "handsFree": hands_free,
            "paused": paused,
        }),
    );
    if update_overlay {
        emit_overlay_state(state.to_string(), hands_free, paused);
    }
}

fn emit_error(title: &str, message: String) {
    emit_overlay_error(&message);
    emit(
        "dictation:error",
        json!({
            "title": title,
            "message": message,
        }),
    );
}

fn emit_processing_error(error: String) {
    let display_message = classify_error(&error);
    emit_overlay_error(&display_message);
    emit(
        "dictation:error",
        json!({
            "title": display_message,
            "message": error,
        }),
    );
}

fn emit(event: &'static str, payload: Value) {
    let host = HOST
        .lock()
        .ok()
        .and_then(|active_host| active_host.as_ref().map(Arc::clone));
    if let Some(host) = host {
        host.emit(event, payload);
    }
}

fn is_hands_free_recording() -> bool {
    HANDS_FREE_RECORDING
        .lock()
        .map(|active_hands_free| *active_hands_free)
        .unwrap_or(false)
}

fn start_level_poller() {
    if LEVEL_POLLER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::Builder::new()
        .name("yap-core-level-poller".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(33));
            if !matches!(state(), RuntimeState::Recording | RuntimeState::Paused) {
                continue;
            }

            let levels = audio::get_levels();
            let elapsed = recording_elapsed_seconds();
            update_peak_level(levels.level);
            emit(
                "dictation:levels",
                json!({
                    "level": levels.level,
                    "bars": levels.bars,
                }),
            );
            emit_overlay_levels(levels.level, levels.bars.to_vec(), elapsed);
        })
        .ok();
}

fn update_peak_level(level: f32) {
    if let Ok(mut peak) = PEAK_LEVEL.lock() {
        *peak = peak.max(level);
    }
}

fn current_peak_level() -> f32 {
    PEAK_LEVEL.lock().map(|peak| *peak).unwrap_or(0.0)
}

fn play_recording_cue(name: &str) {
    if !config::get().sounds_enabled {
        return;
    }
    play_sound(name);
    if name == SOUND_HANDS_FREE {
        std::thread::sleep(PRE_RECORDING_CUE_DELAY);
    }
}

fn play_sound(name: &str) {
    if !config::get().sounds_enabled {
        return;
    }

    let Some(path) = sound_path(name) else {
        return;
    };

    std::thread::spawn(move || {
        if let Ok((_stream, stream_handle)) = rodio::OutputStream::try_default() {
            if let Ok(file) = std::fs::File::open(&path) {
                let source = rodio::Decoder::new(std::io::BufReader::new(file));
                if let Ok(source) = source {
                    let _ = stream_handle.play_raw(rodio::source::Source::convert_samples(source));
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    });
}

fn sound_path(name: &str) -> Option<PathBuf> {
    let file_name = format!("{name}.wav");
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("sounds")
        .join(&file_name);

    let mut candidates = vec![manifest_path];
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(bin_dir) = current_exe.parent() {
            candidates.push(bin_dir.join("sounds").join(&file_name));
            if let Some(resource_dir) = bin_dir.parent() {
                candidates.push(resource_dir.join("sounds").join(&file_name));
            }
        }
    }

    candidates.into_iter().find(|path| path.exists())
}

#[cfg(target_os = "macos")]
fn spawn_overlay(cfg: &AppConfig) {
    crate::sidecar::spawn_for_core(handle_overlay_event);
    emit_overlay_config(cfg);
}

#[cfg(target_os = "windows")]
fn spawn_overlay(cfg: &AppConfig) {
    crate::win_overlay::spawn_for_core(|event| handle_overlay_event(event.to_string()));
    emit_overlay_config(cfg);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn spawn_overlay(_cfg: &AppConfig) {}

#[cfg(target_os = "macos")]
fn stop_overlay() {
    emit_overlay_state("idle".to_string(), false, false);
    emit_overlay_levels(0.0, vec![0.0; 11], 0.0);
    crate::sidecar::stop();
}

#[cfg(target_os = "windows")]
fn stop_overlay() {
    emit_overlay_state("idle".to_string(), false, false);
    emit_overlay_levels(0.0, vec![0.0; 11], 0.0);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn stop_overlay() {}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn handle_overlay_event(event: String) {
    match event.as_str() {
        "ready" => {
            let cfg = config::get();
            emit_overlay_config(&cfg);
            emit_overlay_onboarding(onboarding_step());
            emit_overlay_state(
                state_label(state()).to_string(),
                is_hands_free_recording(),
                matches!(state(), RuntimeState::Paused),
            );
            #[cfg(target_os = "macos")]
            {
                if !hotkey::has_accessibility_permission() {
                    let hotkey_label = HotkeySpec::parse(&cfg.hotkey).label();
                    emit_accessibility_permission_required(&hotkey_label);
                }
            }
        }
        "pill_click" => on_overlay_pill_click(),
        #[cfg(target_os = "macos")]
        "permission_action" => {
            on_accessibility_permission_action();
        }
        "pause" => on_overlay_pause(),
        "stop" => on_overlay_stop(),
        _ => {}
    }
}

#[cfg(target_os = "macos")]
fn emit_overlay_config(cfg: &AppConfig) {
    crate::sidecar::send(&crate::sidecar::OutMessage::Config {
        gradient_enabled: cfg.gradient_enabled,
        always_visible: cfg.always_visible_pill,
        appearance_mode: cfg.appearance_mode.as_str().to_string(),
        hotkey_label: HotkeySpec::parse(&cfg.hotkey).label(),
    });
}

#[cfg(target_os = "windows")]
fn emit_overlay_config(cfg: &AppConfig) {
    crate::win_overlay::update_state(|state| {
        state.gradient_enabled = cfg.gradient_enabled;
        state.always_visible = cfg.always_visible_pill;
        state.appearance_mode = cfg.appearance_mode;
        state.hotkey_label = HotkeySpec::parse(&cfg.hotkey).label();
    });
}

#[cfg(target_os = "macos")]
fn emit_overlay_state(state: String, hands_free: bool, paused: bool) {
    crate::sidecar::send(&crate::sidecar::OutMessage::State {
        state: if paused {
            "recording".to_string()
        } else {
            state
        },
        hands_free,
        paused,
        elapsed: recording_elapsed_seconds(),
    });
}

#[cfg(target_os = "windows")]
fn emit_overlay_state(state: String, hands_free: bool, paused: bool) {
    crate::win_overlay::update_state(|overlay| {
        overlay.mode = if paused {
            "recording".to_string()
        } else {
            state
        };
        overlay.hands_free = hands_free;
        overlay.paused = paused;
        overlay.elapsed = recording_elapsed_seconds();
        overlay.error = None;
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn emit_overlay_state(_state: String, _hands_free: bool, _paused: bool) {}

#[cfg(target_os = "macos")]
fn emit_overlay_levels(level: f32, bars: Vec<f32>, elapsed: f64) {
    crate::sidecar::send(&crate::sidecar::OutMessage::Levels {
        level,
        bars,
        elapsed,
    });
}

#[cfg(target_os = "windows")]
fn emit_overlay_levels(level: f32, bars: Vec<f32>, elapsed: f64) {
    crate::win_overlay::update_state(|overlay| {
        overlay.level = level;
        overlay.elapsed = elapsed;
        if bars.len() == 11 {
            overlay.bars.copy_from_slice(&bars);
        }
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn emit_overlay_levels(_level: f32, _bars: Vec<f32>, _elapsed: f64) {}

#[cfg(target_os = "macos")]
fn emit_overlay_error(message: &str) {
    crate::sidecar::send(&crate::sidecar::OutMessage::Error {
        message: message.to_string(),
    });
}

#[cfg(target_os = "windows")]
fn emit_overlay_error(message: &str) {
    crate::win_overlay::update_state(|overlay| {
        overlay.mode = "error".to_string();
        overlay.error = Some(message.to_string());
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn emit_overlay_error(_message: &str) {}

#[cfg(target_os = "macos")]
fn emit_overlay_onboarding(step: Option<OnboardingStep>) {
    let cfg = config::get();
    let hotkey_label = HotkeySpec::parse(&cfg.hotkey).label();
    let (step, text) = match step {
        Some(step) => (
            step.to_str().to_string(),
            onboarding_text(step, &hotkey_label),
        ),
        None => (String::new(), String::new()),
    };

    crate::sidecar::send(&crate::sidecar::OutMessage::Onboarding {
        step,
        text,
        hotkey_label,
    });
}

#[cfg(target_os = "windows")]
fn emit_overlay_onboarding(step: Option<OnboardingStep>) {
    crate::win_overlay::update_state(|overlay| {
        overlay.onboarding_step =
            step.and_then(|step| crate::win_overlay::OnboardingStep::from_str(step.to_str()));
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn emit_overlay_onboarding(_step: Option<OnboardingStep>) {}

#[cfg(target_os = "macos")]
fn emit_overlay_onboarding_press(pressed: bool) {
    crate::sidecar::send(&crate::sidecar::OutMessage::OnboardingPress { pressed });
}

#[cfg(target_os = "windows")]
fn emit_overlay_onboarding_press(pressed: bool) {
    crate::win_overlay::update_state(|overlay| {
        overlay.is_pressed = pressed;
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn emit_overlay_onboarding_press(_pressed: bool) {}

fn onboarding_text(step: OnboardingStep, hotkey_label: &str) -> String {
    match step {
        OnboardingStep::TryIt => {
            format!("Hold <span class=\"keycap\">{hotkey_label}</span> to start recording")
        }
        OnboardingStep::DoubleTapTip => format!(
            "Double-tap <span class=\"keycap\">{hotkey_label}</span> for hands-free recording"
        ),
        OnboardingStep::ClickTip => "Click the pill for hands-free recording".to_string(),
        OnboardingStep::ApiTip => {
            "Add an API key in the menu bar for better transcription".to_string()
        }
        OnboardingStep::FormattingTip => {
            "Enable formatting in Settings to clean up grammar and punctuation automatically"
                .to_string()
        }
        OnboardingStep::Welcome => "You're all set - enjoy!".to_string(),
    }
}

#[cfg(target_os = "macos")]
fn emit_overlay_permission(title: &str, message: &str, action_label: &str, visible: bool) {
    crate::sidecar::send(&crate::sidecar::OutMessage::Permission {
        title: title.to_string(),
        message: message.to_string(),
        action_label: action_label.to_string(),
        visible,
    });
}

#[cfg(not(target_os = "macos"))]
fn emit_overlay_permission(_title: &str, _message: &str, _action_label: &str, _visible: bool) {}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn recording_elapsed_seconds() -> f64 {
    recording_elapsed_duration().as_secs_f64()
}

fn start_recording_clock() {
    if let Ok(mut started_at) = RECORDING_STARTED_AT.lock() {
        *started_at = Some(Instant::now());
    }
    if let Ok(mut paused_at) = RECORDING_PAUSED_AT.lock() {
        *paused_at = None;
    }
    if let Ok(mut paused_duration) = RECORDING_PAUSED_DURATION.lock() {
        *paused_duration = Duration::ZERO;
    }
}

fn clear_recording_clock() {
    if let Ok(mut started_at) = RECORDING_STARTED_AT.lock() {
        *started_at = None;
    }
    if let Ok(mut paused_at) = RECORDING_PAUSED_AT.lock() {
        *paused_at = None;
    }
    if let Ok(mut paused_duration) = RECORDING_PAUSED_DURATION.lock() {
        *paused_duration = Duration::ZERO;
    }
}

fn pause_recording_clock() {
    if let Ok(mut paused_at) = RECORDING_PAUSED_AT.lock() {
        if paused_at.is_none() {
            *paused_at = Some(Instant::now());
        }
    }
}

fn resume_recording_clock() {
    let paused_at = RECORDING_PAUSED_AT
        .lock()
        .ok()
        .and_then(|mut paused_at| paused_at.take());
    if let Some(paused_at) = paused_at {
        if let Ok(mut paused_duration) = RECORDING_PAUSED_DURATION.lock() {
            *paused_duration += paused_at.elapsed();
        }
    }
}

fn recording_elapsed_duration() -> Duration {
    RECORDING_STARTED_AT
        .lock()
        .ok()
        .and_then(|started_at| *started_at)
        .map(|started_at| {
            let mut paused_duration = RECORDING_PAUSED_DURATION
                .lock()
                .map(|duration| *duration)
                .unwrap_or(Duration::ZERO);
            if let Some(paused_at) = RECORDING_PAUSED_AT
                .lock()
                .ok()
                .and_then(|paused_at| *paused_at)
            {
                paused_duration += paused_at.elapsed();
            }
            started_at.elapsed().saturating_sub(paused_duration)
        })
        .unwrap_or(Duration::ZERO)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn state_label(state: RuntimeState) -> &'static str {
    match state {
        RuntimeState::Idle => "idle",
        RuntimeState::PressPending | RuntimeState::TapPending => "pending",
        RuntimeState::Recording => "recording",
        RuntimeState::Paused => "paused",
        RuntimeState::Processing => "processing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_cleanup_rejects_recording_and_processing_before_touching_files() {
        for busy in [
            RuntimeState::PressPending,
            RuntimeState::Recording,
            RuntimeState::Paused,
            RuntimeState::Processing,
        ] {
            *RUNTIME.lock().unwrap() = busy;
            let result = clear_runtime_cache();
            *RUNTIME.lock().unwrap() = RuntimeState::Idle;
            assert!(result.unwrap_err().contains("Finish the current dictation"));
        }
    }

    #[test]
    fn unformatted_history_has_raw_text_and_default_model_without_formatter_metadata() {
        let cfg = AppConfig {
            tx_provider: TranscriptionProvider::OpenAI,
            fmt_provider: FormattingProvider::Ollama,
            ..Default::default()
        };
        let result = ProcessedDictation::unformatted("Keep my words".into(), &cfg);
        let entry = result.into_history_append("Keep my words".into());
        assert_eq!(entry.raw_text.as_deref(), Some("Keep my words"));
        assert_eq!(
            entry.transcription_model.as_deref(),
            Some("gpt-4o-transcribe")
        );
        assert!(entry.formatted_text.is_none());
        assert!(entry.formatting_provider.is_none());
        assert!(entry.formatting_instruction.is_none());
    }
}
