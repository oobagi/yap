<p align="center">
  <img src="tauri-app/src-tauri/icons/icon.png" width="128" height="128" alt="Yap icon">
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
  <a href="https://github.com/oobagi/yap/actions/workflows/tauri-build.yml">Builds</a>
  &middot;
  <a href="https://github.com/oobagi/yap/blob/main/LICENSE">License</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/macOS-13%2B-blue" alt="macOS 13+">
  <img src="https://img.shields.io/badge/Windows-10%2B-blue" alt="Windows 10+">
  <img src="https://img.shields.io/badge/Tauri-2-24c8db" alt="Tauri 2">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="MIT">
</p>

---

## Install

Download the latest macOS or Windows installer from the repository's **Resources -> Releases** section:

https://github.com/oobagi/yap/releases/latest

## Features

- **Push-to-talk**: Hold fn/Globe or Option on macOS, or the configured Windows trigger, then release to paste.
- **Hands-free mode**: Double-tap the hotkey or click the floating pill to record without holding.
- **Native-feeling overlay**: A floating waveform pill with recording, processing, pause, resume, and error states.
- **Cross-platform shell**: Shared Tauri/Svelte UI with Rust orchestration and platform-native helpers where the OS needs them.
- **Transcription providers**: Apple on-device speech, Gemini, OpenAI, Deepgram, and ElevenLabs.
- **Formatting providers**: Gemini, OpenAI, Anthropic, and Groq with casual, formatted, and professional styles.
- **Clipboard preservation**: Writes the transcript, simulates paste, then restores the previous clipboard.

## Usage

Yap supports three recording modes:

| Mode | How to use |
|---|---|
| Hold to record | Hold the configured hotkey, speak, release |
| Hands-free double-tap | Double-tap the hotkey, speak, tap again to stop |
| Hands-free pill | Click the floating pill, speak, click Stop |

Short or silent recordings are discarded before paid APIs are called.

## Permissions

On macOS, grant these in System Settings -> Privacy & Security:

| Permission | Why it is needed |
|---|---|
| Microphone | Record your voice |
| Speech Recognition | On-device transcription and speech pre-checks |
| Accessibility | Detect the global hotkey and paste into other apps |

On Windows, allow microphone access.

## Configuration

Open the tray/menu bar icon -> **Settings**. Config is stored at `~/.config/yap/config.json`.

Empty model fields use provider defaults. Formatting can reuse the transcription API key when its own key is blank.

## Build From Source

```bash
git clone https://github.com/oobagi/yap.git
cd yap/tauri-app
npm install
npm run dev                 # frontend dev server
npm run check               # Svelte/type checks
npm run tauri -- dev        # run the desktop app in development
npm run tauri -- build      # create app bundles/installers
```

macOS builds require Xcode Command Line Tools for the Swift overlay sidecar. Windows builds require the standard Tauri Windows toolchain.

## Project Layout

| Path | Purpose |
|---|---|
| `tauri-app/src-tauri/src/` | Rust app orchestration, audio, hotkeys, tray, paste, providers, and Windows overlay |
| `tauri-app/src/` | Svelte settings, history, and overlay views |
| `tauri-app/src-tauri/sidecar-overlay/` | macOS Swift/AppKit overlay sidecar |
| `tauri-app/src-tauri/icons/` | App and tray icon assets used by Tauri |
| `tauri-app/src-tauri/sounds/` | Bundled app sounds |

## License

MIT
