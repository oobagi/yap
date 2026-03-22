# Yap for Windows

Windows port of Yap, the voice transcription and formatting tool.

## Prerequisites

- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0) (8.0+)
- Windows 10 version 1904 (19041) or later
- Microphone access

## Build

### Quick build

```powershell
cd windows
dotnet build Yap/Yap.csproj
```

### Release build (self-contained executable)

```powershell
.\windows\build.ps1
```

This produces a single-file self-contained executable in `build/windows/`.

### Debug build

```powershell
.\windows\build.ps1 -Configuration Debug
```

## Missing Resources

The following resources are not yet included and must be created before a full release:

### Icons

No `.ico` files exist yet. The app uses programmatically generated tray icons at runtime, so it will run fine without them. However, a proper `yap.ico` is needed for:

- The executable icon (shown in Explorer, taskbar, etc.)
- Installer branding

Place icon files in `Yap/Resources/Icons/` and uncomment the relevant lines in `Yap/Yap.csproj`.

### Sounds

Sound effect `.wav` files are not included in the repo. To generate them from the macOS `.aiff` source files:

```bash
# Requires ffmpeg
./windows/convert-sounds.sh
```

This converts `Resources/Sounds/*.aiff` to `.wav` format (16-bit PCM, 44100 Hz). The Windows `SoundPlayer` will log warnings but function normally if sound files are missing.

## Project Structure

```
windows/
  Yap/                  # C# WPF application
    Audio/              # Audio recording, transcription, sound playback
    Core/               # Config, logging, orchestrator, state machine
    Models/             # Data models (history entries, etc.)
    Services/           # API clients (Gemini, OpenAI, Deepgram, etc.)
    UI/                 # Overlay, settings window, tray icon
    Resources/
      Icons/            # (empty — needs yap.ico)
      Sounds/           # (empty — run convert-sounds.sh)
  build.ps1             # Release build script
  convert-sounds.sh     # .aiff to .wav converter (requires ffmpeg)
```
