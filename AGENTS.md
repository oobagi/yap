# AGENTS.md

This file gives Codex and other coding agents repository context for Yap.

## Build & Run

```bash
cd desktop
pnpm install
pnpm run check
pnpm run electron:check
pnpm run electron:dev
pnpm run electron:package
pnpm run electron:package:ci
```

User installation should point to GitHub Releases first. These commands are for development and source builds.
Use `electron:package:ci` for CI/tag packaging because it explicitly disables Electron Builder publication; release artifacts are published by the GitHub Actions release job.

The canonical application lives in `desktop/`. Electron is the desktop shell. Swift is used only for the macOS overlay sidecar under `desktop/native-core/sidecar-overlay/`.

Rust native code lives under `desktop/native-core/`. Electron launches the `yap-core` binary from that crate over newline-delimited JSON-RPC.

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

- `desktop/electron/` - Electron main process, preload bridge, tray, windows, IPC, updater, and native sidecar management.
- `desktop/electron-builder.yml` - Electron bundle, native resource, and updater artifact configuration.
- `desktop/native-core/src/yap_core.rs` - Electron native runtime entry point.
- `desktop/native-core/src/commands.rs` - runtime command facade shared by Electron-side native commands.
- `desktop/native-core/src/dictation.rs` - Electron-backed native state machine and pipeline coordination.
- `desktop/native-core/src/audio.rs` - CPAL audio capture, WAV writing, audio levels, and FFT bars.
- `desktop/native-core/src/hotkey.rs` - global hotkey handling.
- `desktop/native-core/src/transcription.rs` - Apple/on-device pre-checks and API transcription providers.
- `desktop/native-core/src/formatting.rs` - LLM cleanup and style formatting.
- `desktop/native-core/src/paste.rs` - clipboard write, paste simulation, and clipboard restore.
- `desktop/native-core/src/win_overlay.rs` - Windows native overlay implementation.
- `desktop/native-core/src/sidecar.rs` - macOS overlay sidecar process management.
- `desktop/native-core/sidecar-overlay/` - Swift/AppKit overlay sidecar for macOS; Windows overlay rendering lives in Rust.
- `desktop/native-core/sounds/` - bundled WAV sound effects.
- `desktop/native-core/icons/` - app, Windows, macOS, and tray icons used by builds.
- `desktop/src/lib/settings/` - Svelte settings UI, transcription history, and update checks.

## Config

Config is stored at `~/.config/yap/config.json` on macOS and `%APPDATA%\yap\config.json` on Windows. Important fields include:

- `hotkey`
- `audioDevice`
- `pressEnterAfterPaste`
- `txProvider`, `txApiKey`, `txModel`
- `fmtProvider`, `fmtApiKey`, `fmtModel`, `fmtStyle`
- `soundsEnabled`, `backgroundAudioMode`
- `gradientEnabled`, `alwaysVisiblePill`
- `historyEnabled`, `speechLocale`
- provider-specific Deepgram, OpenAI, Gemini, and ElevenLabs options

Empty model strings fall back to provider defaults, except Local Whisper transcription and Ollama formatting, which require an explicit model selection. Formatting falls back to the transcription API key when its own API key is blank.

## Working Rules

- Keep cross-platform behavior in the Rust `yap-core` command/dictation runtime where possible.
- Use platform-specific code only for OS integration: hotkeys, overlay behavior, paste, speech, bundling, and permissions.
- Apple on-device transcription is macOS-only; Windows supports Local Whisper or an API transcription provider.

## Repo Skills

- `.codex/skills/yap-release-bump/` - use this bundled skill when preparing Yap version bumps or releases. It exists to keep releases on branch/PR flow, split changes into feature-grouped commits, choose the semantic version from the update contents, and produce GitHub Release notes in the current Yap release style.
