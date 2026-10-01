// Background sync: every few minutes, pull recent mail and the next two days
// of calendar into the engine profile (bader_inbox.json). Mail / calendar
// questions are then answered from this snapshot in one model call, and the
// island + face screen get "unread" and "next meeting" for the idle display.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;

const EVERY: Duration = Duration::from_secs(5 * 60);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub unread: usize,
    pub mails: usize,
    pub next_title: Option<String>,
    pub next_start: Option<String>,
    pub updated: Option<String>,
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        run_once(&app);
        std::thread::sleep(EVERY);
    });
}

pub fn run_once(app: &AppHandle) {
    match crate::voice::run_script(app, "bader_sync.py", &[]) {
        Ok(v) => crate::log::line(format!("sync: {v}")),
        Err(e) => crate::log::line(format!("sync failed: {e}")),
    }
    if let Some(info) = info() {
        let _ = app.emit_to(WINDOW_LABEL, "bader-snapshot", info);
    }
}

/// What the idle screen shows: unread mail and the next meeting still ahead.
pub fn info() -> Option<SnapshotInfo> {
    let raw = std::fs::read_to_string(crate::engine::home().join("bader_inbox.json")).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let mails = v.get("gmail").and_then(Value::as_array).cloned().unwrap_or_default();
    let unread = mails
        .iter()
        .filter(|m| m.get("unread").and_then(Value::as_bool) == Some(true))
        .count();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let next = v
        .get("calendar")
        .and_then(Value::as_array)
        .and_then(|events| {
            events
                .iter()
                .filter(|e| {
                    e.get("end")
                        .and_then(Value::as_str)
                        .and_then(epoch_of)
                        .is_some_and(|end| end > now)
                })
                .min_by_key(|e| e.get("start").and_then(Value::as_str).and_then(epoch_of).unwrap_or(i64::MAX))
                .cloned()
        });
    Some(SnapshotInfo {
        unread,
        mails: mails.len(),
        next_title: next.as_ref().and_then(|e| e.get("title")).and_then(Value::as_str).map(str::to_string),
        next_start: next.as_ref().and_then(|e| e.get("start")).and_then(Value::as_str).map(str::to_string),
        updated: v.get("updated").and_then(Value::as_str).map(str::to_string),
    })
}

/// Seconds since the epoch for "2026-10-01T18:00:00+03:00" (RFC 3339, with offset).
fn epoch_of(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |a: usize, n: usize| s.get(a..a + n)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (num(0, 4)?, num(5, 2)?, num(8, 2)?, num(11, 2)?, num(14, 2)?, num(17, 2)?);
    // Days from civil (Howard Hinnant).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let mut t = days * 86_400 + h * 3600 + mi * 60 + se;
    let rest = &s[19..];
    let tz = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign) = tz.chars().next().filter(|c| *c == '+' || *c == '-') {
        let oh = tz.get(1..3)?.parse::<i64>().ok()?;
        let om = tz.get(4..6).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        let off = oh * 3600 + om * 60;
        t -= if sign == '+' { off } else { -off };
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_rfc3339_with_offset() {
        assert_eq!(super::epoch_of("1970-01-01T03:00:00+03:00"), Some(0));
        assert_eq!(super::epoch_of("2026-10-01T15:00:00Z"), super::epoch_of("2026-10-01T18:00:00+03:00"));
    }
}
