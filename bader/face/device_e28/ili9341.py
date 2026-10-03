# ILI9341 240x320 driver for the 2.8" ESP32-32E display board (LCDWiki E32R28T).
# LCD on SPI: SCK 14, MOSI 13, MISO 12, CS 15, DC 2, backlight 21 (reset is tied to EN).

import time
from machine import Pin, PWM, SPI

W = 240
H = 320


class Display:
    def __init__(self, rot=0, inv=0):
        self.spi = SPI(1, baudrate=40_000_000, sck=Pin(14), mosi=Pin(13), miso=Pin(12))
        self.cs = Pin(15, Pin.OUT, value=1)
        self.dc = Pin(2, Pin.OUT, value=1)
        self.bl = PWM(Pin(21), freq=1000, duty=0)
        self.rot = rot & 3
        self.inv = inv
        self.w, self.h = (H, W) if rot & 1 else (W, H)
        self._win = bytearray(4)
        self.init()

    def cmd(self, c, data=None):
        self.cs(0)
        self.dc(0)
        self.spi.write(bytes((c,)))
        if data:
            self.dc(1)
            self.spi.write(data)
        self.cs(1)

    def init(self):
        self.cmd(0x01)
        time.sleep_ms(150)
        for c, d in (
            (0xCF, b"\x00\xC1\x30"), (0xED, b"\x64\x03\x12\x81"), (0xE8, b"\x85\x00\x78"),
            (0xCB, b"\x39\x2C\x00\x34\x02"), (0xF7, b"\x20"), (0xEA, b"\x00\x00"),
            (0xC0, b"\x23"), (0xC1, b"\x10"), (0xC5, b"\x3E\x28"), (0xC7, b"\x86"),
            (0x3A, b"\x55"), (0xB1, b"\x00\x18"), (0xB6, b"\x08\x82\x27"), (0xF2, b"\x00"),
            (0x26, b"\x01"),
            (0xE0, b"\x0F\x31\x2B\x0C\x0E\x08\x4E\xF1\x37\x07\x10\x03\x0E\x09\x00"),
            (0xE1, b"\x00\x0E\x14\x03\x11\x07\x31\xC1\x48\x08\x0F\x0C\x31\x36\x0F"),
        ):
            self.cmd(c, d)
        self.orient(self.rot, self.inv)
        self.cmd(0x11)
        time.sleep_ms(120)
        self.cmd(0x29)

    def orient(self, rot, inv):
        """rot: quarter turns (0 upright, 1 on its side, 2 upside down, 3 other side)."""
        self.rot, self.inv = rot & 3, inv
        self.w, self.h = (H, W) if self.rot & 1 else (W, H)
        self.cmd(0x36, bytes(((0x48, 0x28, 0x88, 0xE8)[self.rot],)))  # BGR panel
        self.cmd(0x21 if inv else 0x20)

    def backlight(self, level):
        self.bl.duty(int(max(0.0, min(1.0, level)) * 1023))

    def window(self, x, y, w, h):
        b = self._win
        x1, y1 = x + w - 1, y + h - 1
        b[0], b[1], b[2], b[3] = x >> 8, x & 0xFF, x1 >> 8, x1 & 0xFF
        self.cmd(0x2A, b)
        b[0], b[1], b[2], b[3] = y >> 8, y & 0xFF, y1 >> 8, y1 & 0xFF
        self.cmd(0x2B, b)
        self.cs(0)
        self.dc(0)
        self.spi.write(b"\x2C")
        self.dc(1)

    def blit(self, x, y, w, h, buf):
        """buf: w*h RGB565 pixels, big-endian."""
        self.window(x, y, w, h)
        self.spi.write(buf)
        self.cs(1)

    def fill_rect(self, x, y, w, h, color):
        row = bytes((color >> 8, color & 0xFF)) * w
        self.window(x, y, w, h)
        for _ in range(h):
            self.spi.write(row)
        self.cs(1)

    def blit_file(self, path, x, y, w, h, chunk):
        """Raw RGB565 file straight to the screen; chunk is a reusable bytearray."""
        mv = memoryview(chunk)
        self.window(x, y, w, h)
        with open(path, "rb") as f:
            while True:
                n = f.readinto(chunk)
                if not n:
                    break
                self.spi.write(mv[:n])
        self.cs(1)

    def read_id(self):
        """Controller id bytes (ILI9341 answers 00 93 41)."""
        out = []
        self.spi.init(baudrate=2_000_000)
        for i in (1, 2, 3):
            self.cmd(0xD9, bytes((0x10 + i,)))
            self.cs(0)
            self.dc(0)
            self.spi.write(b"\xD3")
            self.dc(1)
            out.append(self.spi.read(1)[0])
            self.cs(1)
        self.spi.init(baudrate=40_000_000)
        return out
