<p align="center">
  <img src="Resources/AppIcon.png" width="128" height="128" alt="Yap icon">
</p>

<h1 align="center">Yap</h1>

<p align="center">
  Hold a key, speak, release — your words are transcribed and pasted wherever you're typing.<br>
  Free and open source voice-to-text for macOS.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/macOS-13%2B-blue" alt="macOS 13+">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="MIT">
  <img src="https://img.shields.io/badge/Swift-5.9-orange" alt="Swift">
</p>

---

## Features

- **Push-to-talk**: Hold fn (Globe) or Option → speak → release to paste
- **Hands-free mode**: Double-tap the hotkey or click the floating pill to record without holding
- **Real-time waveform**: FFT-reactive visualization while recording, gaussian sweep while processing
- **5 transcription providers**: Apple on-device (free), Gemini, OpenAI, Deepgram, ElevenLabs
- **4 formatting providers**: Gemini, OpenAI, Anthropic, Groq — with 3 styles
- **Dictation commands**: Say "period", "new line", "em dash", etc. to insert punctuation
- **Clipboard preservation**: Restores your previous clipboard contents after pasting
- **Fully on-device**: Apple Speech requires no API key or internet connection

## Install

**Download:** Grab the latest `Yap.app.zip` from [Releases](https://github.com/oobagi/yap/releases), unzip, and move to `/Applications`.

**Build from source:**

```bash
git clone https://github.com/oobagi/yap.git
cd yap
./build.sh
cp -r build/Yap.app /Applications/
open /Applications/Yap.app
```

Requires macOS 13+ and Xcode Command Line Tools (`xcode-select --install`).

## Usage

Yap supports three recording modes:

| Mode | How to use |
|---|---|
| **Hold to record** | Hold fn (Globe) or Option → speak → release |
| **Hands-free (double-tap)** | Double-tap the hotkey quickly → speak → tap again to stop |
| **Hands-free (click pill)** | Click the floating overlay pill → speak → click Stop |

During hands-free recording, a pause/resume button appears in the pill. Recordings under 0.4 seconds or below the silence threshold are automatically discarded.

## Permissions

On first launch, grant the following in System Settings → Privacy & Security:

| Permission | Why it's needed |
|---|---|
| Microphone | Record your voice |
| Speech Recognition | On-device transcription pre-check |
| Accessibility | Detect the fn/Option hotkey globally |

## Configuration

Click the menu bar icon → **Settings** to configure providers and styles. Config is saved to `~/.config/yap/config.json`.

### Transcription Providers

| Provider | API key required | Default model |
|---|---|---|
| Apple Dictation | No | On-device |
| Google Gemini | Yes | gemini-2.5-flash |
| OpenAI | Yes | gpt-4o-transcribe |
| Deepgram | Yes | nova-3 |
| ElevenLabs | Yes | scribe_v1 |

### Formatting Providers & Styles

Formatting cleans up transcribed text using an LLM. Leave the provider as **None** to paste raw transcription.

| Provider | Default model |
|---|---|
| Google Gemini | gemini-2.5-flash |
| OpenAI | gpt-4o-mini |
| Anthropic | claude-haiku-4-5-20251001 |
| Groq | llama-3.3-70b-versatile |

| Style | Description | Example output |
|---|---|---|
| **Casual** | Light cleanup, keeps your voice | *so like i was thinking we should move the meeting to friday* |
| **Formatted** | Clean formatting, faithful to what you said | *So I was thinking we should move the meeting to Friday, because Thursday's not going to work for me.* |
| **Professional** | Polished writing, elevated language | *I believe we should reschedule the meeting to Friday, as Thursday will not work for my schedule.* |

> **Tip:** When Gemini is selected for both transcription and formatting, Yap sends a single API call that handles both steps — saving time and cost.

> Empty model field falls back to the provider's default.

## Dictation Commands

Say these words while speaking to insert punctuation symbols:

| Say | Inserts |
|---|---|
| "period" / "full stop" | `.` |
| "comma" | `,` |
| "question mark" | `?` |
| "exclamation mark" / "exclamation point" | `!` |
| "colon" | `:` |
| "semicolon" | `;` |
| "open paren" / "close paren" | `(` `)` |
| "open bracket" / "close bracket" | `[` `]` |
| "open brace" / "close brace" | `{` `}` |
| "open quote" / "close quote" | `"` `"` |
| "dash" / "em dash" | `—` |
| "hyphen" | `-` |
| "ellipsis" / "dot dot dot" | `…` |
| "new line" | line break |
| "new paragraph" | two line breaks |
| "ampersand" | `&` |
| "at sign" | `@` |
| "hashtag" / "hash" | `#` |
| "dollar sign" | `$` |
| "percent" | `%` |
| "asterisk" / "star" | `*` |
| "slash" / "forward slash" | `/` |
| "backslash" | `\` |
| "underscore" | `_` |
| "pipe" | `\|` |
| "tilde" | `~` |
| "caret" | `^` |

Commands are only converted when clearly intended as punctuation, not when used naturally in speech.

## License

MIT
