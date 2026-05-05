# AGENTS.md

This file gives Codex and other coding agents repository context for Yap.

## Build & Run

```bash
cd tauri-app
npm install
npm run check
npm run tauri -- dev
npm run tauri -- build
```

User installation should point to GitHub Releases first. These commands are for development and source builds.

The canonical application lives in `tauri-app/`. The old root-level Swift package has been removed; Swift remains only as the macOS overlay sidecar under `tauri-app/src-tauri/sidecar-overlay/`.

Runtime permissions:

- macOS: Microphone, Speech Recognition, and Accessibility.
- Windows: Microphone access.

## Architecture

Yap is a cross-platform tray/menu bar dictation app. It records speech from a global hotkey, transcribes it, optionally formats it with an LLM, and pastes the result into the active app.

```
Hotkey provider
  -> Audio recorder
  -> Overlay pill
  -> Transcription provider
  -> Optional formatter
  -> Clipboard paste manager
```

## Key Paths

- `tauri-app/src-tauri/src/main.rs` - Tauri binary entry point.
- `tauri-app/src-tauri/src/lib.rs` - Tauri builder, commands, plugins, and setup.
- `tauri-app/src-tauri/src/orchestrator.rs` - state machine and pipeline coordination.
- `tauri-app/src-tauri/src/audio.rs` - CPAL audio capture, WAV writing, audio levels, and FFT bars.
- `tauri-app/src-tauri/src/hotkey.rs` - global hotkey handling.
- `tauri-app/src-tauri/src/transcription.rs` - Apple/on-device pre-checks and API transcription providers.
- `tauri-app/src-tauri/src/formatting.rs` - LLM cleanup and style formatting.
- `tauri-app/src-tauri/src/paste.rs` - clipboard write, paste simulation, and clipboard restore.
- `tauri-app/src-tauri/src/tray.rs` - tray/menu bar icon and menu.
- `tauri-app/src-tauri/src/win_overlay.rs` - Windows native overlay implementation.
- `tauri-app/src-tauri/src/sidecar.rs` - macOS overlay sidecar process management.
- `tauri-app/src-tauri/sidecar-overlay/` - Swift/AppKit overlay sidecar for macOS.
- `tauri-app/src/lib/settings/` - Svelte settings UI.
- `tauri-app/src/lib/history/` - Svelte history UI.
- `tauri-app/src/lib/overlay/` - Svelte overlay view used where needed.

## Config

Config is stored at `~/.config/yap/config.json`. Important fields include:

- `hotkey`
- `audio_device`
- `tx_provider`, `tx_api_key`, `tx_model`
- `fmt_provider`, `fmt_api_key`, `fmt_model`, `fmt_style`
- provider-specific Deepgram, OpenAI, Gemini, and ElevenLabs options

Empty model strings fall back to provider defaults. Formatting falls back to the transcription API key when its own API key is blank.

## Working Rules

- Keep cross-platform behavior in the Rust orchestrator where possible.
- Use platform-specific code only for OS integration: hotkeys, overlay behavior, paste, speech, bundling, and permissions.
- The app and tray icons used by builds live under `tauri-app/src-tauri/icons/`.
