// Bader chat over the engine's "runs" API: one turn = POST /v1/runs, then a
// live event stream (GET /v1/runs/{id}/events). The stream gives the island
// what the old one-shot call could not: which tool Bader is using right now,
// approval requests (human in the middle), and the final answer.
//
// Speed: for mail / calendar questions the latest snapshot from the
// background sync (bader_inbox.json) rides along in the instructions, so the
// engine answers in one model call instead of a multi-step tool loop.

use std::sync::Mutex;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;

/// The approval Bader is waiting on, if any: (run_id, request_id).
static PENDING: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);
/// The run in progress, so it can be stopped.
static CURRENT: Mutex<Option<String>> = Mutex::new(None);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    /// "tool" | "approval" | "approval-resolved" | "interim"
    pub kind: String,
    pub tool: Option<String>,
    pub text: Option<String>,
}

fn emit(app: &AppHandle, kind: &str, tool: Option<String>, text: Option<String>) {
    let _ = app.emit_to(WINDOW_LABEL, "bader-run", RunEvent { kind: kind.into(), tool, text });
}

// ── Preferences (language, approvals) ────────────────────────────────────────

pub fn prefs() -> Value {
    std::fs::read_to_string(crate::engine::home().join("bader_prefs.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

pub fn language_rules(p: &Value) -> String {
    let answer = match p.get("answer_lang").and_then(Value::as_str).unwrap_or("auto") {
        "en" => "Always answer in English, even when the user writes in Arabic.",
        "ar" => "Always answer in Arabic (clear, formal), even when the user writes in English.",
        _ => "Answer in the language the user writes in (Arabic or English).",
    };
    let summary = match p.get("summary_lang").and_then(Value::as_str).unwrap_or("auto") {
        "en" => "Write meeting summaries, briefs and action items in English, whatever language the meeting was in.",
        "ar" => "Write meeting summaries, briefs and action items in Arabic, whatever language the meeting was in.",
        _ => "Write meeting summaries and action items in the meeting's main language.",
    };
    format!("{answer} {summary} Keep names, companies and numbers exactly as written.")
}

// ── Snapshot (fast path for mail / calendar questions) ───────────────────────

const MAIL_WORDS: &[&str] = &[
    "mail", "email", "e-mail", "inbox", "message", "meeting", "calendar", "schedule", "agenda",
    "today", "tomorrow", "brief", "unread", "who wrote", "who sent",
    "بريد", "ايميل", "إيميل", "إيميلات", "ايميلات", "رسائل", "رسالة", "اجتماع", "اجتماعات",
    "تقويم", "جدول", "اليوم", "بكرة", "غدا", "غداً", "موعد", "مواعيد", "ملخص", "لخص",
];

fn wants_snapshot(query: &str) -> bool {
    let q = query.to_lowercase();
    MAIL_WORDS.iter().any(|w| q.contains(w))
}

fn snapshot_text() -> Option<String> {
    let raw = std::fs::read_to_string(crate::engine::home().join("bader_inbox.json")).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let mut out = format!(
        "[Bader snapshot] Mail and calendar as of {} (answer from this; use tools only for full bodies, older items or actions).\n",
        v.get("updated").and_then(Value::as_str).unwrap_or("recently")
    );
    if let Some(events) = v.get("calendar").and_then(Value::as_array) {
        out.push_str("Calendar (next 2 days):\n");
        if events.is_empty() {
            out.push_str("- none\n");
        }
        for e in events {
            out.push_str(&format!(
                "- {} → {} | {}\n",
                e.get("start").and_then(Value::as_str).unwrap_or(""),
                e.get("end").and_then(Value::as_str).unwrap_or(""),
                e.get("title").and_then(Value::as_str).unwrap_or("")
            ));
        }
    }
    if let Some(mails) = v.get("gmail").and_then(Value::as_array) {
        out.push_str("Gmail inbox (last 2 days, newest first, promotions/social excluded):\n");
        for m in mails {
            out.push_str(&format!(
                "- {}{} | from {} | {} | {}\n",
                if m.get("unread").and_then(Value::as_bool) == Some(true) { "[unread] " } else { "" },
                m.get("date").and_then(Value::as_str).unwrap_or(""),
                m.get("from").and_then(Value::as_str).unwrap_or(""),
                m.get("subject").and_then(Value::as_str).unwrap_or(""),
                m.get("snippet").and_then(Value::as_str).unwrap_or("")
            ));
        }
    }
    Some(out)
}

// ── One turn ─────────────────────────────────────────────────────────────────

pub async fn send(
    app: &AppHandle,
    base: &str,
    key: &str,
    system: &str,
    history: Vec<Value>,
    query: &str,
) -> Result<String, String> {
    let p = prefs();
    let mut instructions = format!("{system}\nIMPORTANT: {}", language_rules(&p));
    // The persona says "reply in the user's language"; a fixed choice must win,
    // so it also rides on the message itself.
    let query_owned = match p.get("answer_lang").and_then(Value::as_str).unwrap_or("auto") {
        "en" => format!("{query}\n\n[Reply in English only.]"),
        "ar" => format!("{query}\n\n[Reply in Arabic only.]"),
        _ => query.to_string(),
    };
    let query = query_owned.as_str();
    if wants_snapshot(query) {
        if let Some(s) = snapshot_text() {
            instructions.push_str("\n\n");
            instructions.push_str(&s);
        }
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .build()
        .map_err(|e| e.to_string())?;
    let auth = |r: reqwest::RequestBuilder| if key.is_empty() { r } else { r.bearer_auth(key) };

    let body = json!({ "input": query, "instructions": instructions, "conversation_history": history });
    let res = auth(client.post(format!("{base}/runs")).json(&body))
        .send()
        .await
        .map_err(|_| "Bader engine is not running. Start it from the tray.".to_string())?;
    let started: Value = res.json().await.map_err(|e| format!("Bad engine response: {e}"))?;
    let run_id = started
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            started
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("The engine did not start the run.")
                .to_string()
        })?
        .to_string();
    *CURRENT.lock().unwrap() = Some(run_id.clone());

    let stream_res = auth(client.get(format!("{base}/runs/{run_id}/events")))
        .send()
        .await
        .map_err(|e| format!("Lost the engine: {e}"))?;
    let mut stream = stream_res.bytes_stream();
    let mut buf = String::new();
    let mut result: Option<Result<String, String>> = None;

    'outer: while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Lost the engine: {e}"))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find("\n\n") {
            let frame: String = buf.drain(..pos + 2).collect();
            for line in frame.lines() {
                let Some(data) = line.strip_prefix("data:") else { continue };
                let Ok(ev) = serde_json::from_str::<Value>(data.trim()) else { continue };
                let name = ev.get("event").and_then(Value::as_str).unwrap_or("");
                match name {
                    "tool.started" => {
                        let tool = ev.get("tool").and_then(Value::as_str).map(str::to_string);
                        let preview = ev.get("preview").and_then(Value::as_str).map(|s| s.chars().take(120).collect());
                        emit(app, "tool", tool, preview);
                    }
                    "message.interim" => {
                        let text = ev.get("text").and_then(Value::as_str).map(str::to_string);
                        emit(app, "interim", None, text);
                    }
                    "approval.request" => {
                        let request_id = ev.get("request_id").and_then(Value::as_str).map(str::to_string);
                        let what = ev
                            .get("description")
                            .or_else(|| ev.get("message"))
                            .or_else(|| ev.get("command"))
                            .and_then(Value::as_str)
                            .unwrap_or("An action needs your approval")
                            .to_string();
                        *PENDING.lock().unwrap() = Some((run_id.clone(), request_id));
                        crate::log::line(format!("approval requested: {what}"));
                        emit(app, "approval", None, Some(what));
                    }
                    "run.completed" => {
                        let out = ev.get("output").and_then(Value::as_str).unwrap_or("").trim().to_string();
                        result = Some(if out.is_empty() {
                            Err("No response from the Bader engine.".into())
                        } else {
                            Ok(out)
                        });
                        break 'outer;
                    }
                    "run.failed" | "run.cancelled" | "run.interrupted" => {
                        let err = ev.get("error").and_then(Value::as_str).unwrap_or("The request was stopped.");
                        result = Some(Err(format!("Bader engine: {err}")));
                        break 'outer;
                    }
                    _ => {}
                }
            }
        }
    }
    *CURRENT.lock().unwrap() = None;
    if PENDING.lock().unwrap().as_ref().is_some_and(|(r, _)| r == &run_id) {
        *PENDING.lock().unwrap() = None;
        emit(app, "approval-resolved", None, None);
    }
    result.unwrap_or_else(|| Err("The engine closed the connection.".into()))
}

/// Answers the pending approval: "once" (approve) or "deny". Returns false if
/// nothing was waiting.
pub async fn answer(app: &AppHandle, base: &str, key: &str, choice: &str) -> Result<bool, String> {
    let Some((run_id, request_id)) = PENDING.lock().unwrap().take() else {
        return Ok(false);
    };
    let choice = if choice == "deny" { "deny" } else { "once" };
    let mut body = json!({ "choice": choice });
    if let Some(r) = request_id {
        body["request_id"] = json!(r);
    }
    let mut req = reqwest::Client::new().post(format!("{base}/runs/{run_id}/approval")).json(&body);
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let res = req.send().await.map_err(|e| format!("Could not reach the engine: {e}"))?;
    crate::log::line(format!("approval answered: {choice} ({})", res.status()));
    emit(app, "approval-resolved", None, Some(choice.to_string()));
    Ok(res.status().is_success())
}

pub fn has_pending() -> bool {
    PENDING.lock().unwrap().is_some()
}
