# Bader face v3 — touch screen (2.8" ESP32-32E board: ILI9341 240x320 + XPT2046).
#
# The app owns the layout: it sends pictures of screen regions (text, buttons,
# lists — so Arabic renders properly) and the board reports touches.
#
# App -> board, one line per command; every command answers OK or ERR <why>:
#   PING                      -> PONG bader-face 3.0 <w> <h> touch   (240 320 upright, 320 240 on its side)
#   FACE <name> [seconds]     picture in the face area (y 28..203); then back to idle
#   IMG <x> <y> <w> <h> <n>   + n bytes: RLE picture of a region (see unrle)
#   POSES on|off              rotate idle poses (off while a page covers the face)
#   LED <r> <g> <b> [pulse]   RGB light       BEEP <hz> <ms>    BL <percent>
#   CAL                       touch calibration (4 taps)
#   ROT 0-3|+ · INV 0|1       turn the picture by quarter turns / invert colours (saved)
#   EXIT                      stop (drops to the MicroPython prompt)
# Board -> app:
#   TOUCH <x> <y> · SWIPE left|right|up|down · BTN short|long (BOOT) · IDLE · READY · AWAY
#
# Two apps can be attached at once: the computer on the USB cable and the phone
# over Bluetooth (the same lines, on a "Nordic UART" service named Bader).
# Only one of them owns the screen. The board draws a small PC / PHONE button in
# the top bar (x 36..84); a tap on it hands the screen to the other one: the old
# owner is told AWAY (its commands are answered "ERR away" from then on) and the
# new owner is told READY and draws everything again.

import gc
import json
import math
import os
import select
import sys
import time

import framebuf
import micropython
from machine import Pin, PWM, SoftSPI

from ili9341 import Display, W, H

VERSION = "3.1"
FACES_DIR = "/faces"
FACE_Y, FACE_H = 28, 176
IDLE_ROTATE = 30_000
LONG_MS = 1000
ALIAS = {"working": "thinking", "approval": "surprised"}
MAX_IN = 4096
MAX_OUT = W * 16 * 2
LINK_X, LINK_W = 36, 48  # the board's own PC / PHONE button in the top bar

try:
    with open("/cfg.json") as f:
        cfg = json.load(f)
except (OSError, ValueError):
    cfg = {}

d = Display(cfg.get("rot", 0), cfg.get("inv", 0))
rx = bytearray(MAX_IN)
out = bytearray(MAX_OUT)
rx_mv = memoryview(rx)
out_mv = memoryview(out)
stdin = sys.stdin.buffer

available = set(f[:-4] for f in os.listdir(FACES_DIR) if f.endswith(".raw")) if "faces" in os.listdir("/") else set()
IDLE_SET = [n for n in (["idle"] + sorted(n for n in available if n.startswith("pose_"))) if n in available]
state = {"face": None, "idle": True, "revert_at": 0, "next_pose": 0, "pose_i": 0, "poses": True, "busy": False,
         "link": cfg.get("link", "usb"), "usb_seen": False, "ble_seen": False}


# ── the phone's link: Bluetooth ──────────────────────────────────────────────
class Bt:
    """The same line protocol as the cable, carried over Bluetooth LE."""

    def __init__(self):
        import bluetooth
        self.ble = bluetooth.BLE()
        self.ble.active(True)
        self.ble.config(gap_name="Bader")
        try:
            self.ble.config(mtu=247)
        except (OSError, ValueError):
            pass
        uart = bluetooth.UUID("6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
        tx = (bluetooth.UUID("6E400003-B5A3-F393-E0A9-E50E24DCCA9E"), bluetooth.FLAG_NOTIFY)
        rx_c = (bluetooth.UUID("6E400002-B5A3-F393-E0A9-E50E24DCCA9E"), bluetooth.FLAG_WRITE | bluetooth.FLAG_WRITE_NO_RESPONSE)
        ((self.tx, self.rx),) = self.ble.gatts_register_services(((uart, (tx, rx_c)),))
        # Room for one whole picture band: the phone waits for our answer before the next.
        self.ble.gatts_set_buffer(self.rx, MAX_IN + 600, True)
        self.adv = b"\x02\x01\x06\x06\x09Bader"
        self.resp = b"\x11\x07" + bytes(uart)
        self.conn = None
        self.buf = bytearray()
        self.pending = bytearray()
        self.ble.irq(self._irq)
        self.advertise()

    def advertise(self):
        try:
            self.ble.gap_advertise(200_000, adv_data=self.adv, resp_data=self.resp)
        except OSError:
            pass

    def _irq(self, event, data):
        if event == 1:  # a phone connected
            self.conn = data[0]
            self.buf = bytearray()
            self.pending = bytearray()
        elif event == 2:  # it left
            self.conn = None
            self.advertise()
        elif event == 3 and data[1] == self.rx:  # it wrote something
            self.buf += self.ble.gatts_read(self.rx)

    def fill(self):
        if self.buf:
            chunk = self.buf
            self.buf = bytearray()
            self.pending += chunk

    def send(self, msg):
        if self.conn is None:
            return
        data = msg.encode() + b"\n"
        for i in range(0, len(data), 20):
            try:
                self.ble.gatts_notify(self.conn, self.tx, data[i:i + 20])
            except OSError:
                return


try:
    bt = Bt()
except Exception as e:  # no Bluetooth in this firmware: cable only
    bt = None
if not bt:
    state["link"] = "usb"


def say(src, *args):
    """One line to the computer ("usb") or to the phone ("ble")."""
    msg = " ".join(str(a) for a in args)
    if src == "ble":
        if bt:
            bt.send(msg)
    else:
        print(msg)


def event(*args):
    """Touches and the like go to whoever owns the screen."""
    say(state["link"], *args)

# ── light, sound, button ─────────────────────────────────────────────────────
leds = [PWM(Pin(p), freq=1000, duty=1023) for p in (22, 16, 17)]  # active low
led_state = {"rgb": (0, 0, 0), "pulse": False}
amp = Pin(4, Pin.OUT, value=1)  # low = speaker amplifier on
button = Pin(0, Pin.IN, Pin.PULL_UP)


def set_led(rgb, level=1.0):
    for pwm, c in zip(leds, rgb):
        pwm.duty(1023 - int(c * level * 4))


def beep(hz=1800, ms=25):
    try:
        amp(0)
        p = PWM(Pin(26), freq=hz, duty=300)
        time.sleep_ms(ms)
        p.deinit()
    finally:
        amp(1)


# ── touch ────────────────────────────────────────────────────────────────────
class Touch:
    def __init__(self):
        self.spi = SoftSPI(baudrate=1_000_000, sck=Pin(25), mosi=Pin(32), miso=Pin(39))
        self.cs = Pin(33, Pin.OUT, value=1)
        self.irq = Pin(36, Pin.IN)
        self.b = bytearray(3)
        self.cx = b"\xD0\x00\x00"
        self.cy = b"\x90\x00\x00"

    def _read(self, cmd):
        self.cs(0)
        self.spi.write_readinto(cmd, self.b)
        self.cs(1)
        return ((self.b[1] << 8) | self.b[2]) >> 3

    def raw(self):
        if self.irq.value():
            return None
        xs, ys = [], []
        for _ in range(4):
            # The first reading after switching axis is noisy: read twice, keep the second.
            self._read(self.cx)
            xs.append(self._read(self.cx))
            self._read(self.cy)
            ys.append(self._read(self.cy))
        if self.irq.value():
            return None
        xs.sort()
        ys.sort()
        x, y = (xs[1] + xs[2]) // 2, (ys[1] + ys[2]) // 2
        if x < 60 or x > 4040 or y < 60 or y > 4040 or xs[2] - xs[1] > 250 or ys[2] - ys[1] > 250:
            return None
        return x, y

    def point(self):
        r = self.raw()
        if not r:
            return None
        c = cfg.get("cal") or {"swap": 0, "ax": -0.0667, "bx": 255, "ay": 0.0889, "by": -18}
        u, v = (r[1], r[0]) if c["swap"] else r
        # Calibrated for the upright picture; turn the point with the picture.
        px = max(0, min(W - 1, int(c["ax"] * u + c["bx"])))
        py = max(0, min(H - 1, int(c["ay"] * v + c["by"])))
        rot = d.rot
        if rot == 1:
            return py, W - 1 - px
        if rot == 2:
            return W - 1 - px, H - 1 - py
        if rot == 3:
            return H - 1 - py, px
        return px, py


touch = Touch()


def save_cfg():
    with open("/cfg.json", "w") as f:
        json.dump(cfg, f)


# ── drawing helpers ──────────────────────────────────────────────────────────
def text(msg, x, y, color=0xFFFF, bg=0):
    """Small built-in font (set-up screens only; the app draws the real text)."""
    w = min(len(msg) * 8, d.w)
    n = w * 8 * 2
    fb = framebuf.FrameBuffer(out_mv[:n], w, 8, framebuf.RGB565)
    fb.fill(((bg & 0xFF) << 8) | (bg >> 8))
    fb.text(msg, 0, 0, ((color & 0xFF) << 8) | (color >> 8))
    d.blit(x, y, w, 8, out_mv[:n])


def draw_link():
    """The board's own button: who owns the screen now. Tap = hand it to the other."""
    if not bt:
        return
    phone = state["link"] == "ble"
    bg = 0x1509 if phone else 0x231D
    label = "PHONE" if phone else "PC"
    d.fill_rect(LINK_X, 3, LINK_W, 20, bg)
    text(label, LINK_X + (LINK_W - len(label) * 8) // 2, 9, 0xFFFF, bg)


def set_link(new):
    old = state["link"]
    if new == old or not bt:
        return
    say(old, "AWAY")
    state["link"] = new
    cfg["link"] = new
    save_cfg()
    d.fill_rect(0, 0, d.w, d.h, 0)
    led_state["pulse"] = False
    set_led((0, 0, 0))
    state["face"] = None
    state["poses"] = True
    go_idle()
    draw_link()
    text("Waiting for the phone..." if new == "ble" else "Waiting for the PC...", 8, d.h - 22, 0x8410)
    say(new, "READY")


def cross(x, y, color):
    d.fill_rect(x - 12, y - 1, 25, 3, color)
    d.fill_rect(x - 1, y - 12, 3, 25, color)


def show(name, force=False):
    name = ALIAS.get(name, name)
    if name not in available:
        return False
    if name != state["face"] or force:
        d.blit_file(FACES_DIR + "/" + name + ".raw", 0, FACE_Y, W, FACE_H, out)
        state["face"] = name
    return True


def go_idle(tell=False):
    state["idle"] = True
    state["revert_at"] = 0
    if IDLE_SET:
        show(IDLE_SET[state["pose_i"] % len(IDLE_SET)], force=True)
    state["next_pose"] = time.ticks_add(time.ticks_ms(), IDLE_ROTATE)
    if tell:
        event("IDLE")


@micropython.viper
def unrle(src: ptr8, n: int, dst: ptr8, cap: int) -> int:
    # c < 128: c+1 literal pixels follow; c >= 128: the next pixel repeated c-126 times.
    i = 0
    o = 0
    while i < n:
        c = int(src[i])
        i += 1
        if c < 128:
            k = (c + 1) * 2
            if o + k > cap or i + k > n:
                return -1
            j = 0
            while j < k:
                dst[o + j] = src[i + j]
                j += 1
            o += k
            i += k
        else:
            k = c - 126
            if o + k * 2 > cap or i + 2 > n:
                return -1
            a = src[i]
            b = src[i + 1]
            i += 2
            while k > 0:
                dst[o] = a
                dst[o + 1] = b
                o += 2
                k -= 1
    return o


def read_exact(n, src="usb"):
    """The n bytes of a picture, from whoever sent the command. False if they never came."""
    if src == "ble":
        t0 = time.ticks_ms()
        while len(bt.pending) < n:
            bt.fill()
            if bt.conn is None or time.ticks_diff(time.ticks_ms(), t0) > 4000:
                bt.pending = bytearray()
                return False
            time.sleep_ms(2)
        rx_mv[:n] = bt.pending[:n]
        bt.pending = bt.pending[n:]
        return True
    got = 0
    while got < n:
        k = stdin.readinto(rx_mv[got:n])
        if k:
            got += k
    return True


# ── calibration ──────────────────────────────────────────────────────────────
def wait_tap(poll_serial, timeout_ms):
    """Raw point of one firm tap (middle of the readings), or None on timeout.
    Serial stays answered while waiting."""
    t0 = time.ticks_ms()
    while time.ticks_diff(time.ticks_ms(), t0) < timeout_ms:
        poll_serial()
        if touch.raw():
            xs, ys = [], []
            while len(xs) < 14:
                r = touch.raw()
                if not r:
                    break
                xs.append(r[0])
                ys.append(r[1])
                time.sleep_ms(6)
            while touch.raw():
                time.sleep_ms(10)
            # The first readings, while the finger lands, are off: drop them.
            if len(xs) >= 6:
                xs = sorted(xs[2:])
                ys = sorted(ys[2:])
                return xs[len(xs) // 2], ys[len(ys) // 2]
        time.sleep_ms(15)
    return None


def solve_cal(raw):
    """Calibration from the four corner taps (TL, TR, BR, BL), or None if they disagree."""
    (a0, b0), (a1, b1), (a2, b2), (a3, b3) = raw
    swap = abs(b1 - b0) > abs(a1 - a0)
    u = (b0, b1, b2, b3) if swap else (a0, a1, a2, a3)
    v = (a0, a1, a2, a3) if swap else (b0, b1, b2, b3)
    du_top, du_bot = u[1] - u[0], u[2] - u[3]
    dv_left, dv_right = v[3] - v[0], v[2] - v[1]
    for p, q in ((du_top, du_bot), (dv_left, dv_right)):
        # A real screen spans most of the sensor, and opposite edges agree.
        if abs(p) < 2000 or abs(q) < 2000 or (p > 0) != (q > 0) or abs(p - q) > 0.2 * abs(p):
            return None
    ax = 200 / ((du_top + du_bot) / 2)
    ay = 280 / ((dv_left + dv_right) / 2)
    return {"swap": 1 if swap else 0, "ax": ax, "bx": 20 - ax * (u[0] + u[3]) / 2,
            "ay": ay, "by": 20 - ay * (v[0] + v[1]) / 2}


def calibrate(poll_serial, timeout_ms=45_000):
    state["busy"] = True
    turned = d.rot
    d.orient(0, d.inv)  # the crosses and the maths are for the upright picture
    pts = ((20, 20), (220, 20), (220, 300), (20, 300))
    ok = False
    for attempt in range(3):
        d.fill_rect(0, 0, W, H, 0)
        text("Bader", 100, 120, 0x47FF)
        text("Press each + firmly", 44, 150)
        if attempt:
            text("Not clear - once more", 36, 170, 0xFE08)
        raw = []
        for x, y in pts:
            cross(x, y, 0xFE08)
            r = wait_tap(poll_serial, timeout_ms)
            cross(x, y, 0x0000)
            if not r:
                break
            beep()
            raw.append(r)
            time.sleep_ms(300)
        if len(raw) < 4:
            break  # nobody there: keep what we have
        cal = solve_cal(raw)
        if cal:
            cfg["cal"] = cal
            save_cfg()
            ok = True
            break
    d.fill_rect(0, 0, W, H, 0)
    d.orient(turned, d.inv)
    state["busy"] = False
    state["face"] = None
    go_idle()
    draw_link()
    event("READY")
    return ok


# ── commands ─────────────────────────────────────────────────────────────────
def handle(line, poll_serial, src="usb"):
    parts = line.strip().split()
    if not parts:
        return
    cmd = parts[0].upper()
    if cmd == "PING":
        say(src, "PONG bader-face", VERSION, d.w, d.h, "touch")
        # Nobody has ever spoken on the side that owns the screen: this side takes it.
        if src != state["link"] and not state["ble_seen" if src == "usb" else "usb_seen"] and (src == "usb" or state["link"] == "usb"):
            set_link(src)
        return
    if src != state["link"]:
        # The other one owns the screen. Swallow a picture's bytes so the line stays in step.
        if cmd == "IMG" and len(parts) >= 6:
            try:
                n = int(parts[5])
            except ValueError:
                n = 0
            if 0 < n <= MAX_IN:
                read_exact(n, src)
        say(src, "ERR away")
        return
    if cmd == "IMG" and len(parts) >= 6:
        x, y, w, h, n = (int(p) for p in parts[1:6])
        if n <= 0 or n > MAX_IN:
            say(src, "ERR size")
            return
        if not read_exact(n, src):
            say(src, "ERR short")
            return
        if state["busy"]:
            say(src, "OK")
            return
        if x < 0 or y < 0 or w <= 0 or h <= 0 or x + w > d.w or y + h > d.h or w * h * 2 > MAX_OUT:
            say(src, "ERR rect")
            return
        if unrle(rx, n, out, MAX_OUT) != w * h * 2:
            say(src, "ERR data")
            return
        d.blit(x, y, w, h, out_mv[: w * h * 2])
        if y < FACE_Y + FACE_H and y + h > FACE_Y:
            state["face"] = None  # the face area was painted over
        if y < 24 and x < LINK_X + LINK_W and x + w > LINK_X:
            draw_link()  # the app painted its bar; our button goes back on top
        say(src, "OK")
        return
    if state["busy"]:
        say(src, "OK")
        return
    if cmd == "FACE" and len(parts) >= 2:
        name = parts[1].lower()
        if name == "idle" or name.startswith("pose_"):
            if name in IDLE_SET:
                state["pose_i"] = IDLE_SET.index(name)
            go_idle()
        elif show(name, force=state["face"] is None):
            state["idle"] = False
            state["revert_at"] = 0
            if len(parts) >= 3:
                try:
                    state["revert_at"] = time.ticks_add(time.ticks_ms(), int(float(parts[2]) * 1000))
                except ValueError:
                    pass
        else:
            say(src, "ERR unknown face")
            return
    elif cmd == "POSES" and len(parts) >= 2:
        state["poses"] = parts[1].lower() == "on"
    elif cmd == "LED" and len(parts) >= 4:
        led_state["rgb"] = (int(parts[1]), int(parts[2]), int(parts[3]))
        led_state["pulse"] = len(parts) >= 5 and parts[4].lower() == "pulse"
        set_led(led_state["rgb"])
    elif cmd == "BEEP":
        beep(int(parts[1]) if len(parts) > 1 else 1800, min(int(parts[2]) if len(parts) > 2 else 25, 400))
    elif cmd == "BL" and len(parts) >= 2:
        d.backlight(int(parts[1]) / 100)
    elif cmd == "ROT" and len(parts) >= 2:
        # Quarter turns: 0 upright, 1 on its side, 2 upside down, 3 other side; "+" = next.
        cfg["rot"] = (d.rot + 1) % 4 if parts[1] == "+" else int(parts[1]) & 3
        save_cfg()
        d.orient(cfg["rot"], d.inv)
        d.fill_rect(0, 0, d.w, d.h, 0)
        state["face"] = None
        draw_link()
        # The new size is the answer: the app redraws everything for it.
        say(src, "PONG bader-face", VERSION, d.w, d.h, "touch")
        return
    elif cmd == "INV" and len(parts) >= 2:
        cfg["inv"] = 1 if parts[1] == "1" else 0
        save_cfg()
        d.orient(d.rot, cfg["inv"])
    elif cmd == "CAL":
        say(src, "OK")
        calibrate(poll_serial)
        return
    elif cmd == "LIST":
        say(src, "FACES", " ".join(sorted(available)))
    elif cmd == "ID":
        say(src, "ID", d.read_id(), "cal" if cfg.get("cal") else "nocal", "bt" if bt else "nobt",
            state["link"], "phone" if bt and bt.conn is not None else "nophone", gc.mem_free())
    elif cmd == "EXIT":
        say(src, "OK")
        micropython.kbd_intr(3)
        raise SystemExit
    else:
        say(src, "ERR unknown")
        return
    say(src, "OK")


def main():
    poll = select.poll()
    poll.register(sys.stdin, select.POLLIN)
    line = bytearray()
    t_boot = time.ticks_ms()

    def poll_serial(wait=0):
        nonlocal line
        while poll.poll(wait):
            wait = 0
            ch = stdin.read(1)
            state["usb_seen"] = True
            # Set-up tools (mpremote) break in with Ctrl-C right after a reset.
            if ch == b"\x03" and not line and time.ticks_diff(time.ticks_ms(), t_boot) < 10_000:
                micropython.kbd_intr(3)
                raise KeyboardInterrupt
            if ch in (b"\n", b"\r"):
                if line:
                    text_line = line.decode()
                    line = bytearray()
                    try:
                        handle(text_line, poll_serial)
                    except SystemExit:
                        raise
                    except Exception as e:
                        print("ERR", e)
                    gc.collect()
            elif ch:
                line += ch
                if len(line) > 120:
                    line = bytearray()

    def poll_ble():
        if not bt:
            return
        bt.fill()
        while True:
            i = bt.pending.find(b"\n")
            if i < 0:
                if len(bt.pending) > 160:
                    bt.pending = bytearray()
                return
            text_line = bt.pending[:i].decode().strip()
            bt.pending = bt.pending[i + 1:]
            state["ble_seen"] = True
            if text_line:
                try:
                    handle(text_line, poll_serial, "ble")
                except SystemExit:
                    raise
                except Exception as e:
                    say("ble", "ERR", e)
                gc.collect()

    d.fill_rect(0, 0, d.w, d.h, 0)
    d.backlight(0.9)
    set_led((0, 0, 0))
    # Pictures are binary, so Ctrl-C must be plain data while the app talks to
    # us (poll_serial lets set-up tools in during the first seconds; or send EXIT).
    micropython.kbd_intr(-1)
    if not cfg.get("cal"):
        calibrate(poll_serial)
    else:
        go_idle()
        draw_link()
        event("READY")

    pressed_at = None
    long_sent = False
    pts = []  # points of the touch in progress
    while True:
        poll_serial(12)
        poll_ble()
        now = time.ticks_ms()
        # Touch: reported on release. Where = the middle of all readings (the
        # first and last ones, while the finger lands and lifts, are off).
        p = touch.point()
        if p:
            if len(pts) < 60:
                pts.append(p)
        elif pts:
            if len(pts) >= 2:
                # Click first, report after: the app answers a touch with pictures
                # straight away, and nothing may block while those arrive.
                beep()
                core = pts[1:-1] if len(pts) >= 4 else pts
                dx, dy = core[-1][0] - core[0][0], core[-1][1] - core[0][1]
                if abs(dx) > 60 and abs(dx) > abs(dy):
                    event("SWIPE", "right" if dx > 0 else "left")
                elif abs(dy) > 60:
                    event("SWIPE", "down" if dy > 0 else "up")
                else:
                    xs = sorted(q[0] for q in core)
                    ys = sorted(q[1] for q in core)
                    tx, ty = xs[len(xs) // 2], ys[len(ys) // 2]
                    if bt and ty < 30 and LINK_X - 2 <= tx < LINK_X + LINK_W + 6:
                        set_link("usb" if state["link"] == "ble" else "ble")
                    else:
                        event("TOUCH", tx, ty)
            pts = []
        # BOOT button.
        if button.value() == 0:
            if pressed_at is None:
                pressed_at = now
                long_sent = False
            elif not long_sent and time.ticks_diff(now, pressed_at) >= LONG_MS:
                event("BTN long")
                long_sent = True
        elif pressed_at is not None:
            if not long_sent and time.ticks_diff(now, pressed_at) >= 40:
                event("BTN short")
            pressed_at = None
        if state["revert_at"] and time.ticks_diff(now, state["revert_at"]) >= 0:
            go_idle(tell=True)
        elif (state["idle"] and state["poses"] and state["face"] is not None and len(IDLE_SET) > 1
              and time.ticks_diff(now, state["next_pose"]) >= 0):
            state["pose_i"] = (state["pose_i"] + 1) % len(IDLE_SET)
            go_idle()
        if led_state["pulse"]:
            set_led(led_state["rgb"], 0.15 + 0.85 * (0.5 + 0.5 * math.sin(now / 300)))


main()
