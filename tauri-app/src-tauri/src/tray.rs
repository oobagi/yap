use tauri::{
    image::Image,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

use crate::history;

/// Build and attach the system tray icon with its menu.
///
/// Menu structure (from spec):
///   Yap (disabled label)
///   ----
///   Enabled          (check item)
///   History >        (submenu with recent entries)
///   Settings...
///   ----
///   Quit
pub fn setup_tray(app: &AppHandle) -> Result<(), String> {
    // --- Menu items ---

    let title_item = MenuItemBuilder::with_id("title", "Yap")
        .enabled(false)
        .build(app)
        .map_err(|e| format!("failed to create title item: {e}"))?;

    let enabled_item = CheckMenuItemBuilder::with_id("toggle_enabled", "Enabled")
        .checked(true)
        .build(app)
        .map_err(|e| format!("failed to create enabled item: {e}"))?;

    // History submenu
    let history_submenu = build_history_submenu(app)?;

    let settings_item = MenuItemBuilder::with_id("settings", "Settings...")
        .build(app)
        .map_err(|e| format!("failed to create settings item: {e}"))?;

    let quit_item = MenuItemBuilder::with_id("quit", "Quit")
        .build(app)
        .map_err(|e| format!("failed to create quit item: {e}"))?;

    // --- Build menu ---

    let menu = MenuBuilder::new(app)
        .item(&title_item)
        .separator()
        .item(&enabled_item)
        .item(&history_submenu)
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()
        .map_err(|e| format!("failed to create tray menu: {e}"))?;

    // --- Build tray icon ---

    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .map_err(|e| format!("failed to load tray icon: {e}"))?;

    let _tray = TrayIconBuilder::with_id("yap-tray")
        .tooltip("Yap")
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "toggle_enabled" => {
                    let _ = app.emit("tray:toggle-enabled", ());
                }
                "settings" => {
                    if let Some(window) = app.get_webview_window("settings") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "show_history" => {
                    if let Some(window) = app.get_webview_window("history") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit("tray:show-history", ());
                }
                "clear_history" => {
                    let _ = history::clear();
                    refresh_history_menu(app);
                    let _ = app.emit("tray:history-cleared", ());
                }
                "quit" => {
                    app.exit(0);
                }
                other if other.starts_with("history_") => {
                    let entry_id = other.strip_prefix("history_").unwrap_or("");
                    if let Some(entry) = history::get(entry_id) {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(entry.text);
                        }
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|_tray, _event| {
            // Left-click shows menu via show_menu_on_left_click(true)
        })
        .build(app)
        .map_err(|e| format!("failed to build tray icon: {e}"))?;

    Ok(())
}

/// Build the history submenu from current entries.
fn build_history_submenu(
    app: &AppHandle,
) -> Result<tauri::menu::Submenu<tauri::Wry>, String> {
    let entries = history::load();

    let mut builder = SubmenuBuilder::with_id(app, "history", "History");

    if entries.is_empty() {
        let empty_item = MenuItemBuilder::with_id("history_empty", "No entries")
            .enabled(false)
            .build(app)
            .map_err(|e| format!("failed to create empty history item: {e}"))?;
        builder = builder.item(&empty_item);
    } else {
        for entry in entries.iter().take(10) {
            // Truncate text to 60 chars for display
            let display_text = if entry.text.chars().count() > 60 {
                let truncated: String = entry.text.chars().take(57).collect();
                format!("{}...", truncated)
            } else {
                entry.text.clone()
            };
            // Replace newlines with spaces for menu display
            let display_text = display_text.replace('\n', " ").replace('\r', "");

            let item_id = format!("history_{}", entry.id);
            let item = MenuItemBuilder::with_id(item_id, display_text)
                .build(app)
                .map_err(|e| format!("failed to create history item: {e}"))?;
            builder = builder.item(&item);
        }

        // Separator and utility items
        builder = builder.separator();

        let show_all = MenuItemBuilder::with_id("show_history", "Show All...")
            .build(app)
            .map_err(|e| format!("failed to create show all item: {e}"))?;
        builder = builder.item(&show_all);

        let clear = MenuItemBuilder::with_id("clear_history", "Clear History")
            .build(app)
            .map_err(|e| format!("failed to create clear item: {e}"))?;
        builder = builder.item(&clear);
    }

    builder
        .build()
        .map_err(|e| format!("failed to build history submenu: {e}"))
}

/// Update the tray icon to reflect the current app state.
///
/// States:
///   idle       -> default icon
///   recording  -> recording indicator icon
///   processing -> processing indicator icon
pub fn update_icon(app: &AppHandle, state: &str) {
    if let Some(tray) = app.tray_by_id("yap-tray") {
        // Update tooltip to reflect state
        let tooltip = match state {
            "recording" => "Yap - Recording",
            "processing" => "Yap - Processing",
            _ => "Yap",
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

/// Rebuild the history submenu with current entries.
pub fn refresh_history_menu(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("yap-tray") {
        if let Ok(history_submenu) = build_history_submenu(app) {
            // Rebuild the entire menu with updated history
            let title_item = MenuItemBuilder::with_id("title", "Yap")
                .enabled(false)
                .build(app);
            let enabled_item = CheckMenuItemBuilder::with_id("toggle_enabled", "Enabled")
                .checked(true)
                .build(app);
            let settings_item = MenuItemBuilder::with_id("settings", "Settings...")
                .build(app);
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app);

            if let (Ok(title), Ok(enabled), Ok(settings), Ok(quit)) =
                (title_item, enabled_item, settings_item, quit_item)
            {
                if let Ok(menu) = MenuBuilder::new(app)
                    .item(&title)
                    .separator()
                    .item(&enabled)
                    .item(&history_submenu)
                    .item(&settings)
                    .separator()
                    .item(&quit)
                    .build()
                {
                    let _ = tray.set_menu(Some(menu));
                }
            }
        }
    }
}
