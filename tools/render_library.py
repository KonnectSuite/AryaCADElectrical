"""Draw a contact sheet of a symbol library, so the library can be reviewed.

The sheet is drawn from the same DXF files the app loads (`library/<set>/*.dxf`),
not from the generator, so it shows what a user of the library would see: the
geometry, the identifying letter inside a symbol, and the block name under it.

Usage
-----
    py -3.13 tools/render_library.py --library library/nec \
        --out docs/screenshots/neca-symbol-sheet.png [--cls "NECA Switches"] \
        [--page 1] [--per-page 48]

`--cls` restricts the sheet to one palette class (exact match); `--page`
selects a page of `--per-page` tiles.  Without `--cls` the sheet holds every
symbol whose name starts with the `--prefix` (default: all).
"""

from __future__ import annotations

import argparse
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dxfio  # noqa: E402

from PIL import Image, ImageDraw, ImageFont  # noqa: E402

TILE = 150
COLUMNS = 8
MARGIN = 8
TITLE_HEIGHT = 14
BACKGROUND = (255, 255, 255)
INK = (24, 32, 40)
LABEL = (60, 60, 60)


def _load_font(size):
    for name in ("segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def _polyline_points(entity):
    """LWPOLYLINE vertices: repeated 10/20 pairs (``Entity.vertices`` is empty)."""
    points, x = [], None
    for code, value in entity.pairs:
        if code == 10:
            x = float(value)
        elif code == 20 and x is not None:
            points.append((x, float(value)))
            x = None
    return points


def _shape_ops(drawing):
    """The drawable ops of one symbol DXF, in drawing coordinates."""
    ops = []
    for entity in drawing.entities:
        kind = entity.kind
        if kind == "LINE":
            ops.append(("line", entity.f(10), entity.f(20), entity.f(11), entity.f(21)))
        elif kind == "CIRCLE":
            ops.append(("circle", entity.f(10), entity.f(20), entity.f(40)))
        elif kind == "ARC":
            ops.append(
                ("arc", entity.f(10), entity.f(20), entity.f(40), entity.f(50), entity.f(51))
            )
        elif kind == "LWPOLYLINE":
            points = _polyline_points(entity)
            if points:
                ops.append(("poly", points, bool(entity.i(70) & 1)))
        elif kind == "SOLID":
            corners = []
            for index in range(4):
                code = 10 + index
                if not entity.has(code):
                    break
                corners.append((entity.f(code), entity.f(code + 10)))
            if len(corners) >= 3:
                ops.append(("solid", corners))
        elif kind == "TEXT":
            ops.append(
                ("text", entity.s(1, ""), entity.f(10), entity.f(20), entity.f(40), entity.f(50))
            )
    return ops


def _text_box(value, x, y, height, rotation):
    """Rough extent of a DXF TEXT: the anchor is its baseline start."""
    width = 0.7 * height * max(len(value), 1)
    angle = rotation % 360.0
    if 90.0 < angle < 270.0:
        return x - width, y - height, x, y
    return x, y, x + width, y + height


def _bounds(ops):
    min_x = min_y = math.inf
    max_x = max_y = -math.inf
    for op in ops:
        if op[0] == "line":
            xs, ys = [op[1], op[3]], [op[2], op[4]]
        elif op[0] == "circle":
            xs, ys = [op[1] - op[3], op[1] + op[3]], [op[2] - op[3], op[2] + op[3]]
        elif op[0] == "arc":
            xs, ys = [op[1] - op[3], op[1] + op[3]], [op[2] - op[3], op[2] + op[3]]
        elif op[0] in ("poly", "solid"):
            xs = [p[0] for p in op[1]]
            ys = [p[1] for p in op[1]]
        elif op[0] == "text":
            # The letter is part of the symbol (a thermostat's "T"), so it is
            # part of the extent the tile is scaled to.
            _, value, x, y, height, rotation = op
            x0, y0, x1, y1 = _text_box(value, x, y, height, rotation)
            xs, ys = [x0, x1], [y0, y1]
        else:
            continue
        min_x, max_x = min(min_x, min(xs)), max(max_x, max(xs))
        min_y, max_y = min(min_y, min(ys)), max(max_y, max(ys))
    if min_x is math.inf:
        return 0.0, 0.0, 1.0, 1.0
    return min_x, min_y, max_x, max_y


def _draw_ops(draw, ops, box):
    left, top, size = box
    min_x, min_y, max_x, max_y = _bounds(ops)
    span = max(max_x - min_x, max_y - min_y) or 1.0
    scale = size / span

    def place(x, y):
        return (
            left + (x - min_x) * scale + (size - (max_x - min_x) * scale) / 2,
            top + (max_y - y) * scale + (size - (max_y - min_y) * scale) / 2,
        )

    for op in ops:
        kind = op[0]
        if kind == "line":
            draw.line([place(op[1], op[2]), place(op[3], op[4])], fill=INK, width=1)
        elif kind == "circle":
            cx, cy = place(op[1], op[2])
            r = op[3] * scale
            draw.ellipse([cx - r, cy - r, cx + r, cy + r], outline=INK, width=1)
        elif kind == "arc":
            cx, cy = place(op[1], op[2])
            r = op[3] * scale
            start, end = op[4], op[5]
            # DXF arcs run counter-clockwise in a y-up frame; PIL draws in a
            # y-down frame, so the angles mirror.
            draw.arc(
                [cx - r, cy - r, cx + r, cy + r],
                start=-end,
                end=-start,
                fill=INK,
                width=1,
            )
        elif kind == "poly":
            points = [place(x, y) for x, y in op[1]]
            if op[2] and len(points) > 2:
                points.append(points[0])
            if len(points) > 1:
                draw.line(points, fill=INK, width=1)
        elif kind == "solid":
            points = [place(x, y) for x, y in op[1]]
            if len(points) >= 3:
                draw.polygon(points, fill=INK)
        elif kind == "text":
            _, value, x, y, height, rotation = op
            font = _load_font(max(int(height * scale), 5))
            cx, cy = place(x, y)
            angle = rotation % 360.0
            anchor = "rs" if 90.0 < angle < 270.0 else "ls"
            draw.text((cx, cy), value, fill=INK, font=font, anchor=anchor)


def render(library, out_path, title, page, per_page, prefix, cls):
    label_font = _load_font(11)
    title_font = _load_font(16)
    names = sorted(
        name[:-4] for name in os.listdir(library) if name.lower().endswith(".dxf")
    )
    if cls:
        manifest = os.path.join(library, "library.tsv")
        classes = {}
        if os.path.exists(manifest):
            with open(manifest, encoding="utf-8") as handle:
                for line in handle.read().splitlines()[1:]:
                    parts = line.split("\t")
                    if len(parts) >= 2:
                        classes[parts[0]] = parts[1]
        names = [name for name in names if classes.get(name) == cls]
    if prefix:
        names = [name for name in names if name.startswith(prefix)]
    pages = max(1, math.ceil(len(names) / per_page))
    page = min(page, pages)
    names = names[(page - 1) * per_page : page * per_page]

    rows = max(1, math.ceil(len(names) / COLUMNS))
    width = COLUMNS * TILE + 2 * MARGIN
    height = TITLE_HEIGHT + rows * (TILE + 16) + 2 * MARGIN
    image = Image.new("RGB", (width, height), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw.text((MARGIN, 4), title, fill=INK, font=title_font)

    for index, name in enumerate(names):
        row, column = divmod(index, COLUMNS)
        left = MARGIN + column * TILE
        top = MARGIN + TITLE_HEIGHT + row * (TILE + 16)
        # Each tile is drawn on its own image and pasted, so a symbol whose art
        # reaches the tile edge cannot bleed into its neighbour.
        tile = Image.new("RGB", (TILE, TILE), BACKGROUND)
        tile_draw = ImageDraw.Draw(tile)
        try:
            drawing = dxfio.load(os.path.join(library, name + ".dxf"))
        except Exception as error:  # a broken file must show up, not abort the sheet
            draw.rectangle([left, top, left + TILE, top + TILE], outline=(200, 0, 0))
            draw.text((left + 2, top + 2), "ERROR", fill=(200, 0, 0), font=label_font)
            draw.text((left + 2, top + 16), str(error)[:22], fill=(200, 0, 0), font=label_font)
            continue
        draw.rectangle(
            [left, top, left + TILE, top + TILE], outline=(226, 230, 234), width=1
        )
        _draw_ops(tile_draw, _shape_ops(drawing), (4, 4, TILE - 8))
        image.paste(tile, (left, top))
        label = name if len(name) <= 21 else name[:20] + "…"
        draw.text((left + 2, top + TILE + 2), label, fill=LABEL, font=label_font)

    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    image.save(out_path)
    print(f"wrote {out_path} ({len(names)} symbols, page {page}/{pages})")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", default=os.path.join("library", "nec"))
    parser.add_argument("--out", default=os.path.join("target", "tmp", "symbol-sheet.png"))
    parser.add_argument("--title", default="")
    parser.add_argument("--cls", default="")
    parser.add_argument("--prefix", default="")
    parser.add_argument("--page", type=int, default=1)
    parser.add_argument("--per-page", type=int, default=48)
    args = parser.parse_args(argv)
    title = args.title or ("Symbol library: " + args.library + (" - " + args.cls if args.cls else ""))
    render(args.library, args.out, title, args.page, args.per_page, args.prefix, args.cls)
    return 0


if __name__ == "__main__":
    sys.exit(main())
