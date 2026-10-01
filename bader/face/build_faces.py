"""Builds the Bader face images for the ESP32-C6 screen (172x320 RGB565).

Layout (pixels, top → bottom):
  idle / poses : full-body Bader 0–262, bottom strip 266–319 (app draws "next
                 meeting · unread mail" there; "Bader" until it does)
  work faces   : head 8–176, English + Arabic label 182–262, strip 266–319
                 (app draws what Bader is doing, e.g. "Reading Gmail…")
  approval     : head + "Approve? · موافقة؟", strip 266–319 shows the action

Output: bader/face/faces/<name>.raw (+ PNG previews).

    python3 build_faces.py            (art in ./art, poses in ./art/poses)
"""

import glob
import os
import sys

from PIL import Image, ImageDraw, ImageFont, features

W, H = 172, 320
STRIP_Y = 266
BG = (0, 0, 0)
INK = (235, 238, 242)
GREY = (150, 160, 170)
CYAN = (64, 224, 255)
AMBER = (255, 196, 64)
GREEN = (80, 220, 120)
RED = (244, 80, 94)

# expression file -> (English, Arabic, accent colour)
FACES = {
    "neutral": ("Ready", "جاهز", GREY),
    "listening": ("Listening", "أستمع", CYAN),
    "thinking": ("Thinking", "أفكر", AMBER),
    "working": ("Working", "أعمل", AMBER),
    "speaking": ("Speaking", "أتكلم", CYAN),
    "happy": ("Done", "تم", GREEN),
    "concerned": ("Problem", "في مشكلة", RED),
    "surprised": ("New message", "رسالة جديدة", AMBER),
    "celebrating": ("Hello!", "أهلاً", GREEN),
    "approval": ("Approve?", "موافقة؟", AMBER),
}
# Which expression art each face uses (default: same name).
ART = {"working": "thinking", "approval": "surprised"}

EN_FONT = "/System/Library/Fonts/SFNSRounded.ttf"
AR_FONT = "/System/Library/Fonts/SFArabicRounded.ttf"


def font(path, size):
    try:
        return ImageFont.truetype(path, size)
    except OSError:
        return ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", size)


def ar(text):
    # Pillow with libraqm shapes Arabic itself; otherwise shape it here.
    if features.check("raqm"):
        return text
    import arabic_reshaper
    from bidi.algorithm import get_display
    return get_display(arabic_reshaper.reshape(text))


def rgb565(img):
    pixels = img.get_flattened_data() if hasattr(img, "get_flattened_data") else img.getdata()
    out = bytearray()
    for r, g, b in pixels:
        v = ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)
        out += bytes((v >> 8, v & 0xFF))
    return bytes(out)


def save(canvas, out_dir, name):
    canvas.save(os.path.join(out_dir, f"{name}.png"))
    with open(os.path.join(out_dir, f"{name}.raw"), "wb") as fh:
        fh.write(rgb565(canvas))
    print("built", name)


def centered(d, y, text, f, colour):
    tw = d.textlength(text, font=f)
    d.text(((W - tw) / 2, y), text, font=f, fill=colour)


def body_screen(src, out_dir, name, en_f):
    """Full-body Bader (idle and poses), name in the bottom strip."""
    body = Image.open(src).convert("RGBA")
    body = body.crop(body.getbbox())
    box_w, box_h = W - 4, 252
    scale = min(box_w / body.width, box_h / body.height)
    body = body.resize((int(body.width * scale), int(body.height * scale)), Image.LANCZOS)
    canvas = Image.new("RGB", (W, H), BG)
    canvas.paste(body, ((W - body.width) // 2, 8 + (box_h - body.height) // 2), body)
    centered(ImageDraw.Draw(canvas), STRIP_Y + 14, "Bader", en_f, INK)
    save(canvas, out_dir, name)


def face_screen(src, out_dir, name, en, arabic, accent, en_f, ar_f):
    head = Image.open(src).convert("RGBA")
    head = head.crop(head.getbbox())
    scale = min((W - 12) / head.width, 166 / head.height)
    head = head.resize((int(head.width * scale), int(head.height * scale)), Image.LANCZOS)
    canvas = Image.new("RGB", (W, H), BG)
    canvas.paste(head, ((W - head.width) // 2, 8 + (168 - head.height) // 2), head)
    d = ImageDraw.Draw(canvas)
    d.rounded_rectangle((W // 2 - 22, 184, W // 2 + 22, 188), 2, fill=accent)
    centered(d, 196, en, en_f, INK)
    centered(d, 226, ar(arabic), ar_f, accent)
    # thin divider above the live strip
    d.line((24, STRIP_Y - 2, W - 24, STRIP_Y - 2), fill=(40, 44, 50))
    save(canvas, out_dir, name)


def build(src_dir, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    en_f, ar_f, name_f = font(EN_FONT, 22), font(AR_FONT, 26), font(EN_FONT, 24)
    idle = os.path.join(src_dir, "idle.png")
    if os.path.isfile(idle):
        body_screen(idle, out_dir, "idle", name_f)
    for pose in sorted(glob.glob(os.path.join(src_dir, "poses", "*.png"))):
        stem = os.path.splitext(os.path.basename(pose))[0].replace("-", "_")
        body_screen(pose, out_dir, f"pose_{stem}", name_f)
    for name, (en, arabic, accent) in FACES.items():
        src = os.path.join(src_dir, f"{ART.get(name, name)}.png")
        if not os.path.isfile(src):
            print("missing", src)
            continue
        face_screen(src, out_dir, name, en, arabic, accent, en_f, ar_f)


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    build(sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "art"), os.path.join(here, "faces"))
