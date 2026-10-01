// macOS / non-Windows stand-in for pipe.rs. The Windows build relays Claude
// Code permission requests over a named pipe; Bader's own approvals will come
// from the engine instead, so here the relay is simply absent.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::AppHandle;
use tokio::sync::mpsc;

/// What the island can say about a permission request (unused off Windows).
#[allow(dead_code)]
pub enum Reply {
    Ack,
    Decision(String),
    Decline,
}

/// Permission requests the island has been told about.
#[derive(Default)]
#[allow(dead_code)]
pub struct Pending(pub Mutex<HashMap<String, mpsc::Sender<Reply>>>);

#[allow(dead_code)]
pub fn pipe_name() -> String {
    String::new()
}

pub fn start(_app: AppHandle) {}

pub fn acknowledge(_app: &AppHandle, _request_id: &str) {}

pub fn decline(_app: &AppHandle, _request_id: &str) {}

pub fn answer(_app: &AppHandle, _request_id: &str, _decision: &str) {}
