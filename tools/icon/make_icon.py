"""Generate the Amway CCS Tracker app icon: a teal rounded-square tile with a
white "CCS" monogram. Writes assets/icons/app.png (256) and app.ico (multi-size).
Run: py tools/icon/make_icon.py"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "icons"
FONT = ROOT / "assets" / "fonts" / "Kanit-Medium.ttf"

SIZE = 256
TEAL = (0x00, 0xBC, 0xD4, 255)  # in-app ACCENT
WHITE = (255, 255, 255, 255)
RADIUS = 52
TEXT = "CCS"


def render(size: int) -> Image.Image:
    # Render at 256 then downscale for crisp small sizes.
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=RADIUS, fill=TEAL)
    font = ImageFont.truetype(str(FONT), 92)
    box = d.textbbox((0, 0), TEXT, font=font)
    tw, th = box[2] - box[0], box[3] - box[1]
    pos = ((SIZE - tw) / 2 - box[0], (SIZE - th) / 2 - box[1])
    d.text(pos, TEXT, font=font, fill=WHITE)
    if size != SIZE:
        img = img.resize((size, size), Image.LANCZOS)
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    base = render(SIZE)
    base.save(OUT / "app.png")
    sizes = [16, 32, 48, 64, 128, 256]
    base.save(OUT / "app.ico", sizes=[(s, s) for s in sizes])
    print(f"wrote {OUT / 'app.png'} and {OUT / 'app.ico'}")


if __name__ == "__main__":
    main()
