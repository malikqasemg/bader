"""Builds the Bader face images for the ESP32-C6 screen (172x320 RGB565).

Each face = the expression art (head) on black + a status label in English
and Arabic. Output: bader/face/faces/<name>.raw (+ a PNG preview).

    python3 build_faces.py "/path/to/Bader Face Expressions"
"""

import os
import sys

import arabic_reshaper
from bidi.algorithm import get_display
from PIL import Image, ImageDraw, ImageFont

W, H = 172, 320
BG = (0, 0, 0)
CYAN = (64, 224, 255)
GREY = (150, 160, 170)

# expression file -> (English, Arabic, accent colour)
FACES = {
    "neutral": ("Ready", "جاهز", GREY),
    "listening": ("Listening", "أستمع", CYAN),
    "thinking": ("Thinking", "أفكر", (255, 196, 64)),
    "speaking": ("Speaking", "أتكلم", CYAN),
    "happy": ("Done", "تم", (80, 220, 120)),
    "concerned": ("Problem", "في مشكلة", (244, 80, 94)),
    "surprised": ("New message", "رسالة جديدة", (255, 196, 64)),
    "celebrating": ("Hello!", "أهلاً", (80, 220, 120)),
}

EN_FONT = "/System/Library/Fonts/SFNSRounded.ttf"
AR_FONT = "/System/Library/Fonts/SFArabicRounded.ttf"


def font(path, size):
    try:
        return ImageFont.truetype(path, size)
    except OSError:
        return ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", size)


def ar(text):
    # Pillow with libraqm shapes Arabic and orders it itself; otherwise do it here.
    from PIL import features
    if features.check("raqm"):
        return text
    return get_display(arabic_reshaper.reshape(text))


def rgb565(img):
    out = bytearray()
    for r, g, b in img.get_flattened_data() if hasattr(img, 'get_flattened_data') else img.getdata():
        v = ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)
        out += bytes((v >> 8, v & 0xFF))
    return bytes(out)


def build(src_dir, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    en_f, ar_f = font(EN_FONT, 24), font(AR_FONT, 28)
    for name, (en, arabic, accent) in FACES.items():
        src = os.path.join(src_dir, f"{name}.png")
        if not os.path.isfile(src):
            print("missing", src)
            continue
        head = Image.open(src).convert("RGBA")
        head = head.crop(head.getbbox())
        scale = (W - 8) / head.width
        head = head.resize((W - 8, int(head.height * scale)), Image.LANCZOS)
        canvas = Image.new("RGB", (W, H), BG)
        # Head centred in the top area; label always at the same height.
        top = 24 + max(0, (186 - head.height) // 2)
        canvas.paste(head, ((W - head.width) // 2, top), head)
        d = ImageDraw.Draw(canvas)
        y = 222
        d.rounded_rectangle((W // 2 - 22, y, W // 2 + 22, y + 4), 2, fill=accent)
        y += 16
        for text, f, colour in ((en, en_f, (235, 238, 242)), (ar(arabic), ar_f, accent)):
            tw = d.textlength(text, font=f)
            d.text(((W - tw) / 2, y), text, font=f, fill=colour)
            y += f.size + 14
        canvas.save(os.path.join(out_dir, f"{name}.png"))
        with open(os.path.join(out_dir, f"{name}.raw"), "wb") as fh:
            fh.write(rgb565(canvas))
        print("built", name)


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    build(sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "art"), os.path.join(here, "faces"))
