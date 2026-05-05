# Yap Desktop App

This directory contains the canonical Yap desktop app.

For normal installation, use the latest installer from the root README or GitHub Releases:

https://github.com/oobagi/yap/releases/latest

## Commands

```bash
npm install
npm run dev
npm run check
npm run tauri -- dev
npm run tauri -- build
```

## Structure

- `src-tauri/src/` - Rust application code.
- `src-tauri/sidecar-overlay/` - macOS Swift/AppKit overlay sidecar.
- `src-tauri/icons/` - app, Windows, macOS, and tray icons.
- `src-tauri/sounds/` - bundled feedback sounds.
- `src/lib/settings/` - settings UI.
- `src/lib/history/` - history UI.
- `src/lib/overlay/` - overlay UI.
