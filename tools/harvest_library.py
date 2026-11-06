"""Harvest the client's real ACADE symbol blocks from ``import/Prints`` into ``library/``.

Where the data comes from
-------------------------
``import/Prints/**/*.dxf`` -- 34 real electrical sheets (FC1000495-001..051).
This script reads the sheets, takes the *block definition* of each requested
symbol (the authoritative geometry), flattens nested INSERTs of named blocks,
bakes the modal insert scale (only FAN needs it: drawn at 10 units, inserted at
0.125), and writes one AC1009 DXF per symbol plus the 4-column ``library.tsv``
the loader reads.

What was measured on the real sheets (and enforced here)
--------------------------------------------------------
* every target block definition has **base point (0,0,0)** and **no ATTDEFs**:
  ACADE carries attributes on the insert, and in these exports the symbol
  inserts carry none at all (see ``reports/library-bootstrap.md``), so the
  attribute *set* per symbol comes from this script's table, based on the
  ACADE attribute vocabulary that the sheets' attribute-carrying modules
  actually use (TAG1/TAG2, FAMILY, CAT, MFG, DESC1..3, LOC, INST, XREF,
  TERM01.., X1TERM.., RATING1.., WDTAGALT, ...);
* the sheets are 35.5 x 24.6 drawing units per A3 frame and their dominant text
  height is 0.125, so attribute definitions are emitted 0.125 tall rather than
  at ``ELECLIB``'s span/8 (which would be 0.016 for a 0.125-unit terminal);
* DOT and 24VDCPRX reference anonymous ``*X*`` blocks; those are per-sheet
  anonymous graphics, not symbol geometry, so they are not inlined and the
  omission is recorded per symbol in ``library/harvest_report.tsv``.

Usage
-----
    py -3.13 tools/harvest_library.py [--prints DIR] [--out DIR] [--check]

``--check`` regenerates into memory and compares with the files on disk.
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dxfio  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_PRINTS = os.path.join(ROOT, "import", "Prints")
DEFAULT_OUT = os.path.join(ROOT, "library")

HEADER = "symbol\tclass\tattributes\tscale"

# The client's project text height, measured: 3653 model-space TEXT entities are
# 0.125 units tall (next most common 0.15). Attribute text is a drawing setting in
# ACADE, not a property of the symbol, so it is fixed here instead of span/8.
TEXT_HEIGHT = 0.125
TEXT_STEP = TEXT_HEIGHT * 1.25
FIRST_ROW_GAP = 0.2

# Attribute sets, named after the ACADE vocabulary the sheets use.
A_CONTACT = ["TAG1", "FAMILY", "CAT", "MFG", "DESC1", "LOC", "TERM01", "TERM02"]
A_TERMINAL = ["TAG1", "FAMILY", "CAT", "MFG", "DESC1", "LOC", "TERM01", "TERM02", "XREF"]
A_EARTH = ["TAG1", "FAMILY", "DESC1", "LOC"]
A_DOT = ["TAG1", "DESC1"]
A_BOX = ["TAG1", "FAMILY", "DESC1"]
A_SENSOR = ["TAG1", "FAMILY", "CAT", "MFG", "DESC1", "LOC", "TERM01", "TERM02"]
A_BREAKER = ["TAG1", "FAMILY", "CAT", "MFG", "DESC1", "LOC", "RATING1", "TERM01", "TERM02"]
A_CONNECTOR = ["TAG1", "FAMILY", "CAT", "MFG", "DESC1", "LOC", "TERM01", "TERM02", "XREF"]
A_GRAPHIC = ["TAG1", "DESC1"]

# canonical file/block name, class, attribute set, description, note
TARGETS = [
    dict(name="BOX", cls="Graphic Box", attrs=A_BOX,
         desc="Generic 0.625x0.25 graphic box (175 model-space inserts)",
         note="graphic box, not a device; used with nearby TEXT labels"),
    dict(name="DOT", cls="Junction", attrs=A_DOT,
         desc="Junction dot: one small circle",
         note="nested anonymous *X9 is per-sheet junk, not inlined"),
    dict(name="WDDOT", cls="Junction", attrs=A_DOT,
         desc="Wire dot: three concentric circles",
         note=""),
    dict(name="TERMINAL", cls="Terminal", attrs=A_TERMINAL,
         desc="Terminal: circle with a diagonal lead",
         note="the sheets also have a case-variant 'terminal' block with identical "
              "geometry; merged here because the app's block table is case-insensitive"),
    dict(name="Ground", cls="Earth", attrs=A_EARTH,
         desc="Ground: four-line earth symbol",
         note="sheets also carry a case-variant 'GROUND' with identical geometry"),
    dict(name="ELGND", cls="Earth", attrs=A_EARTH,
         desc="Earth ground: five-line variant",
         note=""),
    dict(name="CONT-NO", cls="Contactor", attrs=A_CONTACT,
         desc="Normally-open contact: 0.875 x 0.25 units",
         note=""),
    dict(name="CONT-NC", cls="Contactor", attrs=A_CONTACT,
         desc="Normally-closed contact (not in the requested list; same family as CONT-NO)",
         note=""),
    dict(name="PB-NO", cls="Button", attrs=A_CONTACT,
         desc="Push-button, normally-open",
         note="sheets also carry a case-variant 'Pb-no' with identical geometry"),
    dict(name="PB-NC", cls="Button", attrs=A_CONTACT,
         desc="Push-button, normally-closed",
         note=""),
    dict(name="TY", cls="Device Marker", attrs=A_GRAPHIC,
         desc="Device marker: four concentric circles",
         note="semantics ambiguous from geometry alone; sits next to device description TEXT"),
    dict(name="PT", cls="Connection Point", attrs=A_GRAPHIC,
         desc="Connection point: small circle with a diagonal",
         note="appears beside wire names (L1, N, GND), so treated as a connection point"),
    dict(name="PLUG", cls="Plug", attrs=A_CONNECTOR,
         desc="Plug / connector: four-line arrow shape",
         note="sheets also carry case-variant 'Plug' (more inserts) with identical geometry"),
    dict(name="ELCB", cls="Circuit Breaker", attrs=A_BREAKER,
         desc="Earth-leakage circuit breaker: two circles, arc and lever",
         note=""),
    dict(name="FAN", cls="Fan", attrs=A_GRAPHIC,
         desc="Fan: 10-unit drawing inserted at 0.125 in the sheets",
         note="modal insert scale 0.125 is baked into the harvested geometry"),
    dict(name="PHONEJACK", cls="Telephone Jack", attrs=A_GRAPHIC,
         desc="Phone jack: 12 lines and 4 polylines",
         note="block exists in 8 sheets but has no model-space insert, so it is "
              "harvested for the palette and cannot be verified in place"),
    dict(name="24VDCPRX", cls="Proximity Sensor", attrs=A_SENSOR,
         desc="24 VDC proximity sensor with an inlined Arrow-l nested block",
         note="nested anonymous *X36 is per-sheet junk, not inlined"),
    dict(name="Arrow-l", cls="Arrow", attrs=A_GRAPHIC,
         desc="Left arrow head (one SOLID)",
         note=""),
]

# Attribute definitions that are not "TAG1" need a human prompt.
PROMPTS = {
    "TAG1": "Tag",
    "TAG2": "Tag 2",
    "CAT": "Part number",
    "MFG": "Manufacturer",
    "DESC1": "Description",
    "DESC2": "Description 2",
    "DESC3": "Description 3",
    "LOC": "Location",
    "INST": "Installation",
    "FAMILY": "Family",
    "XREF": "Cross reference",
    "RATING1": "Rating 1",
    "TERM01": "Terminal 1",
    "TERM02": "Terminal 2",
}

# Group codes that belong to the modern file format, not to the entity geometry:
# handles, owners, subclass markers, paper-space flags, XDATA, lineweights.
STRIP_CODES = {
    5, 330, 331, 340, 347, 348, 390, 410, 420, 430, 440,
    60, 67, 92, 100, 102, 310, 370, 48,
}
# Entities this harvester can place: kind -> transformed, or reported and dropped.
UNSUPPORTED = {"HATCH", "DIMENSION", "LEADER", "MLEADER", "MLINE", "REGION",
               "3DSOLID", "BODY", "WIPEOUT", "OLE2FRAME", "RAY", "XLINE",
               "POLYLINE", "VERTEX", "SEQEND", "VIEWPORT"}


# --------------------------------------------------------------------------- #
# raw DXF parsing
# --------------------------------------------------------------------------- #


def read_pairs(path):
    """Read a DXF file into (group code, value) pairs."""
    pairs = []
    with open(path, "r", encoding="utf-8", errors="replace") as handle:
        lines = handle.read().splitlines()
    index = 0
    while index + 1 < len(lines):
        code_text = lines[index].strip()
        if not code_text:
            index += 1
            continue
        try:
            code = int(code_text)
        except ValueError:
            index += 1
            continue
        pairs.append((code, lines[index + 1].rstrip("\r").rstrip()))
        index += 2
    return pairs


def split_sections(pairs):
    result = {}
    index = 0
    while index < len(pairs):
        code, value = pairs[index]
        if code == 0 and value == "SECTION":
            name = pairs[index + 1][1] if index + 1 < len(pairs) else ""
            body = []
            index += 2
            while index < len(pairs) and pairs[index] != (0, "ENDSEC"):
                body.append(pairs[index])
                index += 1
            result.setdefault(name, []).extend(body)
            index += 1
        else:
            index += 1
    return result


def group_entities(body):
    """(kind, pairs) groups, folding ATTRIB/VERTEX children into their parents."""
    groups = []
    current = None
    for code, value in body:
        if code == 0:
            current = [value, []]
            groups.append(current)
        elif current is not None:
            current[1].append((code, value))
    folded = []
    owner = None
    for kind, pairs in groups:
        if kind in ("ATTRIB", "VERTEX") and owner is not None:
            owner[2].append([kind, pairs, []])
            continue
        if kind == "SEQEND":
            owner = None
            continue
        entry = [kind, pairs, []]
        folded.append(entry)
        owner = entry if kind in ("INSERT", "POLYLINE") else None
    return folded


def first(pairs, code, default=""):
    for c, v in pairs:
        if c == code:
            return v
    return default


def number(pairs, code, default=0.0):
    try:
        return float(first(pairs, code, str(default)) or default)
    except ValueError:
        return default


class Block:
    __slots__ = ("name", "base", "layer", "entities")

    def __init__(self, name, base, layer, entities):
        self.name = name
        self.base = base
        self.layer = layer
        self.entities = entities


class Sheet:
    __slots__ = ("name", "path", "blocks", "inserts")

    def __init__(self, name, path):
        self.name = name
        self.path = path
        self.blocks = {}     # upper-case name -> Block
        self.inserts = []    # model-space insert dicts


def parse_sheet(path):
    sheet = Sheet(os.path.basename(path), path)
    data = split_sections(read_pairs(path))
    blocks_body = data.get("BLOCKS", [])
    header = None
    for kind, pairs, children in group_entities(blocks_body):
        if kind == "BLOCK":
            header = pairs
            name = first(pairs, 2) or first(pairs, 3)
            base = (number(pairs, 10), number(pairs, 20), number(pairs, 30))
            sheet.blocks[name.upper()] = Block(name, base, first(pairs, 8, "0"), [])
            continue
        if kind == "ENDBLK":
            header = None
            continue
        if header is None:
            continue
        name = first(header, 2) or first(header, 3)
        block = sheet.blocks.get(name.upper())
        if block is not None:
            block.entities.append((kind, pairs, children))
    for kind, pairs, children in group_entities(data.get("ENTITIES", [])):
        if kind != "INSERT":
            continue
        if first(pairs, 67, "0").strip() == "1":
            continue                    # paper space: not a drawing component
        attributes = []
        for child_kind, child_pairs, _ in children:
            if child_kind == "ATTRIB":
                attributes.append((first(child_pairs, 2), first(child_pairs, 1)))
        sheet.inserts.append({
            "block": first(pairs, 2),
            "x": number(pairs, 10),
            "y": number(pairs, 20),
            "scale": number(pairs, 41, 1.0),
            "rotation": number(pairs, 50),
            "attributes": attributes,
        })
    return sheet


# --------------------------------------------------------------------------- #
# geometry: transform, sanitise, bounds
# --------------------------------------------------------------------------- #

POINT_CODES = (10, 11, 12, 13)


def _float(value, default=0.0):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def transform_entity(kind, pairs, scale, theta, tx, ty):
    """Apply p' = T + R(theta) * scale * p to one entity's geometry codes.

    Group codes 10/20/30 may repeat (LWPOLYLINE vertices, SPLINE control points,
    fit points), so the point transform walks sequentially and consumes the
    matching 20/30 that follow each 10/11/12/13 rather than looking codes up.
    """
    cos_t, sin_t = math.cos(theta), math.sin(theta)
    rotation_degrees = math.degrees(theta)
    out = []
    index = 0
    while index < len(pairs):
        code, value = pairs[index]
        if code in STRIP_CODES or 1001 <= code <= 1071:
            index += 1
            continue

        # ELLIPSE code 11 is the major-axis *vector*: rotate/scale, do not translate.
        if kind == "ELLIPSE" and code == 11:
            mx = _float(value)
            my = 0.0
            consumed = 1
            if index + 1 < len(pairs) and pairs[index + 1][0] == 21:
                my = _float(pairs[index + 1][1])
                consumed = 2
            sx, sy = scale * mx, scale * my
            out.append((11, dxfio._num(cos_t * sx - sin_t * sy)))
            out.append((21, dxfio._num(sin_t * sx + cos_t * sy)))
            index += consumed
            continue

        if code in POINT_CODES:
            x = _float(value)
            y = 0.0
            consumed = 1
            if index + 1 < len(pairs) and pairs[index + 1][0] == code + 10:
                y = _float(pairs[index + 1][1])
                consumed = 2
                if index + 2 < len(pairs) and pairs[index + 2][0] == code + 20:
                    consumed = 3
            sx, sy = scale * x, scale * y
            out.append((code, dxfio._num(tx + cos_t * sx - sin_t * sy)))
            out.append((code + 10, dxfio._num(ty + sin_t * sx + cos_t * sy)))
            if consumed == 3:
                out.append((code + 20, dxfio._num(_float(pairs[index + 2][1]) * scale)))
            index += consumed
            continue

        if kind in ("TEXT", "MTEXT", "ATTRIB", "ATTDEF") and code == 50:
            out.append((50, dxfio._num(_float(value) + rotation_degrees)))
            index += 1
            continue
        if kind == "ARC" and code in (50, 51):
            out.append((code, dxfio._num(_float(value) + rotation_degrees)))
            index += 1
            continue
        if kind in ("CIRCLE", "ARC") and code in (40, 41):
            out.append((code, dxfio._num(_float(value) * abs(scale))))
            index += 1
            continue
        if kind in ("TEXT", "MTEXT", "ATTRIB", "ATTDEF") and code == 40:
            out.append((40, dxfio._num(_float(value) * abs(scale))))
            index += 1
            continue
        if kind == "LWPOLYLINE" and code in (40, 41, 43):
            out.append((code, dxfio._num(_float(value) * abs(scale))))
            index += 1
            continue
        if kind == "SPLINE" and code in (42, 43, 44):
            # tolerances scale; knots (40) and weights (41) must not
            out.append((code, dxfio._num(_float(value) * abs(scale))))
            index += 1
            continue
        out.append((code, value))
        index += 1
    return out


def compose(outer, inner):
    """Compose two (scale, theta, tx, ty) transforms: outer after inner."""
    s1, t1, x1, y1 = outer
    s2, t2, x2, y2 = inner
    cos_t, sin_t = math.cos(t1), math.sin(t1)
    return (s1 * s2, t1 + t2,
            x1 + cos_t * (s1 * x2) - sin_t * (s1 * y2),
            y1 + sin_t * (s1 * x2) + cos_t * (s1 * y2))


IDENTITY = (1.0, 0.0, 0.0, 0.0)


def flatten(block, sheet, all_sheets, transform=IDENTITY, depth=0,
            dropped=None, inlined=None):
    """Return the block's geometry with nested named blocks inlined."""
    out = []
    if depth > 6:
        dropped.append(f"nesting deeper than 6 in {block.name}")
        return out
    for kind, pairs, children in block.entities:
        if kind == "INSERT":
            nested_name = first(pairs, 2)
            resolved = resolve_block(nested_name, sheet, all_sheets)
            if nested_name.upper().startswith("*"):
                dropped.append(f"anonymous nested block {nested_name}")
                continue
            if resolved is None:
                dropped.append(f"unresolved nested block {nested_name}")
                continue
            nested_block, nested_sheet = resolved
            local = (number(pairs, 41, 1.0), math.radians(number(pairs, 50)),
                     number(pairs, 10), number(pairs, 20))
            inlined.append(nested_name)
            out.extend(flatten(nested_block, nested_sheet, all_sheets,
                               compose(transform, local), depth + 1, dropped, inlined))
            continue
        if kind in UNSUPPORTED:
            dropped.append(f"{kind} entity")
            continue
        new_pairs = transform_entity(kind, pairs, *transform)
        children_out = []
        for child_kind, child_pairs, _ in children:
            if child_kind in ("ATTRIB", "VERTEX"):
                dropped.append(f"{child_kind} child")
                continue
        out.append((kind, new_pairs, children_out))
    return out


def resolve_block(name, sheet, all_sheets):
    """Find a block by name: same sheet first, then any sheet (deterministic order)."""
    upper = name.upper()
    if upper in sheet.blocks:
        return sheet.blocks[upper], sheet
    for other in all_sheets:
        if upper in other.blocks:
            return other.blocks[upper], other
    return None


def entity_bounds(kind, pairs):
    points = []
    if kind == "LINE":
        points = [(number(pairs, 10), number(pairs, 20)), (number(pairs, 11), number(pairs, 21))]
    elif kind in ("CIRCLE", "ARC"):
        cx, cy = number(pairs, 10), number(pairs, 20)
        radius = abs(number(pairs, 40))
        points = [(cx - radius, cy - radius), (cx + radius, cy + radius)]
    elif kind == "LWPOLYLINE":
        xs = [float(v) for c, v in pairs if c == 10]
        ys = [float(v) for c, v in pairs if c == 20]
        points = list(zip(xs, ys))
    elif kind in ("SOLID",):
        for code in (10, 11, 12, 13):
            points.append((number(pairs, code), number(pairs, code + 10)))
    elif kind in ("TEXT", "MTEXT", "ATTDEF", "ATTRIB", "POINT", "INSERT"):
        points = [(number(pairs, 10), number(pairs, 20))]
    elif kind == "SPLINE":
        xs = [float(v) for c, v in pairs if c == 10]
        ys = [float(v) for c, v in pairs if c == 20]
        points = list(zip(xs, ys))
    elif kind == "ELLIPSE":
        cx, cy = number(pairs, 10), number(pairs, 20)
        major = math.hypot(number(pairs, 11), number(pairs, 21))
        points = [(cx - major, cy - major), (cx + major, cy + major)]
    return points


def bounds(entities):
    points = []
    for kind, pairs, _ in entities:
        points.extend(entity_bounds(kind, pairs))
    if not points:
        return None
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    return min(xs), min(ys), max(xs), max(ys)


# --------------------------------------------------------------------------- #
# harvesting
# --------------------------------------------------------------------------- #


def signature(block):
    """Stable geometry fingerprint, for choosing one variant among sheets."""
    digest = hashlib.sha256()
    for kind, pairs, _ in block.entities:
        digest.update(kind.encode())
        digest.update(b"\x00")
        for code, value in pairs:
            if code in STRIP_CODES or 1001 <= code <= 1071:
                continue
            digest.update(f"{code}={value}".encode())
            digest.update(b"\x01")
        digest.update(b"\x02")
    return digest.hexdigest()[:16]


def modal_insert(name, sheets):
    """Most common (scale, rotation) the sheets insert a block at."""
    scales = collections.Counter()
    rotations = collections.Counter()
    for sheet in sheets:
        for insert in sheet.inserts:
            if insert["block"].upper() == name.upper():
                scales[round(insert["scale"], 6)] += 1
                rotations[round(insert["rotation"], 3)] += 1
    scale = scales.most_common(1)[0][0] if scales else 1.0
    rotation = rotations.most_common(1)[0][0] if rotations else 0.0
    return scale, rotation, len(scales)


def build_client_library(prints_dir):
    """Return (files {name: text}, manifest text, report text) without writing."""
    paths = []
    for base, _, names in os.walk(prints_dir):
        for name in sorted(names):
            if name.lower().endswith(".dxf"):
                paths.append(os.path.join(base, name))
    paths.sort()
    sheets = [parse_sheet(path) for path in paths]

    files = {}
    manifest_rows = []
    report_rows = ["symbol\tclass\tsource_sheet\tsource_block\tvariants\t"
                   "entities\tentity_kinds\tinlined\tomitted\tbbox_w\tbbox_h\t"
                   "baked_scale\tattributes"]

    for target in TARGETS:
        upper = target["name"].upper()
        found = []      # (sheet, block)
        for sheet in sheets:
            block = sheet.blocks.get(upper)
            if block is not None:
                found.append((sheet, block))
        if not found:
            report_rows.append(f"{target['name']}\t{target['cls']}\t-\t-\t0\t0\t-\t-\t"
                               f"NOT FOUND\t-\t-\t-\t-")
            continue

        # Pick one variant: the most common geometry signature, first sheet wins ties.
        variants = collections.OrderedDict()
        for sheet, block in found:
            variants.setdefault(signature(block), []).append((sheet, block))
        chosen_sig, chosen_entries = max(variants.items(), key=lambda item: len(item[1]))
        sheet, block = chosen_entries[0]

        scale, _, scale_variants = modal_insert(target["name"], sheets)
        dropped = []
        inlined = []
        entities = flatten(block, sheet, sheets, dropped=dropped, inlined=inlined)

        # Bake the block's base point (all targets are 0) and modal insert scale.
        base_x, base_y = block.base[0], block.base[1]
        final = []
        for kind, pairs, children in entities:
            final.append((kind, transform_entity(kind, pairs, scale, 0.0,
                                                 -base_x * scale, -base_y * scale), []))

        box = bounds(final)
        if box is None:
            report_rows.append(f"{target['name']}\t{target['cls']}\t{sheet.name}\t"
                               f"{block.name}\t{len(variants)}\t0\t-\t-\t"
                               f"NO GEOMETRY\t-\t-\t{scale}\t-")
            continue
        min_x, min_y, max_x, max_y = box

        doc = dxfio.DxfDocument()
        layers = set()
        for kind, pairs, _ in final:
            layer = first(pairs, 8, "0") or "0"
            layers.add(layer)
        for layer in sorted(layers):
            doc.add_layer(layer, 7)
        for kind, pairs, _ in final:
            doc.add_raw([(0, kind)] + list(pairs))

        attributes = target["attrs"]
        for row, tag in enumerate(attributes):
            doc.entities.append(dxfio.DxfDocument.attdef(
                tag=tag,
                prompt=PROMPTS.get(tag, tag),
                default="",
                height=TEXT_HEIGHT,
                x=min_x,
                y=min_y - FIRST_ROW_GAP - row * TEXT_STEP,
            ))

        files[target["name"] + ".dxf"] = doc.render()
        manifest_rows.append((target["name"], target["cls"],
                              "|".join(attributes), "1"))

        kinds = collections.Counter(kind for kind, _, _ in final)
        report_rows.append("\t".join([
            target["name"], target["cls"], sheet.name, block.name,
            str(len(variants)), str(len(final)),
            ",".join(f"{k}:{v}" for k, v in sorted(kinds.items())),
            ",".join(inlined) or "-",
            ",".join(dropped) or "-",
            f"{max_x - min_x:.4f}", f"{max_y - min_y:.4f}",
            str(scale), "|".join(attributes),
        ]))

    manifest_rows.sort(key=lambda row: (row[1].lower(), row[0]))
    manifest = HEADER + "\n" + "".join("\t".join(row) + "\n" for row in manifest_rows)
    report = "\n".join(report_rows) + "\n"
    return files, manifest, report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prints", default=DEFAULT_PRINTS, help="real sheets folder")
    parser.add_argument("--out", default=DEFAULT_OUT, help="output library folder")
    parser.add_argument("--check", action="store_true",
                        help="compare with the files on disk instead of writing")
    args = parser.parse_args(argv)

    files, manifest, report = build_client_library(args.prints)

    expected = dict(files)
    expected["library.tsv"] = manifest
    expected["harvest_report.tsv"] = report

    if args.check:
        problems = []
        for name, text in expected.items():
            path = os.path.join(args.out, name)
            if not os.path.exists(path):
                problems.append(f"missing {name}")
                continue
            with open(path, "r", encoding="ascii") as handle:
                if handle.read() != text:
                    problems.append(f"differs {name}")
        if problems:
            print("client library is NOT in sync with harvest_library.py:")
            for problem in problems:
                print("  -", problem)
            return 1
        print("client library is in sync: "
              f"{len(files)} symbols, {len(manifest.splitlines()) - 1} manifest rows")
        return 0

    os.makedirs(args.out, exist_ok=True)
    for name, text in expected.items():
        with open(os.path.join(args.out, name), "w", encoding="ascii", newline="\n") as handle:
            handle.write(text)
    print(f"wrote {len(files)} harvested symbol DXF files, library.tsv and "
          f"harvest_report.tsv to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
