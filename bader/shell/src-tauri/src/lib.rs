// Bader for Windows — app wiring and the commands the island calls.

mod accounts;
mod buddy;
mod claude;
mod engine;
mod face;
mod files;
mod hooks;
mod hotkey;
mod integrations;
mod island;
mod log;
#[cfg(windows)]
mod pipe;
mod quick;
mod runs;
mod sync;
#[cfg(not(windows))]
#[path = "pipe_stub.rs"]
mod pipe;
mod secrets;
mod settings;
mod tray;
mod voice;
#[cfg(windows)]
mod win_user;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(not(windows))]
use no_window::CommandExt;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{ManagerExt, MacosLauncher};

use claude::{Chat, ChatContext, ChatReply};
use files::DroppedFile;
use hooks::{HookPreview, HookStatus};
use island::{PollGate, ScreenInfo};
use pipe::Pending;
use settings::Settings;

/// Keeps spawned helpers from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub gate: Arc<PollGate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootInfo {
    settings: Settings,
    screen: ScreenInfo,
    version: String,
    hook_path: String,
}

#[tauri::command]
fn boot(app: AppHandle, shared: State<Shared>) -> BootInfo {
    let mut settings = shared.settings.lock().unwrap().clone();
    // The real state of ~/.claude/settings.json wins over whatever we stored.
    settings.hooks_installed = hooks::status().installed;
    let screen = island::screen_info(&app, &settings.screen);
    BootInfo {
        settings,
        screen,
        version: env!("CARGO_PKG_VERSION").to_string(),
        hook_path: settings::hook_exe_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, shared: State<Shared>, settings: Settings) {
    let (screen_changed, autostart_changed) = {
        let mut current = shared.settings.lock().unwrap();
        let screen_changed = current.screen != settings.screen;
        let autostart_changed = current.autostart != settings.autostart;
        *current = settings.clone();
        (screen_changed, autostart_changed)
    };
    if let Err(err) = settings::save(&settings) {
        eprintln!("[bader] could not save settings: {err}");
    }
    if autostart_changed {
        let manager = app.autolaunch();
        let result = if settings.autostart { manager.enable() } else { manager.disable() };
        if let Err(err) = result {
            eprintln!("[bader] autostart: {err}");
        }
    }
    if screen_changed {
        let collapsed = shared.gate.collapsed.load(Ordering::Relaxed);
        island::apply_geometry(&app, &settings.screen, collapsed);
    }
    // Keep the other window in step (island ⇄ settings window).
    let _ = app.emit("settings-changed", settings);
}

/// Hidden island → shrink the window to the invisible wake strip and park the
/// cursor poll; anything else → full panel and 60 Hz polling.
#[tauri::command]
fn set_collapsed(app: AppHandle, shared: State<Shared>, collapsed: bool) {
    let pref = shared.settings.lock().unwrap().screen.clone();
    shared.gate.collapsed.store(collapsed, Ordering::Relaxed);
    island::apply_geometry(&app, &pref, collapsed);
    // The wake strip must always take the mouse, and a resize invalidates the flag.
    island::set_ignore_cursor(&app, false);
    shared.gate.forget_ignore_state();
    shared.gate.set_active(!collapsed);
}

/// The front end pushes the island shape; Rust decides click-through from it.
#[tauri::command]
fn set_island_rect(shared: State<Shared>, x: f64, y: f64, width: f64, height: f64) {
    shared.gate.set_rect(island::IslandRect { x, y, w: width, h: height });
}

#[tauri::command]
fn focus_window(app: AppHandle, focused: bool) {
    let Some(win) = island::window(&app) else { return };
    island::set_activating(&win, focused);
    if focused {
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn reposition(app: AppHandle, shared: State<Shared>) {
    let pref = shared.settings.lock().unwrap().screen.clone();
    let collapsed = shared.gate.collapsed.load(Ordering::Relaxed);
    island::apply_geometry(&app, &pref, collapsed);
}

#[tauri::command]
fn open_url(url: String) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    #[cfg(windows)]
    let _ = Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", &url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    #[cfg(not(windows))]
    let _ = Command::new("open").arg(&url).spawn();
}

/// "Open terminal" opens the working folder in VS Code when `code` is on PATH,
/// and falls back to Explorer otherwise.
#[tauri::command]
fn open_in_vscode(path: Option<String>) -> bool {
    // No `cmd /C` anywhere near this. The path is a project folder chosen by
    // whoever is using Claude Code, and cmd would happily read `&`, `^` and `%`
    // in a folder name as syntax. Finding the launcher ourselves and handing the
    // path over as a separate argument keeps it a path.
    if let Some(code) = find_on_path("code") {
        let mut cmd = Command::new(code);
        if let Some(p) = path.as_deref().filter(|p| !p.is_empty()) {
            cmd.arg(p);
        }
        if cmd.creation_flags(CREATE_NO_WINDOW).spawn().is_ok() {
            return true;
        }
    }
    if let Some(p) = path.as_deref().filter(|p| !p.is_empty()) {
        #[cfg(windows)]
        let _ = Command::new("explorer").arg(p).spawn();
        #[cfg(not(windows))]
        let _ = Command::new("open").arg(p).spawn();
    }
    false
}

/// Our own `where`: walks %PATH% against %PATHEXT%, no shell involved.
/// Rust quotes arguments correctly for `.cmd`/`.bat` targets since 1.77, so
/// spawning `code.cmd` directly is safe.
fn find_on_path(stem: &str) -> Option<std::path::PathBuf> {
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let dirs = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&dirs) {
        for ext in exts.split(';').filter(|e| !e.is_empty()) {
            let candidate = dir.join(format!("{stem}{}", ext.to_lowercase()));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Tray → Pause. Paused means paused: the pollers stop talking to the network,
/// not just the island stopping showing things.
#[tauri::command]
fn set_paused(paused: bool) {
    integrations::set_paused(paused);
}

// ── Claude Code hooks ─────────────────────────────────────────────────────────

#[tauri::command]
fn hooks_status() -> HookStatus {
    hooks::status()
}

/// Returns the diff the user has to look at before anything is written.
#[tauri::command]
fn hooks_preview(install: bool) -> Result<HookPreview, String> {
    hooks::preview(install)
}

/// Only ever called from an explicit click in the settings window.
#[tauri::command]
fn hooks_apply(
    app: AppHandle,
    shared: State<Shared>,
    install: bool,
    fingerprint: String,
) -> Result<String, String> {
    // The fingerprint comes from the preview the user actually looked at, so a
    // settings.json that changed in between is refused rather than overwritten.
    let backup = hooks::write(install, &fingerprint)?;
    let updated = {
        let mut current = shared.settings.lock().unwrap();
        current.hooks_installed = install;
        let _ = settings::save(&current);
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    Ok(backup)
}

#[tauri::command]
fn approval_decision(app: AppHandle, request_id: String, decision: String) {
    pipe::answer(&app, &request_id, &decision);
}

/// The island has the card on screen, so the long wait for a human may begin.
/// Until this arrives the relay only waits a few hundred milliseconds, which is
/// what stops a paused or unresponsive island from freezing Claude Code.
#[tauri::command]
fn approval_ack(app: AppHandle, request_id: String) {
    pipe::acknowledge(&app, &request_id);
}

/// Nobody can act on this request — the island is paused, or another card is
/// already up. Claude Code falls back to asking in the terminal immediately.
#[tauri::command]
fn approval_decline(app: AppHandle, request_id: String) {
    pipe::decline(&app, &request_id);
}

// ── Chat, files and secrets ───────────────────────────────────────────────────

/// One chat turn. The API key and any file bytes stay on the Rust side.
#[tauri::command]
async fn chat_send(
    app: AppHandle,
    shared: State<'_, Shared>,
    chat: State<'_, Chat>,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let model = shared.settings.lock().unwrap().model.clone();
    claude::send(&app, &chat, &model, query, context).await
}

#[tauri::command]
fn face_set(face: State<face::Face>, name: String, seconds: Option<f32>) {
    face.set(&name, seconds);
}

/// A full-width text strip for the face screen, drawn by the island (RGB565, base64).
#[tauri::command]
fn face_strip(face: State<face::Face>, y: u16, h: u16, idle: bool, data: String) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    face.strip(y, h, idle, bytes)
}

/// A picture of a screen region for the touch face (RGB565, base64).
#[tauri::command]
fn face_img(face: State<face::Face>, x: u16, y: u16, w: u16, h: u16, data: String) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    face.img(x, y, w, h, bytes)
}

/// Which face screen is plugged in (size, touch), if any.
#[tauri::command]
fn face_info() -> Option<face::Info> {
    face::info()
}

#[tauri::command]
fn face_cmd(face: State<face::Face>, line: String) -> Result<(), String> {
    face.command(&line)
}

/// Unread mail and upcoming meetings for the face screen's pages.
#[tauri::command]
fn snapshot_lists() -> sync::Lists {
    sync::lists()
}

/// What Bader is doing → the character on the desktop.
#[tauri::command]
fn buddy_event(app: AppHandle, face: String, text: Option<String>) {
    buddy::event(&app, face, text);
}

#[tauri::command]
fn buddy_show(app: AppHandle) {
    buddy::show(&app);
}

#[tauri::command]
fn buddy_hide(app: AppHandle) {
    buddy::hide(&app);
}

/// A click on the character: open Bader's window.
#[tauri::command]
fn buddy_click(app: AppHandle) {
    let _ = app.emit_to(island::WINDOW_LABEL, "buddy-click", ());
}

/// Right-click on the character: the quick-actions menu.
#[tauri::command]
fn buddy_menu(app: AppHandle) {
    buddy::popup_menu(&app);
}

#[tauri::command]
fn face_led(face: State<face::Face>, r: u8, g: u8, b: u8, pulse: bool) {
    face.led(r, g, b, pulse);
}

/// Approve ("once") or deny ("deny") what Bader is waiting on.
#[tauri::command]
async fn run_approve(app: AppHandle, choice: String) -> Result<bool, String> {
    let key = secrets::get("bader-engine-key").unwrap_or_default();
    runs::answer(&app, &claude::engine_url(), &key, &choice).await
}

#[tauri::command]
async fn sync_now(app: AppHandle) -> Option<sync::SnapshotInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        sync::run_once(&app);
        sync::info()
    })
    .await
    .ok()
    .flatten()
}

/// System notification ("answer ready") — shown when the island was closed.
#[tauri::command]
fn notify(app: tauri::AppHandle, title: String, body: String) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

#[tauri::command]
fn snapshot_info() -> Option<sync::SnapshotInfo> {
    sync::info()
}

#[tauri::command]
fn voice_start(rec: State<voice::Recorder>) -> Result<(), String> {
    voice::start(&rec)
}

/// Plays a spoken reply outside the web view (used when the web view won't).
#[tauri::command]
async fn audio_play(data: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || voice::play(&data)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn audio_stop() {
    voice::stop_playing();
}

#[tauri::command]
fn voice_cancel(rec: State<voice::Recorder>) {
    voice::cancel(&rec);
}

#[tauri::command]
async fn voice_stop(app: AppHandle) -> Result<voice::Heard, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let rec = app.state::<voice::Recorder>();
        voice::stop_and_transcribe(&app, &rec)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Accounts (Gmail, Outlook) ────────────────────────────────────────────────

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn accounts_status() -> Result<accounts::AccountsStatus, String> {
    blocking(|| Ok(accounts::status())).await
}

#[tauri::command]
async fn gmail_find_client_file() -> Result<Option<String>, String> {
    blocking(|| Ok(accounts::gmail_find_client_file())).await
}

#[tauri::command]
async fn gmail_set_client(path: String) -> Result<(), String> {
    blocking(move || accounts::gmail_set_client(&path)).await
}

#[tauri::command]
async fn gmail_auth_url() -> Result<String, String> {
    blocking(accounts::gmail_auth_url).await
}

#[tauri::command]
async fn gmail_auth_code(code: String) -> Result<(), String> {
    blocking(move || accounts::gmail_auth_code(&code)).await
}

#[tauri::command]
async fn gmail_disconnect() -> Result<(), String> {
    blocking(accounts::gmail_disconnect).await
}

#[tauri::command]
async fn outlook_start(client_id: String, tenant: String) -> Result<accounts::DeviceCode, String> {
    accounts::outlook_start(&client_id, &tenant).await
}

#[tauri::command]
async fn outlook_wait(device_code: String, interval: u64, expires_in: u64) -> Result<String, String> {
    accounts::outlook_wait(&device_code, interval, expires_in).await
}

#[tauri::command]
fn outlook_disconnect() -> Result<(), String> {
    accounts::outlook_disconnect()
}

#[tauri::command]
async fn capture_screen() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(voice::capture_screen)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_speak(app: AppHandle, text: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || voice::speak(&app, &text))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn engine_status() -> engine::EngineStatus {
    engine::status().await
}

#[tauri::command]
async fn engine_apply(
    app: AppHandle,
    values: std::collections::HashMap<String, String>,
    secrets: std::collections::HashMap<String, String>,
    restart: bool,
) -> Result<(), String> {
    let done = tauri::async_runtime::spawn_blocking(move || {
        engine::apply(values, secrets)?;
        voice::restart_worker();
        if restart {
            engine::restart()?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;
    buddy::sync(&app); // "Show Bader on the desktop" may have changed
    done
}

#[tauri::command]
fn chat_reset(chat: State<Chat>) {
    chat.reset();
}

/// Copies a dropped file into the inbox and reports its name back.
#[tauri::command]
fn ingest_file(path: String) -> Result<DroppedFile, String> {
    files::ingest(&path)
}

/// The island may only ask whether a key exists — never read it.
#[tauri::command]
fn secret_present(key: String) -> bool {
    secrets::present(&key)
}

#[tauri::command]
fn secret_set(key: String, value: String) -> Result<(), String> {
    secrets::set(&key, &value)
}

#[tauri::command]
fn secret_clear(key: String) -> Result<(), String> {
    secrets::clear(&key)
}

/// Opens the configured n8n instance — the URL lives in the Credential Manager.
#[tauri::command]
fn open_n8n() {
    if let Some(url) = secrets::get("n8n-url") {
        open_url(url);
    }
}

/// Refresh buttons in the integration cards.
#[tauri::command]
async fn refresh_integration(app: AppHandle, id: String) {
    integrations::poll_once(app, &id).await;
}

/// Lets the island write to the same log as the Rust side.
#[tauri::command]
fn log_line(message: String) {
    log::line(format!("ui  {message}"));
}

// ── Settings window ───────────────────────────────────────────────────────────

/// WebView2 allows exactly one browser environment per app, and its options are
/// fixed by whichever webview is created first. Every window must therefore ask
/// for the *same* arguments as the island (see `additionalBrowserArgs` in
/// tauri.conf.json) — a mismatch makes the second window come up blank, with no
/// error anywhere.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// In a dev build the pages are served by Vite, so the second window needs the
/// absolute dev URL; a bundled build resolves it inside the app bundle.
fn settings_page_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(dev)]
    if let Some(mut base) = app.config().build.dev_url.clone() {
        base.set_path("/settings.html");
        return WebviewUrl::External(base);
    }
    let _ = app;
    WebviewUrl::App("settings.html".into())
}

/// The settings window is created hidden at launch and only ever shown and
/// hidden afterwards. A WebView2 window created later — on the main thread or
/// not — silently comes up blank in this app, so the window that works is the
/// one that exists before the island's webview does.
fn create_settings_window(app: &AppHandle) {
    let url = settings_page_url(app);
    match WebviewWindowBuilder::new(app, "settings", url)
        .additional_browser_args(BROWSER_ARGS)
        .title("Settings — Bader")
        .inner_size(560.0, 680.0)
        .min_inner_size(460.0, 480.0)
        .resizable(true)
        .visible(false)
        .center()
        .build()
    {
        Ok(win) => {
            // Closing it must only hide it, or it could never be reopened.
            let hidden = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hidden.hide();
                }
            });
        }
        Err(err) => log::line(format!("settings window failed: {err}")),
    }
}

pub fn show_settings_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("settings") else {
        log::line("settings window missing");
        return;
    };
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

#[tauri::command]
fn open_settings_window(app: AppHandle) {
    show_settings_window(&app);
}

pub fn run() {
    let loaded = settings::load();
    let gate = Arc::new(PollGate::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = app.emit_to(island::WINDOW_LABEL, "tray", "open".to_string());
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(Shared {
            settings: Mutex::new(loaded.clone()),
            gate: gate.clone(),
        })
        .manage(Pending::default())
        .manage(Chat::default())
        .manage(voice::Recorder::default())
        .invoke_handler(tauri::generate_handler![
            boot,
            save_settings,
            set_collapsed,
            set_island_rect,
            focus_window,
            reposition,
            open_url,
            open_in_vscode,
            quit_app,
            hooks_status,
            hooks_preview,
            hooks_apply,
            approval_decision,
            approval_ack,
            approval_decline,
            log_line,
            chat_send,
            chat_reset,
            engine_status,
            voice_start,
            face_set,
            face_strip,
            face_led,
            buddy_event,
            buddy_show,
            buddy_hide,
            buddy_click,
            buddy_menu,
            face_img,
            face_info,
            face_cmd,
            snapshot_lists,
            run_approve,
            sync_now,
            snapshot_info,
            notify,
            voice_stop,
            voice_cancel,
            audio_play,
            audio_stop,
            voice_speak,
            capture_screen,
            accounts_status,
            gmail_find_client_file,
            gmail_set_client,
            gmail_auth_url,
            gmail_auth_code,
            gmail_disconnect,
            outlook_start,
            outlook_wait,
            outlook_disconnect,
            engine_apply,
            ingest_file,
            secret_present,
            secret_set,
            secret_clear,
            refresh_integration,
            open_n8n,
            open_settings_window,
            set_paused,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            // macOS: live in the menu bar only, no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::build(&handle)?;
            app.manage(face::Face::start(handle.clone()));
            sync::start(handle.clone());
            voice::prewarm(handle.clone());
            hotkey::start(handle.clone());
            // Before the island: see create_settings_window.
            create_settings_window(&handle);
            buddy::create(&handle, BROWSER_ARGS);
            app.on_menu_event(|app, event| {
                buddy::on_menu(app, event.id.as_ref());
            });
            // First run: the set-up wizard (languages, AI key, voice) opens by itself.
            if engine::needs_setup() {
                let h2 = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    show_settings_window(&h2);
                });
            }

            if let Some(win) = island::window(&handle) {
                island::make_non_activating(&win);
                // macOS: show on every Space, including full-screen apps.
                #[cfg(target_os = "macos")]
                island::float_everywhere(&win);
                island::apply_geometry(&handle, &loaded.screen, false);
                let _ = win.show();
            }
            gate.collapsed.store(false, Ordering::Relaxed);
            gate.set_active(true);
            island::spawn_cursor_poll(handle.clone(), gate.clone());

            log::line(format!("--- Bader {} started ---", env!("CARGO_PKG_VERSION")));
            hooks::ensure_hook_exe(&handle);
            pipe::start(handle.clone());
            integrations::start(handle.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bader");
}

/// `creation_flags` is Windows-only; elsewhere it is a no-op so the same
/// spawn code compiles on macOS.
#[cfg(not(windows))]
mod no_window {
    pub trait CommandExt {
        fn creation_flags(&mut self, _flags: u32) -> &mut Self;
    }
    impl CommandExt for std::process::Command {
        fn creation_flags(&mut self, _flags: u32) -> &mut Self {
            self
        }
    }
}
