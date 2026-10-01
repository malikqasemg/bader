//! Push-to-talk: Control + Option (macOS) / Ctrl + Alt (Windows).
//!
//! Hold the two keys to talk, let go to send. A quick tap toggles the mic
//! instead. Modifier-only combos can't be registered as global shortcuts, so a
//! small thread reads the modifier state every 30 ms (no extra permission).
//! Emits "ptt" with "down" | "up" | "tap".

use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Held this long = push-to-talk; shorter = tap.
const HOLD: Duration = Duration::from_millis(280);

#[cfg(target_os = "macos")]
fn combo_down() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
    }
    // kCGEventSourceStateCombinedSessionState = 0
    let f = unsafe { CGEventSourceFlagsState(0) };
    const SHIFT: u64 = 0x0002_0000;
    const CONTROL: u64 = 0x0004_0000;
    const OPTION: u64 = 0x0008_0000;
    const COMMAND: u64 = 0x0010_0000;
    f & CONTROL != 0 && f & OPTION != 0 && f & (SHIFT | COMMAND) == 0
}

#[cfg(windows)]
fn combo_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
    // VK_CONTROL, VK_MENU (Alt), VK_SHIFT, VK_LWIN, VK_RWIN
    down(0x11) && down(0x12) && !down(0x10) && !down(0x5B) && !down(0x5C)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn combo_down() -> bool {
    false
}

/// What the poller should report for one change of the combo state.
#[derive(Debug, PartialEq)]
enum Edge {
    Down,
    Up,
    Tap,
}

/// Pure state step, kept separate so it can be tested.
fn step(pressed: bool, since: &mut Option<Instant>, held: &mut bool, now: Instant) -> Option<Edge> {
    match (pressed, *since) {
        (true, None) => {
            *since = Some(now);
            None
        }
        (true, Some(t)) if !*held && now.duration_since(t) >= HOLD => {
            *held = true;
            Some(Edge::Down)
        }
        (false, Some(_)) => {
            *since = None;
            if std::mem::take(held) {
                Some(Edge::Up)
            } else {
                Some(Edge::Tap)
            }
        }
        _ => None,
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut since = None;
        let mut held = false;
        loop {
            std::thread::sleep(Duration::from_millis(30));
            if let Some(e) = step(combo_down(), &mut since, &mut held, Instant::now()) {
                let kind = match e {
                    Edge::Down => "down",
                    Edge::Up => "up",
                    Edge::Tap => "tap",
                };
                let _ = app.emit("ptt", kind);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_then_release_is_down_up_and_quick_press_is_tap() {
        let t0 = Instant::now();
        let (mut since, mut held) = (None, false);
        assert_eq!(step(true, &mut since, &mut held, t0), None);
        assert_eq!(step(true, &mut since, &mut held, t0 + Duration::from_millis(300)), Some(Edge::Down));
        assert_eq!(step(true, &mut since, &mut held, t0 + Duration::from_millis(600)), None);
        assert_eq!(step(false, &mut since, &mut held, t0 + Duration::from_millis(700)), Some(Edge::Up));
        assert_eq!(step(true, &mut since, &mut held, t0), None);
        assert_eq!(step(false, &mut since, &mut held, t0 + Duration::from_millis(100)), Some(Edge::Tap));
        assert_eq!(step(false, &mut since, &mut held, t0 + Duration::from_millis(200)), None);
    }
}
