"""The README's footprint chart, from the numbers measured by run.sh (edit them when you re-run it).

Run from the repository root: uv run --with pillow python docs/footprint/chart.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# Measured on 2026-10-18 (HA 2026.10.2 vs HomeMaid 0.1.0, Docker on WSL2, 8 cores).
MEASURES = [
    ("Memory at idle", 472, 26, "less", lambda v: f"{v:.0f} MB"),
    ("Install size", 3460, 35.3, "smaller", lambda v: f"{v / 1000:.2f} GB" if v >= 1000 else f"{v:.0f} MB"),
    ("Startup, until the API answers", 3.58, 0.015, "faster", lambda v: f"{v:.1f} s" if v >= 1 else f"{v * 1000:.0f} ms"),
]

GROUND, PANEL, LINE = (0x15, 0x17, 0x1B), (0x1C, 0x1F, 0x24), (0x34, 0x3A, 0x42)
TEXT, DIM = (0xEE, 0xF0, 0xF2), (0xA2, 0xA9, 0xB3)
LAVENDER, GREY = (0xA6, 0x99, 0xDC), (0x5A, 0x61, 0x6B)
FONTS = Path(__file__).parents[2] / "app/assets/fonts"


def font(name, size):
    return ImageFont.truetype(str(FONTS / name), size)


def main():
    scale = 2
    w, pad, label_w = 1000, 40, 150
    row_h, bar_h, gap = 150, 30, 14
    h = pad * 2 + 50 + row_h * len(MEASURES)
    image = Image.new("RGB", (w * scale, h * scale), GROUND)
    d = ImageDraw.Draw(image)
    s = lambda *v: tuple(round(x * scale) for x in v)
    d.rounded_rectangle(s(0, 0, w - 1, h - 1), radius=20 * scale, fill=PANEL, outline=LINE, width=scale)

    title, small, value = font("Barlow-SemiBold.ttf", 22 * scale), font("BarlowSemiCondensed-Medium.ttf", 18 * scale), font("Barlow-SemiBold.ttf", 18 * scale)
    # Legend.
    x = pad
    for name, color in (("HomeMaid", LAVENDER), ("Home Assistant", GREY)):
        d.rounded_rectangle(s(x, pad + 4, x + 18, pad + 22), radius=4 * scale, fill=color)
        d.text(s(x + 28, pad), name, font=small, fill=TEXT)
        x += 28 + d.textlength(name, font=small) / scale + 36

    y = pad + 50
    for name, ha, hm, better, fmt in MEASURES:
        d.text(s(pad, y), name, font=title, fill=TEXT)
        ratio = ha / hm
        d.text(s(w - pad, y + 4), f"{ratio:.0f}× {better}" if ratio >= 2 else "", font=small, fill=LAVENDER, anchor="ra")
        top = y + 44
        for who, v, color in (("HomeMaid", hm, LAVENDER), ("Home Assistant", ha, GREY)):
            d.text(s(pad, top + 4), who, font=small, fill=DIM)
            full = w - 2 * pad - label_w - 110
            length = max(full * v / max(ha, hm), 4)
            x0 = pad + label_w
            d.rounded_rectangle(s(x0, top, x0 + length, top + bar_h), radius=6 * scale, fill=color)
            d.text(s(x0 + length + 12, top + 3), fmt(v), font=value, fill=TEXT)
            top += bar_h + gap
        y += row_h
    image.save(Path(__file__).with_name("footprint.png"), optimize=True)


if __name__ == "__main__":
    main()
