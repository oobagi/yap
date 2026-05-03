//! Central pipeline orchestrator — equivalent to AppDelegate.swift.
//!
//! Owns the state machine and coordinates the full pipeline:
//! hotkey → audio → transcription → formatting → paste.
//!
//! All public methods are safe to call from any thread. Internal state is
//! guarded by a `Mutex` behind an `Arc` stored in Tauri's managed state.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::prelude::IndexedRandom;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, AudioLevels};
use crate::config::{self, AppConfig};
use crate::formatting::{self, FormattingOptions, FormattingProvider};
use crate::history;
use crate::hotkey::{self, HotkeySpec};
use crate::paste;
use crate::transcription::{self, TranscriptionOptions, TranscriptionProvider};
use crate::tray;
use crate::windows;

const SHORT_TAP_TIP_GRACE: Duration =
    Duration::from_millis((hotkey::DOUBLE_TAP_WINDOW * 1000.0) as u64 + 100);
const HOLD_TO_RECORD_DELAY: Duration = Duration::from_millis(250);
const SOUND_START_PRESS: &str = "Blow";
const SOUND_HANDS_FREE: &str = "HandsFree";
const SOUND_ERROR: &str = "Error";
const SOUND_SUCCESS: &str = "Submarine";
const SOUND_LOADING: &str = "Pop";

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppState {
    Idle,
    PressPending,
    Recording,
    TapPending,
    HandsFreeRecording,
    HandsFreePaused,
    Processing,
}

// ---------------------------------------------------------------------------
// Events emitted to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateChangePayload {
    /// Simplified state string for the frontend: "idle", "recording", "processing".
    state: String,
    /// Whether hands-free mode is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    hands_free: Option<bool>,
    /// Whether hands-free recording is paused.
    #[serde(skip_serializing_if = "Option::is_none")]
    paused: Option<bool>,
    /// Elapsed recording time in seconds (for timer display).
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload {
    message: String,
}

fn emit_levels_to_renderers(app: &AppHandle, levels: &AudioLevels) {
    let _ = app.emit("audio:levels", levels);
    #[cfg(target_os = "macos")]
    crate::sidecar::send(&crate::sidecar::OutMessage::Levels {
        level: levels.level,
        bars: levels.bars.to_vec(),
    });
    #[cfg(target_os = "windows")]
    {
        let l = levels.level;
        let b = levels.bars;
        crate::win_overlay::update_state(|st| {
            st.level = l;
            st.bars = b;
        });
    }
}

// ---------------------------------------------------------------------------
// Onboarding step state machine
// ---------------------------------------------------------------------------

/// All onboarding steps, matching the Swift app's guided flow.
///
/// Flow: tryIt -> nice -> doubleTapTip -> nice -> clickTip -> nice -> apiTip
///       -> formattingTip -> welcome -> complete
///
/// Transient tips (speakTip, holdTip) overlay the current step temporarily.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OnboardingStep {
    TryIt,
    Nice,
    DoubleTapTip,
    ClickTip,
    ApiTip,
    FormattingTip,
    Welcome,
    SpeakTip,
    HoldTip,
}

impl OnboardingStep {
    fn to_str(&self) -> &'static str {
        match self {
            Self::TryIt => "tryIt",
            Self::Nice => "nice",
            Self::DoubleTapTip => "doubleTapTip",
            Self::ClickTip => "clickTip",
            Self::ApiTip => "apiTip",
            Self::FormattingTip => "formattingTip",
            Self::Welcome => "welcome",
            Self::SpeakTip => "speakTip",
            Self::HoldTip => "holdTip",
        }
    }

    fn is_transient_tip(&self) -> bool {
        matches!(self, Self::SpeakTip | Self::HoldTip)
    }

    fn overlay_mode(&self) -> &'static str {
        match self {
            Self::SpeakTip | Self::HoldTip => "noSpeech",
            _ => "idle",
        }
    }
}

/// The next step to advance to after a `nice` celebration.
/// Stored separately since `nice` itself is just a visual celebration.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NiceContext {
    next_step: OnboardingStep,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OnboardingPayload {
    step: String,
    text: String,
    hotkey_label: String,
}

static NICE_MESSAGES: &[&str] = &[
    "Nice! \u{1f389}",
    "Nailed it! \u{2728}",
    "Sounds good! \u{1f44c}",
    "Got it! \u{1f64c}",
    "Perfect! \u{1f3af}",
    "Love it! \u{1f4ab}",
];

/// Build the onboarding card HTML text for a given step.
fn onboarding_text(step: &OnboardingStep, hotkey_label: &str) -> String {
    match step {
        OnboardingStep::TryIt => format!(
            "Hold <span class=\"keycap\">{}</span> and speak — Yap transcribes it",
            hotkey_label
        ),
        OnboardingStep::Nice => {
            let mut rng = rand::rng();
            NICE_MESSAGES
                .choose(&mut rng)
                .unwrap_or(&"Nice! \u{1f389}")
                .to_string()
        }
        OnboardingStep::DoubleTapTip => format!(
            "Double-tap <span class=\"keycap\">{}</span> for hands-free transcription",
            hotkey_label
        ),
        OnboardingStep::ClickTip => "Click the pill for hands-free transcription".to_string(),
        OnboardingStep::ApiTip => {
            "Add an API key in the menu bar for better transcription".to_string()
        }
        OnboardingStep::FormattingTip => {
            "Enable formatting in Settings to clean up grammar and punctuation automatically"
                .to_string()
        }
        OnboardingStep::Welcome => "You're all set — enjoy! \u{1f389}".to_string(),
        OnboardingStep::SpeakTip => "Try speaking up".to_string(),
        OnboardingStep::HoldTip => format!(
            "Hold <span class=\"keycap\">{}</span> — don't just tap it",
            hotkey_label
        ),
    }
}

// ---------------------------------------------------------------------------
// Orchestrator inner (lives behind Arc<Mutex<_>>)
// ---------------------------------------------------------------------------

struct OrchestratorInner {
    state: AppState,
    /// Whether the hotkey listener is enabled (tray toggle).
    enabled: bool,
    /// Peak audio level observed during the current recording session.
    peak_level: f32,
    /// Timestamp when the current recording started.
    recording_start: Option<Instant>,
    /// Elapsed recording time banked before the current pause/resume segment.
    recording_elapsed_before_pause: Duration,
    /// Monotonic id for the active/most recent recording attempt. Used to
    /// cancel delayed feedback from a tap that became part of a double-tap.
    recording_generation: u64,
    /// Whether we should ignore the next key-up event (hands-free entered
    /// while the hotkey was still held from the initial key-down).
    ignore_pending_key_up: bool,
    /// Handle to emit events and manage windows.
    app: AppHandle,

    // -- Onboarding state --
    /// Current onboarding step (None = onboarding complete or not started).
    onboarding_step: Option<OnboardingStep>,
    /// Whether onboarding is complete (mirrors config.onboarding_complete).
    onboarding_complete: bool,
    /// The step that was active before a transient tip (speakTip/holdTip)
    /// overwrote it. Used to restore after the tip auto-dismisses.
    pre_tip_step: Option<OnboardingStep>,
    /// When showing a `nice` step, the next step to advance to.
    nice_context: Option<NiceContext>,
    /// Timestamp of a pending hold-to-confirm action (for apiTip/formattingTip/welcome).
    hold_confirm_start: Option<Instant>,
    /// The hotkey label to display in onboarding cards.
    hotkey_label: String,
}

impl OrchestratorInner {
    fn begin_press_pending(&mut self) -> u64 {
        self.state = AppState::PressPending;
        self.reset_recording_clock();
        self.peak_level = 0.0;
        self.recording_generation = self.recording_generation.wrapping_add(1);
        let generation = self.recording_generation;
        self.emit_state();
        generation
    }

    fn begin_recording(&mut self) -> u64 {
        self.state = AppState::Recording;
        self.reset_recording_clock();
        self.peak_level = 0.0;
        self.recording_generation = self.recording_generation.wrapping_add(1);
        let generation = self.recording_generation;
        self.emit_state();
        generation
    }

    fn activate_pending_recording(&mut self, generation: u64) -> bool {
        if self.state != AppState::PressPending || self.recording_generation != generation {
            return false;
        }
        self.state = AppState::Recording;
        self.reset_recording_clock();
        self.peak_level = 0.0;
        self.emit_state();
        true
    }

    fn reset_recording_clock(&mut self) {
        self.recording_start = Some(Instant::now());
        self.recording_elapsed_before_pause = Duration::ZERO;
    }

    fn pause_recording_clock(&mut self) {
        if let Some(start) = self.recording_start.take() {
            self.recording_elapsed_before_pause += start.elapsed();
        }
    }

    fn resume_recording_clock(&mut self) {
        self.recording_start = Some(Instant::now());
    }

    fn recording_elapsed(&self) -> Duration {
        self.recording_elapsed_before_pause
            + self
                .recording_start
                .map(|start| start.elapsed())
                .unwrap_or_default()
    }

    /// Emit an overlay display state to every renderer without changing the
    /// app's internal pipeline state. Used for prompt-only states like
    /// noSpeech, which are visual states rather than AppState variants.
    fn emit_overlay_state(
        &self,
        state_str: &str,
        hands_free: Option<bool>,
        paused: Option<bool>,
        elapsed: Option<f64>,
    ) {
        let _ = self.app.emit(
            "state:change",
            StateChangePayload {
                state: state_str.to_string(),
                hands_free,
                paused,
                elapsed,
            },
        );

        #[cfg(target_os = "macos")]
        crate::sidecar::send(&crate::sidecar::OutMessage::State {
            state: state_str.to_string(),
            hands_free: hands_free.unwrap_or(false),
            paused: paused.unwrap_or(false),
            elapsed: elapsed.unwrap_or(0.0),
        });

        #[cfg(target_os = "windows")]
        {
            let s = state_str.to_string();
            let hf = hands_free.unwrap_or(false);
            let p = paused.unwrap_or(false);
            let el = elapsed.unwrap_or(0.0);
            crate::win_overlay::update_state(|st| {
                st.mode = s;
                st.hands_free = hf;
                st.paused = p;
                st.elapsed = el;
            });
        }
    }

    /// Emit a state change event to the frontend and update the tray icon.
    fn emit_state(&self) {
        // Map internal state to simplified frontend state string
        let state_str = match self.state {
            AppState::Idle => "idle",
            AppState::PressPending => "recording",
            AppState::Recording => "recording",
            AppState::TapPending => "recording",
            AppState::HandsFreeRecording => "recording",
            AppState::HandsFreePaused => "recording",
            AppState::Processing => "processing",
        };

        // Include hands-free metadata when in a recording state
        let (hands_free, paused, elapsed) = match self.state {
            AppState::HandsFreeRecording => (
                Some(true),
                Some(false),
                Some(self.recording_elapsed().as_secs_f64()),
            ),
            AppState::HandsFreePaused => (
                Some(true),
                Some(true),
                Some(self.recording_elapsed().as_secs_f64()),
            ),
            AppState::Recording | AppState::PressPending | AppState::TapPending => (
                Some(false),
                None,
                Some(self.recording_elapsed().as_secs_f64()),
            ),
            _ => (None, None, None),
        };

        self.emit_overlay_state(state_str, hands_free, paused, elapsed);
        tray::update_icon(&self.app, state_str);
    }

    /// Emit an error event to the frontend.
    fn emit_error(&self, message: &str) {
        let _ = self.app.emit(
            "error:show",
            ErrorPayload {
                message: message.to_string(),
            },
        );
        #[cfg(target_os = "macos")]
        crate::sidecar::send(&crate::sidecar::OutMessage::Error {
            message: message.to_string(),
        });
        #[cfg(target_os = "windows")]
        {
            let msg = message.to_string();
            crate::win_overlay::update_state(|st| {
                st.mode = "error".into();
                st.error = Some(msg);
            });
        }
    }

    /// Emit onboarding step change to the frontend.
    fn emit_onboarding(&self) {
        let step_str = self
            .onboarding_step
            .as_ref()
            .map(|s| s.to_str().to_string())
            .unwrap_or_default();
        let text = self
            .onboarding_step
            .as_ref()
            .map(|s| onboarding_text(s, &self.hotkey_label))
            .unwrap_or_default();

        let _ = self.app.emit(
            "onboarding:step",
            OnboardingPayload {
                step: step_str.clone(),
                text: text.clone(),
                hotkey_label: self.hotkey_label.clone(),
            },
        );

        #[cfg(target_os = "macos")]
        crate::sidecar::send(&crate::sidecar::OutMessage::Onboarding {
            step: step_str.clone(),
            text,
            hotkey_label: self.hotkey_label.clone(),
        });

        #[cfg(target_os = "windows")]
        {
            let step = crate::win_overlay::OnboardingStep::from_str(&step_str);
            let label = self.hotkey_label.clone();
            crate::win_overlay::update_state(|st| {
                st.onboarding_step = step;
                st.hotkey_label = label;
            });
        }
    }

    /// The effective onboarding step for input gating.
    /// When a transient tip is showing, returns the step that was active
    /// before the tip, so input restrictions from the parent step apply.
    fn effective_step(&self) -> Option<&OnboardingStep> {
        self.pre_tip_step.as_ref().or(self.onboarding_step.as_ref())
    }
}

// ---------------------------------------------------------------------------
// Thread-safe wrapper
// ---------------------------------------------------------------------------

/// `Orchestrator` is the single owner of pipeline state. It is stored in
/// Tauri's managed state as `Arc<Orchestrator>` so that commands, hotkey
/// callbacks, and audio callbacks can all reach it.
pub struct Orchestrator {
    inner: Mutex<OrchestratorInner>,
}

impl Orchestrator {
    /// Create a new orchestrator and bind it to the given `AppHandle`.
    fn new(app: AppHandle, cfg: &AppConfig) -> Self {
        let hotkey_label = hotkey_display_label(&cfg.hotkey);
        Self {
            inner: Mutex::new(OrchestratorInner {
                state: AppState::Idle,
                enabled: true,
                peak_level: 0.0,
                recording_start: None,
                recording_elapsed_before_pause: Duration::ZERO,
                recording_generation: 0,
                ignore_pending_key_up: false,
                app,
                onboarding_step: None,
                onboarding_complete: cfg.onboarding_complete,
                pre_tip_step: None,
                nice_context: None,
                hold_confirm_start: None,
                hotkey_label,
            }),
        }
    }

    // -- Snapshot helpers (lock briefly, copy, release) --------------------

    pub fn state(&self) -> AppState {
        self.inner.lock().unwrap().state
    }

    fn app_handle(&self) -> AppHandle {
        self.inner.lock().unwrap().app.clone()
    }

    fn begin_press_to_record(&self, inner: &mut OrchestratorInner) -> u64 {
        let generation = inner.begin_press_pending();
        let app = inner.app.clone();
        play_sound(&app, SOUND_START_PRESS);
        let orch = app.state::<Arc<Orchestrator>>();
        let orch = Arc::clone(&orch);
        std::thread::spawn(move || {
            std::thread::sleep(HOLD_TO_RECORD_DELAY);
            let should_start = {
                let mut inner = orch.inner.lock().unwrap();
                inner.activate_pending_recording(generation)
            };
            if should_start {
                orch.start_audio_capture(generation);
            }
        });
        generation
    }

    // -- Key event handlers -----------------------------------------------

    /// Called by the hotkey module when the modifier key is pressed.
    pub fn on_key_down(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled {
            return;
        }

        // In hands-free mode, pressing the hotkey stops the recording
        if inner.state == AppState::HandsFreeRecording || inner.state == AppState::HandsFreePaused {
            drop(inner);
            self.stop_hands_free_internal();
            return;
        }

        if inner.state != AppState::Idle {
            return;
        }

        // -- Onboarding input gating --
        // Use effective_step so restrictions apply even during transient tips.
        if let Some(eff_step) = inner.effective_step().cloned() {
            match eff_step {
                // Click-only and double-tap-only: fn key fully blocked
                OnboardingStep::ClickTip | OnboardingStep::DoubleTapTip => {
                    return;
                }
                // Confirmation steps: fn hold advances onboarding, never records
                OnboardingStep::ApiTip
                | OnboardingStep::FormattingTip
                | OnboardingStep::Welcome => {
                    log::info(&format!("Hold-to-confirm for: {:?}", eff_step));
                    inner.hold_confirm_start = Some(Instant::now());
                    // Emit a "pressed" event so the frontend can show scale-down feedback
                    let _ = inner.app.emit("onboarding:press", true);
                    #[cfg(target_os = "macos")]
                    crate::sidecar::send(&crate::sidecar::OutMessage::OnboardingPress {
                        pressed: true,
                    });
                    #[cfg(target_os = "windows")]
                    crate::win_overlay::update_state(|st| st.is_pressed = true);
                    let app = inner.app.clone();
                    drop(inner);

                    // After 0.6s hold, advance the step
                    let orch = app.state::<Arc<Orchestrator>>();
                    let orch = Arc::clone(&orch);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(600));
                        let mut inner = orch.inner.lock().unwrap();
                        if let Some(start) = inner.hold_confirm_start {
                            if start.elapsed().as_millis() >= 550 {
                                inner.hold_confirm_start = None;
                                let _ = inner.app.emit("onboarding:press", false);
                                #[cfg(target_os = "macos")]
                                crate::sidecar::send(
                                    &crate::sidecar::OutMessage::OnboardingPress { pressed: false },
                                );
                                #[cfg(target_os = "windows")]
                                crate::win_overlay::update_state(|st| st.is_pressed = false);
                                play_sound(&inner.app, SOUND_ERROR);
                                drop(inner);
                                // Small delay then advance
                                std::thread::sleep(std::time::Duration::from_millis(400));
                                orch.advance_onboarding_step();
                            }
                        }
                    });
                    return;
                }
                // .tryIt and others: dismiss any tip overlay, then record normally
                _ => {
                    drop(inner);
                    self.dismiss_transient_tip();
                    // Re-acquire lock after dismiss
                    let mut inner = self.inner.lock().unwrap();
                    if !inner.enabled || inner.state != AppState::Idle {
                        return;
                    }
                    self.begin_press_to_record(&mut inner);
                    return;
                }
            }
        }

        self.begin_press_to_record(&mut inner);
    }

    /// Internal helper to start audio capture (shared by on_key_down paths).
    fn start_audio_capture(&self, _generation: u64) {
        match start_configured_recording() {
            Ok(_path) => {
                log::info("Recording started");
            }
            Err(e) => {
                log::info(&format!("Failed to start recording: {e}"));
                let mut inner = self.inner.lock().unwrap();
                inner.state = AppState::Idle;
                inner.emit_error(&format!("Recording failed: {e}"));
                drop(inner);
                self.restore_onboarding_if_needed();
            }
        }
    }

    /// Called by the hotkey module when the modifier key is released.
    pub fn on_key_up(&self) {
        let mut inner = self.inner.lock().unwrap();

        // Handle onboarding hold-to-confirm (released too early)
        if inner.hold_confirm_start.is_some() {
            inner.hold_confirm_start = None;
            let _ = inner.app.emit("onboarding:press", false);
            #[cfg(target_os = "macos")]
            crate::sidecar::send(&crate::sidecar::OutMessage::OnboardingPress { pressed: false });
            #[cfg(target_os = "windows")]
            crate::win_overlay::update_state(|st| st.is_pressed = false);
            return;
        }

        if inner.state == AppState::PressPending {
            log::info("Quick tap -- waiting for possible double-tap");
            let app = inner.app.clone();
            let generation = inner.recording_generation;
            inner.state = AppState::TapPending;
            inner.emit_state();
            drop(inner);

            let orch = app.state::<Arc<Orchestrator>>();
            let orch = Arc::clone(&orch);
            std::thread::spawn(move || {
                std::thread::sleep(SHORT_TAP_TIP_GRACE);
                let should_show_tip = {
                    let inner = orch.inner.lock().unwrap();
                    inner.state == AppState::TapPending && inner.recording_generation == generation
                };
                if should_show_tip {
                    log::info("Quick tap -- showing holdTip");
                    orch.show_tip(OnboardingStep::HoldTip);
                }
            });
            return;
        }

        // In hands-free mode, key-up is always ignored. Hands-free recording
        // can only be stopped via the dedicated stop button or pill click.
        // The ignore_pending_key_up flag handles the initial key-up from the
        // double-tap that entered hands-free mode.
        if inner.state == AppState::HandsFreeRecording || inner.state == AppState::HandsFreePaused {
            inner.ignore_pending_key_up = false;
            return;
        }

        if inner.state != AppState::Recording {
            return;
        }

        let duration = inner.recording_elapsed().as_secs_f64();
        let peak = inner.peak_level;
        drop(inner);

        log::info(&format!(
            "Key up -- duration: {:.1}s, peak: {:.3}",
            duration, peak
        ));

        // Too-short tap with low peak: show holdTip
        if duration < 0.5 && peak < 0.15 {
            log::info("Too short / quiet -- waiting for possible double-tap");
            let _ = audio::stop_recording(); // discard
            let (app, generation) = {
                let mut inner = self.inner.lock().unwrap();
                inner.state = AppState::TapPending;
                inner.emit_state();
                (inner.app.clone(), inner.recording_generation)
            };

            let orch = app.state::<Arc<Orchestrator>>();
            let orch = Arc::clone(&orch);
            std::thread::spawn(move || {
                std::thread::sleep(SHORT_TAP_TIP_GRACE);
                let should_show_tip = {
                    let inner = orch.inner.lock().unwrap();
                    inner.state == AppState::TapPending && inner.recording_generation == generation
                };
                if should_show_tip {
                    log::info("Too short / quiet -- showing holdTip");
                    orch.show_tip(OnboardingStep::HoldTip);
                }
            });
            return;
        }

        self.stop_and_process();
    }

    /// Called by the hotkey module on a double-tap of the modifier key.
    pub fn on_double_tap(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled {
            return;
        }

        // Onboarding gating: only allow double-tap when onboarding is complete
        // or we're on the doubleTapTip step.
        let on_double_tap_tip = inner.effective_step() == Some(&OnboardingStep::DoubleTapTip);
        if !inner.onboarding_complete && !on_double_tap_tip {
            return;
        }

        match inner.state {
            AppState::Recording => {
                // Convert current hold-to-record into hands-free
                let app = inner.app.clone();
                inner.state = AppState::HandsFreeRecording;
                inner.ignore_pending_key_up = true;
                inner.emit_state();
                drop(inner);
                play_sound(&app, SOUND_HANDS_FREE);
                log::info("Converted to hands-free recording");
            }
            AppState::Idle | AppState::PressPending | AppState::TapPending => {
                let should_play_start_sound = inner.state == AppState::Idle;
                // Dismiss any transient tip overlay before starting
                if on_double_tap_tip {
                    drop(inner);
                    self.dismiss_transient_tip();
                    let mut inner = self.inner.lock().unwrap();
                    if !inner.enabled {
                        return;
                    }
                    inner.begin_recording();
                    drop(inner);
                } else {
                    inner.begin_recording();
                    drop(inner);
                }

                if should_play_start_sound {
                    play_sound(&self.app_handle(), SOUND_START_PRESS);
                }
                match start_configured_recording() {
                    Ok(_) => {
                        let mut inner = self.inner.lock().unwrap();
                        let app = inner.app.clone();
                        inner.state = AppState::HandsFreeRecording;
                        inner.ignore_pending_key_up = true;
                        inner.emit_state();
                        drop(inner);
                        play_sound(&app, SOUND_HANDS_FREE);
                        log::info("Hands-free recording started");
                    }
                    Err(e) => {
                        log::info(&format!("Failed to start recording: {e}"));
                        let mut inner = self.inner.lock().unwrap();
                        inner.state = AppState::Idle;
                        inner.emit_error(&format!("Recording failed: {e}"));
                    }
                }
            }
            _ => {}
        }
    }

    // -- Hands-free controls -----------------------------------------------

    /// Toggle pause/resume in hands-free mode.
    pub fn toggle_pause(&self) {
        let mut inner = self.inner.lock().unwrap();
        match inner.state {
            AppState::HandsFreeRecording => {
                audio::pause_recording();
                inner.pause_recording_clock();
                inner.state = AppState::HandsFreePaused;
                inner.emit_state();
                log::info("Hands-free: paused");
            }
            AppState::HandsFreePaused => {
                audio::resume_recording();
                inner.resume_recording_clock();
                inner.state = AppState::HandsFreeRecording;
                inner.emit_state();
                log::info("Hands-free: resumed");
            }
            _ => {}
        }
    }

    /// Stop hands-free recording and process the audio.
    pub fn stop_hands_free(&self) {
        self.stop_hands_free_internal();
    }

    fn stop_hands_free_internal(&self) {
        let state = self.state();
        if state != AppState::HandsFreeRecording && state != AppState::HandsFreePaused {
            return;
        }
        log::info("Hands-free: stopping");
        self.stop_and_process();
    }

    // -- Pill click handler (from frontend IPC) ----------------------------

    /// Called when the user clicks the overlay pill.
    pub fn on_pill_click(&self) {
        let mut inner = self.inner.lock().unwrap();

        if !inner.enabled {
            return;
        }

        // Always allow clicks that control an active recording (stop hands-free,
        // convert hold-to-record → hands-free). Only gate clicks that START a
        // new recording from idle.
        let is_active = matches!(
            inner.state,
            AppState::Recording
                | AppState::PressPending
                | AppState::TapPending
                | AppState::HandsFreeRecording
                | AppState::HandsFreePaused
        );

        if !is_active {
            // Onboarding gating: only allow starting a new recording via click
            // when onboarding is complete or we're on the clickTip step.
            let on_click_tip = inner.effective_step() == Some(&OnboardingStep::ClickTip);
            if !inner.onboarding_complete && !on_click_tip {
                return;
            }
        }

        match inner.state {
            AppState::Idle => {
                // Start click-to-record (hands-free)
                inner.begin_recording();
                drop(inner);

                match start_configured_recording() {
                    Ok(_) => {
                        let mut inner = self.inner.lock().unwrap();
                        let app = inner.app.clone();
                        inner.state = AppState::HandsFreeRecording;
                        inner.ignore_pending_key_up = false;
                        inner.emit_state();
                        drop(inner);
                        play_sound(&app, SOUND_HANDS_FREE);
                        log::info("Pill click: hands-free recording started");
                    }
                    Err(e) => {
                        log::info(&format!("Pill click: failed to start recording: {e}"));
                        let mut inner = self.inner.lock().unwrap();
                        inner.state = AppState::Idle;
                        inner.emit_error(&format!("Recording failed: {e}"));
                    }
                }
            }
            AppState::Recording => {
                // Convert hold-to-record into hands-free
                let app = inner.app.clone();
                inner.state = AppState::HandsFreeRecording;
                inner.ignore_pending_key_up = true;
                inner.emit_state();
                drop(inner);
                play_sound(&app, SOUND_HANDS_FREE);
                log::info("Pill click: converted to hands-free");
            }
            AppState::PressPending => {
                // Ignore while waiting to decide whether this press is a tap or hold.
            }
            AppState::TapPending => {
                // Ignore while a short tap is waiting to become a double-tap or a hold tip.
            }
            AppState::HandsFreeRecording | AppState::HandsFreePaused => {
                // Ignore pill body clicks in hands-free mode.
                // Only the dedicated stop/pause buttons control the session.
            }
            AppState::Processing => {
                // Ignore clicks during processing
            }
        }
    }

    // -- Settings change handler ------------------------------------------

    /// Called when settings are saved. Reloads config and restarts the hotkey
    /// listener with the new modifier.
    pub fn on_settings_changed(&self) {
        log::info("Settings changed -- reloading");
        let cfg = config::get();

        // Restart hotkey with the configured shortcut.
        hotkey::stop();
        let spec = parse_hotkey_spec(&cfg.hotkey);
        let app = self.app_handle();
        let orch = app.state::<Arc<Orchestrator>>();
        start_hotkey_listener(Arc::clone(&orch), spec);

        // Update hotkey label for onboarding cards
        {
            let mut inner = self.inner.lock().unwrap();
            inner.hotkey_label = hotkey_display_label(&cfg.hotkey);
            // Re-emit onboarding if active (to update keycap labels)
            if inner.onboarding_step.is_some() {
                inner.emit_onboarding();
            }
        }

        // Push appearance settings to the overlay
        {
            let inner = self.inner.lock().unwrap();
            let _ = inner.app.emit(
                "gradient:toggle",
                serde_json::json!({
                    "enabled": cfg.gradient_enabled,
                }),
            );
            let _ = inner.app.emit(
                "overlay:visibility",
                serde_json::json!({
                    "visible": cfg.always_visible_pill,
                }),
            );
            let _ = inner.app.emit("settings:changed", ());

            #[cfg(target_os = "macos")]
            crate::sidecar::send(&crate::sidecar::OutMessage::Config {
                gradient_enabled: cfg.gradient_enabled,
                always_visible: cfg.always_visible_pill,
                hotkey_label: inner.hotkey_label.clone(),
            });

            #[cfg(target_os = "windows")]
            {
                let ge = cfg.gradient_enabled;
                let av = cfg.always_visible_pill;
                let label = inner.hotkey_label.clone();
                crate::win_overlay::update_state(|st| {
                    st.gradient_enabled = ge;
                    st.always_visible = av;
                    st.hotkey_label = label;
                });
            }
        }
    }

    // -- Enable/disable toggle (from tray) --------------------------------

    pub fn toggle_enabled(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.enabled = !inner.enabled;
        let enabled = inner.enabled;
        log::info(&format!("Enabled: {enabled}"));
        let _ = inner.app.emit("enabled:changed", enabled);
    }

    // -- Audio level callback ---------------------------------------------

    /// Called from the audio level polling loop with fresh levels.
    pub fn on_audio_levels(&self, levels: AudioLevels) {
        let mut inner = self.inner.lock().unwrap();
        if levels.level > inner.peak_level {
            inner.peak_level = levels.level;
        }
        let app = inner.app.clone();
        drop(inner);
        emit_levels_to_renderers(&app, &levels);
    }

    // -- Onboarding flow ---------------------------------------------------

    /// Start onboarding if the user hasn't completed it yet.
    fn start_onboarding_if_needed(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.onboarding_complete {
            return;
        }
        log::info("Starting onboarding");
        inner.onboarding_step = Some(OnboardingStep::TryIt);
        inner.emit_onboarding();
    }

    /// Advance from the current onboarding step to the next.
    fn advance_onboarding_step(&self) {
        let mut inner = self.inner.lock().unwrap();
        let step = inner.onboarding_step.clone();
        log::info(&format!("Advancing onboarding from: {:?}", step));

        match step.as_ref() {
            Some(OnboardingStep::DoubleTapTip) => {
                // Fallback; normally double-tap triggers click -> apiTip
                inner.onboarding_step = Some(OnboardingStep::ApiTip);
                inner.emit_onboarding();
            }
            Some(OnboardingStep::ClickTip) => {
                inner.onboarding_step = Some(OnboardingStep::ApiTip);
                inner.emit_onboarding();
            }
            Some(OnboardingStep::ApiTip) => {
                inner.onboarding_step = Some(OnboardingStep::FormattingTip);
                inner.emit_onboarding();
            }
            Some(OnboardingStep::FormattingTip) => {
                inner.onboarding_step = Some(OnboardingStep::Welcome);
                inner.emit_onboarding();
            }
            Some(OnboardingStep::Welcome) => {
                drop(inner);
                self.finalize_onboarding();
            }
            _ => {
                drop(inner);
                self.finalize_onboarding();
            }
        }
    }

    /// Mark onboarding as complete and persist to config.
    fn finalize_onboarding(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.onboarding_complete = true;
            inner.onboarding_step = None;
            inner.pre_tip_step = None;
            inner.nice_context = None;
            inner.emit_onboarding();
        }
        let _ = config::update(|cfg| {
            cfg.onboarding_complete = true;
        });
        log::info("Onboarding finalized");
    }

    /// Show a transient tip (speakTip or holdTip). These auto-dismiss after 2.5s.
    fn show_tip(&self, tip: OnboardingStep) {
        // Capture the current step before overwriting
        let mut inner = self.inner.lock().unwrap();
        let pre_tip = inner.onboarding_step.clone();
        inner.pre_tip_step = pre_tip.clone();
        inner.state = AppState::Idle;
        inner.onboarding_step = Some(tip.clone());
        inner.emit_state();
        if tip.overlay_mode() != "idle" {
            inner.emit_overlay_state(tip.overlay_mode(), None, None, None);
        }
        inner.emit_onboarding();

        let app = inner.app.clone();
        let onboarding_complete = inner.onboarding_complete;
        drop(inner);

        play_sound(&app, SOUND_ERROR);

        // Auto-dismiss after 2.5s
        let orch = app.state::<Arc<Orchestrator>>();
        let orch = Arc::clone(&orch);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let mut inner = orch.inner.lock().unwrap();
            // Only dismiss if we're still showing this exact tip
            if inner.onboarding_step.as_ref() == Some(&tip) {
                inner.pre_tip_step = None;
                if onboarding_complete {
                    // Post-onboarding: just clear
                    inner.onboarding_step = None;
                    inner.emit_state();
                    inner.emit_onboarding();
                } else {
                    // During onboarding: restore to the pre-tip step
                    let restore_to = match pre_tip.as_ref() {
                        Some(OnboardingStep::ClickTip) => OnboardingStep::ClickTip,
                        Some(OnboardingStep::DoubleTapTip) => OnboardingStep::DoubleTapTip,
                        _ => OnboardingStep::TryIt,
                    };
                    inner.onboarding_step = Some(restore_to);
                    inner.emit_state();
                    inner.emit_onboarding();
                }
            }
        });
    }

    /// Dismiss any currently-showing transient tip and restore the previous step.
    fn dismiss_transient_tip(&self) {
        let mut inner = self.inner.lock().unwrap();
        let is_tip = inner
            .onboarding_step
            .as_ref()
            .is_some_and(OnboardingStep::is_transient_tip);
        if !is_tip {
            return;
        }
        if inner.onboarding_complete {
            inner.onboarding_step = None;
        } else {
            let restore = inner.pre_tip_step.clone().unwrap_or(OnboardingStep::TryIt);
            inner.onboarding_step = Some(restore);
        }
        inner.pre_tip_step = None;
        inner.emit_state();
        inner.emit_onboarding();
    }

    /// After a successful transcription+paste during onboarding, show the
    /// "nice" celebration and then advance to the next step.
    fn on_successful_paste_onboarding(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.onboarding_complete {
            return;
        }

        let next_step = match inner.onboarding_step.as_ref() {
            Some(OnboardingStep::TryIt) => Some(OnboardingStep::DoubleTapTip),
            Some(OnboardingStep::DoubleTapTip) => Some(OnboardingStep::ClickTip),
            Some(OnboardingStep::ClickTip) => Some(OnboardingStep::ApiTip),
            _ => None,
        };

        if let Some(next) = next_step {
            inner.nice_context = Some(NiceContext { next_step: next });
            inner.onboarding_step = Some(OnboardingStep::Nice);
            inner.emit_onboarding();
            play_sound(&inner.app, SOUND_SUCCESS);

            let app = inner.app.clone();
            drop(inner);

            // After 1.5s, advance from nice to the next step
            let orch = app.state::<Arc<Orchestrator>>();
            let orch = Arc::clone(&orch);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let mut inner = orch.inner.lock().unwrap();
                if inner.onboarding_step == Some(OnboardingStep::Nice) {
                    if let Some(ctx) = inner.nice_context.take() {
                        inner.onboarding_step = Some(ctx.next_step);
                        inner.emit_onboarding();
                    }
                }
            });
        }
    }

    /// Restore onboarding card after an error auto-dismisses or processing finishes.
    fn restore_onboarding_if_needed(&self) {
        let inner = self.inner.lock().unwrap();
        if inner.onboarding_complete {
            return;
        }
        let step = inner.onboarding_step.clone();
        let app = inner.app.clone();
        drop(inner);

        // Only restore if we're in a step that should bounce back
        let restore_to = match step.as_ref() {
            Some(OnboardingStep::TryIt)
            | Some(OnboardingStep::SpeakTip)
            | Some(OnboardingStep::HoldTip) => Some(OnboardingStep::TryIt),
            Some(OnboardingStep::ClickTip) => Some(OnboardingStep::ClickTip),
            Some(OnboardingStep::DoubleTapTip) => Some(OnboardingStep::DoubleTapTip),
            _ => None,
        };

        if let Some(target) = restore_to {
            let orch = app.state::<Arc<Orchestrator>>();
            let orch = Arc::clone(&orch);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let mut inner = orch.inner.lock().unwrap();
                inner.onboarding_step = Some(target);
                inner.emit_onboarding();
            });
        }
    }

    // -- Core pipeline (stop recording, transcribe, format, paste) --------

    fn stop_and_process(&self) {
        hotkey::clear_tap_sequence();

        // Stop the audio recorder and get the WAV path
        let wav_path = match audio::stop_recording() {
            Ok(p) => p,
            Err(e) => {
                log::info(&format!("Stop recording failed: {e}"));
                let mut inner = self.inner.lock().unwrap();
                inner.state = AppState::Idle;
                inner.emit_state();
                return;
            }
        };

        let peak = {
            let inner = self.inner.lock().unwrap();
            inner.peak_level
        };

        // Silence check -- use the existing no-speech card above the pill.
        if peak < 0.15 {
            log::info(&format!("Silence detected (peak {:.3}) -- skipping", peak));
            self.show_tip(OnboardingStep::SpeakTip);
            return;
        }

        let cfg = config::get();

        // Pre-flight: API providers require an API key. On-device (None) doesn't.
        if cfg.tx_provider != TranscriptionProvider::None && cfg.tx_api_key.is_empty() {
            log::info("No API key configured for transcription provider");
            let app = self.app_handle();
            play_sound(&app, SOUND_ERROR);
            let mut inner = self.inner.lock().unwrap();
            inner.state = AppState::Idle;
            inner.emit_error("Set up an API key in Settings");
            drop(inner);
            // Auto-open settings window
            let _ = windows::show_app_window(&app, "settings");
            return;
        }

        // Transition to processing
        let app = {
            let mut inner = self.inner.lock().unwrap();
            inner.state = AppState::Processing;
            inner.emit_state();
            inner.app.clone()
        };
        play_sound(&app, SOUND_LOADING);

        // Spawn the async transcription/formatting pipeline
        let orch = app.state::<Arc<Orchestrator>>();
        let orch = Arc::clone(&orch);

        tauri::async_runtime::spawn(async move {
            let result = process_audio_pipeline(&wav_path, &cfg).await;
            match result {
                Ok(text) => {
                    if text.is_empty() {
                        log::info("Pipeline completed without transcription text");
                        orch.show_tip(OnboardingStep::SpeakTip);
                        return;
                    }

                    if !text.is_empty() {
                        // Add to history
                        let tx_provider = format!("{:?}", cfg.tx_provider).to_lowercase();
                        let fmt_provider = if cfg.fmt_provider != FormattingProvider::None {
                            Some(format!("{:?}", cfg.fmt_provider).to_lowercase())
                        } else {
                            None
                        };
                        let fmt_style = if cfg.fmt_provider != FormattingProvider::None {
                            Some(format!("{:?}", cfg.fmt_style).to_lowercase())
                        } else {
                            None
                        };
                        let _ = history::append(text.clone(), tx_provider, fmt_provider, fmt_style);

                        // Refresh tray history menu
                        let app = orch.app_handle();
                        tray::refresh_history_menu(&app);

                        // Paste the result
                        if let Err(e) = paste::paste_text(&text) {
                            log::info(&format!("Paste failed: {e}"));
                        }
                    }

                    // Return to idle
                    {
                        let mut inner = orch.inner.lock().unwrap();
                        inner.state = AppState::Idle;
                        inner.emit_state();
                    }

                    // Trigger onboarding celebration + advancement if applicable
                    orch.on_successful_paste_onboarding();
                }
                Err(e) => {
                    log::info(&format!("Pipeline error: {e}"));
                    if is_no_speech_error(&e) {
                        orch.show_tip(OnboardingStep::SpeakTip);
                        return;
                    }
                    play_sound(&orch.app_handle(), SOUND_ERROR);
                    // Set internal state to idle but show error to frontend.
                    // Frontend auto-dismisses the error back to idle after 2s.
                    let mut inner = orch.inner.lock().unwrap();
                    inner.state = AppState::Idle;
                    inner.emit_error(&classify_error(&e));
                    drop(inner);
                    // After error auto-dismisses, restore onboarding
                    let orch2 = Arc::clone(&orch);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(2500));
                        orch2.restore_onboarding_if_needed();
                    });
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Async pipeline: transcribe -> format -> return text
// ---------------------------------------------------------------------------

fn start_configured_recording() -> Result<PathBuf, String> {
    let cfg = config::get();
    let device = cfg.audio_device.trim();
    audio::start_recording((!device.is_empty()).then_some(device))
}

async fn process_audio_pipeline(wav_path: &PathBuf, cfg: &AppConfig) -> Result<String, String> {
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

    // Resolve formatting API key (fall back to tx_api_key if empty)
    let fmt_api_key = if cfg.fmt_api_key.is_empty() {
        cfg.tx_api_key.clone()
    } else {
        cfg.fmt_api_key.clone()
    };

    // Check for Gemini one-shot optimization: when both transcription and
    // formatting use Gemini, a single API call handles both.
    let use_oneshot = cfg.tx_provider == TranscriptionProvider::Gemini
        && cfg.fmt_provider == FormattingProvider::Gemini
        && cfg.tx_provider.can_also_format();

    // Apple Speech pre-check: before calling expensive API providers, run a
    // quick on-device check to confirm speech exists in the audio. This saves
    // API costs on silence/noise that slipped past the peak-level gate.
    if cfg.tx_provider != TranscriptionProvider::None {
        let check_path = wav_path.clone();
        let has_speech = tokio::task::spawn_blocking(move || crate::speech::pre_check(&check_path))
            .await
            .unwrap_or(true); // if the task panics, proceed anyway

        if !has_speech {
            return Err("No speech detected".to_string());
        }
    }

    let raw_text = if use_oneshot {
        log::info("One-shot: Gemini transcribe+format");
        transcription::transcribe_gemini_oneshot(wav_path, &tx_options, cfg.fmt_style).await?
    } else {
        // Standard two-step: transcribe, then optionally format
        log::info(&format!("Transcribing with {:?}", cfg.tx_provider));
        transcription::transcribe(cfg.tx_provider, wav_path, &tx_options).await?
    };

    let trimmed = raw_text.trim().to_string();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // Prompt regurgitation guard
    let lower = trimmed.to_lowercase();
    if lower.contains("transcribe this audio")
        || lower.contains("respond with only a json")
        || lower.contains("dictation commands")
    {
        log::info("Discarded -- model regurgitated the prompt");
        return Ok(String::new());
    }

    // If we used one-shot, text is already formatted
    if use_oneshot {
        return Ok(trimmed);
    }

    // Format if a provider is configured
    if cfg.fmt_provider != FormattingProvider::None && !fmt_api_key.is_empty() {
        log::info(&format!("Formatting with {:?}", cfg.fmt_provider));
        let fmt_options = FormattingOptions {
            api_key: fmt_api_key,
            model: cfg.fmt_model.clone(),
            style: cfg.fmt_style,
        };
        let formatted = formatting::format(cfg.fmt_provider, &trimmed, &fmt_options).await?;

        // Check formatted text for prompt regurgitation too
        let fl = formatted.to_lowercase();
        if fl.contains("transcribe this audio")
            || fl.contains("respond with only a json")
            || fl.contains("dictation commands")
        {
            log::info("Discarded formatted text -- prompt regurgitation");
            return Ok(trimmed);
        }

        Ok(formatted)
    } else {
        Ok(trimmed)
    }
}

// ---------------------------------------------------------------------------
// Error classification (maps API errors to user-friendly messages)
// ---------------------------------------------------------------------------

fn classify_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("quota") || lower.contains("rate") || lower.contains("429") {
        "Rate limited -- try again".to_string()
    } else if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("authentication")
        || lower.contains("authorization")
        || lower.contains("invalid api key")
        || lower.contains("api key invalid")
        || lower.contains("missing api key")
        || lower.contains("invalid token")
    {
        "Invalid API key".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "Request timed out".to_string()
    } else if lower.contains("offline") || lower.contains("network") || lower.contains("internet") {
        "No internet connection".to_string()
    } else {
        "Something went wrong".to_string()
    }
}

fn is_no_speech_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("no speech") || lower.contains("no transcription")
}

// ---------------------------------------------------------------------------
// Sound effects
// ---------------------------------------------------------------------------

pub(crate) fn play_sound(app: &AppHandle, name: &str) {
    let cfg = config::get();
    if !cfg.sounds_enabled {
        return;
    }

    // Attempt to load the sound from the app's resource directory
    let resource_path = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("sounds").join(format!("{name}.wav")));

    if let Some(path) = resource_path {
        if path.exists() {
            std::thread::spawn(move || {
                if let Ok((_stream, stream_handle)) = rodio::OutputStream::try_default() {
                    if let Ok(file) = std::fs::File::open(&path) {
                        let source = rodio::Decoder::new(std::io::BufReader::new(file));
                        if let Ok(source) = source {
                            let _ = stream_handle
                                .play_raw(rodio::source::Source::convert_samples(source));
                            // Keep thread alive while audio plays
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Hotkey modifier parsing
// ---------------------------------------------------------------------------

fn parse_hotkey_spec(hotkey: &str) -> HotkeySpec {
    HotkeySpec::parse(hotkey)
}

fn hotkey_display_label(hotkey: &str) -> String {
    parse_hotkey_spec(hotkey).label()
}

// ---------------------------------------------------------------------------
// Hotkey listener setup (connects hotkey callbacks to orchestrator)
// ---------------------------------------------------------------------------

fn start_hotkey_listener(orch: Arc<Orchestrator>, spec: HotkeySpec) {
    let orch_down = Arc::clone(&orch);
    let orch_up = Arc::clone(&orch);
    let orch_double = Arc::clone(&orch);

    hotkey::set_callbacks(
        move || orch_down.on_key_down(),
        move || orch_up.on_key_up(),
        move || orch_double.on_double_tap(),
    );

    hotkey::start(spec);
}

// ---------------------------------------------------------------------------
// Audio level polling loop
// ---------------------------------------------------------------------------

/// Spawn a background thread that polls audio levels every ~33ms and
/// forwards them to the orchestrator (which emits to the frontend).
/// Also emits elapsed time once per second for the timer display.
fn start_level_poller(orch: Arc<Orchestrator>) {
    std::thread::Builder::new()
        .name("yap-level-poller".into())
        .spawn(move || {
            let mut tick_count: u32 = 0;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(33));
                let state = orch.state();
                if state == AppState::Recording
                    || state == AppState::HandsFreeRecording
                    || state == AppState::HandsFreePaused
                {
                    let levels = audio::get_levels();
                    orch.on_audio_levels(levels);

                    // Emit state (with elapsed time) roughly once per second
                    // for the timer display (every ~30 ticks at 33ms = ~990ms)
                    tick_count += 1;
                    if tick_count % 30 == 0 {
                        let inner = orch.inner.lock().unwrap();
                        inner.emit_state();
                    }
                } else {
                    tick_count = 0;
                }
            }
        })
        .expect("failed to spawn level poller thread");
}

// ---------------------------------------------------------------------------
// Initialization (called from lib.rs setup)
// ---------------------------------------------------------------------------

/// Initialize the orchestrator and wire up all modules.
///
/// This is the main entry point called from `lib.rs` during app setup.
/// It loads configuration, starts the hotkey listener, sets up the tray,
/// and begins the audio level polling loop.
pub fn init(app: &AppHandle) {
    // Load config
    let _ = config::load();
    let cfg = config::get();

    // Create orchestrator and store as managed state
    let orch = Arc::new(Orchestrator::new(app.clone(), &cfg));
    app.manage(Arc::clone(&orch));

    // Set up system tray
    let _ = tray::setup_tray(app);

    // Start hotkey listener
    let spec = parse_hotkey_spec(&cfg.hotkey);
    log::info(&format!("Starting hotkey: {}", spec.label()));
    start_hotkey_listener(Arc::clone(&orch), spec);

    // Start audio level poller
    start_level_poller(Arc::clone(&orch));

    // -- Overlay setup: native sidecar on macOS, WebView on Windows --

    #[cfg(target_os = "macos")]
    {
        // Close the WebView overlay — macOS uses the native sidecar instead.
        // The window is defined in tauri.conf.json (needed for Windows) but
        // we don't need it on macOS.
        if let Some(overlay) = app.get_webview_window("overlay") {
            let _ = overlay.destroy();
        }

        // Spawn native Swift overlay sidecar (NSPanel + SwiftUI)
        crate::sidecar::spawn(app);

        // The sidecar sends "ready" once it's initialized.
        // Start onboarding after a short delay to ensure it's up.
        let orch2 = Arc::clone(&orch);
        std::thread::Builder::new()
            .name("yap-sidecar-wait".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                log::info("Sidecar overlay assumed ready -- starting onboarding check");
                orch2.start_onboarding_if_needed();
            })
            .ok();
    }

    #[cfg(target_os = "windows")]
    {
        // Destroy the WebView overlay — Windows uses a native Win32 pill instead
        if let Some(overlay) = app.get_webview_window("overlay") {
            let _ = overlay.destroy();
        }

        // Spawn native Win32 overlay (layered window + tiny-skia rendering)
        crate::win_overlay::spawn(app);

        let orch2 = Arc::clone(&orch);
        std::thread::Builder::new()
            .name("yap-win-overlay-wait".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                log::info("Win32 overlay ready -- starting onboarding check");
                orch2.start_onboarding_if_needed();
            })
            .ok();
    }

    log::info("Orchestrator initialized -- ready");
}

// ---------------------------------------------------------------------------
// Simple logging helper (writes to ~/.config/yap/debug.log)
// ---------------------------------------------------------------------------

pub(crate) mod log {
    use std::io::Write;

    pub fn info(message: &str) {
        eprintln!("[yap] {message}");

        // Also append to the debug log file
        if let Ok(dir) = crate::config::config_dir() {
            let path = dir.join("debug.log");
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let timestamp = chrono::Utc::now().to_rfc3339();
                let _ = writeln!(file, "[{timestamp}] {message}");
            }
        }
    }
}
