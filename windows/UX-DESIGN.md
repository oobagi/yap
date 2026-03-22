# Yap for Windows -- UX Design Document

This document defines the UX architecture for bringing Yap to Windows. Every decision is grounded in the macOS source as the reference implementation. The goal is full feature parity while making Yap feel like it was built for Windows from day one.

---

## Table of Contents

1. [System Tray Adaptation](#1-system-tray-adaptation)
2. [Hotkey Adaptation](#2-hotkey-adaptation)
3. [Overlay Positioning](#3-overlay-positioning)
4. [Overlay Visual Design](#4-overlay-visual-design)
5. [Settings Window](#5-settings-window)
6. [Onboarding Flow Adaptation](#6-onboarding-flow-adaptation)
7. [Permissions Differences](#7-permissions-differences)
8. [Windows-Specific Considerations](#8-windows-specific-considerations)

---

## 1. System Tray Adaptation

### macOS Reference

On macOS, Yap lives in the **menu bar** (top-right). It uses `NSStatusItem` with a custom template icon (`MenuIconTemplate.png`, 14x14pt). The menu contains:

| Menu Item | Behavior |
|---|---|
| **Yap** (disabled title) | Brand label, non-interactive |
| ---separator--- | |
| **Enabled** (Cmd+E) | Toggle on/off, checkmark state |
| **History** (submenu) | Last 10 entries (truncated at 60 chars), "Show All...", "Clear History" |
| **Settings...** (Cmd+,) | Opens settings window |
| ---separator--- | |
| **Quit** (Cmd+Q) | Terminates the app |

The menu bar icon changes to reflect state:
- **Idle**: custom template icon (or `mic` SF Symbol fallback)
- **Recording / Hands-Free / Paused**: `mic.fill`
- **Processing**: `ellipsis.circle`

### Windows Implementation

**Location**: Windows system tray (notification area, bottom-right of taskbar).

**Tray icon**: Use a `NotifyIcon` (WPF `System.Windows.Forms.NotifyIcon` or the WinUI equivalent). The icon should be an `.ico` file at 16x16, 24x24, 32x32, and 48x48 resolutions. Provide both light and dark variants so the icon remains visible on any taskbar theme.

**Icon states**:

| State | Icon Description |
|---|---|
| Idle | Microphone outline (thin stroke, analogous to the macOS template icon) |
| Recording / Hands-Free / Paused | Filled microphone |
| Processing | Ellipsis in circle (or a small animated dot sequence) |

**Tooltip**: The tray icon tooltip (shown on hover) should display the current state:
- Idle: `"Yap -- Press [hotkey] to record"`
- Recording: `"Yap -- Recording..."`
- Processing: `"Yap -- Processing..."`

**Context menu** (right-click on tray icon):

| Item | Shortcut | Notes |
|---|---|---|
| **Yap** | -- | Disabled label, same as macOS |
| --- | | |
| **Enabled** | -- | Checkmark toggle. No keyboard shortcut in the context menu itself; the global hotkey handles recording. |
| **History** | -- | Submenu: last 10 entries (truncated at 60 chars), "Show All...", "Clear History" |
| **Settings...** | -- | Opens settings window |
| --- | | |
| **Quit** | -- | Exits the app |

**Left-click behavior**: On macOS, clicking the status item opens the menu. On Windows, left-clicking a tray icon typically does something different from right-clicking. Recommended: **left-click opens/focuses the Settings window** (matching the most useful action), **right-click opens the context menu**. This is the pattern used by apps like Discord and Slack on Windows.

**Startup behavior**: Yap should add itself to the Windows startup folder (or registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) with a Settings toggle to enable/disable "Start with Windows". macOS handles this via Login Items; Windows needs explicit implementation.

---

## 2. Hotkey Adaptation

### macOS Reference

Yap on macOS supports two modifier keys, chosen in Settings:

- **fn (Globe) key** -- modifier mask `0x00800000`. This is the default. The fn key has no equivalent on standard Windows keyboards.
- **Option key** -- modifier mask `CGEventFlags.maskAlternate`. This maps conceptually to Alt on Windows, but Alt is heavily used for menu navigation (Alt+F, etc.) and would conflict.

The hotkey supports three interaction modes:
1. **Hold to record**: Key down starts recording, key up stops and processes.
2. **Double-tap**: Two quick presses (within 350ms) enters hands-free recording mode.
3. **Click the pill**: Clicking the always-visible pill starts hands-free recording (no key needed).

### Windows Hotkey Recommendations

Since Windows has no fn/Globe key, the default must be different. The right choice balances three things: the key must be (a) easy to reach, (b) rarely used for anything else, and (c) comfortable to hold.

**Recommended default: Caps Lock**

Rationale:
- Caps Lock is the de facto standard for voice input tools on Windows (Superwhisper, Talon, Whisper.cpp-based tools all default to it).
- It is easy to reach with the left pinky while keeping both hands on the keyboard.
- It is rarely needed for its original purpose (most people never deliberately use Caps Lock).
- Yap would suppress the Caps Lock toggle behavior while intercepting it, just as the macOS version suppresses the emoji picker on fn.

**Alternative options (available in the hotkey picker)**:

| Key | Pros | Cons |
|---|---|---|
| **Caps Lock** (default) | Ergonomic, popular for voice tools, rarely needed | Must suppress toggle; some users remap it |
| **Right Alt** | Unused on US layouts | Used for AltGr on European keyboards (dead key input) |
| **Insert** | Almost never used in modern apps | Awkward position on many keyboards |
| **Scroll Lock** | Truly dead key on modern systems | Very awkward position; missing on many laptops |
| **F13--F24** | No system meaning at all | Require special keyboards; not discoverable |
| **Pause/Break** | Unused | Missing on most laptops |

**Hotkey customization UI**: Unlike macOS (which only offers fn or Option in a dropdown), the Windows version must include a **hotkey picker** in Settings. This is necessary because:
1. There is no universally safe default like fn.
2. International keyboard layouts vary significantly.
3. Power users will want to pick their own key.

The picker should work as follows:
- A "Press a key..." button that captures the next keypress.
- Display the captured key name in a `KeyCapView` style label.
- Validate the captured key: reject Escape, Enter, Space, Backspace, and standard typing keys (A-Z, 0-9). Allow modifier keys (Caps Lock, Scroll Lock, Insert, F-keys, Pause) and modifier combos (e.g., Ctrl+Shift+Y).
- Show a warning if the user picks a key that conflicts with common Windows shortcuts (e.g., Alt alone, Win key).

**Implementation note**: On Windows, keyboard hooks use `SetWindowsHookEx` with `WH_KEYBOARD_LL` (low-level keyboard hook). This is the equivalent of macOS's `CGEventTap`. Unlike macOS, this does **not** require any special permission -- see Section 7.

**Double-tap detection**: Same logic as macOS. Track `lastKeyUpTime` and compare against a 350ms window. No platform-specific changes needed.

**How to communicate the hotkey**: All onboarding text, the `KeyCapView` label, and the tooltip must dynamically use the configured hotkey name. On macOS, the default label is "fn". On Windows, the default label should be "Caps Lock" (or whatever the user has configured). The `KeyCapView` rendering remains the same -- a rounded rectangle with the key name inside.

---

## 3. Overlay Positioning

### macOS Reference

The overlay pill is positioned at the **bottom-center** of the primary screen. Specific details from the source:

- Panel size: 1400x700pt (large to accommodate the lava lamp gradient blobs).
- The pill itself is centered horizontally. Vertically, the pill's center is at panel Y coordinate ~435 (measuring from the bottom of the panel).
- On-screen Y: `screenFrame.minY + 330 - panelHeight`. This places the pill's visual center roughly 115pt above the bottom edge of the screen.
- Off-screen Y: `screenFrame.minY - panelHeight` (fully below the screen).
- Slide-in animation: 0.5s with custom bezier `(0.16, 1, 0.3, 1)` -- a springy ease-out.
- Slide-out animation: 0.4s with `(0.4, 0, 1, 1)` -- a quick ease-in.
- The panel is borderless, non-activating, click-through (except for the pill region), and floats above all spaces.

### Windows Implementation

**Position: Bottom-center of the primary monitor, above the taskbar.**

The Windows taskbar defaults to 48px tall (40px on Windows 10, variable on Windows 11 with small/large taskbar settings). The pill should be positioned so that it floats approximately 24-32px above the top edge of the taskbar, matching the macOS distance above the Dock.

The implementation approach:
1. Query the taskbar position and size using `Shell_NotifyIcon` or `SystemParametersInfo(SPI_GETWORKAREA)` to get the working area (screen minus taskbar).
2. Position the overlay window's bottom edge 24px above the working area bottom.
3. If the taskbar is on the top, left, or right (rare but possible), adjust accordingly. Simplest approach: always place the pill centered at the bottom of the working area.

**Window properties** (WPF equivalent of the NSPanel setup):

| macOS Property | Windows WPF Equivalent |
|---|---|
| `.borderless` | `WindowStyle="None"`, `AllowsTransparency="True"` |
| `.nonactivatingPanel` | `ShowActivated="False"`, `ShowInTaskbar="False"` |
| `level = .floating` | `Topmost="True"` |
| `backgroundColor = .clear` | `Background="Transparent"` |
| `hasShadow = false` | Default with transparent window; no frame shadow |
| `collectionBehavior = [.canJoinAllSpaces]` | Not applicable (Windows has virtual desktops but no equivalent API for pinning; use `Topmost` to stay visible) |
| `hidesOnDeactivate = false` | Default behavior (owned windows don't hide) |
| `isMovableByWindowBackground = false` | Don't set `MouseLeftButtonDown` drag handler |

**Slide animation**:
- Use WPF `Storyboard` or `DoubleAnimation` targeting `Window.Top`.
- Slide-in: animate `Top` from below-screen to target position over 500ms with a custom easing (WPF `SplineDoubleKeyFrame` with control points `0.16,1, 0.3,1`).
- Slide-out: animate `Top` from current to below-screen over 400ms with `0.4,0, 1,1`.

**Multi-monitor behavior**:
- Always show the pill on the **primary monitor** (the one with the taskbar, unless the user has moved it).
- If Yap is recording and the user moves focus to another monitor, the pill stays on the primary monitor. This matches macOS behavior (`canJoinAllSpaces` keeps it on the current space, not necessarily the focused monitor).
- For a future enhancement, consider following the active monitor (the one where the cursor or focused window is), but for v1, primary monitor is sufficient and simpler.

**Taskbar auto-hide**: If the taskbar is set to auto-hide, use the full screen height as the working area and position the pill at the very bottom. The auto-hidden taskbar will overlap the pill when it slides up, which is acceptable since auto-hide is an intentional screen-space optimization.

---

## 4. Overlay Visual Design

### macOS Reference

The pill overlay has several visual layers and states. Here is the complete breakdown:

**Pill shape and structure**:
- Capsule shape (fully rounded rectangle).
- Background: `Color.black.opacity(0.75)` layered with `.thinMaterial` (vibrancy/blur). Bordered with `Color.white.opacity(0.3)` 1px stroke.
- Shadow: `black.opacity(0.35)` radius 16, y-offset 4 when expanded.
- Minimum size when expanded: 40x28 content area plus 12px horizontal and 6px vertical padding.
- When minimized (idle, always-visible): scaled to 0.5x, lower opacity border, smaller shadow.
- Hover on minimized pill: scales to 0.65x, shows "Click to start transcribing" label and mic icon.
- Press-down effect: 0.85x scale, 0.7 opacity.

**Recording state -- Bar Visualizer**:
- 11 vertical bars in an `HStack` with 2px spacing.
- Each bar: 3px wide, `RoundedRectangle` with 1.5px corner radius.
- Bar heights range from 5px (min) to 28px (max), driven by FFT band levels.
- Center bars are taller (position scale peaks at 1.0 for bar 5), edges drop to 0.35.
- Audio levels drive a bounce factor: pill scales from 1.0 to 1.25 based on `pow(level, 1.5) * 0.25`.
- Spring animation on bar heights: stiffness 280, damping 18.
- Bars appear with a horizontal scale-in from center: `scaleEffect(x: appeared ? 1 : 0.001)`.

**Processing state -- Shimmer wave**:
- Same 11 bars, but audio decays to zero and a gaussian wave sweeps across.
- Wave cycles every 1.2 seconds, moving from left to right.
- At wave peak: bar opacity = 0.95, bar height boosted by up to 14px.
- At wave trough: bar opacity = 0.35, bar height at minimum.
- The gaussian width is `exp(-distance^2 / 6.0)`.
- Crossfade from audio to wave: audio decays over 0.35s, wave fades in over 0.35s with 0.15s delay.

**Hands-free mode**:
- Pause button (play/pause icon in a circle, `white.opacity(0.15)` background) flies out to `x: -49` from center.
- Stop button (stop icon in a circle, `red.opacity(0.85)` background) flies out to `x: +49`.
- Scale/fly animation uses spring: response 0.35, damping 0.8.
- Paused state: bars replaced with static low-opacity bars (`white.opacity(0.25)`, 5px tall).
- Elapsed timer: shown after 10 seconds, `monospaced` font, `white.opacity(0.5)`, positioned above the pill.

**Lava lamp gradient background**:
- Four ellipses (purple, blue, cyan, indigo) with sizes 280-360px, blurred at radius 55.
- Each ellipse follows a lissajous orbit: `cos(t * speed + offset) * amplitude`.
- Speed scales with energy: `0.4 + energy * 0.6`.
- Brightness scales: `0.25 + energy * 0.25`.
- During "nice!" celebration: blobs orbit outward (radius up to 150px) with envelope `sin(phase / 4)`.
- Energy levels: recording = 1.0, processing = 0.6, onboarding = 0.3, hover = 0.15, idle = 0.4.

**Error state**:
- Exclamation triangle icon (red) + message text in the pill.
- Auto-dismisses after 2 seconds.
- Shake animation: horizontal sinusoidal displacement, 6 cycles over 0.5s, amplitude 10px, decaying.

**No-speech state**:
- Static low-opacity bars (same as paused visual).
- Shake animation applied.

### Windows WPF/WinUI Implementation

**Pill rendering**: Use a WPF `Canvas` or `Border` with `CornerRadius` set to half the height (producing a capsule). For the blur material effect, use:
- On Windows 11: `Mica` or `Acrylic` material via `WindowsCompositionTarget` / `DesktopAcrylicBackdrop`. This gives the closest match to macOS `.thinMaterial`.
- On Windows 10: `SetWindowCompositionAttribute` with `ACCENT_ENABLE_ACRYLICBLURBEHIND`. Fallback to a semi-transparent dark background if acrylic is unavailable.
- Alternative: Use `Win2D` (DirectX) for the blur layer if WPF performance is insufficient.

**Bar visualizer**: Use a WPF `ItemsControl` with horizontal `StackPanel` and individual `Rectangle` elements. Animate `Height` using `DoubleAnimation` with `DecelerationRatio` to approximate the spring feel. For per-frame FFT-driven updates, use `CompositionTarget.Rendering` event (fires at 60fps).

**Shimmer wave**: Same `CompositionTarget.Rendering` loop, computing per-frame gaussian positions and updating bar heights and opacities.

**Lava lamp gradient**: Options, from best to simplest:
1. **Win2D / CanvasAnimatedControl**: Render four blurred ellipses on a `CanvasAnimatedControl` with the same lissajous math. Best performance.
2. **WPF with `RenderTargetBitmap`**: Render ellipses to a bitmap, apply `BlurEffect`, update at 30fps. Simpler but more CPU-bound.
3. **HLSL shader**: Write a pixel shader that computes the metaball-like blend. Most performant but highest dev cost.

Recommendation: Use **Win2D** for the gradient. It is a well-maintained DirectX wrapper, supports hardware-accelerated blur, and is available for both WPF (via hosting) and WinUI 3.

**Color scheme**: The pill uses absolute colors (black background, white bars, white text) and does not follow system theme. This is intentional -- the overlay must be legible over any content beneath it. The lava lamp colors (purple, blue, cyan, indigo) are also hardcoded. **No changes needed for dark/light mode for the overlay itself.** The overlay always uses its dark translucent style.

**Transparency**: The overlay window must have `AllowsTransparency="True"` and `Background="Transparent"` in WPF, or use a layered window with `WS_EX_LAYERED` and `WS_EX_TRANSPARENT` (for click-through regions) in Win32.

**Click-through behavior**: On macOS, `OverlayContentView.hitTest` returns `nil` for areas outside the pill, making them click-through. On Windows:
- Use `WS_EX_TRANSPARENT` extended style for the window, then override `WndProc` to handle `WM_NCHITTEST` -- return `HTTRANSPARENT` for regions outside the pill, `HTCLIENT` for the pill region.
- Alternatively, use `InputTransparent` regions if using WinUI 3.

**Animations summary**:

| Animation | macOS | WPF Equivalent |
|---|---|---|
| Slide in/out | `NSAnimationContext` with custom bezier | `Storyboard` with `SplineDoubleKeyFrame` |
| Bar heights | `interpolatingSpring(stiffness: 280, damping: 18)` | `DoubleAnimation` with `DecelerationRatio=0.7` or custom spring easing |
| Pill scale | `.spring(response: 0.25, dampingFraction: 0.45)` | `DoubleAnimation` targeting `ScaleTransform` with custom easing |
| Hands-free buttons | `.spring(response: 0.35, dampingFraction: 0.8)` | `DoubleAnimation` on `TranslateTransform.X` |
| Shake | Custom `GeometryEffect` with sinusoidal | `DoubleAnimation` on `TranslateTransform.X` with keyframes |
| Lava lamp blobs | `TimelineView(.animation)` at 60fps | `CompositionTarget.Rendering` or Win2D animation loop |
| Celebration orbit | Triggered by `celebrationPhase` | Same math, driven by `DispatcherTimer` or `CompositionTarget.Rendering` |

---

## 5. Settings Window

### macOS Reference

The Settings window is a 500x680 `NSWindow` with `.titled` and `.closable` style. It contains a scrollable `Form` with `.grouped` form style. Layout:

**Sections**:

1. **General**: Hotkey picker (dropdown: "fn / Globe" or "Option")
2. **Transcription**: Provider dropdown, API Key text field, Model text field with description, provider-specific options (Deepgram: smart format toggle, language, keywords; OpenAI: language, prompt; Gemini: temperature slider; ElevenLabs: language)
3. **Formatting**: Provider dropdown, API Key, "Use same API key" toggle (when both providers share a backend), Model text field, Style segmented picker (Casual / Formatted / Professional), Style preview card (before/after comparison)
4. **Appearance**: "Sound effects" toggle, "Gradient background" toggle, "Always-visible idle pill" toggle
5. **History**: "Save transcription history" toggle

**Footer bar**: "Reset Onboarding" (plain text button, left-aligned), "Cancel" and "Save" buttons (right-aligned). Cancel uses `.cancelAction` (Escape key), Save uses `.defaultAction` (Enter key).

### Windows Implementation

**Window style**: Standard WPF `Window` with title bar, close button, no resize handle. Size: 500x700 (slightly taller to accommodate the hotkey picker, which is more complex on Windows). Use `ResizeMode="NoResize"`.

**Design language**: Follow **WinUI 3 / Fluent Design** patterns for native feel:
- Use `NavigationView` or a simple `ScrollViewer` with stacked sections. No tabs -- the macOS version uses a single scrolling form and it works well. Keep the same structure.
- Section headers: Bold text with a horizontal separator, matching WinUI `SettingsCard` grouping style.
- Text fields: Use WinUI `TextBox` with `Header` property for labels and `PlaceholderText` for hints.
- Dropdowns: Use WinUI `ComboBox` instead of macOS `Picker`.
- Toggles: Use WinUI `ToggleSwitch` (the Windows-native toggle) instead of macOS `Toggle` (checkbox).
- Segmented picker: Use `RadioButtons` with horizontal orientation (WinUI pattern) instead of macOS segmented control.

**Hotkey section** (Windows-specific addition):

This replaces the macOS "fn / Option" dropdown with a richer control:

```
Hotkey
[Caps Lock]  [Change...]

 When you want to record, hold this key and speak.
 Double-tap it for hands-free mode.
```

The `[Caps Lock]` is a `KeyCapView`-styled display of the current hotkey. `[Change...]` opens a modal capture dialog:

```
Press the key you want to use as your hotkey...

Listening...  [Cancel]
```

When a key is pressed, it shows: `"Use [Insert] as your hotkey?"` with `[Confirm]` and `[Cancel]`.

**Style preview card**: Port directly. Use a `Border` with `CornerRadius="8"` containing a `Grid` with two columns (Before / After) separated by a vertical `Separator`. Header row with style name and description.

**Appearance section**: Add a "Start with Windows" toggle (absent on macOS because macOS handles this via Login Items in System Settings).

**Button bar**: Same layout -- "Reset Onboarding" left-aligned as a `HyperlinkButton` or plain text, "Cancel" and "Save" right-aligned as standard `Button` controls. Bind Enter to Save, Escape to Cancel.

**Settings storage**: macOS uses `UserDefaults`. On Windows, use one of:
- **AppData JSON file**: `%APPDATA%\Yap\config.json`. This is the closest equivalent to the macOS `~/.config/yap/config.json` approach and is portable.
- **Windows Registry**: `HKCU\Software\Yap\Settings`. More "Windows-native" but harder to debug and back up.

Recommendation: Use **AppData JSON** (`%APPDATA%\Yap\config.json`) for consistency with the macOS config location pattern. The macOS version already uses JSON config at `~/.config/yap/config.json`.

---

## 6. Onboarding Flow Adaptation

### macOS Reference

The macOS onboarding has the following steps, each displayed as a floating card above the pill:

| Step | Card Text | Input Expected | Next Step |
|---|---|---|---|
| `.tryIt` | "Hold [fn] and speak -- Yap transcribes it" | Hold fn, speak, release | `.nice` then `.doubleTapTip` |
| `.doubleTapTip` | "Double-tap [fn] for hands-free transcription" | Double-tap fn | `.nice` then `.clickTip` |
| `.clickTip` | "Click the pill for hands-free transcription" | Click the pill | `.nice` then `.apiTip` |
| `.apiTip` | "Add an API key in the menu bar for better transcription" | Hold fn for 0.6s (confirm) | `.formattingTip` |
| `.formattingTip` | "Enable formatting in Settings to clean up grammar..." | Hold fn for 0.6s (confirm) | `.welcome` |
| `.welcome` | "You're all set -- enjoy!" | Hold fn for 0.6s (confirm) | Onboarding complete |

Transient tips (shown on error, auto-dismiss after 2.5s):
- `.holdTip`: "Hold [fn] -- don't just tap it" (shown on short taps < 0.5s)
- `.speakTip`: "Didn't catch that -- speak up while holding [fn]" (shown when no speech detected)

The `.nice` step shows a random celebration message ("Nice!", "Nailed it!", etc.) for 1.5s, plays a "Submarine" sound, then advances.

### Windows Adaptation

The flow structure remains **identical** -- same number of steps, same progression logic, same celebration messages. The only changes are text and key references.

**Updated onboarding text**:

| Step | Windows Card Text |
|---|---|
| `.tryIt` | "Hold [Caps Lock] and speak -- Yap transcribes it" |
| `.doubleTapTip` | "Double-tap [Caps Lock] for hands-free transcription" |
| `.clickTip` | "Click the pill for hands-free transcription" |
| `.apiTip` | "Add an API key in Settings for better transcription" |
| `.formattingTip` | "Enable formatting in Settings to clean up grammar and punctuation automatically" |
| `.welcome` | "You're all set -- enjoy!" |
| `.holdTip` | "Hold [Caps Lock] -- don't just tap it" |
| `.speakTip` | "Didn't catch that -- speak up while holding [Caps Lock]" |

Changes from macOS:
1. All `[fn]` references become `[Caps Lock]` (or whatever the user's configured hotkey is -- the `hotkeyLabel` state variable handles this dynamically).
2. The `.apiTip` text changes from "in the menu bar" to "in Settings" because on Windows the natural place to find API key configuration is the Settings window, not the system tray context menu. (The tray menu should still have a "Settings..." item, but "menu bar" is macOS-specific language.)
3. **Hold-to-confirm interaction is unchanged**: Steps `.apiTip`, `.formattingTip`, and `.welcome` still use the 0.6s hold-to-advance mechanism with the configured hotkey.

**Simplification opportunities**:

The onboarding could theoretically be simplified because Windows requires fewer permissions (see Section 7). However, the steps are not about permissions -- they are about teaching the three input modes (hold, double-tap, click) and pointing users to configuration. All three input modes exist on Windows, so all steps should remain.

One possible simplification: If the user has already configured an API key before first launch (e.g., by editing the config file), skip `.apiTip`. This is the same logic as macOS.

**Sound effects**: The macOS version plays three system sounds:
- "Blow" (recording start)
- "Pop" (recording stop / short tap)
- "Submarine" (onboarding celebration)

These are macOS system sounds at `/System/Library/Sounds/`. On Windows, bundle equivalent `.wav` files with the application. Keep the same semantic mapping but use sounds that feel appropriate on Windows. Consider using Windows system sounds (`SystemAsterisk`, `SystemExclamation`) as an alternative, but bundled sounds ensure consistency. Ship three small `.wav` files in the app resources.

---

## 7. Permissions Differences

### macOS Requirements

macOS requires three explicit permissions, each requiring user interaction in System Settings:

| Permission | Purpose | User Action Required |
|---|---|---|
| **Microphone** | Audio recording | System dialog on first access via `AVCaptureDevice.requestAccess(for: .audio)` |
| **Speech Recognition** | Apple on-device transcription | System dialog via `SFSpeechRecognizer.requestAuthorization` |
| **Accessibility** | `CGEventTap` for keyboard interception | Manual toggle in System Settings > Privacy & Security > Accessibility |

The Accessibility permission is the most friction-heavy because it cannot be auto-prompted and requires navigating to System Settings manually. If it is not granted, the hotkey does not work at all.

### Windows Requirements

| Permission | Purpose | User Action Required |
|---|---|---|
| **Microphone** | Audio recording | Auto-prompted by Windows on first access. Settings > Privacy > Microphone. |
| **(none)** | Keyboard hooks | `SetWindowsHookEx(WH_KEYBOARD_LL)` requires **no permission** on Windows. Any application can install a low-level keyboard hook. |
| **(none)** | Speech recognition | Windows Speech Recognition does not require a separate permission. If using API-based transcription (Gemini, OpenAI, etc.), no local permission is needed at all. |

### Impact on UX

This dramatically simplifies the first-run experience:

1. **No Accessibility permission dance.** On macOS, if Accessibility is not enabled, Yap shows a notification ("Accessibility permission required") and the hotkey silently fails. On Windows, the hotkey works immediately. No error handling needed for this case.

2. **Microphone permission is auto-prompted.** Windows shows a system dialog the first time `NAudio` or `WASAPI` tries to open the microphone. No need for a pre-emptive permission request at launch. However, Yap should still detect denial and show a clear error: "Microphone access denied. Enable it in Windows Settings > Privacy > Microphone."

3. **No Speech Recognition permission.** On macOS, Apple Speech (SFSpeechRecognizer) requires explicit authorization. On Windows, if using the Windows Speech API (`System.Speech.Recognition`), no permission is needed. If using API-based transcription, it is just an HTTP call -- no local permission involved.

**What to remove from onboarding**: Nothing. The onboarding steps are not about permissions -- they are about teaching input modes and pointing to configuration. However, the first-run experience should feel faster because there is no "please go to System Settings" friction.

**What to add**: A single check at startup: is the microphone available? If not (permission denied or no microphone hardware), show an error state in the tray icon tooltip and a notification: "Yap needs microphone access to work."

---

## 8. Windows-Specific Considerations

### High DPI / Display Scaling

Windows display scaling (100%, 125%, 150%, 175%, 200%, etc.) is the most significant rendering difference from macOS.

- The overlay window, pill, bars, and all UI elements must use **DPI-aware sizing**. In WPF, this is automatic -- WPF uses device-independent pixels (DIPs), which are 1:1 at 96 DPI and scaled proportionally at higher DPI settings. All hardcoded sizes (pill padding, bar width, gradient blob sizes) should be specified in DIPs.
- For Win32 interop (window positioning, hook coordinates), use `GetDpiForWindow` and scale pixel values accordingly.
- Mark the application manifest as **per-monitor DPI aware** (`<dpiAwareness>PerMonitorV2</dpiAwareness>`) so the overlay renders sharply on each monitor. Without this, Windows will bitmap-scale the window, causing blur on non-primary monitors with different scaling.

### Windows 10 vs. Windows 11 Visual Differences

| Feature | Windows 10 | Windows 11 |
|---|---|---|
| Rounded corners | Not supported natively | 8px corner radius on all windows |
| Acrylic/Mica | `SetWindowCompositionAttribute` (limited) | Full `DesktopAcrylicBackdrop` / `MicaBackdrop` support |
| System tray | Overflow area, all icons visible | Simplified tray with overflow flyout |
| Dark mode | Partial (apps opt in) | Comprehensive system-wide |
| Taskbar height | 40px | 48px (default), variable |

Implementation strategy:
- **Rounded corners on the pill**: Always use WPF `CornerRadius` -- this works on both Windows 10 and 11. Do not rely on the Windows 11 window corner rounding (which only applies to titled windows).
- **Blur material**: Try `DesktopAcrylicBackdrop` first (Windows 11). If unavailable, fall back to `SetWindowCompositionAttribute`. If both fail, use a solid semi-transparent dark background. The pill should look good in all three cases.
- **System tray**: On Windows 11, the system tray has a smaller overflow area. Ensure the Yap icon is promoted to the main tray area (the user may need to do this manually via Settings > Taskbar > Notification area). Show a first-run tip if the icon is in overflow: "Pin Yap to the taskbar for quick access."

### Dark/Light Mode Detection

On macOS, the pill overlay ignores system theme (it is always dark). The Settings window follows the system appearance automatically via SwiftUI.

On Windows:
- **Overlay**: Same as macOS -- always dark translucent. No theme adaptation needed.
- **Settings window**: Must follow the Windows system theme. Detect the current theme via the registry key `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme` (0 = dark, 1 = light) or via `UISettings.GetColorValue(UIColorType.Background)`.
- **Tray icon**: Provide both light and dark icon variants. Detect the taskbar theme via `SystemUsesLightTheme` registry value and select the appropriate icon.
- **React to theme changes**: Listen for `WM_SETTINGCHANGE` with `"ImmersiveColorSet"` parameter to detect runtime theme switches and update the Settings window and tray icon accordingly.

### Windows Notifications for Errors

On macOS, Yap uses the deprecated `NSUserNotification` for permission-related messages. On Windows:

- Use **Windows Toast Notifications** (via `Microsoft.Toolkit.Uwp.Notifications` or the `ToastNotificationManager` API).
- Toast scenarios:
  - "Microphone access denied" -- shown once on startup if microphone is blocked.
  - "Yap needs microphone access to work" -- if recording fails due to audio device error.
- Toasts should have an action button: "Open Settings" that navigates to `ms-settings:privacy-microphone`.
- Do **not** use toast notifications for transient errors (API failures, rate limits, etc.). Those are handled by the pill overlay's error state, same as macOS.

### Paste Simulation

On macOS, `PasteManager` uses `CGEvent` to simulate Cmd+V. On Windows:
- Use `SendInput` to simulate Ctrl+V.
- Same clipboard save/restore pattern: save current clipboard, set transcription text, simulate Ctrl+V (with 50ms delay), restore clipboard after 300ms.
- Use `System.Windows.Clipboard` for clipboard operations.
- Important: The `SendInput` call requires the foreground window to be the target app. Since Yap's overlay is non-activating (`ShowActivated="False"`), the previously focused window should remain in the foreground.

### Audio Recording

On macOS, Yap uses `AVAudioEngine` with an input tap. On Windows:
- Use **NAudio** (the de facto .NET audio library) with `WasapiCapture` for microphone input.
- Write to a WAV file in the temp directory, same as macOS.
- FFT for band levels: NAudio includes `FastFourierTransform`, or use `MathNet.Numerics` for Accelerate-equivalent FFT computation. Apply the same Hann window, 1024-point FFT, 6 logarithmic bands (80Hz-8kHz), mirrored to 11 display bars.

### Config and Data Locations

| macOS Path | Windows Equivalent |
|---|---|
| `~/.config/yap/config.json` | `%APPDATA%\Yap\config.json` |
| `~/.config/yap/debug.log` | `%APPDATA%\Yap\debug.log` |
| `~/.config/yap/history.json` | `%APPDATA%\Yap\history.json` |
| Temp audio file | `%TEMP%\yap_recording.wav` |

### Keyboard Layout Considerations

The Caps Lock key scan code is consistent across layouts (`VK_CAPITAL`, scan code 0x3A). However, the display name should be localized:
- English: "Caps Lock"
- German: "Feststelltaste" (or just "Caps Lock" -- most users know the English name)
- Use `GetKeyNameText` Win32 API to get the OS-localized key name for display in the `KeyCapView` and onboarding text.

### Installer and Distribution

- Ship as an MSI or MSIX installer.
- MSIX is preferred for Windows 11 (cleaner install/uninstall, auto-update support via App Installer).
- Include a "Start with Windows" checkbox in the installer.
- Application should be single-instance (use a named mutex to prevent duplicate launches).

---

## Summary of Key Differences

| Aspect | macOS | Windows |
|---|---|---|
| App chrome | Menu bar icon | System tray icon |
| Default hotkey | fn (Globe) | Caps Lock |
| Hotkey options | fn or Option (dropdown) | Any key (hotkey picker with capture) |
| Keyboard hook | `CGEventTap` (requires Accessibility) | `SetWindowsHookEx` (no permission) |
| Permissions | 3 (Mic, Speech, Accessibility) | 1 (Microphone, auto-prompted) |
| Overlay position | Bottom-center, above Dock | Bottom-center, above Taskbar |
| Blur material | `.thinMaterial` | Acrylic (Win11) / fallback solid |
| Settings storage | `UserDefaults` + JSON config | `%APPDATA%\Yap\config.json` |
| Paste simulation | `CGEvent` Cmd+V | `SendInput` Ctrl+V |
| Audio recording | `AVAudioEngine` | NAudio `WasapiCapture` |
| Transcription fallback | Apple `SFSpeechRecognizer` | Windows `System.Speech` (or none -- require API key) |
| Theme | Follows system via SwiftUI | Detect via registry, apply manually to Settings window |
| Startup | Login Items (macOS-managed) | Registry `Run` key or Startup folder |

---

*Document authored by ArchitectUX for the Yap Windows port. Reference implementation: macOS source at `/Users/jaden/Developer/yap/Sources/`.*
