# VoiceType 🎙️

A macOS menu bar app that turns speech into text. Hold a key, talk, release — your words get transcribed, optionally formatted by AI, and pasted into whatever you're typing in. Like [Wispr Flow](https://wisprflow.com), but free and open source.

## How It Works

1. **Hold** the fn key (or Option)
2. **Speak** — a floating pill with a waveform shows you're recording
3. **Release** — audio is transcribed, formatted, and pasted at your cursor

That's it. Works in any app — text editors, browsers, chat apps, terminals.

## Transcription Providers

Choose what turns your audio into text:

| Provider | Accuracy (WER) | Cost | Notes |
|----------|----------------|------|-------|
| **Apple Dictation** | 16.5% | Free | On-device, no API key, works offline |
| **Google Gemini** | 6.7% | ~$0.14/hr | Can also format in one API call |
| **OpenAI** | 5.4% | ~$0.36/hr | GPT-4o Transcribe |
| **Deepgram** | 7.6% | ~$0.07/hr | Nova-3 |
| **ElevenLabs** | 6.8% | ~$0.30/hr | Scribe v1 |

All providers except Apple Dictation require an API key (bring your own).

## AI Formatting

Optionally clean up transcriptions with AI:

- **Casual** — lowercase, keeps slang and contractions, minimal punctuation
- **Formatted** — proper caps and punctuation, keeps your exact words
- **Professional** — polished, formal language, expanded contractions

Formatting providers: **Gemini**, **OpenAI**, or **Anthropic**. Or skip formatting entirely and get raw transcription.

If you use Gemini for both transcription and formatting, it handles everything in a single API call.

## Dictation Commands

Say punctuation out loud and it gets converted to symbols:

> "Open parenthesis hello close parenthesis period" → `(hello).`

Supports: period, comma, question mark, exclamation, parentheses, brackets, braces, quotes, dashes, ellipsis, new line, new paragraph, and common symbols.

## Install

```bash
git clone https://github.com/igaboo/voicetype.git
cd voicetype
chmod +x build.sh && ./build.sh
cp -r build/VoiceType.app /Applications/
open /Applications/VoiceType.app
```

Requires macOS 12+ and Xcode Command Line Tools (`xcode-select --install`).

First launch will prompt for **Microphone**, **Speech Recognition**, and **Accessibility** permissions.

> **Important:** Launch with `open /Applications/VoiceType.app`, not by running the binary directly. Running the binary makes macOS attribute Accessibility permissions to Terminal instead of VoiceType.

## Settings

Click the menu bar icon → **Settings** (⌘,):

- **Hotkey** — fn/Globe or Option
- **Transcription** — provider, API key, model
- **Formatting** — provider, API key, model, style

Config is stored at `~/.config/voicetype/config.json`. Debug logs at `~/.config/voicetype/debug.log`.

## Update

```bash
cd voicetype && git pull
./build.sh && cp -r build/VoiceType.app /Applications/
pkill -f VoiceType; open /Applications/VoiceType.app
```

## Technical Details

- Pure Swift — no external dependencies, just system frameworks
- ~1,800 lines across 9 source files
- Ad-hoc code signed (no certificate needed)
- Silence detection skips API calls on accidental taps
- Errors display in the floating overlay pill with auto-dismiss
- Clipboard-swap paste: saves clipboard → sets text → Cmd+V → restores clipboard

## License

MIT
