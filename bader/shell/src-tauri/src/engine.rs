// Bader engine control: read and change the engine's AI and voice settings,
// and restart it. The engine is Hermes running the "bader" profile; its files
// live in the profile folder (config.yaml + .env).
//
// - YAML settings go through the engine's own CLI (`hermes -p bader config set`),
//   so the engine validates them and keeps its file format.
// - API keys are written straight into the profile's .env (never passed on a
//   command line, never sent to the island).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::Value;

const PROFILE: &str = "bader";
const HEALTH_URL: &str = "http://127.0.0.1:8642/health";

/// YAML keys the settings window may change.
const YAML_KEYS: &[&str] = &[
    "model.provider",
    "model.default",
    "model.base_url",
    "stt.enabled",
    "stt.provider",
    "stt.language",
    "stt.local.model",
    "stt.local.language",
    "tts.provider",
    "tts.edge.voice",
    "voice.auto_tts",
];

/// Bader's own voice choices (Arabic + English), kept in bader_voice.json.
const VOICE_KEYS: &[&str] = &[
    "bader.voice_ar",
    "bader.voice_en",
    "bader.answer_lang",
    "bader.summary_lang",
    "bader.approvals",
    "bader.quick_lane",
    "bader.quick_model",
    "bader.screen_reply",
    // First-run setup: main + second language, and "setup finished".
    "bader.primary_lang",
    "bader.second_lang",
    "bader.setup_done",
    "bader.buddy",
];

/// .env keys the settings window may write.
const ENV_KEYS: &[&str] = &[
    "OPENROUTER_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "VOICE_TOOLS_OPENAI_KEY",
    // Phone access (Telegram bot) — see Settings → Phone.
    "TELEGRAM_BOT_TOKEN",
    "TELEGRAM_ALLOWED_USERS",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub found: bool,
    pub running: bool,
    pub home: String,
    pub values: HashMap<String, String>,
    pub keys: HashMap<String, bool>,
}

/// Profile folder: BADER_ENGINE_HOME, else the Hermes default for this OS.
pub fn home() -> PathBuf {
    if let Some(p) = std::env::var_os("BADER_ENGINE_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hermes");
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".hermes");
    base.join("profiles").join(PROFILE)
}

/// The engine launcher: BADER_HERMES_BIN, the usual install spots, then PATH.
fn hermes_bin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BADER_HERMES_BIN").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        let l = PathBuf::from(l).join("hermes");
        candidates.push(l.join("bin").join("hermes.exe"));
        candidates.push(l.join("bin").join("hermes.cmd"));
    }
    #[cfg(not(windows))]
    if let Some(h) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(h).join(".local").join("bin").join("hermes"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join(if cfg!(windows) { "hermes.exe" } else { "hermes" }));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Minimal reader for `a.b.c: value` lines in the engine's config.yaml.
fn yaml_values(text: &str, wanted: &[&str]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let Some((key, rest)) = trimmed.split_once(':') else { continue };
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let mut path: Vec<&str> = stack.iter().map(|(_, k)| k.as_str()).collect();
        path.push(key.trim());
        let full = path.join(".");
        let value = rest.trim();
        if value.is_empty() {
            stack.push((indent, key.trim().to_string()));
        } else if wanted.contains(&full.as_str()) && !out.contains_key(&full) {
            out.insert(full, value.trim_matches(|c| c == '\'' || c == '"').to_string());
        }
    }
    out
}

fn env_present(text: &str, key: &str) -> bool {
    text.lines().any(|l| {
        l.split_once('=')
            .is_some_and(|(k, v)| k.trim() == key && !v.trim().is_empty())
    })
}

/// Replaces or appends `KEY=value` in a .env text; an empty value removes the line.
fn env_upsert(text: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    for l in text.lines() {
        let is_key = l.split_once('=').is_some_and(|(k, _)| k.trim() == key);
        if is_key {
            if !done && !value.is_empty() {
                out.push(format!("{key}={value}"));
            }
            done = true;
        } else {
            out.push(l.to_string());
        }
    }
    if !done && !value.is_empty() {
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

pub async fn status() -> EngineStatus {
    let dir = home();
    let config = std::fs::read_to_string(dir.join("config.yaml")).unwrap_or_default();
    let env = std::fs::read_to_string(dir.join(".env")).unwrap_or_default();
    let keys = ENV_KEYS
        .iter()
        .map(|k| (k.to_string(), env_present(&env, k)))
        .collect();
    let running = reqwest::Client::new()
        .get(HEALTH_URL)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    EngineStatus {
        found: !config.is_empty(),
        running,
        home: dir.display().to_string(),
        values: {
            let mut v = yaml_values(&config, YAML_KEYS);
            let voices: Value = std::fs::read_to_string(dir.join("bader_voice.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null);
            for (k, j) in [("bader.voice_ar", "ar"), ("bader.voice_en", "en")] {
                if let Some(s) = voices.get(j).and_then(Value::as_str) {
                    v.insert(k.to_string(), s.to_string());
                }
            }
            let prefs: Value = std::fs::read_to_string(dir.join("bader_prefs.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null);
            for (k, j) in [("bader.answer_lang", "answer_lang"), ("bader.summary_lang", "summary_lang")] {
                v.insert(k.to_string(), prefs.get(j).and_then(Value::as_str).unwrap_or("auto").to_string());
            }
            v.insert(
                "bader.quick_lane".into(),
                (prefs.get("quick_lane").and_then(Value::as_bool) != Some(false)).to_string(),
            );
            v.insert(
                "bader.quick_model".into(),
                prefs.get("quick_model").and_then(Value::as_str).unwrap_or("").to_string(),
            );
            // Touch screen buttons: "ask" each time, "text" only, or "voice" (text + voice).
            v.insert(
                "bader.screen_reply".into(),
                prefs.get("screen_reply").and_then(Value::as_str).unwrap_or("ask").to_string(),
            );
            v.insert(
                "bader.primary_lang".into(),
                prefs.get("primary_lang").and_then(Value::as_str).unwrap_or("en").to_string(),
            );
            v.insert(
                "bader.second_lang".into(),
                prefs.get("second_lang").and_then(Value::as_str).unwrap_or("ar").to_string(),
            );
            // The character on the desktop: always | events | off.
            v.insert(
                "bader.buddy".into(),
                prefs.get("buddy").and_then(Value::as_str).unwrap_or("always").to_string(),
            );
            v.insert(
                "bader.setup_done".into(),
                (prefs.get("setup_done").and_then(Value::as_bool) == Some(true)).to_string(),
            );
            v.insert(
                "bader.approvals".into(),
                (prefs.get("approvals").and_then(Value::as_bool) != Some(false)).to_string(),
            );
            v
        },
        keys,
    }
}

/// Applies YAML values (through the engine CLI) and .env keys (direct write).
pub fn apply(mut values: HashMap<String, String>, secrets: HashMap<String, String>) -> Result<(), String> {
    for k in values.keys().chain(secrets.keys()) {
        if !YAML_KEYS.contains(&k.as_str())
            && !ENV_KEYS.contains(&k.as_str())
            && !VOICE_KEYS.contains(&k.as_str())
        {
            return Err(format!("Setting not allowed: {k}"));
        }
    }
    let dir = home();
    let answer = values.remove("bader.answer_lang");
    let summary = values.remove("bader.summary_lang");
    let approvals = values.remove("bader.approvals");
    if answer.is_some() || summary.is_some() || approvals.is_some() {
        write_prefs(&dir, answer, summary, approvals)?;
    }
    let quick_lane = values.remove("bader.quick_lane");
    let quick_model = values.remove("bader.quick_model");
    let screen_reply = values.remove("bader.screen_reply").filter(|v| ["ask", "text", "voice"].contains(&v.as_str()));
    let primary = values.remove("bader.primary_lang").filter(|v| ["en", "ar"].contains(&v.as_str()));
    let second = values.remove("bader.second_lang").filter(|v| ["en", "ar", "none"].contains(&v.as_str()));
    let setup_done = values.remove("bader.setup_done");
    let buddy = values.remove("bader.buddy").filter(|v| ["always", "events", "off"].contains(&v.as_str()));
    if quick_lane.is_some()
        || quick_model.is_some()
        || screen_reply.is_some()
        || primary.is_some()
        || second.is_some()
        || setup_done.is_some()
        || buddy.is_some()
    {
        let path = dir.join("bader_prefs.json");
        let mut cur: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(q) = quick_lane {
            cur["quick_lane"] = Value::Bool(q != "false");
        }
        if let Some(m) = quick_model {
            cur["quick_model"] = Value::String(m.trim().to_string());
        }
        if let Some(r) = screen_reply {
            cur["screen_reply"] = Value::String(r);
        }
        if let Some(l) = primary {
            cur["primary_lang"] = Value::String(l);
        }
        if let Some(l) = second {
            cur["second_lang"] = Value::String(l);
        }
        if let Some(b) = buddy {
            cur["buddy"] = Value::String(b);
        }
        if let Some(d) = setup_done {
            cur["setup_done"] = Value::Bool(d == "true");
        }
        std::fs::write(&path, serde_json::to_string_pretty(&cur).unwrap_or_default())
            .map_err(|e| format!("Could not save preferences: {e}"))?;
    }
    let ar = values.remove("bader.voice_ar");
    let en = values.remove("bader.voice_en");
    if ar.is_some() || en.is_some() {
        let path = dir.join("bader_voice.json");
        let mut cur: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(a) = ar {
            cur["ar"] = Value::String(a);
        }
        if let Some(e) = en {
            cur["en"] = Value::String(e);
        }
        std::fs::write(&path, serde_json::to_string_pretty(&cur).unwrap_or_default())
            .map_err(|e| format!("Could not save voices: {e}"))?;
    }
    if !secrets.is_empty() {
        let path = dir.join(".env");
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        for (k, v) in &secrets {
            if !ENV_KEYS.contains(&k.as_str()) {
                continue;
            }
            if v.contains('\n') || v.contains('\r') {
                return Err("Key must be a single line.".into());
            }
            text = env_upsert(&text, k, v.trim());
        }
        std::fs::write(&path, text).map_err(|e| format!("Could not write engine keys: {e}"))?;
    }
    if !values.is_empty() {
        let bin = hermes_bin().ok_or("Engine not found on this computer.")?;
        for (k, v) in &values {
            if !YAML_KEYS.contains(&k.as_str()) {
                continue;
            }
            let out = Command::new(&bin)
                .args(["-p", PROFILE, "config", "set", k, v])
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("Could not run the engine: {e}"))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!("Engine refused {k}: {}", err.trim().chars().take(200).collect::<String>()));
            }
        }
    }
    crate::log::line(format!(
        "engine settings changed: {} values, {} keys",
        values.len(),
        secrets.len()
    ));
    Ok(())
}

fn lang_word(v: &str) -> &'static str {
    match v {
        "en" => "en",
        "ar" => "ar",
        _ => "auto",
    }
}

/// Saves language + approval preferences (bader_prefs.json) and mirrors them
/// into the engine persona, so Telegram / WhatsApp follow them too.
fn write_prefs(
    dir: &std::path::Path,
    answer: Option<String>,
    summary: Option<String>,
    approvals: Option<String>,
) -> Result<(), String> {
    let path = dir.join("bader_prefs.json");
    let mut cur: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(a) = answer {
        cur["answer_lang"] = Value::String(lang_word(&a).into());
    }
    if let Some(s) = summary {
        cur["summary_lang"] = Value::String(lang_word(&s).into());
    }
    if let Some(a) = approvals {
        cur["approvals"] = Value::Bool(a != "false");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&cur).unwrap_or_default())
        .map_err(|e| format!("Could not save preferences: {e}"))?;

    let answer_rule = match cur.get("answer_lang").and_then(Value::as_str).unwrap_or("auto") {
        "en" => "- Always reply in English, even when the user writes in Arabic.",
        "ar" => "- Always reply in Arabic (clear, formal), even when the user writes in English.",
        _ => "- Reply in the language the user writes in.",
    };
    let summary_rule = match cur.get("summary_lang").and_then(Value::as_str).unwrap_or("auto") {
        "en" => "- Meeting summaries, briefs and action items: always in English, whatever the meeting language.",
        "ar" => "- Meeting summaries, briefs and action items: always in Arabic, whatever the meeting language.",
        _ => "- Meeting summaries and action items: use the meeting's main language.",
    };
    let approval_rule = if cur.get("approvals").and_then(Value::as_bool) == Some(false) {
        "- The user turned approvals off: do what they ask without asking first."
    } else {
        "- Sending, replying, forwarding or deleting mail, and creating or deleting calendar events need the user's approval. The system asks automatically; just run the action and wait."
    };
    let block = format!(
        "<!-- bader:prefs -->\n## Language\n{answer_rule}\n{summary_rule}\n\n## Actions in the user's name\n{approval_rule}\n<!-- /bader:prefs -->"
    );
    let soul_path = dir.join("SOUL.md");
    let soul = std::fs::read_to_string(&soul_path).unwrap_or_default();
    let new = match (soul.find("<!-- bader:prefs -->"), soul.find("<!-- /bader:prefs -->")) {
        (Some(a), Some(b)) if b > a => {
            format!("{}{}{}", &soul[..a], block, &soul[b + "<!-- /bader:prefs -->".len()..])
        }
        _ => format!("{}\n\n{}\n", soul.trim_end(), block),
    };
    std::fs::write(&soul_path, new).map_err(|e| format!("Could not update the persona: {e}"))
}

/// Stops the engine (if running) and starts it again in the background.
pub fn restart() -> Result<(), String> {
    let bin = hermes_bin().ok_or("Engine not found on this computer.")?;
    #[cfg(not(windows))]
    {
        // The engine's command line looks like `… '-p', 'bader', 'gateway', 'run' …`.
        let _ = Command::new("pkill")
            .args(["-f", &format!("{PROFILE}.{{0,6}}gateway.{{0,6}}run")])
            .status();
    }
    #[cfg(windows)]
    {
        // Bundled-engine process management for Windows comes with the engine
        // installer; until then a running engine is restarted by hand.
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let log_dir = crate::settings::local_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log = std::fs::File::create(log_dir.join("engine.log")).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    Command::new(&bin)
        // --replace takes over from an engine that is still draining old work.
        .args(["-p", PROFILE, "gateway", "run", "--replace"])
        .env("HERMES_ACCEPT_HOOKS", "1")
        // Tools the engine runs (mail, calendar) must see this profile's tokens.
        .env("HERMES_HOME", home())
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Could not start the engine: {e}"))?;
    crate::log::line("engine restarted");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_yaml_values() {
        let y = "model:\n  default: minimax/minimax-m3\n  provider: openrouter\ntts:\n  provider: edge\n  edge:\n    voice: ar-SA-HamedNeural\n  openai:\n    voice: alloy\n";
        let v = yaml_values(y, YAML_KEYS);
        assert_eq!(v["model.default"], "minimax/minimax-m3");
        assert_eq!(v["model.provider"], "openrouter");
        assert_eq!(v["tts.edge.voice"], "ar-SA-HamedNeural");
        assert!(!v.contains_key("tts.openai.voice"));
    }

    #[test]
    fn env_upsert_replaces_appends_and_removes() {
        let t = "A=1\nOPENAI_API_KEY=old\n";
        let t = env_upsert(t, "OPENAI_API_KEY", "new");
        assert!(t.contains("OPENAI_API_KEY=new") && !t.contains("old"));
        let t = env_upsert(&t, "ANTHROPIC_API_KEY", "x");
        assert!(t.contains("ANTHROPIC_API_KEY=x"));
        let t = env_upsert(&t, "A", "");
        assert!(!t.contains("A=1"));
        assert!(env_present(&t, "OPENAI_API_KEY") && !env_present(&t, "A"));
    }
}

/// True until the first-run setup has been finished (or skipped).
pub fn needs_setup() -> bool {
    let prefs: Value = std::fs::read_to_string(home().join("bader_prefs.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    prefs.get("setup_done").and_then(Value::as_bool) != Some(true)
}

/// One value from bader_prefs.json.
pub fn pref_get(key: &str) -> Option<Value> {
    let prefs: Value = std::fs::read_to_string(home().join("bader_prefs.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    prefs.get(key).cloned()
}

/// Sets one value in bader_prefs.json (the rest is kept).
pub fn pref_set(key: &str, value: Value) {
    let path = home().join("bader_prefs.json");
    let mut cur: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    cur[key] = value;
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&cur).unwrap_or_default());
}
