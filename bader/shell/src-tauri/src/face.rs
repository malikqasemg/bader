// Bader's live face: an ESP32-C6 screen on USB (Waveshare ESP32-C6-LCD-1.47
// running bader/face/device). The app sends one line per state change:
// "FACE thinking", "FACE happy 3", … The screen is optional — if none is
// plugged in, every call is a quiet no-op, and it is picked up when plugged in.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ESPRESSIF_VID: u16 = 0x303A;

pub struct Face {
    tx: Mutex<Sender<String>>,
}

impl Face {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || run(rx));
        Face { tx: Mutex::new(tx) }
    }

    /// Shows `name`; with `seconds`, the screen returns to "neutral" by itself.
    pub fn set(&self, name: &str, seconds: Option<f32>) {
        let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(24).collect();
        if safe.is_empty() {
            return;
        }
        let line = match seconds {
            Some(s) if s > 0.0 => format!("FACE {safe} {s:.1}"),
            _ => format!("FACE {safe}"),
        };
        let _ = self.tx.lock().unwrap().send(line);
    }
}

type Port = Box<dyn serialport::SerialPort>;

/// Finds and opens the Bader face (an Espressif USB device answering PING).
fn connect() -> Option<Port> {
    let ports = serialport::available_ports().ok()?;
    for p in ports {
        let espressif = matches!(&p.port_type, serialport::SerialPortType::UsbPort(u) if u.vid == ESPRESSIF_VID);
        if !espressif {
            continue;
        }
        // macOS lists each device twice (tty + cu); the cu one does not wait for carrier.
        if cfg!(target_os = "macos") && p.port_name.contains("/tty.") {
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
                crate::log::line(format!("face screen connected on {}", p.port_name));
                return Some(port);
            }
        }
    }
    None
}

fn run(rx: Receiver<String>) {
    let mut port: Option<Port> = None;
    let mut last: Option<String> = None;
    let mut next_try = Instant::now();
    loop {
        // Wait for the next state, but wake up now and then to find a screen.
        let msg = rx.recv_timeout(Duration::from_secs(3));
        if let Ok(line) = &msg {
            last = Some(line.clone());
        }
        if matches!(msg, Err(mpsc::RecvTimeoutError::Disconnected)) {
            return;
        }
        if port.is_none() && Instant::now() >= next_try {
            port = connect();
            next_try = Instant::now() + Duration::from_secs(5);
            if port.is_none() {
                continue;
            }
            // A newly found screen shows whatever the app is doing now.
            if msg.is_err() {
                if let (Some(p), Some(l)) = (port.as_mut(), &last) {
                    let _ = p.write_all(format!("{l}\n").as_bytes());
                }
                continue;
            }
        }
        if let (Ok(line), Some(p)) = (&msg, port.as_mut()) {
            let _ = p.clear(serialport::ClearBuffer::Input);
            if p.write_all(format!("{line}\n").as_bytes()).and_then(|_| p.flush()).is_err() {
                crate::log::line("face screen disconnected");
                port = None;
            }
        }
    }
}
