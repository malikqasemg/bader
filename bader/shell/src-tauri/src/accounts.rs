// Accounts: sign Bader in to Gmail and Outlook.
//
// Gmail  — the engine's google-workspace setup script (PKCE OAuth): store the
//          customer's Google client file, open Google's sign-in page, then
//          exchange the address the browser lands on for a token.
// Outlook — Microsoft device-code sign-in (no redirect server needed): show a
//          short code, the user enters it at microsoft.com/devicelogin, the
//          token is saved in the engine profile for the Outlook skill.
//
// Tokens live in the engine profile folder; the island only learns
// "connected / not connected" through the *-connected markers.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::{json, Value};

const OUTLOOK_SCOPES: &str =
    "offline_access User.Read Mail.ReadWrite Mail.Send Calendars.ReadWrite";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsStatus {
    pub gmail: bool,
    pub gmail_client: bool,
    pub outlook: bool,
    pub outlook_client_id: String,
    pub outlook_tenant: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

// ── Gmail ─────────────────────────────────────────────────────────────────────

fn gsetup_script() -> Option<PathBuf> {
    let rel = ["skills", "productivity", "google-workspace", "scripts", "setup.py"];
    let mut a = crate::engine::home();
    let mut b = crate::voice::engine_root();
    for r in rel {
        a.push(r);
        b.push(r);
    }
    [a, b].into_iter().find(|p| p.is_file())
}

/// Runs the Google setup script in the engine; returns (exit ok, stdout).
fn gsetup(args: &[&str]) -> Result<(bool, String), String> {
    let root = crate::voice::engine_root();
    let python = crate::voice::engine_python(&root);
    if !python.is_file() {
        return Err("Bader engine is not installed on this computer.".into());
    }
    let script = gsetup_script().ok_or("Google sign-in is missing from the engine.")?;
    let mut cmd = Command::new(&python);
    cmd.arg(&script)
        .args(args)
        .current_dir(&root)
        .env("PYTHONPATH", &root)
        .env("HERMES_HOME", crate::engine::home())
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("Could not run the engine: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    Ok((out.status.success(), text))
}

fn last_json(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        return Some(v);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(&text[start..=end]).ok()
}

pub fn gmail_connected() -> bool {
    gsetup(&["--check"]).map(|(ok, _)| ok).unwrap_or(false)
}

fn gmail_client_present() -> bool {
    crate::engine::home().join("google_client_secret.json").is_file()
}

/// Finds a Google client file the user saved on the Desktop or in Downloads.
pub fn gmail_find_client_file() -> Option<String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    for dir in ["Desktop", "Downloads"] {
        let Ok(entries) = std::fs::read_dir(home.join(dir)) else { continue };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                name.starts_with("client_secret") && name.ends_with(".json")
            })
            .collect();
        found.sort();
        if let Some(p) = found.pop() {
            return Some(p.display().to_string());
        }
    }
    None
}

pub fn gmail_set_client(path: &str) -> Result<(), String> {
    let (ok, out) = gsetup(&["--client-secret", path])?;
    if ok {
        Ok(())
    } else {
        Err(short(&out, "That Google client file was not accepted."))
    }
}

/// Starts Google sign-in: returns the URL to open in the browser.
pub fn gmail_auth_url() -> Result<String, String> {
    // Older engines print the URL; newer ones can print JSON — accept both.
    let (_, out) = gsetup(&["--auth-url"])?;
    if let Some(url) = last_json(&out)
        .and_then(|v| v.get("auth_url").and_then(Value::as_str).map(str::to_string))
    {
        return Ok(url);
    }
    out.lines()
        .map(str::trim)
        .find(|l| l.starts_with("https://accounts.google.com/"))
        .map(str::to_string)
        .ok_or_else(|| short(&out, "Could not start Google sign-in."))
}

/// Finishes Google sign-in with the address the browser landed on.
pub fn gmail_auth_code(code: &str) -> Result<(), String> {
    let (ok, out) = gsetup(&["--auth-code", code.trim()])?;
    if ok && gmail_connected() {
        let _ = crate::secrets::set("gmail-connected", "1");
        crate::log::line("gmail connected");
        return Ok(());
    }
    let fresh = last_json(&out)
        .and_then(|v| v.get("fresh_auth_url").and_then(Value::as_str).map(str::to_string));
    match fresh {
        Some(url) => Err(format!("That sign-in expired. Open this one and try again: {url}")),
        None => Err(short(&out, "Google sign-in did not finish.")),
    }
}

pub fn gmail_disconnect() -> Result<(), String> {
    let _ = gsetup(&["--revoke"]);
    let _ = crate::secrets::clear("gmail-connected");
    Ok(())
}

// ── Outlook (Microsoft Graph, device code) ────────────────────────────────────

fn outlook_settings_path() -> PathBuf {
    crate::engine::home().join("outlook_app.json")
}

fn outlook_token_path() -> PathBuf {
    crate::engine::home().join("outlook_token.json")
}

fn outlook_app() -> (String, String) {
    let v: Value = std::fs::read_to_string(outlook_settings_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    (
        v.get("client_id").and_then(Value::as_str).unwrap_or("").to_string(),
        v.get("tenant").and_then(Value::as_str).unwrap_or("common").to_string(),
    )
}

pub async fn outlook_start(client_id: &str, tenant: &str) -> Result<DeviceCode, String> {
    let client_id = client_id.trim();
    let tenant = if tenant.trim().is_empty() { "common" } else { tenant.trim() };
    if client_id.len() < 8 {
        return Err("Enter the Outlook Client ID from your Microsoft app registration.".into());
    }
    std::fs::create_dir_all(crate::engine::home()).map_err(|e| e.to_string())?;
    std::fs::write(
        outlook_settings_path(),
        serde_json::to_string_pretty(&json!({ "client_id": client_id, "tenant": tenant })).unwrap(),
    )
    .map_err(|e| e.to_string())?;

    let url = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/devicecode");
    let res = reqwest::Client::new()
        .post(url)
        .form(&[("client_id", client_id), ("scope", OUTLOOK_SCOPES)])
        .send()
        .await
        .map_err(|e| format!("Could not reach Microsoft: {e}"))?;
    let v: Value = res.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = v.get("error_description").and_then(Value::as_str) {
        return Err(first_line(err));
    }
    Ok(DeviceCode {
        user_code: v.get("user_code").and_then(Value::as_str).unwrap_or("").to_string(),
        verification_uri: v
            .get("verification_uri")
            .and_then(Value::as_str)
            .unwrap_or("https://microsoft.com/devicelogin")
            .to_string(),
        device_code: v.get("device_code").and_then(Value::as_str).unwrap_or("").to_string(),
        interval: v.get("interval").and_then(Value::as_u64).unwrap_or(5),
        expires_in: v.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
    })
}

/// Waits until the user finishes signing in (or the code expires).
pub async fn outlook_wait(device_code: &str, interval: u64, expires_in: u64) -> Result<String, String> {
    let (client_id, tenant) = outlook_app();
    let url = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token");
    let client = reqwest::Client::new();
    let mut waited = 0u64;
    let mut every = interval.max(2);
    while waited < expires_in {
        tokio::time::sleep(std::time::Duration::from_secs(every)).await;
        waited += every;
        let res = client
            .post(&url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id.as_str()),
                ("device_code", device_code),
            ])
            .send()
            .await
            .map_err(|e| format!("Could not reach Microsoft: {e}"))?;
        let v: Value = res.json().await.map_err(|e| e.to_string())?;
        if v.get("access_token").is_some() {
            let mut token = v.clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let exp = v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
            token["expires_at"] = json!(now + exp);
            token["client_id"] = json!(client_id);
            token["tenant"] = json!(tenant);
            std::fs::write(outlook_token_path(), serde_json::to_string_pretty(&token).unwrap())
                .map_err(|e| e.to_string())?;
            let who = whoami(v.get("access_token").and_then(Value::as_str).unwrap_or("")).await;
            let _ = crate::secrets::set("outlook-connected", "1");
            crate::log::line("outlook connected");
            return Ok(who);
        }
        match v.get("error").and_then(Value::as_str).unwrap_or("") {
            "authorization_pending" => continue,
            "slow_down" => every += 5,
            "authorization_declined" => return Err("Sign-in was declined.".into()),
            "expired_token" => return Err("The code expired. Start again.".into()),
            _ => {
                let msg = v
                    .get("error_description")
                    .and_then(Value::as_str)
                    .unwrap_or("Microsoft sign-in failed.");
                return Err(first_line(msg));
            }
        }
    }
    Err("The code expired. Start again.".into())
}

async fn whoami(token: &str) -> String {
    let res = reqwest::Client::new()
        .get("https://graph.microsoft.com/v1.0/me")
        .bearer_auth(token)
        .send()
        .await;
    let Ok(res) = res else { return String::new() };
    let v: Value = res.json().await.unwrap_or(Value::Null);
    v.get("mail")
        .and_then(Value::as_str)
        .or_else(|| v.get("userPrincipalName").and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

pub fn outlook_disconnect() -> Result<(), String> {
    let _ = std::fs::remove_file(outlook_token_path());
    let _ = crate::secrets::clear("outlook-connected");
    Ok(())
}

pub fn status() -> AccountsStatus {
    let (client_id, tenant) = outlook_app();
    let outlook = outlook_token_path().is_file();
    let gmail = gmail_connected();
    // Keep the island markers in step with what is really there.
    let mark = |key: &str, on: bool| {
        if on {
            let _ = crate::secrets::set(key, "1");
        } else {
            let _ = crate::secrets::clear(key);
        }
    };
    mark("outlook-connected", outlook);
    mark("gmail-connected", gmail);
    AccountsStatus {
        gmail,
        gmail_client: gmail_client_present(),
        outlook,
        outlook_client_id: client_id,
        outlook_tenant: tenant,
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).trim().chars().take(240).collect()
}

fn short(out: &str, fallback: &str) -> String {
    let line = out
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('{') && !l.starts_with('}'))
        .unwrap_or("");
    if line.is_empty() {
        fallback.to_string()
    } else {
        line.chars().take(240).collect()
    }
}
