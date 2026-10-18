"""Launcher icons from the logo (1024 px, transparent background), for both flavors.

Run from app/: uv run --with pillow python tools/icon.py tools/logo.png
Writes the adaptive icon foreground (Android 8+) and the legacy square icons (Android 7);
the background is the app's ground color (res/values/colors.xml). The demo flavor's icon has a
DÉMO tag so the two apps can be told apart.
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont

GROUND = (0x15, 0x17, 0x1B, 255)
BLUE = (0x7F, 0xA8, 0xFF, 255)
DENSITIES = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
# Adaptive icons are 108 dp; launcher masks keep at least the 66 dp circle in the middle.
SAFE = 33 / 108


def reach(logo):
    """The logo's center (of its outline) and how far its farthest opaque pixel lies from it."""
    alpha = logo.getchannel("A")
    x0, y0, x1, y1 = alpha.getbbox()
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    w = logo.width
    far = max(
        ((i % w - cx) ** 2 + (i // w - cy) ** 2) ** 0.5
        for i, a in enumerate(alpha.tobytes())
        if a > 16
    )
    return cx, cy, far


def foreground(logo, size, demo):
    """The logo filling the safe circle of a size x size canvas (raised a little for the tag)."""
    cx, cy, far = reach(logo)
    radius = SAFE * size * (0.74 if demo else 0.96)
    k = radius / far
    scaled = logo.resize((round(logo.width * k), round(logo.height * k)), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    lift = 0.09 * size if demo else 0
    canvas.alpha_composite(scaled, (round(size / 2 - cx * k), round(size / 2 - lift - cy * k)))
    if demo:
        draw = ImageDraw.Draw(canvas)
        font = ImageFont.truetype("assets/fonts/BarlowSemiCondensed-SemiBold.ttf", round(size * 0.075))
        text = "DÉMO"
        l, t, r, b = draw.textbbox((0, 0), text, font=font)
        pad_x, pad_y = size * 0.035, size * 0.016
        w, h = r - l + 2 * pad_x, b - t + 2 * pad_y
        x, y = (size - w) / 2, size / 2 + SAFE * size * 0.56
        draw.rounded_rectangle((x, y, x + w, y + h), radius=h / 2, fill=BLUE)
        draw.text((x + pad_x - l, y + pad_y - t), text, font=font, fill=GROUND)
    return canvas


def main(path):
    logo = Image.open(path).convert("RGBA")
    for flavor, demo in (("main", False), ("demo", True)):
        big = foreground(logo, 432 * 2, demo)
        for name, scale in DENSITIES.items():
            res = f"android/app/src/{flavor}/res/mipmap-{name}"
            fg = big.resize((round(108 * scale),) * 2, Image.LANCZOS)
            # Legacy icons (48 dp): the visible 72 dp middle on the ground color.
            legacy = Image.new("RGBA", fg.size, GROUND)
            legacy.alpha_composite(fg)
            inset = round(18 * scale)
            legacy = legacy.crop((inset, inset, fg.width - inset, fg.height - inset))
            legacy = legacy.resize((round(48 * scale),) * 2, Image.LANCZOS)
            os.makedirs(res, exist_ok=True)
            fg.save(f"{res}/ic_launcher_foreground.png", optimize=True)
            legacy.convert("RGB").save(f"{res}/ic_launcher.png", optimize=True)


if __name__ == "__main__":
    main(sys.argv[1])
