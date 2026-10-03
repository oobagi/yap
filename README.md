<p align="center">
  <img src="desktop/native-core/icons/icon.png" width="128" height="128" alt="Yap icon">
</p>

<h1 align="center">Yap</h1>

<p align="center">
  Hold a key, speak, release. Yap transcribes your words and pastes them wherever you are typing.
</p>

<p align="center">
  <a href="https://github.com/oobagi/yap/releases/latest">Download</a>
  &middot;
  <a href="https://github.com/oobagi/yap/releases">Releases</a>
  &middot;
  <a href="https://github.com/oobagi/yap/issues">Issues</a>
  &middot;
  <a href="https://github.com/oobagi/yap/actions/workflows/electron-build.yml">Builds</a>
  &middot;
  <a href="https://github.com/oobagi/yap/blob/main/LICENSE">License</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/macOS-26%2B-blue" alt="macOS 26+">
  <img src="https://img.shields.io/badge/Windows-10%2B-blue" alt="Windows 10+">
  <img src="https://img.shields.io/badge/Electron-42-47848f" alt="Electron 42">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="MIT">
</p>

---

## Install

Download the latest macOS or Windows installer from GitHub Releases:

https://github.com/oobagi/yap/releases/latest

## Features

- Global push-to-talk dictation that pastes into the active app.
- Hands-free recording from the hotkey or floating pill.
- Local macOS 26+ SpeechAnalyzer transcription or cloud transcription with Gemini, OpenAI, Deepgram, and ElevenLabs.
- Optional cleanup styles for casual, formatted, professional, or custom text, with local or cloud AI.
- Local transcript history with original text, formatted output, and model/instruction details, plus in-app update checks.

## Set Up Transcription

Open the tray/menu bar icon, choose **Settings**, then pick a transcription provider. Windows supports Local Whisper or an API provider.

- On-device transcription (macOS 26+ only, no API key)
- **Local Whisper**: search Hugging Face to download and select a compatible GGML model, or enter an existing model path. A model selection is required; Yap does not choose a default. No API key is needed.
- [Gemini](https://ai.google.dev/gemini-api/docs/api-key)
- [OpenAI](https://platform.openai.com/api-keys)
- [Deepgram](https://developers.deepgram.com/docs/create-additional-api-keys)
- [ElevenLabs](https://elevenlabs.io/docs/api-reference/authentication)

For API providers, paste the provider key into **Settings -> Transcription -> API key**. Leave the model field blank to use Yap's default.

## Optional Formatting

Formatting can clean up the transcript after transcription. Choose **Casual**, **Formatted**, **Professional**, or **Custom**. Custom lets you supply instructions such as “make my dictation concise and professional.”

- **Apple On-device**: uses Apple Intelligence on an eligible Mac running macOS 26 or newer, with Apple Intelligence enabled and its model ready. No API key is needed.
- **Ollama**: install and run [Ollama](https://ollama.com), then use Yap's model manager to search the library, download, and select a local formatting model. A model selection is required; Yap does not choose a default. No API key is needed. Downloaded models are shared with your Ollama installation.

Cloud formatting providers:

- [Gemini](https://ai.google.dev/gemini-api/docs/api-key)
- [OpenAI](https://platform.openai.com/api-keys)
- [Anthropic](https://platform.claude.com/settings/keys)
- [Groq](https://console.groq.com/keys)

For cloud providers, paste the provider key into **Settings -> Formatting -> API key**. If formatting uses the same provider as transcription, Yap can reuse the transcription key. If a local formatter is unavailable, Yap pastes the original transcription.

Open an entry's **Details** in **History** to compare the original and formatted text and see the instruction used. Original text is unavailable when Gemini transcribes and formats in a single request. **Advanced -> Clean Cache** removes temporary audio and the debug log while dictation is idle; it keeps history and downloaded models.

## Config

Settings are stored in `~/.config/yap/config.json` on macOS and `%APPDATA%\yap\config.json` on Windows.
Most options can be changed from **Settings** in the tray/menu bar app.

```json
{
  "hotkey": "fn",
  "txProvider": "openai",
  "txApiKey": "",
  "txModel": "",
  "fmtProvider": "none",
  "fmtApiKey": "",
  "fmtModel": "",
  "fmtStyle": "formatted",
  "fmtCustomPrompt": "",
  "backgroundAudioMode": "mute",
  "alwaysVisiblePill": true,
  "historyEnabled": true
}
```

## Build From Source

```bash
git clone https://github.com/oobagi/yap.git
cd yap/desktop
pnpm install
pnpm run check               # Svelte/type checks
pnpm run electron:check      # Electron main/preload type checks
pnpm run electron:dev        # run the Electron desktop app in development
pnpm run electron:package    # create app bundles/installers
pnpm run electron:package:ci # CI packaging without Electron Builder publishing
```

macOS builds require Xcode Command Line Tools for the Swift/AppKit overlay sidecar. Windows builds require the standard Rust Windows/MSVC toolchain for the native `yap-core` runtime.

## License

MIT
