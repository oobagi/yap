using System;
using System.Threading.Tasks;
using System.Windows;
using Yap.Audio;
using Yap.Formatting;
using Yap.Input;
using Yap.Models;
using Yap.Onboarding;
using Yap.Transcription;
using Yap.UI;
using Application = System.Windows.Application;

namespace Yap.Core
{
    /// <summary>
    /// Central orchestrator. Owns the state machine and coordinates the full pipeline:
    ///   hotkey -> start recording -> stop recording -> check duration/silence -> transcribe -> format -> paste.
    ///
    /// Direct port of AppDelegate from the macOS version.
    ///
    /// State machine: Idle -> Recording -> Processing -> Idle
    /// Also supports: HandsFreeRecording, HandsFreePaused
    /// </summary>
    public class AppOrchestrator
    {
        // State
        private AppState _state = AppState.Idle;
        private DateTime _recordingStart;
        private float _peakAudioLevel;
        private bool _isEnabled = true;
        private bool _ignorePendingKeyUp;

        // Components
        private readonly AudioRecorder _audioRecorder = new();
        private readonly Input.HotkeyManager _hotkeyManager = new();
        private readonly PasteManager _pasteManager = new();
        private readonly SoundPlayer _soundPlayer = new();
        private readonly WindowsSpeechTranscriber _windowsSpeech = new();
        private readonly OnboardingManager _onboarding = new();

        // API providers (created from config)
        private ITranscriptionProvider? _apiTranscriber;
        private IFormattingProvider? _textFormatter;
        private string _formattingStyle = "formatted";

        // UI
        private OverlayWindow? _overlay;
        private SettingsWindow? _settingsWindow;
        private HistoryWindow? _historyWindow;
        private TrayIcon? _trayIcon;

        /// <summary>Get/set enabled state.</summary>
        public bool IsEnabled
        {
            get => _isEnabled;
            set
            {
                _isEnabled = value;
                Logger.Log($"Enabled: {value}");
            }
        }

        /// <summary>
        /// Wire up a TrayIcon so the orchestrator can update it.
        /// Called by App.xaml.cs after constructing both objects.
        /// </summary>
        public void SetTrayIcon(TrayIcon tray) => _trayIcon = tray;

        /// <summary>
        /// Full initialization: load config, set up hotkey, engines, overlay, sounds.
        /// </summary>
        public void Initialize()
        {
            Logger.Log("Orchestrator: initializing");

            // Create overlay window
            _overlay = new OverlayWindow();
            _overlay.OnClickToRecord = StartClickRecording;
            _overlay.SetAlwaysVisible(Config.Current.AlwaysVisiblePill);
            if (Config.Current.AlwaysVisiblePill)
            {
                _overlay.Show();
            }

            // Set up hotkey
            SetupHotkey();

            // Set up transcription/formatting engines
            SetupEngines();

            // Preload sounds
            _soundPlayer.PreloadSounds();

            // Set up onboarding
            SetupOnboarding();
            _onboarding.StartIfNeeded();
        }

        /// <summary>
        /// Clean shutdown: stop hotkey, dispose resources.
        /// </summary>
        public void Shutdown()
        {
            Logger.Log("Orchestrator: shutting down");
            _hotkeyManager.Stop();
            _audioRecorder.Dispose();
            _soundPlayer.Dispose();
            _hotkeyManager.Dispose();
        }

        // MARK: - Settings

        public void SettingsDidChange()
        {
            Logger.Log("Settings changed, reloading...");
            _hotkeyManager.Stop();
            SetupHotkey();
            SetupEngines();
            _overlay?.SetAlwaysVisible(Config.Current.AlwaysVisiblePill);
            _onboarding.OnSettingsChanged();
        }

        public void OpenSettings()
        {
            if (_settingsWindow == null || !_settingsWindow.IsLoaded)
            {
                _settingsWindow = new SettingsWindow();
                _settingsWindow.SettingsChanged += SettingsDidChange;
            }
            _settingsWindow.Show();
            _settingsWindow.Activate();
        }

        public void OpenHistory()
        {
            if (_historyWindow == null || !_historyWindow.IsLoaded)
            {
                _historyWindow = new HistoryWindow();
            }
            _historyWindow.Show();
            _historyWindow.Activate();
        }

        // MARK: - Hotkey Setup

        private void SetupHotkey()
        {
            var config = Config.Current;
            _hotkeyManager.SetHotkeyFromName(config.Hotkey);

            _hotkeyManager.OnKeyDown = StartRecording;
            _hotkeyManager.OnKeyUp = StopAndTranscribe;
            _hotkeyManager.OnDoubleTap = StartHandsFreeRecording;

            bool started = _hotkeyManager.Start();
            Logger.Log($"Hotkey ({config.Hotkey}): {(started ? "OK" : "FAILED")}");

            if (!started)
            {
                _overlay?.ShowError("Hotkey failed — try running as Administrator");
            }
        }

        // MARK: - Engine Setup

        private void SetupEngines()
        {
            var config = Config.Current;

            // Transcription
            var txProvider = TranscriptionDefaults.FromString(config.TxProvider);
            var txKey = config.TxApiKey;
            var txModel = string.IsNullOrEmpty(config.TxModel) ? null : config.TxModel;

            if (txProvider != TranscriptionProviderType.None && !string.IsNullOrEmpty(txKey))
            {
                _apiTranscriber = txProvider switch
                {
                    TranscriptionProviderType.Gemini => new GeminiTranscriber(txKey, txModel, config.GeminiTemperature),
                    TranscriptionProviderType.OpenAI => new OpenAiTranscriber(txKey, txModel, config.OaiLanguage, config.OaiPrompt),
                    TranscriptionProviderType.Deepgram => new DeepgramTranscriber(
                        txKey, txModel, config.DgSmartFormat, config.DgLanguage,
                        string.IsNullOrEmpty(config.DgKeywords) ? null
                            : config.DgKeywords.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)),
                    TranscriptionProviderType.ElevenLabs => new ElevenLabsTranscriber(txKey, txModel, config.ElLanguageCode),
                    _ => null
                };
                Logger.Log($"Transcription: {config.TxProvider}");
            }
            else
            {
                _apiTranscriber = null;
                Logger.Log("Transcription: Windows Speech");
            }

            // Formatting
            var fmtProvider = FormattingDefaults.FromString(config.FmtProvider);
            _formattingStyle = string.IsNullOrEmpty(config.FmtStyle) ? "formatted" : config.FmtStyle;
            var fmtModel = string.IsNullOrEmpty(config.FmtModel) ? null : config.FmtModel;

            // Resolve formatting API key: use its own, or fall back to transcription key if same provider
            var fmtKey = config.FmtApiKey;
            if (string.IsNullOrEmpty(fmtKey) && config.FmtProvider == config.TxProvider)
            {
                fmtKey = txKey;
            }

            if (fmtProvider != FormattingProviderType.None && !string.IsNullOrEmpty(fmtKey))
            {
                _textFormatter = fmtProvider switch
                {
                    FormattingProviderType.Gemini => new GeminiFormatter(fmtKey, fmtModel, _formattingStyle),
                    FormattingProviderType.OpenAI => new OpenAiFormatter(fmtKey, fmtModel, _formattingStyle),
                    FormattingProviderType.Anthropic => new AnthropicFormatter(fmtKey, fmtModel, _formattingStyle),
                    FormattingProviderType.Groq => new GroqFormatter(fmtKey, fmtModel, _formattingStyle),
                    _ => null
                };
                Logger.Log($"Formatting: {config.FmtProvider} / {_formattingStyle}");
            }
            else
            {
                _textFormatter = null;
                Logger.Log("Formatting: disabled");
            }
        }

        // MARK: - Onboarding Setup

        private void SetupOnboarding()
        {
            _onboarding.OnStepChanged = (step, text) =>
            {
                _overlay?.ShowOnboardingStep(step, text);
            };

            _onboarding.OnComplete = () =>
            {
                _overlay?.HideOnboarding();
            };

            _onboarding.OnPressDown = () =>
            {
                _overlay?.PressDown();
            };

            _onboarding.OnPressRelease = () =>
            {
                _overlay?.PressRelease();
            };

            _onboarding.OnShake = () =>
            {
                _overlay?.Shake();
            };

            _onboarding.OnShowNoSpeech = () =>
            {
                // Don't auto-dismiss; the onboarding manager controls tip timing
                _overlay?.ShowNoSpeech(autoDismiss: false);
            };

            _onboarding.OnPlaySound = (name) =>
            {
                _soundPlayer.Play(name);
            };
        }

        // MARK: - Recording Flow

        private void StartRecording()
        {
            Logger.Log("Key down - starting recording");

            // Check onboarding gating
            if (_onboarding.IsActive)
            {
                var action = _onboarding.HandleKeyDown();
                switch (action)
                {
                    case OnboardingKeyDownAction.Block:
                        Logger.Log("Onboarding: key down blocked");
                        return;
                    case OnboardingKeyDownAction.HoldToConfirm:
                        Logger.Log("Onboarding: hold-to-confirm started");
                        return;
                    case OnboardingKeyDownAction.AllowRecording:
                        // Fall through to normal recording
                        break;
                }
            }

            if (!_isEnabled || _state != AppState.Idle)
            {
                Logger.Log($"Skipped: enabled={_isEnabled} state={_state}");
                return;
            }

            _state = AppState.Recording;
            _recordingStart = DateTime.Now;
            _peakAudioLevel = 0;
            UpdateIcon(AppState.Recording);
            _overlay?.ShowRecording();

            _audioRecorder.OnLevelUpdate = level =>
            {
                _overlay?.UpdateLevel(level);
                if (level > _peakAudioLevel) _peakAudioLevel = level;
            };
            _audioRecorder.OnBandLevels = bands =>
            {
                _overlay?.UpdateBandLevels(bands);
            };

            try
            {
                _audioRecorder.Start();
                // Delay chime so hardware has settled after engine start
                FireAndForget(() => DelayedAction(100, () => _soundPlayer.Play("Blow")));
            }
            catch (Exception ex)
            {
                Logger.Log($"Recording failed: {ex.Message}");
                _state = AppState.Idle;
                UpdateIcon(AppState.Idle);
                _overlay?.Dismiss();
                _onboarding.RestoreIfNeeded();
            }
        }

        private void StopAndTranscribe()
        {
            Logger.Log("Key up - stopping recording");

            // Check onboarding hold-to-confirm
            if (_onboarding.IsActive)
            {
                var action = _onboarding.HandleKeyUp();
                if (action == OnboardingKeyUpAction.Handled)
                {
                    Logger.Log("Onboarding: key up handled (hold-to-confirm cancelled)");
                    return;
                }
            }

            // In hands-free mode, ignore key-up if fn was held when we entered
            if (_state == AppState.HandsFreeRecording || _state == AppState.HandsFreePaused)
            {
                if (_ignorePendingKeyUp)
                {
                    _ignorePendingKeyUp = false;
                    return;
                }
                StopHandsFreeRecording();
                return;
            }

            if (_state != AppState.Recording) return;

            var duration = (DateTime.Now - _recordingStart).TotalSeconds;
            Logger.Log($"Duration: {duration:F1}s, peak: {_peakAudioLevel}");

            // Too short = accidental tap (unless speech was detected via peak level)
            if (duration < 0.5 && _peakAudioLevel < 0.15)
            {
                Logger.Log("Too short - cancelling");
                _audioRecorder.Cancel();
                _soundPlayer.Play("Pop");
                _state = AppState.Idle;
                UpdateIcon(AppState.Idle);
                _overlay?.Dismiss();

                // Show holdTip during onboarding
                if (_onboarding.IsActive)
                {
                    _onboarding.HandleTooShort();
                }
                return;
            }

            _state = AppState.Processing;
            UpdateIcon(AppState.Processing);
            _overlay?.ShowProcessing();

            var audioPath = _audioRecorder.Stop();
            _soundPlayer.Play("Pop");

            if (audioPath == null)
            {
                FinishProcessing();
                return;
            }

            FireAndForget(() => ProcessRecordedAudioAsync(audioPath));
        }

        /// <summary>
        /// Shared audio processing pipeline used by both hold-to-record and hands-free modes.
        /// </summary>
        private async Task ProcessRecordedAudioAsync(string audioPath)
        {
            // Silence check - levels are RMS * 18, so 0.15 is the quiet speech threshold
            if (_peakAudioLevel < 0.15f)
            {
                Logger.Log($"Silence detected (peak {_peakAudioLevel}) - skipping");
                ShowSpeakTipOrError();
                return;
            }

            if (_apiTranscriber != null)
            {
                // Quick pre-check: run Windows Speech to confirm speech exists before API call
                Logger.Log("Pre-check: running Windows Speech to detect speech...");
                bool hasSpeech = await _windowsSpeech.HasSpeechAsync(audioPath);

                if (!hasSpeech)
                {
                    Logger.Log("Pre-check: no speech detected - skipping API call");
                    ShowSpeakTipOrError();
                    return;
                }

                Logger.Log("Pre-check: speech detected, proceeding with API");
                await SendToApiAsync(audioPath);
            }
            else
            {
                // Windows Speech -> optional format
                Logger.Log("Windows Speech transcription");
                var result = await _windowsSpeech.TranscribeAsync(audioPath);

                if (result.Success)
                {
                    await MaybeFormatAsync(result.Text);
                }
                else
                {
                    Logger.Log($"Windows Speech failed: {result.Error?.Message}");
                    ShowSpeakTipOrError();
                }
            }
        }

        /// <summary>
        /// Show the speak tip (during onboarding) or a generic error (post-onboarding).
        /// Resets state to idle in both cases.
        /// </summary>
        private void ShowSpeakTipOrError()
        {
            _state = AppState.Idle;
            UpdateIcon(AppState.Idle);

            if (_onboarding.IsActive)
            {
                _onboarding.HandleSilence();
            }
            else
            {
                ShowError("Didn't catch that - speak up");
            }
        }

        /// <summary>
        /// Send audio to the configured API transcription provider.
        /// </summary>
        private async Task SendToApiAsync(string audioPath)
        {
            // Check for one-shot capability (Gemini as both transcriber and formatter)
            bool canOneShot = _apiTranscriber!.CanAlsoFormat
                && _textFormatter != null
                && _apiTranscriber.ProviderName == _textFormatter.ProviderName;

            if (canOneShot)
            {
                Logger.Log($"One-shot: {_apiTranscriber.ProviderName} transcribe+format");
                var result = await _apiTranscriber.TranscribeAsync(audioPath, _formattingStyle);
                await HandleResultAsync(result);
            }
            else
            {
                Logger.Log($"Two-step: {_apiTranscriber.ProviderName} transcribe -> {_textFormatter?.ProviderName ?? "none"} format");
                var result = await _apiTranscriber.TranscribeAsync(audioPath);

                if (result.Success)
                {
                    await MaybeFormatAsync(result.Text);
                }
                else
                {
                    Logger.Log($"Transcription failed: {result.Error?.Message}");
                    ShowError(GetUserFriendlyError(result.Error));
                }
            }
        }

        /// <summary>
        /// Format text if a formatter is configured, otherwise paste raw.
        /// </summary>
        private async Task MaybeFormatAsync(string text)
        {
            Logger.Log($"Transcription: \"{text}\"");
            var trimmed = text.Trim();

            if (string.IsNullOrEmpty(trimmed))
            {
                FinishProcessing();
                return;
            }

            // Discard prompt regurgitation
            var lower = trimmed.ToLowerInvariant();
            if (lower.Contains("transcribe this audio") ||
                lower.Contains("respond with only a json") ||
                lower.Contains("dictation commands"))
            {
                Logger.Log("[Warning] Discarded - model regurgitated prompt");
                ShowError("Couldn't process - try again");
                return;
            }

            if (_textFormatter != null)
            {
                var result = await _textFormatter.FormatAsync(text);

                if (result.Success)
                {
                    Logger.Log($"Formatted: \"{result.Text}\"");
                    await PasteTextAsync(result.Text);
                }
                else
                {
                    Logger.Log($"Format failed, using raw: {result.Error?.Message}");
                    await PasteTextAsync(text);
                }
                FinishProcessing();
            }
            else
            {
                await PasteTextAsync(text);
                FinishProcessing();
            }
        }

        /// <summary>
        /// Handle a final result from one-shot transcription+formatting.
        /// </summary>
        private async Task HandleResultAsync(TranscriptionResult result)
        {
            if (result.Success)
            {
                Logger.Log($"Result: \"{result.Text}\"");
                var trimmed = result.Text.Trim();

                // Prompt regurgitation guard
                var lower = trimmed.ToLowerInvariant();
                if (lower.Contains("transcribe this audio") ||
                    lower.Contains("respond with only a json") ||
                    lower.Contains("dictation commands"))
                {
                    Logger.Log("[Warning] Discarded - model regurgitated prompt");
                    ShowError("Couldn't process - try again");
                    return;
                }

                if (!string.IsNullOrEmpty(trimmed))
                {
                    await PasteTextAsync(trimmed);
                }
                FinishProcessing();
            }
            else
            {
                Logger.Log($"Failed: {result.Error?.Message}");
                ShowError(GetUserFriendlyError(result.Error));
            }
        }

        /// <summary>Whether the last paste was a successful transcription (for onboarding tracking).</summary>
        private bool _lastPasteWasSuccess;

        private async Task PasteTextAsync(string text)
        {
            var txProvider = _apiTranscriber?.ProviderName ?? "windows_speech";
            var fmtProvider = _textFormatter?.ProviderName;
            var fmtStyle = _textFormatter != null ? _formattingStyle : null;
            HistoryManager.Shared.Append(text, txProvider, fmtProvider, fmtStyle);
            await _pasteManager.PasteAsync(text);
            _lastPasteWasSuccess = true;
        }

        private void FinishProcessing()
        {
            _state = AppState.Idle;
            UpdateIcon(AppState.Idle);
            _overlay?.Dismiss();

            // Notify onboarding of successful transcription AFTER dismissing
            // so that the Nice celebration overlay re-appears on top
            if (_lastPasteWasSuccess && _onboarding.IsActive)
            {
                _lastPasteWasSuccess = false;
                bool celebrated = _onboarding.HandleTranscriptionSuccess();
                if (celebrated)
                {
                    _soundPlayer.Play("Submarine");
                    return; // Don't restore — Nice auto-advances
                }
            }
            _lastPasteWasSuccess = false;

            _onboarding.RestoreIfNeeded();
        }

        // MARK: - Click-to-Record

        private void StartClickRecording()
        {
            if (!_isEnabled) return;

            // Onboarding gating for pill click
            if (_onboarding.IsActive)
            {
                var action = _onboarding.HandlePillClick();
                if (action == OnboardingClickAction.Block)
                {
                    Logger.Log("Onboarding: pill click blocked");
                    return;
                }
                // ClickTip step: dismiss transient tip if showing and proceed
                _onboarding.DismissTransientTip();
            }

            // If already in hold-to-record, convert to hands-free
            if (_state == AppState.Recording)
            {
                Logger.Log("Pill clicked during hold-recording - converting to hands-free");
                StartHandsFreeRecording();
                return;
            }

            // If in hands-free, stop and restart
            if (_state == AppState.HandsFreeRecording || _state == AppState.HandsFreePaused)
            {
                Logger.Log("Pill clicked during hands-free - stopping and restarting");
                _audioRecorder.Cancel();
                _overlay?.ContractHandsFree();
                _state = AppState.Idle;
                UpdateIcon(AppState.Idle);
            }

            if (_state != AppState.Idle)
            {
                Logger.Log($"Skipped pill click: state={_state}");
                return;
            }

            Logger.Log("Pill clicked - starting hands-free recording");

            _state = AppState.Recording;
            _recordingStart = DateTime.Now;
            _peakAudioLevel = 0;
            UpdateIcon(AppState.Recording);
            _overlay?.ShowRecording();

            _audioRecorder.OnLevelUpdate = level =>
            {
                _overlay?.UpdateLevel(level);
                if (level > _peakAudioLevel) _peakAudioLevel = level;
            };
            _audioRecorder.OnBandLevels = bands =>
            {
                _overlay?.UpdateBandLevels(bands);
            };

            try
            {
                _audioRecorder.Start();
                _soundPlayer.Play("Blow");

                // Immediately enter hands-free mode
                _state = AppState.HandsFreeRecording;
                _ignorePendingKeyUp = false;
                _overlay?.ShowHandsFreeRecording(
                    ToggleHandsFreePause,
                    StopHandsFreeRecording);
            }
            catch (Exception ex)
            {
                Logger.Log($"Recording failed: {ex.Message}");
                _state = AppState.Idle;
                UpdateIcon(AppState.Idle);
                _overlay?.Dismiss();
            }
        }

        // MARK: - Hands-Free Recording

        private void StartHandsFreeRecording()
        {
            Logger.Log("Double-tap - entering hands-free mode");

            // Onboarding gating for double-tap
            if (_onboarding.IsActive)
            {
                var action = _onboarding.HandleDoubleTap();
                if (action == OnboardingDoubleTapAction.Block)
                {
                    Logger.Log("Onboarding: double-tap blocked");
                    return;
                }
                // DoubleTapTip step: dismiss transient tip if showing
                _onboarding.DismissTransientTip();
            }

            if (_state == AppState.Recording)
            {
                // Convert hold-to-record to hands-free
                _state = AppState.HandsFreeRecording;
                _ignorePendingKeyUp = _hotkeyManager.IsHeld;
                _overlay?.ShowHandsFreeRecording(
                    ToggleHandsFreePause,
                    StopHandsFreeRecording);
            }
            else if (_state == AppState.Idle)
            {
                // Start fresh hands-free recording
                if (!_isEnabled) return;

                _recordingStart = DateTime.Now;
                _peakAudioLevel = 0;
                UpdateIcon(AppState.Recording);
                _overlay?.ShowRecording();

                _audioRecorder.OnLevelUpdate = level =>
                {
                    _overlay?.UpdateLevel(level);
                    if (level > _peakAudioLevel) _peakAudioLevel = level;
                };
                _audioRecorder.OnBandLevels = bands =>
                {
                    _overlay?.UpdateBandLevels(bands);
                };

                try
                {
                    _audioRecorder.Start();
                    _state = AppState.HandsFreeRecording;
                    _ignorePendingKeyUp = _hotkeyManager.IsHeld;
                    _overlay?.ShowHandsFreeRecording(
                        ToggleHandsFreePause,
                        StopHandsFreeRecording);
                    _soundPlayer.Play("Blow");
                }
                catch (Exception ex)
                {
                    Logger.Log($"Recording failed: {ex.Message}");
                    _state = AppState.Idle;
                    UpdateIcon(AppState.Idle);
                    _overlay?.Dismiss();
                }
            }
        }

        private void ToggleHandsFreePause()
        {
            if (_state == AppState.HandsFreeRecording)
            {
                _audioRecorder.Pause();
                _state = AppState.HandsFreePaused;
                _overlay?.SetHandsFreePaused(true);
                Logger.Log("Hands-free: paused");
            }
            else if (_state == AppState.HandsFreePaused)
            {
                _audioRecorder.Resume();
                _state = AppState.HandsFreeRecording;
                _overlay?.SetHandsFreePaused(false);
                Logger.Log("Hands-free: resumed");
            }
        }

        private void StopHandsFreeRecording()
        {
            if (_state != AppState.HandsFreeRecording && _state != AppState.HandsFreePaused) return;
            Logger.Log("Hands-free: stopping");

            var audioPath = _audioRecorder.Stop();
            _soundPlayer.Play("Pop");

            if (audioPath == null)
            {
                _state = AppState.Idle;
                UpdateIcon(AppState.Idle);
                _overlay?.ContractHandsFree();
                FireAndForget(() => DelayedAction(250, () => _overlay?.Dismiss()));
                return;
            }

            // Check silence before committing to processing UI
            if (_peakAudioLevel < 0.15f)
            {
                Logger.Log($"Silence detected (peak {_peakAudioLevel}) - skipping");
                _state = AppState.Idle;
                UpdateIcon(AppState.Idle);
                _overlay?.ContractHandsFree();
                FireAndForget(() => DelayedAction(250, () => ShowSpeakTipOrError()));
                return;
            }

            _state = AppState.Processing;
            UpdateIcon(AppState.Processing);
            _overlay?.ShowProcessing();
            FireAndForget(() => ProcessRecordedAudioAsync(audioPath));
        }

        // MARK: - Error display

        private void ShowError(string message)
        {
            _state = AppState.Idle;
            UpdateIcon(AppState.Idle);
            _overlay?.ShowError(message);
        }

        /// <summary>
        /// Convert an exception to a short user-friendly error message.
        /// Mirrors showError() from the macOS AppDelegate.
        /// </summary>
        private static string GetUserFriendlyError(Exception? error)
        {
            if (error == null) return "Something went wrong";

            if (error is TranscriptionException txEx)
            {
                return txEx.Kind switch
                {
                    TranscriptionErrorKind.ApiError when
                        txEx.Message.Contains("quota", StringComparison.OrdinalIgnoreCase) ||
                        txEx.Message.Contains("rate", StringComparison.OrdinalIgnoreCase) ||
                        txEx.Message.Contains("429")
                        => "Rate limited - try again",
                    TranscriptionErrorKind.ApiError when
                        txEx.Message.Contains("auth", StringComparison.OrdinalIgnoreCase) ||
                        txEx.Message.Contains("key", StringComparison.OrdinalIgnoreCase) ||
                        txEx.Message.Contains("401") ||
                        txEx.Message.Contains("403")
                        => "Invalid API key",
                    TranscriptionErrorKind.ApiError => "API error",
                    TranscriptionErrorKind.TruncatedResponse => "Response cut off - try again",
                    TranscriptionErrorKind.Timeout => "Request timed out",
                    TranscriptionErrorKind.NetworkError => "No internet connection",
                    _ => txEx.Message.Length <= 50 ? txEx.Message : "Something went wrong"
                };
            }

            var desc = error.Message;
            if (desc.Contains("timed out", StringComparison.OrdinalIgnoreCase) ||
                desc.Contains("timeout", StringComparison.OrdinalIgnoreCase))
                return "Request timed out";
            if (desc.Contains("offline", StringComparison.OrdinalIgnoreCase) ||
                desc.Contains("network", StringComparison.OrdinalIgnoreCase))
                return "No internet connection";

            return "Something went wrong";
        }

        // MARK: - Helpers

        private void UpdateIcon(AppState state)
        {
            _trayIcon?.UpdateIcon(state);
        }

        private static async Task DelayedAction(int milliseconds, Action action)
        {
            await Task.Delay(milliseconds);
            Application.Current?.Dispatcher.BeginInvoke(action);
        }

        /// <summary>
        /// Safely run an async task in a fire-and-forget manner, catching and logging unhandled exceptions.
        /// </summary>
        private async void FireAndForget(Func<Task> action)
        {
            try { await action(); }
            catch (Exception ex) { Logger.Log($"Unhandled error: {ex.Message}"); }
        }
    }
}
