// Bader's live face: an ESP32-C6 screen on USB (Waveshare ESP32-C6-LCD-1.47
// running bader/face/device). Talks USB serial, 115200, line based:
//
//   app → screen   FACE <name> [seconds]      show a face (then back to idle)
//                  STRIP <y> <h> [idle]       + 172*h*2 bytes RGB565: draw a strip
//                                              (idle = keep it and redraw on every idle pose)
//                  LED <r> <g> <b> [pulse]    the RGB light
//                  PING                       → PONG bader-face <version>
//   screen → app   BTN short | BTN long       the BOOT button
//
// The button answers a pending approval (short = approve, long = deny);
// otherwise it is passed to the island (short press = talk to Bader).
// The screen is optional: with none plugged in every call is a quiet no-op.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

const ESPRESSIF_VID: u16 = 0x303A;
pub const W: usize = 172;

enum Msg {
    Line(String),
    Strip { y: u16, h: u16, idle: bool, data: Vec<u8> },
}

pub struct Face {
    tx: Mutex<Sender<Msg>>,
}

impl Face {
    pub fn start(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        std::thread::spawn(move || run(app, rx));
        Face { tx: Mutex::new(tx) }
    }

    fn send(&self, m: Msg) {
        let _ = self.tx.lock().unwrap().send(m);
    }

    /// Shows `name`; with `seconds`, the screen returns to idle by itself.
    pub fn set(&self, name: &str, seconds: Option<f32>) {
        let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(24).collect();
        if safe.is_empty() {
            return;
        }
        self.send(Msg::Line(match seconds {
            Some(s) if s > 0.0 => format!("FACE {safe} {s:.1}"),
            _ => format!("FACE {safe}"),
        }));
    }

    pub fn led(&self, r: u8, g: u8, b: u8, pulse: bool) {
        self.send(Msg::Line(format!("LED {r} {g} {b}{}", if pulse { " pulse" } else { "" })));
    }

    /// RGB565 big-endian strip, full width.
    pub fn strip(&self, y: u16, h: u16, idle: bool, data: Vec<u8>) -> Result<(), String> {
        if data.len() != W * h as usize * 2 || y as usize + h as usize > 320 {
            return Err("strip size does not match".into());
        }
        self.send(Msg::Strip { y, h, idle, data });
        Ok(())
    }
}

type Port = Box<dyn serialport::SerialPort>;

/// Finds and opens the Bader face (an Espressif USB device answering PING).
fn connect() -> Option<Port> {
    let ports = serialport::available_ports().ok()?;
    for p in ports {
        let espressif = matches!(&p.port_type, serialport::SerialPortType::UsbPort(u) if u.vid == ESPRESSIF_VID);
        if !espressif || (cfg!(target_os = "macos") && p.port_name.contains("/tty.")) {
            continue;
        }
        let Ok(mut port) = serialport::new(&p.port_name, 115_200)
            .timeout(Duration::from_millis(800))
            .open()
        else {
            continue;
        };
        let _ = port.write_data_terminal_ready(true);
        let _ = port.clear(serialport::ClearBuffer::Input);
        if port.write_all(b"\nPING\n").is_err() {
            continue;
        }
        let Ok(clone) = port.try_clone() else { continue };
        let mut reader = BufReader::new(clone);
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut line = String::new();
        while Instant::now() < deadline {
            line.clear();
            if reader.read_line(&mut line).is_ok() && line.contains("bader-face") {
                crate::log::line(format!("face screen connected on {} ({})", p.port_name, line.trim()));
                return Some(port);
            }
        }
    }
    None
}

/// Reads button presses from the screen until the port goes away.
fn spawn_reader(app: AppHandle, port: &Port) {
    let Ok(mut clone) = port.try_clone() else { return };
    let _ = clone.set_timeout(Duration::from_secs(3600));
    std::thread::spawn(move || {
        let mut reader = BufReader::new(clone);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {
                    let l = line.trim();
                    if let Some(kind) = l.strip_prefix("BTN ") {
                        on_button(&app, kind.trim());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => return,
            }
        }
    });
}

fn on_button(app: &AppHandle, kind: &str) {
    crate::log::line(format!("face button: {kind}"));
    if crate::runs::has_pending() {
        let choice = if kind == "long" { "deny" } else { "once" };
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let key = crate::secrets::get("bader-engine-key").unwrap_or_default();
            let _ = crate::runs::answer(&app2, &crate::claude::engine_url(), &key, choice).await;
        });
        return;
    }
    let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-button", kind.to_string());
}

fn write_msg(p: &mut Port, m: &Msg) -> std::io::Result<()> {
    match m {
        Msg::Line(l) => p.write_all(format!("{l}\n").as_bytes())?,
        Msg::Strip { y, h, idle, data } => {
            p.write_all(format!("STRIP {y} {h}{}\n", if *idle { " idle" } else { "" }).as_bytes())?;
            for chunk in data.chunks(4096) {
                p.write_all(chunk)?;
            }
        }
    }
    p.flush()
}

fn run(app: AppHandle, rx: Receiver<Msg>) {
    let mut port: Option<Port> = None;
    // Replayed to a screen that is plugged in later: last face, last idle strip.
    let mut last_face: Option<String> = None;
    let mut last_idle: Option<(u16, u16, Vec<u8>)> = None;
    let mut next_try = Instant::now();
    loop {
        let msg = rx.recv_timeout(Duration::from_secs(3));
        if matches!(msg, Err(mpsc::RecvTimeoutError::Disconnected)) {
            return;
        }
        if port.is_none() && Instant::now() >= next_try {
            port = connect();
            next_try = Instant::now() + Duration::from_secs(5);
            if let Some(p) = port.as_mut() {
                spawn_reader(app.clone(), p);
                if let Some((y, h, data)) = &last_idle {
                    let _ = write_msg(p, &Msg::Strip { y: *y, h: *h, idle: true, data: data.clone() });
                }
                if let Some(f) = &last_face {
                    let _ = write_msg(p, &Msg::Line(f.clone()));
                }
            }
        }
        let Ok(m) = msg else { continue };
        match &m {
            Msg::Line(l) if l.starts_with("FACE ") => last_face = Some(l.clone()),
            Msg::Strip { idle: true, y, h, data } => last_idle = Some((*y, *h, data.clone())),
            _ => {}
        }
        if let Some(p) = port.as_mut() {
            if write_msg(p, &m).is_err() {
                crate::log::line("face screen disconnected");
                port = None;
            }
        }
    }
}
