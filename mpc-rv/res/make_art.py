# mpc-rv artwork:
# - the app icon is MPC-HC's "321" icon (so users recognize the player) with a "REVAMP" badge added;
# - the idle-screen logo shows that same badged icon with the "mpc-rv" name, a "RUST REVAMP" badge
#   and a tagline, so the running player isn't mistaken for the original MPC-HC.
#
#   python3 res/make_art.py <mpc-hc res folder> res
#
# The first argument is MPC-HC's `src/mpc-hc/res` (for icon.ico); the outputs are res/logo.png and
# res/icons/{48,128,256}.png. Needs Pillow and the DejaVu fonts.
import os, sys
from PIL import Image, ImageDraw, ImageFont

SRC, OUT = sys.argv[1], sys.argv[2]
BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
REG = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
WHITE = (255, 255, 255)
# The "revamp" badges: GNOME's yellow, a darker golden edge, near-black text (white on yellow is hard to read).
YELLOW, YELLOW_EDGE, INK = (246, 211, 45), (200, 136, 0), (36, 31, 49)


def pill(text, height, k):
    """A yellow rounded badge with dark text, drawn at k x scale (downsample it for smooth edges)."""
    font = ImageFont.truetype(BOLD, int(height * 0.58 * k))
    pad = height * 0.42 * k
    w = int(ImageDraw.Draw(Image.new("RGBA", (1, 1))).textlength(text, font=font) + 2 * pad)
    h = int(height * k)
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([0, 0, w - 1, h - 1], radius=h / 2, fill=YELLOW + (255,), outline=YELLOW_EDGE + (255,), width=max(1, int(h * 0.04)))
    d.text((w / 2, h / 2 + k * 0.5), text, font=font, fill=INK + (255,), anchor="mm")
    return im


# Icon: MPC-HC's "321" icon (largest size in icon.ico), with a "REVAMP" badge just above the digits,
# on the right (the digits start at row 476 and end at column 848 of the 1024 px image).
ico = Image.open(os.path.join(SRC, "icon.ico"))
ico.size = max(ico.info["sizes"])
ico.load()
base = ico.convert("RGBA").resize((1024, 1024), Image.LANCZOS)
b = pill("REVAMP", 185, 1)
DIGITS_TOP, DIGITS_RIGHT = 476, 848
base.alpha_composite(b, (DIGITS_RIGHT - b.width + 100, DIGITS_TOP - b.height - 40))
os.makedirs(os.path.join(OUT, "icons"), exist_ok=True)
for size in (48, 128, 256):
    base.resize((size, size), Image.LANCZOS).save(os.path.join(OUT, "icons", f"{size}.png"))


# Logo (shown centered on the black video area while idle): the badged icon, name, badge and tagline.
NAME, TAGLINE = "mpc-rv", "Unofficial Media Player Classic rewrite"
K = 3  # drawn at 3x, saved at 1x
NAME_FONT, TAG_FONT = ImageFont.truetype(BOLD, 44 * K), ImageFont.truetype(REG, 11 * K)
_measure = ImageDraw.Draw(Image.new("RGBA", (1, 1)))
# Wide enough for the longer of the name and the tagline.
W, H = 122 + int(max(_measure.textlength(NAME, font=NAME_FONT), _measure.textlength(TAGLINE, font=TAG_FONT)) / K) + 12, 150
im = Image.new("RGBA", (W * K, H * K), (0, 0, 0, 0))
g = base.resize((112 * K, 112 * K), Image.LANCZOS)
im.alpha_composite(g, (2 * K, 19 * K))
d = ImageDraw.Draw(im)
x = 122 * K
d.text((x, 30 * K), NAME, font=NAME_FONT, fill=(240, 240, 240, 255))
badge_font = ImageFont.truetype(BOLD, 11 * K)
label = "RUST REVAMP"
tw = d.textlength(label, font=badge_font)
bx, by, bh = x + 2 * K, 88 * K, 20 * K
d.rounded_rectangle([bx, by, bx + tw + 18 * K, by + bh], radius=bh / 2, fill=YELLOW + (255,))
d.text((bx + 9 * K, by + bh / 2), label, font=badge_font, fill=INK + (255,), anchor="lm")
d.text((x + 2 * K, 116 * K), TAGLINE, font=TAG_FONT, fill=(150, 150, 150, 255))
im.resize((W, H), Image.LANCZOS).save(os.path.join(OUT, "logo.png"))
