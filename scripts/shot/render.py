#!/usr/bin/env python3
"""ANSI (tmux `capture-pane -e`) → PNG, one cell per glyph, with Pillow.

Understands the SGR codes nebula emits through crossterm: reset, bold, dim, italic, underline, reverse,
16 colours, 256-colour and truecolour foreground/background. Everything else is ignored, never fatal.

    python3 render.py capture.ansi out.png [--cols 190] [--rows 50] [--font F.ttf] [--size 13] [--scale 2]

Box drawing (U+2500–257F) and block elements (U+2580–259F) are drawn as shapes on the cell grid, so
borders join with no gaps whatever the line height. A glyph the main font lacks comes from the first
fallback font that has it (fontTools reads the cmaps; without it every glyph comes from the main font).
The font is --font, else $SHOT_FONT, else the first of FONT_CANDIDATES on disk; the bold face is the
font's `-Bold` sibling when there is one.
"""
import argparse
import os
import re
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit("render.py needs Pillow: python3 -m pip install pillow (shot.sh makes a venv for this)")

try:
    from fontTools.ttLib import TTFont
except ImportError:  # fallbacks off: every glyph from the main font
    TTFont = None

FONT_CANDIDATES = [
    "/fonts/geist/geist-font/GeistMono/ttf/GeistMono-Regular.ttf",
    "/usr/share/fonts/truetype/geist/GeistMono-Regular.ttf",
    "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
]
FALLBACK_CANDIDATES = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansMath-Regular.ttf",
    "/usr/share/fonts/truetype/ancient-scripts/Symbola_hint.ttf",
]

ANSI16 = [
    (0, 0, 0), (205, 49, 49), (13, 188, 121), (229, 229, 16), (36, 114, 200), (188, 63, 188), (17, 168, 205), (229, 229, 229),
    (102, 102, 102), (241, 76, 76), (35, 209, 139), (245, 245, 67), (59, 142, 234), (214, 112, 214), (41, 184, 219), (255, 255, 255),
]
SGR = re.compile(r"\x1b\[([0-9;:]*)m")
OTHER_ESC = re.compile(r"\x1b\][^\x07]*\x07|\x1b\[[0-9;?]*[A-Za-z]|\x1b[()][A-Za-z0-9]")
# Kete Workshop dark: bone on soot, what the default theme paints.
DEFAULT_FG = (237, 230, 218)
DEFAULT_BG = (22, 20, 18)


def xterm256(n):
    if n < 16:
        return ANSI16[n]
    if n < 232:
        n -= 16
        r, g, b = n // 36, (n // 6) % 6, n % 6
        return tuple(0 if v == 0 else 55 + v * 40 for v in (r, g, b))
    v = 8 + (n - 232) * 10
    return (v, v, v)


def wide(ch):
    o = ord(ch)
    return (0x1100 <= o <= 0x115F or 0x2E80 <= o <= 0xA4CF or 0xAC00 <= o <= 0xD7A3 or 0xF900 <= o <= 0xFAFF
            or 0xFE30 <= o <= 0xFE4F or 0xFF00 <= o <= 0xFF60 or 0xFFE0 <= o <= 0xFFE6 or 0x1F300 <= o <= 0x1FAFF)


def parse(text, cols, rows):
    """Rows of (char, fg, bg, bold, dim, underline, italic) cells."""
    blank = (" ", DEFAULT_FG, DEFAULT_BG, False, False, False, False)
    grid = []
    state = dict(fg=None, bg=None, bold=False, dim=False, ul=False, rev=False, it=False)
    for line in text.split("\n")[:rows]:
        cells = []
        i = 0
        while i < len(line) and len(cells) < cols:
            if line[i] == "\x1b":
                m = SGR.match(line, i)
                if m:
                    codes = [int(c) if c.isdigit() else 0 for c in re.split("[;:]", m.group(1))] or [0]
                    j = 0
                    while j < len(codes):
                        c = codes[j]
                        if c == 0:
                            state.update(fg=None, bg=None, bold=False, dim=False, ul=False, rev=False, it=False)
                        elif c == 1: state["bold"] = True
                        elif c == 2: state["dim"] = True
                        elif c == 3: state["it"] = True
                        elif c == 4: state["ul"] = True
                        elif c == 7: state["rev"] = True
                        elif c == 22: state["bold"] = state["dim"] = False
                        elif c == 23: state["it"] = False
                        elif c == 24: state["ul"] = False
                        elif c == 27: state["rev"] = False
                        elif 30 <= c <= 37: state["fg"] = ANSI16[c - 30]
                        elif 90 <= c <= 97: state["fg"] = ANSI16[c - 90 + 8]
                        elif 40 <= c <= 47: state["bg"] = ANSI16[c - 40]
                        elif 100 <= c <= 107: state["bg"] = ANSI16[c - 100 + 8]
                        elif c == 39: state["fg"] = None
                        elif c == 49: state["bg"] = None
                        elif c in (38, 48, 58) and j + 1 < len(codes):
                            key = {38: "fg", 48: "bg", 58: None}[c]
                            if codes[j + 1] == 5 and j + 2 < len(codes):
                                if key: state[key] = xterm256(codes[j + 2])
                                j += 2
                            elif codes[j + 1] == 2 and j + 4 < len(codes):
                                if key: state[key] = tuple(codes[j + 2:j + 5])
                                j += 4
                        j += 1
                    i = m.end()
                    continue
                m = OTHER_ESC.match(line, i)
                i = m.end() if m else i + 1
                continue
            ch = line[i]
            fg = state["fg"] or DEFAULT_FG
            bg = state["bg"] or DEFAULT_BG
            if state["rev"]:
                fg, bg = bg, fg
            cells.append((ch, fg, bg, state["bold"], state["dim"], state["ul"], state["it"]))
            if wide(ch) and len(cells) < cols:
                cells.append(("", fg, bg, False, False, False, False))  # the glyph's second cell
            i += 1
        while len(cells) < cols:
            cells.append(blank)
        grid.append(cells)
    while len(grid) < rows:
        grid.append([blank] * cols)
    return grid


# --- box drawing: each char as (up, right, down, left) weights, 0 none, 1 light, 2 heavy, 3 double ---
def _box_table():
    t = {}
    L, H, D = 1, 2, 3
    spec = {
        0x2500: (0, L, 0, L), 0x2501: (0, H, 0, H), 0x2502: (L, 0, L, 0), 0x2503: (H, 0, H, 0),
        0x250C: (0, L, L, 0), 0x250D: (0, H, L, 0), 0x250E: (0, L, H, 0), 0x250F: (0, H, H, 0),
        0x2510: (0, 0, L, L), 0x2511: (0, 0, L, H), 0x2512: (0, 0, H, L), 0x2513: (0, 0, H, H),
        0x2514: (L, L, 0, 0), 0x2515: (L, H, 0, 0), 0x2516: (H, L, 0, 0), 0x2517: (H, H, 0, 0),
        0x2518: (L, 0, 0, L), 0x2519: (L, 0, 0, H), 0x251A: (H, 0, 0, L), 0x251B: (H, 0, 0, H),
        0x251C: (L, L, L, 0), 0x251D: (L, H, L, 0), 0x2520: (H, L, H, 0), 0x2523: (H, H, H, 0),
        0x2524: (L, 0, L, L), 0x2525: (L, 0, L, H), 0x2528: (H, 0, H, L), 0x252B: (H, 0, H, H),
        0x252C: (0, L, L, L), 0x252F: (0, H, L, H), 0x2530: (0, L, H, L), 0x2533: (0, H, H, H),
        0x2534: (L, L, 0, L), 0x2537: (L, H, 0, H), 0x2538: (H, L, 0, L), 0x253B: (H, H, 0, H),
        0x253C: (L, L, L, L), 0x254B: (H, H, H, H), 0x253F: (L, H, L, H), 0x2542: (H, L, H, L),
        0x2550: (0, D, 0, D), 0x2551: (D, 0, D, 0), 0x2554: (0, D, D, 0), 0x2557: (0, 0, D, D),
        0x255A: (D, D, 0, 0), 0x255D: (D, 0, 0, D), 0x2560: (D, D, D, 0), 0x2563: (D, 0, D, D),
        0x2566: (0, D, D, D), 0x2569: (D, D, 0, D), 0x256C: (D, D, D, D),
        0x2574: (0, 0, 0, L), 0x2575: (L, 0, 0, 0), 0x2576: (0, L, 0, 0), 0x2577: (0, 0, L, 0),
        0x2578: (0, 0, 0, H), 0x2579: (H, 0, 0, 0), 0x257A: (0, H, 0, 0), 0x257B: (0, 0, H, 0),
        0x257C: (0, H, 0, L), 0x257D: (L, 0, H, 0), 0x257E: (0, L, 0, H), 0x257F: (H, 0, L, 0),
    }
    for k, v in spec.items():
        t[chr(k)] = v
    return t


BOX = _box_table()
ROUND = {"╭": (0, 1, 1, 0), "╮": (0, 0, 1, 1), "╯": (1, 0, 0, 1), "╰": (1, 1, 0, 0)}
DASHED = {"╌": (2, False, 1), "╍": (2, False, 2), "┄": (3, False, 1), "┅": (3, False, 2), "┈": (4, False, 1),
          "┉": (4, False, 2), "╎": (2, True, 1), "╏": (2, True, 2), "┆": (3, True, 1), "┇": (3, True, 2),
          "┊": (4, True, 1), "┋": (4, True, 2)}


def draw_box(draw, ch, x0, y0, cw, chh, color, scale):
    lw = max(1, int(round(scale)))
    hw = max(2, int(round(scale * 2.2)))
    cx = x0 + cw // 2
    cy = y0 + chh // 2
    x1, y1 = x0 + cw, y0 + chh

    def hseg(xa, xb, w, y=cy):
        top = y - w // 2
        if xb <= xa:
            return
        draw.rectangle([xa, top, xb - 1, top + w - 1], fill=color)

    def vseg(ya, yb, w, x=cx):
        left = x - w // 2
        if yb <= ya:
            return
        draw.rectangle([left, ya, left + w - 1, yb - 1], fill=color)

    if ch in DASHED:
        n, vertical, weight = DASHED[ch]
        w = lw if weight == 1 else hw
        if vertical:
            step = chh / n
            for k in range(n):
                vseg(int(y0 + k * step + step * 0.2), int(y0 + k * step + step * 0.8), w)
        else:
            step = cw / n
            for k in range(n):
                hseg(int(x0 + k * step + step * 0.15), int(x0 + k * step + step * 0.85), w)
        return True
    if ch in ROUND:
        up, right, down, left = ROUND[ch]
        r = min(cw, chh) // 2
        # straight legs out to the cell edges, an arc joining them around the centre
        if up: vseg(y0, cy - r + 1, lw)
        if down: vseg(cy + r, y1, lw)
        if left: hseg(x0, cx - r + 1, lw)
        if right: hseg(cx + r, x1, lw)
        half = lw // 2
        if right and down:
            bb, a = [cx - half, cy - half, cx + 2 * r - half, cy + 2 * r - half], (180, 270)
        elif left and down:
            bb, a = [cx - 2 * r + half, cy - half, cx + half, cy + 2 * r - half], (270, 360)
        elif left and up:
            bb, a = [cx - 2 * r + half, cy - 2 * r + half, cx + half, cy + half], (0, 90)
        else:
            bb, a = [cx - half, cy - 2 * r + half, cx + 2 * r - half, cy + half], (90, 180)
        draw.arc(bb, a[0], a[1], fill=color, width=lw)
        return True
    spec = BOX.get(ch)
    if not spec:
        return False
    up, right, down, left = spec
    if 3 in spec:  # double lines: two light strokes a gap apart
        gap = max(2, lw * 2)
        for off in (-gap, gap):
            if left or right:
                xa = x0 if left else cx - (gap if (up or down) else 0)
                xb = x1 if right else cx + (gap if (up or down) else 0)
                hseg(xa, xb + 1, lw, cy + off)
            if up or down:
                ya = y0 if up else cy - gap
                yb = y1 if down else cy + gap
                vseg(ya, yb + 1, lw, cx + off)
        return True
    wmax = max(hw if v == 2 else lw for v in spec if v) if any(spec) else lw
    if left: hseg(x0, cx + (wmax + 1) // 2, hw if left == 2 else lw)
    if right: hseg(cx - wmax // 2, x1, hw if right == 2 else lw)
    if up: vseg(y0, cy + (wmax + 1) // 2, hw if up == 2 else lw)
    if down: vseg(cy - wmax // 2, y1, hw if down == 2 else lw)
    return True


def draw_block(draw, ch, x0, y0, cw, chh, color, bg):
    o = ord(ch)
    x1, y1 = x0 + cw, y0 + chh
    if o == 0x2588:
        draw.rectangle([x0, y0, x1 - 1, y1 - 1], fill=color)
    elif 0x2581 <= o <= 0x2587:  # lower eighths
        h = int(round(chh * (o - 0x2580) / 8))
        draw.rectangle([x0, y1 - h, x1 - 1, y1 - 1], fill=color)
    elif o == 0x2580:
        draw.rectangle([x0, y0, x1 - 1, y0 + chh // 2 - 1], fill=color)
    elif 0x2589 <= o <= 0x258F:  # left eighths
        w = int(round(cw * (0x2590 - o) / 8))
        draw.rectangle([x0, y0, x0 + w - 1, y1 - 1], fill=color)
    elif o == 0x2590:
        draw.rectangle([x0 + cw // 2, y0, x1 - 1, y1 - 1], fill=color)
    elif o == 0x2594:
        draw.rectangle([x0, y0, x1 - 1, y0 + max(1, chh // 8) - 1], fill=color)
    elif o == 0x2595:
        draw.rectangle([x1 - max(1, cw // 8), y0, x1 - 1, y1 - 1], fill=color)
    elif o in (0x2591, 0x2592, 0x2593):
        a = {0x2591: 0.25, 0x2592: 0.5, 0x2593: 0.75}[o]
        mix = tuple(int(bg[k] + (color[k] - bg[k]) * a) for k in range(3))
        draw.rectangle([x0, y0, x1 - 1, y1 - 1], fill=mix)
    elif 0x2596 <= o <= 0x259F:  # quadrants: UL UR LL LR bits
        q = {0x2596: "0010", 0x2597: "0001", 0x2598: "1000", 0x2599: "1011", 0x259A: "1001", 0x259B: "1110",
             0x259C: "1101", 0x259D: "0100", 0x259E: "0110", 0x259F: "0111"}[o]
        mx, my = x0 + cw // 2, y0 + chh // 2
        boxes = [(x0, y0, mx, my), (mx, y0, x1, my), (x0, my, mx, y1), (mx, my, x1, y1)]
        for bit, (a, b, c, d) in zip(q, boxes):
            if bit == "1":
                draw.rectangle([a, b, c - 1, d - 1], fill=color)
    else:
        return False
    return True


class Fonts:
    def __init__(self, path, size, fallbacks):
        self.size = size
        self.main = ImageFont.truetype(path, size)
        self.bold = self._bold(path, size)
        self.italic = self._sibling(path, size, ["Italic"]) or self.main
        self.cmaps = {}
        self.main_cmap = self._cmap(path)
        self.missing = set()
        self.fallbacks = []
        for p in fallbacks:
            if os.path.isfile(p) and p != path:
                try:
                    self.fallbacks.append((ImageFont.truetype(p, size), self._cmap(p)))
                except Exception:
                    pass

    @staticmethod
    def _sibling(path, size, words):
        base = os.path.basename(path)
        if "Regular" not in base:
            return None
        for word in words:
            cand = os.path.join(os.path.dirname(path), base.replace("Regular", word))
            if os.path.isfile(cand):
                return ImageFont.truetype(cand, size)
        return None

    def _bold(self, path, size):
        f = self._sibling(path, size, ["SemiBold", "Bold"])
        if f:
            return f
        try:
            return ImageFont.truetype(path, size, index=1)
        except Exception:
            return self.main

    @staticmethod
    def _cmap(path):
        if TTFont is None:
            return None
        try:
            t = TTFont(path, fontNumber=0, lazy=True)
            return set(t.getBestCmap() or {})
        except Exception:
            return None

    def pick(self, ch, bold, italic):
        o = ord(ch)
        if self.main_cmap is None or o in self.main_cmap:
            return (self.bold if bold else self.italic if italic else self.main), False
        for font, cmap in self.fallbacks:
            if cmap and o in cmap:
                return font, True
        if self.main_cmap is not None:
            self.missing.add(ch)
        return self.main, False


def render(grid, out, font_path, size, scale, line_height, pad, fallbacks):
    px = int(round(size * scale))
    fonts = Fonts(font_path, px, fallbacks)
    cw = int(round(fonts.main.getlength("M")))
    chh = int(round(px * line_height))
    asc, desc = fonts.main.getmetrics()
    base_y = (chh - (asc + desc)) // 2  # centre the font's line box in the cell
    rows, cols = len(grid), len(grid[0])
    p = int(round(pad * scale))
    img = Image.new("RGB", (cols * cw + 2 * p, rows * chh + 2 * p), DEFAULT_BG)
    draw = ImageDraw.Draw(img)
    for y, row in enumerate(grid):
        for x, (c, fg, bg, b, dim, ul, it) in enumerate(row):
            x0, y0 = p + x * cw, p + y * chh
            if bg != DEFAULT_BG:
                draw.rectangle([x0, y0, x0 + cw - 1, y0 + chh - 1], fill=bg)
        for x, (c, fg, bg, b, dim, ul, it) in enumerate(row):
            if c in ("", " "):
                continue
            x0, y0 = p + x * cw, p + y * chh
            color = tuple((v + bgv) // 2 for v, bgv in zip(fg, bg)) if dim else fg
            if draw_box(draw, c, x0, y0, cw, chh, color, scale) or draw_block(draw, c, x0, y0, cw, chh, color, bg):
                pass
            else:
                font, fallback = fonts.pick(c, b, it)
                span = 2 if wide(c) else 1
                if fallback:
                    # centre a fallback glyph in its cell, shrunk to fit if it is wider
                    w = font.getlength(c)
                    if w > cw * span * 1.05:
                        font = ImageFont.truetype(font.path, max(6, int(px * cw * span / w)))
                        w = font.getlength(c)
                    fa, fd = font.getmetrics()
                    draw.text((x0 + (cw * span - w) / 2, y0 + (chh - (fa + fd)) // 2), c, font=font, fill=color)
                else:
                    draw.text((x0, y0 + base_y), c, font=font, fill=color)
            if ul:
                uy = y0 + chh - max(2, int(scale * 2))
                draw.rectangle([x0, uy, x0 + cw - 1, uy + max(1, int(scale)) - 1], fill=color)
    img.save(out, optimize=True)
    if fonts.missing:  # drawn as the main font's empty box: fix the font list or the text
        sys.stderr.write("render.py: no font has %s\n" % " ".join("%s (U+%04X)" % (c, ord(c)) for c in sorted(fonts.missing)))
    return img.size


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ansi"); ap.add_argument("png")
    ap.add_argument("--cols", type=int, default=190); ap.add_argument("--rows", type=int, default=50)
    ap.add_argument("--font", default=os.environ.get("SHOT_FONT"))
    ap.add_argument("--size", type=float, default=float(os.environ.get("SHOT_FONT_SIZE", 13)))
    ap.add_argument("--scale", type=float, default=float(os.environ.get("SHOT_SCALE", 1)))
    ap.add_argument("--line-height", type=float, default=float(os.environ.get("SHOT_LINE_HEIGHT", 1.35)))
    ap.add_argument("--pad", type=int, default=int(os.environ.get("SHOT_PAD", 0)), help="margin in px at scale 1")
    a = ap.parse_args()
    font = a.font or next((f for f in FONT_CANDIDATES if os.path.isfile(f)), None)
    if not font:
        sys.exit("render.py: no monospace font found; pass --font or set SHOT_FONT")
    text = open(a.ansi, encoding="utf-8", errors="replace").read()
    size = render(parse(text, a.cols, a.rows), a.png, font, a.size, a.scale, a.line_height, a.pad, FALLBACK_CANDIDATES)
    print("%s %dx%d" % (a.png, *size))


if __name__ == "__main__":
    main()
