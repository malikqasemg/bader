// Bader engine client. The island's chat goes to the Hermes engine that runs
// hidden beside the shell (OpenAI-compatible API server, default
// http://127.0.0.1:8642/v1). The engine owns tools, memory, skills and the
// Telegram / WhatsApp channels; the shell only sends the user's turn.
//
// Adapted from coucou's Claude client (MIT). The engine key never leaves the
// Credential Manager, and file bytes never cross the IPC boundary.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::secrets;

/// Engine endpoint; override with BADER_ENGINE_URL (e.g. a server install).
const DEFAULT_ENGINE_URL: &str = "http://127.0.0.1:8642/v1";
/// Text and code files are inlined; anything larger is skipped.
const MAX_INLINE_TEXT: u64 = 200_000;

/// The engine advertises itself as "hermes-agent" unless a profile name is set.
pub const DEFAULT_MODEL: &str = "hermes-agent";

const SYSTEM_PROMPT: &str = "You are Bader, a personal AI assistant. \
Reply in the language the user writes in (Arabic or English). Be short and direct: answer first, detail on request. \
If the user asks you to do something, do it. If an action is your own idea, ask before doing it.";

#[derive(Default)]
pub struct Chat {
    /// Full multi-turn history, including tool_use / tool_result blocks.
    messages: Mutex<Vec<Value>>,
}

impl Chat {
    pub fn reset(&self) {
        self.messages.lock().unwrap().clear();
    }

    fn is_empty(&self) -> bool {
        self.messages.lock().unwrap().is_empty()
    }

    fn push(&self, message: Value) {
        self.messages.lock().unwrap().push(message);
    }

    fn pop(&self) {
        self.messages.lock().unwrap().pop();
    }

    fn snapshot(&self) -> Vec<Value> {
        self.messages.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatContext {
    File { name: String, path: String },
    Window { app_name: String, title: String, url: Option<String> },
    /// A screenshot of the user's screen, attached to this one question.
    Screen { path: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReply {
    pub text: String,
}

/// One chat turn. Returns the assistant's text, or a message the island shows
/// in the note view.
pub async fn send(
    chat: &Chat,
    model: &str,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let key = secrets::get("bader-engine-key").unwrap_or_default();

    let mut content: Vec<Value> = Vec::new();
    // A screenshot rides along with the question it was taken for.
    if let Some(ChatContext::Screen { path }) = &context {
        if let Some(block) = file_block(path) {
            content.push(block);
        }
        content.push(json!({ "type": "text", "text": "This is a screenshot of my screen right now." }));
    }
    // File / window context rides along with the first message only.
    if chat.is_empty() {
        match &context {
            Some(ChatContext::File { name, path }) => {
                if let Some(block) = file_block(path) {
                    content.push(block);
                }
                content.push(json!({ "type": "text", "text": format!("File: {name}") }));
            }
            Some(ChatContext::Window { app_name, title, url }) => {
                let mut text = format!("Context — App: {app_name}, Window: {title}");
                if let Some(url) = url {
                    text.push_str(&format!(", URL: {url}"));
                }
                content.push(json!({ "type": "text", "text": text }));
            }
            Some(ChatContext::Screen { .. }) | None => {}
        }
    }
    content.push(json!({ "type": "text", "text": query }));
    chat.push(json!({ "role": "user", "content": content }));

    let mut messages = vec![json!({ "role": "system", "content": SYSTEM_PROMPT })];
    messages.extend(chat.snapshot());
    let model = if model.trim().is_empty() || model.starts_with("claude-") { DEFAULT_MODEL } else { model };
    let body = json!({ "model": model, "messages": messages, "stream": false });

    let response = match call(&key, &body).await {
        Ok(v) => v,
        Err(err) => {
            chat.pop(); // keep the history consistent with what the engine saw
            return Err(err);
        }
    };

    let text = response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    if text.is_empty() {
        chat.pop();
        return Err("No response from the Bader engine.".into());
    }
    chat.push(json!({ "role": "assistant", "content": text.clone() }));
    Ok(ChatReply { text })
}

fn engine_url() -> String {
    std::env::var("BADER_ENGINE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ENGINE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

async fn call(key: &str, body: &Value) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let mut request = client
        .post(format!("{}/chat/completions", engine_url()))
        .header("content-type", "application/json")
        .json(body);
    if !key.is_empty() {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "Bader engine is not running. Start it from the tray.".to_string())?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.chars().take(200).collect());
        return Err(format!("Bader engine {status}: {detail}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("Bad engine response: {e}"))
}

/// Image → image_url block (data URI), text/code → inline text. PDFs are left
/// to the engine's own file tools.
fn file_block(path: &str) -> Option<Value> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let image = match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    };
    if let Some(media) = image {
        let bytes = std::fs::read(path).ok()?;
        return Some(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{media};base64,{}", base64(&bytes)) },
        }));
    }
    if ext == "pdf" {
        return Some(json!({ "type": "text", "text": format!("PDF file at: {path}") }));
    }

    let len = std::fs::metadata(path).ok()?.len();
    if len > MAX_INLINE_TEXT {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    Some(json!({ "type": "text", "text": format!("File contents:\n{text}") }))
}

/// Small standalone base64 encoder — not worth another dependency.
/// Also used for Stripe's basic auth.
pub(crate) fn base64_for(bytes: &[u8]) -> String {
    base64(bytes)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
