// Notification-area icon: Open, Settings, Pause, Quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::island::WINDOW_LABEL;

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Bader", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let buddy = CheckMenuItem::with_id(
        app, "buddy-toggle", "Show Bader on the desktop", true, crate::buddy::enabled(), None::<&str>,
    )?;
    app.manage(crate::buddy::TrayTick(buddy.clone()));

    let toggle_window = MenuItem::with_id(app, "buddy-window", "Show / hide Bader's window", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open, &toggle_window, &buddy, &sep1, &settings, &pause, &sep2, &quit])?;

    let mut builder = TrayIconBuilder::with_id("bader")
        .tooltip("Bader")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app: &AppHandle, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "settings" => crate::show_settings_window(app),
            id if id.starts_with("buddy-") => {} // handled once, app-wide (see buddy::on_menu)
            id => {
                let _ = app.emit_to(WINDOW_LABEL, "tray", id.to_string());
            }
        });

    // Bader's head in the tray / menu bar; falls back to the app icon.
    match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        Ok(icon) => builder = builder.icon(icon),
        Err(_) => {
            if let Some(icon) = app.default_window_icon().cloned() {
                builder = builder.icon(icon);
            }
        }
    }

    builder.build(app)?;
    Ok(())
}
