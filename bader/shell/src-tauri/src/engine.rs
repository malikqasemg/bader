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

/// .env keys the settings window may write.
const ENV_KEYS: &[&str] = &[
    "OPENROUTER_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "VOICE_TOOLS_OPENAI_KEY",
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
        values: yaml_values(&config, YAML_KEYS),
        keys,
    }
}

/// Applies YAML values (through the engine CLI) and .env keys (direct write).
pub fn apply(values: HashMap<String, String>, secrets: HashMap<String, String>) -> Result<(), String> {
    for k in values.keys().chain(secrets.keys()) {
        if !YAML_KEYS.contains(&k.as_str()) && !ENV_KEYS.contains(&k.as_str()) {
            return Err(format!("Setting not allowed: {k}"));
        }
    }
    let dir = home();
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

/// Stops the engine (if running) and starts it again in the background.
pub fn restart() -> Result<(), String> {
    let bin = hermes_bin().ok_or("Engine not found on this computer.")?;
    #[cfg(not(windows))]
    {
        let _ = Command::new("pkill")
            .args(["-f", "--", &format!("-p {PROFILE} gateway run")])
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
        .args(["-p", PROFILE, "gateway", "run"])
        .env("HERMES_ACCEPT_HOOKS", "1")
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
