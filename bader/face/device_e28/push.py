"""Updates files on a Bader touch face that is already running (no mpremote needed).

    python3 push.py <port> [--rm <file>] [<local-file> ...]     files land in / on the board
"""
import os
import sys
import time

import serial

args = sys.argv[2:]
p = serial.Serial()
p.port = sys.argv[1]
p.baudrate = 115200
p.timeout = 2
p.dtr = False
p.rts = False
p.open()
time.sleep(3.0)
p.reset_input_buffer()
p.write(b"\nEXIT\n")
time.sleep(0.5)
p.write(b"\r\x03\x03")
time.sleep(0.3)
p.write(b"\r\x01")  # raw REPL
time.sleep(0.3)
p.reset_input_buffer()


def run(code):
    p.write(code.encode() + b"\x04")
    out = p.read_until(b"\x04>")
    if not out.startswith(b"OK") or b"Traceback" in out:
        sys.exit(f"board said: {out!r}")
    return out[2:].split(b"\x04")[0].decode()


i = 0
while i < len(args):
    if args[i] == "--rm":
        print(run(f"import os\ntry:\n os.remove('/{args[i + 1]}')\n print('removed')\nexcept OSError:\n print('not there')"), end="")
        i += 2
        continue
    data = open(args[i], "rb").read()
    name = os.path.basename(args[i])
    run(f"f=open('/{name}.new','wb')")
    for k in range(0, len(data), 512):
        run(f"f.write({data[k:k + 512]!r})")
    run(f"f.close()\nimport os\nos.rename('/{name}.new','/{name}')")
    print("pushed", name, len(data))
    i += 1
p.write(b"\x02")  # leave raw REPL
time.sleep(0.2)
p.write(b"\x04")  # soft reset: main.py starts again
time.sleep(0.5)
