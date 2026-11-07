"""Generate the starting sheet templates (IEC / ISO 5457 and NEC / ASME Y14.1).

Each template is a plain DXF at 1 unit = 1 mm with:

* the trim edge, an inner frame with the ISO / ASME zone reference grid, centring marks
  and a consistent rule weight;
* a title block drawn to the standard's sheet layout: ISO 7200 fields for IEC
  (first-angle projection symbol, revision table, company / title / drawing number /
  project / scale / size / sheet / date / drawn / checked / approved), and the ASME
  Y14.1 arrangement for NEC with a third-angle symbol and its own revision table;
* the editable fields as plain TEXT so the ordinary text editor (and the new-drawing-set
  wizard) can fill them: ``COMPANY NAME``, ``DRAWING TITLE``, ``PROJECT``, ``AR-0001``,
  ``1 / 1``, ``DRAWN``;
* a ``WD_M`` insert carrying the AutoCAD Electrical-style numbering contract that
  ``ELECSETUP`` reads (format, mode, start value, suffixes, sheet). Its attributes are
  invisible, like the real block, so the drawing starts clean;
* the electrical layers (``WIRES``, ``JUNCTIONS``, ``WIRENO``, ``XREF``,
  ``ELEC-SYMBOLS``) with their house colours, so a new print looks right before anything
  is drawn.

Start a drawing with:  ``ELECNEW templates\\iec\\A3_landscape.dxf``

The script is the source of truth: rerun it to regenerate every file after editing a
layout. Nothing here is version-specific beyond ISO 5457 / ISO 7200 and ASME Y14.1.
"""

from __future__ import annotations

import argparse
import os

import dxfio

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DEFAULT_OUT = os.path.join(ROOT, "templates")

MM_PER_INCH = 25.4

# (file stem, width mm, height mm, size label)
IEC_SIZES = [
    ("A4_landscape", 297.0, 210.0, "A4"),
    ("A4_portrait", 210.0, 297.0, "A4"),
    ("A3_landscape", 420.0, 297.0, "A3"),
    ("A3_portrait", 297.0, 420.0, "A3"),
    ("A2_landscape", 594.0, 420.0, "A2"),
    ("A1_landscape", 841.0, 594.0, "A1"),
    ("A0_landscape", 1189.0, 841.0, "A0"),
    ("B1_landscape", 1000.0, 707.0, "B1"),
]

NEC_SIZES = [
    ("ANSI_A_letter", 11.0 * MM_PER_INCH, 8.5 * MM_PER_INCH, "A"),
    ("ANSI_A_letter_portrait", 8.5 * MM_PER_INCH, 11.0 * MM_PER_INCH, "A"),
    ("ANSI_Legal", 14.0 * MM_PER_INCH, 8.5 * MM_PER_INCH, "Legal"),
    ("ANSI_B_tabloid", 17.0 * MM_PER_INCH, 11.0 * MM_PER_INCH, "B"),
    ("ANSI_C", 22.0 * MM_PER_INCH, 17.0 * MM_PER_INCH, "C"),
    ("ANSI_D", 34.0 * MM_PER_INCH, 22.0 * MM_PER_INCH, "D"),
    ("ANSI_E", 44.0 * MM_PER_INCH, 34.0 * MM_PER_INCH, "E"),
]

LAYERS = [
    ("BORDER", 7),
    ("ZONES", 8),
    ("TITLE_BLOCK", 7),
    ("WIRES", 3),
    ("JUNCTIONS", 3),
    ("WIRENO", 2),
    ("XREF", 6),
    ("ELEC-SYMBOLS", 7),
]

RULE_LAYER = "TITLE_BLOCK"
ZONE_LAYER = "ZONES"
CAPTION_HEIGHT = 1.8
VALUE_HEIGHT = 3.2

IEC_STANDARD = "IEC 61082 / ISO 5457"
NEC_STANDARD = "ASME Y14.1 / NEC"

# The numbering contract ELECSETUP reads. Format templates follow AutoCAD Electrical's
# `%F` family: family, number, sheet, cross-reference. Both standards start unfrozen
# (`MODE=S`) so a new print can number itself; set MODE=R once the sheet is released.
IEC_SETUP = [
    ("TAGFMT", "%F%N"),
    ("TAGMODE", "S"),
    ("TAG-START", "1"),
    ("TAG-RSUF", "A,B,C,D,E,F,G,H,J,K,L,M,N,P,R,S,T,U,V,W,X,Y"),
    ("WIREFMT", "%N"),
    ("WIREMODE", "S"),
    ("WIRE-START", "1"),
    ("XREFFMT", "%S.%N"),
    ("ALT_XREFFMT", "%N"),
    ("REFNUMS", "1"),
    ("SHEET", ""),
    ("COMP_LAY", "SYMS"),
    ("TERM_LAY", "TERMS"),
    ("WNUM_LAY", "WIRENO"),
    ("XREF_LAY", "XREF"),
    ("RUNGHORV", "H"),
    ("PH3SPACE", "0.5"),
]

NEC_SETUP = [
    ("TAGFMT", "%F%N"),
    ("TAGMODE", "S"),
    ("TAG-START", "1"),
    ("TAG-RSUF", "A,B,C,D,E,F,G,H,J,K,L,M,N,P,R,S,T,U,V,W,X,Y"),
    ("WIREFMT", "%N"),
    ("WIREMODE", "S"),
    ("WIRE-START", "1"),
    ("XREFFMT", "%N"),
    ("ALT_XREFFMT", "%N"),
    ("REFNUMS", "1"),
    ("SHEET", ""),
    ("COMP_LAY", "SYMS"),
    ("TERM_LAY", "TERMS"),
    ("WNUM_LAY", "WIRENO"),
    ("XREF_LAY", "XREF"),
    ("RUNGHORV", "H"),
    ("PH3SPACE", "0.5"),
]

IEC_PROMPTS = {
    "TAGFMT": "Component tag format template",
    "TAGMODE": "Tag numbering mode (S = sequential, R = references fixed)",
    "TAG-START": "First component number",
    "TAG-RSUF": "Tag number suffixes",
    "WIREFMT": "Wire number format template",
    "WIREMODE": "Wire numbering mode",
    "WIRE-START": "First wire number",
    "XREFFMT": "Cross-reference format",
}

NEC_PROMPTS = dict(IEC_PROMPTS)


# --------------------------------------------------------------------------- #
# drawing helpers
# --------------------------------------------------------------------------- #


def text(doc, value, x, y, height, layer=RULE_LAYER, rotation=0.0):
    doc.add_text(value, x, y, height, layer=layer, rotation=rotation)


def line(doc, x1, y1, x2, y2, layer=RULE_LAYER):
    doc.add_line(x1, y1, x2, y2, layer=layer)


def rect(doc, x0, y0, x1, y1, layer=RULE_LAYER):
    line(doc, x0, y0, x1, y0, layer)
    line(doc, x1, y0, x1, y1, layer)
    line(doc, x1, y1, x0, y1, layer)
    line(doc, x0, y1, x0, y0, layer)


def text_width(value, height):
    """Rough advance width of a single-line AutoCAD text, for centring."""
    return len(value) * height * 0.62


def centred(doc, value, cx, y, height, layer=ZONE_LAYER):
    text(doc, value, cx - text_width(value, height) / 2.0, y, height, layer)


def caption(doc, value, x, y, height=CAPTION_HEIGHT):
    text(doc, value, x, y, height)


def value_text(doc, value, x, y, height=VALUE_HEIGHT):
    text(doc, value, x, y, height)


def field_rule(doc, x0, x1, y):
    """A thin fill-in rule for a field with no value yet."""
    line(doc, x0, y, x1, y, layer=RULE_LAYER)


def cell(doc, x0, y0, x1, y1, name, value=None, value_height=VALUE_HEIGHT,
         pad=2.6, rule=True):
    """Caption along the top of a cell and either a value or a fill-in rule.

    Short cells (the 10 mm bottom row) get a smaller caption and value so the two
    never collide; taller cells centre the value in the space below the caption.
    """
    height = y1 - y0
    cap_height = min(CAPTION_HEIGHT, max(1.4, height * 0.14))
    cap_y = y1 - 0.7 - cap_height
    caption(doc, name, x0 + pad, cap_y, cap_height)
    vh = min(value_height, max(2.6, height * 0.36))
    area_bottom = y0 + 0.7
    area_top = cap_y - 1.1
    visible = max(0.0, area_top - area_bottom - vh)
    baseline = area_bottom + visible * 0.45
    if value is not None:
        value_text(doc, value, x0 + pad, baseline, vh)
    elif rule:
        field_rule(doc, x0 + pad, x1 - pad, baseline + 0.2)


def projection_symbol(doc, x0, y0, third_angle):
    """ISO projection symbol: a frustum and two concentric circles, 20 x 10 mm."""
    if third_angle:
        circles, frustum = x0 + 4.5, x0 + 10.0
    else:
        circles, frustum = x0 + 13.5, x0
    doc.add_circle(circles, y0 + 5.0, 3.2, layer=RULE_LAYER)
    doc.add_circle(circles, y0 + 5.0, 1.6, layer=RULE_LAYER)
    doc.add_lwpolyline(
        [
            (frustum, y0 + 2.0),
            (frustum + 7.0, y0 + 3.5),
            (frustum + 7.0, y0 + 6.5),
            (frustum, y0 + 8.0),
        ],
        closed=True,
        layer=RULE_LAYER,
    )
    caption_value = "THIRD ANGLE" if third_angle else "FIRST ANGLE"
    centred(doc, caption_value, x0 + 10.0, y0 - 3.4, 1.6, RULE_LAYER)


def revision_table(doc, x0, y0, width, rows, ratios, third_angle=False):
    """The drawing-office revision history strip above the title block.

    ``ratios`` are the column shares of ``width``: revision, date, description and, for
    NEC, the approval initials.
    """
    header = 7.0
    body = 7.0
    total = header + rows * body
    rect(doc, x0, y0, x0 + width, y0 + total)
    line(doc, x0, y0 + total - header, x0 + width, y0 + total - header)

    names = ["REV", "DATE", "DESCRIPTION"]
    if len(ratios) == 4:
        names.append("APPR")
    x = x0
    for index, ratio in enumerate(ratios[:-1]):
        x += width * ratio
        line(doc, x, y0, x, y0 + total)
    for index, name in enumerate(names):
        left = x0 + sum(width * ratio for ratio in ratios[:index])
        caption(doc, name, left + 2.0, y0 + total - header + 2.4, 1.6)
    for row in range(1, rows):
        line(doc, x0, y0 + total - header - row * body, x0 + width,
             y0 + total - header - row * body)

    projection_symbol(doc, x0 + width + 6.0, y0 + 4.0, third_angle)


def frame(doc, width, height, standard):
    """Trim edge, ISO/ASME frame, zone reference grid and centring marks."""
    left, other = (20.0, 10.0) if standard == "iec" else (25.4, 12.7)
    right = width - other
    top = height - other
    bottom = other

    rect(doc, 0.0, 0.0, width, height, "BORDER")
    rect(doc, left, bottom, right, top, "BORDER")

    # Zone reference grid: columns numbered at top and bottom, rows lettered at the sides.
    columns = max(4, int(round((right - left) / 50.0)))
    rows = max(3, int(round((top - bottom) / 50.0)))
    tick = other * 0.62
    for index in range(1, columns):
        x = left + (right - left) * index / columns
        line(doc, x, bottom, x, bottom - tick, ZONE_LAYER)
        line(doc, x, top, x, top + tick, ZONE_LAYER)
    for index in range(1, rows):
        y = bottom + (top - bottom) * index / rows
        line(doc, left, y, left - tick, y, ZONE_LAYER)
        line(doc, right, y, right + tick, y, ZONE_LAYER)

    zone_height = max(2.2, min(other * 0.5, 3.6))
    number_y = bottom - other * 0.72
    number_top_y = top + other * 0.28
    for index in range(columns):
        cx = left + (right - left) * (index + 0.5) / columns
        centred(doc, str(index + 1), cx, number_y, zone_height)
        centred(doc, str(index + 1), cx, number_top_y, zone_height)
    for index in range(rows):
        cy = bottom + (top - bottom) * (index + 0.5) / rows
        label = chr(ord("A") + index)
        text(doc, label, left - other * 0.82, cy - zone_height * 0.35, zone_height, ZONE_LAYER)
        text(doc, label, right + other * 0.22, cy - zone_height * 0.35, zone_height, ZONE_LAYER)

    # Centring marks: short registration ticks at the middle of each side (ISO 5457).
    mark = min(5.0, other * 0.4)
    line(doc, width / 2.0, 0.0, width / 2.0, mark, ZONE_LAYER)
    line(doc, width / 2.0, height, width / 2.0, height - mark, ZONE_LAYER)
    line(doc, 0.0, height / 2.0, mark, height / 2.0, ZONE_LAYER)
    line(doc, width, height / 2.0, width - mark, height / 2.0, ZONE_LAYER)


def _block_frame(doc, width, height, standard, block_height):
    """Common title-block corner maths.

    Returns ``(left, bottom, right, top, block_width, scale_x)`` where ``scale_x`` maps
    the nominal 180 mm layout onto a narrow sheet frame (ANSI A portrait is 177.8 mm wide
    inside the frame, so the block shrinks instead of running off the paper).
    """
    margin_left, other = (20.0, 10.0) if standard == "iec" else (25.4, 12.7)
    right = width - other
    bottom = other
    available = right - margin_left
    block_width = min(180.0, available)
    left = right - block_width
    top = bottom + block_height
    return left, bottom, right, top, block_width, block_width / 180.0


def title_block_iec(doc, width, height, size_label):
    """ISO 7200 title block in the lower right corner (nominal 180 x 64 mm)."""
    left, bottom, right, top, block_w, sx = _block_frame(doc, width, height, "iec", 64.0)

    def X(value):
        return left + value * sx

    rect(doc, left, bottom, right, top)
    for y in (bottom + 10.0, bottom + 24.0, bottom + 42.0):
        line(doc, left, y, right, y)
    line(doc, X(120.0), bottom, X(120.0), top)

    # Bottom row: scale, size, sheet, date, revision.
    for value in (30.0, 60.0, 90.0, 120.0):
        line(doc, X(value), bottom, X(value), bottom + 10.0)
    cell(doc, left, bottom, X(30.0), bottom + 10.0, "SCALE", "1:1")
    cell(doc, X(30.0), bottom, X(60.0), bottom + 10.0, "SIZE", size_label)
    cell(doc, X(60.0), bottom, X(90.0), bottom + 10.0, "SHEET", "1 / 1", 3.6)
    cell(doc, X(90.0), bottom, X(120.0), bottom + 10.0, "DATE")
    cell(doc, X(120.0), bottom, right, bottom + 10.0, "REV", "0")

    # Right column: drawn, checked, approved, project.
    line(doc, X(120.0), bottom + 10.0, right, bottom + 10.0)
    line(doc, X(120.0), bottom + 33.0, right, bottom + 33.0)
    cell(doc, X(120.0), bottom + 10.0, right, bottom + 24.0, "DRAWN")
    cell(doc, X(120.0), bottom + 24.0, right, bottom + 33.0, "CHECKED")
    cell(doc, X(120.0), bottom + 33.0, right, bottom + 42.0, "APPROVED")
    # PROJECT has no rule: the wizard appends the project name under the caption.
    cell(doc, X(120.0), bottom + 42.0, right, top, "PROJECT", rule=False)

    # Drawing number, title and company.
    caption(doc, "DWG NO", X(2.6), bottom + 21.9)
    text(doc, "AR-0001", X(2.6), bottom + 13.4, 6.0)
    text(doc, "DRAWING TITLE", X(2.6), bottom + 29.3, 6.5)
    text(doc, "COMPANY NAME", X(2.6), bottom + 50.0, 6.0)
    text(doc, IEC_STANDARD, X(2.6), bottom + 44.4, 1.6)

    revision_table(doc, left, top + 6.0, block_w - 26.0, 2, [0.14, 0.25, 0.61])


def title_block_nec(doc, width, height, size_label):
    """ASME Y14.1 title block in the lower right corner (nominal 180 x 56 mm)."""
    left, bottom, right, top, block_w, sx = _block_frame(doc, width, height, "nec", 56.0)

    def X(value):
        return left + value * sx

    rect(doc, left, bottom, right, top)
    for y in (bottom + 10.0, bottom + 24.0, bottom + 40.0):
        line(doc, left, y, right, y)
    line(doc, X(120.0), bottom, X(120.0), top)

    # Bottom row: scale, size, sheet, date, revision.
    for value in (30.0, 60.0, 90.0, 120.0):
        line(doc, X(value), bottom, X(value), bottom + 10.0)
    cell(doc, left, bottom, X(30.0), bottom + 10.0, "SCALE", "1:1")
    cell(doc, X(30.0), bottom, X(60.0), bottom + 10.0, "SIZE", size_label)
    cell(doc, X(60.0), bottom, X(90.0), bottom + 10.0, "SHEET", "1 / 1", 3.6)
    cell(doc, X(90.0), bottom, X(120.0), bottom + 10.0, "DATE")
    cell(doc, X(120.0), bottom, right, bottom + 10.0, "REV", "0")

    # Right column: drawn, checked, approved, project.
    line(doc, X(120.0), bottom + 10.0, right, bottom + 10.0)
    line(doc, X(120.0), bottom + 10.0, X(120.0), bottom + 24.0)
    line(doc, X(150.0), bottom + 24.0, X(150.0), bottom + 40.0)
    cell(doc, X(120.0), bottom + 10.0, right, bottom + 24.0, "DRAWN")
    cell(doc, X(120.0), bottom + 24.0, X(150.0), bottom + 40.0, "CHECKED")
    cell(doc, X(150.0), bottom + 24.0, right, bottom + 40.0, "APPROVED")
    cell(doc, X(120.0), bottom + 40.0, right, top, "PROJECT", rule=False)

    caption(doc, "DWG NO", X(2.6), bottom + 21.9)
    text(doc, "AR-0001", X(2.6), bottom + 13.4, 6.0)
    text(doc, "DRAWING TITLE", X(2.6), bottom + 30.0, 6.0)
    text(doc, "COMPANY NAME", X(2.6), bottom + 46.6, 6.0)
    text(doc, NEC_STANDARD, X(2.6), bottom + 41.8, 1.6)

    revision_table(
        doc, left, top + 6.0, block_w - 26.0, 3, [0.13, 0.24, 0.44, 0.19], True
    )


def wd_m_block(doc, setup, prompts, standard_note):
    definitions = [
        dxfio.DxfDocument.attdef(
            tag=tag,
            prompt=prompts.get(tag, tag),
            default=value,
            height=1.0,
            x=0.0,
            y=0.0,
            flags=1,
        )
        for tag, value in setup
    ]
    definitions.append(
        dxfio.DxfDocument.attdef(
            tag="STANDARD",
            prompt="Template standard",
            default=standard_note,
            height=1.0,
            flags=1,
        )
    )
    doc.define_block("WD_M", base=(0.0, 0.0, 0.0), entities=definitions)
    attributes = [(tag, value) for tag, value in setup]
    attributes.append(("STANDARD", standard_note))
    doc.entities.append(
        dxfio.DxfDocument.insert(
            "WD_M", 0.0, 0.0, attributes=attributes, layer="0", attribute_flags=1
        )
    )


def build(stem, width, height, size_label, standard):
    doc = dxfio.DxfDocument(extents=(0.0, 0.0, width, height))
    for name, color in LAYERS:
        doc.add_layer(name, color)
    frame(doc, width, height, standard)
    if standard == "iec":
        title_block_iec(doc, width, height, size_label)
        wd_m_block(doc, IEC_SETUP, IEC_PROMPTS, IEC_STANDARD)
    else:
        title_block_nec(doc, width, height, size_label)
        wd_m_block(doc, NEC_SETUP, NEC_PROMPTS, NEC_STANDARD)
    return doc


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=DEFAULT_OUT, help="output folder (default: templates/)")
    args = parser.parse_args(argv)

    written = []
    for stem, width, height, label in IEC_SIZES:
        target = os.path.join(args.out, "iec", f"{stem}.dxf")
        os.makedirs(os.path.dirname(target), exist_ok=True)
        build(stem, width, height, label, "iec").save(target)
        written.append((target, width, height))
    for stem, width, height, label in NEC_SIZES:
        target = os.path.join(args.out, "nec", f"{stem}.dxf")
        os.makedirs(os.path.dirname(target), exist_ok=True)
        build(stem, width, height, label, "nec").save(target)
        written.append((target, width, height))

    print(f"wrote {len(written)} template(s) to {args.out}")
    for target, width, height in written:
        print(f"  {os.path.relpath(target, os.path.dirname(args.out)):<34} {width:.1f} x {height:.1f} mm")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
