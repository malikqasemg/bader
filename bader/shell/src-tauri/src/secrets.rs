// API keys never reach the front end — the island can only ask whether a key is present.

#[cfg(not(target_os = "macos"))]
const SERVICE: &str = "io.inetgenius.bader";

/// Every key Bader may store. Anything outside this list is refused.
pub const KNOWN_KEYS: &[&str] = &[
    "bader-engine-key",
    "bader-license-key",
    "outlook-connected",
    "webex-connected",
    "gmail-connected",
    "anthropic-api-key",
    "n8n-url",
    "n8n-api-key",
    "vercel-token",
    "github-token",
    "stripe-api-key",
    "resend-api-key",
    "notion-api-key",
    "calcom-api-key",
];

// Windows: the Credential Manager. macOS: a private file (owner-only) in Bader's
// own folder — the Keychain asks for the login password again for every item
// after every update of an app that is not signed with an Apple Developer ID,
// which made Bader unusable. Revisit when the Mac build is Developer-ID signed.

#[cfg(not(target_os = "macos"))]
mod store {
    use keyring::Entry;

    fn entry(key: &str) -> Option<Entry> {
        Entry::new(super::SERVICE, key).ok()
    }

    pub fn get(key: &str) -> Option<String> {
        entry(key)?.get_password().ok()
    }

    pub fn set(key: &str, value: &str) -> Result<(), String> {
        entry(key).ok_or("credential store unavailable")?.set_password(value).map_err(|e| e.to_string())
    }

    pub fn clear(key: &str) -> Result<(), String> {
        match entry(key).ok_or("credential store unavailable")?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "macos")]
mod store {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    fn path() -> std::path::PathBuf {
        crate::settings::local_dir().join("secrets.json")
    }

    fn load() -> BTreeMap<String, String> {
        std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(map: &BTreeMap<String, String>) -> Result<(), String> {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("tmp");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(serde_json::to_string_pretty(map).unwrap_or_default().as_bytes()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
    }

    pub fn get(key: &str) -> Option<String> {
        let _g = LOCK.lock().unwrap();
        load().get(key).cloned()
    }

    pub fn set(key: &str, value: &str) -> Result<(), String> {
        let _g = LOCK.lock().unwrap();
        let mut map = load();
        map.insert(key.to_string(), value.to_string());
        save(&map)
    }

    pub fn clear(key: &str) -> Result<(), String> {
        let _g = LOCK.lock().unwrap();
        let mut map = load();
        if map.remove(key).is_some() {
            save(&map)?;
        }
        Ok(())
    }
}

fn known(key: &str) -> bool {
    KNOWN_KEYS.contains(&key)
}

pub fn get(key: &str) -> Option<String> {
    if !known(key) {
        return None;
    }
    store::get(key).filter(|v| !v.is_empty())
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    if !known(key) {
        return Err(format!("unknown key {key}"));
    }
    if value.is_empty() {
        return store::clear(key);
    }
    store::set(key, value)
}

pub fn clear(key: &str) -> Result<(), String> {
    if !known(key) {
        return Err(format!("unknown key {key}"));
    }
    store::clear(key)
}

pub fn present(key: &str) -> bool {
    get(key).is_some()
}
