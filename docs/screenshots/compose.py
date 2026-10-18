"""The README visuals from raw captures of the demo app (adb screencap) and of the PC window.

Run from the repository root:
    uv run --with pillow python docs/screenshots/compose.py <captures folder>
The folder holds home.png, sheet.png, automations.png, dry-run.png, remote.png (phone, full screen)
and desktop.png (the PC window). Writes the same names here, plus hero.png.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

GROUND = (0x15, 0x17, 0x1B, 255)
LINE = (0x34, 0x3A, 0x42, 255)
OUT = Path(__file__).parent
PHONES = ["home", "sheet", "automations", "dry-run", "remote"]

# The phone's status bar and gesture bar, at 1008 px wide: not part of the app.
STATUS, GESTURE = 0.050, 0.022


def rounded(image, radius, border=True):
    """`image` with rounded corners on transparency, and a hairline border."""
    image = image.convert("RGBA")
    mask = Image.new("L", image.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, *image.size), radius=radius, fill=255)
    out = Image.new("RGBA", image.size, (0, 0, 0, 0))
    out.paste(image, (0, 0), mask)
    if border:
        ImageDraw.Draw(out).rounded_rectangle((0, 0, image.width - 1, image.height - 1), radius=radius, outline=LINE, width=2)
    return out


def phone(path):
    image = Image.open(path)
    w, h = image.size
    return image.crop((0, round(h * STATUS), w, round(h * (1 - GESTURE))))


def fit(image, width):
    return image.resize((width, round(image.height * width / image.width)), Image.LANCZOS)


def main(captures):
    captures = Path(captures)
    shots = {name: phone(captures / f"{name}.png") for name in PHONES}
    shots["desktop"] = Image.open(captures / "desktop.png")
    for name, image in shots.items():
        rounded(fit(image, 600), 28).save(OUT / f"{name}.png", optimize=True)

    # Hero: three phones and the PC window on a graphite plate.
    height, gap, margin = 1100, 56, 72
    parts = [fit(shots[n], round(shots[n].width * height / shots[n].height)) for n in ("home", "automations", "remote")]
    desk = shots["desktop"]
    parts.append(fit(desk, round(desk.width * height / desk.height)))
    width = sum(p.width for p in parts) + gap * (len(parts) - 1) + 2 * margin
    hero = Image.new("RGBA", (width, height + 2 * margin), (0, 0, 0, 0))
    ImageDraw.Draw(hero).rounded_rectangle((0, 0, *hero.size), radius=40, fill=GROUND)
    x = margin
    for part in parts:
        hero.alpha_composite(rounded(part, 36), (x, margin))
        x += part.width + gap
    fit(hero, 2000).save(OUT / "hero.png", optimize=True)


if __name__ == "__main__":
    main(sys.argv[1])
