# Bader face — main loop for the ESP32-C6 screen (Waveshare ESP32-C6-LCD-1.47).
#
# From the Bader app, over USB serial, one command per line:
#   FACE <name> [seconds]   show a face; after <seconds> back to idle
#   STRIP <y> <h> [idle]    then 172*h*2 raw RGB565 bytes: draw a full-width strip
#                           ("idle": keep it and redraw it on every idle pose)
#   LED <r> <g> <b> [pulse] RGB light (0-255); "pulse" breathes it
#   PING / LIST
# To the app:
#   BTN short | BTN long    BOOT button (short < 1 s; long >= 1 s)
# Idle: full-body Bader and the poses rotate every IDLE_ROTATE ms.

import sys
import select
import time
import os
import math

import micropython
import neopixel
from machine import Pin

from st7789 import Display, W

VERSION = "2.0"
FACES_DIR = "/faces"
IDLE_ROTATE = 30_000
LONG_MS = 1000

d = Display()
available = set(f[:-4] for f in os.listdir(FACES_DIR) if f.endswith(".raw"))
IDLE_SET = ["idle"] + sorted(n for n in available if n.startswith("pose_"))
IDLE_SET = [n for n in IDLE_SET if n in available] or ["neutral"]

state = {"face": None, "idle": True, "revert_at": 0, "next_pose": 0, "pose_i": 0}
idle_strip = {"y": 0, "h": 0, "buf": None}
led = neopixel.NeoPixel(Pin(8), 1)
led_state = {"rgb": (0, 0, 0), "pulse": False}
button = Pin(9, Pin.IN, Pin.PULL_UP)
stdin = sys.stdin.buffer


def set_led(rgb, level=1.0):
    led[0] = tuple(int(c * level) for c in rgb)
    led.write()


def draw_idle_strip():
    if idle_strip["buf"] is not None:
        d.blit_buf(idle_strip["y"], idle_strip["h"], idle_strip["buf"])


def show(name, force=False):
    if name not in available:
        return False
    if name != state["face"] or force:
        d.blit_file(FACES_DIR + "/" + name + ".raw")
        state["face"] = name
    return True


def go_idle():
    state["idle"] = True
    state["revert_at"] = 0
    show(IDLE_SET[state["pose_i"] % len(IDLE_SET)], force=True)
    draw_idle_strip()
    state["next_pose"] = time.ticks_add(time.ticks_ms(), IDLE_ROTATE)


def read_exact(n):
    """Reads n raw bytes from USB (Ctrl-C is just data while doing it)."""
    buf = bytearray(n)
    mv = memoryview(buf)
    got = 0
    micropython.kbd_intr(-1)
    try:
        while got < n:
            k = stdin.readinto(mv[got:])
            if k:
                got += k
    finally:
        micropython.kbd_intr(3)
    return buf


def handle(line):
    parts = line.strip().split()
    if not parts:
        return
    cmd = parts[0].upper()
    if cmd == "FACE" and len(parts) >= 2:
        name = parts[1].lower()
        if name in ("idle",) or name.startswith("pose_"):
            if name.startswith("pose_") and name in IDLE_SET:
                state["pose_i"] = IDLE_SET.index(name)
            go_idle()
            return
        if show(name):
            state["idle"] = False
            state["revert_at"] = 0
            if len(parts) >= 3:
                try:
                    state["revert_at"] = time.ticks_add(time.ticks_ms(), int(float(parts[2]) * 1000))
                except ValueError:
                    pass
        else:
            print("ERR unknown face")
    elif cmd == "STRIP" and len(parts) >= 3:
        y, h = int(parts[1]), int(parts[2])
        if h <= 0 or y + h > 320:
            print("ERR strip")
            return
        buf = read_exact(W * h * 2)
        if len(parts) >= 4 and parts[3].lower() == "idle":
            idle_strip.update(y=y, h=h, buf=buf)
            if state["idle"]:
                d.blit_buf(y, h, buf)
        elif not state["idle"]:
            d.blit_buf(y, h, buf)
    elif cmd == "LED" and len(parts) >= 4:
        led_state["rgb"] = (int(parts[1]), int(parts[2]), int(parts[3]))
        led_state["pulse"] = len(parts) >= 5 and parts[4].lower() == "pulse"
        set_led(led_state["rgb"])
    elif cmd == "BL" and len(parts) >= 2:
        d.backlight(int(parts[1]) / 100)
    elif cmd == "PING":
        print("PONG bader-face", VERSION)
    elif cmd == "LIST":
        print("FACES", " ".join(sorted(available)))


def main():
    d.fill(0x0000)
    d.backlight(0.9)
    set_led((0, 0, 0))
    go_idle()
    poll = select.poll()
    poll.register(sys.stdin, select.POLLIN)
    line = bytearray()
    pressed_at = None
    long_sent = False
    while True:
        if poll.poll(20):
            ch = stdin.read(1)
            if ch in (b"\n", b"\r"):
                if line:
                    try:
                        handle(line.decode())
                    except Exception as e:
                        print("ERR", e)
                line = bytearray()
            elif ch:
                line += ch
                if len(line) > 120:
                    line = bytearray()
        now = time.ticks_ms()
        # Button: short press on release, long press as soon as it is held long enough.
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
            go_idle()
        elif state["idle"] and len(IDLE_SET) > 1 and time.ticks_diff(now, state["next_pose"]) >= 0:
            state["pose_i"] = (state["pose_i"] + 1) % len(IDLE_SET)
            go_idle()
        if led_state["pulse"]:
            set_led(led_state["rgb"], 0.15 + 0.85 * (0.5 + 0.5 * math.sin(now / 300)))


main()
