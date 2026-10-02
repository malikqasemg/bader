// Quick lane: most questions ("what's on today?", "who emailed me?", "explain
// X", small talk) don't need the full engine with its tools and its large
// prompt. A fast model answers them directly — with the mail/calendar snapshot
// — and streams the words into the island as they come (~2–3 s instead of
// ~10 s). When the question needs tools or an action (send, search the web,
// make a file, use the computer, …) the fast model answers with a hand-off
// marker and the full engine takes over.

use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;

const HANDOFF: &str = "[[ENGINE]]";

const RULES: &str = "You are Bader (بدر), a personal AI assistant for a busy executive. \
Short and direct: answer first, detail only if asked. Plain text, no markdown symbols. \
You have NO tools in this mode. If answering needs ANY action or tool — sending, replying, forwarding or deleting mail; \
creating or changing calendar events; making or editing files or documents (Word, Excel, PowerPoint, PDF); \
searching the internet or opening websites; reading files; running commands; using the computer or browser; \
transcribing recordings; setting reminders or remembering something for later; or any fact that is not in this prompt \
and not stable general knowledge (news, prices, weather, anything recent) — reply with exactly [[ENGINE]] and nothing else. \
If the user refers to an earlier conversation, task, file or meeting ('yesterday', 'the transcript I shared', 'last time') answer from the 'Earlier asks' list below when it holds enough detail; if it does not, reply [[ENGINE]]. Otherwise answer from the snapshot below and general knowledge. Never invent emails, meetings, names, numbers or dates.";

/// Endpoint, key and model for the quick lane, from the engine's own settings.
fn lane() -> Option<(String, String, String)> {
    let home = crate::engine::home();
    let config = std::fs::read_to_string(home.join("config.yaml")).ok()?;
    let env = std::fs::read_to_string(home.join(".env")).unwrap_or_default();
    let env_get = |k: &str| {
        env.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(key, v)| key.trim() == k && !v.trim().is_empty())
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
    };
    let prefs = crate::runs::prefs();
    if prefs.get("quick_lane").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let provider = yaml_value(&config, "model.provider").unwrap_or_default();
    let engine_model = yaml_value(&config, "model.default").unwrap_or_default();
    let custom = prefs.get("quick_model").and_then(Value::as_str).map(str::to_string).filter(|s| !s.trim().is_empty());
    match provider.as_str() {
        "openrouter" => Some((
            "https://openrouter.ai/api/v1".into(),
            env_get("OPENROUTER_API_KEY")?,
            custom.unwrap_or_else(|| "anthropic/claude-haiku-4.5".into()),
        )),
        "openai" => Some((
            yaml_value(&config, "model.base_url").unwrap_or_else(|| "https://api.openai.com/v1".into()),
            env_get("OPENAI_API_KEY")?,
            custom.unwrap_or(engine_model),
        )),
        _ => None,
    }
}

fn yaml_value(text: &str, path: &str) -> Option<String> {
    let mut parts = path.split('.');
    let (section, key) = (parts.next()?, parts.next()?);
    let mut inside = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.trim().is_empty() {
            inside = line.trim_end() == format!("{section}:");
            continue;
        }
        if inside {
            let t = line.trim();
            if let Some(v) = t.strip_prefix(&format!("{key}:")) {
                let v = v.trim().trim_matches(|c| c == '\'' || c == '"');
                if !v.is_empty() && line.len() - line.trim_start().len() == 2 {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Local date parts. `log::now_parts` is UTC on macOS/Linux, so ask `date` there.
fn local_parts() -> (u32, u32, u32, u32, u32) {
    #[cfg(not(windows))]
    if let Ok(out) = std::process::Command::new("date").arg("+%Y %m %d %H %M").output() {
        let v: Vec<u32> = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();
        if v.len() == 5 {
            return (v[0], v[1], v[2], v[3], v[4]);
        }
    }
    let (y, mo, d, h, mi, _) = crate::log::now_parts();
    (y, mo, d, h, mi)
}

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Days since 1970-01-01 for a civil date (Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn weekday(y: u32, m: u32, d: u32) -> &'static str {
    // 1970-01-01 was a Thursday (index 4).
    DAYS[(days_from_civil(y as i64, m as i64, d as i64) + 4).rem_euclid(7) as usize]
}

/// Today with its weekday, so "yesterday" / "3 days ago" are counted from the right day.
fn now_line() -> String {
    let (y, mo, d, h, mi) = local_parts();
    let today = weekday(y, mo, d);
    let yesterday = DAYS[(DAYS.iter().position(|x| *x == today).unwrap_or(0) + 6) % 7];
    format!(
        "Now (user's local time): {today} {y:04}-{mo:02}-{d:02} {h:02}:{mi:02}. Yesterday was {yesterday}. \
Count 'N days ago' from this date; convert other time zones to local time before comparing days."
    )
}

/// Tries the quick lane. Ok(Some(text)) = answered; Ok(None) = hand to the engine.
pub async fn try_answer(app: &AppHandle, history: &[Value], query: &str) -> Result<Option<String>, String> {
    let Some((base, key, model)) = lane() else { return Ok(None) };
    let prefs = crate::runs::prefs();
    let mut system = format!("{RULES}\nIMPORTANT: {}\n{}", crate::runs::language_rules(&prefs), now_line());
    if let Some(s) = crate::runs::snapshot_text() {
        system.push_str("\n\n");
        system.push_str(&s);
    }
    let mut messages = vec![json!({ "role": "system", "content": system })];
    // Recent turns only: enough for follow-ups, small enough to stay fast.
    let start = history.len().saturating_sub(12);
    messages.extend(history[start..].iter().cloned());
    messages.push(json!({ "role": "user", "content": query }));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(&key)
        .header("X-Title", "Bader")
        .json(&json!({ "model": model, "messages": messages, "stream": true, "max_tokens": 900 }))
        .send()
        .await;
    let Ok(res) = res else { return Ok(None) };
    if !res.status().is_success() {
        crate::log::line(format!("quick lane {}: {}", model, res.status()));
        return Ok(None);
    }

    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    let mut text = String::new();
    let mut decided = false; // past the point where a hand-off marker could appear
    let first_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let next = if decided {
            stream.next().await
        } else {
            match tokio::time::timeout_at(first_deadline, stream.next()).await {
                Ok(n) => n,
                Err(_) => return Ok(None), // too slow to start: let the engine do it
            }
        };
        let Some(chunk) = next else { break };
        let Ok(chunk) = chunk else { break };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line: String = buf.drain(..=pos).collect();
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            let delta = v
                .pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
                .unwrap_or("");
            if delta.is_empty() {
                continue;
            }
            text.push_str(delta);
            if !decided {
                let t = text.trim_start();
                if t.starts_with(HANDOFF) || (HANDOFF.starts_with(t) && !t.is_empty()) {
                    if t.len() >= HANDOFF.len() {
                        crate::log::line("quick lane → engine");
                        return Ok(None);
                    }
                    continue; // could still become the marker
                }
                decided = true;
                let _ = app.emit_to(WINDOW_LABEL, "bader-delta", text.clone());
                continue;
            }
            let _ = app.emit_to(WINDOW_LABEL, "bader-delta", delta.to_string());
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() || text.contains(HANDOFF) {
        return Ok(None);
    }
    crate::log::line(format!("quick lane answered ({model})"));
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_nested_yaml() {
        let y = "model:\n  default: minimax/minimax-m3\n  provider: openrouter\n  base_url: 'https://openrouter.ai/api/v1'\nother:\n  provider: x\n";
        assert_eq!(super::yaml_value(y, "model.provider").as_deref(), Some("openrouter"));
        assert_eq!(super::yaml_value(y, "model.base_url").as_deref(), Some("https://openrouter.ai/api/v1"));
    }
}

#[cfg(test)]
mod date_tests {
    #[test]
    fn weekday_is_right() {
        assert_eq!(super::weekday(2026, 10, 2), "Friday");
        assert_eq!(super::weekday(1970, 1, 1), "Thursday");
        assert_eq!(super::weekday(2024, 2, 29), "Thursday");
    }
}
