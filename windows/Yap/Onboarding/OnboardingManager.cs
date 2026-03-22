using System;
using System.Windows.Threading;
using Yap.Core;

namespace Yap.Onboarding
{
    /// <summary>
    /// Onboarding step enum. Mirrors the macOS OnboardingStep enum.
    /// </summary>
    public enum OnboardingStep
    {
        TryIt,
        Nice,
        DoubleTapTip,
        ClickTip,
        ApiTip,
        FormattingTip,
        Welcome,
        // Transient tips
        SpeakTip,
        HoldTip
    }

    /// <summary>
    /// Manages the onboarding flow state machine.
    /// Tracks current step, manages transitions, handles transient tips,
    /// and gates input based on current onboarding state.
    ///
    /// Direct port of onboarding logic from macOS AppDelegate.
    /// </summary>
    public class OnboardingManager
    {
        // Nice messages pool (matching spec)
        private static readonly string[] NiceMessages =
        {
            "Nice! \U0001F389",
            "Nailed it! \u2728",
            "Sounds good! \U0001F44C",
            "Got it! \U0001F64C",
            "Perfect! \U0001F3AF",
            "Love it! \U0001F4AB"
        };

        private static readonly Random _random = new();

        // Current step state
        private OnboardingStep? _currentStep;
        private OnboardingStep? _nextStepAfterNice;

        /// <summary>
        /// The onboarding step that was active before a transient tip appeared.
        /// Used to enforce the same input restrictions during the tip as before it.
        /// </summary>
        private OnboardingStep? _preTipStep;

        // Timers
        private DispatcherTimer? _tipDismissTimer;
        private DispatcherTimer? _niceAdvanceTimer;
        private DispatcherTimer? _holdConfirmTimer;
        private DispatcherTimer? _holdAdvanceTimer;
        private DispatcherTimer? _welcomeCompleteTimer;
        private DispatcherTimer? _restoreTimer;

        /// <summary>Current onboarding step (null if onboarding is complete or not started).</summary>
        public OnboardingStep? CurrentStep => _currentStep;

        /// <summary>Whether onboarding is currently active (a step is being shown).</summary>
        public bool IsActive => _currentStep != null;

        /// <summary>Whether onboarding has been completed (config flag).</summary>
        public bool IsComplete => Config.Current.OnboardingComplete;

        /// <summary>The hotkey display label (e.g., "Caps Lock", "F24").</summary>
        public string HotkeyLabel { get; set; } = "Caps Lock";

        // Callbacks — the orchestrator wires these up
        public Action<OnboardingStep, string>? OnStepChanged { get; set; }
        public Action? OnComplete { get; set; }
        public Action? OnPressDown { get; set; }
        public Action? OnPressRelease { get; set; }
        public Action? OnShake { get; set; }
        public Action? OnShowNoSpeech { get; set; }
        public Action<string>? OnPlaySound { get; set; }

        /// <summary>
        /// The logical onboarding step for input gating. When the overlay shows a transient tip
        /// (.SpeakTip / .HoldTip), returns the step that caused the tip so that input restrictions
        /// from the parent step are enforced regardless of the current visual state.
        /// </summary>
        public OnboardingStep? EffectiveStep => _preTipStep ?? _currentStep;

        /// <summary>
        /// Start onboarding if not already completed.
        /// </summary>
        public void StartIfNeeded()
        {
            if (IsComplete) return;

            HotkeyLabel = GetHotkeyDisplayName();
            AdvanceTo(OnboardingStep.TryIt);
        }

        /// <summary>
        /// Advance to a specific onboarding step.
        /// </summary>
        public void AdvanceTo(OnboardingStep step)
        {
            _currentStep = step;
            Logger.Log($"Onboarding: advanced to {step}");

            string text = GetStepText(step);
            OnStepChanged?.Invoke(step, text);
        }

        /// <summary>
        /// Complete onboarding: set config flag, clear state, notify UI.
        /// </summary>
        public void CompleteOnboarding()
        {
            _currentStep = null;
            _preTipStep = null;
            CancelAllTimers();

            var config = Config.Current;
            config.OnboardingComplete = true;
            Config.Save(config);

            OnComplete?.Invoke();
            Logger.Log("Onboarding: finalized");
        }

        /// <summary>
        /// Get display text for a given onboarding step.
        /// </summary>
        public string GetStepText(OnboardingStep step)
        {
            return step switch
            {
                OnboardingStep.TryIt => $"Hold [{HotkeyLabel}] and speak",
                OnboardingStep.Nice => NiceMessages[_random.Next(NiceMessages.Length)],
                OnboardingStep.DoubleTapTip => $"Double-tap [{HotkeyLabel}] for hands-free",
                OnboardingStep.ClickTip => "Click the pill to start/stop",
                OnboardingStep.ApiTip => "Add a transcription API in Settings",
                OnboardingStep.FormattingTip => "Enable formatting in Settings for cleaner text",
                OnboardingStep.Welcome => "You're all set \U0001F389",
                OnboardingStep.SpeakTip => $"Didn't catch that \u2014 speak up while holding [{HotkeyLabel}]",
                OnboardingStep.HoldTip => $"Hold [{HotkeyLabel}] \u2014 don't just tap it",
                _ => ""
            };
        }

        // =====================================================================
        // Input gating
        // =====================================================================

        /// <summary>
        /// Determines what action to take when the user presses the hotkey down.
        /// Returns the action type the orchestrator should perform.
        /// </summary>
        public OnboardingKeyDownAction HandleKeyDown()
        {
            if (!IsActive) return OnboardingKeyDownAction.AllowRecording;

            var step = EffectiveStep;
            switch (step)
            {
                // Double-tap-only and click-only: fn key is fully blocked
                case OnboardingStep.ClickTip:
                case OnboardingStep.DoubleTapTip:
                    return OnboardingKeyDownAction.Block;

                // Hold-to-confirm steps
                case OnboardingStep.ApiTip:
                case OnboardingStep.FormattingTip:
                case OnboardingStep.Welcome:
                    StartHoldToConfirm(step!.Value);
                    return OnboardingKeyDownAction.HoldToConfirm;

                // TryIt and other steps: dismiss any transient tip and allow recording
                default:
                    DismissTransientTip();
                    return OnboardingKeyDownAction.AllowRecording;
            }
        }

        /// <summary>
        /// Determines what action to take when the user releases the hotkey.
        /// </summary>
        public OnboardingKeyUpAction HandleKeyUp()
        {
            // Handle hold-to-confirm release (too early)
            if (_holdConfirmTimer != null)
            {
                CancelHoldToConfirm();
                return OnboardingKeyUpAction.Handled;
            }

            return OnboardingKeyUpAction.AllowStop;
        }

        /// <summary>
        /// Determines what action to take on double-tap.
        /// </summary>
        public OnboardingDoubleTapAction HandleDoubleTap()
        {
            if (!IsActive) return OnboardingDoubleTapAction.Allow;

            var step = EffectiveStep;

            // Only allowed during doubleTapTip step
            if (step == OnboardingStep.DoubleTapTip)
                return OnboardingDoubleTapAction.Allow;

            // All other onboarding steps block double-tap
            return OnboardingDoubleTapAction.Block;
        }

        /// <summary>
        /// Determines what action to take on pill click.
        /// </summary>
        public OnboardingClickAction HandlePillClick()
        {
            if (!IsActive) return OnboardingClickAction.Allow;

            var step = EffectiveStep;

            // Only allowed during clickTip step
            if (step == OnboardingStep.ClickTip)
                return OnboardingClickAction.Allow;

            // All other onboarding steps block pill click
            return OnboardingClickAction.Block;
        }

        // =====================================================================
        // Recording result handlers
        // =====================================================================

        /// <summary>
        /// Called after a successful transcription+paste. Determines the next onboarding step.
        /// Returns true if the onboarding was advanced (caller should play celebration sound).
        /// </summary>
        public bool HandleTranscriptionSuccess()
        {
            if (!IsActive) return false;

            OnboardingStep? nextStep = _currentStep switch
            {
                OnboardingStep.TryIt => OnboardingStep.DoubleTapTip,
                OnboardingStep.DoubleTapTip => OnboardingStep.ClickTip,
                OnboardingStep.ClickTip => OnboardingStep.ApiTip,
                _ => null
            };

            if (nextStep != null)
            {
                ShowNice(nextStep.Value);
                return true;
            }

            return false;
        }

        /// <summary>
        /// Called when a recording was too short (hold tip).
        /// Shows the holdTip transient state.
        /// </summary>
        public void HandleTooShort()
        {
            ShowTransientTip(OnboardingStep.HoldTip);
        }

        /// <summary>
        /// Called when silence was detected (speak tip).
        /// Shows the speakTip transient state.
        /// </summary>
        public void HandleSilence()
        {
            ShowTransientTip(OnboardingStep.SpeakTip);
        }

        /// <summary>
        /// Called after processing finishes to restore onboarding UI if needed.
        /// </summary>
        public void RestoreIfNeeded()
        {
            if (IsComplete) return;

            var step = _currentStep;
            if (step == null) return;

            OnboardingStep restoreTo;
            switch (step)
            {
                case OnboardingStep.TryIt:
                case OnboardingStep.SpeakTip:
                case OnboardingStep.HoldTip:
                    restoreTo = OnboardingStep.TryIt;
                    break;
                case OnboardingStep.ClickTip:
                    restoreTo = OnboardingStep.ClickTip;
                    break;
                case OnboardingStep.DoubleTapTip:
                    restoreTo = OnboardingStep.DoubleTapTip;
                    break;
                default:
                    return;
            }

            _restoreTimer?.Stop();
            _restoreTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
            _restoreTimer.Tick += (_, _) =>
            {
                _restoreTimer.Stop();
                _restoreTimer = null;
                AdvanceTo(restoreTo);
            };
            _restoreTimer.Start();
        }

        // =====================================================================
        // Hold-to-Confirm
        // =====================================================================

        private void StartHoldToConfirm(OnboardingStep step)
        {
            Logger.Log($"Onboarding: hold-to-confirm for {step}");
            OnPressDown?.Invoke();

            _holdConfirmTimer?.Stop();
            _holdConfirmTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(600) };
            _holdConfirmTimer.Tick += (_, _) =>
            {
                _holdConfirmTimer.Stop();
                _holdConfirmTimer = null;

                OnPressRelease?.Invoke();
                OnPlaySound?.Invoke("Pop");

                // Wait 0.4s then advance
                _holdAdvanceTimer?.Stop();
                _holdAdvanceTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(400) };
                _holdAdvanceTimer.Tick += (_, _) =>
                {
                    _holdAdvanceTimer.Stop();
                    _holdAdvanceTimer = null;
                    AdvanceFromConfirmStep(step);
                };
                _holdAdvanceTimer.Start();
            };
            _holdConfirmTimer.Start();
        }

        private void CancelHoldToConfirm()
        {
            _holdConfirmTimer?.Stop();
            _holdConfirmTimer = null;
            OnPressRelease?.Invoke();
            OnShake?.Invoke();
        }

        private void AdvanceFromConfirmStep(OnboardingStep step)
        {
            switch (step)
            {
                case OnboardingStep.ApiTip:
                    AdvanceTo(OnboardingStep.FormattingTip);
                    break;
                case OnboardingStep.FormattingTip:
                    AdvanceTo(OnboardingStep.Welcome);
                    break;
                case OnboardingStep.Welcome:
                    CompleteOnboarding();
                    break;
                default:
                    CompleteOnboarding();
                    break;
            }
        }

        // =====================================================================
        // Nice (celebration) step
        // =====================================================================

        private void ShowNice(OnboardingStep nextStep)
        {
            _nextStepAfterNice = nextStep;
            AdvanceTo(OnboardingStep.Nice);

            // Auto-advance after 1.5s
            _niceAdvanceTimer?.Stop();
            _niceAdvanceTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(1500) };
            _niceAdvanceTimer.Tick += (_, _) =>
            {
                _niceAdvanceTimer.Stop();
                _niceAdvanceTimer = null;
                if (_currentStep == OnboardingStep.Nice && _nextStepAfterNice != null)
                {
                    AdvanceTo(_nextStepAfterNice.Value);
                    _nextStepAfterNice = null;
                }
            };
            _niceAdvanceTimer.Start();
        }

        // =====================================================================
        // Transient tips
        // =====================================================================

        private void ShowTransientTip(OnboardingStep tipStep)
        {
            // Capture which step we're on before the tip overwrites it
            _preTipStep = _currentStep;

            OnShowNoSpeech?.Invoke();
            AdvanceTo(tipStep);

            // Auto-dismiss after 2.5s
            _tipDismissTimer?.Stop();
            _tipDismissTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(2500) };
            _tipDismissTimer.Tick += (_, _) =>
            {
                _tipDismissTimer.Stop();
                _tipDismissTimer = null;

                if (_currentStep != tipStep) return;

                // Capture pre-tip step before clearing
                var savedPreTipStep = _preTipStep;
                _preTipStep = null;

                if (IsComplete)
                {
                    OnComplete?.Invoke();
                    return;
                }

                // Use the saved pre-tip step to choose restore target
                OnboardingStep target;
                switch (savedPreTipStep)
                {
                    case OnboardingStep.ClickTip:
                        target = OnboardingStep.ClickTip;
                        break;
                    case OnboardingStep.DoubleTapTip:
                        target = OnboardingStep.DoubleTapTip;
                        break;
                    default:
                        target = OnboardingStep.TryIt;
                        break;
                }

                // Brief delay before restoring
                _restoreTimer?.Stop();
                _restoreTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
                _restoreTimer.Tick += (_, _) =>
                {
                    _restoreTimer.Stop();
                    _restoreTimer = null;
                    AdvanceTo(target);
                };
                _restoreTimer.Start();
            };
            _tipDismissTimer.Start();
        }

        /// <summary>
        /// Dismiss any currently-showing transient tip and restore the overlay
        /// to the step that was active before the tip. No-op if not in a transient tip.
        /// </summary>
        public void DismissTransientTip()
        {
            if (_currentStep != OnboardingStep.SpeakTip && _currentStep != OnboardingStep.HoldTip)
                return;

            _tipDismissTimer?.Stop();
            _tipDismissTimer = null;

            if (IsComplete)
            {
                OnComplete?.Invoke();
            }
            else
            {
                AdvanceTo(_preTipStep ?? OnboardingStep.TryIt);
            }
            _preTipStep = null;
        }

        // =====================================================================
        // Settings integration
        // =====================================================================

        /// <summary>
        /// Called when settings change. Updates the hotkey label for onboarding text.
        /// </summary>
        public void OnSettingsChanged()
        {
            if (!IsActive) return;
            HotkeyLabel = GetHotkeyDisplayName();

            // Re-display current step with updated hotkey label
            if (_currentStep != null)
            {
                string text = GetStepText(_currentStep.Value);
                OnStepChanged?.Invoke(_currentStep.Value, text);
            }
        }

        // =====================================================================
        // Helpers
        // =====================================================================

        private string GetHotkeyDisplayName()
        {
            var configName = Config.Current.Hotkey;
            int vk = Input.HotkeyManager.NameToVirtualKey(configName);
            return Input.HotkeyManager.VirtualKeyToName(vk);
        }

        private void CancelAllTimers()
        {
            _tipDismissTimer?.Stop();
            _tipDismissTimer = null;
            _niceAdvanceTimer?.Stop();
            _niceAdvanceTimer = null;
            _holdConfirmTimer?.Stop();
            _holdConfirmTimer = null;
            _holdAdvanceTimer?.Stop();
            _holdAdvanceTimer = null;
            _welcomeCompleteTimer?.Stop();
            _welcomeCompleteTimer = null;
            _restoreTimer?.Stop();
            _restoreTimer = null;
        }
    }

    // =====================================================================
    // Action enums for orchestrator communication
    // =====================================================================

    public enum OnboardingKeyDownAction
    {
        /// <summary>Allow normal recording to start.</summary>
        AllowRecording,
        /// <summary>Block all input (step doesn't allow fn key).</summary>
        Block,
        /// <summary>Hold-to-confirm behavior (apiTip, formattingTip, welcome).</summary>
        HoldToConfirm
    }

    public enum OnboardingKeyUpAction
    {
        /// <summary>Allow normal stop-and-transcribe.</summary>
        AllowStop,
        /// <summary>Key-up was handled by onboarding (hold-to-confirm cancel).</summary>
        Handled
    }

    public enum OnboardingDoubleTapAction
    {
        Allow,
        Block
    }

    public enum OnboardingClickAction
    {
        Allow,
        Block
    }
}
