# Yap Windows -- Architecture & Interface Contracts

**Version**: 1.0
**Status**: Accepted
**Target**: .NET 8, C#, WPF

This document defines the platform abstraction interfaces for Yap. Each interface below has exactly one macOS implementation (in Swift) and one Windows implementation (in C#/WPF). The spec (`SPEC.md`) defines the behavioral contract; this document defines the code contract.

---

## Project Structure

```
windows/
  Yap.sln
  Yap/
    App.xaml                       # WPF entry point (no main window, tray-only)
    Yap.csproj
    Interfaces/
      IHotkeyProvider.cs
      IAudioRecorder.cs
      ITranscriber.cs
      ITextFormatter.cs
      IOverlayManager.cs
      IPasteManager.cs
      ITrayManager.cs
      IConfigStore.cs
      ISoundPlayer.cs
    Services/
      HotkeyProvider.cs            # Low-level keyboard hook impl
      AudioRecorder.cs             # NAudio WASAPI capture impl
      NativeTranscriber.cs         # Windows.Media.SpeechRecognition impl
      ApiTranscriber.cs            # HTTP-based transcription (Gemini, OpenAI, Deepgram, ElevenLabs)
      TextFormatter.cs             # HTTP-based formatting (Gemini, OpenAI, Anthropic, Groq)
      OverlayManager.cs            # WPF overlay window impl
      PasteManager.cs              # SendInput-based paste impl
      TrayManager.cs               # NotifyIcon tray impl
      ConfigStore.cs               # JSON file config impl
      SoundPlayer.cs               # WAV playback impl
    Models/
      AppState.cs
      Config.cs
      HistoryEntry.cs
      TranscriptionProvider.cs
      FormattingProvider.cs
      FormattingStyle.cs
    ViewModels/
      OverlayViewModel.cs
      SettingsViewModel.cs
    Views/
      OverlayWindow.xaml
      SettingsWindow.xaml
      HistoryWindow.xaml
    Resources/
      Sounds/
        Pop.wav
        Blow.wav
        Submarine.wav
      Icons/
        tray-idle.ico
        tray-recording.ico
        tray-processing.ico
    AppOrchestrator.cs             # Central state machine (equivalent to AppDelegate)
```

---

## Interface Definitions

### IHotkeyProvider

Monitors a system-wide modifier key and fires callbacks for press, release, and double-tap.

```csharp
/// <summary>
/// System-wide hotkey monitoring. Consumes the monitored key events
/// to prevent system side effects (e.g., Alt menu activation, emoji picker).
/// </summary>
public interface IHotkeyProvider : IDisposable
{
    /// <summary>
    /// Fired when the monitored modifier key is pressed (and no other modifiers are held).
    /// </summary>
    event Action? KeyDown;

    /// <summary>
    /// Fired when the monitored modifier key is released.
    /// </summary>
    event Action? KeyUp;

    /// <summary>
    /// Fired when the modifier key is double-tapped (key-up to key-down within 350ms).
    /// When this fires, KeyDown does NOT fire for the second press.
    /// </summary>
    event Action? DoubleTap;

    /// <summary>
    /// Whether the monitored key is currently held down.
    /// </summary>
    bool IsHeld { get; }

    /// <summary>
    /// Start monitoring. Returns false if permissions are insufficient.
    /// </summary>
    /// <param name="hotkeyType">"fn" for F24/Globe key, "option" for Alt key.</param>
    bool Start(string hotkeyType);

    /// <summary>
    /// Stop monitoring and unhook.
    /// </summary>
    void Stop();
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Hook type | `CGEvent.tapCreate` at HID level (fallback: session level) | `SetWindowsHookEx(WH_KEYBOARD_LL, ...)` |
| fn key mask | Raw flag `0x00800000` on `flagsChanged` events | F24 virtual key (`VK_F24 = 0x87`) or configurable |
| Option/Alt mask | `CGEventFlags.maskAlternate` | `VK_MENU` (Alt key) |
| Event consumption | Return `nil` from tap callback | Set `hookStruct.flags` and return 1 from hook proc |
| Other-modifier guard | Check `flags.intersection(otherModifiers)` is empty | Check `GetKeyState` for Shift/Ctrl/Win/Alt |
| Emoji picker suppression | Also consume keycode 63/179 on keyDown/keyUp | Not applicable (no fn emoji picker on Windows) |
| Double-tap window | 0.35 seconds | 0.35 seconds |

---

### IAudioRecorder

Records microphone audio to a WAV file with real-time FFT-based level reporting.

```csharp
/// <summary>
/// Records microphone audio to a temporary 16-bit PCM WAV file.
/// Provides real-time RMS level and FFT band level callbacks.
/// </summary>
public interface IAudioRecorder : IDisposable
{
    /// <summary>
    /// Fired on the UI thread with overall RMS level (0.0 to 1.0).
    /// Computed as: min(rms * 18.0, 1.0).
    /// </summary>
    event Action<float>? LevelUpdated;

    /// <summary>
    /// Fired on the UI thread with 11 mirrored FFT band levels (each 0.0 to 1.0).
    /// 6 raw log-spaced bands (80Hz-8kHz) mirrored to 11 display bars.
    /// See SPEC.md section 5 for the exact mirror mapping.
    /// </summary>
    event Action<float[]>? BandLevelsUpdated;

    /// <summary>
    /// Whether recording is currently paused (engine running but not writing audio).
    /// </summary>
    bool IsPaused { get; }

    /// <summary>
    /// Start recording. Creates a new audio engine and begins capturing.
    /// Throws on failure (e.g., no microphone permission).
    /// </summary>
    void Start();

    /// <summary>
    /// Stop recording and return the path to the WAV file.
    /// Returns null if no valid audio was captured.
    /// </summary>
    string? Stop();

    /// <summary>
    /// Cancel recording without producing output. Cleans up resources.
    /// </summary>
    void Cancel();

    /// <summary>
    /// Pause recording. Engine stays running (levels still fire) but audio
    /// is not written to the file.
    /// </summary>
    void Pause();

    /// <summary>
    /// Resume recording. Audio is seamlessly appended to the existing file.
    /// </summary>
    void Resume();
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Audio API | `AVAudioEngine` with input node tap, buffer size 2048 | NAudio `WasapiCapture` or `WaveInEvent` |
| FFT library | Accelerate framework (`vDSP_fft_zrip`) | NAudio FFT or MathNet.Numerics |
| WAV format | 16-bit PCM, device sample rate, device channels | 16-bit PCM, device sample rate, mono preferred |
| Temp file | `NSTemporaryDirectory/yap_recording.wav` | `Path.GetTempPath()/yap_recording.wav` |
| Window function | Hann (`vDSP_hann_window`, normalized) | Hann window (manual or library) |
| FFT size | 1024 | 1024 |

---

### ITranscriber

On-device speech recognition for primary transcription (when no API is configured) and as a pre-check before API calls.

```csharp
/// <summary>
/// On-device speech recognition. Used as primary transcriber (no API configured)
/// or as a pre-check to verify speech exists before making API calls.
/// </summary>
public interface ITranscriber
{
    /// <summary>
    /// Request speech recognition permission from the OS.
    /// </summary>
    Task<bool> RequestAuthorizationAsync();

    /// <summary>
    /// Transcribe a local audio file using on-device speech recognition.
    /// </summary>
    /// <param name="audioFilePath">Path to the WAV file.</param>
    /// <returns>Transcribed text, or empty string if no speech detected.</returns>
    /// <exception cref="TranscriptionException">If recognition fails or is unavailable.</exception>
    Task<string> TranscribeAsync(string audioFilePath);
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| API | `SFSpeechRecognizer` (locale: `en-US`) | `Windows.Media.SpeechRecognition.SpeechRecognizer` or `System.Speech` |
| Partial results | Disabled (`shouldReportPartialResults = false`) | Wait for final result only |
| Punctuation | macOS 13+: `addsPunctuation = true` | Depends on engine capabilities |
| On-device | Preferred but not required | Preferred but not required |

---

### ITextFormatter

Handles both API-based transcription (for audio) and LLM-based text formatting.

```csharp
/// <summary>
/// API-based audio transcription and LLM-based text formatting.
/// Handles Gemini, OpenAI, Deepgram, ElevenLabs (transcription)
/// and Gemini, OpenAI, Anthropic, Groq (formatting).
/// </summary>
public interface ITextFormatter
{
    /// <summary>
    /// Transcribe audio using an API provider.
    /// </summary>
    /// <param name="audioFilePath">Path to the WAV file.</param>
    /// <param name="provider">Transcription provider (gemini, openai, deepgram, elevenlabs).</param>
    /// <param name="apiKey">Provider API key.</param>
    /// <param name="model">Model name (null/empty for provider default).</param>
    /// <param name="options">Provider-specific options.</param>
    /// <param name="style">If non-null AND provider supports one-shot, combine transcription + formatting.</param>
    /// <returns>Transcribed (and optionally formatted) text.</returns>
    Task<string> TranscribeAudioAsync(
        string audioFilePath,
        string provider,
        string apiKey,
        string? model,
        TranscriptionOptions options,
        string? style = null
    );

    /// <summary>
    /// Format already-transcribed text using an LLM provider.
    /// Text shorter than 3 characters is returned as-is.
    /// On failure, returns the original text (does not throw).
    /// </summary>
    /// <param name="text">Raw transcription text.</param>
    /// <param name="provider">Formatting provider (gemini, openai, anthropic, groq).</param>
    /// <param name="apiKey">Provider API key.</param>
    /// <param name="model">Model name (null/empty for provider default).</param>
    /// <param name="style">Formatting style: casual, formatted, professional.</param>
    /// <returns>Formatted text, or original text on failure.</returns>
    Task<string> FormatTextAsync(
        string text,
        string provider,
        string apiKey,
        string? model,
        string style
    );
}

/// <summary>
/// Provider-specific transcription options.
/// </summary>
public class TranscriptionOptions
{
    // Deepgram
    public bool DgSmartFormat { get; set; } = true;
    public List<string> DgKeywords { get; set; } = new();
    public string DgLanguage { get; set; } = "";

    // OpenAI
    public string OaiLanguage { get; set; } = "";
    public string OaiPrompt { get; set; } = "";

    // Gemini
    public double GeminiTemperature { get; set; } = 0.0;

    // ElevenLabs
    public string ElLanguageCode { get; set; } = "";
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| HTTP client | `URLSession.shared.dataTask` | `HttpClient` (singleton, reused) |
| Multipart encoding | Manual boundary-based assembly | `MultipartFormDataContent` |
| JSON parsing | `JSONSerialization` | `System.Text.Json.JsonSerializer` |
| Retry logic | Manual retry loop with `DispatchQueue.asyncAfter` | `Polly` or manual retry with `Task.Delay` |
| Max retries | 2 (3 total attempts) | 2 (3 total attempts) |
| Retry backoff | `attempt * 0.5` seconds | `attempt * 0.5` seconds |
| Timeout | `max(30, 30 + audioBytes/64000)` seconds | Same formula |
| Formatter timeout | 15s (Gemini, OpenAI, Anthropic), 10s (Groq) | Same |

**Key behavioral requirement**: The prompt regurgitation guard must be applied to all results. Check (lowercased) for `"transcribe this audio"`, `"respond with only a json"`, `"dictation commands"`. Discard if found.

---

### IOverlayManager

Controls the floating overlay pill window and its animations.

```csharp
/// <summary>
/// Manages the floating overlay pill that shows recording/processing state.
/// The overlay is a transparent, always-on-top, click-through window
/// positioned at the bottom center of the screen.
/// </summary>
public interface IOverlayManager
{
    /// <summary>
    /// Show the recording state: full-size pill with FFT-reactive bars.
    /// </summary>
    void ShowRecording();

    /// <summary>
    /// Show hands-free recording UI with pause/stop buttons.
    /// </summary>
    /// <param name="onPauseResume">Called when pause/resume button is clicked.</param>
    /// <param name="onStop">Called when stop button is clicked.</param>
    void ShowHandsFreeRecording(Action onPauseResume, Action onStop);

    /// <summary>
    /// Set the hands-free paused visual state (static bars, play icon).
    /// </summary>
    void SetHandsFreePaused(bool paused);

    /// <summary>
    /// Show the processing state: gaussian wave sweep animation.
    /// </summary>
    void ShowProcessing();

    /// <summary>
    /// Show an error message in the pill. Auto-dismisses after 2 seconds.
    /// </summary>
    /// <param name="message">Short error message to display.</param>
    void ShowError(string message);

    /// <summary>
    /// Show the no-speech visual state (static low bars + shake animation).
    /// </summary>
    void ShowNoSpeech();

    /// <summary>
    /// Dismiss the overlay (return to idle state).
    /// If alwaysVisible is false, slides off-screen.
    /// </summary>
    void Dismiss();

    /// <summary>
    /// Contract the hands-free UI (buttons fly back, pill shrinks).
    /// </summary>
    void ContractHandsFree();

    /// <summary>
    /// Update the overall audio level for bounce animation.
    /// </summary>
    /// <param name="level">RMS level 0.0 to 1.0.</param>
    void UpdateLevel(float level);

    /// <summary>
    /// Update the 11-bar FFT band levels.
    /// </summary>
    /// <param name="levels">Array of 11 floats, each 0.0 to 1.0.</param>
    void UpdateBandLevels(float[] levels);

    /// <summary>
    /// Advance onboarding to a specific step.
    /// </summary>
    void AdvanceOnboarding(string step);

    /// <summary>
    /// Complete onboarding and hide onboarding UI.
    /// </summary>
    void CompleteOnboarding();

    /// <summary>
    /// Set the hotkey label shown in onboarding cards ("fn" or "option").
    /// </summary>
    void SetHotkeyLabel(string label);

    /// <summary>
    /// Play the press-down animation (scale 0.85, opacity 0.7).
    /// </summary>
    void PressDown();

    /// <summary>
    /// Play the press-release animation (spring back to normal).
    /// </summary>
    void PressRelease();

    /// <summary>
    /// Play the horizontal shake animation.
    /// </summary>
    void Shake();

    /// <summary>
    /// Set whether the pill is always visible when idle.
    /// </summary>
    void SetAlwaysVisible(bool visible, bool animated = true);

    /// <summary>
    /// Enable or disable the lava lamp gradient background.
    /// </summary>
    void SetGradientEnabled(bool enabled);

    /// <summary>
    /// Register a callback for when the pill is clicked to start recording.
    /// </summary>
    void SetOnClickToRecord(Action callback);

    /// <summary>
    /// The current onboarding step, or null if not in onboarding.
    /// </summary>
    string? CurrentOnboardingStep { get; }
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Window type | `NSPanel` (borderless, nonactivatingPanel) | WPF `Window` with `AllowsTransparency=True`, `WindowStyle=None`, `ShowInTaskbar=False` |
| UI framework | SwiftUI embedded via `NSHostingView` | WPF with XAML or SkiaSharp for custom rendering |
| Click-through | Custom `hitTest` returning nil for non-pill areas | `WS_EX_TRANSPARENT` extended style, toggle per-region |
| Level | `.floating` | `Topmost = true` |
| All desktops | `canJoinAllSpaces`, `fullScreenAuxiliary`, `stationary` | Use virtual desktop APIs or always-on-top |
| Animations | SwiftUI `.spring()`, `.timingCurve()` | WPF `Storyboard`, `DoubleAnimation`, or Composition APIs |
| Blur material | SwiftUI `.thinMaterial` | Acrylic/Mica via `WindowCompositionAttribute` or `BackdropMaterial` |
| Gradient | Custom `TimelineView` with animated ellipses | WPF `CompositionTarget.Rendering` or `DispatcherTimer` |

---

### IPasteManager

Writes text to the clipboard, simulates a paste keystroke, then restores the previous clipboard.

```csharp
/// <summary>
/// Pastes text into the focused application by writing to the clipboard,
/// simulating Ctrl+V, and restoring the previous clipboard contents.
/// </summary>
public interface IPasteManager
{
    /// <summary>
    /// Paste the given text into the currently focused application.
    /// Sequence: save clipboard -> set text -> wait 50ms -> Ctrl+V -> wait 300ms -> restore clipboard.
    /// </summary>
    /// <param name="text">The text to paste.</param>
    void Paste(string text);
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Clipboard API | `NSPasteboard.general` | `System.Windows.Clipboard` (must run on STA thread) |
| Keystroke simulation | `CGEvent` with virtual keycode 0x09, `.maskCommand` | `SendInput` with `VK_CONTROL` + `VK_V` |
| Event posting | `cgAnnotatedSessionEventTap` | `SendInput` (user-level, no elevation needed) |
| Paste delay | 50ms | 50ms |
| Restore delay | 300ms | 300ms |
| Thread affinity | Main thread via `DispatchQueue.main.asyncAfter` | STA thread via `Dispatcher.InvokeAsync` |

---

### ITrayManager

Manages the system tray icon and context menu.

```csharp
/// <summary>
/// System tray (notification area) icon and context menu management.
/// </summary>
public interface ITrayManager : IDisposable
{
    /// <summary>
    /// Fired when "Enabled" is toggled.
    /// </summary>
    event Action<bool>? EnabledToggled;

    /// <summary>
    /// Fired when "Settings..." is clicked.
    /// </summary>
    event Action? SettingsRequested;

    /// <summary>
    /// Fired when "Quit" is clicked.
    /// </summary>
    event Action? QuitRequested;

    /// <summary>
    /// Fired when "Show All..." history is clicked.
    /// </summary>
    event Action? HistoryWindowRequested;

    /// <summary>
    /// Fired when a history entry is clicked (copies text).
    /// </summary>
    event Action<string>? HistoryCopyRequested;

    /// <summary>
    /// Fired when "Clear History" is clicked.
    /// </summary>
    event Action? HistoryClearRequested;

    /// <summary>
    /// Initialize the tray icon with the idle state.
    /// </summary>
    void Initialize();

    /// <summary>
    /// Update the tray icon to reflect the current app state.
    /// idle = mic outline, recording/handsFree/paused = mic filled, processing = ellipsis.
    /// </summary>
    /// <param name="state">Current app state.</param>
    void UpdateIcon(string state);

    /// <summary>
    /// Rebuild the history submenu with current entries.
    /// Called each time the tray menu is opened.
    /// </summary>
    /// <param name="entries">History entries (max 10), newest first.</param>
    void RebuildHistoryMenu(IReadOnlyList<HistoryEntry> entries);
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Tray API | `NSStatusBar.system.statusItem` with `NSMenu` | `System.Windows.Forms.NotifyIcon` or `Hardcodet.NotifyIcon.Wpf` |
| Icons | SF Symbols (`mic`, `mic.fill`, `ellipsis.circle`) or template PNG | `.ico` files bundled as resources |
| Menu rebuild | `menuWillOpen` delegate method | `ContextMenu.Opening` event |
| Menu shortcuts | `keyEquivalent` on `NSMenuItem` | `InputGestureText` on `MenuItem` |

---

### IConfigStore

Reads and writes application configuration.

```csharp
/// <summary>
/// Persistent configuration storage.
/// macOS uses UserDefaults; Windows uses a JSON file at %APPDATA%\yap\config.json.
/// </summary>
public interface IConfigStore
{
    /// <summary>
    /// Get a string value. Returns defaultValue if key is not set.
    /// </summary>
    string GetString(string key, string defaultValue = "");

    /// <summary>
    /// Get a boolean value. Returns defaultValue if key is not set.
    /// </summary>
    bool GetBool(string key, bool defaultValue = false);

    /// <summary>
    /// Get a double value. Returns defaultValue if key is not set.
    /// </summary>
    double GetDouble(string key, double defaultValue = 0.0);

    /// <summary>
    /// Set a string value.
    /// </summary>
    void SetString(string key, string value);

    /// <summary>
    /// Set a boolean value.
    /// </summary>
    void SetBool(string key, bool value);

    /// <summary>
    /// Set a double value.
    /// </summary>
    void SetDouble(string key, double value);

    /// <summary>
    /// Persist any pending changes to disk.
    /// On macOS this is a no-op (UserDefaults auto-syncs).
    /// On Windows this writes the JSON file atomically.
    /// </summary>
    void Save();

    /// <summary>
    /// Reload configuration from disk.
    /// </summary>
    void Reload();
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Backend | `UserDefaults.standard` | JSON file at `%APPDATA%\yap\config.json` |
| Persistence | Automatic (NSUserDefaults syncs) | Manual `Save()` call writes JSON atomically |
| File format | N/A (plist-backed) | UTF-8 JSON with `JsonSerializerOptions.WriteIndented` |
| Concurrency | Thread-safe (NSUserDefaults) | Use `lock` or `ReaderWriterLockSlim` |
| Directory creation | N/A | `Directory.CreateDirectory` on first write |

**Known settings keys** (see SPEC.md section 10 for complete list):

```csharp
public static class SettingsKeys
{
    public const string Hotkey = "hotkey";                    // "fn" | "option"
    public const string TxProvider = "txProvider";            // "none" | "gemini" | "openai" | "deepgram" | "elevenlabs"
    public const string TxApiKey = "txApiKey";
    public const string TxModel = "txModel";
    public const string FmtProvider = "fmtProvider";          // "none" | "gemini" | "openai" | "anthropic" | "groq"
    public const string FmtApiKey = "fmtApiKey";
    public const string FmtModel = "fmtModel";
    public const string FmtStyle = "fmtStyle";                // "casual" | "formatted" | "professional"
    public const string OnboardingComplete = "onboardingComplete";
    public const string DgSmartFormat = "dgSmartFormat";
    public const string DgKeywords = "dgKeywords";
    public const string DgLanguage = "dgLanguage";
    public const string OaiLanguage = "oaiLanguage";
    public const string OaiPrompt = "oaiPrompt";
    public const string GeminiTemperature = "geminiTemperature";
    public const string ElLanguageCode = "elLanguageCode";
    public const string SoundsEnabled = "soundsEnabled";
    public const string GradientEnabled = "gradientEnabled";
    public const string AlwaysVisiblePill = "alwaysVisiblePill";
    public const string HistoryEnabled = "historyEnabled";
}
```

---

### ISoundPlayer

Plays short sound effects with zero-latency preloading.

```csharp
/// <summary>
/// Plays bundled sound effects. Preloads all sounds at startup
/// for zero-latency playback during recording/processing.
/// </summary>
public interface ISoundPlayer
{
    /// <summary>
    /// Preload all sound files into memory. Call once at app startup.
    /// Expected sounds: "Pop", "Blow", "Submarine".
    /// </summary>
    void PreloadAll();

    /// <summary>
    /// Play a sound by name. No-op if sounds are disabled or the sound is not found.
    /// Rewinds to the beginning if the sound is already playing.
    /// </summary>
    /// <param name="name">Sound name: "Pop", "Blow", or "Submarine".</param>
    void Play(string name);

    /// <summary>
    /// Whether sound effects are enabled. Checked on each Play() call.
    /// Reads from IConfigStore's "soundsEnabled" key (default: true).
    /// </summary>
    bool IsEnabled { get; }
}
```

**Platform implementation notes**:

| Aspect | macOS (Swift) | Windows (C#) |
|--------|---------------|--------------|
| Format | AIFF (`Pop.aiff`, `Blow.aiff`, `Submarine.aiff`) | WAV (`Pop.wav`, `Blow.wav`, `Submarine.wav`) |
| Player | `AVAudioPlayer` with `prepareToPlay()` | `SoundPlayer` with `Load()` or `NAudio.Wave.WaveOutEvent` |
| Preloading | `AVAudioPlayer(contentsOf:)` + `prepareToPlay()` | Load WAV bytes into memory, create player instances |
| Rewind | Set `currentTime = 0` before `play()` | `Stop()` + `Play()` or seek to start |
| Source | App bundle (`Bundle.main.url(forResource:)`) | Embedded resource or `Resources/Sounds/` directory |

---

## AppOrchestrator

The `AppOrchestrator` is the central coordinator -- equivalent to `AppDelegate` on macOS. It owns the state machine and wires all interfaces together.

```csharp
/// <summary>
/// Central orchestrator that owns the state machine and coordinates
/// the full pipeline: hotkey -> record -> transcribe -> format -> paste.
/// This is NOT an interface -- it is the concrete application core.
/// </summary>
public class AppOrchestrator : IDisposable
{
    // Dependencies (injected)
    private readonly IHotkeyProvider _hotkey;
    private readonly IAudioRecorder _recorder;
    private readonly ITranscriber _nativeTranscriber;
    private readonly ITextFormatter _formatter;
    private readonly IOverlayManager _overlay;
    private readonly IPasteManager _paster;
    private readonly ITrayManager _tray;
    private readonly IConfigStore _config;
    private readonly ISoundPlayer _sounds;

    // State
    private AppState _state = AppState.Idle;
    private DateTime _recordingStart;
    private float _peakAudioLevel;
    private bool _isEnabled = true;
    private bool _ignorePendingKeyUp;

    // Lifecycle
    public void Initialize();         // Wire events, request permissions, start hotkey, setup engines
    public void Shutdown();            // Cleanup all resources

    // Recording flow
    private void StartRecording();     // Key down handler
    private void StopAndTranscribe();  // Key up handler
    private void StartHandsFreeRecording();  // Double-tap handler
    private void StartClickRecording();      // Pill click handler
    private void ToggleHandsFreePause();
    private void StopHandsFreeRecording();
    private void ProcessRecordedAudio(string audioFilePath);
    private void SendToApi(string audioFilePath);
    private void MaybeFormat(string text);
    private void HandleResult(string text);
    private void PasteText(string text);
    private void FinishProcessing();

    // Error handling
    private void ShowError(Exception error);
    private void ShowTip(string tipStep);

    // Onboarding
    private void StartOnboardingIfNeeded();
    private void AdvanceOnboardingStep();
    private void FinalizeOnboarding();

    // Settings
    private void OnSettingsChanged();
    private void SetupEngines();
}
```

---

## Dependency Graph

```
AppOrchestrator
  |
  +-- IHotkeyProvider       (system input)
  +-- IAudioRecorder         (microphone capture)
  +-- ITranscriber           (on-device speech)
  +-- ITextFormatter         (API transcription + LLM formatting)
  +-- IOverlayManager        (UI overlay)
  +-- IPasteManager          (clipboard + keystroke)
  +-- ITrayManager           (system tray)
  +-- IConfigStore           (persistence)
  +-- ISoundPlayer           (audio feedback)
```

All interfaces communicate with the orchestrator via events or callbacks. No interface depends on another interface directly -- all coordination flows through `AppOrchestrator`.

---

## Key Architectural Decisions

### ADR-001: Interface-Based Abstraction Over Shared Code

**Context**: Yap must run on macOS (Swift) and Windows (C#). The two platforms share zero compiled code.

**Decision**: Define behavioral contracts as C# interfaces. Each platform implements all interfaces natively. No shared runtime, no cross-compilation, no Electron/MAUI.

**Consequences**: Maximum native fidelity on each platform. Trade-off: feature parity must be maintained through the spec and tests, not through shared code. Any behavioral change requires updating both implementations.

### ADR-002: Single Orchestrator Pattern

**Context**: The application has a linear pipeline with clear state transitions. Multiple coordination patterns were considered (event bus, mediator, actor model).

**Decision**: Single `AppOrchestrator` class owns the state machine and all inter-component communication. Interfaces never call each other directly.

**Consequences**: Simple to reason about, easy to debug. Trade-off: the orchestrator accumulates complexity as features grow. Mitigated by keeping interfaces focused (single responsibility) so the orchestrator is primarily wiring, not logic.

### ADR-003: Config Store Abstraction Over Platform Defaults

**Context**: macOS uses `UserDefaults` (plist-backed, auto-syncing). Windows needs a custom solution.

**Decision**: Abstract config access behind `IConfigStore`. macOS wraps `UserDefaults`; Windows uses a JSON file with explicit `Save()`.

**Consequences**: Identical config keys work on both platforms. Trade-off: Windows requires explicit save calls, which is a behavioral difference the orchestrator must handle. macOS implementation's `Save()` is a no-op.

### ADR-004: WPF Over WinUI 3 for Windows

**Context**: WinUI 3 is newer but has gaps in system tray support, always-on-top behavior, and click-through windows. WPF is mature and has full Win32 interop.

**Decision**: Use WPF (.NET 8) for the Windows implementation.

**Consequences**: Full access to Win32 APIs for keyboard hooks, SendInput, and window styles. Acrylic blur requires P/Invoke but is well-documented. Trade-off: WPF is older and will eventually be superseded, but for a tray-based utility app it is the pragmatic choice today.
