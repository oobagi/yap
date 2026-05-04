# Yap -- Cross-Platform Specification

**Version**: 1.0
**Status**: Accepted
**Applies to**: macOS and Windows Tauri app

This document is the single source of truth for feature parity in the Tauri-based Yap app. Shared behavior should live in Rust/Svelte where possible; platform-specific behavior should be limited to OS integration such as hotkeys, overlays, paste simulation, speech permissions, and bundling.

---

## 1. App Identity

| Field | Value |
|-------|-------|
| App name | Yap |
| macOS bundle ID | `com.yap.desktop` |
| Windows app ID | `com.yap.desktop` |
| Version scheme | SemVer: `MAJOR.MINOR.PATCH` |
| Current version | `2.0.1` |
| Activation policy | **Background/accessory** -- no Dock icon (macOS `LSUIElement = true`), no taskbar window (Windows: tray-only) |

---

## 2. Pipeline

The full data flow from user input to pasted text:

```
HotkeyProvider (modifier key press/release/double-tap)
  -> AudioRecorder (microphone -> 16-bit PCM WAV + real-time FFT levels)
  -> OverlayManager (floating pill with waveform bars / processing animation)
  -> Transcription (Apple Speech on-device OR API: Gemini / OpenAI / Deepgram / ElevenLabs)
  -> Formatting (optional LLM: Gemini / OpenAI / Anthropic / Groq)
  -> PasteManager (clipboard write -> simulated Cmd+V / Ctrl+V -> clipboard restore)
```

### Pipeline Modes

1. **Apple Speech only** -- on-device transcription, optional formatting.
2. **API transcription only** -- API transcription (with Apple Speech pre-check), optional formatting.
3. **Gemini one-shot** -- when Gemini is both transcriber and formatter, a single API call handles both (audio + style prompt -> JSON response). Only Gemini supports this optimization.
4. **Two-step** -- API transcription followed by a separate LLM formatting call.

---

## 3. State Machine

### States

| State | Description |
|-------|-------------|
| `idle` | Waiting for hotkey. Overlay is hidden or shows minimized pill. |
| `recording` | Hotkey held. Microphone active. Overlay shows waveform bars. |
| `handsFreeRecording` | Double-tap or click-to-record engaged. Microphone active. Pause/stop buttons visible. |
| `handsFreePaused` | Hands-free recording paused. Engine running but audio not written. Bars show static low state. |
| `processing` | Audio sent to transcription/formatting. Overlay shows sweep animation. |

### Transitions

```
idle --(key down)--> recording
idle --(double-tap)--> handsFreeRecording
idle --(pill click)--> handsFreeRecording  (via recording, immediately converted)
recording --(key up, duration >= 0.5s OR peak >= 0.15)--> processing
recording --(key up, duration < 0.5s AND peak < 0.15)--> idle  (show holdTip)
recording --(double-tap)--> handsFreeRecording
recording --(pill click)--> handsFreeRecording
handsFreeRecording --(pause button / key up)--> handsFreePaused  (if not ignoring pending key-up)
handsFreeRecording --(stop button / key up)--> processing
handsFreePaused --(resume button)--> handsFreeRecording
handsFreePaused --(stop button / key up)--> processing
processing --(result received)--> idle
processing --(error)--> idle  (show error overlay)
```

### Guards

- `startRecording()` is blocked when `state != idle` or `isEnabled == false`.
- During onboarding, specific steps block specific input types (see section 14).

---

## 4. Hotkey Behavior

### Supported Hotkeys

| Hotkey | macOS mask | Windows equivalent |
|--------|------------|-------------------|
| fn / Globe | `0x00800000` (raw flag) | F24 or configurable virtual key |
| Option | `CGEventFlags.maskAlternate` | Alt key |

### Press/Release Semantics

1. **Key down**: Start recording. Consume the event (prevent system side effects like emoji picker or menu focus).
2. **Key up**: Stop recording. If duration < 0.5s AND peak audio < 0.15, show `holdTip` and cancel. Otherwise, transition to `processing`.

### Double-Tap Detection

- **Window**: 0.35 seconds between consecutive key-up and key-down.
- **Behavior**: On double-tap, call `onDoubleTap` instead of `onKeyDown`. This enters hands-free mode.
- **Implementation**: Track `lastKeyUpTime`. If `now - lastKeyUpTime < 0.35s` on next key-down, fire double-tap.

### Event Consumption

- macOS: CGEventTap at HID level (preferred) or session level. Consumes `flagsChanged`, and for fn key also suppresses keycode 63/179 (`keyDown`/`keyUp`) to prevent emoji picker.
- Windows: Low-level keyboard hook (`WH_KEYBOARD_LL`). Consume the key events to prevent Alt menu activation or other system behavior.

### Other-Modifier Guard

If other modifiers are held simultaneously (Shift, Control, Command/Win, or the other trigger key), the hotkey does NOT fire. This prevents stealing fn+arrows, Option+letter, etc.

---

## 5. Audio Recording

### Format

| Property | Value |
|----------|-------|
| Encoding | Linear PCM (uncompressed) |
| Bit depth | 16-bit integer |
| Endianness | Little-endian |
| Sample rate | Device default (typically 44100 or 48000 Hz) |
| Channels | Device default (typically 1 or 2) |
| Container | WAV |
| Temp file path | System temp directory, filename `yap_recording.wav` |

### Audio Engine

- macOS/Windows: Rust `cpal` capture with device-specific input stream configuration.

### Real-time Level Computation

**RMS level** (overall volume):
```
rms = sqrt(sum(sample[i]^2) / frameCount)
level = min(rms * 18.0, 1.0)
```
Reported to overlay as a single float 0.0-1.0.

### FFT Specification

| Property | Value |
|----------|-------|
| FFT size | 1024 points |
| Window function | Hann (normalized) |
| Frequency range | 80 Hz to min(8000, Nyquist) Hz |
| Band count | 6 logarithmic bands |
| Band distribution | Log2-spaced from 80 Hz to 8000 Hz |
| Display bars | 11 (6 raw bands mirrored) |

**Band computation**:
1. Apply Hann window to first 1024 samples.
2. Forward real FFT (split complex).
3. Compute magnitudes (`zvmags`).
4. Divide frequency range [80, 8000] Hz into 6 log2-spaced bands.
5. Average magnitudes within each band's bin range.
6. Normalize by peak band value (relative distribution).
7. Compute overall volume gate: `volume = min(pow(rms * 18.0, 0.6), 1.0)`.
8. Multiply each band by volume.

**Mirror mapping** (6 raw bands -> 11 display bars):

```
bar[0]  = raw[5]*0.5  + raw[4]*0.3  + raw[3]*0.2
bar[1]  = raw[4]*0.5  + raw[3]*0.3  + raw[5]*0.2
bar[2]  = raw[3]*0.6  + raw[2]*0.25 + raw[4]*0.15
bar[3]  = raw[2]*0.7  + raw[1]*0.2  + raw[3]*0.1
bar[4]  = raw[1]*0.8  + raw[0]*0.15 + raw[2]*0.05
bar[5]  = raw[0]                                     (center)
bar[6]  = raw[1]*0.85 + raw[0]*0.1  + raw[2]*0.05
bar[7]  = raw[2]*0.7  + raw[1]*0.2  + raw[3]*0.1
bar[8]  = raw[3]*0.6  + raw[2]*0.25 + raw[4]*0.15
bar[9]  = raw[4]*0.5  + raw[3]*0.3  + raw[5]*0.2
bar[10] = raw[5]*0.5  + raw[4]*0.3  + raw[3]*0.2
```

### Pause/Resume (Hands-free)

When paused, the audio engine stays running (levels still update) but samples are NOT written to the file. Resume seamlessly stitches new audio to the existing file.

### Silence and Duration Thresholds

| Check | Threshold | Consequence |
|-------|-----------|-------------|
| Too-short recording | duration < 0.5s AND peak < 0.15 | Cancel, show `holdTip` |
| Silence detected | peak audio level < 0.15 | Skip transcription, show `speakTip` |

Peak audio level is the maximum RMS*18 value seen during the recording (the `level` value sent to `onLevelUpdate`). The 0.15 threshold means approximately quiet speech on the raw RMS scale.

---

## 6. Transcription Providers

### Provider: None (Apple Speech / Windows Speech)

- macOS: `SFSpeechRecognizer` with locale `en-US`. `shouldReportPartialResults = false`. On macOS 13+, `addsPunctuation = true`.
- Windows: `System.Speech.Recognition.SpeechRecognitionEngine` or `Windows.Media.SpeechRecognition`.
- Used as: (a) primary transcriber when no API is configured, (b) pre-check before API calls to verify speech exists.

### Provider: Gemini

| Field | Value |
|-------|-------|
| Endpoint | `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={apiKey}` |
| Auth | API key in query string |
| Default model | `gemini-2.5-flash` |
| Method | POST |
| Content-Type | `application/json` |
| Can one-shot | **Yes** -- only provider that can transcribe + format in a single call |

**Request body**:
```json
{
  "contents": [{
    "parts": [
      {"inline_data": {"mime_type": "audio/wav", "data": "<base64-encoded-audio>"}},
      {"text": "<prompt>"}
    ]
  }],
  "generationConfig": {
    "temperature": <geminiTemperature, default 0.0>,
    "maxOutputTokens": 2048,
    "responseMimeType": "application/json"
  }
}
```

**Response parsing**:
1. Check `candidates[0].finishReason == "STOP"`. If not, treat as `truncatedResponse` error (retryable).
2. Extract `candidates[0].content.parts[0].text`.
3. Parse as JSON, extract `"text"` field.
4. Error path: check `error.message` field.

**Provider-specific options**:
- `geminiTemperature`: Double, 0.0-1.0, default 0.0.

### Provider: OpenAI

| Field | Value |
|-------|-------|
| Endpoint | `https://api.openai.com/v1/audio/transcriptions` |
| Auth | `Authorization: Bearer {apiKey}` |
| Default model | `gpt-4o-transcribe` |
| Method | POST |
| Content-Type | `multipart/form-data` |

**Request**: multipart form with fields:
- `file`: audio WAV data, filename `recording.wav`, content-type `audio/wav`
- `model`: model name
- `language` (optional): ISO 639-1 code
- `prompt` (optional): context string

**Response parsing**:
1. JSON object with `"text"` field.
2. Error: `error.message` field.

**Provider-specific options**:
- `oaiLanguage`: String, ISO 639-1 code, default empty (auto-detect).
- `oaiPrompt`: String, context prompt, default empty.

### Provider: Deepgram

| Field | Value |
|-------|-------|
| Endpoint | `https://api.deepgram.com/v1/listen?model={model}&smart_format={bool}&language={lang}&keywords={kw}` |
| Auth | `Authorization: Token {apiKey}` |
| Default model | `nova-3` |
| Method | POST |
| Content-Type | `audio/wav` |

**Request**: Raw audio bytes in body. Parameters in query string.

**Response parsing**:
1. `results.channels[0].alternatives[0].transcript`
2. Error: `err_msg` field.

**Provider-specific options**:
- `dgSmartFormat`: Bool, default true. Adds `smart_format=true` to query.
- `dgLanguage`: String, ISO 639-1 code, default empty.
- `dgKeywords`: Comma-separated string, each keyword URL-encoded and added as `keywords=` param.

### Provider: ElevenLabs

| Field | Value |
|-------|-------|
| Endpoint | `https://api.elevenlabs.io/v1/speech-to-text` |
| Auth | `xi-api-key: {apiKey}` |
| Default model | `scribe_v1` |
| Method | POST |
| Content-Type | `multipart/form-data` |

**Request**: multipart form with fields:
- `file`: audio WAV data, filename `recording.wav`, content-type `audio/wav`
- `model_id`: model name
- `language_code` (optional): ISO 639-1 code

**Response parsing**:
1. JSON object with `"text"` field.
2. Error: `detail.message` field.

**Provider-specific options**:
- `elLanguageCode`: String, ISO 639-1 code, default empty.

### Retry Logic (All Providers)

- Max retries: **2** (total of up to 3 attempts).
- Retryable errors: `truncatedResponse`, `noResponse`, `parseFailed`, `NSURLErrorTimedOut`, `NSURLErrorNetworkConnectionLost`.
- Backoff: `attempt * 0.5` seconds between retries.

### Timeout Scaling

```
estimatedSeconds = audioData.count / 64000  (conservative middle estimate for PCM WAV)
timeout = max(30.0, 30.0 + estimatedSeconds)
```

### Apple Speech Pre-check

Before sending audio to any API provider, run a quick Apple Speech / Windows Speech on-device transcription. If the result is empty or an error, skip the API call entirely and show `speakTip`. This saves API costs on silence/noise recordings.

---

## 7. Formatting Providers

All formatters receive already-transcribed text and clean it up according to a style. Input is wrapped as `<input>{text}</input>`. All must respond with JSON: `{"text":"cleaned version"}`.

### Minimum Input Length

Text shorter than 3 characters is passed through unformatted.

### Provider: Gemini (Formatting)

| Field | Value |
|-------|-------|
| Endpoint | `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={apiKey}` |
| Auth | API key in query string |
| Default model | `gemini-2.5-flash` |
| Timeout | 15 seconds |
| Temperature | 0.0 |
| maxOutputTokens | 2048 |

**Request body**:
```json
{
  "contents": [{
    "parts": [{"text": "{style.prompt}\n\n<input>{text}</input>"}]
  }],
  "generationConfig": {
    "temperature": 0.0,
    "maxOutputTokens": 2048,
    "responseMimeType": "application/json"
  }
}
```

### Provider: OpenAI (Formatting)

| Field | Value |
|-------|-------|
| Endpoint | `https://api.openai.com/v1/chat/completions` |
| Auth | `Authorization: Bearer {apiKey}` |
| Default model | `gpt-4o-mini` |
| Timeout | 15 seconds |
| Temperature | 0.3 |
| max_tokens | 2048 |

**Request body**:
```json
{
  "model": "{model}",
  "messages": [
    {"role": "system", "content": "{style.prompt}"},
    {"role": "user", "content": "<input>{text}</input>"}
  ],
  "max_tokens": 2048,
  "temperature": 0.3
}
```

**Response**: `choices[0].message.content` -- parse JSON, extract `"text"`.

### Provider: Anthropic

| Field | Value |
|-------|-------|
| Endpoint | `https://api.anthropic.com/v1/messages` |
| Auth | `x-api-key: {apiKey}` |
| Required header | `anthropic-version: 2023-06-01` |
| Default model | `claude-haiku-4-5-20251001` |
| Timeout | 15 seconds |
| Temperature | 0.0 |
| max_tokens | 2048 |

**Request body** (note: assistant prefill forces JSON output):
```json
{
  "model": "{model}",
  "system": "{style.prompt}",
  "messages": [
    {"role": "user", "content": "<input>{text}</input>"},
    {"role": "assistant", "content": "{"}
  ],
  "max_tokens": 2048,
  "temperature": 0.0,
  "stop_sequences": ["}"]
}
```

**Response parsing**: `content[0].text` gives the interior of the JSON object (without outer braces). Prepend `{` and append `}`, then parse `{"text":"..."}`.

### Provider: Groq

| Field | Value |
|-------|-------|
| Endpoint | `https://api.groq.com/openai/v1/chat/completions` |
| Auth | `Authorization: Bearer {apiKey}` |
| Default model | `llama-3.3-70b-versatile` |
| Timeout | 10 seconds |
| Temperature | 0.3 |
| max_tokens | 2048 |

**Request body**: Same structure as OpenAI (chat completions API).

**Response**: Same parsing as OpenAI.

### Formatting Styles

Three styles with distinct prompts:

#### Casual

**Text formatting prompt**:
```
You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"}
Rules: remove ONLY filler sounds (um, uh, er). Keep everything else exactly as spoken --
casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation.
PRESERVE all existing symbols -- parentheses, quotes, brackets, etc.
Convert spoken punctuation commands to symbols (e.g. "period" -> ., "open parenthesis" -> (, "comma" -> ,).
NEVER respond conversationally. ONLY output the JSON object.
```

#### Formatted

**Text formatting prompt**:
```
You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"}
Rules: remove filler words (um, uh, er, like, you know). Fix punctuation and capitalization.
Keep the speaker's EXACT words and sentence structure -- do not rephrase or rewrite.
Keep contractions as spoken. Only fix obvious grammar errors.
PRESERVE all existing symbols -- parentheses, quotes, brackets, etc.
Convert spoken punctuation commands to symbols (e.g. "period" -> ., "open parenthesis" -> (, "comma" -> ,).
NEVER respond conversationally. ONLY output the JSON object.
```

#### Professional

**Text formatting prompt**:
```
You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"}
Rules: remove all filler words. Elevate the language to sound polished and professional.
Fix grammar, improve word choice, use proper punctuation and capitalization.
Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning.
PRESERVE all existing symbols -- parentheses, quotes, brackets, etc.
Convert spoken punctuation commands to symbols (e.g. "period" -> ., "open parenthesis" -> (, "comma" -> ,).
NEVER respond conversationally. ONLY output the JSON object.
```

### Audio Transcription Prompts (Gemini One-Shot)

Each style also has an `audioPrompt` for combined transcription + formatting. These prompts include the shared dictation commands block and noise rejection rule.

**Shared dictation commands block** (included in all audio prompts):
```
DICTATION COMMANDS -- when the speaker says any of these, insert the symbol instead of the words:
"period" or "full stop" -> . | "comma" -> , | "question mark" -> ? | "exclamation mark" or "exclamation point" -> !
"colon" -> : | "semicolon" -> ; | "open parenthesis" or "open paren" -> ( | "close parenthesis" or "close paren" -> )
"open bracket" -> [ | "close bracket" -> ] | "open brace" or "open curly" -> { | "close brace" or "close curly" -> }
"open quote" or "open quotes" -> " | "close quote" or "close quotes" or "end quote" -> "
"dash" or "em dash" -> -- | "hyphen" -> - | "ellipsis" or "dot dot dot" -> ...
"new line" or "newline" -> insert a line break | "new paragraph" -> insert two line breaks
"ampersand" -> & | "at sign" -> @ | "hashtag" or "hash" -> # | "dollar sign" -> $ | "percent" or "percent sign" -> %
"asterisk" or "star" -> * | "slash" or "forward slash" -> / | "backslash" -> \
"underscore" -> _ | "pipe" -> | | "tilde" -> ~ | "caret" -> ^
Only convert these when the speaker clearly intends them as punctuation commands, not when used naturally in speech.
```

**Shared noise rule** (included in all audio prompts):
```
IGNORE all background noise, sound effects, music, and non-speech sounds.
Only transcribe human speech. If there is no speech, respond with {"text":""}.
```

**Plain transcription prompt** (no formatting, for two-step flow):
```
Transcribe this audio exactly as spoken, with proper punctuation and capitalization.
{dictation commands}
{noise rule}
You MUST respond with ONLY a JSON object: {"text":"transcription here"}
```

### JSON Extraction

All provider responses go through a JSON extraction function:
1. Strip markdown code fences (` ```json `, ` ``` `).
2. Try direct JSON parse for `{"text":"..."}`.
3. If that fails, find first `{` and last `}`, try parsing that slice.
4. If all parsing fails, return raw string.

### Prompt Regurgitation Guard

After transcription/formatting, check the result (lowercased) for these substrings:
- `"transcribe this audio"`
- `"respond with only a json"`
- `"dictation commands"`

If any match, discard the result entirely. This catches cases where the model echoes back the system prompt instead of transcribing.

### Fallback Behavior

- If formatting fails, paste raw transcription text (do not show error).
- If Gemini formatting returns non-STOP finishReason, fall back to raw text.

---

## 8. Overlay UI

### Window Properties

| Property | macOS | Windows |
|----------|-------|---------|
| Type | NSPanel sidecar (borderless, nonactivatingPanel) | Win32 layered window |
| Level | Floating | Topmost |
| Click-through | Yes (except pill region and buttons) | Yes (WS_EX_TRANSPARENT except hit regions) |
| Background | Transparent | Transparent |
| Shadow | None at window level (SwiftUI handles it) | None at window level |
| Spaces/Desktops | All spaces, full-screen auxiliary, stationary | All virtual desktops |
| Size | 1400 x 700 pt | 1400 x 700 dp |
| Position | Centered horizontally, bottom of screen, pill center ~330pt from bottom | Same proportional positioning |

### Overlay Modes

| Mode | Visual |
|------|--------|
| `idle` | Minimized pill (scale 0.5, or 0.65 on hover). 11 static low bars at opacity 0.25. Shows mic icon on hover. |
| `recording` | Full-size pill. 11 FFT-reactive bars. Audio bounce effect: `scale = 1.0 + pow(level, 1.5) * 0.25`. |
| `processing` | Pill at 0.8 scale. Gaussian wave sweep across 11 bars. Sweep cycle: 1.2s. Shimmer opacity: 0.35 (dim) to 0.95 (bright). |
| `noSpeech` | Same as idle bars (all low, 0.25 opacity). Triggers shake animation. |
| `error(message)` | Warning triangle icon + message text. Auto-dismiss after 2.0 seconds. |

### Bar Visualizer

- 11 bars, each 3pt wide, 1.5pt corner radius, 2pt spacing.
- Height range: 5pt (min) to 28pt (max).
- Position scaling (center emphasis): `[0.35, 0.45, 0.6, 0.78, 0.92, 1.0, 0.94, 0.8, 0.63, 0.48, 0.38]`
- Recording: bars are audio-reactive with spring animation (stiffness 280, damping 18).
- Processing: gaussian wave `exp(-distance^2 / 6.0)` sweeps left to right in 1.2s cycles.

### Pill Shape

- Capsule shape (fully rounded ends).
- Background: `black 0.75 opacity` + `thinMaterial` blur, overlaid.
- Border: `white 0.3 opacity`, 1pt width (expanded) or `white 0.35 opacity`, 1.5pt width (minimized).
- Shadow: `16pt radius, 4pt y-offset, black 0.35` (expanded) or `6pt radius, 2pt y-offset, black 0.1` (minimized).
- Padding: 12pt horizontal, 6pt vertical (standard); 7pt horizontal when hands-free.

### Hands-Free UI

When hands-free mode is active:
- Pill width expands from 52pt to 124pt content area.
- Pause/play button: 26x26pt circle, `white 0.15 opacity` background, positioned 49pt left of center.
- Stop button: 26x26pt circle, `red 0.85 opacity` background, positioned 49pt right of center.
- Buttons animate in with scale (0.001 to 1.0) and opacity (0 to 1).
- When paused, bars show static low state (0.25 opacity, 5pt height).

### Elapsed Timer (Hands-Free)

- Appears after **10 seconds** of recording.
- Font: system monospaced, 11pt, medium weight.
- Color: white at 0.5 opacity.
- Format: `M:SS` (e.g., `0:15`, `1:30`).
- Updates every 0.5 seconds.
- Pausing stops the timer; resuming continues from where it left off.

### Slide Animation

- **Slide in**: 0.5s, timing curve `(0.16, 1, 0.3, 1)` (ease-out).
- **Slide out**: 0.4s, timing curve `(0.4, 0, 1, 1)` (ease-in).
- Only slides if `alwaysVisible` is false; otherwise stays in place.

### Shake Animation

- Horizontal oscillation: `10 * sin(progress * pi * 6) * (1 - progress)`.
- Duration: 0.5s ease-out.
- Triggered on: no-speech detection, onboarding hold released too early.

### Lava Lamp Background

- 4 colored ellipses (purple, blue, cyan, indigo) with blur radius 55pt.
- Lissajous drift paths at different speeds.
- Energy levels: recording=1.0, processing=0.6, onboarding=0.3, idle=0.4, hovering=0.15.
- Brightness: `0.25 + energy * 0.25`.
- Can be disabled via `gradientEnabled` setting.

### Always-Visible Mode

When `alwaysVisiblePill` is true, the pill remains on screen in idle state (minimized). When false, the pill slides off-screen when idle.

### Click-to-Record

- Clicking the pill in idle state starts hands-free recording.
- Clicking during hold-to-record converts to hands-free.
- Clicking during hands-free stops and restarts a new session.
- Hovering over minimized pill shows tooltip: "Click to start transcribing".
- Hover pill scale: 0.65 (vs 0.5 normal minimized).

---

## 9. System Tray / Menu Bar

### Menu Items

| Item | Shortcut | Behavior |
|------|----------|----------|
| "Yap" (title) | -- | Disabled label |
| separator | -- | -- |
| "Enabled" | Cmd+E / Ctrl+E | Toggle, checkmark state. Controls `isEnabled` flag. |
| "History" (submenu) | -- | Submenu with recent entries, "Show All...", "Clear History" |
| "Settings..." | Cmd+, / Ctrl+, | Opens settings window |
| separator | -- | -- |
| "Quit" | Cmd+Q / Ctrl+Q | Terminates app |

### History Submenu

- Shows up to 10 most recent entries, truncated to 60 characters with ellipsis.
- Clicking an entry copies its full text to clipboard.
- "Show All..." opens the History window.
- "Clear History" clears all entries (disabled when empty).
- Menu is rebuilt each time it opens.

### Icon States

| State | macOS SF Symbol | Windows equivalent |
|-------|-----------------|-------------------|
| idle | `mic` (or custom `MenuIconTemplate.png`, 14x14pt template) | Mic outline icon |
| recording / handsFree / paused | `mic.fill` | Filled mic icon |
| processing | `ellipsis.circle` | Ellipsis circle icon |

---

## 10. Settings

### All Settings Keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `hotkey` | String | `"fn"` | `"fn"` or `"option"` |
| `txProvider` | String | `"none"` | Transcription provider: `none`, `gemini`, `openai`, `deepgram`, `elevenlabs` |
| `txApiKey` | String | `""` | Transcription API key |
| `txModel` | String | `""` | Transcription model (empty = provider default) |
| `fmtProvider` | String | `"none"` | Formatting provider: `none`, `gemini`, `openai`, `anthropic`, `groq` |
| `fmtApiKey` | String | `""` | Formatting API key (empty = use txApiKey if same provider) |
| `fmtModel` | String | `""` | Formatting model (empty = provider default) |
| `fmtStyle` | String | `"formatted"` | `"casual"`, `"formatted"`, `"professional"` |
| `onboardingComplete` | Bool | `false` | Whether onboarding has been completed |
| `dgSmartFormat` | Bool | `true` | Deepgram: enable smart formatting |
| `dgKeywords` | String | `""` | Deepgram: comma-separated boost keywords |
| `dgLanguage` | String | `""` | Deepgram: ISO 639-1 language code |
| `oaiLanguage` | String | `""` | OpenAI: ISO 639-1 language code |
| `oaiPrompt` | String | `""` | OpenAI: context prompt |
| `geminiTemperature` | Double | `0.0` | Gemini: temperature (0.0-1.0) |
| `elLanguageCode` | String | `""` | ElevenLabs: ISO 639-1 language code |
| `soundsEnabled` | Bool | `true` | Play sound effects |
| `gradientEnabled` | Bool | `true` | Show lava lamp gradient background |
| `alwaysVisiblePill` | Bool | `true` | Keep pill visible when idle |
| `historyEnabled` | Bool | `true` | Save transcription history to disk |

### API Key Sharing

When transcription and formatting use the same backend (Gemini-Gemini or OpenAI-OpenAI), the formatting API key can be left empty to reuse the transcription key. A "Use same API key" toggle controls this in the UI.

### Settings Window

- Size: 500 x 680 pt/dp.
- Sections: General (hotkey), Transcription, Formatting, Appearance, History.
- Save button persists all values and triggers `settingsDidChange()`.
- Cancel button discards changes.
- "Reset Onboarding" button clears `onboardingComplete`.

---

## 11. Config Storage

### macOS

- **Primary storage**: `UserDefaults.standard` (NSUserDefaults).
- **History file**: `~/.config/yap/history.json`
- **Debug log**: `~/.config/yap/debug.log`

### Windows

- **Primary storage**: `%APPDATA%\yap\config.json`
- **History file**: `%APPDATA%\yap\history.json`
- **Debug log**: `%APPDATA%\yap\debug.log`

### Config JSON Format (Windows)

```json
{
  "hotkey": "fn",
  "txProvider": "none",
  "txApiKey": "",
  "txModel": "",
  "fmtProvider": "none",
  "fmtApiKey": "",
  "fmtModel": "",
  "fmtStyle": "formatted",
  "onboardingComplete": false,
  "dgSmartFormat": true,
  "dgKeywords": "",
  "dgLanguage": "",
  "oaiLanguage": "",
  "oaiPrompt": "",
  "geminiTemperature": 0.0,
  "elLanguageCode": "",
  "soundsEnabled": true,
  "gradientEnabled": true,
  "alwaysVisiblePill": true,
  "historyEnabled": true
}
```

---

## 12. History

### Data Model

```json
{
  "id": "uuid-string",
  "timestamp": "ISO-8601 date",
  "text": "transcribed and/or formatted text",
  "transcriptionProvider": "gemini|openai|deepgram|elevenlabs|apple",
  "formattingProvider": "gemini|openai|anthropic|groq|null",
  "formattingStyle": "casual|formatted|professional|null"
}
```

### Behavior

- **Max entries**: 10. When exceeding 10, oldest entries are dropped.
- **Storage**: `history.json` -- JSON array of `HistoryEntry` objects.
- **Insert order**: newest first (prepend).
- **Gated by**: `historyEnabled` setting. When disabled, `append()` is a no-op.
- **Clear**: Replaces file with empty array.

### History Window

- Size: 480 x 400 pt/dp.
- Each row shows: text (3-line limit), relative timestamp, provider label, "Copy" button.
- Relative time format: "just now" (<60s), "N min ago" (<1h), "Nh ago" (<24h), then short date+time.

---

## 13. Paste Behavior

### Sequence

1. **Save** current clipboard contents (string type).
2. **Clear** clipboard, **set** transcribed/formatted text.
3. **Wait 50ms** for clipboard to be ready.
4. **Simulate Cmd+V** (macOS) or **Ctrl+V** (Windows) using synthetic keyboard events.
   - macOS: `CGEvent` with virtual keycode `0x09` (V key), `.maskCommand` flag.
   - Windows: `SendInput` with `VK_CONTROL` + `VK_V`.
5. **Wait 300ms** for paste to complete.
6. **Restore** previous clipboard contents (clear + set previous string, or leave empty if none).

### Platform Notes

- macOS: Posts events at `cgAnnotatedSessionEventTap` level.
- Windows: Uses `SendInput` API for reliable keystroke simulation.
- Both platforms must handle the case where previous clipboard was empty (restore to empty).

---

## 14. Onboarding

### Steps (in order)

| Step | Card Text | Input Allowed |
|------|-----------|---------------|
| `tryIt` | "Hold {fn/option} and speak -- Yap transcribes it" | fn hold starts recording |
| `nice(next)` | Random celebration message (e.g., "Nice! 🎉") | No input (auto-advances after 1.5s) |
| `doubleTapTip` | "Double-tap {fn/option} for hands-free transcription" | Double-tap only (fn hold blocked) |
| `nice(next)` | Random celebration message | Auto-advances |
| `clickTip` | "Click the pill for hands-free transcription" | Pill click only (fn hold and double-tap blocked) |
| `nice(next)` | Random celebration message | Auto-advances |
| `apiTip` | "Add an API key in the menu bar for better transcription" | Hold-to-confirm (0.6s hold advances) |
| `formattingTip` | "Enable formatting in Settings to clean up grammar and punctuation automatically" | Hold-to-confirm |
| `welcome` | "You're all set -- enjoy! 🎉" | Hold-to-confirm |

### Nice Messages Pool

```
"Nice! 🎉", "Nailed it! ✨", "Sounds good! 👌",
"Got it! 🙌", "Perfect! 🎯", "Love it! 💫"
```

### Transient Tips

| Tip | Trigger | Card Text | Auto-dismiss |
|-----|---------|-----------|--------------|
| `speakTip` | Silence detected or no speech | "Didn't catch that -- speak up while holding {fn}" | 2.5 seconds |
| `holdTip` | Recording too short | "Hold {fn} -- don't just tap it" | 2.5 seconds |

After auto-dismiss, the overlay restores to whichever onboarding step was active before the tip appeared.

### Hold-to-Confirm Behavior

For `apiTip`, `formattingTip`, and `welcome` steps:
1. On key-down: show press-down animation (scale 0.85, opacity 0.7).
2. Start 0.6s timer.
3. If key released before 0.6s: cancel, play shake animation.
4. If held for 0.6s: release animation, play "Pop" sound, wait 0.4s, advance to next step.

### Completion

After `welcome` step is confirmed, set `onboardingComplete = true` and hide onboarding UI. Pill returns to normal idle behavior.

---

## 15. Sound Effects

### Sound Map

| Event | Sound | File |
|-------|-------|------|
| Recording starts | "Blow" | `Blow.aiff` |
| Recording stops / processing begins | "Pop" | `Pop.aiff` |
| Onboarding hold-to-confirm success | "Pop" | `Pop.aiff` |
| Onboarding step completion (nice) | "Submarine" | `Submarine.aiff` |
| Short-tap cancel (holdTip) | "Pop" | `Pop.aiff` |

### Preloading

All three sounds (`Pop`, `Blow`, `Submarine`) are preloaded at app launch with `prepareToPlay()` for zero-latency playback.

### Timing

- Recording start sound is delayed by **0.1 seconds** after `engine.start()` to let hardware settle.

### Gating

Sounds are gated by the `soundsEnabled` setting. When false, `playSound()` is a no-op.

### Platform Notes

- macOS: System AIFF sounds from the app bundle via `AVAudioPlayer`.
- Windows: WAV equivalents bundled with the app, played via `SoundPlayer` or `MediaPlayer`.

---

## 16. Permissions

### macOS

| Permission | API | Purpose |
|------------|-----|---------|
| Microphone | `AVCaptureDevice.requestAccess(for: .audio)` | Audio recording |
| Speech Recognition | `SFSpeechRecognizer.requestAuthorization` | On-device transcription and pre-check |
| Accessibility | System Settings > Privacy & Security > Accessibility | CGEventTap for hotkey monitoring and Cmd+V simulation |

### Windows

| Permission | API | Purpose |
|------------|-----|---------|
| Microphone | UWP capability or user consent dialog | Audio recording |
| Speech Recognition | Windows Speech runtime | On-device transcription |
| UI Automation / Input simulation | No special permission needed (SendInput works without elevation) | Ctrl+V simulation |
| Low-level keyboard hook | No special permission | Hotkey monitoring |

---

## 17. Logging

### Log Function

All logging goes through a single global `log()` function that:
1. Writes to OS-level structured logging (macOS: `os_log`, Windows: ETW or Debug output).
2. Appends timestamped line to `debug.log` file.

### Log File Format

```
[ISO-8601-timestamp] Message text
```

### Log File Location

- macOS: `~/.config/yap/debug.log`
- Windows: `%APPDATA%\yap\debug.log`

---

## 18. Error Display

### Error Classification

| Condition | Message shown |
|-----------|---------------|
| Rate limited (response contains "quota", "rate", "429") | "Rate limited -- try again" |
| Auth failure (contains "auth", "key", "401", "403") | "Invalid API key" |
| Other API error | "API error" |
| Truncated response | "Response cut off -- try again" |
| Timeout | "Request timed out" |
| Network error | "No internet connection" |
| Unknown | "Something went wrong" |

### Error Overlay

- Shows warning triangle icon (red) + message text in the pill.
- Auto-dismisses after **2.0 seconds**.
- After dismiss, restores onboarding state if applicable (after additional 0.5s).

---

## 19. Multipart Form Data

For providers that require `multipart/form-data` (OpenAI, ElevenLabs):

```
--{boundary}\r\n
Content-Disposition: form-data; name="file"; filename="recording.wav"\r\n
Content-Type: audio/wav\r\n
\r\n
{audio bytes}
\r\n
--{boundary}\r\n
Content-Disposition: form-data; name="{field_name}"\r\n
\r\n
{field_value}\r\n
--{boundary}--\r\n
```

Boundary is a UUID string.
