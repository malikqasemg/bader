# Bader face — main loop for the ESP32-C6 screen.
#
# The Bader app sends one line per change over USB serial:
#   FACE <name> [seconds]   show a face; after <seconds> go back to "neutral"
#   BL <0-100>              backlight level
#   PING                    -> "PONG bader-face <version>"
# Faces live in /faces/<name>.raw (172x320 RGB565).

import sys
import select
import time
import os

from st7789 import Display

VERSION = "1.0"
FACES_DIR = "/faces"
IDLE = "neutral"

d = Display()
available = set(f[:-4] for f in os.listdir(FACES_DIR) if f.endswith(".raw"))
current = None
revert_at = 0


def show(name):
    global current
    if name not in available or name == current:
        return name in available
    d.blit_file(FACES_DIR + "/" + name + ".raw")
    current = name
    return True


def handle(line):
    global revert_at
    parts = line.strip().split()
    if not parts:
        return
    cmd = parts[0].upper()
    if cmd == "FACE" and len(parts) >= 2:
        ok = show(parts[1].lower())
        revert_at = 0
        if ok and len(parts) >= 3:
            try:
                revert_at = time.ticks_add(time.ticks_ms(), int(float(parts[2]) * 1000))
            except ValueError:
                pass
        if not ok:
            print("ERR unknown face")
    elif cmd == "BL" and len(parts) >= 2:
        try:
            d.backlight(int(parts[1]) / 100)
        except ValueError:
            print("ERR")
    elif cmd == "PING":
        print("PONG bader-face", VERSION)
    elif cmd == "LIST":
        print("FACES", " ".join(sorted(available)))


def main():
    global revert_at
    d.fill(0x0000)
    d.backlight(0.9)
    show("celebrating" if "celebrating" in available else IDLE)
    revert_at = time.ticks_add(time.ticks_ms(), 2500)
    poll = select.poll()
    poll.register(sys.stdin, select.POLLIN)
    buf = ""
    while True:
        if poll.poll(50):
            ch = sys.stdin.read(1)
            if ch in ("\n", "\r"):
                if buf:
                    handle(buf)
                buf = ""
            elif ch:
                buf += ch
                if len(buf) > 80:
                    buf = ""
        if revert_at and time.ticks_diff(time.ticks_ms(), revert_at) >= 0:
            revert_at = 0
            show(IDLE)


main()
