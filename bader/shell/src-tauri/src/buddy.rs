// Bader on the desktop: a small transparent window in the bottom-right corner
// with the character in it (src/buddy). It never takes the keyboard focus.

use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const LABEL: &str = "buddy";
// 30% smaller than the first version.
const W: f64 = 170.0;
const H: f64 = 230.0;
/// Room for the Dock / taskbar under the character.
const BOTTOM: f64 = 84.0;
const SIDE: f64 = 18.0;

#[derive(Serialize, Clone)]
struct BuddyEvent {
    face: String,
    text: Option<String>,
}

fn page_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(dev)]
    if let Some(mut base) = app.config().build.dev_url.clone() {
        base.set_path("/buddy.html");
        return WebviewUrl::External(base);
    }
    let _ = app;
    WebviewUrl::App("buddy.html".into())
}

/// Created hidden at launch, like the settings window (see create_settings_window).
pub fn create(app: &AppHandle, browser_args: &str) {
    match WebviewWindowBuilder::new(app, LABEL, page_url(app))
        .additional_browser_args(browser_args)
        .title("Bader")
        .inner_size(W, H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .build()
    {
        Ok(win) => {
            #[cfg(target_os = "macos")]
            crate::island::float_everywhere(&win);
            crate::island::make_non_activating(&win);
        }
        Err(err) => crate::log::line(format!("buddy window failed: {err}")),
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

fn place(win: &WebviewWindow) {
    let Ok(Some(m)) = win.primary_monitor() else { return };
    let scale = m.scale_factor();
    let (mp, ms) = (*m.position(), *m.size());
    let x = mp.x + ms.width as i32 - ((W + SIDE) * scale) as i32;
    let y = mp.y + ms.height as i32 - ((H + BOTTOM) * scale) as i32;
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

pub fn show(app: &AppHandle) {
    let Some(win) = window(app) else { return };
    place(&win);
    #[cfg(target_os = "macos")]
    {
        // show() would make the app active; this only brings the window forward.
        let w = win.clone();
        let _ = win.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            if let Ok(ptr) = w.ns_window() {
                if !ptr.is_null() {
                    let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                    ns.orderFrontRegardless();
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.show();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.hide();
    }
}

/// What Bader is doing now (and an optional line to say) → the character.
pub fn event(app: &AppHandle, face: String, text: Option<String>) {
    let text = text.map(|t| t.chars().take(110).collect::<String>()).filter(|t| !t.trim().is_empty());
    let _ = app.emit_to(LABEL, "buddy", BuddyEvent { face, text });
}

// ── Show / hide, right-click menu ────────────────────────────────────────────

/// The "Show Bader on the desktop" tick in the tray menu, kept in step with the setting.
pub struct TrayTick(pub CheckMenuItem<tauri::Wry>);

pub fn enabled() -> bool {
    crate::engine::pref_get("buddy").and_then(|v| v.as_str().map(|s| s != "off")).unwrap_or(true)
}

/// Shows or hides the character for good (the setting), from any menu.
pub fn set_enabled(app: &AppHandle, on: bool) {
    crate::engine::pref_set("buddy", serde_json::Value::String(if on { "always" } else { "off" }.into()));
    if let Some(tick) = app.try_state::<TrayTick>() {
        let _ = tick.0.set_checked(on);
    }
    let _ = app.emit_to(LABEL, "buddy-mode", ());
}

/// Right-click on the character: quick actions.
pub fn popup_menu(app: &AppHandle) {
    let Some(win) = window(app) else { return };
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let build = || -> tauri::Result<Menu<tauri::Wry>> {
        Menu::with_items(
            app,
            &[
                &item("buddy-talk", "Talk  ·  تكلّم")?,
                &item("buddy-brief", "Brief  ·  موجز")?,
                &item("buddy-mail", "Mail  ·  البريد")?,
                &item("buddy-meetings", "Meetings  ·  اجتماعاتي")?,
                &PredefinedMenuItem::separator(app)?,
                &item("buddy-window", "Show / hide Bader's window")?,
                &item("buddy-hide", "Hide Bader from the desktop")?,
                &PredefinedMenuItem::separator(app)?,
                &item("buddy-quit", "Quit Bader")?,
            ],
        )
    };
    match build() {
        Ok(menu) => {
            let _ = win.popup_menu(&menu);
        }
        Err(err) => crate::log::line(format!("buddy menu failed: {err}")),
    }
}

/// Menu clicks (the right-click menu and the tray tick). True when it was ours.
pub fn on_menu(app: &AppHandle, id: &str) -> bool {
    match id {
        "buddy-quit" => app.exit(0),
        "buddy-hide" => set_enabled(app, false),
        "buddy-toggle" => set_enabled(app, !enabled()),
        "buddy-talk" | "buddy-brief" | "buddy-mail" | "buddy-meetings" | "buddy-window" => {
            let _ = app.emit_to(crate::island::WINDOW_LABEL, "buddy-action", id.trim_start_matches("buddy-").to_string());
        }
        _ => return false,
    }
    true
}

/// The setting was changed elsewhere (Settings window): tray tick and character follow.
pub fn sync(app: &AppHandle) {
    if let Some(tick) = app.try_state::<TrayTick>() {
        let _ = tick.0.set_checked(enabled());
    }
    let _ = app.emit_to(LABEL, "buddy-mode", ());
}
