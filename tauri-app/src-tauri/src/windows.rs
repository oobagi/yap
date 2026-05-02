use tauri::{AppHandle, Manager, WebviewWindow, Window, WindowEvent};

const APP_WINDOW_LABELS: &[&str] = &["settings", "history"];

pub fn show_app_window(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("window not found: {label}"))?;

    activate_app(app);
    window.unminimize().map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;

    Ok(())
}

pub fn hide_app_window(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("window not found: {label}"))?;

    window.hide().map_err(|e| e.to_string())?;
    hide_app_if_no_windows_visible(app);

    Ok(())
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    if !APP_WINDOW_LABELS.contains(&window.label()) {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = hide_app_window(window.app_handle(), window.label());
    }
}

pub fn hide_app_if_no_windows_visible(app: &AppHandle) {
    if app_windows(app).any(|window| window.is_visible().unwrap_or(false)) {
        return;
    }

    deactivate_app(app);
}

fn app_windows(app: &AppHandle) -> impl Iterator<Item = WebviewWindow> + '_ {
    APP_WINDOW_LABELS
        .iter()
        .filter_map(|label| app.get_webview_window(label))
}

#[cfg(target_os = "macos")]
fn activate_app(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    let _ = app.show();
}

#[cfg(not(target_os = "macos"))]
fn activate_app(_app: &AppHandle) {}

#[cfg(target_os = "macos")]
fn deactivate_app(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
fn deactivate_app(_app: &AppHandle) {}
