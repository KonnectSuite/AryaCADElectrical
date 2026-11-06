"""Generate the starting sheet templates (IEC / ISO 5457 and NEC / ASME Y14.1).

Each template is a plain DXF at 1 unit = 1 mm with:
  * the paper edge and an inner frame with zone references;
  * a standard title block in the bottom-right corner, its fields as plain TEXT so they can
    be edited with the ordinary text editor;
  * a `WD_M` insert carrying the AutoCAD Electrical-style numbering contract that
    `ELECSETUP` reads (format, mode, start value, suffixes, sheet), so a new drawing starts
    with the intended conventions;
  * the electrical layers (`WIRES`, `JUNCTIONS`, `WIRENO`, `XREF`, `ELEC-SYMBOLS`) with their
    house colours, so a new print looks right before anything is drawn.

Start a drawing with:  `ELECNEW templates\\iec\\A3_landscape.dxf`

The script is the source of truth: rerun it to regenerate every file after editing a
layout. Nothing here is version-specific beyond ISO 5457 / ASME Y14.1 sheet sizes.
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
]

NEC_SIZES = [
    ("ANSI_A_letter", 11.0 * MM_PER_INCH, 8.5 * MM_PER_INCH, "A"),
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


def text(doc, value, x, y, height, layer="TITLE_BLOCK", rotation=0.0):
    doc.add_text(value, x, y, height, layer=layer, rotation=rotation)


def line(doc, x1, y1, x2, y2, layer="BORDER"):
    doc.add_line(x1, y1, x2, y2, layer=layer)


def frame(doc, width, height, standard):
    """Paper edge, inner frame and zone references."""
    left, other = (20.0, 10.0) if standard == "iec" else (25.4, 12.7)
    right = width - other
    top = height - other
    bottom = other

    line(doc, 0, 0, width, 0)
    line(doc, width, 0, width, height)
    line(doc, width, height, 0, height)
    line(doc, 0, height, 0, 0)
    line(doc, left, bottom, right, bottom)
    line(doc, right, bottom, right, top)
    line(doc, right, top, left, top)
    line(doc, left, top, left, bottom)

    # Zone grid: columns along top/bottom, rows down the sides.
    columns = max(4, int(round((right - left) / 50.0)))
    rows = max(3, int(round((top - bottom) / 50.0)))
    for index in range(1, columns):
        x = left + (right - left) * index / columns
        line(doc, x, bottom, x, bottom - other * 0.6, layer="ZONES")
        line(doc, x, top, x, top + other * 0.6, layer="ZONES")
    for index in range(1, rows):
        y = bottom + (top - bottom) * index / rows
        line(doc, left, y, left - other * 0.6, y, layer="ZONES")
        line(doc, right, y, right + other * 0.6, y, layer="ZONES")
    for index in range(columns):
        x = left + (right - left) * (index + 0.5) / columns
        text(doc, str(index + 1), x - 2.0, bottom - other * 0.85, other * 0.5, layer="ZONES")
        text(doc, str(index + 1), x - 2.0, top + other * 0.25, other * 0.5, layer="ZONES")
    for index in range(rows):
        y = bottom + (top - bottom) * (index + 0.5) / rows
        label = chr(ord("A") + index)
        text(doc, label, left - other * 0.85, y - other * 0.25, other * 0.5, layer="ZONES")
        text(doc, label, right + other * 0.3, y - other * 0.25, other * 0.5, layer="ZONES")


def title_block_iec(doc, width, height, size_label, standard_note):
    block_w, block_h = 180.0, 63.0
    right = width - 10.0
    bottom = 10.0
    left = right - block_w
    top = bottom + block_h

    line(doc, left, bottom, left + block_w, bottom, layer="TITLE_BLOCK")
    line(doc, left + block_w, bottom, left + block_w, top, layer="TITLE_BLOCK")
    line(doc, left + block_w, top, left, top, layer="TITLE_BLOCK")
    line(doc, left, top, left, bottom, layer="TITLE_BLOCK")

    # Horizontal rules: company / title / data rows.
    row_company = bottom + block_h - 16.0
    row_data = bottom + 20.0
    line(doc, left, row_company, left + block_w, row_company, layer="TITLE_BLOCK")
    line(doc, left, row_data, left + block_w, row_data, layer="TITLE_BLOCK")
    # Vertical splits in the data rows.
    mid = left + 118.0
    line(doc, mid, bottom, mid, row_data, layer="TITLE_BLOCK")
    line(doc, left + 60.0, bottom, left + 60.0, row_data, layer="TITLE_BLOCK")
    line(doc, left + 90.0, bottom, left + 90.0, row_data, layer="TITLE_BLOCK")

    text(doc, "COMPANY NAME", left + 3.0, row_company + 4.0, 5.0)
    text(doc, "DRAWING TITLE", left + 3.0, row_data + 5.0, 6.0)
    text(doc, "PROJECT", left + 3.0, row_data + 1.0, 2.5)

    text(doc, "DWG NO", left + 121.0, row_data + 5.0, 2.5)
    text(doc, "AR-0001", left + 121.0, row_data + 1.0, 4.0)
    text(doc, "REV", left + 151.0, row_data + 5.0, 2.5)
    text(doc, "0", left + 151.0, row_data + 1.0, 4.0)

    text(doc, "SCALE", left + 2.0, bottom + 8.0, 2.5)
    text(doc, "1:1", left + 2.0, bottom + 2.5, 3.5)
    text(doc, "SIZE", left + 62.0, bottom + 8.0, 2.5)
    text(doc, size_label, left + 62.0, bottom + 2.5, 3.5)
    text(doc, "SHEET", left + 92.0, bottom + 8.0, 2.5)
    text(doc, "1 / 1", left + 92.0, bottom + 2.5, 3.5)

    text(doc, "DATE", left + 121.0, bottom + 14.0, 2.0)
    text(doc, "DRAWN", left + 121.0, bottom + 8.0, 2.0)
    text(doc, "CHECKED", left + 121.0, bottom + 2.0, 2.0)
    text(doc, "APPROVED", left + 151.0, bottom + 14.0, 2.0)
    text(doc, standard_note, left + 121.0, bottom + 18.0, 1.8)
    text(doc, "PROJECTION: FIRST ANGLE", left + 3.0, top - 5.0, 2.0)


def title_block_nec(doc, width, height, size_label, standard_note):
    block_w, block_h = 180.0, 50.0
    other = 12.7
    right = width - other
    bottom = other
    left = right - block_w
    top = bottom + block_h

    line(doc, left, bottom, left + block_w, bottom, layer="TITLE_BLOCK")
    line(doc, left + block_w, bottom, left + block_w, top, layer="TITLE_BLOCK")
    line(doc, left + block_w, top, left, top, layer="TITLE_BLOCK")
    line(doc, left, top, left, bottom, layer="TITLE_BLOCK")

    row_top = bottom + 30.0
    row_mid = bottom + 15.0
    line(doc, left, row_top, left + block_w, row_top, layer="TITLE_BLOCK")
    line(doc, left, row_mid, left + block_w, row_mid, layer="TITLE_BLOCK")
    line(doc, left + 95.0, bottom, left + 95.0, top, layer="TITLE_BLOCK")
    line(doc, left + 60.0, bottom, left + 60.0, row_mid, layer="TITLE_BLOCK")
    line(doc, left + 78.0, bottom, left + 78.0, row_mid, layer="TITLE_BLOCK")
    line(doc, left + 120.0, bottom, left + 120.0, top, layer="TITLE_BLOCK")

    text(doc, "COMPANY NAME", left + 3.0, row_top + 8.0, 5.0)
    text(doc, "DRAWING TITLE", left + 3.0, row_mid + 4.0, 6.0)
    text(doc, "PROJECT", left + 3.0, bottom + 6.0, 2.5)
    text(doc, "THIRD ANGLE PROJECTION", left + 3.0, top - 5.0, 2.0)

    text(doc, "DWG NO", left + 98.0, row_top + 5.0, 2.5)
    text(doc, "AR-0001", left + 98.0, row_top + 1.0, 4.0)
    text(doc, "REV", left + 123.0, row_top + 5.0, 2.5)
    text(doc, "0", left + 123.0, row_top + 1.0, 4.0)
    text(doc, "SCALE", left + 62.0, row_mid + 3.0, 2.5)
    text(doc, "1:1", left + 62.0, bottom + 3.0, 3.5)
    text(doc, "SIZE", left + 80.0, row_mid + 3.0, 2.5)
    text(doc, size_label, left + 80.0, bottom + 3.0, 3.5)
    text(doc, "SHEET", left + 98.0, row_mid + 3.0, 2.5)
    text(doc, "1 / 1", left + 98.0, bottom + 3.0, 3.5)
    text(doc, "DATE", left + 123.0, row_mid + 3.0, 2.0)
    text(doc, "DRAWN", left + 145.0, row_mid + 3.0, 2.0)
    text(doc, "CHECKED", left + 123.0, bottom + 8.0, 2.0)
    text(doc, "APPROVED", left + 145.0, bottom + 8.0, 2.0)
    text(doc, standard_note, left + 123.0, bottom + 2.0, 1.8)


def wd_m_block(doc, setup, prompts, standard_note):
    definitions = [
        dxfio.DxfDocument.attdef(
            tag=tag,
            prompt=prompts.get(tag, tag),
            default=value,
            height=1.0,
            x=0.0,
            y=0.0,
        )
        for tag, value in setup
    ]
    definitions.append(
        dxfio.DxfDocument.attdef(
            tag="STANDARD", prompt="Template standard", default=standard_note, height=1.0
        )
    )
    doc.define_block("WD_M", base=(0.0, 0.0, 0.0), entities=definitions)
    attributes = [(tag, value) for tag, value in setup]
    attributes.append(("STANDARD", standard_note))
    doc.entities.append(
        dxfio.DxfDocument.insert("WD_M", 0.0, 0.0, attributes=attributes, layer="0")
    )


def build(stem, width, height, size_label, standard):
    doc = dxfio.DxfDocument(extents=(0.0, 0.0, width, height))
    for name, color in LAYERS:
        doc.add_layer(name, color)
    frame(doc, width, height, standard)
    if standard == "iec":
        title_block_iec(doc, width, height, size_label, "IEC 61082 / ISO 5457")
        wd_m_block(doc, IEC_SETUP, IEC_PROMPTS, "IEC 61082 / ISO 5457")
    else:
        title_block_nec(doc, width, height, size_label, "ASME Y14.1 / NEC")
        wd_m_block(doc, NEC_SETUP, NEC_PROMPTS, "ASME Y14.1 / NEC")
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
