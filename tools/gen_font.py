#!/usr/bin/env python3
"""Turns the letters and icons the mod draws in the world into triangles (src/font.rs), so they
stay smooth at any size instead of looking like pixels. Letters come from DejaVu Sans Bold, the
icons from DejaVu Sans (both under a free licence).

Each glyph's outline is read with fontTools, its curves cut into short straight pieces, and the
filled shape (with its holes) cut into triangles with earcut. Coordinates are stored in
sixteenths of a text pixel, y up from the baseline (letters) or from the icon cell's bottom."""
import pathlib
import mapbox_earcut as earcut
import numpy as np
from fontTools.ttLib import TTFont
from fontTools.pens.basePen import BasePen

ROOT = pathlib.Path(__file__).resolve().parents[1] / "mods/waypoints/src"
BOLD = TTFont("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
SYMBOLS = TTFont("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
SIZE = 11.0     # letters' em size in text pixels (as the old 11 px font)
H = 12          # text line height in pixels
ICON = 13       # icon cell size
UNIT = 16       # stored units per pixel
STEPS = 4       # straight pieces per curve, before simplifying
TOLERANCE = 0.12  # pixels a letter outline may move when simplified
ICON_TOLERANCE = 0.3

CHARS = [chr(c) for c in range(32, 127)] + list("ñÑáéíóúÁÉÍÓÚüÜ¿¡")
ICONS = ["★", "♥", "⌂", "⚑", "⚔", "☠", "✿", "☀", "☽", "♦", "♣", "♠", "♪", "✚", "▲", "●", "⚓", "☺", "⚡", "✖"]


class Flatten(BasePen):
    """Collects each contour as a list of points, curves cut into `STEPS` pieces."""

    def __init__(self, glyphs):
        super().__init__(glyphs)
        self.contours, self.cur = [], []

    def _moveTo(self, p):
        self.cur = [p]

    def _lineTo(self, p):
        self.cur.append(p)

    def _qCurveToOne(self, p1, p2):
        p0 = self.cur[-1]
        for i in range(1, STEPS + 1):
            t = i / STEPS
            a, b, c = (1 - t) ** 2, 2 * (1 - t) * t, t * t
            self.cur.append((a * p0[0] + b * p1[0] + c * p2[0], a * p0[1] + b * p1[1] + c * p2[1]))

    def _curveToOne(self, p1, p2, p3):
        p0 = self.cur[-1]
        for i in range(1, STEPS + 1):
            t = i / STEPS
            a, b, c, d = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t ** 3
            self.cur.append((a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]))

    def _closePath(self):
        if len(self.cur) > 1 and self.cur[0] == self.cur[-1]:
            self.cur.pop()
        if len(self.cur) >= 3:
            self.contours.append(self.cur)
        self.cur = []

    _endPath = _closePath


def area(c):
    return sum(c[i][0] * c[(i + 1) % len(c)][1] - c[(i + 1) % len(c)][0] * c[i][1] for i in range(len(c))) / 2


def inside(p, c):
    x, y, hit = p[0], p[1], False
    for i in range(len(c)):
        (x0, y0), (x1, y1) = c[i], c[(i + 1) % len(c)]
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            hit = not hit
    return hit


def simplify(c, tolerance):
    """Fewer points where the outline is nearly straight (`tolerance` in stored units)."""
    from shapely.geometry import LinearRing
    ring = LinearRing(c).simplify(tolerance, preserve_topology=True)
    pts = [(round(x), round(y)) for x, y in ring.coords]
    return pts[:-1] if len(pts) > 1 and pts[0] == pts[-1] else pts


def clean(c, tolerance):
    """Drops repeated and straight-through points (after rounding to stored units)."""
    c = simplify(c, tolerance * UNIT)
    out = []
    for p in c:
        if not out or out[-1] != p:
            out.append(p)
    if len(out) > 1 and out[0] == out[-1]:
        out.pop()
    changed = True
    while changed and len(out) > 3:
        changed = False
        for i in range(len(out)):
            a, b, c2 = out[i - 1], out[i], out[(i + 1) % len(out)]
            if (b[0] - a[0]) * (c2[1] - a[1]) - (b[1] - a[1]) * (c2[0] - a[0]) == 0:
                out.pop(i)
                changed = True
                break
    return out


def triangles(contours, tolerance=None):
    """Fills (counter-clockwise in y-up) with their holes (clockwise), cut into triangles."""
    contours = [clean(c, tolerance or TOLERANCE) for c in contours]
    contours = [c for c in contours if len(c) >= 3 and area(c) != 0]
    # TrueType draws filled outlines clockwise; with y up that is a negative area.
    fills = [c for c in contours if area(c) < 0]
    holes = [c for c in contours if area(c) > 0]
    verts, idx = [], []
    groups = [[f] for f in fills]
    for h in holes:
        owners = [g for g in groups if inside(h[0], g[0])]
        if owners:
            min(owners, key=lambda g: abs(area(g[0]))).append(h)
    for g in groups:
        flat, rings = [], []
        for ring in g:
            flat.extend(ring)
            rings.append(len(flat))
        tri = earcut.triangulate_float32(np.array(flat, dtype=np.float32).reshape(-1, 2), np.array(rings, dtype=np.uint32))
        base = len(verts)
        verts.extend(flat)
        # Each triangle counter-clockwise in y-up, so it faces +Z (the front).
        t = [int(v) for v in tri]
        for i in range(0, len(t), 3):
            a, b, c = flat[t[i]], flat[t[i + 1]], flat[t[i + 2]]
            cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
            if cross == 0:
                continue
            if cross > 0:
                idx.extend([base + t[i], base + t[i + 1], base + t[i + 2]])
            else:
                idx.extend([base + t[i], base + t[i + 2], base + t[i + 1]])
    return verts, idx


def outline(font, ch, scale, dx, dy):
    gs = font.getGlyphSet()
    name = font.getBestCmap()[ord(ch)]
    pen = Flatten(gs)
    gs[name].draw(pen)
    return [[(round((x * scale + dx) * UNIT), round((y * scale + dy) * UNIT)) for x, y in c] for c in pen.contours], gs[name].width * scale


def text_glyph(ch):
    scale = SIZE / BOLD["head"].unitsPerEm
    if ch == " ":
        return round(BOLD.getGlyphSet()[BOLD.getBestCmap()[32]].width * scale * UNIT), [], []
    contours, width = outline(BOLD, ch, scale, 0, 0)
    verts, idx = triangles(contours)
    return round(width * UNIT), verts, idx


def icon_glyph(ch):
    gs = SYMBOLS.getGlyphSet()
    pen = Flatten(gs)
    gs[SYMBOLS.getBestCmap()[ord(ch)]].draw(pen)
    xs = [p[0] for c in pen.contours for p in c]
    ys = [p[1] for c in pen.contours for p in c]
    scale = (ICON - 1) / max(max(xs) - min(xs), max(ys) - min(ys))  # fits the cell
    cx, cy = (max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2
    contours = [[(round(((x - cx) * scale + ICON / 2) * UNIT), round(((y - cy) * scale + ICON / 2) * UNIT)) for x, y in c] for c in pen.contours]
    return triangles(contours, ICON_TOLERANCE)


def slice_lit(pairs):
    return "&[" + ", ".join(f"({x}, {y})" for x, y in pairs) + "]"


def idx_lit(idx):
    return "&[" + ", ".join(str(i) for i in idx) + "]"


lines = ["//! Generated by tools/gen_font.py: the mod's letters and icons as triangles. Points are in",
         f"//! 1/{UNIT} of a text pixel, y up: from the baseline for letters, from the cell's bottom for icons.", "",
         f"pub const HEIGHT: u8 = {H};", f"pub const ICON: u8 = {ICON};", f"pub const UNIT: f32 = {UNIT}.0;",
         "/// How far the baseline sits above the bottom of a text line, in pixels.",
         "pub const BASELINE: f32 = 3.0;", "",
         "pub struct Glyph {", "    pub ch: char,", "    /// Advance in stored units.", "    pub advance: u16,",
         "    pub points: &'static [(i16, i16)],", "    pub triangles: &'static [u8],", "}", "",
         "pub struct Icon {", "    pub points: &'static [(i16, i16)],", "    pub triangles: &'static [u8],", "}", "",
         "pub const GLYPHS: &[Glyph] = &["]
total_v = 0
for ch in CHARS:
    adv, verts, idx = text_glyph(ch)
    assert len(verts) < 256, ch
    total_v += len(verts)
    lit = {"'": "'\\''", "\\": "'\\\\'"}.get(ch, "'" + ch + "'")
    lines.append(f"    Glyph {{ ch: {lit}, advance: {adv}, points: {slice_lit(verts)}, triangles: {idx_lit(idx)} }},")
lines.append("];")
lines.append("")
lines.append("/// The icons in the order of `config::ICONS`.")
lines.append(f"pub const ICONS: [Icon; {len(ICONS)}] = [")
for ch in ICONS:
    verts, idx = icon_glyph(ch)
    assert len(verts) < 256, ch
    lines.append(f"    Icon {{ points: {slice_lit(verts)}, triangles: {idx_lit(idx)} }},")
    print(ch, len(verts), len(idx) // 3)
lines.append("];")
text = "\n".join(lines) + "\n"
(ROOT / "font.rs").write_text(text)
print(len(CHARS), "glyphs,", total_v, "letter points,", len(text), "bytes")
