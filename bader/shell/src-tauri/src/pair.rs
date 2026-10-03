//! Pairing a phone with this Bader.
//!
//! The phone app is a Bader of its own: it talks to the AI service and to the
//! user's mail and calendar directly. To start, it needs the same keys this
//! computer already holds. They are handed over once, as a QR code the phone's
//! camera reads off this screen - nothing is sent over the network.

use serde_json::{json, Value};

fn env_value(key: &str) -> Option<String> {
    let env = std::fs::read_to_string(crate::engine::home().join(".env")).ok()?;
    env.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .find(|(k, v)| k.trim() == key && !v.trim().is_empty())
        .map(|(_, v)| v.trim().trim_matches('"').trim_matches('\'').to_string())
}

/// What the phone needs. Short keys keep the QR code easy to scan.
pub fn payload() -> Result<Value, String> {
    let ai = env_value("OPENROUTER_API_KEY").ok_or("Add your OpenRouter key in Settings first.")?;
    let mut out = json!({
        "bader": 1,
        "ai": ai,
        "l1": crate::engine::pref_get("primary_lang").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "en".into()),
        "l2": crate::engine::pref_get("second_lang").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "ar".into()),
    });
    let token: Option<Value> = std::fs::read_to_string(crate::engine::home().join("google_token.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    if let Some(t) = token {
        let pick = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        if !pick("refresh_token").is_empty() {
            out["g"] = json!({ "id": pick("client_id"), "s": pick("client_secret"), "r": pick("refresh_token"), "a": pick("account") });
        }
    }
    Ok(out)
}

/// The pairing code as an SVG picture.
pub fn code_svg() -> Result<String, String> {
    use qrcode::{render::svg, EcLevel, QrCode};
    let text = payload()?.to_string();
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L).map_err(|e| e.to_string())?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(300, 300)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_full_payload_fits_in_one_code() {
        // Longest realistic payload: ~75-char AI key, Google ids and a refresh token.
        let text = serde_json::json!({"bader":1,"ai":"k".repeat(80),"l1":"en","l2":"ar",
            "g":{"id":"i".repeat(75),"s":"s".repeat(40),"r":"r".repeat(140),"a":"someone@example.com"}})
        .to_string();
        assert!(qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::L).is_ok());
    }
}
