"""Generate the electrical symbol library: ``library/*.dxf`` + ``library/library.tsv``.

Everything here is derived from the loader in ``src/app/commands/elec.rs``
(``elec_lib`` / ``read_library_manifest``), not guessed:

* one ``.dxf`` per symbol; **the file stem is the block name** the app defines;
* symbol geometry goes in **model space** at local coordinates around the
  insertion point, because ``Document::entities()`` returns every entity
  (block definitions included) and ``ELECLIB`` feeds exactly that set to
  ``define_block_from_owned_entities(..., stem, DVec3::ZERO)``.  Base point is
  therefore the origin: the wire connection line is ``y = 0`` and the
  terminals sit at ``x = +/-5``;
* each symbol carries real ``ATTDEF`` entities, so the loader's
  "fabricate attribute definitions" branch is skipped (it would place 2.0-tall
  text at ``y = 4 - row*2.5`` regardless of symbol size).  The attributes are
  sized and positioned the way the code itself sizes them:
  ``height = span/8`` (the unit ``ELECLIB`` measures for ``label_height``) and
  ``step = 1.25 * height`` (``elec_insert``/``PlaceSymbolCommand``);
* ``library.tsv`` is the 4-column manifest ``read_library_manifest`` parses:
  ``symbol <TAB> class <TAB> attribute|tags <TAB> scale``.  Line 0 is skipped
  (so it is a header).  ``class`` is what ``default_prefix`` turns into the tag
  prefix; ``scale`` is what ``ELECPLACE`` inserts the block at.

Scale
-----
One drawing unit is one millimetre, which is the only scale the code fixes by
absolute sizes: ``DOT_RADIUS = 1.0``, ``LABEL_HEIGHT = 2.5`` (wire numbers) and
``2.0`` (cross references, ``elec.rs``), and ``ELECSHEET`` lays an A3 sheet out
as 420 x 297.  A device symbol is therefore ~10 units tall and its attribute
text is 10/8 = 1.25 units, which is what ``ELECLIB`` will measure for it.

Scale
-----
Measured, not guessed: the real client sheets in ``import/Prints`` are
35.5 x 24.6 drawing units for an A3 frame (see ``reports/library-bootstrap.md``)
and their dominant text height is 0.125 units, so one device symbol is about
**one drawing unit** and its label text 0.125.  This starter set is therefore
authored on a 10-unit canvas and emitted at ``UNIT_SCALE = 0.1``, which gives
the same convention as the harvested client library: a symbol is ~1 unit,
``span / 8`` is 0.125, and the two libraries can be used on the same sheet.

Usage
-----
    py -3.13 tools/make_library.py [--out DIR] [--check]

``--check`` regenerates into memory and compares with what is on disk, so the
committed library can be proven to match this script.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dxfio  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STANDARD = "iec"
DEFAULT_OUT = os.path.join(ROOT, "library", "starter")

# 10-unit design canvas -> 1-unit drawing symbols (see "Scale" above).
UNIT_SCALE = 0.1

# The symbols stay ~1 drawing unit (the harvested client convention), so on the
# millimetre sheets the manifest asks ``ELECLIB``/``ELECPLACE`` for 25.4 x scale.
LIBRARY_SCALE = 25.4

HEADER = "symbol\tclass\tattributes\tscale\tstandard"

# Attribute sets.  ``field_for_attribute`` in elec.rs recognises the well-known
# fields; the rest are carried verbatim on the insert (``Component::attributes``)
# and are what ACADE-style symbols keep beside the geometry.
A_2TERM = ["TAG1", "CAT", "MFG", "DESC1", "LOC", "TERM01", "TERM02"]
A_DEVICE = ["TAG1", "CAT", "MFG", "DESC1", "LOC"]
A_TERMINAL = ["TAG1", "CAT", "MFG", "DESC1", "LOC", "XREF"]
A_PLC = ["TAG1", "CAT", "MFG", "DESC1", "LOC", "ADDR01", "ADDR02"]
A_LABEL = ["TAG1", "DESC1"]
A_EARTH = ["TAG1", "DESC1"]

# Wire connection points, in the 10-unit design canvas.  These are emitted as
# invisible ATTDEFs at the actual pin position, the way AutoCAD Electrical stores
# them (`X4TERM01` on the left, `X1TERM02` on the right); the electrical layer
# reads that insertion point as the pin, so a wire ending on it joins the net
# and the wire list can say `-K1:1 -> -M1:2`.
PINS_2TERM = [(-5.0, 0.0, "X4TERM01"), (5.0, 0.0, "X1TERM02")]
PINS_EARTH = [(0.0, 3.5, "X4TERM01")]
PINS_PLC_DI = [(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02")]
PINS_PLC_DO = [(5.0, 1.0, "X1TERM01"), (5.0, -1.0, "X1TERM02")]
PINS_TERMINAL_BLOCK = [(-5.0, 0.5, "X4TERM01"), (0.0, 0.5, "X4TERM02"), (5.0, 0.5, "X1TERM03")]
PINS_CABLE_ENTRY = [(-5.0, 0.0, "X4TERM01")]
PINS_NONE = []
PINS_NETWORK_SWITCH = [
    (-5.0, 1.0, "X4TERM01"),
    (-5.0, -1.0, "X4TERM02"),
    (5.0, 1.0, "X1TERM03"),
    (5.0, -1.0, "X1TERM04"),
]
PINS_HMI = [
    (-5.0, 1.5, "X4TERM01"),
    (-5.0, -1.5, "X4TERM02"),
    (5.0, 0.0, "X1TERM03"),
]
PINS_IO_MODULE = [
    (-5.0, 3.0, "X4TERM01"),
    (-5.0, 1.0, "X4TERM02"),
    (-5.0, -1.0, "X4TERM03"),
    (-5.0, -3.0, "X4TERM04"),
    (5.0, 3.0, "X1TERM05"),
    (5.0, 1.0, "X1TERM06"),
    (5.0, -1.0, "X1TERM07"),
    (5.0, -3.0, "X1TERM08"),
]


def canonical_pins(pins):
    """Number a symbol's connection points so no two of them share a pin number.

    The convention the electrical layer reads (`crates/elec/src/pins.rs`) is that the
    digits after `TERM` are the pin number and the digit after `X` only says which side
    the wire approaches from, so `X4TERM01` and `X1TERM02` are pins 1 and 2 of one
    device.  Numbering each side from 1 - which is what the importers used to do - makes
    both ends of a device read as pin 1, and the wire list then prints `-K1:1 -> -K1:1`
    instead of `-K1:1 -> -M1:2`.  Left-hand pins are numbered 1..n top to bottom and
    right-hand pins continue from there, exactly as `PINS_2TERM` above already does.

    The list keeps its original order, so only the tag text of a colliding symbol moves.
    """

    def side_of(tag):
        match = re.fullmatch(r"X(\d)TERM\d+", tag.strip().upper())
        return int(match.group(1)) if match else None

    def rank(side):
        # Left-hand (X4) first, then right-hand (X1), then any other side, then
        # anything that is not a connection point at all.
        if side == 4:
            return (0, 0)
        if side == 1:
            return (1, 0)
        if side is None:
            return (3, 0)
        return (2, side)

    by_side = {}
    for position, (x, y, tag) in enumerate(pins):
        by_side.setdefault(side_of(tag), []).append(position)
    numbered = list(pins)
    number = 1
    for side in sorted(by_side, key=rank):
        column = sorted(by_side[side], key=lambda p: (-pins[p][1], pins[p][0]))
        if side is None:
            continue
        for position in column:
            x, y, _ = pins[position]
            numbered[position] = (x, y, f"X{side}TERM{number:02d}")
            number += 1
    return numbered


def L(x1, y1, x2, y2):
    return ("line", x1, y1, x2, y2)


def C(cx, cy, r):
    return ("circle", cx, cy, r)


def A(cx, cy, r, start, end):
    return ("arc", cx, cy, r, start, end)


def P(points, closed=False):
    return ("poly", list(points), closed)


def T(value, x, y, height, rotation=0.0):
    return ("text", value, x, y, height, rotation)


def rect(x1, y1, x2, y2):
    """Four lines, counter-clockwise - a rectangle as geometry, not an entity."""
    return [L(x1, y1, x2, y1), L(x2, y1, x2, y2), L(x2, y2, x1, y2), L(x1, y2, x1, y1)]


def leads(left=5.0, right=5.0, inner=2.5, y=0.0):
    return [L(-left, y, -inner, y), L(inner, y, right, y)]


def no_contact(inner=2.5):
    """Normally-open contact: fixed contact bar plus an angled moving blade."""
    return leads(inner=inner) + [
        L(-inner, -1.5, -inner, 1.5),      # fixed contact
        L(-inner, 0.0, inner, 2.5),        # moving blade, open
    ]


def nc_contact(inner=2.5):
    """Normally-closed contact: the blade rests on the fixed contact."""
    return leads(inner=inner) + [
        L(-inner, 0.0, inner, 0.0),        # blade, closed
        L(inner, 0.0, inner + 1.0, 2.0),   # operator link
    ]


def actuator():
    """The push-button / limit-switch operator above the contact."""
    return [L(0.0, 0.0, 0.0, 3.0), L(-1.5, 3.0, 1.5, 3.0)]

# --------------------------------------------------------------------------- #
# standard drawing primitives (IEC 60617 / NEMA-ICS symbology)
# --------------------------------------------------------------------------- #
BAR = 1.2          # fixed-contact bar half-height (IEC)
ANSI_BAR = 1.5     # NEMA contact bar half-height


def contact_iec(closed=False):
    """IEC 60617 make/break contact: a pivoting blade against a fixed bar."""
    geom = [L(-5.0, 0.0, -1.6, 0.0), L(-1.6, -BAR, -1.6, BAR), L(1.6, 0.0, 5.0, 0.0)]
    if closed:
        geom += [L(-1.6, 0.0, 1.6, 0.0), L(1.6, -BAR, 1.6, BAR), C(-1.6, 0.0, 0.22)]
    else:
        geom += [L(-1.6, 0.0, 1.55, 2.05)]
    return geom


def contact_ansi(closed=False):
    """NEMA contact: two upright bars with a gap; the slash marks a break."""
    geom = [L(-5.0, 0.0, -0.55, 0.0), L(-0.55, -ANSI_BAR, -0.55, ANSI_BAR),
            L(0.55, -ANSI_BAR, 0.55, ANSI_BAR), L(0.55, 0.0, 5.0, 0.0)]
    if closed:
        geom += [L(-0.95, -1.9, 0.95, 1.9)]
    return geom


def operator_bar(height=2.6, width=1.5):
    """Push-button operator: stem and actuating bar pressing the blade."""
    return [L(0.0, 1.0, 0.0, height), L(-width, height, width, height)]


def operator_mushroom():
    """Emergency-stop operator: stem, wide plate and cap."""
    return [L(0.0, 0.0, 0.0, 1.6), L(-2.2, 1.6, 2.2, 1.6),
            L(-1.7, 2.4, 1.7, 2.4), L(-1.7, 1.6, -1.7, 2.4), L(1.7, 1.6, 1.7, 2.4)]


def winding(x, y0, y1, r=0.9, toward=1.0):
    """A chain of half-circle windings; toward=+1 bulges right, -1 left."""
    arcs = []
    y = y0
    while y < y1 - 1e-6:
        cy = y + r
        if toward > 0:
            arcs.append(A(x, cy, r, -90.0, 90.0))
        else:
            arcs.append(A(x, cy, r, 90.0, 270.0))
        y += 2.0 * r
    return arcs


def core_lines(x, y0, y1, gap=0.35):
    return [L(x - gap, y0, x - gap, y1), L(x + gap, y0, x + gap, y1)]


def zigzag(x0, x1, y=0.0, amp=1.1, teeth=4):
    points = [(x0, y)]
    span = (x1 - x0) / (teeth * 2.0)
    for index in range(teeth * 2):
        points.append((x0 + span * (index + 0.5), y + (amp if index % 2 == 0 else -amp)))
    points.append((x1, y))
    return [P(points)]


def dashed(x0, y0, x1, y1, dash=0.7, gap=0.45):
    """Mechanical / non-electrical link."""
    out = []
    length = math.hypot(x1 - x0, y1 - y0)
    if length == 0.0:
        return out
    ux, uy = (x1 - x0) / length, (y1 - y0) / length
    pos = 0.0
    while pos < length:
        end = min(pos + dash, length)
        out.append(L(x0 + ux * pos, y0 + uy * pos, x0 + ux * end, y0 + uy * end))
        pos = end + gap
    return out


def arrow(x, y, dx, dy, size=0.9):
    """Hollow arrowhead pointing along (dx, dy)."""
    length = math.hypot(dx, dy) or 1.0
    ux, uy = dx / length, dy / length
    px, py = -uy, ux
    base_x, base_y = x - ux * size, y - uy * size
    return [P([(x, y), (base_x + px * size * 0.5, base_y + py * size * 0.5),
               (base_x - px * size * 0.5, base_y - py * size * 0.5)], closed=True)]


def ground_bars(y=0.0, width=1.4):
    return [L(-width, y, width, y), L(-width * 0.66, y - 0.7, width * 0.66, y - 0.7),
            L(-width * 0.33, y - 1.4, width * 0.33, y - 1.4)]


def circle_motor(three_phase=False):
    geom = [L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
            T("M", -0.9, -0.7, 1.6)]
    if three_phase:
        geom += [T("3~", -1.1, 1.0, 1.0)]
    return geom


def device_box(width=3.0, height=2.5, glyph=None, glyph_size=1.1):
    geom = [L(-5.0, 0.0, -width, 0.0), L(width, 0.0, 5.0, 0.0)]
    geom += rect(-width, -height, width, height)
    if glyph:
        geom.append(T(glyph, -len(glyph) * glyph_size * 0.3, -glyph_size * 0.5, glyph_size))
    return geom


def terminal_circle():
    return [L(-5.0, 0.0, -1.2, 0.0), L(1.2, 0.0, 5.0, 0.0), C(0.0, 0.0, 1.2)]


# --------------------------------------------------------------------------- #
# the symbol set
# --------------------------------------------------------------------------- #
# Each entry: block/file name, class (drives the tag prefix), attribute set,
# geometry, and a one-line description for the report.

SYMBOLS = [
    # ---- operating coils -------------------------------------------------------
    dict(
        name="CONTACTOR_COIL", cls="Contactor", attrs=A_2TERM,
        desc="Contactor operating coil: filled-out box between the coil terminals",
        geom=leads(inner=2.4) + rect(-2.4, -1.4, 2.4, 1.4),
        geom_nec=leads(inner=1.5) + [C(0.0, 0.0, 1.5)],
    ),
    dict(
        name="RELAY_COIL", cls="Relay", attrs=A_2TERM,
        desc="Relay coil: box with the diagonal that separates it from a contactor",
        geom=leads(inner=2.4) + rect(-2.4, -1.4, 2.4, 1.4) + [L(-2.4, -1.4, 2.4, 1.4)],
        geom_nec=leads(inner=1.5) + [C(0.0, 0.0, 1.5), L(-1.5, -1.5, 1.5, 1.5)],
    ),
    dict(
        name="TIMER_COIL", cls="Timer", attrs=A_2TERM,
        desc="Time-delay coil: contactor box with the clock face",
        geom=leads(inner=2.4) + rect(-2.4, -1.4, 2.4, 1.4)
        + [C(0.0, 3.6, 1.3), L(0.0, 3.6, 0.0, 4.6), L(0.0, 3.6, 0.9, 3.3)],
    ),
    # ---- contacts -------------------------------------------------------------
    dict(
        name="CONTACT_NO", cls="Contactor", attrs=A_2TERM,
        desc="Normally-open contact (IEC make contact / NEMA open contact)",
        geom=contact_iec(closed=False), geom_nec=contact_ansi(closed=False),
    ),
    dict(
        name="CONTACT_NC", cls="Contactor", attrs=A_2TERM,
        desc="Normally-closed contact (IEC break contact / NEMA closed contact)",
        geom=contact_iec(closed=True), geom_nec=contact_ansi(closed=True),
    ),
    dict(
        name="RELAY_CONTACT_NO", cls="Relay", attrs=A_2TERM,
        desc="Relay contact, normally open, linked to its coil",
        geom=contact_iec(closed=False) + dashed(0.0, -1.2, 0.0, -3.4),
        geom_nec=contact_ansi(closed=False) + dashed(0.0, -1.5, 0.0, -3.4),
    ),
    dict(
        name="RELAY_CONTACT_NC", cls="Relay", attrs=A_2TERM,
        desc="Relay contact, normally closed, linked to its coil",
        geom=contact_iec(closed=True) + dashed(0.0, -1.2, 0.0, -3.4),
        geom_nec=contact_ansi(closed=True) + dashed(0.0, -1.5, 0.0, -3.4),
    ),
    dict(
        name="TIMER_CONTACT_NO", cls="Timer", attrs=A_2TERM,
        desc="Time-delay contact with the clock face",
        geom=contact_iec(closed=False) + [C(0.0, 3.6, 1.1), L(0.0, 3.6, 0.0, 4.4), L(0.0, 3.6, 0.7, 3.3)],
        geom_nec=contact_ansi(closed=False) + [C(0.0, 3.6, 1.1), L(0.0, 3.6, 0.0, 4.4), L(0.0, 3.6, 0.7, 3.3)],
    ),
    dict(
        name="TIMER_CONTACT_NC", cls="Timer", attrs=A_2TERM,
        desc="Time-delay break contact with the clock face",
        geom=contact_iec(closed=True) + [C(0.0, 3.6, 1.1), L(0.0, 3.6, 0.0, 4.4), L(0.0, 3.6, 0.7, 3.3)],
        geom_nec=contact_ansi(closed=True) + [C(0.0, 3.6, 1.1), L(0.0, 3.6, 0.0, 4.4), L(0.0, 3.6, 0.7, 3.3)],
    ),
    # ---- operators ------------------------------------------------------------
    dict(
        name="PUSHBUTTON_NO", cls="Button", attrs=A_2TERM,
        desc="Push-button, normally open (momentary make)",
        geom=contact_iec(closed=False) + operator_bar(),
        geom_nec=contact_ansi(closed=False) + operator_bar(),
    ),
    dict(
        name="PUSHBUTTON_NC", cls="Button", attrs=A_2TERM,
        desc="Push-button, normally closed (momentary break)",
        geom=contact_iec(closed=True) + operator_bar(),
        geom_nec=contact_ansi(closed=True) + operator_bar(),
    ),
    dict(
        name="EMERGENCY_STOP", cls="Button", attrs=A_2TERM,
        desc="Emergency stop: mushroom head on a break contact",
        geom=contact_iec(closed=True) + operator_mushroom(),
        geom_nec=contact_ansi(closed=True) + operator_mushroom(),
    ),
    dict(
        name="SELECTOR_SWITCH", cls="Selector", attrs=A_2TERM,
        desc="Two-position selector switch with its lever",
        geom=contact_iec(closed=False) + [L(0.0, 0.0, 1.8, 2.6), C(2.1, 2.9, 0.45)],
        geom_nec=contact_ansi(closed=False) + [L(0.0, 0.0, 1.8, 2.6), C(2.1, 2.9, 0.45)],
    ),
    dict(
        name="LIMIT_SWITCH", cls="Limit Switch", attrs=A_2TERM,
        desc="Limit switch: open contact with a straight lever",
        geom=contact_iec(closed=False) + [L(0.0, 0.0, 0.0, 2.6), L(0.0, 2.6, 2.4, 2.6)],
        geom_nec=contact_ansi(closed=False) + [L(0.0, 0.0, 0.0, 2.6), L(0.0, 2.6, 2.4, 2.6)],
    ),
    dict(
        name="LIMIT_SWITCH_ROLLER", cls="Limit Switch", attrs=A_2TERM,
        desc="Limit switch: open contact with a roller lever",
        geom=contact_iec(closed=False)
        + [L(0.0, 0.0, 0.0, 2.4), L(0.0, 2.4, 2.0, 3.1), C(2.5, 3.3, 0.55)],
        geom_nec=contact_ansi(closed=False)
        + [L(0.0, 0.0, 0.0, 2.4), L(0.0, 2.4, 2.0, 3.1), C(2.5, 3.3, 0.55)],
    ),
    dict(
        name="SWITCH_DISCONNECT", cls="Switch", attrs=A_2TERM,
        desc="Isolating switch: knife blade between two contacts",
        geom=[L(-5.0, 0.0, -2.6, 0.0), C(-2.2, 0.0, 0.35), C(2.2, 0.0, 0.35),
              L(2.6, 0.0, 5.0, 0.0), L(-2.2, 0.0, 2.2, 2.4)],
    ),
    dict(
        name="FOOT_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Foot switch: pedal plate over a break contact",
        geom=contact_iec(closed=True)
        + [L(0.0, 0.0, 0.0, 1.5),
           P([(-2.4, 1.5), (2.4, 1.5), (1.5, 2.5), (-1.5, 2.5)], closed=True)],
        geom_nec=contact_ansi(closed=True)
        + [L(0.0, 0.0, 0.0, 1.5),
           P([(-2.4, 1.5), (2.4, 1.5), (1.5, 2.5), (-1.5, 2.5)], closed=True)],
    ),
    dict(
        name="LEVEL_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Level switch: float on a lever arm",
        geom=contact_iec(closed=False) + [L(0.0, 0.0, 0.0, 2.4), L(0.0, 2.4, 2.2, 3.2), C(2.7, 3.2, 0.8)],
        geom_nec=contact_ansi(closed=False) + [L(0.0, 0.0, 0.0, 2.4), L(0.0, 2.4, 2.2, 3.2), C(2.7, 3.2, 0.8)],
    ),
    dict(
        name="PRESSURE_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Pressure switch: diaphragm with the set-point screw",
        geom=contact_iec(closed=False) + [L(0.0, 0.0, 0.0, 1.8), C(0.0, 2.9, 1.1), L(0.0, 4.0, 0.0, 4.6)],
        geom_nec=contact_ansi(closed=False) + [L(0.0, 0.0, 0.0, 1.8), C(0.0, 2.9, 1.1), L(0.0, 4.0, 0.0, 4.6)],
    ),
    dict(
        name="FLOW_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Flow switch: paddle across the pipe",
        geom=contact_iec(closed=False)
        + [L(0.0, 0.0, 0.0, 1.8), rect(-1.6, 1.8, 1.6, 3.2), L(0.0, 3.2, 0.0, 4.0)],
        geom_nec=contact_ansi(closed=False)
        + [L(0.0, 0.0, 0.0, 1.8), rect(-1.6, 1.8, 1.6, 3.2), L(0.0, 3.2, 0.0, 4.0)],
    ),
    # ---- protection -----------------------------------------------------------
    dict(
        name="FUSE", cls="Fuse", attrs=A_2TERM,
        desc="Fuse: rectangle with the element drawn through it",
        geom=leads(inner=2.2) + rect(-2.2, -1.0, 2.2, 1.0) + [L(-2.2, 0.0, 2.2, 0.0)],
        geom_nec=leads(inner=2.2) + rect(-2.2, -1.0, 2.2, 1.0) + zigzag(-1.8, 1.8, 0.0, 0.7, 3),
    ),
    dict(
        name="FUSE_3PHASE", cls="Fuse", attrs=A_DEVICE,
        desc="Three-pole fuse carrier",
        geom=[L(-5.0, 1.6, -3.0, 1.6), L(-5.0, 0.0, -3.0, 0.0), L(-5.0, -1.6, -3.0, -1.6),
              L(3.0, 1.6, 5.0, 1.6), L(3.0, 0.0, 5.0, 0.0), L(3.0, -1.6, 5.0, -1.6)]
        + [L(-3.0, 2.3, 3.0, 2.3), L(-3.0, -2.3, 3.0, -2.3)]
        + [L(-3.0, 2.3, -3.0, -2.3), L(3.0, 2.3, 3.0, -2.3)]
        + [L(-3.0, 1.6, 0.0, 1.6), L(0.0, 1.6, 0.0, 0.0), L(3.0, 0.0, 0.0, 0.0),
           L(-3.0, -1.6, 0.0, -1.6), L(0.0, -1.6, 0.0, 0.0)],
    ),
    dict(
        name="MCB", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Miniature circuit breaker: switch blade with the trip box",
        geom=[L(-5.0, 0.0, -2.0, 0.0), L(-2.0, -BAR, -2.0, BAR), L(2.0, 0.0, 5.0, 0.0),
              L(-2.0, 0.0, 2.0, 2.0),
              rect(1.2, 1.6, 2.8, 3.2), L(1.2, 1.6, 2.8, 3.2), L(1.2, 3.2, 2.8, 1.6)],
        geom_nec=[L(-5.0, 0.0, -2.0, 0.0), L(-2.0, -BAR, -2.0, BAR), L(2.0, 0.0, 5.0, 0.0),
                  L(-2.0, 0.0, 2.6, 2.5), L(1.2, 1.6, 2.8, 2.8), L(2.8, 1.6, 1.2, 2.8)],
    ),
    dict(
        name="MCCB", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Moulded-case circuit breaker: trip unit on a wide body",
        geom=leads(inner=3.0) + rect(-3.0, -2.0, 3.0, 2.0)
        + [L(-3.0, 0.0, 3.0, 0.0), L(-0.6, 0.0, 1.6, 2.6),
           rect(1.2, 1.4, 2.8, 3.2), L(1.2, 1.4, 2.8, 3.2), L(1.2, 3.2, 2.8, 1.4)],
        geom_nec=leads(inner=3.0) + rect(-3.0, -2.0, 3.0, 2.0)
        + [L(-3.0, 0.0, 3.0, 0.0), L(-0.6, 0.0, 1.6, 2.6),
           rect(1.2, 1.4, 2.8, 3.2), L(1.2, 1.4, 2.8, 3.2), L(1.2, 3.2, 2.8, 1.4)],
    ),
    dict(
        name="RCD", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Residual-current device: toroid, trip and test button",
        geom=leads(inner=2.6) + rect(-2.6, -2.2, 2.6, 2.2)
        + [C(0.0, 0.6, 1.1), L(-2.6, 0.0, 2.6, 0.0),
           L(-1.6, -2.2, -1.6, -3.2), L(1.6, -2.2, 1.6, -3.2), C(0.0, -3.6, 0.5)],
    ),
    dict(
        name="SURGE_PROTECTOR", cls="Surge Protection", attrs=A_2TERM,
        desc="Surge protective device: varistor block to protective earth",
        geom=leads(inner=2.2) + rect(-2.2, -2.2, 2.2, 2.2)
        + [P([(-1.2, 2.2), (1.2, -2.2)]), arrow(1.2, -2.2, -1.0, 1.8, 0.8)]
        + [L(0.0, -2.2, 0.0, -3.4)] + ground_bars(-3.4, 1.4),
    ),
    dict(
        name="THERMAL_MAGNETIC", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Thermal-magnetic trip: bimetal zig-zag beside the coil",
        geom=leads(inner=2.6) + rect(-2.6, -2.2, 2.6, 2.2)
        + zigzag(-1.6, 1.6, 0.8, 0.7, 3) + [L(-1.6, -1.2, 1.6, -1.2), C(0.0, -1.2, 0.6)],
    ),
    dict(
        name="THERMAL_OVERLOAD", cls="Overload", attrs=A_2TERM,
        desc="Thermal overload relay: bimetal heater and its break contact",
        geom=leads(inner=2.6) + rect(-2.6, -2.0, 2.6, 2.0)
        + zigzag(-1.8, 1.8, 0.6, 0.8, 4) + [L(-1.8, -1.2, 1.8, -1.2)],
    ),
    # ---- machines and supplies -------------------------------------------------
    dict(
        name="MOTOR", cls="Motor", attrs=A_DEVICE,
        desc="Motor: circle with M",
        geom=circle_motor(False), geom_nec=circle_motor(False),
    ),
    dict(
        name="MOTOR_3PHASE", cls="Motor", attrs=A_DEVICE,
        desc="Three-phase motor: circle with M and 3~, three phase leads",
        geom=circle_motor(True)
        + [L(-2.1, 2.1, -3.4, 3.4), L(0.0, 3.0, 0.0, 4.4), L(2.1, 2.1, 3.4, 3.4)],
        geom_nec=circle_motor(False)
        + [L(-2.1, 2.1, -3.4, 3.4), L(0.0, 3.0, 0.0, 4.4), L(2.1, 2.1, 3.4, 3.4)],
    ),
    dict(
        name="SERVO_MOTOR", cls="Motor", attrs=A_DEVICE,
        desc="Servo motor: motor with an encoder box on the shaft",
        geom=circle_motor(False) + rect(-1.4, 3.2, 1.4, 4.8) + [L(0.0, 3.0, 0.0, 3.2)],
    ),
    dict(
        name="GEAR_MOTOR", cls="Motor", attrs=A_DEVICE,
        desc="Gear motor: motor with the gear rim",
        geom=circle_motor(False)
        + [L(0.0, 3.0, 0.0, 4.1), L(2.1, 2.1, 2.9, 2.9), L(3.0, 0.0, 4.1, 0.0),
           L(2.1, -2.1, 2.9, -2.9), L(0.0, -3.0, 0.0, -4.1), L(-2.1, -2.1, -2.9, -2.9),
           L(-3.0, 0.0, -4.1, 0.0), L(-2.1, 2.1, -2.9, 2.9)],
    ),
    dict(
        name="GENERATOR", cls="Generator", attrs=A_DEVICE,
        desc="Generator: circle with G",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              T("G", -0.9, -0.7, 1.6)],
    ),
    dict(
        name="TRANSFORMER", cls="Transformer", attrs=A_DEVICE,
        desc="Transformer: two windings either side of the core",
        geom=[L(-5.0, 0.0, -4.4, 0.0), L(-4.4, 0.0, -4.4, -1.8), L(-4.4, -1.8, -1.8, -1.8)]
        + winding(-1.8, -1.8, 1.8, 0.6, 1.0)
        + core_lines(0.0, -2.1, 2.1)
        + winding(1.8, -1.8, 1.8, 0.6, -1.0)
        + [L(1.8, 1.8, 4.4, 1.8), L(4.4, 1.8, 4.4, 0.0), L(4.4, 0.0, 5.0, 0.0)],
        geom_nec=[L(-5.0, 0.0, -1.6, 0.0), C(-1.6, 0.0, 1.6), C(1.6, 0.0, 1.6),
                  L(1.6, 0.0, 5.0, 0.0)],
    ),
    dict(
        name="TRANSFORMER_3PHASE", cls="Transformer", attrs=A_DEVICE,
        desc="Three-phase transformer: three winding pairs on one core",
        geom=[L(-5.0, y, -2.6, y) for y in (2.2, 0.0, -2.2)]
        + [w for y in (2.2, 0.0, -2.2) for w in winding(-2.6, y - 0.55, y + 0.55, 0.55, 1.0)]
        + core_lines(0.0, -3.0, 3.0)
        + [w for y in (2.2, 0.0, -2.2) for w in winding(2.6, y - 0.55, y + 0.55, 0.55, -1.0)]
        + [L(2.6, y, 5.0, y) for y in (2.2, 0.0, -2.2)],
    ),
    dict(
        name="PILOT_LAMP", cls="Lamp", attrs=A_DEVICE,
        desc="Pilot lamp: circle with the crossed filaments",
        geom=[L(-5.0, 0.0, -2.5, 0.0), L(2.5, 0.0, 5.0, 0.0), C(0.0, 0.0, 2.5),
              L(-1.8, -1.8, 1.8, 1.8), L(-1.8, 1.8, 1.8, -1.8)],
    ),
    dict(
        name="POWER_SUPPLY", cls="Power Supply", attrs=A_DEVICE,
        desc="Power supply: box with the DC output symbol",
        geom=device_box(3.0, 2.4)
        + [L(-1.6, -0.6, -0.6, -0.6), L(-1.1, -1.2, -1.1, 0.0),
           L(0.6, -0.6, 1.6, -0.6),
           dashed(-1.6, 1.4, 1.6, 1.4, 0.5, 0.35)],
    ),
    dict(
        name="UPS", cls="Power Supply", attrs=A_DEVICE,
        desc="Uninterruptible supply: box with the battery cells",
        geom=device_box(3.0, 2.5, "UPS")
        + [L(-2.0, -1.8, -1.0, -1.8), L(-0.6, -1.8, 0.4, -1.8),
           L(-2.0, -2.3, -1.0, -2.3), L(-0.6, -2.3, 0.4, -2.3)],
    ),
    dict(
        name="VFD", cls="Drive", attrs=A_DEVICE,
        desc="Variable-frequency drive: box with the rectifier bridge glyph",
        geom=device_box(3.0, 2.6)
        + [L(-1.6, 0.8, 1.6, 0.8), L(-1.6, 0.8, -0.9, 0.1), L(-1.6, 0.8, -0.9, 1.5),
           L(1.4, -0.9, 0.2, -0.9), L(1.4, -0.9, 0.7, -0.2), L(1.4, -0.9, 0.7, -1.6)],
    ),
    dict(
        name="SOFT_STARTER", cls="Drive", attrs=A_DEVICE,
        desc="Soft starter: box with the rising ramp",
        geom=device_box(3.0, 2.4) + [L(-1.4, -1.2, 0.2, -1.2), L(0.2, -1.2, 1.4, 0.8)],
    ),
    dict(
        name="ENERGY_METER", cls="Meter", attrs=A_DEVICE,
        desc="Energy meter: circle with kWh",
        geom=[L(-5.0, 0.0, -2.8, 0.0), L(2.8, 0.0, 5.0, 0.0), C(0.0, 0.0, 2.8),
              T("kWh", -1.9, -0.6, 1.0)],
    ),
    dict(
        name="HEATER", cls="Heater", attrs=A_2TERM,
        desc="Heating element: resistor zig-zag",
        geom=leads(inner=2.0) + zigzag(-2.0, 2.0, 0.0, 1.3, 4),
    ),
    dict(
        name="FAN", cls="Fan", attrs=A_DEVICE,
        desc="Fan: circle with three blades",
        geom=[L(-5.0, 0.0, -2.4, 0.0), L(2.4, 0.0, 5.0, 0.0), C(0.0, 0.0, 2.4)]
        + [A(0.0, 0.0, 1.2, 20.0, 160.0), A(0.0, 0.0, 1.2, 140.0, 280.0),
           A(0.0, 0.0, 1.2, 260.0, 400.0)],
    ),
    dict(
        name="BELL", cls="Bell", attrs=A_DEVICE,
        desc="Bell: dome with its striker",
        geom=[L(-5.0, 0.0, -2.0, 0.0), L(2.0, 0.0, 5.0, 0.0), L(-2.0, 0.0, -2.0, 0.6),
              A(0.0, 0.6, 2.2, 180.0, 360.0), L(2.0, 0.6, 2.0, 0.0),
              L(0.0, 2.8, 0.0, 3.6), C(0.4, 3.9, 0.4)],
    ),
    dict(
        name="BUZZER", cls="Buzzer", attrs=A_DEVICE,
        desc="Buzzer: semicircle body with sound arcs",
        geom=[L(-5.0, 0.0, -2.0, 0.0), L(2.0, 0.0, 5.0, 0.0), L(-2.0, 0.0, -2.0, 1.0),
              A(0.0, 1.0, 2.0, 180.0, 360.0), L(2.0, 1.0, 2.0, 0.0),
              A(0.0, 1.0, 2.8, 200.0, 340.0), A(0.0, 1.0, 3.6, 210.0, 330.0)],
    ),
    dict(
        name="SOLENOID_VALVE", cls="Solenoid", attrs=A_2TERM,
        desc="Solenoid valve: coil beside the two-way valve body",
        geom=leads(inner=2.2) + rect(-2.2, -1.8, 2.2, 1.8)
        + [P([(2.2, 1.8), (4.6, 0.6), (4.6, -0.6), (2.2, -1.8)], closed=True)]
        + dashed(0.0, 1.8, 0.0, 3.2, 0.4, 0.3),
    ),
    dict(
        name="VALVE_2WAY", cls="Valve", attrs=A_2TERM,
        desc="Two-way valve: opposed triangles with the actuator stem",
        geom=leads(inner=2.6) + [P([(-2.6, 1.6), (0.0, -0.4), (2.6, 1.6), (0.0, 0.4)], closed=True)]
        + [L(0.0, 1.6, 0.0, 3.2), L(-1.2, 3.2, 1.2, 3.2)],
    ),
    dict(
        name="VALVE_3WAY", cls="Valve", attrs=A_DEVICE,
        desc="Three-way valve: two opposed bodies on one stem",
        geom=[L(-5.0, 0.0, -3.4, 0.0), L(3.4, 0.0, 5.0, 0.0), L(0.0, 3.0, 0.0, 3.8),
              L(-1.2, 3.8, 1.2, 3.8)]
        + [P([(-3.4, 1.4), (0.0, 0.0), (-3.4, -1.4)], closed=True),
           P([(3.4, 1.4), (0.0, 0.0), (3.4, -1.4)], closed=True)],
    ),
    # ---- sensors ---------------------------------------------------------------
    dict(
        name="PROXIMITY_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Inductive proximity switch: body, sensing face and target",
        geom=leads(inner=1.2) + rect(-1.2, -2.0, 2.2, 2.0)
        + [L(2.2, 1.2, 3.4, 1.2), L(3.4, 1.2, 3.4, -1.2), L(3.4, -1.2, 2.2, -1.2)],
    ),
    dict(
        name="PHOTO_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Photoelectric switch: body with the light beam and target",
        geom=leads(inner=1.2) + rect(-1.2, -2.0, 2.2, 2.0)
        + arrow(-1.2, 0.6, -2.2, 0.0, 0.8) + [L(-3.6, 1.4, -3.6, -1.4)],
    ),
    dict(
        name="CAPACITIVE_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Capacitive switch: body with the field plates",
        geom=leads(inner=1.2) + rect(-1.2, -2.0, 2.2, 2.0)
        + [L(2.8, 1.2, 2.8, -1.2), L(3.6, 1.2, 3.6, -1.2)],
    ),
    dict(
        name="ULTRASONIC_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Ultrasonic switch: body with the sound cone",
        geom=leads(inner=1.2) + rect(-1.2, -2.0, 2.2, 2.0)
        + [A(2.2, 0.0, 1.0, -50.0, 50.0), A(2.2, 0.0, 1.8, -50.0, 50.0)],
    ),
    dict(
        name="TEMPERATURE_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Temperature probe: bulb and stem",
        geom=leads(inner=1.4) + rect(-1.4, -2.0, 1.4, 1.4) + [L(0.0, 1.4, 0.0, 2.6), C(0.0, 3.2, 0.6)],
    ),
    dict(
        name="ENCODER", cls="Sensor", attrs=A_2TERM,
        desc="Encoder: body with the slotted disc",
        geom=leads(inner=1.6) + rect(-1.6, -1.8, 1.6, 1.8)
        + [C(0.0, 0.0, 1.1), L(-1.1, 0.0, 1.1, 0.0), L(0.0, -1.1, 0.0, 1.1)],
    ),
    # ---- PLC and I/O -----------------------------------------------------------
    dict(
        name="PLC-DI", cls="PLC Digital Input", attrs=A_PLC, pins=PINS_PLC_DI,
        desc="PLC digital input module: two inputs on the left",
        geom=[L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0)] + rect(-4.0, -3.0, 4.0, 3.0)
        + [T("DI", -1.0, 0.6, 1.2), arrow(-4.0, 1.0, 1.4, 0.0, 0.8), arrow(-4.0, -1.0, 1.4, 0.0, 0.8)],
    ),
    dict(
        name="PLC-DO", cls="PLC Digital Output", attrs=A_PLC, pins=PINS_PLC_DO,
        desc="PLC digital output module: two outputs on the right",
        geom=[L(4.0, 1.0, 5.0, 1.0), L(4.0, -1.0, 5.0, -1.0)] + rect(-4.0, -3.0, 4.0, 3.0)
        + [T("DO", -1.0, 0.6, 1.2), arrow(4.0, 1.0, 1.6, 0.0, 0.8), arrow(4.0, -1.0, 1.6, 0.0, 0.8)],
    ),
    dict(
        name="PLC-AI", cls="PLC Analog Input", attrs=A_PLC, pins=PINS_PLC_DI,
        desc="PLC analog input module",
        geom=[L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0)] + rect(-4.0, -3.0, 4.0, 3.0)
        + [T("AI", -1.0, 0.6, 1.2), L(-3.2, 1.0, -1.8, 0.4), L(-1.8, 0.4, -2.4, 0.4),
           L(-1.8, 0.4, -2.2, 1.0)],
    ),
    dict(
        name="PLC-AO", cls="PLC Analog Output", attrs=A_PLC, pins=PINS_PLC_DO,
        desc="PLC analog output module",
        geom=[L(4.0, 1.0, 5.0, 1.0), L(4.0, -1.0, 5.0, -1.0)] + rect(-4.0, -3.0, 4.0, 3.0)
        + [T("AO", -1.0, 0.6, 1.2), L(1.8, 0.4, 3.2, 1.0), L(2.6, 1.0, 3.2, 1.0),
           L(3.2, 0.4, 3.2, 1.0)],
    ),
    dict(
        name="PLC-CPU", cls="PLC", attrs=A_PLC,
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02"), (5.0, 0.0, "X1TERM03")],
        desc="PLC processor: body, run LED and the programming port",
        geom=[L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0), L(4.0, 0.0, 5.0, 0.0)]
        + rect(-4.0, -3.0, 4.0, 3.0)
        + [T("CPU", -1.6, 0.4, 1.2), C(2.4, 2.2, 0.4), rect(1.4, -2.4, 3.2, -1.0)],
    ),
    dict(
        name="PLC-PSU", cls="PLC", attrs=A_PLC, pins=PINS_PLC_DI,
        desc="PLC power supply: 24 V output",
        geom=[L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0)] + rect(-4.0, -3.0, 4.0, 3.0)
        + [T("24V", -1.4, 0.4, 1.2), L(1.4, -1.4, 2.6, -1.4), L(2.0, -1.8, 2.0, -1.0)],
    ),
    dict(
        name="PLC-COUPLER", cls="PLC", attrs=A_PLC, pins=PINS_NETWORK_SWITCH,
        desc="PLC bus coupler: two field leads and the bus symbol",
        geom=[L(-5.0, 1.0, -3.4, 1.0), L(-5.0, -1.0, -3.4, -1.0),
              L(3.4, 1.0, 5.0, 1.0), L(3.4, -1.0, 5.0, -1.0)] + rect(-3.4, -2.4, 3.4, 2.4)
        + [T("CPL", -1.4, 0.2, 1.2), L(-2.4, -1.6, 2.4, -1.6),
           L(-1.6, -2.0, -1.6, -1.2), L(1.6, -2.0, 1.6, -1.2)],
    ),
    dict(
        name="IO_MODULE", cls="IO Module", attrs=A_DEVICE, pins=PINS_IO_MODULE,
        desc="Field I/O module: eight terminals on a slim body",
        geom=rect(-3.5, -4.0, 3.5, 4.0)
        + [L(-5.0, 3.0, -3.5, 3.0), L(-5.0, 1.0, -3.5, 1.0),
           L(-5.0, -1.0, -3.5, -1.0), L(-5.0, -3.0, -3.5, -3.0),
           L(3.5, 3.0, 5.0, 3.0), L(3.5, 1.0, 5.0, 1.0),
           L(3.5, -1.0, 5.0, -1.0), L(3.5, -3.0, 5.0, -3.0),
           T("I/O", -1.4, -0.6, 1.2)],
    ),
    dict(
        name="NETWORK_SWITCH", cls="Network Switch", attrs=A_DEVICE, pins=PINS_NETWORK_SWITCH,
        desc="Managed Ethernet switch: four port leads",
        geom=rect(-3.0, -2.5, 3.0, 2.5)
        + [L(-5.0, 1.0, -3.0, 1.0), L(-5.0, -1.0, -3.0, -1.0),
           L(3.0, 1.0, 5.0, 1.0), L(3.0, -1.0, 5.0, -1.0),
           T("SW", -1.2, -0.5, 1.2),
           rect(-2.2, 1.6, 0.2, 2.2), rect(0.4, 1.6, 2.4, 2.2)],
    ),
    dict(
        name="HMI", cls="HMI", attrs=A_DEVICE, pins=PINS_HMI,
        desc="Operator panel: screen in a bezel",
        geom=rect(-3.0, -4.0, 3.0, 4.0) + rect(-2.2, -2.8, 2.2, 2.2)
        + [L(-5.0, 1.5, -3.0, 1.5), L(-5.0, -1.5, -3.0, -1.5), L(3.0, 0.0, 5.0, 0.0),
           T("HMI", -1.6, -3.6, 1.0)],
    ),
    # ---- terminals and wiring --------------------------------------------------
    dict(
        name="TERMINAL", cls="Terminal", attrs=A_TERMINAL,
        desc="Feed-through terminal: open circle on the conductor",
        geom=terminal_circle() + [L(0.0, 1.2, 0.0, 2.2)],
    ),
    dict(
        name="TERMINAL_BLOCK", cls="Terminal", attrs=A_TERMINAL, pins=PINS_TERMINAL_BLOCK,
        desc="Three-way terminal block on its rail",
        geom=rect(-5.0, -1.5, 5.0, 2.5)
        + [C(-3.0, 0.5, 0.8), C(0.0, 0.5, 0.8), C(3.0, 0.5, 0.8),
           L(-5.0, -1.5, 5.0, -1.5)],
    ),
    dict(
        name="TERMINAL_GROUND", cls="Terminal", attrs=A_TERMINAL, pins=PINS_EARTH,
        desc="Protective-earth terminal: circle over the earth bars",
        geom=[L(0.0, 3.5, 0.0, 1.4), C(0.0, 0.6, 1.0)] + ground_bars(-0.6, 1.4),
    ),
    dict(
        name="TERMINAL_END_STOP", cls="Terminal", attrs=A_TERMINAL, pins=PINS_NONE,
        desc="End stop: bracket that closes the terminal rail",
        geom=rect(-1.0, -2.4, 1.0, 2.4)
        + [L(-1.0, -2.4, -2.6, -3.2), L(1.0, -2.4, 2.6, -3.2), L(-2.6, 0.0, -1.0, 0.0)],
    ),
    dict(
        name="TERMINAL_SEPARATOR", cls="Terminal", attrs=A_TERMINAL, pins=PINS_NONE,
        desc="Group separator: the plate between terminal groups",
        geom=[L(-1.4, -2.4, -1.4, 2.4), L(1.4, -2.4, 1.4, 2.4)]
        + dashed(-1.4, 0.0, 1.4, 0.0, 0.5, 0.35),
    ),
    dict(
        name="CONNECTOR_2P", cls="Connector", attrs=A_DEVICE,
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02"),
              (5.0, 1.0, "X1TERM03"), (5.0, -1.0, "X1TERM04")],
        desc="Two-pole connector: plug body with two pins either side",
        geom=[L(-5.0, 1.0, -2.6, 1.0), L(-5.0, -1.0, -2.6, -1.0),
              L(2.6, 1.0, 5.0, 1.0), L(2.6, -1.0, 5.0, -1.0)]
        + rect(-2.6, -2.0, 2.6, 2.0) + [C(-1.2, 1.0, 0.3), C(-1.2, -1.0, 0.3)],
    ),
    dict(
        name="CONNECTOR_4P", cls="Connector", attrs=A_DEVICE,
        pins=[(-5.0, 1.5, "X4TERM01"), (-5.0, 0.5, "X4TERM02"),
              (-5.0, -0.5, "X4TERM03"), (-5.0, -1.5, "X4TERM04"),
              (5.0, 1.5, "X1TERM05"), (5.0, 0.5, "X1TERM06"),
              (5.0, -0.5, "X1TERM07"), (5.0, -1.5, "X1TERM08")],
        desc="Four-pole connector: four pins either side",
        geom=rect(-2.6, -2.4, 2.6, 2.4)
        + [L(-5.0, y, -2.6, y) for y in (1.5, 0.5, -0.5, -1.5)]
        + [L(2.6, y, 5.0, y) for y in (1.5, 0.5, -0.5, -1.5)]
        + [C(-1.2, 1.5, 0.3), C(-1.2, -1.5, 0.3)],
    ),
    dict(
        name="TEST_POINT", cls="Test Point", attrs=A_TERMINAL, pins=[(-5.0, 0.0, "X4TERM01")],
        desc="Test point: tapping stud on the conductor",
        geom=[L(-5.0, 0.0, -1.4, 0.0), C(0.0, 0.0, 1.4), L(0.0, 1.4, 0.0, 2.4)],
    ),
    dict(
        name="EARTH", cls="Earth", attrs=A_EARTH, pins=PINS_EARTH,
        desc="Protective earth: conductor to the earth bars",
        geom=[L(0.0, 3.5, 0.0, 0.0)] + ground_bars(0.0, 1.6),
    ),
    dict(
        name="JUNCTION", cls="Junction", attrs=A_EARTH, pins=PINS_NONE,
        desc="Junction dot where three conductors meet",
        geom=[C(0.0, 0.0, 0.9), C(0.0, 0.0, 0.5)],
    ),
    dict(
        name="DIN_RAIL", cls="Rail", attrs=A_EARTH, pins=PINS_NONE,
        desc="DIN rail: hat profile in section",
        geom=[L(-5.0, -1.0, -4.2, -1.0), L(-4.2, -1.0, -4.2, 1.0), L(-4.2, 1.0, 4.2, 1.0),
              L(4.2, 1.0, 4.2, -1.0), L(4.2, -1.0, 5.0, -1.0),
              L(-3.0, -1.0, -3.0, 1.0), L(-1.0, -1.0, -1.0, 1.0),
              L(1.0, -1.0, 1.0, 1.0), L(3.0, -1.0, 3.0, 1.0)],
    ),
    dict(
        name="WIRE_DUCT", cls="Wire Duct", attrs=A_EARTH, pins=PINS_NONE,
        desc="Wire duct: slotted trunking",
        geom=rect(-4.0, -2.0, 4.0, 2.0)
        + [L(-4.0, 0.8, 4.0, 0.8), L(-4.0, -0.8, 4.0, -0.8)]
        + [L(x, -2.0, x, 2.0) for x in (-2.6, -1.0, 1.0, 2.6)],
    ),
    dict(
        name="CABLE_ENTRY", cls="Cable", attrs=A_DEVICE, pins=PINS_CABLE_ENTRY,
        desc="Cable entry: gland with the cable and its core",
        geom=[L(-5.0, 0.0, -2.4, 0.0), C(0.0, 0.0, 2.0), L(1.4, 1.4, 3.4, 3.4),
              L(0.0, -2.0, 0.0, -3.2), L(-0.9, -2.6, 0.9, -2.6)],
    ),
    dict(
        name="WIRE_LABEL", cls="Wire Label", attrs=A_LABEL,
        desc="Wire number label drawn on the conductor",
        geom=rect(-4.0, -1.5, 4.0, 1.5) + [L(-5.0, 0.0, -4.0, 0.0), L(4.0, 0.0, 5.0, 0.0)],
    ),
]


# --------------------------------------------------------------------------- #
# geometry helpers
# --------------------------------------------------------------------------- #


def geometry_bounds(geom):
    """Bounds of the drawable primitives.

    TEXT is deliberately excluded: ``elec_symbol_segments`` (the palette and the
    label-height measurement) only walks LINE/CIRCLE/ARC/LWPOLYLINE, so the span
    this function reports is exactly the span ``ELECLIB`` will measure.
    """
    min_x = min_y = math.inf
    max_x = max_y = -math.inf

    def take(x, y):
        nonlocal min_x, min_y, max_x, max_y
        min_x = min(min_x, x)
        min_y = min(min_y, y)
        max_x = max(max_x, x)
        max_y = max(max_y, y)

    for op in geom:
        kind = op[0]
        if kind == "line":
            _, x1, y1, x2, y2 = op
            take(x1, y1)
            take(x2, y2)
        elif kind == "circle":
            _, cx, cy, r = op
            take(cx - r, cy - r)
            take(cx + r, cy + r)
        elif kind == "arc":
            _, cx, cy, r, start, end = op
            for x, y in dxfio.arc_points(cx, cy, r, start, end, steps=16):
                take(x, y)
        elif kind == "poly":
            for x, y in op[1]:
                take(x, y)
        elif kind == "solid":
            for x, y in op[1]:
                take(x, y)
    return min_x, min_y, max_x, max_y


def label_height_for(span):
    """The height ELECLIB would measure for a symbol of this span (span / 8)."""
    if span <= 0.0:
        return 0.125
    return min(max(span / 8.0, 0.01), 50.0)


def emit_entity(doc, op):
    kind = op[0]
    if kind == "line":
        doc.entities.append(dxfio.DxfDocument._line(*op[1:]))
    elif kind == "circle":
        doc.entities.append(dxfio.DxfDocument._circle(*op[1:]))
    elif kind == "arc":
        doc.entities.append(dxfio.DxfDocument._arc(*op[1:]))
    elif kind == "poly":
        doc.entities.append(dxfio.DxfDocument._lwpolyline(op[1], op[2]))
    elif kind == "solid":
        doc.entities.append(dxfio.DxfDocument._solid(op[1]))
    elif kind == "text":
        doc.entities.append(dxfio.DxfDocument._text(*op[1:]))
    else:  # pragma: no cover - a typo in the catalogue, not a runtime path
        raise ValueError(f"unknown geometry op {kind!r}")


def scale_ops(geom, factor):
    """Scale a geometry list from the 10-unit design canvas to drawing units."""
    scaled = []
    for op in geom:
        kind = op[0]
        if kind == "line":
            _, x1, y1, x2, y2 = op
            scaled.append(("line", x1 * factor, y1 * factor, x2 * factor, y2 * factor))
        elif kind == "circle":
            _, cx, cy, r = op
            scaled.append(("circle", cx * factor, cy * factor, r * factor))
        elif kind == "arc":
            _, cx, cy, r, start, end = op
            scaled.append(("arc", cx * factor, cy * factor, r * factor, start, end))
        elif kind == "poly":
            scaled.append(("poly", [(x * factor, y * factor) for x, y in op[1]], op[2]))
        elif kind == "solid":
            scaled.append(("solid", [(x * factor, y * factor) for x, y in op[1]]))
        elif kind == "text":
            _, value, x, y, height, rotation = op
            scaled.append(("text", value, x * factor, y * factor, height * factor, rotation))
        else:  # pragma: no cover
            raise ValueError(f"unknown geometry op {kind!r}")
    return scaled


def flatten(geom):
    """One flat op list, so a builder that returns a list can sit inside a list."""
    flat = []
    for op in geom:
        if isinstance(op[0], str):
            flat.append(op)
        else:
            flat.extend(flatten(op))
    return flat


# Shapes imported from the QElectroTech element collection (see
# ``tools/qet_import.py``).  They replace the hand-drawn geometry for the blocks
# they cover, while the block name, class and attribute set stay ours.
QET_SYMBOLS = {}
_qet_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), "qet_symbols.json")
if os.path.exists(_qet_file):
    with open(_qet_file, encoding="utf-8") as _handle:
        QET_SYMBOLS = json.load(_handle)


# --------------------------------------------------------------------------- #
# NECA 100 published construction symbols
# --------------------------------------------------------------------------- #

# ``tools/neca_symbols.py`` unfolds every drawing in the Standard's CAD set to
# 10-unit canvas ops; ``tools/neca_index.py`` reads the Standard's own text to
# name each one.  These are drawings of real symbols, so the NEMA library uses
# them verbatim instead of hand-drawn approximations.
NECA_SHAPES = {}
_neca_shapes_path = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "neca_shapes.json"
)
if os.path.exists(_neca_shapes_path):
    with open(_neca_shapes_path, encoding="utf-8") as _handle:
        NECA_SHAPES = json.load(_handle)

NECA_INDEX = {}
_neca_index_path = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "neca_index.json"
)
if os.path.exists(_neca_index_path):
    with open(_neca_index_path, encoding="utf-8") as _handle:
        NECA_INDEX = json.load(_handle)

# Schedules, legends, one-line risers and the NFPA 170 plates are drawings, not
# symbols; the abbreviation sheet is a legend as well.
NECA_SKIP_SECTIONS = {"Schedules", "One-Line", "13.0", "NFPA"}

# The Standard's section numbers, named the way the palette groups them.
NECA_CLASSES = {
    "1.0": "NECA Wiring Methods",
    "2.0": "NECA Luminaire Fixtures",
    "3.0": "NECA Outlets",
    "4.0": "NECA Switches",
    "5.0": "NECA Motor Controls",
    "6.0": "NECA Security",
    "7.0": "NECA Fire Alarm",
    "8.0": "NECA Power Distribution",
    "9.0": "NECA Teledata",
    "10.0": "NECA Site Work",
    "11.0": "NECA Ground Fault",
    "12.0": "NECA Miscellaneous",
    "14.0": "NECA Nurse Call",
}

# A drawing keeps its identifying letter (a thermostat's "T", a pull station's
# "F"); the rest of its text is the Standard's example annotation ("240A",
# "(a)", "xxxV x%%C xW PRI") and does not belong on a library symbol.  Up to two
# characters may carry a digit: a switch's pole count is written "S2"/"S3"/"4".
NECA_GLYPH = re.compile(r"^(?:[A-Za-z&+]{1,5}|[A-Za-z0-9]{1,2})$")


def neca_geometry(geom):
    """The drawable ops of a NECA drawing, with example annotation removed."""
    ops = []
    for op in geom:
        if op[0] == "text":
            if not NECA_GLYPH.match(op[1]):
                continue
            ops.append(tuple(op))
        else:
            ops.append(tuple(op))
    return ops


def _slug(text, limit):
    """`SINGLE POLE SWITCH, 20A` -> `SINGLE_POLE_SWITCH` (at most `limit`)."""
    words = re.findall(r"[A-Z0-9]+", text.upper())
    out = []
    length = 0
    for word in words:
        extra = len(word) if not out else len(word) + 1
        if length + extra > limit:
            break
        out.append(word)
        length += extra
    # A name that ends on a preposition reads like a truncation.
    while out and out[-1] in {"A", "AN", "AND", "AS", "AT", "BY", "FOR", "IN", "OF", "ON", "OR", "THE", "TO", "WITH"}:
        out.pop()
    return "_".join(out)


def neca_catalogue():
    """Catalogue entries for the published NECA 100 construction symbols.

    The block name is ``N<code>_<words of the Standard's description>``, so the
    palette can be searched by either; the description of the Standard is kept
    on the entry for documentation.
    """
    entries = []
    used = {entry["name"] for entry in SYMBOLS}
    for key, shape in sorted(NECA_SHAPES.items()):
        section, stem = key.split("/", 1)
        meta = NECA_INDEX.get(stem, {})
        file_section = meta.get("file_section", section)
        if file_section in NECA_SKIP_SECTIONS:
            continue
        description = (meta.get("name") or "").strip()
        if not description or "Legend" in description or "Schedule" in description:
            continue
        geom = neca_geometry(shape["geom"])
        if not any(op[0] != "text" for op in geom):
            continue
        code = stem.upper()
        prefix = "N" + code
        room = max(40 - len(prefix) - 1, 4)
        name = prefix + "_" + (_slug(description, room) or "SYMBOL")
        if name in used:
            name = name[:38] + "_2"
        used.add(name)
        entries.append(
            dict(
                name=name,
                cls=NECA_CLASSES.get(file_section, "NECA Symbols"),
                attrs=A_DEVICE,
                desc=description,
                geom=geom,
                # No declared pins: the drawing has no terminal objects, so the
                # app derives the connection points from its leads (`insert_pins`).
                pins=[],
            )
        )
    return entries



def build_symbol(entry):
    """Return (dxf document, manifest row tuple) for one catalogue entry."""
    name = entry["name"]
    # Which geometry family the run uses:
    #   IEC  - the published EN 60617 shape from QElectroTech when there is one,
    #          otherwise our hand-drawn IEC drawing;
    #   NEMA - the NECA 100 drawing for a construction-symbol entry, the
    #          hand-drawn ANSI variant where a block has one, otherwise our IEC
    #          drawing.  The EN 60617 shape is never used: it is the other
    #          standard, and handing it to a NEMA run was the old bug.
    if STANDARD == "nec":
        geom = scale_ops(flatten(entry.get("geom_nec") or entry["geom"]), UNIT_SCALE)
        pins = entry.get("pins", PINS_2TERM)
    else:
        imported = QET_SYMBOLS.get(name)
        if imported is not None:
            geom = scale_ops(flatten(imported["geom"]), UNIT_SCALE)
            pins = [tuple(pin) for pin in imported["pins"]] or entry.get(
                "pins", PINS_2TERM
            )
        else:
            geom = scale_ops(flatten(entry["geom"]), UNIT_SCALE)
            pins = entry.get("pins", PINS_2TERM)
    # Whatever produced the pin list - our constants, the QET import, the NECA import -
    # the numbers are made unique here, so a stale intermediate cannot ship a symbol
    # whose two ends are both pin 1.
    pins = canonical_pins(pins)
    attrs = entry["attrs"]

    doc = dxfio.DxfDocument()
    for op in geom:
        emit_entity(doc, op)

    min_x, min_y, max_x, max_y = geometry_bounds(geom)
    span = max(max_x - min_x, max_y - min_y)
    height = round(label_height_for(span), 4)
    step = height * 1.25
    # Attributes hang below the lowest geometry, one text height clear of it -
    # the same rule elec_insert uses for a symbol with no attribute definitions.
    for row, tag in enumerate(attrs):
        doc.entities.append(
            dxfio.DxfDocument.attdef(
                tag=tag,
                prompt=_prompt_for(tag),
                default="",
                height=height,
                x=min_x,
                y=min_y - height - row * step,
            )
        )

    # Wire connection points sit ON the geometry (the end of a lead), and are
    # invisible: they are electrical data, not text on the drawing.
    for px, py, tag in pins:
        doc.entities.append(
            dxfio.DxfDocument.attdef(
                tag=tag,
                prompt=_prompt_for(tag),
                default="",
                height=height,
                x=px * UNIT_SCALE,
                y=py * UNIT_SCALE,
                flags=1,
            )
        )

    sector = 0
    documented = attrs + [tag for _, _, tag in pins]
    # The last column says which standard the drawing is in, so the palette can name
    # the library it is showing (`ELECLIB NEC` loads library/nec, every row NEC).
    standard = "NEC" if STANDARD == "nec" else "IEC"
    return doc, (name, entry["cls"], "|".join(documented), str(LIBRARY_SCALE), standard)


PROMPTS = {
    "TAG1": "Tag",
    "CAT": "Part number",
    "MFG": "Manufacturer",
    "DESC1": "Description",
    "LOC": "Location",
    "TERM01": "Terminal 1",
    "TERM02": "Terminal 2",
    "XREF": "Cross reference",
    "ADDR01": "Address 1",
    "ADDR02": "Address 2",
    "X1TERM01": "Pin 1 position",
    "X1TERM02": "Pin 2 position",
}


def _prompt_for(tag):
    return PROMPTS.get(tag, tag)


# --------------------------------------------------------------------------- #
# generation
# --------------------------------------------------------------------------- #


def build_all():
    """Return (files: {name: text}, manifest: str) without touching the disk."""
    entries = list(SYMBOLS)
    if STANDARD == "nec":
        entries.extend(neca_catalogue())
    files = {}
    rows = []
    for entry in entries:
        doc, row = build_symbol(entry)
        files[f"{entry['name']}.dxf"] = doc.render()
        rows.append(row)
    rows.sort(key=lambda row: (row[1].lower(), row[0]))
    manifest = HEADER + "\n" + "".join("\t".join(row) + "\n" for row in rows)
    return files, manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=DEFAULT_OUT, help="output folder (default: library/)")
    parser.add_argument("--check", action="store_true",
                        help="compare with the files on disk instead of writing")
    parser.add_argument("--standard", choices=("iec", "nec"), default="iec",
                        help="symbology family: IEC 60617 (default) or NEMA/ANSI")
    args = parser.parse_args(argv)

    global STANDARD
    STANDARD = args.standard
    if args.out == DEFAULT_OUT and args.standard == "nec":
        args.out = os.path.join(ROOT, "library", "nec")
    files, manifest = build_all()

    if args.check:
        problems = []
        for name, text in files.items():
            path = os.path.join(args.out, name)
            if not os.path.exists(path):
                problems.append(f"missing {name}")
                continue
            with open(path, "r", encoding="ascii") as handle:
                if handle.read() != text:
                    problems.append(f"differs {name}")
        manifest_path = os.path.join(args.out, "library.tsv")
        if not os.path.exists(manifest_path):
            problems.append("missing library.tsv")
        else:
            with open(manifest_path, "r", encoding="ascii") as handle:
                if handle.read() != manifest:
                    problems.append("differs library.tsv")
        extra = sorted(
            name for name in os.listdir(args.out)
            if name.lower().endswith(".dxf") and name not in files
        )
        if extra:
            problems.append("unexpected dxf: " + ", ".join(extra))
        if problems:
            print("library is NOT in sync with make_library.py:")
            for problem in problems:
                print("  -", problem)
            return 1
        print(f"library is in sync: {len(files)} symbols, {len(manifest.splitlines()) - 1} manifest rows")
        return 0

    os.makedirs(args.out, exist_ok=True)
    for name, text in files.items():
        with open(os.path.join(args.out, name), "w", encoding="ascii", newline="\n") as handle:
            handle.write(text)
    with open(os.path.join(args.out, "library.tsv"), "w", encoding="ascii", newline="\n") as handle:
        handle.write(manifest)
    print(f"wrote {len(files)} symbol DXF files and library.tsv to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
