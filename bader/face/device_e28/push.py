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
# Opening the port restarts the board: wait until it says READY (Bluetooth makes that a few seconds).
t0 = time.time()
seen = b""
while b"READY" not in seen and time.time() - t0 < 12:
    seen += p.read(64)
time.sleep(0.3)
p.reset_input_buffer()
# EXIT restarts main.py; Ctrl-C in its first seconds drops to the prompt. When
# exactly that lands varies, so keep asking until the raw prompt answers cleanly.
p.write(b"\nEXIT\n")
for attempt in range(8):
    time.sleep(0.7)
    p.write(b"\r\x03\x03")
    time.sleep(0.4)
    p.write(b"\r\x01")  # raw REPL
    time.sleep(0.6)
    p.reset_input_buffer()
    p.write(b"print('SYNC')\x04")
    out = p.read_until(b"\x04>")
    if out.startswith(b"OK") and b"SYNC" in out and b"Traceback" not in out:
        break
else:
    sys.exit("the board did not give its prompt")


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
