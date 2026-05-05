//! Native Win32 overlay pill for Windows — full visual parity with macOS sidecar.
//!
//! Creates a layered window (WS_EX_LAYERED) rendered with tiny-skia + fontdue.
//! Per-pixel alpha via UpdateLayeredWindow gives automatic click-through on
//! transparent pixels — no hacking needed.
//!
//! Features:
//!   - Lava lamp gradient background (radial gradient blobs)
//!   - Spring-physics animated waveform bars
//!   - Hands-free controls (pause/stop buttons)
//!   - Onboarding cards with text rendering
//!   - Error messages with auto-dismiss
//!   - Hover state with tooltip
//!   - Processing shimmer sweep
//!   - No-speech flat bars
//!   - Elapsed time display
//!   - Smooth 30fps animation loop

#![cfg(target_os = "windows")]

use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Instant;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::orchestrator;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Canvas large enough for gradient halo + onboarding card + pill.
///
/// The visible pill stays anchored to the bottom of this window. Extra canvas
/// room prevents the soft Windows-native gradient from being cut off at the
/// layered-window bounds.
const CANVAS_W: i32 = 1040;
const CANVAS_H: i32 = 520;

const WM_YAP_UPDATE: u32 = WM_USER + 1;

const SLIDE_OFFSET_Y: f32 = 80.0;
const PILL_CENTER_BOTTOM_INSET: f32 = 45.0;
const ACTIVE_STACK_OFFSET_Y: f32 = 0.0;
const MINIMIZED_STACK_OFFSET_Y: f32 = 34.0;
const HOVER_TOOLTIP_OFFSET_Y: f32 = -24.0;
const HOVER_TOOLTIP_TRANSITION_Y: f32 = 4.0;
const CARD_GAP_Y: f32 = 50.0;
const TIMER_GAP_Y: f32 = 40.0;
const EXPANDED_PILL_SCALE: f32 = 0.82;
const HOVER_PILL_SCALE: f32 = 0.58;
const IDLE_PILL_SCALE: f32 = 0.5;
const PROCESSING_SCALE: f32 = 0.8;
const PRESS_SCALE: f32 = 0.85;
const AUDIO_BOUNCE_SCALE: f32 = 0.12;
const FRAME_INTERVAL_MS: u32 = 16;
const MAX_ANIMATION_DT: f32 = 1.0 / 30.0;

// Pill geometry
const BAR_COUNT: usize = 11;
const BAR_W: f32 = 3.0;
const BAR_GAP: f32 = 2.0;
const BAR_MIN_H: f32 = 5.0;
const BAR_MAX_H: f32 = 28.0;
const BARS_TOTAL_W: f32 = BAR_COUNT as f32 * BAR_W + (BAR_COUNT - 1) as f32 * BAR_GAP;

const STANDARD_CONTENT_W: f32 = 76.0; // 52px bars + 12px horizontal padding on each side.
const IDLE_CONTENT_W: f32 = 64.0; // 40px empty target + 12px horizontal padding on each side.
const HANDS_FREE_CONTENT_W: f32 = 138.0; // 124px controls + 7px horizontal padding on each side.
const EXPANDED_PILL_H: f32 = 40.0;
const IDLE_PILL_H: f32 = 20.0;
const PILL_HIT_PADDING: f32 = 12.0;
const CONTROL_BUTTON_OFFSET: f32 = 49.0;
const CONTROL_HIT_RADIUS: f32 = 17.0;
const GRADIENT_OFFSET_Y: f32 = 18.0;
const GRADIENT_MOTION_SCALE: f32 = 0.38;
const GRADIENT_DITHER_ALPHA: f32 = 3.0;

const POSITION_SCALE: [f32; 11] = [
    0.35, 0.45, 0.6, 0.78, 0.92, 1.0, 0.94, 0.8, 0.63, 0.48, 0.38,
];

// ---------------------------------------------------------------------------
// Onboarding step enum (mirrors orchestrator)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum OnboardingStep {
    TryIt,
    DoubleTapTip,
    ClickTip,
    ApiTip,
    FormattingTip,
    Welcome,
}

impl OnboardingStep {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "tryIt" => Some(Self::TryIt),
            "doubleTapTip" => Some(Self::DoubleTapTip),
            "clickTip" => Some(Self::ClickTip),
            "apiTip" => Some(Self::ApiTip),
            "formattingTip" => Some(Self::FormattingTip),
            "welcome" => Some(Self::Welcome),
            _ => None,
        }
    }

    /// Whether this step shows "Hold [key] to continue/finish" in the pill.
    fn shows_hold_prompt(&self) -> bool {
        matches!(self, Self::ApiTip | Self::FormattingTip | Self::Welcome)
    }
}

// ---------------------------------------------------------------------------
// Spring physics
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Spring {
    current: f32,
    target: f32,
    velocity: f32,
    stiffness: f32,
    damping: f32,
}

impl Spring {
    fn new(value: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            current: value,
            target: value,
            velocity: 0.0,
            stiffness,
            damping,
        }
    }

    fn set_target(&mut self, t: f32) {
        self.target = t;
    }

    #[allow(dead_code)]
    fn snap(&mut self, v: f32) {
        self.current = v;
        self.target = v;
        self.velocity = 0.0;
    }

    /// Advance one tick. dt in seconds.
    fn tick(&mut self, dt: f32) {
        let accel = -self.stiffness * (self.current - self.target) - self.damping * self.velocity;
        self.velocity += accel * dt;
        self.current += self.velocity * dt;
        // Snap if close enough and slow enough
        if (self.current - self.target).abs() < 0.001 && self.velocity.abs() < 0.01 {
            self.current = self.target;
            self.velocity = 0.0;
        }
    }

    fn val(&self) -> f32 {
        self.current
    }

    fn is_settled(&self) -> bool {
        self.current == self.target && self.velocity == 0.0
    }
}

// ---------------------------------------------------------------------------
// Overlay state (shared between render + orchestrator threads)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct OverlayState {
    // Core state
    pub mode: String, // "idle" | "pending" | "recording" | "processing" | "noSpeech" | "error"
    pub bars: [f32; 11],
    pub level: f32,
    pub hands_free: bool,
    pub paused: bool,
    pub hovering: bool,
    pub error: Option<String>,
    pub elapsed: f64,

    // Onboarding
    pub onboarding_step: Option<OnboardingStep>,
    pub hotkey_label: String,

    // Config
    pub gradient_enabled: bool,
    pub always_visible: bool,

    // Pressed state (onboarding key press visual)
    pub is_pressed: bool,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            mode: "idle".into(),
            bars: [0.0; 11],
            level: 0.0,
            hands_free: false,
            paused: false,
            hovering: false,
            error: None,
            elapsed: 0.0,
            onboarding_step: None,
            hotkey_label: "fn".into(),
            gradient_enabled: true,
            always_visible: true,
            is_pressed: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Animation state (owned by render thread, NOT shared)
// ---------------------------------------------------------------------------

struct AnimState {
    bar_springs: [Spring; 11],
    bar_opacity_springs: [Spring; 11],
    pill_scale: Spring,           // 0.5 (minimized) → 1.0 (expanded)
    pill_opacity: Spring,         // For fade in/out
    gradient_energy: Spring,      // 0.0–1.0
    gradient_opacity: Spring,     // 0.0–1.0
    hover_progress: Spring,       // 0→1 for hover transition
    slide_y: Spring,              // Y offset for slide in/out
    stack_y: Spring,              // Active/minimized stack offset
    content_width: Spring,        // Pill content width, including controls
    audio_bounce: Spring,         // Recording scale pulse driven by audio level
    hands_free_progress: Spring,  // 0→1 for pause/stop control entrance
    error_timer: Option<Instant>, // When error was shown
    shake_progress: f32,          // 0→1 for no-speech shake
    shake_active: bool,
    prev_mode: String,
    prev_onboarding_step: Option<OnboardingStep>,
    start_time: Instant,
    last_frame: Instant,
    visible: bool, // Whether overlay should be visible at all
}

impl AnimState {
    fn new() -> Self {
        Self {
            bar_springs: std::array::from_fn(|_| Spring::new(BAR_MIN_H, 280.0, 18.0)),
            bar_opacity_springs: std::array::from_fn(|_| Spring::new(0.25, 180.0, 24.0)),
            pill_scale: Spring::new(0.5, 180.0, 22.0),
            pill_opacity: Spring::new(1.0, 200.0, 20.0),
            gradient_energy: Spring::new(0.0, 120.0, 18.0),
            gradient_opacity: Spring::new(0.0, 160.0, 20.0),
            hover_progress: Spring::new(0.0, 200.0, 22.0),
            slide_y: Spring::new(80.0, 150.0, 20.0), // start off-screen
            stack_y: Spring::new(MINIMIZED_STACK_OFFSET_Y, 150.0, 20.0),
            content_width: Spring::new(IDLE_CONTENT_W, 180.0, 22.0),
            audio_bounce: Spring::new(1.0, 160.0, 24.0),
            hands_free_progress: Spring::new(0.0, 180.0, 22.0),
            error_timer: None,
            shake_progress: 0.0,
            shake_active: false,
            prev_mode: "idle".into(),
            prev_onboarding_step: None,
            start_time: Instant::now(),
            last_frame: Instant::now(),
            visible: true,
        }
    }

    /// Update all animations for one frame. dt in seconds.
    fn tick(&mut self, state: &OverlayState, dt: f32) {
        let is_expanded = state.mode != "idle" || state.onboarding_step.is_some();
        let is_active = is_expanded;
        let is_minimized = state.mode == "idle" && state.onboarding_step.is_none();

        // -- Mode transitions --
        if state.mode != self.prev_mode {
            if state.mode == "noSpeech" {
                self.shake_active = true;
                self.shake_progress = 0.0;
            }
            if state.mode == "recording" && self.prev_mode == "idle" {
                // Slide in
                self.slide_y.set_target(0.0);
                self.visible = true;
            }
            if state.mode == "idle" && !state.always_visible && state.onboarding_step.is_none() {
                // Slide out
                self.slide_y.set_target(SLIDE_OFFSET_Y);
            }
            self.prev_mode = state.mode.clone();
        }

        if state.onboarding_step != self.prev_onboarding_step {
            self.prev_onboarding_step = state.onboarding_step.clone();
        }

        // Visibility
        if state.always_visible || is_active {
            self.slide_y.set_target(0.0);
            self.visible = true;
        } else if state.mode == "idle" && !state.always_visible && state.onboarding_step.is_none() {
            self.slide_y.set_target(SLIDE_OFFSET_Y);
        }
        if self.slide_y.val() > SLIDE_OFFSET_Y - 1.0 && self.slide_y.is_settled() {
            self.visible = false;
        }

        // -- Stack and pill geometry --
        self.stack_y.set_target(if is_expanded {
            ACTIVE_STACK_OFFSET_Y
        } else {
            MINIMIZED_STACK_OFFSET_Y
        });
        self.content_width
            .set_target(pill_content_width(state, None));
        self.hands_free_progress.set_target(
            if state.hands_free && (state.mode == "recording" || state.mode == "processing") {
                1.0
            } else {
                0.0
            },
        );

        let base_scale = if is_expanded {
            EXPANDED_PILL_SCALE
        } else if state.hovering && is_minimized {
            HOVER_PILL_SCALE
        } else {
            IDLE_PILL_SCALE
        };
        let press_scale = if state.is_pressed { PRESS_SCALE } else { 1.0 };
        let proc_scale = if state.mode == "processing" {
            PROCESSING_SCALE
        } else {
            1.0
        };
        self.pill_scale
            .set_target(base_scale * press_scale * proc_scale);

        let audio_bounce = if state.mode == "recording" && !state.paused {
            1.0 + state.level.min(1.0).powf(1.5) * AUDIO_BOUNCE_SCALE
        } else {
            1.0
        };
        self.audio_bounce.set_target(audio_bounce);

        // -- Gradient energy --
        let energy = match state.mode.as_str() {
            "pending" => 0.0,
            "recording" => 1.0,
            "processing" => 0.6,
            _ => {
                if state.hovering {
                    0.15
                } else if state.onboarding_step.is_some() {
                    0.3
                } else {
                    0.0
                }
            }
        };
        let show_gradient = state.mode != "pending"
            && (is_expanded || (state.hovering && is_minimized))
            && state.gradient_enabled;
        self.gradient_energy
            .set_target(if show_gradient { energy } else { 0.0 });
        self.gradient_opacity
            .set_target(if show_gradient { 1.0 } else { 0.0 });

        // -- Hover --
        self.hover_progress.set_target(
            if state.hovering && state.mode == "idle" && state.onboarding_step.is_none() {
                1.0
            } else {
                0.0
            },
        );

        // -- Bar heights --
        let is_processing = state.mode == "processing";
        let t = self.start_time.elapsed().as_secs_f64();
        let wave_t = (t % 1.2) / 1.2;

        for i in 0..BAR_COUNT {
            let scale = POSITION_SCALE[i];
            let bar_ceiling = BAR_MIN_H + (BAR_MAX_H - BAR_MIN_H) * scale;

            let target_h = if is_processing {
                let wave_center = -5.0 + wave_t as f32 * 20.0;
                let dist = (i as f32 - wave_center).abs();
                let wave = (-dist * dist / 6.0).exp();
                (BAR_MIN_H + 14.0 * wave).min(BAR_MAX_H)
            } else if state.mode == "recording" && !state.paused {
                let bar_level = state.bars[i];
                let overall: f32 = state.bars.iter().sum::<f32>() / 11.0;
                let blended = overall * 0.7 + bar_level * 0.3;
                let scaled = (blended / 0.75).min(1.0);
                let driven = scaled.powf(0.6);
                BAR_MIN_H.max((BAR_MIN_H + (bar_ceiling - BAR_MIN_H) * driven).min(bar_ceiling))
            } else {
                BAR_MIN_H // flat bars for idle, paused, noSpeech
            };

            let target_opacity = if is_processing {
                let wave_center = -5.0 + wave_t as f32 * 20.0;
                let dist = (i as f32 - wave_center).abs();
                let wave = (-dist * dist / 6.0).exp();
                0.35 + 0.6 * wave
            } else if state.mode == "recording" && !state.paused {
                0.9
            } else {
                0.25
            };

            self.bar_springs[i].set_target(target_h);
            self.bar_springs[i].tick(dt);
            self.bar_opacity_springs[i].set_target(target_opacity);
            self.bar_opacity_springs[i].tick(dt);
        }

        // -- Error auto-dismiss --
        if state.mode == "error" {
            if self.error_timer.is_none() {
                self.error_timer = Some(Instant::now());
            }
        } else {
            self.error_timer = None;
        }

        // -- Shake animation --
        if self.shake_active {
            self.shake_progress += dt * 2.0; // 0.5s duration
            if self.shake_progress >= 1.0 {
                self.shake_progress = 1.0;
                self.shake_active = false;
            }
        }

        // Tick all springs
        self.pill_scale.tick(dt);
        self.pill_opacity.tick(dt);
        self.gradient_energy.tick(dt);
        self.gradient_opacity.tick(dt);
        self.hover_progress.tick(dt);
        self.slide_y.tick(dt);
        self.stack_y.tick(dt);
        self.content_width.tick(dt);
        self.audio_bounce.tick(dt);
        self.hands_free_progress.tick(dt);
    }

    fn needs_animation(&self, state: &OverlayState) -> bool {
        state.mode == "processing"
            || state.mode == "recording"
            || !self.pill_scale.is_settled()
            || !self.gradient_energy.is_settled()
            || !self.gradient_opacity.is_settled()
            || !self.hover_progress.is_settled()
            || !self.slide_y.is_settled()
            || !self.stack_y.is_settled()
            || !self.content_width.is_settled()
            || !self.audio_bounce.is_settled()
            || !self.hands_free_progress.is_settled()
            || !self.pill_opacity.is_settled()
            || self.shake_active
            || self.bar_springs.iter().any(|s| !s.is_settled())
    }

    /// Should the error be dismissed?
    fn should_dismiss_error(&self) -> bool {
        self.error_timer
            .map(|t| t.elapsed().as_secs_f64() > 2.5)
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Font renderer (fontdue)
// ---------------------------------------------------------------------------

struct FontRenderer {
    font: fontdue::Font,
}

impl FontRenderer {
    fn load() -> Option<Self> {
        // Try loading Windows system font Segoe UI
        let paths = [
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\tahoma.ttf",
        ];
        for path in &paths {
            if let Ok(data) = std::fs::read(path) {
                let settings = fontdue::FontSettings {
                    collection_index: 0,
                    scale: 40.0,
                    load_substitutions: true,
                };
                if let Ok(font) = fontdue::Font::from_bytes(data, settings) {
                    return Some(Self { font });
                }
            }
        }
        None
    }

    /// Measure text width at given size.
    fn measure(&self, text: &str, size: f32) -> f32 {
        let mut width = 0.0;
        for ch in text.chars() {
            let metrics = self.font.metrics(ch, size);
            width += metrics.advance_width;
        }
        width
    }

    /// Render text onto pixmap at (x, y) with given color and size.
    /// y is the baseline. Returns the advance width.
    fn render(
        &self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        mut x: f32,
        y: f32,
        size: f32,
        color: [u8; 4], // RGBA
    ) -> f32 {
        let start_x = x;
        for ch in text.chars() {
            let (metrics, bitmap) = self.font.rasterize(ch, size);
            if !bitmap.is_empty() && metrics.width > 0 && metrics.height > 0 {
                let gx = x + metrics.xmin as f32;
                let gy = y - metrics.height as f32 - metrics.ymin as f32;
                // Composite each pixel
                for row in 0..metrics.height {
                    for col in 0..metrics.width {
                        let alpha = bitmap[row * metrics.width + col];
                        if alpha == 0 {
                            continue;
                        }
                        let px = (gx + col as f32) as i32;
                        let py = (gy + row as f32) as i32;
                        if px < 0
                            || py < 0
                            || px >= pixmap.width() as i32
                            || py >= pixmap.height() as i32
                        {
                            continue;
                        }
                        let idx = (py as u32 * pixmap.width() + px as u32) as usize * 4;
                        let data = pixmap.data_mut();
                        if idx + 3 >= data.len() {
                            continue;
                        }
                        // Alpha blend (premultiplied)
                        let sa = (alpha as u16 * color[3] as u16) / 255;
                        let sr = (color[0] as u16 * sa) / 255;
                        let sg = (color[1] as u16 * sa) / 255;
                        let sb = (color[2] as u16 * sa) / 255;
                        let da = data[idx + 3] as u16;
                        let dr = data[idx] as u16;
                        let dg = data[idx + 1] as u16;
                        let db = data[idx + 2] as u16;
                        let inv_sa = 255 - sa;
                        data[idx] = ((sr + dr * inv_sa / 255).min(255)) as u8;
                        data[idx + 1] = ((sg + dg * inv_sa / 255).min(255)) as u8;
                        data[idx + 2] = ((sb + db * inv_sa / 255).min(255)) as u8;
                        data[idx + 3] = ((sa + da * inv_sa / 255).min(255)) as u8;
                    }
                }
            }
            x += metrics.advance_width;
        }
        x - start_x
    }

    /// Render centered text. Returns the width.
    fn render_centered(
        &self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        cx: f32,
        y: f32,
        size: f32,
        color: [u8; 4],
    ) -> f32 {
        let w = self.measure(text, size);
        self.render(pixmap, text, cx - w / 2.0, y, size, color)
    }
}

// ---------------------------------------------------------------------------
// Statics
// ---------------------------------------------------------------------------

static STATE: OnceLock<Arc<Mutex<OverlayState>>> = OnceLock::new();
static HWND_CELL: OnceLock<isize> = OnceLock::new();
static CONTROL_TX: OnceLock<mpsc::Sender<ControlCommand>> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
enum HitTarget {
    Pill,
    Pause,
    Stop,
}

#[derive(Clone, Copy)]
enum ControlCommand {
    PillClick,
    TogglePause,
    Stop,
}

// ---------------------------------------------------------------------------
// Public API (called from orchestrator thread)
// ---------------------------------------------------------------------------

pub fn update_state(f: impl FnOnce(&mut OverlayState)) {
    if let Some(state) = STATE.get() {
        let mut s = state.lock().unwrap();
        f(&mut s);
        if s.mode != "idle" || s.onboarding_step.is_some() {
            s.hovering = false;
        }
    }
    post_overlay_update();
}

fn post_overlay_update() {
    if let Some(&raw_hwnd) = HWND_CELL.get() {
        let hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);
        unsafe {
            let _ = PostMessageW(hwnd, WM_YAP_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}

fn on_pill_click_callback() {
    dispatch_control_command(ControlCommand::PillClick);
}

fn init_control_dispatcher(app: &tauri::AppHandle) {
    if CONTROL_TX.get().is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel::<ControlCommand>();
    if CONTROL_TX.set(tx).is_err() {
        return;
    }

    let app = app.clone();
    std::thread::Builder::new()
        .name("yap-win-overlay-controls".into())
        .spawn(move || {
            use tauri::Manager;

            while let Ok(command) = rx.recv() {
                let orch: tauri::State<'_, Arc<orchestrator::Orchestrator>> = app.state();
                match command {
                    ControlCommand::PillClick => orch.on_pill_click(),
                    ControlCommand::TogglePause => orch.toggle_pause(),
                    ControlCommand::Stop => orch.stop_hands_free(),
                }
            }
        })
        .expect("failed to spawn overlay control thread");
}

fn dispatch_control_command(command: ControlCommand) {
    if let Some(tx) = CONTROL_TX.get() {
        let _ = tx.send(command);
    }
}

fn init_mouse_hook() {
    std::thread::Builder::new()
        .name("yap-win-overlay-mouse-hook".into())
        .spawn(move || unsafe {
            let hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(low_level_mouse_proc),
                HINSTANCE::default(),
                0,
            );

            let hook = match hook {
                Ok(hook) => hook,
                Err(e) => {
                    orchestrator::log::info(&format!("Win32 mouse hook failed: {e}"));
                    return;
                }
            };

            orchestrator::log::info("Win32 mouse hook installed");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnhookWindowsHookEx(hook);
        })
        .expect("failed to spawn overlay mouse hook thread");
}

unsafe extern "system" fn low_level_mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
        let mouse = *(lparam.0 as *const MSLLHOOKSTRUCT);
        handle_global_mouse_move(mouse.pt);
    } else if code >= 0 && wparam.0 as u32 == WM_LBUTTONDOWN {
        let mouse = *(lparam.0 as *const MSLLHOOKSTRUCT);
        if handle_global_mouse_down(mouse.pt) {
            return LRESULT(1);
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

fn handle_global_mouse_down(point: POINT) -> bool {
    match hit_target_from_current_geometry(point) {
        Some(HitTarget::Pill) => {
            on_pill_click_callback();
            true
        }
        Some(HitTarget::Pause) => {
            handle_hands_free_control_click(HitTarget::Pause);
            true
        }
        Some(HitTarget::Stop) => {
            handle_hands_free_control_click(HitTarget::Stop);
            true
        }
        _ => false,
    }
}

fn handle_global_mouse_move(point: POINT) {
    let should_hover = matches!(
        hit_target_from_current_geometry(point),
        Some(HitTarget::Pill)
    ) && STATE
        .get()
        .map(|state| {
            let state = state.lock().unwrap();
            state.mode == "idle" && state.onboarding_step.is_none()
        })
        .unwrap_or(false);

    if set_hovering(should_hover) {
        post_overlay_update();
    }
}

fn handle_hands_free_control_click(target: HitTarget) {
    let mut control_command = None;

    update_state(|st| {
        if !st.hands_free || st.mode != "recording" {
            return;
        }

        match target {
            HitTarget::Pause => {
                st.paused = !st.paused;
                control_command = Some(ControlCommand::TogglePause);
            }
            HitTarget::Stop => {
                st.mode = "processing".into();
                st.hands_free = false;
                st.paused = false;
                control_command = Some(ControlCommand::Stop);
            }
            HitTarget::Pill => {}
        }
    });

    if let Some(command) = control_command {
        dispatch_control_command(command);
    }
}

/// Spawn the overlay window on a dedicated thread.
pub fn spawn(app: &tauri::AppHandle) {
    let _ = STATE.set(Arc::new(Mutex::new(OverlayState::default())));
    init_control_dispatcher(app);
    init_mouse_hook();

    std::thread::Builder::new()
        .name("yap-win-overlay".into())
        .spawn(|| {
            unsafe { run_window_loop() };
        })
        .expect("failed to spawn overlay thread");
}

// ---------------------------------------------------------------------------
// Win32 window loop
// ---------------------------------------------------------------------------

unsafe fn run_window_loop() {
    let class_name = w!("YapOverlay");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        hInstance: HINSTANCE::default(),
        lpszClassName: class_name,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassExW(&wc);

    // Position: bottom-center of the usable primary monitor work area so the
    // pill clears the Windows taskbar.
    let work_area = primary_work_area();
    let x = work_area.left + ((work_area.right - work_area.left) - CANVAS_W) / 2;
    let y = work_area.bottom - CANVAS_H;

    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
        class_name,
        w!("Yap Overlay"),
        WS_POPUP,
        x,
        y,
        CANVAS_W,
        CANVAS_H,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let _ = HWND_CELL.set(hwnd.0 as isize);

    // Initial render
    let font = FontRenderer::load();
    let mut anim = AnimState::new();

    render_frame(hwnd, &mut anim, &font);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

    // 16ms timer gives the native gradient the same visual cadence as the
    // Svelte/CSS renderer on platforms where that path is used.
    let _ = SetTimer(hwnd, 1, FRAME_INTERVAL_MS, None);

    // Store animation + font state in window's user data
    let ctx = Box::new(RenderContext { anim, font });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ctx) as isize);

    // Message loop
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

struct RenderContext {
    anim: AnimState,
    font: Option<FontRenderer>,
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_YAP_UPDATE | WM_TIMER => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RenderContext;
            if !ptr.is_null() {
                let ctx = &mut *ptr;
                // Check error auto-dismiss
                if ctx.anim.should_dismiss_error() {
                    update_state(|st| {
                        st.mode = "idle".into();
                        st.error = None;
                    });
                }
                let state = STATE.get().map(|s| s.lock().unwrap().clone());
                if state.is_some() {
                    let force_render = msg == WM_YAP_UPDATE;
                    if force_render {
                        render_frame(hwnd, &mut ctx.anim, &ctx.font);
                        return LRESULT(0);
                    }
                }
                if let Some(current) = &state {
                    if ctx.anim.needs_animation(current) {
                        render_frame(hwnd, &mut ctx.anim, &ctx.font);
                    }
                }
            }
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            position_overlay_window(hwnd);
            post_overlay_update();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            // Track mouse leave
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);

            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RenderContext;
            if !ptr.is_null() {
                let state = STATE.get().map(|s| s.lock().unwrap().clone());
                if let Some(state) = state {
                    let ctx = &mut *ptr;
                    let mouse_x = get_x_lparam(lparam) as f32;
                    let mouse_y = get_y_lparam(lparam) as f32;
                    if set_hovering(
                        state.mode == "idle"
                            && state.onboarding_step.is_none()
                            && hit_test_overlay(mouse_x, mouse_y, &state, &ctx.anim),
                    ) {
                        let _ = PostMessageW(hwnd, WM_YAP_UPDATE, WPARAM(0), LPARAM(0));
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if set_hovering(false) {
                let _ = PostMessageW(hwnd, WM_YAP_UPDATE, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let state = STATE.get().map(|s| s.lock().unwrap().clone());
            if let Some(state) = state {
                // Determine what was clicked based on mouse position
                let mouse_x = (lparam.0 & 0xFFFF) as i16 as f32;
                let mouse_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RenderContext;
                let anim = if ptr.is_null() {
                    None
                } else {
                    Some(&(*ptr).anim)
                };
                let geom = overlay_geometry(&state, anim);
                if let Some(anim) = anim {
                    if !hit_test_overlay(mouse_x, mouse_y, &state, anim) {
                        return LRESULT(0);
                    }
                }

                if state.hands_free && state.mode == "recording" {
                    // Check pause/stop button hits
                    let button_offset = CONTROL_BUTTON_OFFSET
                        * geom.content_scale
                        * hands_free_hit_progress(&state, &geom);
                    let pause_cx = geom.content_cx - button_offset;
                    let stop_cx = geom.content_cx + button_offset;

                    let pause_dist =
                        ((mouse_x - pause_cx).powi(2) + (mouse_y - geom.pill_cy).powi(2)).sqrt();
                    let stop_dist =
                        ((mouse_x - stop_cx).powi(2) + (mouse_y - geom.pill_cy).powi(2)).sqrt();

                    let control_hit_radius = CONTROL_HIT_RADIUS * geom.content_scale.max(0.75);
                    if pause_dist < control_hit_radius {
                        handle_hands_free_control_click(HitTarget::Pause);
                    } else if stop_dist < control_hit_radius {
                        handle_hands_free_control_click(HitTarget::Stop);
                    }
                } else if state.mode != "processing" {
                    on_pill_click_callback();
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn primary_work_area() -> RECT {
    unsafe {
        let primary_center = POINT {
            x: GetSystemMetrics(SM_CXSCREEN) / 2,
            y: GetSystemMetrics(SM_CYSCREEN) / 2,
        };
        let monitor = MonitorFromPoint(primary_center, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            info.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: GetSystemMetrics(SM_CXSCREEN),
                bottom: GetSystemMetrics(SM_CYSCREEN),
            }
        }
    }
}

fn position_overlay_window(hwnd: HWND) {
    unsafe {
        let work_area = primary_work_area();
        let x = work_area.left + ((work_area.right - work_area.left) - CANVAS_W) / 2;
        let y = work_area.bottom - CANVAS_H;
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            CANVAS_W,
            CANVAS_H,
            SWP_NOACTIVATE | SWP_NOSIZE,
        );
    }
}

fn get_x_lparam(lparam: LPARAM) -> i32 {
    (lparam.0 & 0xFFFF) as i16 as i32
}

fn get_y_lparam(lparam: LPARAM) -> i32 {
    ((lparam.0 >> 16) & 0xFFFF) as i16 as i32
}

fn set_hovering(hovering: bool) -> bool {
    if let Some(state) = STATE.get() {
        let mut s = state.lock().unwrap();
        if s.hovering != hovering {
            s.hovering = hovering;
            return true;
        }
    }
    false
}

fn hit_target_from_current_geometry(point: POINT) -> Option<HitTarget> {
    let state = STATE
        .get()
        .and_then(|state| state.lock().ok().map(|guard| guard.clone()))?;
    if state.mode == "processing"
        || (state.mode == "idle" && state.onboarding_step.is_none() && !state.always_visible)
    {
        return None;
    }

    let &raw_hwnd = HWND_CELL.get()?;
    let hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);
    let mut rect = RECT::default();
    unsafe {
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return None;
        }
    }

    let x = point.x as f32 - rect.left as f32;
    let y = point.y as f32 - rect.top as f32;
    let geom = overlay_geometry(&state, None);

    if state.hands_free && state.mode == "recording" {
        let button_offset =
            CONTROL_BUTTON_OFFSET * geom.content_scale * hands_free_hit_progress(&state, &geom);
        let pause_cx = geom.content_cx - button_offset;
        let stop_cx = geom.content_cx + button_offset;
        let control_hit_radius = CONTROL_HIT_RADIUS * geom.content_scale.max(0.75) + 8.0;

        if distance(x, y, pause_cx, geom.pill_cy) <= control_hit_radius {
            return Some(HitTarget::Pause);
        }
        if distance(x, y, stop_cx, geom.pill_cy) <= control_hit_radius {
            return Some(HitTarget::Stop);
        }

        return None;
    }

    let half_w = geom.pill_w / 2.0 + PILL_HIT_PADDING;
    let half_h = geom.pill_h / 2.0 + PILL_HIT_PADDING;
    (x >= geom.content_cx - half_w
        && x <= geom.content_cx + half_w
        && y >= geom.pill_cy - half_h
        && y <= geom.pill_cy + half_h)
        .then_some(HitTarget::Pill)
}

fn distance(x: f32, y: f32, cx: f32, cy: f32) -> f32 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
}

struct OverlayGeometry {
    cx: f32,
    pill_cy: f32,
    content_cx: f32,
    pill_w: f32,
    pill_h: f32,
    content_scale: f32,
    hands_free_progress: f32,
    shake_offset: f32,
}

fn overlay_geometry(state: &OverlayState, anim: Option<&AnimState>) -> OverlayGeometry {
    let cx = CANVAS_W as f32 / 2.0;
    let slide_y = anim.map(|a| a.slide_y.val()).unwrap_or(0.0);
    let stack_y = anim.map(|a| a.stack_y.val()).unwrap_or_else(|| {
        let is_expanded = state.mode != "idle" || state.onboarding_step.is_some();
        if is_expanded {
            ACTIVE_STACK_OFFSET_Y
        } else {
            MINIMIZED_STACK_OFFSET_Y
        }
    });
    let pill_scale = anim.map(|a| a.pill_scale.val()).unwrap_or_else(|| {
        let is_expanded = state.mode != "idle" || state.onboarding_step.is_some();
        if is_expanded {
            EXPANDED_PILL_SCALE
        } else if state.hovering && state.mode == "idle" && state.onboarding_step.is_none() {
            HOVER_PILL_SCALE
        } else {
            IDLE_PILL_SCALE
        }
    });
    let audio_bounce = anim.map(|a| a.audio_bounce.val()).unwrap_or_else(|| {
        if state.mode == "recording" && !state.paused {
            1.0 + state.level.min(1.0).powf(1.5) * AUDIO_BOUNCE_SCALE
        } else {
            1.0
        }
    });
    let content_scale = pill_scale * audio_bounce;
    let shake_offset = anim
        .filter(|a| a.shake_active)
        .map(|a| {
            let p = a.shake_progress;
            4.0 * (p * std::f32::consts::PI * 6.0).sin() * (1.0 - p)
        })
        .unwrap_or(0.0);
    let is_expanded = state.mode != "idle" || state.onboarding_step.is_some();
    let content_w = anim
        .map(|a| a.content_width.val())
        .unwrap_or_else(|| pill_content_width(state, None));
    let hands_free_progress = anim
        .map(|a| a.hands_free_progress.val())
        .unwrap_or_else(|| {
            if state.hands_free && (state.mode == "recording" || state.mode == "processing") {
                1.0
            } else {
                0.0
            }
        });
    let pill_h = if is_expanded {
        EXPANDED_PILL_H
    } else {
        IDLE_PILL_H
    } * content_scale;

    OverlayGeometry {
        cx,
        pill_cy: CANVAS_H as f32 - PILL_CENTER_BOTTOM_INSET + stack_y + slide_y,
        content_cx: cx + shake_offset,
        pill_w: content_w * content_scale,
        pill_h,
        content_scale,
        hands_free_progress,
        shake_offset,
    }
}

fn pill_content_width(state: &OverlayState, font: Option<&FontRenderer>) -> f32 {
    if state.hands_free && (state.mode == "recording" || state.mode == "processing") {
        HANDS_FREE_CONTENT_W
    } else if state.mode != "idle" || state.onboarding_step.is_some() {
        if let Some(ref step) = state.onboarding_step {
            if step.shows_hold_prompt() && (state.mode == "idle" || state.mode == "noSpeech") {
                if let Some(fr) = font {
                    let suffix = if state.onboarding_step.as_ref() == Some(&OnboardingStep::Welcome)
                    {
                        "to finish"
                    } else {
                        "to continue"
                    };
                    fr.measure("Hold ", 12.0)
                        + fr.measure(&state.hotkey_label, 12.0)
                        + 20.0
                        + 6.0
                        + fr.measure(suffix, 12.0)
                        + 24.0
                } else {
                    hold_prompt_fallback_width(&state.hotkey_label)
                }
            } else {
                STANDARD_CONTENT_W
            }
        } else {
            STANDARD_CONTENT_W
        }
    } else {
        IDLE_CONTENT_W
    }
}

fn hold_prompt_fallback_width(hotkey_label: &str) -> f32 {
    let key_w = hotkey_label.chars().count() as f32 * 7.0 + 20.0;
    let suffix_w = 68.0;
    28.0 + key_w + 6.0 + suffix_w + 24.0
}

fn hit_test_overlay(x: f32, y: f32, state: &OverlayState, anim: &AnimState) -> bool {
    if state.mode == "processing" {
        return false;
    }

    let geom = overlay_geometry(state, Some(anim));

    if state.hands_free && state.mode == "recording" {
        let button_offset =
            CONTROL_BUTTON_OFFSET * geom.content_scale * hands_free_hit_progress(state, &geom);
        let pause_cx = geom.content_cx - button_offset;
        let stop_cx = geom.content_cx + button_offset;
        let pause_dist = ((x - pause_cx).powi(2) + (y - geom.pill_cy).powi(2)).sqrt();
        let stop_dist = ((x - stop_cx).powi(2) + (y - geom.pill_cy).powi(2)).sqrt();
        let control_hit_radius = CONTROL_HIT_RADIUS * geom.content_scale.max(0.75);
        if pause_dist < control_hit_radius || stop_dist < control_hit_radius {
            return true;
        }
    }

    let half_w = geom.pill_w / 2.0 + PILL_HIT_PADDING;
    let half_h = geom.pill_h / 2.0 + PILL_HIT_PADDING;
    x >= geom.content_cx - half_w
        && x <= geom.content_cx + half_w
        && y >= geom.pill_cy - half_h
        && y <= geom.pill_cy + half_h
}

fn hands_free_hit_progress(state: &OverlayState, geom: &OverlayGeometry) -> f32 {
    if state.hands_free && state.mode == "recording" {
        1.0
    } else {
        geom.hands_free_progress
    }
}

fn hands_free_visual_progress(state: &OverlayState, geom: &OverlayGeometry) -> f32 {
    if state.hands_free && state.mode == "recording" {
        1.0
    } else {
        geom.hands_free_progress
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_frame(hwnd: HWND, anim: &mut AnimState, font: &Option<FontRenderer>) {
    let state = match STATE.get() {
        Some(s) => s.lock().unwrap().clone(),
        None => return,
    };

    // Advance animation using real frame time so timer jitter does not turn
    // spring movement into visible jumps.
    let now = Instant::now();
    let mut dt = now.duration_since(anim.last_frame).as_secs_f32();
    if dt <= 0.0 {
        dt = 1.0 / 60.0;
    }
    dt = dt.min(MAX_ANIMATION_DT);
    anim.last_frame = now;
    anim.tick(&state, dt);

    let w = CANVAS_W as u32;
    let h = CANVAS_H as u32;

    let mut pixmap = match tiny_skia::Pixmap::new(w, h) {
        Some(p) => p,
        None => return,
    };

    // Don't render if not visible and settled
    if !anim.visible && anim.slide_y.is_settled() {
        blit_to_layered_window(hwnd, &pixmap, w, h);
        return;
    }

    let geom = overlay_geometry(&state, Some(anim));
    let cx = geom.cx;
    let pill_cy = geom.pill_cy;

    // -- Lava lamp gradient --
    if anim.gradient_opacity.val() > 0.01 {
        render_gradient(&mut pixmap, anim, cx, pill_cy);
    }

    // -- Onboarding card (above pill) --
    let is_expanded = state.mode != "idle" || state.onboarding_step.is_some();
    if let Some(ref step) = state.onboarding_step {
        if state.mode == "idle" || state.mode == "noSpeech" {
            if let Some(ref fr) = font {
                render_onboarding_card(
                    &mut pixmap,
                    fr,
                    step,
                    &state.hotkey_label,
                    cx,
                    pill_cy - CARD_GAP_Y,
                );
            }
        }
    }

    // -- Error card (above pill, matching onboarding/transient prompt cards) --
    if state.mode == "error" {
        if let Some(ref msg) = state.error {
            if let Some(ref fr) = font {
                render_error_card(&mut pixmap, fr, msg, cx, pill_cy - CARD_GAP_Y);
            }
        }
    }

    // -- Elapsed time (above pill, hands-free) --
    if state.mode == "recording" && state.elapsed >= 10.0 {
        if let Some(ref fr) = font {
            let elapsed_text = format_elapsed(state.elapsed);
            fr.render_centered(
                &mut pixmap,
                &elapsed_text,
                cx,
                pill_cy - TIMER_GAP_Y,
                11.0,
                [255, 255, 255, 128],
            );
        }
    }

    // -- Pill background --
    let pill_scale = geom.content_scale;
    let pill_w = geom.pill_w;
    let pill_h = geom.pill_h;

    let pill_x = cx - pill_w / 2.0 + geom.shake_offset;
    let pill_y = pill_cy - pill_h / 2.0;
    let pill_r = pill_h / 2.0;

    // Pill capsule background
    let capsule = rounded_rect(pill_x, pill_y, pill_w, pill_h, pill_r);
    let mut bg_paint = tiny_skia::Paint::default();
    let bg_alpha = if is_expanded { 0.75 } else { 0.4 };
    bg_paint.set_color(tiny_skia::Color::from_rgba(0.06, 0.06, 0.1, bg_alpha).unwrap());
    bg_paint.anti_alias = true;
    pixmap.fill_path(
        &capsule,
        &bg_paint,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );

    // Pill border
    let border_alpha = if is_expanded { 0.3 } else { 0.35 };
    let mut border_paint = tiny_skia::Paint::default();
    border_paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, border_alpha).unwrap());
    border_paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: if is_expanded { 1.0 } else { 1.5 },
        ..Default::default()
    };
    pixmap.stroke_path(&capsule, &border_paint, &stroke, Default::default(), None);

    // -- Pill content --
    let pill_content_cx = geom.content_cx;

    if let Some(ref step) = state.onboarding_step {
        if step.shows_hold_prompt() && (state.mode == "idle" || state.mode == "noSpeech") {
            // "Hold [key] to continue/finish" inside pill
            if let Some(ref fr) = font {
                let suffix = if *step == OnboardingStep::Welcome {
                    "to finish"
                } else {
                    "to continue"
                };
                let font_size = 12.0 * pill_scale;
                let gap = 6.0 * pill_scale;
                let hold_w = fr.measure("Hold ", font_size);
                let key_w = fr.measure(&state.hotkey_label, font_size) + 20.0 * pill_scale;
                let suffix_w = fr.measure(suffix, font_size);
                let total_w = hold_w + key_w + gap + suffix_w;
                let start_x = pill_content_cx - total_w / 2.0;

                let text_y = pill_cy + 4.0 * pill_scale;
                let color = [255, 255, 255, 200];
                let mut x = start_x;
                x += fr.render(&mut pixmap, "Hold ", x, text_y, font_size, color);
                // Draw keycap
                render_keycap(
                    &mut pixmap,
                    font,
                    &state.hotkey_label,
                    x,
                    pill_cy,
                    font_size,
                );
                x += key_w + gap;
                fr.render(&mut pixmap, suffix, x, text_y, font_size, color);
            }
        } else if state.mode == "idle" || state.mode == "noSpeech" {
            // Flat bars for onboarding idle
            render_flat_bars(&mut pixmap, pill_content_cx, pill_cy, pill_scale);
        }
    }

    match state.mode.as_str() {
        "recording" | "processing" => {
            let controls_progress = hands_free_visual_progress(&state, &geom);
            if state.hands_free || controls_progress > 0.01 {
                render_hands_free_content(
                    &mut pixmap,
                    anim,
                    &state,
                    pill_content_cx,
                    pill_cy,
                    pill_scale,
                    controls_progress,
                );
            } else {
                render_bars(&mut pixmap, anim, pill_content_cx, pill_cy, pill_scale);
            }
        }
        "pending" => {
            render_bars(&mut pixmap, anim, pill_content_cx, pill_cy, pill_scale);
        }
        "error" => {
            render_flat_bars(&mut pixmap, pill_content_cx, pill_cy, pill_scale);
        }
        "noSpeech" => {
            if state.onboarding_step.is_none()
                || !state.onboarding_step.as_ref().unwrap().shows_hold_prompt()
            {
                render_flat_bars(&mut pixmap, pill_content_cx, pill_cy, pill_scale);
            }
        }
        "idle" => {}
        _ => {}
    }

    // -- Hover tooltip (above pill when minimized, matching the macOS sidecar) --
    if anim.hover_progress.val() > 0.01 && state.mode == "idle" && state.onboarding_step.is_none() {
        if let Some(ref fr) = font {
            let hover_progress = anim.hover_progress.val().clamp(0.0, 1.0);
            let alpha = (hover_progress * 255.0) as u8;
            let tooltip_y = pill_cy - pill_h / 2.0
                + HOVER_TOOLTIP_OFFSET_Y
                + (1.0 - hover_progress) * HOVER_TOOLTIP_TRANSITION_Y;
            fr.render_centered(
                &mut pixmap,
                "Click to start transcribing",
                cx,
                tooltip_y,
                13.0,
                [255, 255, 255, alpha],
            );
        }
    }

    blit_to_layered_window(hwnd, &pixmap, w, h);
}

// ---------------------------------------------------------------------------
// Sub-renderers
// ---------------------------------------------------------------------------

fn render_gradient(pixmap: &mut tiny_skia::Pixmap, anim: &AnimState, cx: f32, cy: f32) {
    let energy = anim.gradient_energy.val();
    let opacity = anim.gradient_opacity.val().clamp(0.0, 1.0);
    let cy = cy + GRADIENT_OFFSET_Y;
    let t = anim.start_time.elapsed().as_secs_f64();
    let speed = 0.1 + energy as f64 * 0.08;
    let brightness = 0.12 + energy * 0.2;
    let group_x = (t * 0.42 * speed).cos() as f32 * 18.0 * GRADIENT_MOTION_SCALE;
    let group_y = (t * 0.34 * speed).sin() as f32 * 8.0 * GRADIENT_MOTION_SCALE;

    // Four oversized, overlapping ellipses. Windows layered windows do not get
    // the CSS blur used by the Svelte path, so the softness is baked into a
    // broad falloff and slow grouped motion instead of obvious orbiting.
    struct Blob {
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    }

    let blobs = [
        Blob {
            x: cx + group_x - 44.0 + (t * 0.58 * speed).sin() as f32 * 13.0 * GRADIENT_MOTION_SCALE,
            y: cy + 24.0 + group_y + (t * 0.43 * speed).cos() as f32 * 7.0 * GRADIENT_MOTION_SCALE,
            rx: 180.0,
            ry: 92.0,
            r: 147.0 / 255.0,
            g: 51.0 / 255.0,
            b: 234.0 / 255.0,
            a: brightness * 0.5,
        },
        Blob {
            x: cx
                + group_x
                + 38.0
                + (t * 0.5 * speed + 1.3).cos() as f32 * 15.0 * GRADIENT_MOTION_SCALE,
            y: cy
                + 30.0
                + group_y
                + (t * 0.37 * speed + 0.8).sin() as f32 * 9.0 * GRADIENT_MOTION_SCALE,
            rx: 210.0,
            ry: 104.0,
            r: 59.0 / 255.0,
            g: 130.0 / 255.0,
            b: 246.0 / 255.0,
            a: brightness * 0.5,
        },
        Blob {
            x: cx + group_x - 16.0
                + (t * 0.46 * speed + 2.6).sin() as f32 * 12.0 * GRADIENT_MOTION_SCALE,
            y: cy
                + 38.0
                + group_y
                + (t * 0.41 * speed + 1.9).cos() as f32 * 8.0 * GRADIENT_MOTION_SCALE,
            rx: 168.0,
            ry: 84.0,
            r: 34.0 / 255.0,
            g: 211.0 / 255.0,
            b: 238.0 / 255.0,
            a: brightness * 0.42,
        },
        Blob {
            x: cx
                + group_x
                + 10.0
                + (t * 0.39 * speed + 4.1).cos() as f32 * 14.0 * GRADIENT_MOTION_SCALE,
            y: cy
                + 20.0
                + group_y
                + (t * 0.52 * speed + 3.4).sin() as f32 * 7.0 * GRADIENT_MOTION_SCALE,
            rx: 194.0,
            ry: 94.0,
            r: 99.0 / 255.0,
            g: 102.0 / 255.0,
            b: 241.0 / 255.0,
            a: brightness * 0.46,
        },
    ];

    // Render each blob as a radial gradient ellipse
    for blob in &blobs {
        // Direct pixel compositing for soft elliptical blobs (fast enough at this scale)
        let (bx0, by0) = (
            (blob.x - blob.rx * 1.9) as i32,
            (blob.y - blob.ry * 1.9) as i32,
        );
        let (bx1, by1) = (
            (blob.x + blob.rx * 1.9) as i32,
            (blob.y + blob.ry * 1.9) as i32,
        );
        let bx0 = bx0.max(0) as u32;
        let by0 = by0.max(0) as u32;
        let bx1 = bx1.min(pixmap.width() as i32) as u32;
        let by1 = by1.min(pixmap.height() as i32) as u32;

        let pw = pixmap.width();
        let data = pixmap.data_mut();

        for py in by0..by1 {
            for px in bx0..bx1 {
                let dx = (px as f32 - blob.x) / blob.rx;
                let dy = (py as f32 - blob.y) / blob.ry;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > 3.61 {
                    continue;
                } // 1.9^2, beyond visible range

                let falloff = (-dist_sq * 0.9).exp();
                let dither = (gradient_dither(px, py) - 0.5) * GRADIENT_DITHER_ALPHA * opacity;
                let alpha = (blob.a * opacity * falloff * 255.0 + dither).clamp(0.0, 255.0) as u16;
                if alpha == 0 {
                    continue;
                }

                let idx = (py * pw + px) as usize * 4;
                if idx + 3 >= data.len() {
                    continue;
                }

                let sr = (blob.r * 255.0) as u16;
                let sg = (blob.g * 255.0) as u16;
                let sb = (blob.b * 255.0) as u16;

                // Premultiplied alpha blend (additive for glow effect)
                let pr = (sr * alpha / 255) as u8;
                let pg = (sg * alpha / 255) as u8;
                let pb = (sb * alpha / 255) as u8;
                let pa = alpha as u8;

                let dr = data[idx] as u16;
                let dg = data[idx + 1] as u16;
                let db = data[idx + 2] as u16;
                let da = data[idx + 3] as u16;

                let inv_a = 255 - alpha;
                data[idx] = ((pr as u16 + dr * inv_a / 255).min(255)) as u8;
                data[idx + 1] = ((pg as u16 + dg * inv_a / 255).min(255)) as u8;
                data[idx + 2] = ((pb as u16 + db * inv_a / 255).min(255)) as u8;
                data[idx + 3] = ((pa as u16 + da * inv_a / 255).min(255)) as u8;
            }
        }
    }
}

fn gradient_dither(x: u32, y: u32) -> f32 {
    let mut n = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263));
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xff) as f32 / 255.0
}

fn render_bars(pixmap: &mut tiny_skia::Pixmap, anim: &AnimState, cx: f32, cy: f32, scale: f32) {
    let bar_w = BAR_W * scale;
    let bar_gap = BAR_GAP * scale;
    let bars_total_w = BARS_TOTAL_W * scale;
    let start_x = cx - bars_total_w / 2.0;

    for i in 0..BAR_COUNT {
        let bar_h = anim.bar_springs[i].val() * scale;
        let x = start_x + i as f32 * (bar_w + bar_gap);
        let y = cy - bar_h / 2.0;
        let opacity = anim.bar_opacity_springs[i].val();

        let bar_path = rounded_rect(x, y, bar_w, bar_h, 1.5 * scale);
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, opacity).unwrap());
        paint.anti_alias = true;
        pixmap.fill_path(
            &bar_path,
            &paint,
            tiny_skia::FillRule::Winding,
            Default::default(),
            None,
        );
    }
}

fn render_hands_free_content(
    pixmap: &mut tiny_skia::Pixmap,
    anim: &AnimState,
    state: &OverlayState,
    cx: f32,
    cy: f32,
    scale: f32,
    controls_progress: f32,
) {
    // Bars in center
    if state.paused {
        render_flat_bars(pixmap, cx, cy, scale);
    } else {
        render_bars(pixmap, anim, cx, cy, scale);
    }

    // Pause button (left)
    let controls_progress = controls_progress.clamp(0.0, 1.0);
    if controls_progress > 0.01 {
        let control_scale = scale * controls_progress;
        let control_alpha = controls_progress;
        let pause_cx = cx - CONTROL_BUTTON_OFFSET * scale * controls_progress;
        render_circle_button(
            pixmap,
            pause_cx,
            cy,
            13.0 * control_scale,
            [255, 255, 255, (38.0 * control_alpha) as u8],
        );
        if state.paused {
            draw_play_icon(pixmap, pause_cx, cy, control_scale, control_alpha);
        } else {
            draw_pause_icon(pixmap, pause_cx, cy, control_scale, control_alpha);
        }

        // Stop button (right)
        let stop_cx = cx + CONTROL_BUTTON_OFFSET * scale * controls_progress;
        render_circle_button(
            pixmap,
            stop_cx,
            cy,
            13.0 * control_scale,
            [217, 48, 48, (217.0 * control_alpha) as u8],
        );
        draw_stop_icon(pixmap, stop_cx, cy, control_scale, control_alpha);
    }
}

fn render_flat_bars(pixmap: &mut tiny_skia::Pixmap, cx: f32, cy: f32, scale: f32) {
    let bar_w = BAR_W * scale;
    let bar_gap = BAR_GAP * scale;
    let bar_h = BAR_MIN_H * scale;
    let start_x = cx - BARS_TOTAL_W * scale / 2.0;
    for i in 0..BAR_COUNT {
        let x = start_x + i as f32 * (bar_w + bar_gap);
        let y = cy - bar_h / 2.0;
        let bar_path = rounded_rect(x, y, bar_w, bar_h, 1.5 * scale);
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.25).unwrap());
        paint.anti_alias = true;
        pixmap.fill_path(
            &bar_path,
            &paint,
            tiny_skia::FillRule::Winding,
            Default::default(),
            None,
        );
    }
}

fn render_circle_button(pixmap: &mut tiny_skia::Pixmap, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
    let mut pb = tiny_skia::PathBuilder::new();
    // Approximate circle with 4 cubic beziers
    let k = 0.5522847498; // magic number for circle approximation
    let kr = k * r;
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
    pb.cubic_to(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
    pb.cubic_to(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
    pb.cubic_to(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(
            tiny_skia::Color::from_rgba(
                color[0] as f32 / 255.0,
                color[1] as f32 / 255.0,
                color[2] as f32 / 255.0,
                color[3] as f32 / 255.0,
            )
            .unwrap(),
        );
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Default::default(),
            None,
        );
    }
}

fn draw_pause_icon(pixmap: &mut tiny_skia::Pixmap, cx: f32, cy: f32, scale: f32, alpha: f32) {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.9 * alpha).unwrap());
    paint.anti_alias = true;
    // Two vertical bars
    let bar_w = 2.5 * scale;
    let bar_h = 10.0 * scale;
    let gap = 3.5 * scale;
    let left = rounded_rect(
        cx - gap / 2.0 - bar_w,
        cy - bar_h / 2.0,
        bar_w,
        bar_h,
        1.0 * scale,
    );
    let right = rounded_rect(cx + gap / 2.0, cy - bar_h / 2.0, bar_w, bar_h, 1.0 * scale);
    pixmap.fill_path(
        &left,
        &paint,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );
    pixmap.fill_path(
        &right,
        &paint,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );
}

fn draw_play_icon(pixmap: &mut tiny_skia::Pixmap, cx: f32, cy: f32, scale: f32, alpha: f32) {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.9 * alpha).unwrap());
    paint.anti_alias = true;
    let mut pb = tiny_skia::PathBuilder::new();
    let size = 6.0 * scale;
    pb.move_to(cx - size * 0.4, cy - size);
    pb.line_to(cx + size * 0.8, cy);
    pb.line_to(cx - size * 0.4, cy + size);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Default::default(),
            None,
        );
    }
}

fn draw_stop_icon(pixmap: &mut tiny_skia::Pixmap, cx: f32, cy: f32, scale: f32, alpha: f32) {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, alpha).unwrap());
    paint.anti_alias = true;
    let size = 4.5 * scale;
    let rect = rounded_rect(cx - size, cy - size, size * 2.0, size * 2.0, 1.5 * scale);
    pixmap.fill_path(
        &rect,
        &paint,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );
}

fn render_error_card(
    pixmap: &mut tiny_skia::Pixmap,
    font: &FontRenderer,
    message: &str,
    cx: f32,
    cy: f32,
) {
    let font_size = 14.0;
    let icon_w = 10.0;
    let gap = 7.0;
    let text_w = font.measure(message, font_size);
    let content_w = icon_w + gap + text_w;
    let pad_h = 16.0;
    let pad_v = 10.0;
    let card_w = content_w + pad_h * 2.0;
    let card_h = font_size + pad_v * 2.0;
    let card_r = card_h / 2.0;

    let card_x = cx - card_w / 2.0;
    let card_y = cy - card_h / 2.0;
    let card_path = rounded_rect(card_x, card_y, card_w, card_h, card_r);

    let mut bg_paint = tiny_skia::Paint::default();
    bg_paint.set_color(tiny_skia::Color::from_rgba(0.06, 0.06, 0.1, 0.75).unwrap());
    bg_paint.anti_alias = true;
    pixmap.fill_path(
        &card_path,
        &bg_paint,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );

    let mut border_paint = tiny_skia::Paint::default();
    border_paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.3).unwrap());
    border_paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(&card_path, &border_paint, &stroke, Default::default(), None);

    // Warning triangle icon (simplified)
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba(1.0, 0.42, 0.42, 1.0).unwrap());
    paint.anti_alias = true;
    let content_x = cx - content_w / 2.0;
    let icon_x = content_x;
    let mut pb = tiny_skia::PathBuilder::new();
    let s = icon_w;
    pb.move_to(icon_x, cy + s * 0.45);
    pb.line_to(icon_x + s, cy + s * 0.45);
    pb.line_to(icon_x + s / 2.0, cy - s * 0.55);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Default::default(),
            None,
        );
    }

    // Error text
    font.render(
        pixmap,
        message,
        content_x + icon_w + gap,
        cy + font_size * 0.35,
        font_size,
        [255, 255, 255, 230],
    );
}

fn render_onboarding_card(
    pixmap: &mut tiny_skia::Pixmap,
    font: &FontRenderer,
    step: &OnboardingStep,
    hotkey_label: &str,
    cx: f32,
    cy: f32,
) {
    let text = onboarding_card_text(step, hotkey_label);
    let font_size = 14.0;
    let text_w = font.measure(&text, font_size);
    let pad_h = 16.0;
    let pad_v = 10.0;
    let card_w = text_w + pad_h * 2.0;
    let card_h = font_size + pad_v * 2.0;
    let card_r = card_h / 2.0;

    let card_x = cx - card_w / 2.0;
    let card_y = cy - card_h / 2.0;

    // Card background
    let card_path = rounded_rect(card_x, card_y, card_w, card_h, card_r);
    let mut bg_paint = tiny_skia::Paint::default();
    bg_paint.set_color(tiny_skia::Color::from_rgba(0.06, 0.06, 0.1, 0.75).unwrap());
    bg_paint.anti_alias = true;
    pixmap.fill_path(
        &card_path,
        &bg_paint,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );

    // Card border
    let mut border_paint = tiny_skia::Paint::default();
    border_paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.3).unwrap());
    border_paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(&card_path, &border_paint, &stroke, Default::default(), None);

    // Card text
    font.render_centered(
        pixmap,
        &text,
        cx,
        cy + font_size * 0.35,
        font_size,
        [255, 255, 255, 230],
    );
}

fn render_keycap(
    pixmap: &mut tiny_skia::Pixmap,
    font: &Option<FontRenderer>,
    label: &str,
    x: f32,
    cy: f32,
    font_size: f32,
) {
    let fr = match font {
        Some(f) => f,
        None => return,
    };
    let text_w = fr.measure(label, font_size);
    let ui_scale = (font_size / 12.0).max(0.5);
    let pad = 10.0 * ui_scale;
    let kw = text_w + pad * 2.0;
    let kh = font_size + 10.0 * ui_scale;
    let ky = cy - kh / 2.0;

    // Keycap background
    let key_path = rounded_rect(x, ky, kw, kh, 5.0 * ui_scale);
    let mut bg = tiny_skia::Paint::default();
    bg.set_color(tiny_skia::Color::from_rgba(0.25, 0.25, 0.25, 1.0).unwrap());
    bg.anti_alias = true;
    pixmap.fill_path(
        &key_path,
        &bg,
        tiny_skia::FillRule::Winding,
        Default::default(),
        None,
    );

    // Keycap border
    let mut border = tiny_skia::Paint::default();
    border.set_color(tiny_skia::Color::from_rgba(0.45, 0.45, 0.45, 1.0).unwrap());
    border.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(&key_path, &border, &stroke, Default::default(), None);

    // Keycap text
    fr.render_centered(
        pixmap,
        label,
        x + kw / 2.0,
        cy + font_size * 0.35,
        font_size,
        [255, 255, 255, 255],
    );
}

fn onboarding_card_text(step: &OnboardingStep, hotkey_label: &str) -> String {
    match step {
        OnboardingStep::TryIt => format!("Hold {} to start recording", hotkey_label),
        OnboardingStep::DoubleTapTip => {
            format!("Double-tap {} for hands-free recording", hotkey_label)
        }
        OnboardingStep::ClickTip => "Click the pill for hands-free recording".to_string(),
        OnboardingStep::ApiTip => {
            "Add an API key in the menu bar for better transcription".to_string()
        }
        OnboardingStep::FormattingTip => {
            "Enable formatting in Settings to clean up grammar and punctuation automatically"
                .to_string()
        }
        OnboardingStep::Welcome => "You're all set \u{2014} enjoy!".to_string(),
    }
}

fn format_elapsed(seconds: f64) -> String {
    let s = seconds as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = 0.5522847498 * r; // kappa for cubic bezier circle approximation
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

// ---------------------------------------------------------------------------
// Blit to layered window
// ---------------------------------------------------------------------------

fn blit_to_layered_window(hwnd: HWND, pixmap: &tiny_skia::Pixmap, w: u32, h: u32) {
    unsafe {
        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(hdc_screen);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let old = SelectObject(hdc_mem, hbm);

        // Convert RGBA premultiplied → BGRA premultiplied
        let src = pixmap.data();
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
        for i in (0..dst.len()).step_by(4) {
            dst[i] = src[i + 2]; // B
            dst[i + 1] = src[i + 1]; // G
            dst[i + 2] = src[i]; // R
            dst[i + 3] = src[i + 3]; // A
        }

        let pt_src = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: w as i32,
            cy: h as i32,
        };
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let pt_dst = POINT {
            x: rect.left,
            y: rect.top,
        };

        let blend = BLENDFUNCTION {
            BlendOp: 0,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1,
        };

        let _ = UpdateLayeredWindow(
            hwnd,
            hdc_screen,
            Some(&pt_dst),
            Some(&size),
            hdc_mem,
            Some(&pt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
    }
}
