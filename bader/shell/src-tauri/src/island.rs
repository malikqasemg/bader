// Island window: placement on the chosen display, the two window sizes
// (full panel / invisible wake strip), click-through and the cursor poll.
//
// There is no notch on a PC, so the island is a black shape drawn at the top
// centre of the main display inside a borderless, transparent, always-on-top
// window that never takes focus.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(windows)]
use windows::core::BOOL;
#[cfg(windows)]
use windows::Win32::Foundation::LPARAM;
#[cfg(windows)]
use windows::Win32::System::Ole::RevokeDragDrop;
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetClassNameW};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

/// Logical size of the full window — the largest island view, like the macOS panel.
pub const PANEL_W: f64 = 720.0;
pub const PANEL_H: f64 = 320.0;
/// Logical size of the invisible strip that wakes the island when it is hidden.
pub const STRIP_W: f64 = 240.0;
pub const STRIP_H: f64 = 6.0;

pub const WINDOW_LABEL: &str = "island";

/// Margin around the island that still counts as "on the island", in logical px.
/// Wider than the macOS 6 pt because a click must never be swallowed.
const HIT_MARGIN: f64 = 14.0;

#[derive(Serialize, Clone)]
pub struct CursorPayload {
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Clone)]
pub struct ScreenInfo {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// The island shape in window-logical coordinates, pushed by the front end.
/// The poll thread owns the click-through decision so it lands in the same 16 ms
/// tick as the cursor read — an IPC round trip here loses clicks.
#[derive(Clone, Copy, Default)]
pub struct IslandRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Wakes / parks the cursor poll thread so a hidden island costs literally nothing.
pub struct PollGate {
    active: Mutex<bool>,
    cv: Condvar,
    pub collapsed: AtomicBool,
    pub rect: Mutex<IslandRect>,
    /// Mirrors the window flag so we only call into Win32 when it changes.
    ignoring: AtomicBool,
}

impl PollGate {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(false),
            cv: Condvar::new(),
            collapsed: AtomicBool::new(true),
            rect: Mutex::new(IslandRect::default()),
            ignoring: AtomicBool::new(false),
        }
    }

    pub fn set_rect(&self, rect: IslandRect) {
        *self.rect.lock().unwrap() = rect;
    }

    /// Forces the next poll tick to re-apply the flag (after a window resize).
    pub fn forget_ignore_state(&self) {
        self.ignoring.store(false, Ordering::Relaxed);
    }

    pub fn set_active(&self, on: bool) {
        let mut guard = self.active.lock().unwrap();
        *guard = on;
        self.cv.notify_all();
    }

    fn wait_until_active(&self) {
        let mut guard = self.active.lock().unwrap();
        while !*guard {
            guard = self.cv.wait(guard).unwrap();
        }
    }

    fn is_active(&self) -> bool {
        *self.active.lock().unwrap()
    }
}

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(WINDOW_LABEL)
}

#[cfg(windows)]
fn cursor_physical(_app: &AppHandle) -> Option<(f64, f64)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x as f64, p.y as f64))
}

#[cfg(not(windows))]
fn cursor_physical(app: &AppHandle) -> Option<(f64, f64)> {
    let p = app.cursor_position().ok()?;
    Some((p.x, p.y))
}

/// Lets dropped files reach the app again.
///
/// wry installs its drop target by walking the webview's child windows **once**,
/// when the webview is created. WebView2 creates `Chrome_RenderWidgetHostHWND`
/// later and registers its own target on it; being the innermost window, that one
/// wins, and since the page has no HTML5 drop handler it refuses everything — the
/// "no drop" cursor, with nothing reaching Tauri. Revoking it makes OLE fall
/// through to the target wry registered on the parent widget, which is the one
/// that feeds Tauri's drag events.
///
/// Cheap and idempotent, so it is simply re-run whenever a drag might be starting.
#[cfg(not(windows))]
pub fn unblock_webview_drops(_app: &AppHandle) {}

#[cfg(windows)]
pub fn unblock_webview_drops(app: &AppHandle) {
    for label in [WINDOW_LABEL, "settings"] {
        let Some(win) = app.get_webview_window(label) else { continue };
        let Some(hwnd) = hwnd_of(&win) else { continue };
        unsafe {
            let _ = EnumChildWindows(Some(hwnd), Some(revoke_render_widget), LPARAM(0));
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn revoke_render_widget(hwnd: HWND, _: LPARAM) -> BOOL {
    let mut name = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut name) };
    if len > 0 {
        let class = String::from_utf16_lossy(&name[..len as usize]);
        if class == "Chrome_RenderWidgetHostHWND" {
            let _ = unsafe { RevokeDragDrop(hwnd) };
        }
    }
    true.into()
}

/// True while the left mouse button is held — the only signal we get that a
/// drag might be in flight before it reaches the window.
#[cfg(windows)]
fn left_button_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}

/// macOS delivers drags to the window whatever its click-through state, so the
/// press-to-accept trick is not needed there.
#[cfg(not(windows))]
fn left_button_down() -> bool {
    false
}

fn monitor_contains(m: &Monitor, x: f64, y: f64) -> bool {
    let p = m.position();
    let s = m.size();
    x >= p.x as f64
        && x < (p.x + s.width as i32) as f64
        && y >= p.y as f64
        && y < (p.y + s.height as i32) as f64
}

/// The display the island lives on: the primary one, or the one under the cursor.
fn target_monitor(app: &AppHandle, pref: &str) -> Option<Monitor> {
    let monitors = app.available_monitors().ok()?;
    if pref == "cursor" {
        if let Some((cx, cy)) = cursor_physical(app) {
            if let Some(m) = monitors.iter().find(|m| monitor_contains(m, cx, cy)) {
                return Some(m.clone());
            }
        }
    }
    app.primary_monitor()
        .ok()
        .flatten()
        .or_else(|| monitors.into_iter().next())
}

pub fn screen_info(app: &AppHandle, pref: &str) -> ScreenInfo {
    match target_monitor(app, pref) {
        Some(m) => {
            let scale = m.scale_factor();
            let p = m.position();
            let s = m.size();
            ScreenInfo {
                x: p.x as f64 / scale,
                y: p.y as f64 / scale,
                width: s.width as f64 / scale,
                height: s.height as f64 / scale,
                scale,
            }
        }
        None => ScreenInfo { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0, scale: 1.0 },
    }
}

/// Places and sizes the window. `collapsed` picks the wake strip instead of the panel.
pub fn apply_geometry(app: &AppHandle, pref: &str, collapsed: bool) {
    let Some(win) = window(app) else { return };
    let Some(m) = target_monitor(app, pref) else { return };

    let scale = m.scale_factor();
    let mp = *m.position();
    let ms = *m.size();

    // macOS: the open window starts at the very top of the screen and is taller by
    // the menu bar; the closed wake strip sits just under the menu bar.
    let inset = menu_inset(&m);
    let (lw, lh) = if collapsed { (STRIP_W, STRIP_H) } else { (PANEL_W, PANEL_H + inset) };
    let drop = if collapsed { inset } else { 0.0 };
    let pw = (lw * scale).round().max(1.0) as u32;
    let ph = (lh * scale).round().max(1.0) as u32;
    let x = mp.x + (ms.width as i32 - pw as i32) / 2;
    let y = mp.y;

    let _ = win.set_size(PhysicalSize::new(pw, ph));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    // Moving across displays can rescale the window: re-assert the physical size.
    let _ = win.set_size(PhysicalSize::new(pw, ph));
    let _ = win.set_always_on_top(true);
    pin_to_top(&win, lw, lh, drop);
}

/// Height of the menu bar on this monitor, in logical pixels (macOS; 0 elsewhere).
#[cfg(target_os = "macos")]
pub fn menu_inset(m: &tauri::Monitor) -> f64 {
    ((m.work_area().position.y - m.position().y) as f64 / m.scale_factor()).clamp(0.0, 80.0)
}

#[cfg(not(target_os = "macos"))]
pub fn menu_inset(_m: &tauri::Monitor) -> f64 {
    0.0
}

/// The same, for the monitor the island lives on.
pub fn top_inset(app: &AppHandle, pref: &str) -> f64 {
    target_monitor(app, pref).map(|m| menu_inset(&m)).unwrap_or(0.0)
}

/// AppKit pushes any window that reaches over the menu bar back under it
/// (`constrainFrameRect:toScreen:`). The island sits at the very top edge, so
/// its window class is told to leave frames alone. Done once.
#[cfg(target_os = "macos")]
fn allow_over_menu_bar(ns: &objc2_app_kit::NSWindow) {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2_foundation::NSRect;
    use std::sync::Once;

    unsafe extern "C-unwind" fn keep(_this: *mut AnyObject, _cmd: Sel, rect: NSRect, _screen: *mut AnyObject) -> NSRect {
        rect
    }
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let obj: &AnyObject = &*(ns as *const objc2_app_kit::NSWindow as *const AnyObject);
        let cls: *const AnyClass = obj.class();
        let imp: Imp = std::mem::transmute(
            keep as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, NSRect, *mut AnyObject) -> NSRect,
        );
        let types = c"{CGRect={CGPoint=dd}{CGSize=dd}}@:{CGRect={CGPoint=dd}{CGSize=dd}}@";
        objc2::ffi::class_replaceMethod(cls as *mut AnyClass, objc2::sel!(constrainFrameRect:toScreen:), imp, types.as_ptr());
    });
}

/// macOS keeps ordinary windows under the menu bar; the island belongs at the
/// very top edge of the screen (where a notch would be), over the menu bar.
/// The window toolkit applies its own size / position a moment later, so the
/// frame is set natively now and again shortly after.
#[cfg(target_os = "macos")]
fn pin_to_top(win: &WebviewWindow, w: f64, h: f64, drop: f64) {
    fn pin(win: &WebviewWindow, w: f64, h: f64, drop: f64) {
        let target = win.clone();
        let _ = win.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            let Ok(ptr) = target.ns_window() else { return };
            if ptr.is_null() {
                return;
            }
            let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            allow_over_menu_bar(ns);
            let Some(screen) = ns.screen() else { return };
            let sf = screen.frame();
            let mut f = ns.frame();
            f.size.width = w;
            f.size.height = h;
            // Centred, top edge on the top edge of the screen (AppKit's y grows upwards).
            f.origin.x = sf.origin.x + ((sf.size.width - w) / 2.0).round();
            f.origin.y = sf.origin.y + sf.size.height - h - drop;
            ns.setFrame_display(f, true);
            let got = ns.frame();
            if (got.origin.y - f.origin.y).abs() > 0.5 {
                crate::log::line(format!(
                    "island pin: wanted y {} h {}, got y {} h {} (screen h {})",
                    f.origin.y, f.size.height, got.origin.y, got.size.height, sf.size.height
                ));
            }
        });
    }
    // Only the newest placement may re-apply itself (open → close in quick succession).
    static GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mine = GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    pin(win, w, h, drop);
    let later = win.clone();
    tauri::async_runtime::spawn(async move {
        for ms in [40u64, 160, 400] {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            if GEN.load(std::sync::atomic::Ordering::SeqCst) != mine {
                return;
            }
            pin(&later, w, h, drop);
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn pin_to_top(_win: &WebviewWindow, _w: f64, _h: f64, _drop: f64) {}

#[cfg(windows)]
fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    let raw = win.hwnd().ok()?.0 as isize;
    if raw == 0 {
        return None;
    }
    Some(HWND(raw as *mut _))
}

/// WS_EX_NOACTIVATE keeps clicks from stealing focus; WS_EX_TOOLWINDOW keeps the
/// island out of Alt-Tab.
#[cfg(not(windows))]
pub fn make_non_activating(_win: &WebviewWindow) {}

#[cfg(windows)]
pub fn make_non_activating(win: &WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let want = ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
    }
}

/// Temporarily allow activation so a text field inside the island can be typed in.
/// macOS: give the island keyboard focus while a text field is in use.
#[cfg(not(windows))]
pub fn set_activating(win: &WebviewWindow, activating: bool) {
    if activating {
        let _ = win.set_focus();
    }
}

#[cfg(windows)]
pub fn set_activating(win: &WebviewWindow, activating: bool) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let want = if activating {
            ex & !(WS_EX_NOACTIVATE.0 as isize)
        } else {
            ex | WS_EX_NOACTIVATE.0 as isize
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
    }
}

/// Position, size and scale of the monitor the island lives on. Any change here
/// means the island has to be placed again.
fn current_screen_key(app: &AppHandle) -> Option<(i32, i32, u32, u32, u64)> {
    let pref = app
        .try_state::<crate::Shared>()
        .map(|s| s.settings.lock().unwrap().screen.clone())
        .unwrap_or_else(|| "primary".into());
    let m = target_monitor(app, &pref)?;
    let p = m.position();
    let size = m.size();
    Some((p.x, p.y, size.width, size.height, m.scale_factor().to_bits()))
}

/// Emits `cursor` (window-logical coordinates) at ~60 Hz while the island is
/// visible. Parked on a condvar the rest of the time.
pub fn spawn_cursor_poll(app: AppHandle, gate: Arc<PollGate>) {
    std::thread::spawn(move || {
        let mut was_down = false;
        // Remembered across wakes so a display change while hidden is noticed the
        // moment the island comes back.
        let mut last_screen: Option<(i32, i32, u32, u32, u64)> = None;
        loop {
            gate.wait_until_active();
            let mut last = (f64::MIN, f64::MIN);
            let mut ticks: u32 = 0;
            while gate.is_active() {
                std::thread::sleep(Duration::from_millis(16));

                // Monitors get plugged in, unplugged, rearranged and rescaled, and
                // an island pinned to coordinates that no longer exist is an island
                // nobody can reach. Checked about twice a second — the cursor poll
                // is already running, so this costs one monitor query.
                ticks = ticks.wrapping_add(1);
                if ticks % 30 == 0 {
                    let now = current_screen_key(&app);
                    if now.is_some() && now != last_screen {
                        let first = last_screen.is_none();
                        last_screen = now;
                        if !first {
                            crate::log::line("display layout changed — repositioning".to_string());
                            let _ = app.emit_to(WINDOW_LABEL, "screen-changed", ());
                        }
                    }
                }

                let Some(win) = window(&app) else { continue };
                let Ok(origin) = win.outer_position() else { continue };
                let scale = win.scale_factor().unwrap_or(1.0);
                let Some((cx, cy)) = cursor_physical(&app) else { continue };
                let x = (cx - origin.x as f64) / scale;
                let y = (cy - origin.y as f64) / scale;
                let size = match win.inner_size() {
                    Ok(s) => (s.width as f64 / scale, s.height as f64 / scale),
                    Err(_) => (PANEL_W, PANEL_H),
                };
                if (x - last.0).abs() < 1.0 && (y - last.1).abs() < 1.0 {
                    continue;
                }
                last = (x, y);

                // Click-through: the window only takes the mouse over the island
                // shape. A small entry margin means the flag is already off by the
                // time a moving cursor reaches a button.
                let r = *gate.rect.lock().unwrap();
                let on_island = r.w > 0.0
                    && x >= r.x - HIT_MARGIN
                    && x <= r.x + r.w + HIT_MARGIN
                    && y >= r.y - HIT_MARGIN
                    && y <= r.y + r.h + HIT_MARGIN;

                // A file being dragged has to be able to find us. WS_EX_TRANSPARENT
                // — what click-through is on Windows — hides the window from
                // WindowFromPoint, so OLE finds no drop target and shows the "no
                // drop" cursor. macOS has no such problem: AppKit delivers drags to
                // registered destinations whatever ignoresMouseEvents says. So while
                // a button is held anywhere over the panel, the whole panel takes
                // the mouse, which also makes the drop zone as forgiving as the Mac's.
                // A press may be the start of a drag: make sure the drop target is
                // ours before the file arrives.
                let down = left_button_down();
                if down && !was_down {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || unblock_webview_drops(&handle));
                }
                was_down = down;

                let dragging = down
                    && x >= 0.0
                    && x <= size.0
                    && y >= 0.0
                    && y <= size.1;

                let accept = on_island || dragging;
                if gate.ignoring.load(Ordering::Relaxed) == accept {
                    gate.ignoring.store(!accept, Ordering::Relaxed);
                    let _ = win.set_ignore_cursor_events(!accept);
                }

                let _ = win.emit("cursor", CursorPayload { x, y });
            }
        }
    });
}

pub fn set_ignore_cursor(app: &AppHandle, ignore: bool) {
    if let Some(win) = window(app) {
        let _ = win.set_ignore_cursor_events(ignore);
    }
}

/// macOS: float above every app, full-screen ones included, on every Space.
#[cfg(target_os = "macos")]
pub fn float_everywhere(win: &WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    let Ok(ptr) = win.ns_window() else { return };
    if ptr.is_null() {
        return;
    }
    let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle;
    ns.setCollectionBehavior(behavior);
    // Status-bar level: above normal and floating windows, like a notch panel.
    ns.setLevel(25);
}
