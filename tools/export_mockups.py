"""Export the e-paper mockups as standalone SVG files (text converted to paths).

Geometry mirrors the Claude Design canvas mockups (see CLAUDE.md, "Mockups").

Requirements:
    pip install fonttools   (in a virtualenv)
    Atkinson Hyperlegible Bold TTF, e.g. from
    https://github.com/google/fonts/raw/main/ofl/atkinsonhyperlegible/AtkinsonHyperlegible-Bold.ttf

Usage:
    python tools/export_mockups.py <path/to/AtkinsonHyperlegible-Bold.ttf> docs/mockups
"""
import sys
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

FONT_PATH, OUT_DIR = Path(sys.argv[1]), Path(sys.argv[2])
font = TTFont(FONT_PATH)
UPM = font["head"].unitsPerEm
ASC = font["hhea"].ascent / UPM
DESC = -font["hhea"].descent / UPM
CMAP = font.getBestCmap()
GLYPHS = font.getGlyphSet()
HMTX = font["hmtx"]

W, H = 800, 480
BLACK, WHITE = "#000", "#fff"


def text_width(text, size):
    return sum(HMTX[CMAP[ord(c)]][0] for c in text) * size / UPM


def text_path(text, size, x, baseline, fill):
    """Return an SVG path for text starting at x on the given baseline."""
    scale = size / UPM
    pen = SVGPathPen(GLYPHS)
    cursor = 0
    for c in text:
        name = CMAP[ord(c)]
        tpen = TransformPen(pen, (scale, 0, 0, -scale, x + cursor * scale, baseline))
        GLYPHS[name].draw(tpen)
        cursor += HMTX[name][0]
    return f'<path d="{pen.getCommands()}" fill="{fill}"/>'


def baseline_for(line_top, size, line_height):
    """CSS baseline position of a line box of the given height."""
    half_leading = (line_height - (ASC + DESC) * size) / 2
    return line_top + half_leading + ASC * size


def centered_text(text, size, cx, cy, fill):
    x = cx - text_width(text, size) / 2
    return text_path(text, size, x, baseline_for(cy - size / 2, size, size), fill)


def ring(cx, cy, box, stroke):
    return f'<circle cx="{cx}" cy="{cy}" r="{box / 2 - stroke / 2}" fill="none" stroke="{BLACK}" stroke-width="{stroke}"/>'


def person(cx, cy, letter, home, activity, dims):
    slot, core, core_stroke, inner_inset, ring_stroke, letter_size = dims
    parts = []
    if activity:
        parts.append(ring(cx, cy, slot, ring_stroke))
        parts.append(ring(cx, cy, slot - 2 * inner_inset, ring_stroke))
    fill = BLACK if home else WHITE
    parts.append(
        f'<circle cx="{cx}" cy="{cy}" r="{core / 2 - core_stroke / 2}" fill="{fill}" '
        f'stroke="{BLACK}" stroke-width="{core_stroke}"/>'
    )
    parts.append(centered_text(letter, letter_size, cx, cy, WHITE if home else BLACK))
    return parts


PEOPLE = ["R", "M", "H", "L"]
PHASE1_DIMS = (178, 130, 12, 12, 6, 88)
PHASE2_DIMS = (110, 74, 8, 9, 5, 50)


def svg(parts):
    body = "\n  ".join(parts)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="163.2mm" height="97.92mm" viewBox="0 0 {W} {H}">\n'
        f'  <rect width="{W}" height="{H}" fill="{WHITE}"/>\n  {body}\n</svg>\n'
    )


def phase1(states):
    slot, gap = PHASE1_DIMS[0], 16
    x0 = (W - (4 * slot + 3 * gap)) / 2
    parts = []
    for i, p in enumerate(PEOPLE):
        home, activity = states.get(p, (False, False))
        parts += person(x0 + slot / 2 + i * (slot + gap), H / 2, p, home, activity, PHASE1_DIMS)
    return svg(parts)


def wrap(text, size, max_width):
    lines, current = [], ""
    for word in text.split():
        candidate = f"{current} {word}".strip()
        if current and text_width(candidate, size) > max_width:
            lines.append(current)
            current = word
        else:
            current = candidate
    return lines + [current]


def phase2(buses, states):
    tag_size, tag_pad_x, tag_pad_y, tag_gap = 36, 16, 6, 16
    dir_size, dir_lh, dir_max = 26, 26 * 1.15, 260
    time_size, block_gap, zone_gap, sep_w = 96, 12, 56, 4
    slot, circle_gap = PHASE2_DIMS[0], 8

    blocks = []
    for line, direction, time in buses:
        tag_w = text_width(line, tag_size) + 2 * tag_pad_x
        lines = wrap(direction, dir_size, dir_max)
        dir_w = min(text_width(direction, dir_size), dir_max)
        row_w = tag_w + tag_gap + dir_w
        row_h = max(tag_size + 2 * tag_pad_y, len(lines) * dir_lh)
        blocks.append((line, lines, time, tag_w, row_w, row_h))

    left_w = max(max(b[4] for b in blocks), max(text_width(t, time_size) for _, _, t in buses))
    total = left_w + zone_gap + sep_w + zone_gap + slot
    x_left = (W - total) / 2
    x_sep = x_left + left_w + zone_gap
    x_circles = x_sep + sep_w + zone_gap

    parts = []
    half = H / len(blocks)
    for i, (line, lines, time, tag_w, row_w, row_h) in enumerate(blocks):
        content_h = row_h + block_gap + time_size
        top = i * half + (half - content_h) / 2
        row_cy = top + row_h / 2
        tag_h = tag_size + 2 * tag_pad_y
        parts.append(
            f'<rect x="{x_left}" y="{row_cy - tag_h / 2}" width="{tag_w}" height="{tag_h}" rx="6" fill="{BLACK}"/>'
        )
        parts.append(centered_text(line, tag_size, x_left + tag_w / 2, row_cy, WHITE))
        dir_x = x_left + tag_w + tag_gap
        dir_top = row_cy - len(lines) * dir_lh / 2
        for j, text in enumerate(lines):
            parts.append(text_path(text, dir_size, dir_x, baseline_for(dir_top + j * dir_lh, dir_size, dir_lh), BLACK))
        time_top = top + row_h + block_gap
        time_x = x_left + left_w - text_width(time, time_size)
        parts.append(text_path(time, time_size, time_x, baseline_for(time_top, time_size, time_size), BLACK))

    parts.append(f'<rect x="{x_sep}" y="{H / 4}" width="{sep_w}" height="{H / 2}" fill="{BLACK}"/>')

    col_top = (H - (4 * slot + 3 * circle_gap)) / 2
    for i, p in enumerate(PEOPLE):
        home, activity = states.get(p, (False, False))
        cy = col_top + slot / 2 + i * (slot + circle_gap)
        parts += person(x_circles + slot / 2, cy, p, home, activity, PHASE2_DIMS)
    return svg(parts)


BUSES = [("319", "Marché International de Rungis", "8:08"), ("297", "Antony RER", "8:17")]
OUTPUTS = {
    "phase1-r-home.svg": phase1({"R": (True, False)}),
    "phase1-all-away.svg": phase1({}),
    "phase1-activity-ring.svg": phase1({"R": (True, False), "H": (True, True)}),
    "phase2-buses-presence.svg": phase2(BUSES, {"R": (True, False), "H": (True, True)}),
}
OUT_DIR.mkdir(parents=True, exist_ok=True)
for name, content in OUTPUTS.items():
    (OUT_DIR / name).write_text(content)
    print(name)
