// Small append-only log at %LOCALAPPDATA%\Bader\bader.log — the Windows
// equivalent of nbLog() in HookServer.swift. Nothing leaves the machine.

use std::io::Write;

use crate::settings;

pub fn line(message: impl AsRef<str>) {
    let (y, mo, d, h, mi, se) = now_parts();
    let stamp = format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{se:02}");
    let dir = settings::local_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("bader.log");
    // Keep it from growing forever: start fresh past ~1 MB.
    if std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{stamp} {}", message.as_ref());
    }
}

/// Date and time as (year, month, day, hour, minute, second). Local time on
/// Windows; UTC elsewhere (no extra dependency for a log stamp).
pub fn now_parts() -> (u32, u32, u32, u32, u32, u32) {
    #[cfg(windows)]
    {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        (t.wYear as u32, t.wMonth as u32, t.wDay as u32, t.wHour as u32, t.wMinute as u32, t.wSecond as u32)
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Civil-from-days (Howard Hinnant).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        (y as u32, m as u32, d as u32, (rem / 3600) as u32, (rem % 3600 / 60) as u32, (rem % 60) as u32)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn now_parts_is_sane() {
        let (y, mo, d, h, mi, s) = super::now_parts();
        assert!(y >= 2025 && (1..=12).contains(&mo) && (1..=31).contains(&d));
        assert!(h < 24 && mi < 60 && s < 61);
    }
}
