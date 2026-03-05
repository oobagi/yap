# VoiceType 🎙️

A lightweight macOS menu bar app — hold a key, speak, release, and your words are transcribed and pasted into whatever you're typing in. Like Wispr Flow, but yours.

**Free. No API keys. Uses Apple's built-in speech recognition.**

## How It Works

1. **Hold fn key** → starts recording
2. **Release fn key** → transcribes via Apple Speech Recognition
3. **Transcribed text** → auto-pasted into the focused input field
4. Menu bar icon shows state: 🎤 idle · 🎤● recording · ⋯ processing

## Requirements

- macOS 12+ (Monterey or later)
- Xcode Command Line Tools (`xcode-select --install`)

## Setup

### 1. Install Command Line Tools (if needed)
```bash
xcode-select --install
```

### 2. Build & install
```bash
cd voicetype
chmod +x build.sh
./build.sh
cp -r build/VoiceType.app /Applications/
```

### 3. Launch
```bash
open /Applications/VoiceType.app
```

macOS will prompt for three permissions — grant all:
- **Microphone** — to record your voice
- **Speech Recognition** — to transcribe audio
- **Accessibility** — to detect the hotkey and simulate paste

## Configuration (optional)

Create `~/.config/voicetype/config.json` to change the hotkey:

```json
{
    "hotkey": "fn"
}
```

**Hotkey options:**
- `"fn"` — fn / Globe key 🌐 (default)
- `"option"` — Option (⌥) key

### fn key on Apple Silicon Macs

On newer MacBooks the fn key doubles as the Globe (🌐) key. If the emoji picker keeps appearing:

1. **System Settings → Keyboard → "Press 🌐 key to"** → set to **"Do Nothing"**
2. Or switch to `"option"` in config

## Menu Bar

Click the mic icon in the menu bar:
- **Enabled** — toggle on/off (⌘E)
- **Quit** — exit the app (⌘Q)

## How It's Built

Pure Swift, zero external dependencies:
- `Speech` framework — Apple's on-device speech recognition
- `AVAudioEngine` — microphone recording
- `CGEventTap` — global hotkey detection
- `CGEvent` — simulated Cmd+V paste
- `NSStatusItem` — menu bar icon

## Troubleshooting

**"Failed to create event tap"** → Grant Accessibility permission, then restart the app.

**No transcription** → Check Speech Recognition permission in System Settings. Look at Console.app for `VoiceType` log messages.

**fn key opens emoji picker** → Change system setting (see above) or use `"option"` hotkey.

**Paste doesn't work in some apps** → The app must accept Cmd+V. Some terminal emulators may use Cmd+Shift+V instead.

## Launch at Login (optional)

To start VoiceType automatically:
1. Open **System Settings → General → Login Items**
2. Click **+** and select **VoiceType** from Applications
