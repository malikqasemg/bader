# Bader face v3 — touch screen (2.8" ESP32-32E board: ILI9341 240x320 + XPT2046).
#
# The app owns the layout: it sends pictures of screen regions (text, buttons,
# lists — so Arabic renders properly) and the board reports touches.
#
# App -> board, one line per command; every command answers OK or ERR <why>:
#   PING                      -> PONG bader-face 3.0 240 320 touch
#   FACE <name> [seconds]     picture in the face area (y 28..203); then back to idle
#   IMG <x> <y> <w> <h> <n>   + n bytes: RLE picture of a region (see unrle)
#   POSES on|off              rotate idle poses (off while a page covers the face)
#   LED <r> <g> <b> [pulse]   RGB light       BEEP <hz> <ms>    BL <percent>
#   CAL                       touch calibration (3 taps)
#   ROT 0|1 · INV 0|1         flip the picture / invert colours (saved)
#   EXIT                      stop (drops to the MicroPython prompt)
# Board -> app:
#   TOUCH <x> <y> · SWIPE left|right|up|down · BTN short|long (BOOT) · IDLE · READY

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

VERSION = "3.0"
FACES_DIR = "/faces"
FACE_Y, FACE_H = 28, 176
IDLE_ROTATE = 30_000
LONG_MS = 1000
ALIAS = {"working": "thinking", "approval": "surprised"}
MAX_IN = 4096
MAX_OUT = W * 16 * 2

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
state = {"face": None, "idle": True, "revert_at": 0, "next_pose": 0, "pose_i": 0, "poses": True, "busy": False}

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
        for _ in range(5):
            xs.append(self._read(self.cx))
            ys.append(self._read(self.cy))
        if self.irq.value():
            return None
        xs.sort()
        ys.sort()
        x, y = xs[2], ys[2]
        if x < 60 or x > 4040 or y < 60 or y > 4040 or xs[4] - xs[0] > 300 or ys[4] - ys[0] > 300:
            return None
        return x, y

    def point(self):
        r = self.raw()
        if not r:
            return None
        c = cfg.get("cal") or {"swap": 0, "ax": -0.0667, "bx": 255, "ay": 0.0889, "by": -18}
        u, v = (r[1], r[0]) if c["swap"] else r
        x = int(c["ax"] * u + c["bx"])
        y = int(c["ay"] * v + c["by"])
        return max(0, min(W - 1, x)), max(0, min(H - 1, y))


touch = Touch()


def save_cfg():
    with open("/cfg.json", "w") as f:
        json.dump(cfg, f)


# ── drawing helpers ──────────────────────────────────────────────────────────
def text(msg, x, y, color=0xFFFF):
    """Small built-in font (set-up screens only; the app draws the real text)."""
    w = min(len(msg) * 8, W)
    n = w * 8 * 2
    fb = framebuf.FrameBuffer(out_mv[:n], w, 8, framebuf.RGB565)
    fb.fill(0)
    fb.text(msg, 0, 0, ((color & 0xFF) << 8) | (color >> 8))
    d.blit(x, y, w, 8, out_mv[:n])


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
        print("IDLE")


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


def read_exact(n):
    got = 0
    while got < n:
        k = stdin.readinto(rx_mv[got:n])
        if k:
            got += k


# ── calibration ──────────────────────────────────────────────────────────────
def wait_tap(poll_serial, timeout_ms):
    """Median raw point of one tap, or None on timeout. Serial stays answered."""
    t0 = time.ticks_ms()
    while time.ticks_diff(time.ticks_ms(), t0) < timeout_ms:
        poll_serial()
        r = touch.raw()
        if r:
            xs, ys = [], []
            while len(xs) < 9:
                r = touch.raw()
                if not r:
                    break
                xs.append(r[0])
                ys.append(r[1])
                time.sleep_ms(8)
            while touch.raw():
                time.sleep_ms(10)
            if len(xs) >= 5:
                xs.sort()
                ys.sort()
                return xs[len(xs) // 2], ys[len(ys) // 2]
        time.sleep_ms(15)
    return None


def calibrate(poll_serial, timeout_ms=45_000):
    state["busy"] = True
    d.fill_rect(0, 0, W, H, 0)
    text("Bader", 100, 120, 0x47FF)
    text("Tap each cross", 64, 150)
    pts = ((20, 20), (220, 20), (20, 300))
    raw = []
    for x, y in pts:
        cross(x, y, 0xFE08)
        r = wait_tap(poll_serial, timeout_ms)
        cross(x, y, 0x0000)
        if not r:
            break
        beep()
        raw.append(r)
        time.sleep_ms(250)
    ok = False
    if len(raw) == 3:
        (a0, b0), (a1, b1), (a2, b2) = raw
        swap = abs(b1 - b0) > abs(a1 - a0)
        u0, u1 = (b0, b1) if swap else (a0, a1)
        v0, v2 = (a0, a2) if swap else (b0, b2)
        if abs(u1 - u0) > 800 and abs(v2 - v0) > 800:
            ax = 200 / (u1 - u0)
            ay = 280 / (v2 - v0)
            cfg["cal"] = {"swap": 1 if swap else 0, "ax": ax, "bx": 20 - ax * u0, "ay": ay, "by": 20 - ay * v0}
            save_cfg()
            ok = True
    d.fill_rect(0, 0, W, H, 0)
    state["busy"] = False
    state["face"] = None
    go_idle()
    print("READY")
    return ok


# ── commands ─────────────────────────────────────────────────────────────────
def handle(line, poll_serial):
    parts = line.strip().split()
    if not parts:
        return
    cmd = parts[0].upper()
    if cmd == "PING":
        print("PONG bader-face", VERSION, W, H, "touch")
        return
    if cmd == "IMG" and len(parts) >= 6:
        x, y, w, h, n = (int(p) for p in parts[1:6])
        if n <= 0 or n > MAX_IN:
            print("ERR size")
            return
        read_exact(n)
        if state["busy"]:
            print("OK")
            return
        if x < 0 or y < 0 or w <= 0 or h <= 0 or x + w > W or y + h > H or w * h * 2 > MAX_OUT:
            print("ERR rect")
            return
        if unrle(rx, n, out, MAX_OUT) != w * h * 2:
            print("ERR data")
            return
        d.blit(x, y, w, h, out_mv[: w * h * 2])
        if y < FACE_Y + FACE_H and y + h > FACE_Y:
            state["face"] = None  # the face area was painted over
        print("OK")
        return
    if state["busy"]:
        print("OK")
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
            print("ERR unknown face")
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
    elif cmd in ("ROT", "INV") and len(parts) >= 2:
        cfg[cmd.lower()] = 1 if parts[1] == "1" else 0
        save_cfg()
        d.orient(cfg.get("rot", 0), cfg.get("inv", 0))
    elif cmd == "CAL":
        print("OK")
        calibrate(poll_serial)
        return
    elif cmd == "LIST":
        print("FACES", " ".join(sorted(available)))
    elif cmd == "ID":
        print("ID", d.read_id(), "cal" if cfg.get("cal") else "nocal")
    elif cmd == "EXIT":
        print("OK")
        micropython.kbd_intr(3)
        raise SystemExit
    else:
        print("ERR unknown")
        return
    print("OK")


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

    d.fill_rect(0, 0, W, H, 0)
    d.backlight(0.9)
    set_led((0, 0, 0))
    # Pictures are binary, so Ctrl-C must be plain data while the app talks to
    # us (poll_serial lets set-up tools in during the first seconds; or send EXIT).
    micropython.kbd_intr(-1)
    if not cfg.get("cal"):
        calibrate(poll_serial)
    else:
        go_idle()
        print("READY")

    pressed_at = None
    long_sent = False
    t_start = None  # first touch point
    t_last = None
    while True:
        poll_serial(15)
        now = time.ticks_ms()
        # Touch: tap on release; a long drag is a swipe.
        p = touch.point()
        if p:
            if t_start is None:
                t_start = p
            t_last = p
        elif t_start is not None:
            # Click first, report after: the app answers a touch with pictures
            # straight away, and nothing may block while those arrive.
            beep()
            dx, dy = t_last[0] - t_start[0], t_last[1] - t_start[1]
            if abs(dx) > 60 and abs(dx) > abs(dy):
                print("SWIPE", "right" if dx > 0 else "left")
            elif abs(dy) > 60:
                print("SWIPE", "down" if dy > 0 else "up")
            else:
                print("TOUCH", t_start[0], t_start[1])
            t_start = t_last = None
        # BOOT button.
        if button.value() == 0:
            if pressed_at is None:
                pressed_at = now
                long_sent = False
            elif not long_sent and time.ticks_diff(now, pressed_at) >= LONG_MS:
                print("BTN long")
                long_sent = True
        elif pressed_at is not None:
            if not long_sent and time.ticks_diff(now, pressed_at) >= 40:
                print("BTN short")
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
