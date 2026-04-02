//! Native Win32 overlay pill for Windows.
//!
//! Creates a small layered window (WS_EX_LAYERED) rendered with tiny-skia.
//! Per-pixel alpha via UpdateLayeredWindow gives automatic click-through on
//! transparent pixels — no hacking needed.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::orchestrator;

// ---------------------------------------------------------------------------
// Overlay state (shared between render + orchestrator threads)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct OverlayState {
    pub mode: String,         // "idle" | "recording" | "processing"
    pub bars: [f32; 11],
    pub level: f32,
    pub hands_free: bool,
    pub paused: bool,
    pub hovering: bool,
    pub error: Option<String>,
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
        }
    }
}

static STATE: OnceLock<Arc<Mutex<OverlayState>>> = OnceLock::new();
static HWND: OnceLock<HWND> = OnceLock::new();

const WM_YAP_UPDATE: u32 = WM_USER + 1;

// Pill dimensions
const PILL_W: i32 = 200;
const PILL_H: i32 = 56;
const PILL_H_MINIMIZED: i32 = 28;

// ---------------------------------------------------------------------------
// Public API (called from orchestrator thread)
// ---------------------------------------------------------------------------

pub fn update_state(f: impl FnOnce(&mut OverlayState)) {
    if let Some(state) = STATE.get() {
        let mut s = state.lock().unwrap();
        f(&mut s);
    }
    // Trigger re-render on the overlay thread
    if let Some(&hwnd) = HWND.get() {
        unsafe { let _ = PostMessageW(Some(hwnd), WM_YAP_UPDATE, WPARAM(0), LPARAM(0)); }
    }
}

pub fn on_pill_click_callback() {
    // This runs on the overlay's window thread — dispatch to orchestrator
    if let Some(app) = crate::sidecar::get_app_handle() {
        use tauri::Manager;
        let orch: tauri::State<'_, Arc<orchestrator::Orchestrator>> = app.state();
        orch.on_pill_click();
    }
}

/// Spawn the overlay window on a dedicated thread.
pub fn spawn(app: &tauri::AppHandle) {
    let _ = STATE.set(Arc::new(Mutex::new(OverlayState::default())));

    // Store app handle for click callbacks
    crate::sidecar::store_app_handle(app);

    std::thread::Builder::new()
        .name("yap-win-overlay".into())
        .spawn(|| {
            unsafe { run_window_loop() };
        })
        .expect("failed to spawn overlay thread");
}

// ---------------------------------------------------------------------------
// Win32 window
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

    // Position: bottom-center of primary monitor
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let x = (screen_w - PILL_W) / 2;
    let y = screen_h - PILL_H - 30; // 30px from bottom

    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        class_name,
        w!("Yap Overlay"),
        WS_POPUP,
        x, y, PILL_W, PILL_H,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let _ = HWND.set(hwnd);

    // Initial render
    render_pill(hwnd);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

    // Timer for animation (processing shimmer, level decay) — 33ms ≈ 30fps
    let _ = SetTimer(Some(hwnd), 1, 33, None);

    // Message loop
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_YAP_UPDATE => {
            render_pill(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            // Re-render for animations (processing shimmer)
            let should_animate = STATE.get()
                .map(|s| {
                    let s = s.lock().unwrap();
                    s.mode == "processing" || s.mode == "recording"
                })
                .unwrap_or(false);
            if should_animate {
                render_pill(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            // Start tracking mouse leave
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);

            if let Some(state) = STATE.get() {
                let mut s = state.lock().unwrap();
                if !s.hovering {
                    s.hovering = true;
                    drop(s);
                    render_pill(hwnd);
                    // Grow window for hover
                    resize_pill(hwnd, true);
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if let Some(state) = STATE.get() {
                let mut s = state.lock().unwrap();
                s.hovering = false;
                drop(s);
                render_pill(hwnd);
                resize_pill(hwnd, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let mode = STATE.get()
                .map(|s| s.lock().unwrap().mode.clone())
                .unwrap_or_default();
            if mode != "processing" {
                on_pill_click_callback();
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

unsafe fn resize_pill(hwnd: HWND, expanded: bool) {
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let h = if expanded { PILL_H } else { PILL_H_MINIMIZED };
    let x = (screen_w - PILL_W) / 2;
    let y = screen_h - h - 30;
    let _ = SetWindowPos(
        hwnd, None, x, y, PILL_W, h,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}

// ---------------------------------------------------------------------------
// Rendering with tiny-skia
// ---------------------------------------------------------------------------

fn render_pill(hwnd: HWND) {
    let state = match STATE.get() {
        Some(s) => s.lock().unwrap().clone(),
        None => return,
    };

    let is_expanded = state.mode != "idle" || state.hovering;
    let w = PILL_W as u32;
    let h = if is_expanded { PILL_H as u32 } else { PILL_H_MINIMIZED as u32 };

    let mut pixmap = match tiny_skia::Pixmap::new(w, h) {
        Some(p) => p,
        None => return,
    };

    // -- Draw capsule background --
    let radius = h as f32 / 2.0;
    let bg_alpha = if is_expanded { 0.75 } else { 0.4 };
    let bg = tiny_skia::Color::from_rgba(0.06, 0.06, 0.1, bg_alpha).unwrap();

    let capsule = rounded_rect(1.0, 1.0, w as f32 - 2.0, h as f32 - 2.0, radius);
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(bg);
    paint.anti_alias = true;
    pixmap.fill_path(&capsule, &paint, tiny_skia::FillRule::Winding, Default::default(), None);

    // -- Draw border --
    let border_color = tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, if is_expanded { 0.3 } else { 0.35 }).unwrap();
    let mut stroke_paint = tiny_skia::Paint::default();
    stroke_paint.set_color(border_color);
    stroke_paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: if is_expanded { 1.0 } else { 1.5 },
        ..Default::default()
    };
    pixmap.stroke_path(&capsule, &stroke_paint, &stroke, Default::default(), None);

    // -- Draw content based on mode --
    match state.mode.as_str() {
        "recording" | "processing" => {
            draw_bars(&mut pixmap, &state, w, h);
        }
        "idle" if state.hovering => {
            draw_mic_icon(&mut pixmap, w, h);
        }
        _ => {}
    }

    // -- Blit to layered window via UpdateLayeredWindow --
    blit_to_layered_window(hwnd, &pixmap, w, h);
}

fn draw_bars(pixmap: &mut tiny_skia::Pixmap, state: &OverlayState, w: u32, h: u32) {
    let bar_count = 11;
    let bar_w: f32 = 3.0;
    let gap: f32 = 2.0;
    let total_w = bar_count as f32 * bar_w + (bar_count - 1) as f32 * gap;
    let start_x = (w as f32 - total_w) / 2.0;
    let center_y = h as f32 / 2.0;

    let position_scale: [f32; 11] = [0.35, 0.45, 0.6, 0.78, 0.92, 1.0, 0.94, 0.8, 0.63, 0.48, 0.38];
    let min_h: f32 = 5.0;
    let max_h: f32 = 28.0;

    let is_processing = state.mode == "processing";
    let t = if is_processing {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        (now % 1.2) / 1.2
    } else {
        0.0
    };

    for i in 0..bar_count {
        let scale = position_scale[i];
        let bar_level = state.bars.get(i).copied().unwrap_or(0.0);

        let bar_ceiling = min_h + (max_h - min_h) * scale;
        let bar_h = if is_processing {
            // Processing: wave sweep
            let wave_center = -5.0 + t as f32 * 20.0;
            let dist = (i as f32 - wave_center).abs();
            let wave = (-dist * dist / 6.0_f32).exp();
            (min_h + 14.0 * wave).min(max_h)
        } else {
            // Recording: audio-reactive
            let overall: f32 = state.bars.iter().sum::<f32>() / 11.0;
            let blended = overall * 0.7 + bar_level * 0.3;
            let scaled = (blended / 0.75).min(1.0);
            let driven = scaled.powf(0.6);
            min_h.max((min_h + (bar_ceiling - min_h) * driven).min(bar_ceiling))
        };

        let x = start_x + i as f32 * (bar_w + gap);
        let y = center_y - bar_h / 2.0;

        let opacity = if is_processing {
            let wave_center = -5.0 + t as f32 * 20.0;
            let dist = (i as f32 - wave_center).abs();
            let wave = (-dist * dist / 6.0_f32).exp();
            0.35 + 0.6 * wave
        } else {
            0.9
        };

        let bar_path = rounded_rect(x, y, bar_w, bar_h, 1.5);
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, opacity).unwrap());
        paint.anti_alias = true;
        pixmap.fill_path(&bar_path, &paint, tiny_skia::FillRule::Winding, Default::default(), None);
    }
}

fn draw_mic_icon(pixmap: &mut tiny_skia::Pixmap, w: u32, h: u32) {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, 0.9).unwrap());
    paint.anti_alias = true;

    // Mic body (rounded rect)
    let mic = rounded_rect(cx - 4.0, cy - 9.0, 8.0, 13.0, 4.0);
    pixmap.fill_path(&mic, &paint, tiny_skia::FillRule::Winding, Default::default(), None);

    // Mic arc (simplified as a thicker stroke arc approximated by lines)
    let stroke = tiny_skia::Stroke { width: 1.5, ..Default::default() };
    let mut pb = tiny_skia::PathBuilder::new();
    // Approximate arc with bezier
    pb.move_to(cx - 7.0, cy);
    pb.cubic_to(cx - 7.0, cy + 6.0, cx - 4.0, cy + 9.0, cx, cy + 9.0);
    pb.cubic_to(cx + 4.0, cy + 9.0, cx + 7.0, cy + 6.0, cx + 7.0, cy);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Default::default(), None);
    }

    // Mic stem
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx, cy + 9.0);
    pb.line_to(cx, cy + 12.0);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Default::default(), None);
    }
}

/// Create a rounded rectangle path.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = tiny_skia::PathBuilder::new();
    // Top-left to top-right
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w, y, x + w, y, x + w, y + r);
    // Right side
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h, x + w, y + h, x + w - r, y + h);
    // Bottom
    pb.line_to(x + r, y + h);
    pb.cubic_to(x, y + h, x, y + h, x, y + h - r);
    // Left side
    pb.line_to(x, y + r);
    pb.cubic_to(x, y, x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

/// Blit a tiny-skia Pixmap to the Win32 layered window.
fn blit_to_layered_window(hwnd: HWND, pixmap: &tiny_skia::Pixmap, w: u32, h: u32) {
    unsafe {
        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));

        // Create a 32-bit BGRA DIB section
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(Some(hdc_mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .unwrap();
        let old = SelectObject(hdc_mem, hbm);

        // Copy pixel data (tiny-skia is RGBA premultiplied, Win32 wants BGRA premultiplied)
        let src = pixmap.data();
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
        for i in (0..dst.len()).step_by(4) {
            dst[i] = src[i + 2];     // B
            dst[i + 1] = src[i + 1]; // G
            dst[i + 2] = src[i];     // R
            dst[i + 3] = src[i + 3]; // A
        }

        // UpdateLayeredWindow for per-pixel alpha
        let mut pt_src = POINT { x: 0, y: 0 };
        let mut size = SIZE { cx: w as i32, cy: h as i32 };

        // Get current window position
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let mut pt_dst = POINT { x: rect.left, y: rect.top };

        let blend = BLENDFUNCTION {
            BlendOp: 0,       // AC_SRC_OVER
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1,   // AC_SRC_ALPHA
        };

        let _ = UpdateLayeredWindow(
            hwnd,
            Some(hdc_screen),
            Some(&mut pt_dst),
            Some(&mut size),
            Some(hdc_mem),
            Some(&mut pt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        // Cleanup
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);
    }
}
