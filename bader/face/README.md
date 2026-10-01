# Bader face — live status screen

Bader's face on a **Waveshare ESP32-C6-LCD-1.47** (ESP32-C6FH8, 172×320 ST7789),
plugged into the computer by USB-C. The Bader app finds it automatically and
shows what Bader is doing.

| App state | Face | Label |
|---|---|---|
| idle | neutral | Ready · جاهز |
| mic recording | listening | Listening · أستمع |
| waiting for the engine | thinking | Thinking · أفكر |
| reading the reply aloud | speaking | Speaking · أتكلم |
| reply done | happy (3 s) | Done · تم |
| error | concerned (6 s) | Problem · في مشكلة |
| new message (planned) | surprised | New message · رسالة جديدة |
| start-up | celebrating | Hello! · أهلاً |

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

## Protocol (USB serial, 115200, one line per command)
- `FACE <name> [seconds]` — show a face; after `seconds` return to neutral
- `BL <0-100>` — backlight
- `PING` → `PONG bader-face <version>` · `LIST` → available faces

Note: while the Bader app is running it holds the USB port; quit it before using mpremote.
