# Bader face — live status screen

Bader's face on a **Waveshare ESP32-C6-LCD-1.47** (ESP32-C6FH8, 172×320 ST7789),
plugged into the computer by USB-C. The Bader app finds it automatically and
shows what Bader is doing.

| App state | Face | Label |
|---|---|---|
| start-up and idle | idle | full-body Bader + "Bader" |
| mic recording | listening | Listening · أستمع |
| waiting for the engine | thinking | Thinking · أفكر |
| reading the reply aloud | speaking | Speaking · أتكلم |
| reply done | happy (3 s) | Done · تم |
| error | concerned (6 s) | Problem · في مشكلة |
| new message (planned) | surprised | New message · رسالة جديدة |
| greeting (spare) | celebrating | Hello! · أهلاً |
| spare | neutral | Ready · جاهز |

## Files
- `art/` — source expressions (1254×1254 PNG, transparent)
- `build_faces.py` — makes `faces/<name>.raw` (RGB565) + PNG previews with the labels
- `device/st7789.py`, `device/main.py` — MicroPython program on the board
- `firmware/` — MicroPython 1.29 for ESP32-C6

## Set up a board
```
python3 -m esptool --chip esp32c6 --port <port> erase_flash
python3 -m esptool --chip esp32c6 --port <port> --baud 460800 write_flash 0 firmware/ESP32_GENERIC_C6-20260824-v1.29.0.bin
python3 build_faces.py art
python3 -m mpremote connect <port> mkdir :faces
python3 -m mpremote connect <port> cp device/st7789.py :st7789.py + cp device/main.py :main.py
for f in faces/*.raw; do python3 -m mpremote connect <port> cp $f :$f; done
python3 -m mpremote connect <port> reset
```

## Protocol v2 (USB serial, 115200)
App → screen, one line per command:
- `FACE <name> [seconds]` — show a face; after `seconds` back to idle
- `STRIP <y> <h> [idle]` + `172*h*2` raw RGB565 bytes — draw a full-width strip
  (the island renders the text, so Arabic works); `idle` = keep it under idle poses
- `LED <r> <g> <b> [pulse]` — RGB light (amber pulse = waiting for approval)
- `PING` → `PONG bader-face 2.0` · `LIST` → faces on the board

Screen → app:
- `BTN short` / `BTN long` — BOOT button. Approval pending: short = approve,
  long (≥ 1 s) = deny. Otherwise short = start/stop talking to Bader.

Idle: full-body Bader and the poses in `art/poses` rotate every 30 s, with
"next meeting · unread mail" in the bottom strip (from the background sync).

Note: this board has no touch panel. The touch version is the
Waveshare ESP32-C6-Touch-LCD-1.47 (same screen + touch).
Quit the Bader app before using mpremote (the app holds the USB port).

---

# Touch face — 2.8" ESP32-32E display (240×320, touch, light, speaker)

The bigger screen. With both screens plugged in, the app uses this one.

**What it shows**
- Top bar: time, date, unread mail.
- Bader's face or pose, what he is doing (English + Arabic), and touch buttons.
- Home buttons: **Talk · Brief · Mail · Day**. Tap Bader = talk.
- **Mail** and **Day** pages: tap a row and Bader explains that email / meeting.
- After an answer: the full text on the screen (tap or swipe for the next page).
- Approvals: **Approve / Deny** buttons. While listening: **Send / Cancel**. While speaking: **Stop**.
- Swipe left/right on the home page: Home → Day → Mail.

**Files**
- `device_e28/ili9341.py`, `device_e28/main.py` — MicroPython program (protocol v3)
- `device_e28/selftest.py` — talk to the board from a terminal
- `faces_e28/` — 240×176 pictures for the face area (`build_faces.py` makes them)
- `firmware/ESP32_GENERIC-…bin` — MicroPython 1.29 for the classic ESP32

**Set up a board** (quit the Bader app first)
```
python3 -m esptool --chip esp32 --port <port> erase-flash
python3 -m esptool --chip esp32 --port <port> --baud 460800 write-flash 0x1000 firmware/ESP32_GENERIC-20260824-v1.29.0.bin
python3 build_faces.py
python3 -m mpremote connect <port> mkdir :faces
python3 -m mpremote connect <port> cp device_e28/ili9341.py :ili9341.py
for f in faces_e28/*.raw; do python3 -m mpremote connect <port> cp $f :faces/$(basename $f); done
python3 -m mpremote connect <port> cp device_e28/main.py :main.py
python3 -m mpremote connect <port> reset
```
First start: tap the three crosses (touch calibration, saved on the board).
Hold the BOOT button for a second to calibrate again.

**Protocol v3** (USB serial 115200; every command answers `OK` or `ERR …`)
- `PING` → `PONG bader-face 3.0 240 320 touch`
- `FACE <name> [seconds]` — picture in the face area; then back to the idle pose (`IDLE` is sent)
- `IMG <x> <y> <w> <h> <n>` + n bytes — RLE picture of a region (≤ 16 rows, ≤ 4000 bytes;
  byte c < 128: c+1 literal pixels, c ≥ 128: next pixel × (c−126); pixels are RGB565 big-endian)
- `POSES on|off` · `LED r g b [pulse]` · `BEEP hz ms` · `BL percent` · `CAL` · `ROT 0|1` · `INV 0|1` · `EXIT`
- Board → app: `TOUCH x y` · `SWIPE left|right|up|down` · `BTN short|long` · `IDLE` · `READY`

Pins: LCD SCK 14, MOSI 13, MISO 12, CS 15, DC 2, backlight 21 · touch (XPT2046) CLK 25,
MOSI 32, MISO 39, CS 33, IRQ 36 · light R 22, G 16, B 17 (low = on) · speaker 26, amp enable 4 (low) · BOOT 0.
