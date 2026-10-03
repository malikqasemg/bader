"""Talks to a Bader touch face over USB the way the app does (for set-up checks).

    python3 selftest.py <port> [command ...]      e.g. selftest.py /dev/cu.usbserial-8310 ID "FACE happy"
With no command: PING, ID, LIST, then listens for touches for 20 s.
"""
import sys
import time

import serial

port = serial.Serial()
port.port = sys.argv[1]
port.baudrate = 115200
port.timeout = 0.3
port.dtr = False
port.rts = False
port.open()
time.sleep(2.8)
port.reset_input_buffer()


def say(cmd, wait=2.5):
    port.write((cmd + "\n").encode())
    end = time.time() + wait
    out = []
    while time.time() < end:
        line = port.readline().decode(errors="replace").strip()
        if line:
            out.append(line)
            if line == "OK" or line.startswith(("ERR", "PONG")):
                break
    print(f"{cmd!r} -> {out}", flush=True)


cmds = sys.argv[2:]
for c in cmds or ["PING", "ID", "LIST"]:
    say(c)
if not cmds:
    end = time.time() + 20
    while time.time() < end:
        line = port.readline().decode(errors="replace").strip()
        if line:
            print("event:", line, flush=True)
