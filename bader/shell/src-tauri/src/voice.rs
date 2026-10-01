// Bader voice in the island: record from the microphone, turn speech into text
// with the engine's speech-to-text (Arabic or English, detected per message),
// and turn replies into speech with the engine's voice.
//
// Recording is native (cpal) so it works the same on macOS and Windows; the
// engine does the speech work through bader_voice.py, run with the engine's
// own Python so the Voice settings apply here too.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

/// Longest recording kept, in seconds (matches the engine's voice limit).
const MAX_SECONDS: usize = 120;

#[derive(Default)]
pub struct Recorder {
    inner: Mutex<Option<Active>>,
}

struct Active {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<Result<(Vec<f32>, u32), String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    pub text: String,
    pub language: String,
}

/// Starts recording from the default microphone.
pub fn start(rec: &Recorder) -> Result<(), String> {
    let mut slot = rec.inner.lock().unwrap();
    if slot.is_some() {
        return Ok(());
    }
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let thread = std::thread::spawn(move || -> Result<(Vec<f32>, u32), String> {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                let _ = ready_tx.send(Err("No microphone found.".into()));
                return Err("No microphone found.".into());
            }
        };
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Microphone not available: {e}");
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(msg);
            }
        };
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = rate as usize * MAX_SECONDS;
        let err_fn = |e| crate::log::line(format!("mic stream error: {e}"));

        let sink = samples.clone();
        let push = move |frame_iter: &mut dyn Iterator<Item = f32>| {
            let mut buf = sink.lock().unwrap();
            if buf.len() < cap {
                buf.extend(frame_iter);
            }
        };
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let push = push;
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        let mut it = data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32);
                        push(&mut it);
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let push = push;
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        let mut it = data
                            .chunks(channels)
                            .map(|f| f.iter().map(|s| *s as f32 / 32768.0).sum::<f32>() / channels as f32);
                        push(&mut it);
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let push = push;
                device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _| {
                        let mut it = data.chunks(channels).map(|f| {
                            f.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).sum::<f32>() / channels as f32
                        });
                        push(&mut it);
                    },
                    err_fn,
                    None,
                )
            }
            other => {
                let msg = format!("Unsupported microphone format: {other:?}");
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(msg);
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Could not open the microphone: {e}");
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(msg);
            }
        };
        if let Err(e) = stream.play() {
            let msg = format!("Could not start the microphone: {e}");
            let _ = ready_tx.send(Err(msg.clone()));
            return Err(msg);
        }
        let _ = ready_tx.send(Ok(()));
        let _ = stop_rx.recv();
        drop(stream);
        let data = std::mem::take(&mut *samples.lock().unwrap());
        Ok((data, rate))
    });
    match ready_rx.recv() {
        Ok(Ok(())) => {
            *slot = Some(Active { stop: stop_tx, thread });
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Microphone thread stopped.".into()),
    }
}

/// Stops recording and saves a 16-bit mono WAV. Returns its path.
fn stop_to_wav(rec: &Recorder) -> Result<PathBuf, String> {
    let active = rec.inner.lock().unwrap().take().ok_or("Not recording.")?;
    let _ = active.stop.send(());
    let (samples, rate) = active.thread.join().map_err(|_| "Recorder crashed.".to_string())??;
    if samples.len() < (rate as usize) / 3 {
        return Err("Too short — hold on a little longer.".into());
    }
    let dir = crate::settings::local_dir().join("voice");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("input.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        w.write_sample(v).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn cancel(rec: &Recorder) {
    if let Some(active) = rec.inner.lock().unwrap().take() {
        let _ = active.stop.send(());
        let _ = active.thread.join();
    }
}

/// Stops recording and returns what was said.
pub fn stop_and_transcribe(app: &AppHandle, rec: &Recorder) -> Result<Heard, String> {
    let wav = stop_to_wav(rec)?;
    let out = run_helper(app, &["stt", &wav.display().to_string()])?;
    if out.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(out
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Nothing was heard.")
            .to_string());
    }
    Ok(Heard {
        text: out.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
        language: out.get("language").and_then(Value::as_str).unwrap_or("").to_string(),
    })
}

/// Speaks `text` with Bader's voice; returns the audio as a data URL.
pub fn speak(app: &AppHandle, text: &str) -> Result<String, String> {
    let dir = crate::settings::local_dir().join("voice");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Unique names per call: several sentences are synthesised at once.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let text_path = dir.join(format!("reply-{n}.txt"));
    let out_path = dir.join(format!("reply-{n}.mp3"));
    std::fs::write(&text_path, text).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&out_path);
    let out = run_helper(
        app,
        &["tts", &text_path.display().to_string(), &out_path.display().to_string()],
    )?;
    if out.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(out.get("error").and_then(Value::as_str).unwrap_or("Voice failed.").to_string());
    }
    let file = out
        .get("file")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or(out_path);
    let bytes = std::fs::read(&file).map_err(|e| format!("No voice file: {e}"))?;
    let _ = std::fs::remove_file(&text_path);
    let _ = std::fs::remove_file(&file);
    let mime = match file.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "audio/mpeg",
    };
    Ok(format!("data:{mime};base64,{}", crate::claude::base64_for(&bytes)))
}

/// Engine folder that holds the Python environment (`hermes-agent`).
pub(crate) fn engine_root() -> PathBuf {
    if let Some(p) = std::env::var_os("BADER_ENGINE_ROOT").filter(|v| !v.is_empty()) {
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
    base.join("hermes-agent")
}

pub(crate) fn engine_python(root: &PathBuf) -> PathBuf {
    if let Some(p) = std::env::var_os("BADER_ENGINE_PYTHON").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        root.join("venv").join("Scripts").join("python.exe")
    } else {
        root.join("venv").join("bin").join("python")
    }
}

fn helper_script(app: &AppHandle, name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join(name));
        candidates.push(dir.join("resources").join(name));
    }
    // Development builds: next to the sources.
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join(name));
    candidates.into_iter().find(|p| p.is_file())
}

fn run_helper(app: &AppHandle, args: &[&str]) -> Result<Value, String> {
    // Fast path: the long-lived voice worker (speech model already loaded).
    let req = match args {
        ["stt", path] => Some(serde_json::json!({ "op": "stt", "path": path })),
        ["tts", text_path, out] => Some(serde_json::json!({ "op": "tts", "text_path": text_path, "out": out })),
        _ => None,
    };
    if let Some(req) = req {
        match worker_call(app, &req) {
            Ok(v) => return Ok(v),
            Err(e) => crate::log::line(format!("voice worker unavailable ({e}); one-shot fallback")),
        }
    }
    run_script(app, "bader_voice.py", args)
}

// ── Voice worker: one Python process that keeps the speech model loaded ──────

struct Worker {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    out: std::io::BufReader<std::process::ChildStdout>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

fn spawn_worker(app: &AppHandle) -> Result<Worker, String> {
    use std::io::BufRead;
    let root = engine_root();
    let python = engine_python(&root);
    if !python.is_file() {
        return Err("engine not installed".into());
    }
    let script = helper_script(app, "bader_voice.py").ok_or("voice helper missing")?;
    let mut cmd = Command::new(&python);
    cmd.arg(&script)
        .arg("serve")
        .current_dir(&root)
        .env("PYTHONPATH", &root)
        .env("HERMES_HOME", crate::engine::home())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let mut out = std::io::BufReader::new(child.stdout.take().ok_or("no stdout")?);
    let mut line = String::new();
    loop {
        line.clear();
        if out.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("worker exited during start-up".into());
        }
        if line.starts_with("@@") {
            break;
        }
    }
    crate::log::line("voice worker ready");
    Ok(Worker { child, stdin, out })
}

fn worker_call(app: &AppHandle, req: &Value) -> Result<Value, String> {
    use std::io::{BufRead, Write};
    let mut slot = WORKER.lock().unwrap();
    if slot.is_none() {
        *slot = Some(spawn_worker(app)?);
    }
    let w = slot.as_mut().unwrap();
    let result = (|| -> Result<Value, String> {
        writeln!(w.stdin, "{req}").map_err(|e| e.to_string())?;
        w.stdin.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        loop {
            line.clear();
            if w.out.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                return Err("worker exited".into());
            }
            if let Some(json) = line.strip_prefix("@@") {
                return serde_json::from_str(json.trim()).map_err(|e| e.to_string());
            }
        }
    })();
    if result.is_err() {
        if let Some(mut dead) = slot.take() {
            let _ = dead.child.kill();
        }
    }
    result
}

/// Starts the voice worker in the background so the first voice note is quick.
pub fn prewarm(app: AppHandle) {
    std::thread::spawn(move || {
        let mut slot = WORKER.lock().unwrap();
        if slot.is_none() {
            match spawn_worker(&app) {
                Ok(w) => *slot = Some(w),
                Err(e) => crate::log::line(format!("voice worker not started: {e}")),
            }
        }
    });
}

/// Restarts the worker (after Voice settings change).
pub fn restart_worker() {
    if let Some(mut w) = WORKER.lock().unwrap().take() {
        let _ = w.child.kill();
    }
}

/// Runs one of Bader's helper scripts with the engine's Python; returns the
/// JSON object it prints last.
pub fn run_script(app: &AppHandle, name: &str, args: &[&str]) -> Result<Value, String> {
    let root = engine_root();
    let python = engine_python(&root);
    if !python.is_file() {
        return Err("Bader engine is not installed on this computer.".into());
    }
    let script = helper_script(app, name).ok_or("Helper script missing from the app.")?;
    let mut cmd = Command::new(&python);
    cmd.arg(&script)
        .args(args)
        .current_dir(&root)
        .env("PYTHONPATH", &root)
        .env("HERMES_HOME", crate::engine::home())
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("Could not run the voice engine: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let last = stdout.lines().rev().find(|l| l.trim_start().starts_with('{')).unwrap_or("");
    serde_json::from_str(last).map_err(|_| "The voice engine gave no answer.".to_string())
}

/// Takes a screenshot of the main display for one question; returns its path.
/// macOS asks once for Screen Recording permission.
pub fn capture_screen() -> Result<String, String> {
    let dir = crate::settings::local_dir().join("voice");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("screen.jpg");
    let _ = std::fs::remove_file(&path);
    #[cfg(target_os = "macos")]
    {
        let ok = Command::new("screencapture")
            .args(["-x", "-m", "-t", "jpg"])
            .arg(&path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok || !path.is_file() {
            return Err("Could not see the screen. Allow Bader in System Settings → Privacy & Security → Screen Recording.".into());
        }
        // Keep it light for the model: longest side 1920 px.
        let _ = Command::new("sips").args(["-Z", "1920"]).arg(&path).output();
        Ok(path.display().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Looking at the screen is coming to Windows next.".into())
    }
}
