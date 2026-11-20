"""Draws the helper's program icon, assets/app.ico, the one Windows shows for the .exe.

    python tools/make-icon.py FONT [--preview preview.png]

FONT is Inter's variable font (any of its files with the weight axis: .ttf, .otf or .woff2), drawn at weight 800.
Needs Pillow. Run it again only when the design changes; the .ico is committed.

From 48 px up the icon is the LFG+ wordmark: "LFG" in white, and the speech bubble with its plus cut out standing in
for the "+", raised against the G's shoulder, on a dark rounded tile so it reads on any background. Smaller than that
"LFG" can't be read, so 16 to 40 px show the bubble alone, filling the square, as the tray does.
"""

import argparse
import sys

from PIL import Image, ImageDraw, ImageFont

ACCENT = (0x4A, 0x8F, 0xE3, 255)
TILE = (0x0F, 0x11, 0x15, 255)
WHITE = (255, 255, 255, 255)

SMALL = [16, 20, 24, 32, 40]
LARGE = [48, 64, 96, 128, 256]
MASTER = 1024  # every size is drawn at this size and scaled down, for smooth edges

# The bubble on its 32 by 32 grid, as in the website's favicon.
BODY = (2, 3, 30, 24)
BODY_RADIUS = 6
TAIL = [(14.2, 23.0), (14.2, 24.0), (8.2, 29.2), (7.6, 29.5), (6.9, 29.3), (6.5, 28.6), (6.5, 22.0)]
PLUS = [((16, 8.6), (16, 18.4)), ((11.1, 13.5), (20.9, 13.5))]
PLUS_WIDTH = 4.2

# The wordmark, in ems of the font size, as the website lays it out: the bubble is 0.86 em square, starts 0.06 em
# before the end of "LFG", and its top sits 0.34 em above the top of the line (1.203 em above the baseline).
BUBBLE_EM = 0.86
BUBBLE_OVERLAP_EM = 0.06
BUBBLE_TOP_EM = 1.203


def bubble_mask(size):
    """The bubble, white on black, filling a size by size square, with its plus cut out."""
    k = size / 32
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle([BODY[0] * k, BODY[1] * k, BODY[2] * k, BODY[3] * k], radius=BODY_RADIUS * k, fill=255)
    draw.polygon([(x * k, y * k) for x, y in TAIL], fill=255)
    width = PLUS_WIDTH * k
    for (x0, y0), (x1, y1) in PLUS:
        draw.line([(x0 * k, y0 * k), (x1 * k, y1 * k)], fill=0, width=round(width))
        for x, y in ((x0, y0), (x1, y1)):  # round caps
            draw.ellipse([x * k - width / 2, y * k - width / 2, x * k + width / 2, y * k + width / 2], fill=0)
    return mask


def bubble_icon(size):
    """The bubble alone, centred with a little margin, on a transparent square."""
    master = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    mask = bubble_mask(MASTER)
    # The drawing spans x 2..30 and y 3..29.5 of 32: shift it to the middle.
    shifted = Image.new("L", (MASTER, MASTER), 0)
    shifted.paste(mask, (0, round((32 - 29.5 - 3) / 2 * MASTER / 32)))
    master.paste(Image.new("RGBA", (MASTER, MASTER), ACCENT), (0, 0), shifted)
    return master.resize((size, size), Image.LANCZOS)


def wordmark_icon(font_path, size):
    """LFG+ on a dark rounded tile."""
    em = 400
    font = ImageFont.truetype(font_path, em)
    font.set_variation_by_axes([800])
    ascent = font.getmetrics()[0]
    text_width = font.getlength("LFG")
    left, top, right, bottom = font.getbbox("LFG")  # relative to the line's top, ascent above the baseline
    baseline = ascent
    bubble = round(BUBBLE_EM * em)
    bubble_x = round(text_width - BUBBLE_OVERLAP_EM * em)
    bubble_y = round(baseline - BUBBLE_TOP_EM * em)
    # The bubble's drawn part within its square: x 2..30, y 3..29.5 of 32.
    span = (
        min(left, bubble_x + bubble * 2 / 32),
        min(top, bubble_y + bubble * 3 / 32),
        max(right, bubble_x + bubble * 30 / 32),
        max(bottom, bubble_y + bubble * 29.5 / 32),
    )
    width, height = span[2] - span[0], span[3] - span[1]
    canvas = Image.new("RGBA", (round(width) + 2, round(height) + 2), (0, 0, 0, 0))
    ox, oy = -span[0] + 1, -span[1] + 1
    ImageDraw.Draw(canvas).text((ox, oy), "LFG", font=font, fill=WHITE)
    canvas.paste(Image.new("RGBA", (bubble, bubble), ACCENT), (round(bubble_x + ox), round(bubble_y + oy)), bubble_mask(bubble))

    master = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    ImageDraw.Draw(master).rounded_rectangle([0, 0, MASTER - 1, MASTER - 1], radius=MASTER * 0.2, fill=TILE)
    inner = MASTER * 0.78  # the wordmark's width on the tile
    scale = inner / canvas.width
    mark = canvas.resize((round(canvas.width * scale), round(canvas.height * scale)), Image.LANCZOS)
    master.alpha_composite(mark, ((MASTER - mark.width) // 2, (MASTER - mark.height) // 2))
    return master.resize((size, size), Image.LANCZOS)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("font")
    parser.add_argument("--out", default="assets/app.ico")
    parser.add_argument("--preview")
    args = parser.parse_args()

    images = [bubble_icon(s) for s in SMALL] + [wordmark_icon(args.font, s) for s in LARGE]
    largest = images[-1]
    largest.save(args.out, format="ICO", sizes=[(i.width, i.height) for i in images], append_images=images[:-1])
    print(f"wrote {args.out}: {', '.join(str(i.width) for i in images)} px")

    if args.preview:
        # Every size at its real pixels, on a light and a dark background.
        pad = 16
        width = pad + sum(i.width + pad for i in images)
        height = pad + 2 * (256 + pad)
        sheet = Image.new("RGBA", (width, height), (255, 255, 255, 255))
        for row, background in enumerate([(243, 243, 243, 255), (32, 32, 32, 255)]):
            y = pad + row * (256 + pad)
            ImageDraw.Draw(sheet).rectangle([0, y - pad // 2, width, y + 256 + pad // 2], fill=background)
            x = pad
            for image in images:
                sheet.alpha_composite(image, (x, y + 256 - image.height))
                x += image.width + pad
        sheet.save(args.preview)
        print(f"wrote {args.preview}")


if __name__ == "__main__":
    sys.exit(main())
