"""Import the converted NECA 100-2024 drawings as symbol geometry.

Each drawing is plain art, so the geometry is read directly, scaled to the
10-unit design canvas and given two pins at the outermost lead ends (NECA
drawings carry no pin objects).  Output feeds ``make_library.py``.

The drawings use more than LINE/LWPOLYLINE: thermostats are a CIRCLE with a
TEXT letter, fixtures use ARC, and several symbols are assembled from block
references (INSERT) instead of loose geometry.  All of it is unfolded here, so
the library gets the drawing the standard prints rather than a partial one.

Run: ``py -3.13 tools/neca_symbols.py``
"""

import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dxfio  # noqa: E402

CANVAS = 10.0
ROOT = os.path.join("target", "tmp", "neca2024", "dxf2")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "neca_shapes.json")
MAX_DEPTH = 8

IDENTITY = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


# --------------------------------------------------------------------------- #
# 2D affine transform: (m00, m01, m10, m11, tx, ty)
# --------------------------------------------------------------------------- #


def apply(transform, point):
    m00, m01, m10, m11, tx, ty = transform
    x, y = point
    return (m00 * x + m01 * y + tx, m10 * x + m11 * y + ty)


def compose(parent, local):
    """parent after local."""
    pm00, pm01, pm10, pm11, ptx, pty = parent
    lm00, lm01, lm10, lm11, ltx, lty = local
    return (
        pm00 * lm00 + pm01 * lm10,
        pm00 * lm01 + pm01 * lm11,
        pm10 * lm00 + pm11 * lm10,
        pm10 * lm01 + pm11 * lm11,
        pm00 * ltx + pm01 * lty + ptx,
        pm10 * ltx + pm11 * lty + pty,
    )


def insert_transform(entity, block):
    """Transform from block-local coordinates to the parent's coordinates."""
    scale_x = entity.f(41, 1.0) or 1.0
    scale_y = entity.f(42, 1.0) or 1.0
    angle = math.radians(entity.f(50, 0.0))
    cosine, sine = math.cos(angle), math.sin(angle)
    m00, m01 = scale_x * cosine, -scale_y * sine
    m10, m11 = scale_x * sine, scale_y * cosine
    base_x, base_y = block.base[0], block.base[1]
    return (
        m00, m01, m10, m11,
        entity.f(10) - (m00 * base_x + m01 * base_y),
        entity.f(20) - (m10 * base_x + m11 * base_y),
    )


def scale_of(transform):
    m00, m01, m10, m11, _tx, _ty = transform
    return math.sqrt(abs(m00 * m11 - m01 * m10)) or 1.0


def angle_of(transform):
    m00, m01, m10, m11, _tx, _ty = transform
    return math.degrees(math.atan2(m10, m00))


# --------------------------------------------------------------------------- #
# entity -> geometry ops
# --------------------------------------------------------------------------- #


def polyline_points(entity):
    points, x = [], None
    for code, value in entity.pairs:
        if code == 10:
            x = float(value)
        elif code == 20 and x is not None:
            points.append((x, float(value)))
            x = None
    return points


def walk(entities, drawing, transform=IDENTITY, depth=0):
    """Geometry ops of one entity list, with block references unfolded."""
    ops = []
    for entity in entities:
        kind = entity.kind
        if kind == "LINE":
            x1, y1 = apply(transform, (entity.f(10), entity.f(20)))
            x2, y2 = apply(transform, (entity.f(11), entity.f(21)))
            ops.append(("line", x1, y1, x2, y2))
        elif kind == "LWPOLYLINE":
            points = [apply(transform, point) for point in polyline_points(entity)]
            closed = bool(entity.i(70) & 1)
            if len(points) >= 2:
                ops.append(("poly", points, closed))
        elif kind == "POLYLINE":
            points = [apply(transform, (v.f(10), v.f(20))) for v in entity.vertices]
            closed = bool(entity.i(70) & 1)
            if len(points) >= 2:
                ops.append(("poly", points, closed))
        elif kind == "CIRCLE":
            cx, cy = apply(transform, (entity.f(10), entity.f(20)))
            radius = entity.f(40) * scale_of(transform)
            if radius > 0:
                ops.append(("circle", cx, cy, radius))
        elif kind == "ARC":
            cx, cy = apply(transform, (entity.f(10), entity.f(20)))
            radius = entity.f(40) * scale_of(transform)
            rotation = angle_of(transform)
            if radius > 0:
                ops.append(("arc", cx, cy, radius,
                            entity.f(50) + rotation, entity.f(51) + rotation))
        elif kind == "TEXT":
            value = entity.s(1)
            if not value.strip():
                continue
            x, y = apply(transform, (entity.f(10), entity.f(20)))
            height = (entity.f(40) or 2.5) * scale_of(transform)
            if height <= 0:
                continue
            ops.append(("text", value.strip(), x, y, height,
                        entity.f(50) + angle_of(transform)))
        elif kind == "ATTDEF":
            # The drawings label a device with an attribute definition whose
            # default value is the Standard's letter (``TS``, ``EP``, ``HOA``).
            value = entity.s(1) or entity.s(2)
            if not value.strip():
                continue
            x, y = apply(transform, (entity.f(10), entity.f(20)))
            height = (entity.f(40) or 2.5) * scale_of(transform)
            if height <= 0:
                continue
            ops.append(("text", value.strip(), x, y, height,
                        entity.f(50) + angle_of(transform)))
        elif kind == "SOLID":
            corners = []
            for index in range(4):
                code = 10 + index
                if not entity.has(code):
                    break
                corners.append(apply(transform, (entity.f(code), entity.f(code + 10))))
            # A triangle is written with its fourth corner repeated.
            if len(corners) > 1 and corners[-1] == corners[-2]:
                corners.pop()
            if len(corners) >= 3:
                ops.append(("solid", corners))
        elif kind == "INSERT" and depth < MAX_DEPTH:
            block = drawing.blocks.get((entity.s(2) or "").upper())
            if block is None:
                continue
            ops.extend(walk(block.entities, drawing,
                            compose(transform, insert_transform(entity, block)),
                            depth + 1))
    return ops


# --------------------------------------------------------------------------- #
# normalisation and pins
# --------------------------------------------------------------------------- #


def sample(op):
    """Rough point cloud of one op, for bounds and pin placement."""
    kind = op[0]
    if kind == "line":
        return [(op[1], op[2]), (op[3], op[4])]
    if kind == "poly":
        return list(op[1])
    if kind == "solid":
        return list(op[1])
    if kind == "circle":
        _, cx, cy, radius = op
        return [(cx - radius, cy), (cx + radius, cy), (cx, cy - radius), (cx, cy + radius)]
    if kind == "arc":
        _, cx, cy, radius, start, end = op
        return dxfio.arc_points(cx, cy, radius, start, end, steps=16)
    if kind == "text":
        _, value, x, y, height, _rotation = op
        return [(x, y), (x + 0.7 * height * len(value), y + height)]
    return []


def trim_annotation(ops):
    """Drop the Standard's example annotation that sits outside the drawing.

    A NECA drawing is an *example*: besides the symbol it carries the annotation
    the contractor would add around it (an amperage ``A`` thirty units to the
    left, ``NEMA xx`` to the right).  Text inside or just beside the art is the
    symbol's own letter (a thermostat's ``T``, a time switch's ``TS``); text
    further away is annotation and does not belong in the library block.
    """
    drawable = [op for op in ops if op[0] != "text"]
    if not drawable:
        return ops
    points = [point for op in drawable for point in sample(op)]
    min_x = min(x for x, _ in points)
    max_x = max(x for x, _ in points)
    min_y = min(y for _, y in points)
    max_y = max(y for _, y in points)
    pad = 0.35 * (max(max_x - min_x, max_y - min_y) or 1.0)
    kept = []
    for op in ops:
        if op[0] == "text":
            x, y = op[2], op[3]
            if not (min_x - pad <= x <= max_x + pad and min_y - pad <= y <= max_y + pad):
                continue
        kept.append(op)
    return kept


def normalise(ops):
    points = [point for op in ops for point in sample(op)]
    xs = [x for x, _ in points]
    ys = [y for _, y in points]
    min_x, max_x, min_y, max_y = min(xs), max(xs), min(ys), max(ys)
    span = max(max_x - min_x, max_y - min_y) or 1.0
    scale = CANVAS / span
    cx, cy = (min_x + max_x) / 2.0, (min_y + max_y) / 2.0

    def place(point):
        return (round((point[0] - cx) * scale, 3), round((point[1] - cy) * scale, 3))

    geom = []
    for op in ops:
        kind = op[0]
        if kind == "line":
            (x1, y1), (x2, y2) = place((op[1], op[2])), place((op[3], op[4]))
            geom.append(["line", x1, y1, x2, y2])
        elif kind == "poly":
            geom.append(["poly", [list(place(point)) for point in op[1]], bool(op[2])])
        elif kind == "solid":
            geom.append(["solid", [list(place(point)) for point in op[1]]])
        elif kind == "circle":
            x, y = place((op[1], op[2]))
            geom.append(["circle", x, y, round(op[3] * scale, 3)])
        elif kind == "arc":
            x, y = place((op[1], op[2]))
            geom.append(["arc", x, y, round(op[3] * scale, 3),
                         round(op[4], 3), round(op[5], 3)])
        elif kind == "text":
            x, y = place((op[2], op[3]))
            geom.append(["text", op[1], x, y, round(op[4] * scale, 3),
                         round(op[5], 3)])

    left = min(x for x, _ in points)
    right = max(x for x, _ in points)
    left_y = [y for x, y in points if abs(x - left) < 1e-9]
    right_y = [y for x, y in points if abs(x - right) < 1e-9]
    left_pin = place((left, sum(left_y) / len(left_y)))
    right_pin = place((right, sum(right_y) / len(right_y)))
    pins = [
        [left_pin[0], left_pin[1], "X4TERM01"],
        [right_pin[0], right_pin[1], "X1TERM01"],
    ]
    return geom, pins


# --------------------------------------------------------------------------- #
# main
# --------------------------------------------------------------------------- #


def read(path):
    drawing = dxfio.load(path)
    return trim_annotation(walk(drawing.entities, drawing))


def _same_op(left, right, tolerance=0.05):
    """True when two ops draw the same thing in the same place."""
    if left[0] != right[0]:
        return False
    if left[0] == "text":
        return left[1] == right[1] and all(
            abs(left[i] - right[i]) <= tolerance for i in range(2, 5)
        )
    if left[0] == "circle":
        return all(abs(left[i] - right[i]) <= tolerance for i in range(1, 4))
    if left[0] in ("line", "arc"):
        return all(abs(left[i] - right[i]) <= tolerance for i in range(1, len(left)))
    if left[0] in ("poly", "solid"):
        return len(left[1]) == len(right[1]) and all(
            abs(a - b) <= tolerance
            for (ax, ay), (bx, by) in zip(left[1], right[1])
            for a, b in ((ax, bx), (ay, by))
        )
    return left == right


def merge_alternates(ops_by_key):
    """Fold an ``a``-alternate drawing into the base drawing it belongs to.

    Section 4 is printed twice per device: the base drawing is the wall line
    with the device letter ("S"), and the alternate drawing is the same letter
    plus the modifier that separates the variants ("S2", "SCB", "SDT").  Each
    alternate is text-only, so on its own it would be a floating label; what the
    palette wants is the base art plus the modifier, under the base's name.
    """
    merged = {}
    for key, ops in ops_by_key.items():
        if not key.endswith("a"):
            merged.setdefault(key, list(ops))
            continue
        base = key[:-1]
        base_ops = ops_by_key.get(base)
        if base_ops is None:
            merged.setdefault(key, list(ops))
            continue
        extra = [op for op in ops if not any(_same_op(op, b) for b in base_ops)]
        if not extra:
            continue  # an exact reprint of the base: keep one block, not two
        renamed = merged.setdefault(base, list(base_ops))
        renamed.extend(extra)
    return merged


def main():
    raw = {}
    skipped = []
    for section in sorted(os.listdir(ROOT)):
        folder = os.path.join(ROOT, section)
        if not os.path.isdir(folder):
            continue
        code = section.split(" ")[0]
        for name in sorted(os.listdir(folder)):
            if not name.lower().endswith(".dxf"):
                continue
            try:
                ops = read(os.path.join(folder, name))
            except Exception as error:  # noqa: BLE001 - report and continue
                skipped.append("%s/%s: %s" % (section, name, error))
                continue
            if not ops:
                skipped.append("%s/%s: no geometry" % (section, name))
                continue
            raw["%s/%s" % (code, name[:-4])] = ops
    shapes = {}
    for key, ops in merge_alternates(raw).items():
        geom, pins = normalise(ops)
        shapes[key] = {
            "source": "%s/%s" % (key.split("/", 1)[0], key.split("/", 1)[1]),
            "geom": geom,
            "pins": pins,
        }
    with open(OUT, "w", encoding="utf-8") as handle:
        json.dump(shapes, handle, indent=1, sort_keys=True)
    print("wrote %d NECA shape(s) to %s" % (len(shapes), OUT))
    if skipped:
        print("skipped %d file(s):" % len(skipped))
        for line in skipped:
            print("  -", line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
