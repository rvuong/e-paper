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


def tag(text, x, cy):
    """Line number in white on a black tag; returns the parts and the tag width."""
    size, pad_x, pad_y = 28, 12, 4
    w, h = text_width(text, size) + 2 * pad_x, size + 2 * pad_y
    return [
        f'<rect x="{x}" y="{cy - h / 2}" width="{w}" height="{h}" rx="6" fill="{BLACK}"/>',
        centered_text(text, size, x + w / 2, cy, WHITE),
    ], w


# Pictograms shown instead of the times (Tabler Icons, MIT licence), 24 px grid.
MOON = (
    '<path d="M12 3c.132 0 .263 0 .393 0a7.5 7.5 0 0 0 7.497 12.75a9 9 0 1 1 -7.89 -12.75z" '
    'fill="#000" stroke="#000" stroke-width="2" stroke-linejoin="round"/>'
)
WRENCH = (
    '<path d="M7 10h3v-3l-3.5 -3.5a6 6 0 0 1 8 8l6 6a2 2 0 0 1 -3 3l-6 -6a6 6 0 0 1 -8 -8l3.5 3.5" '
    'fill="none" stroke="#000" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>'
)


def pictogram(icon, right, top, size=96):
    """Pictogram right-aligned in a cell, like a time."""
    return f'<g transform="translate({right - size} {top}) scale({size / 24})">{icon}</g>'


def direction(top, lines, label, times):
    """Header row (tags then label, 40 px high), then the two departure times,
    or a pictogram (MOON or WRENCH) in the first cell."""
    x, row_h, cy = 24, 40, top + 20
    parts = []
    for i, line in enumerate(lines):
        tag_parts, w = tag(line, x, cy)
        parts += tag_parts
        x += w + (8 if i < len(lines) - 1 else 12)
    label_size, label_lh = 22, 22 * 1.15
    parts.append(text_path(label, label_size, x, baseline_for(cy - label_lh / 2, label_size, label_lh), BLACK))
    time_size, cell_w, cell_gap = 96, 275, 32
    time_top = top + row_h + 8
    if times in (MOON, WRENCH):
        return parts + [pictogram(times, 24 + cell_w, time_top)]
    for i, time in enumerate(times):
        right = 24 + i * (cell_w + cell_gap) + cell_w
        x = right - text_width(time, time_size)
        parts.append(text_path(time, time_size, x, baseline_for(time_top, time_size, time_size), BLACK))
    return parts


def phase2(clock, directions, states):
    parts = [text_path(clock, 64, 24, baseline_for(14, 64, 64), BLACK)]
    parts.append(f'<rect x="24" y="92" width="610" height="4" fill="{BLACK}"/>')
    parts.append(f'<rect x="630" y="48" width="4" height="384" fill="{BLACK}"/>')
    for top, (lines, label, times) in zip((124, 300), directions):
        parts += direction(top, lines, label, times)

    slot, circle_gap, x_circles, col_top = PHASE2_DIMS[0], 8, 666, 8
    for i, p in enumerate(PEOPLE):
        home, activity = states.get(p, (False, False))
        cy = col_top + slot / 2 + i * (slot + circle_gap)
        parts += person(x_circles + slot / 2, cy, p, home, activity, PHASE2_DIMS)
    return svg(parts)


DIRECTIONS = [
    (["319"], "Marché International de Rungis", ["8:08", "8:31"]),
    (["297", "401"], "Antony RER", ["8:12", "8:21"]),
]
OUTPUTS = {
    "phase1-r-home.svg": phase1({"R": (True, False)}),
    "phase1-all-away.svg": phase1({}),
    "phase1-activity-ring.svg": phase1({"R": (True, False), "H": (True, True)}),
    "phase2-buses-presence.svg": phase2("8:04", DIRECTIONS, {"R": (True, False), "H": (True, True)}),
    "phase2-end-of-service.svg": phase2(
        "21:15",
        [(["319"], "Marché International de Rungis", MOON), (["297", "401"], "Antony RER", ["21:22", "21:40"])],
        {"R": (True, False), "H": (True, True)},
    ),
    "phase2-bus-data-unavailable.svg": phase2(
        "8:04",
        [(["319"], "Marché International de Rungis", WRENCH), (["297", "401"], "Antony RER", WRENCH)],
        {"R": (True, False), "H": (True, True)},
    ),
}
OUT_DIR.mkdir(parents=True, exist_ok=True)
for name, content in OUTPUTS.items():
    (OUT_DIR / name).write_text(content)
    print(name)
