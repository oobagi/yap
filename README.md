<p align="center">
  <img src="Resources/AppIcon.png" width="128" height="128" alt="VoiceType icon">
</p>

<h1 align="center">VoiceType</h1>

<p align="center">
  Hold a key, speak, release — your words are transcribed and pasted wherever you're typing.<br>
  Free and open source voice-to-text for macOS.
</p>

---

## Install

**Download:** Grab the latest `VoiceType.app.zip` from [Releases](https://github.com/igaboo/voicetype/releases), unzip, and move to `/Applications`.

**Build from source:**

```bash
git clone https://github.com/igaboo/voicetype.git
cd voicetype
./build.sh
cp -r build/VoiceType.app /Applications/
open /Applications/VoiceType.app
```

Requires macOS 12+ and Xcode Command Line Tools (`xcode-select --install`).

On first launch, grant Microphone, Speech Recognition, and Accessibility permissions in System Settings.

## Setup

Click the menu bar icon → **Settings** to configure your transcription and formatting providers. Works out of the box with Apple Dictation (free, on-device) — add an API key to upgrade accuracy and enable AI formatting.

| | Providers |
|---|---|
| **Transcription** | Apple Dictation, Gemini, OpenAI, Deepgram, ElevenLabs |
| **Formatting** | Gemini, OpenAI, Anthropic — Casual, Formatted, or Professional |

Gemini can handle both transcription and formatting in a single API call.

## License

MIT
