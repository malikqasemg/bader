# Bader face — ST7789 driver for the Waveshare ESP32-C6-LCD-1.47 (172x320).
# Pins: MOSI 6, SCLK 7, CS 14, DC 15, RST 21, backlight 22.

import time
from machine import Pin, SPI, PWM

W, H = 172, 320
X_OFF, Y_OFF = 34, 0


class Display:
    def __init__(self):
        self.spi = SPI(1, baudrate=40_000_000, polarity=0, phase=0,
                       sck=Pin(7), mosi=Pin(6))
        self.cs = Pin(14, Pin.OUT, value=1)
        self.dc = Pin(15, Pin.OUT, value=0)
        self.rst = Pin(21, Pin.OUT, value=1)
        self.bl = PWM(Pin(22), freq=1000, duty_u16=0)
        self._init()

    def _cmd(self, c, data=None):
        self.cs(0)
        self.dc(0)
        self.spi.write(bytes([c]))
        if data:
            self.dc(1)
            self.spi.write(data)
        self.cs(1)

    def _init(self):
        self.rst(0); time.sleep_ms(20); self.rst(1); time.sleep_ms(120)
        self._cmd(0x01); time.sleep_ms(150)          # software reset
        self._cmd(0x11); time.sleep_ms(120)          # sleep out
        self._cmd(0x3A, b"\x55")                     # 16-bit colour
        self._cmd(0x36, b"\x00")                     # portrait, RGB
        self._cmd(0x21)                              # inversion on (IPS)
        self._cmd(0x13)                              # normal display
        self._cmd(0x29); time.sleep_ms(20)           # display on

    def window(self, x0, y0, x1, y1):
        x0 += X_OFF; x1 += X_OFF; y0 += Y_OFF; y1 += Y_OFF
        self._cmd(0x2A, bytes([x0 >> 8, x0 & 255, x1 >> 8, x1 & 255]))
        self._cmd(0x2B, bytes([y0 >> 8, y0 & 255, y1 >> 8, y1 & 255]))
        self._cmd(0x2C)

    def backlight(self, level):
        """0.0 – 1.0"""
        self.bl.duty_u16(int(max(0, min(1, level)) * 65535))

    def fill(self, rgb565):
        self.window(0, 0, W - 1, H - 1)
        line = bytes([rgb565 >> 8, rgb565 & 255]) * W
        self.cs(0); self.dc(1)
        for _ in range(H):
            self.spi.write(line)
        self.cs(1)

    def blit_file(self, path, x=0, y=0, w=W, h=H):
        """Streams a raw big-endian RGB565 file of w*h pixels to (x, y)."""
        self.window(x, y, x + w - 1, y + h - 1)
        buf = bytearray(w * 2 * 16)
        mv = memoryview(buf)
        with open(path, "rb") as f:
            self.cs(0); self.dc(1)
            while True:
                n = f.readinto(buf)
                if not n:
                    break
                self.spi.write(mv[:n])
            self.cs(1)

    def blit_buf(self, y, h, buf):
        """Draws a full-width strip from an RGB565 buffer (172*h*2 bytes)."""
        self.window(0, y, W - 1, y + h - 1)
        self.cs(0); self.dc(1)
        self.spi.write(buf)
        self.cs(1)
