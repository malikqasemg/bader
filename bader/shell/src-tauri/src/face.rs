// Bader's live face: a small ESP32 screen on USB. Two kinds are supported:
//
//   v2  Waveshare ESP32-C6-LCD-1.47 (172×320, one button)      bader/face/device
//   v3  2.8" ESP32-32E touch display (240×320, touch, speaker)  bader/face/device_e28
//
// USB serial, 115200, line based. `PING` → `PONG bader-face <ver> [<w> <h> touch]`.
//
//   app → screen   FACE <name> [seconds]            show a face (then back to idle)
//                  STRIP <y> <h> [idle] + raw       v2: full-width RGB565 strip
//                  IMG <x> <y> <w> <h> <n> + n      v3: RLE picture of a region
//                  LED <r> <g> <b> [pulse] · POSES on|off · BEEP · BL · CAL
//   screen → app   BTN short|long · TOUCH <x> <y> · SWIPE <dir> · IDLE · READY
//                  v3 answers every command with OK / ERR (one command at a time:
//                  a UART has no flow control).
//
// With both plugged in, the touch screen wins. The BOOT button answers a pending
// approval (short = approve, long = deny); otherwise it goes to the island.
// The screen is optional: with none plugged in every call is a quiet no-op.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Espressif native USB, CH340 and CP210x USB-serial chips.
const VIDS: [u16; 3] = [0x303A, 0x1A86, 0x10C4];
const ESPRESSIF_VID: u16 = 0x303A;
/// The v3 board's buffers: compressed bytes per IMG, pixels (×2 bytes) per IMG.
const MAX_IN: usize = 4000;
const MAX_OUT: usize = 240 * 16 * 2;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub w: u16,
    pub h: u16,
    pub touch: bool,
    /// 2 = strips and a button; 3 = regions, touch, acknowledged commands.
    pub proto: u8,
}

static INFO: Mutex<Option<Info>> = Mutex::new(None);

pub fn info() -> Option<Info> {
    *INFO.lock().unwrap()
}

enum Msg {
    Line(String),
    Strip { y: u16, h: u16, idle: bool, data: Vec<u8> },
    Img { x: u16, y: u16, w: u16, h: u16, data: Vec<u8> },
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

    /// v2: RGB565 big-endian strip, full width.
    pub fn strip(&self, y: u16, h: u16, idle: bool, data: Vec<u8>) -> Result<(), String> {
        let w = info().map(|i| i.w as usize).unwrap_or(172);
        if data.len() != w * h as usize * 2 || y as usize + h as usize > 320 {
            return Err("strip size does not match".into());
        }
        self.send(Msg::Strip { y, h, idle, data });
        Ok(())
    }

    /// v3: RGB565 big-endian picture of a region.
    pub fn img(&self, x: u16, y: u16, w: u16, h: u16, data: Vec<u8>) -> Result<(), String> {
        if data.len() != w as usize * h as usize * 2 || w == 0 || h == 0 {
            return Err("picture size does not match".into());
        }
        self.send(Msg::Img { x, y, w, h, data });
        Ok(())
    }

    /// A few harmless device settings the island may send as they are.
    pub fn command(&self, line: &str) -> Result<(), String> {
        let l = line.trim();
        let ok = ["POSES on", "POSES off", "CAL"].contains(&l)
            || ["BEEP ", "BL "].iter().any(|p| {
                l.strip_prefix(p).is_some_and(|rest| rest.split(' ').all(|n| n.parse::<u16>().is_ok()))
            });
        if !ok {
            return Err("command not allowed".into());
        }
        self.send(Msg::Line(l.to_string()));
        Ok(())
    }
}

type Port = Box<dyn serialport::SerialPort>;

/// PackBits on 16-bit pixels: c < 128 → c+1 literal pixels; c ≥ 128 → next pixel × (c-126).
pub(crate) fn rle16(px: &[u8]) -> Vec<u8> {
    let n = px.len() / 2;
    let at = |i: usize| (px[i * 2], px[i * 2 + 1]);
    let mut out = Vec::with_capacity(px.len() / 4);
    let mut i = 0;
    while i < n {
        let mut run = 1;
        while i + run < n && run < 129 && at(i + run) == at(i) {
            run += 1;
        }
        if run >= 2 {
            out.push((run + 126) as u8);
            out.extend_from_slice(&px[i * 2..i * 2 + 2]);
            i += run;
            continue;
        }
        let start = i;
        i += 1;
        while i < n && i - start < 128 && !(i + 1 < n && at(i + 1) == at(i)) {
            i += 1;
        }
        out.push((i - start - 1) as u8);
        out.extend_from_slice(&px[start * 2..i * 2]);
    }
    out
}

fn parse_pong(line: &str) -> Option<Info> {
    let rest = line.trim().split("bader-face").nth(1)?;
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let major = parts.first()?.split('.').next()?.parse::<u8>().ok()?;
    if major >= 3 && parts.len() >= 3 {
        Some(Info {
            w: parts[1].parse().ok()?,
            h: parts[2].parse().ok()?,
            touch: parts.get(3) == Some(&"touch"),
            proto: 3,
        })
    } else {
        Some(Info { w: 172, h: 320, touch: false, proto: 2 })
    }
}

/// Opens `name` and asks who is there.
fn probe(name: &str, uart: bool) -> Option<(Port, Info)> {
    let mut port = serialport::new(name, 115_200).timeout(Duration::from_millis(700)).open().ok()?;
    if uart {
        // USB-serial chips reset the board when the port opens; release both
        // lines so it runs, then give it time to start.
        let _ = port.write_data_terminal_ready(false);
        let _ = port.write_request_to_send(false);
        std::thread::sleep(Duration::from_millis(2600));
    } else {
        let _ = port.write_data_terminal_ready(true);
    }
    let _ = port.clear(serialport::ClearBuffer::Input);
    let clone = port.try_clone().ok()?;
    let mut reader = BufReader::new(clone);
    for _ in 0..(if uart { 3 } else { 1 }) {
        if port.write_all(b"\nPING\n").is_err() {
            return None;
        }
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut line = String::new();
        while Instant::now() < deadline {
            line.clear();
            if reader.read_line(&mut line).is_ok() && line.contains("bader-face") {
                if let Some(info) = parse_pong(&line) {
                    crate::log::line(format!("face screen connected on {name} ({})", line.trim()));
                    return Some((port, info));
                }
            }
        }
    }
    None
}

/// Finds the Bader face; with two plugged in, the touch screen wins.
/// `rejected` remembers serial devices that are not ours, so they are left alone.
fn connect(rejected: &mut HashSet<String>) -> Option<(Port, Info)> {
    let ports = serialport::available_ports().ok()?;
    let present: HashSet<&str> = ports.iter().map(|p| p.port_name.as_str()).collect();
    rejected.retain(|n| present.contains(n.as_str()));
    let mut best: Option<(Port, Info)> = None;
    for p in &ports {
        let serialport::SerialPortType::UsbPort(u) = &p.port_type else { continue };
        if !VIDS.contains(&u.vid)
            || rejected.contains(&p.port_name)
            || (cfg!(target_os = "macos") && p.port_name.contains("/tty."))
        {
            continue;
        }
        match probe(&p.port_name, u.vid != ESPRESSIF_VID) {
            Some(found) => {
                if best.as_ref().is_none_or(|(_, b)| found.1.touch && !b.touch) {
                    best = Some(found);
                }
            }
            None => {
                rejected.insert(p.port_name.clone());
            }
        }
    }
    best
}

/// Reads what the screen says until the port goes away.
fn spawn_reader(app: AppHandle, port: &Port, acks: Sender<bool>) {
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
                    if l == "OK" || l.starts_with("PONG") {
                        let _ = acks.send(true);
                    } else if l.starts_with("ERR") {
                        crate::log::line(format!("face screen: {l}"));
                        let _ = acks.send(false);
                    } else if let Some(kind) = l.strip_prefix("BTN ") {
                        on_button(&app, kind.trim());
                    } else if let Some(rest) = l.strip_prefix("TOUCH ") {
                        let mut it = rest.split_whitespace().filter_map(|n| n.parse::<u16>().ok());
                        if let (Some(x), Some(y)) = (it.next(), it.next()) {
                            let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-touch", (x, y));
                        }
                    } else if let Some(dir) = l.strip_prefix("SWIPE ") {
                        let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-swipe", dir.trim().to_string());
                    } else if l == "IDLE" {
                        let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-idle", ());
                    } else if l == "READY" {
                        let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-ready", info());
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

struct Link {
    port: Port,
    info: Info,
    acks: Receiver<bool>,
}

impl Link {
    /// v3: one command at a time, each one acknowledged.
    fn acked(&mut self, head: &str, payload: &[u8]) -> std::io::Result<bool> {
        while self.acks.try_recv().is_ok() {}
        self.port.write_all(head.as_bytes())?;
        self.port.write_all(b"\n")?;
        if !payload.is_empty() {
            self.port.write_all(payload)?;
        }
        self.port.flush()?;
        match self.acks.recv_timeout(Duration::from_millis(3000)) {
            Ok(ok) => Ok(ok),
            // The reader stopped: the screen was unplugged.
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(std::io::ErrorKind::BrokenPipe.into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // A byte was lost and the board is still waiting for the rest of
                // a picture: fill it up so it listens again.
                crate::log::line(format!("face screen: no answer to {}", head.split(' ').next().unwrap_or("")));
                self.port.write_all(&vec![b'\n'; MAX_IN + 16])?;
                self.port.flush()?;
                std::thread::sleep(Duration::from_millis(400));
                Ok(false)
            }
        }
    }

    fn write(&mut self, m: &Msg) -> std::io::Result<bool> {
        match m {
            Msg::Line(l) if self.info.proto >= 3 => self.acked(l, &[]),
            Msg::Line(l) => {
                if l.starts_with("POSES") || l.starts_with("BEEP") || l == "CAL" {
                    return Ok(true); // v3 only
                }
                self.port.write_all(format!("{l}\n").as_bytes())?;
                self.port.flush()?;
                Ok(true)
            }
            Msg::Strip { y, h, idle, data } => {
                if self.info.proto >= 3 {
                    return Ok(true);
                }
                self.port.write_all(format!("STRIP {y} {h}{}\n", if *idle { " idle" } else { "" }).as_bytes())?;
                for chunk in data.chunks(4096) {
                    self.port.write_all(chunk)?;
                }
                self.port.flush()?;
                Ok(true)
            }
            Msg::Img { x, y, w, h, data } => {
                if self.info.proto < 3 || x + w > self.info.w || y + h > self.info.h {
                    return Ok(true);
                }
                let row = *w as usize * 2;
                let max_rows = (MAX_OUT / row).max(1);
                let mut top = 0usize;
                let mut all_ok = true;
                while top < *h as usize {
                    let mut rows = max_rows.min(*h as usize - top);
                    let mut enc = rle16(&data[top * row..(top + rows) * row]);
                    while enc.len() > MAX_IN && rows > 1 {
                        rows /= 2;
                        enc = rle16(&data[top * row..(top + rows) * row]);
                    }
                    let head = format!("IMG {x} {} {w} {rows} {}", *y as usize + top, enc.len());
                    all_ok &= self.acked(&head, &enc)?;
                    top += rows;
                }
                Ok(all_ok)
            }
        }
    }
}

/// Drops pictures and faces that a newer message in the queue replaces.
fn coalesce(queue: Vec<Msg>) -> Vec<Msg> {
    let mut keep = vec![true; queue.len()];
    for i in 0..queue.len() {
        for later in &queue[i + 1..] {
            let replaced = match (&queue[i], later) {
                (Msg::Img { x, y, w, h, .. }, Msg::Img { x: x2, y: y2, w: w2, h: h2, .. }) => {
                    (x, y, w, h) == (x2, y2, w2, h2)
                }
                (Msg::Strip { y, idle, .. }, Msg::Strip { y: y2, idle: i2, .. }) => y == y2 && idle == i2,
                (Msg::Line(a), Msg::Line(b)) => a.starts_with("LED ") && b.starts_with("LED "),
                _ => false,
            };
            if replaced {
                keep[i] = false;
                break;
            }
        }
    }
    queue.into_iter().zip(keep).filter_map(|(m, k)| k.then_some(m)).collect()
}

fn run(app: AppHandle, rx: Receiver<Msg>) {
    let mut link: Option<Link> = None;
    let mut rejected: HashSet<String> = HashSet::new();
    // v2 only — replayed to a screen plugged in later: last face, last idle strip.
    let mut last_face: Option<String> = None;
    let mut last_idle: Option<(u16, u16, Vec<u8>)> = None;
    let mut next_try = Instant::now();
    loop {
        let msg = rx.recv_timeout(Duration::from_secs(3));
        if matches!(msg, Err(mpsc::RecvTimeoutError::Disconnected)) {
            return;
        }
        if link.is_none() && Instant::now() >= next_try {
            if let Some((port, info)) = connect(&mut rejected) {
                let (ack_tx, ack_rx) = mpsc::channel();
                spawn_reader(app.clone(), &port, ack_tx);
                let mut l = Link { port, info, acks: ack_rx };
                *INFO.lock().unwrap() = Some(info);
                if info.proto < 3 {
                    if let Some((y, h, data)) = &last_idle {
                        let _ = l.write(&Msg::Strip { y: *y, h: *h, idle: true, data: data.clone() });
                    }
                    if let Some(f) = &last_face {
                        let _ = l.write(&Msg::Line(f.clone()));
                    }
                }
                link = Some(l);
                // The island redraws everything for the screen that just arrived.
                let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-ready", Some(info));
            }
            next_try = Instant::now() + Duration::from_secs(5);
        }
        let Ok(first) = msg else { continue };
        let mut queue = vec![first];
        while let Ok(more) = rx.try_recv() {
            queue.push(more);
        }
        for m in coalesce(queue) {
            match &m {
                Msg::Line(l) if l.starts_with("FACE ") => last_face = Some(l.clone()),
                Msg::Strip { idle: true, y, h, data } => last_idle = Some((*y, *h, data.clone())),
                _ => {}
            }
            if let Some(l) = link.as_mut() {
                if l.write(&m).is_err() {
                    crate::log::line("face screen disconnected");
                    link = None;
                    *INFO.lock().unwrap() = None;
                    let _ = app.emit_to(crate::island::WINDOW_LABEL, "face-ready", None::<Info>);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The board's decoder, in Rust.
    fn unrle(src: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < src.len() {
            let c = src[i] as usize;
            i += 1;
            if c < 128 {
                out.extend_from_slice(&src[i..i + (c + 1) * 2]);
                i += (c + 1) * 2;
            } else {
                for _ in 0..c - 126 {
                    out.extend_from_slice(&src[i..i + 2]);
                }
                i += 2;
            }
        }
        out
    }

    #[test]
    fn rle_round_trips() {
        let mut px = Vec::new();
        for i in 0..2000u32 {
            let v = if i % 97 < 60 { 0u16 } else { (i * 7919 % 65536) as u16 };
            px.extend_from_slice(&v.to_be_bytes());
        }
        px.extend(std::iter::repeat_n([0xAB, 0xCD], 700).flatten());
        px.extend_from_slice(&[1, 2]);
        let enc = rle16(&px);
        assert!(enc.len() < px.len());
        assert_eq!(unrle(&enc), px);
        assert_eq!(unrle(&rle16(&[9, 9])), vec![9, 9]);
    }

    #[test]
    fn pong_is_parsed() {
        assert_eq!(parse_pong("PONG bader-face 2.0"), Some(Info { w: 172, h: 320, touch: false, proto: 2 }));
        assert_eq!(
            parse_pong("PONG bader-face 3.0 240 320 touch\r\n"),
            Some(Info { w: 240, h: 320, touch: true, proto: 3 })
        );
    }
}
