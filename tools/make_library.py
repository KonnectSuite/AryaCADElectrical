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
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dxfio  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_OUT = os.path.join(ROOT, "library", "starter")

# 10-unit design canvas -> 1-unit drawing symbols (see "Scale" above).
UNIT_SCALE = 0.1

# The symbols stay ~1 drawing unit (the harvested client convention), so on the
# millimetre sheets the manifest asks ``ELECLIB``/``ELECPLACE`` for 25.4 x scale.
LIBRARY_SCALE = 25.4

HEADER = "symbol\tclass\tattributes\tscale"

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
    (5.0, 1.0, "X1TERM01"),
    (5.0, -1.0, "X1TERM02"),
]
PINS_HMI = [
    (-5.0, 1.5, "X4TERM01"),
    (-5.0, -1.5, "X4TERM02"),
    (5.0, 0.0, "X1TERM01"),
]
PINS_IO_MODULE = [
    (-5.0, 3.0, "X4TERM01"),
    (-5.0, 1.0, "X4TERM02"),
    (-5.0, -1.0, "X4TERM03"),
    (-5.0, -3.0, "X4TERM04"),
    (5.0, 3.0, "X1TERM01"),
    (5.0, 1.0, "X1TERM02"),
    (5.0, -1.0, "X1TERM03"),
    (5.0, -3.0, "X1TERM04"),
]


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
# the symbol set
# --------------------------------------------------------------------------- #
# Each entry: block/file name, class (drives the tag prefix), attribute set,
# geometry, and a one-line description for the report.

SYMBOLS = [
    dict(
        name="CONTACTOR_COIL", cls="Contactor", attrs=A_2TERM,
        desc="Contactor / relay operating coil, rectangle body",
        geom=leads(inner=2.5) + rect(-2.5, -2.5, 2.5, 2.5),
    ),
    dict(
        name="RELAY_COIL", cls="Relay", attrs=A_2TERM,
        desc="Relay coil: rectangle with the diagonal that distinguishes it from a contactor coil",
        geom=leads(inner=2.5) + rect(-2.5, -2.5, 2.5, 2.5) + [L(-2.5, -2.5, 2.5, 2.5)],
    ),
    dict(
        name="CONTACT_NO", cls="Contactor", attrs=A_2TERM,
        desc="Normally-open contact",
        geom=no_contact(),
    ),
    dict(
        name="CONTACT_NC", cls="Contactor", attrs=A_2TERM,
        desc="Normally-closed contact",
        geom=nc_contact(),
    ),
    dict(
        name="PUSHBUTTON_NO", cls="Button", attrs=A_2TERM,
        desc="Push-button, normally-open (momentary)",
        geom=no_contact() + actuator(),
    ),
    dict(
        name="PUSHBUTTON_NC", cls="Button", attrs=A_2TERM,
        desc="Push-button, normally-closed (momentary)",
        geom=nc_contact() + actuator(),
    ),
    dict(
        name="SELECTOR_SWITCH", cls="Selector", attrs=A_2TERM,
        desc="Two-position selector switch with a lever",
        geom=nc_contact() + [L(0.0, 0.0, 2.0, 3.0), C(2.3, 3.3, 0.4)],
    ),
    dict(
        name="LIMIT_SWITCH", cls="Limit Switch", attrs=A_2TERM,
        desc="Limit switch: open contact with a roller actuator",
        geom=no_contact() + [L(0.0, 0.0, 0.0, 3.0), C(0.0, 3.6, 0.6)],
    ),
    dict(
        name="FUSE", cls="Fuse", attrs=A_2TERM,
        desc="Fuse: rectangle with the diagonal through it",
        geom=leads(inner=1.25) + rect(-2.5, -1.25, 2.5, 1.25) + [L(-2.5, -1.25, 2.5, 1.25)],
    ),
    dict(
        name="MCB", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Miniature circuit breaker: switch with a trip cross",
        geom=leads(inner=2.5) + rect(-2.5, -1.5, 2.5, 1.5)
        + [L(-2.5, -1.5, 2.5, 1.5), L(0.0, 4.0, 1.0, 5.0), L(1.0, 4.0, 0.0, 5.0)],
    ),
    dict(
        name="THERMAL_OVERLOAD", cls="Overload", attrs=A_2TERM,
        desc="Thermal overload: heater element drawn as a zig-zag",
        geom=leads(inner=2.5) + rect(-2.5, -2.0, 2.5, 2.0)
        + [P([(-1.5, 0.0), (-0.75, 1.2), (0.0, 0.0), (0.75, 1.2), (1.5, 0.0)])],
    ),
    dict(
        name="MOTOR", cls="Motor", attrs=A_DEVICE,
        desc="Motor: circle with M",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              T("M", -1.0, -0.9, 1.8)],
    ),
    dict(
        name="MOTOR_3PHASE", cls="Motor", attrs=A_DEVICE,
        desc="Three-phase motor: circle with M and three phase stubs",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              L(-2.1, 2.1, -3.2, 3.2), L(0.0, 3.0, 0.0, 4.2), L(2.1, 2.1, 3.2, 3.2),
              T("M", -1.0, -0.9, 1.8)],
    ),
    dict(
        name="PILOT_LAMP", cls="Lamp", attrs=A_DEVICE,
        desc="Pilot lamp: circle with an X",
        geom=[L(-5.0, 0.0, -2.5, 0.0), L(2.5, 0.0, 5.0, 0.0), C(0.0, 0.0, 2.5),
              L(-1.8, -1.8, 1.8, 1.8), L(-1.8, 1.8, 1.8, -1.8)],
    ),
    dict(
        name="TERMINAL", cls="Terminal", attrs=A_TERMINAL,
        desc="Single terminal: open circle on the wire line",
        geom=[L(-5.0, 0.0, -1.2, 0.0), L(1.2, 0.0, 5.0, 0.0), C(0.0, 0.0, 1.2),
              L(0.0, 1.2, 0.0, 2.4)],
    ),
    dict(
        name="TERMINAL_BLOCK", cls="Terminal", attrs=A_TERMINAL,
        desc="Three-way terminal block: rail with three terminal circles",
        pins=PINS_TERMINAL_BLOCK,
        geom=rect(-5.0, -1.5, 5.0, 2.5) + [C(-3.0, 0.5, 0.8), C(0.0, 0.5, 0.8), C(3.0, 0.5, 0.8)],
    ),
    dict(
        name="EARTH", cls="Earth", attrs=A_EARTH,
        desc="Earth / ground: three shrinking bars",
        pins=PINS_EARTH,
        geom=[L(0.0, 3.5, 0.0, 0.0), L(-3.0, 0.0, 3.0, 0.0),
              L(-2.0, -1.2, 2.0, -1.2), L(-1.0, -2.4, 1.0, -2.4)],
    ),
    dict(
        name="JUNCTION", cls="Junction", attrs=A_EARTH,
        desc="Junction dot (drawn where three or more wires meet)",
        pins=PINS_NONE,
        geom=[C(0.0, 0.0, 1.0)],
    ),
    dict(
        name="PLC-DI", cls="PLC Digital Input", attrs=A_PLC,
        desc="PLC digital input module: body with two input arrows and pins",
        pins=PINS_PLC_DI,
        geom=rect(-4.0, -3.0, 4.0, 3.0)
        + [P([(-5.0, 1.0), (-4.0, 1.0)]), P([(-5.0, -1.0), (-4.0, -1.0)]),
           T("DI", -1.2, 1.0, 1.2)],
    ),
    dict(
        name="PLC-DO", cls="PLC Digital Output", attrs=A_PLC,
        desc="PLC digital output module: body with two output arrows and pins",
        pins=PINS_PLC_DO,
        geom=rect(-4.0, -3.0, 4.0, 3.0)
        + [P([(4.0, 1.0), (5.0, 1.0)]), P([(4.0, -1.0), (5.0, -1.0)]),
           T("DO", -1.2, 1.0, 1.2)],
    ),
    dict(
        name="POWER_SUPPLY", cls="Power Supply", attrs=A_DEVICE,
        desc="Power supply: box with a DC output mark",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), L(3.0, 2.5, 5.0, 2.5)]
        + rect(-3.0, -2.5, 3.0, 2.5)
        + [L(-2.0, 0.0, -1.0, 0.0), L(-1.5, 0.5, -1.5, -0.5)],
    ),
    dict(
        name="TRANSFORMER", cls="Transformer", attrs=A_DEVICE,
        desc="Transformer: two windings with a core line",
        geom=[L(-5.0, 0.0, -3.5, 0.0), L(3.5, 0.0, 5.0, 0.0),
              C(-1.7, 0.0, 1.7), C(1.7, 0.0, 1.7), L(0.0, -2.4, 0.0, 2.4)],
    ),
    dict(
        name="VFD", cls="Drive", attrs=A_DEVICE,
        desc="Variable frequency drive: box with VFD",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0)] + rect(-3.0, -2.5, 3.0, 2.5)
        + [T("VFD", -2.4, -0.7, 1.2)],
    ),
    dict(
        name="CABLE_ENTRY", cls="Cable", attrs=A_DEVICE,
        desc="Cable entry: gland circle with a cable stub",
        pins=PINS_CABLE_ENTRY,
        geom=[L(-5.0, 0.0, -2.0, 0.0), C(0.0, 0.0, 2.0), L(1.4, 1.4, 3.5, 3.5),
              L(0.0, -2.0, 0.0, -3.5)],
    ),
    dict(
        name="WIRE_LABEL", cls="Wire Label", attrs=A_LABEL,
        desc="Wire number label frame drawn on the conductor",
        geom=rect(-4.0, -1.5, 4.0, 1.5) + [L(-5.0, 0.0, -4.0, 0.0), L(4.0, 0.0, 5.0, 0.0)],
    ),
    dict(
        name="SOLENOID_VALVE", cls="Solenoid", attrs=A_2TERM,
        desc="Solenoid valve: coil rectangle with the valve triangle",
        geom=leads(inner=2.5) + rect(-2.5, -2.0, 2.5, 2.0)
        + [P([(2.5, 2.0), (5.0, 3.0), (5.0, 1.0), (2.5, 2.0)], closed=True)],
    ),
    dict(
        name="PROXIMITY_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Inductive proximity sensor: box with a target arrow",
        geom=leads(inner=1.0) + rect(-1.0, -2.0, 3.0, 2.0)
        + [L(1.5, 2.0, 1.5, 3.5), L(0.0, -2.0, 0.0, -3.0)],
    ),
    dict(
        name="NETWORK_SWITCH", cls="Network Switch", attrs=A_DEVICE,
        desc="Managed Ethernet switch: body with four port leads",
        pins=PINS_NETWORK_SWITCH,
        geom=rect(-3.0, -2.5, 3.0, 2.5)
        + [L(-5.0, 1.0, -3.0, 1.0), L(-5.0, -1.0, -3.0, -1.0),
           L(3.0, 1.0, 5.0, 1.0), L(3.0, -1.0, 5.0, -1.0),
           T("SW", -1.3, -0.7, 1.2)],
    ),
    dict(
        name="HMI", cls="HMI", attrs=A_DEVICE,
        desc="Operator panel: screen body with power and network leads",
        pins=PINS_HMI,
        geom=rect(-3.0, -4.0, 3.0, 4.0) + rect(-2.2, -2.8, 2.2, 2.2)
        + [L(-5.0, 1.5, -3.0, 1.5), L(-5.0, -1.5, -3.0, -1.5),
           L(3.0, 0.0, 5.0, 0.0), T("HMI", -1.8, -1.4, 1.2)],
    ),
    dict(
        name="IO_MODULE", cls="IO Module", attrs=A_DEVICE,
        desc="Field I/O module: eight terminals on a slim body",
        pins=PINS_IO_MODULE,
        geom=rect(-3.5, -4.0, 3.5, 4.0)
        + [L(-5.0, 3.0, -3.5, 3.0), L(-5.0, 1.0, -3.5, 1.0),
           L(-5.0, -1.0, -3.5, -1.0), L(-5.0, -3.0, -3.5, -3.0),
           L(3.5, 3.0, 5.0, 3.0), L(3.5, 1.0, 5.0, 1.0),
           L(3.5, -1.0, 5.0, -1.0), L(3.5, -3.0, 5.0, -3.0),
           T("I/O", -1.5, -0.9, 1.2)],
    ),
    # ---- control gear: relays, timers, switching ------------------------------
    dict(
        name="RELAY_CONTACT_NO", cls="Relay", attrs=A_2TERM,
        desc="Relay contact, normally-open, with the relay link below the blade",
        geom=no_contact() + [L(0.0, 0.0, 0.0, -1.0), L(0.0, -1.5, 0.0, -2.4),
                             L(-0.9, -2.4, 0.9, -2.4)],
    ),
    dict(
        name="RELAY_CONTACT_NC", cls="Relay", attrs=A_2TERM,
        desc="Relay contact, normally-closed, with the relay link below the blade",
        geom=nc_contact() + [L(0.0, 0.0, 0.0, -1.0), L(0.0, -1.5, 0.0, -2.4),
                             L(-0.9, -2.4, 0.9, -2.4)],
    ),
    dict(
        name="TIMER_COIL", cls="Timer", attrs=A_2TERM,
        desc="Timer coil: coil rectangle with the clock face inside",
        geom=leads(inner=2.5) + rect(-2.5, -2.5, 2.5, 2.5)
        + [C(0.0, 0.0, 1.3), L(0.0, 0.0, 0.0, 0.9), L(0.0, 0.0, 0.7, 0.3)],
    ),
    dict(
        name="TIMER_CONTACT_NO", cls="Timer", attrs=A_2TERM,
        desc="Timer contact, normally-open, clock above the blade",
        geom=no_contact() + [C(0.0, 3.4, 1.0), L(0.0, 3.4, 0.0, 4.0), L(0.0, 3.4, 0.6, 3.4)],
    ),
    dict(
        name="TIMER_CONTACT_NC", cls="Timer", attrs=A_2TERM,
        desc="Timer contact, normally-closed, clock above the blade",
        geom=nc_contact() + [C(0.0, 3.4, 1.0), L(0.0, 3.4, 0.0, 4.0), L(0.0, 3.4, 0.6, 3.4)],
    ),
    dict(
        name="SWITCH_DISCONNECT", cls="Switch", attrs=A_2TERM,
        desc="Disconnect switch: knife blade between two fixed contacts",
        geom=leads(inner=2.5) + [L(-2.5, -1.6, -2.5, 1.6), L(2.5, -1.6, 2.5, 1.6),
                                 L(-2.5, 0.0, 2.2, 2.2)],
    ),
    dict(
        name="EMERGENCY_STOP", cls="Button", attrs=A_2TERM,
        desc="Emergency stop: mushroom head over a normally-closed contact",
        geom=nc_contact() + [L(0.0, 0.0, 0.0, 2.6), C(0.0, 3.6, 1.1), L(-1.5, 4.7, 1.5, 4.7)],
    ),
    dict(
        name="FOOT_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Foot switch: pedal plate over a normally-closed contact",
        geom=nc_contact() + [L(0.0, 0.0, 0.0, 2.2), L(-1.8, 2.2, 1.8, 2.2), L(-1.8, 2.9, 1.8, 2.9)],
    ),
    dict(
        name="LIMIT_SWITCH_ROLLER", cls="Limit Switch", attrs=A_2TERM,
        desc="Limit switch with a roller lever",
        geom=no_contact() + [L(0.0, 0.0, 0.0, 2.6), L(0.0, 2.6, 2.0, 3.4), C(2.4, 3.6, 0.5)],
    ),
    dict(
        name="LEVEL_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Level switch: float on a lever arm",
        geom=leads(inner=2.5) + [L(-2.5, 0.0, -2.5, 2.4), L(-2.5, 2.4, 1.4, 2.4), C(2.2, 2.4, 0.8)],
    ),
    dict(
        name="PRESSURE_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Pressure switch: diaphragm circle with a trip bar",
        geom=leads(inner=2.5) + [C(0.0, 0.0, 2.2), T("P", -0.5, -0.8, 1.4),
                                 L(0.0, 2.2, 0.0, 3.4), L(-1.2, 3.4, 1.2, 3.4)],
    ),
    dict(
        name="FLOW_SWITCH", cls="Switch", attrs=A_2TERM,
        desc="Flow switch: paddle in the pipe",
        geom=leads(inner=2.5) + rect(-1.4, -1.4, 1.4, 1.4)
        + [L(1.4, 0.0, 2.8, 1.2), L(2.8, 1.2, 3.6, 1.2)],
    ),
    # ---- protection ------------------------------------------------------------
    dict(
        name="MCCB", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Moulded-case circuit breaker: wide body with a trip cross",
        geom=leads(inner=3.0) + rect(-3.0, -2.0, 3.0, 2.0)
        + [L(-3.0, -2.0, 3.0, 2.0), L(0.0, 4.0, 1.0, 5.0), L(1.0, 4.0, 0.0, 5.0)],
    ),
    dict(
        name="RCD", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Residual-current device: body with a test loop",
        geom=leads(inner=2.5) + rect(-2.5, -2.0, 2.5, 2.0)
        + [C(0.0, 0.0, 0.9), L(-1.4, -2.0, -1.4, -3.2), L(1.4, -2.0, 1.4, -3.2),
           C(0.0, -3.6, 0.5)],
    ),
    dict(
        name="SURGE_PROTECTOR", cls="Surge Protection", attrs=A_2TERM,
        desc="Surge protective device: varistor block to earth",
        geom=leads(inner=2.0) + rect(-2.0, -2.5, 2.0, 2.5)
        + [P([(-1.0, 1.4), (1.0, 1.4), (-1.0, -1.4), (1.0, -1.4)], closed=True),
           L(-1.5, -3.2, 1.5, -3.2), L(-1.0, -3.9, 1.0, -3.9), L(-0.5, -4.6, 0.5, -4.6)],
    ),
    dict(
        name="FUSE_3PHASE", cls="Fuse", attrs=A_DEVICE,
        desc="Three-pole fuse: three elements on one carrier",
        geom=[L(-5.0, 1.5, -3.0, 1.5), L(-5.0, 0.0, -3.0, 0.0), L(-5.0, -1.5, -3.0, -1.5),
              L(3.0, 1.5, 5.0, 1.5), L(3.0, 0.0, 5.0, 0.0), L(3.0, -1.5, 5.0, -1.5)]
        + rect(-3.0, -2.5, 3.0, 2.5)
        + [L(-1.0, 2.5, -1.0, -2.5), L(1.0, 2.5, 1.0, -2.5)],
    ),
    dict(
        name="THERMAL_MAGNETIC", cls="Circuit Breaker", attrs=A_2TERM,
        desc="Thermal-magnetic trip element",
        geom=leads(inner=2.5) + rect(-2.5, -2.0, 2.5, 2.0)
        + [L(-1.5, 1.4, 0.0, -1.2), L(0.0, -1.2, 1.5, 1.4)],
    ),
    # ---- machines and supplies --------------------------------------------------
    dict(
        name="TRANSFORMER_3PHASE", cls="Transformer", attrs=A_DEVICE,
        desc="Three-phase transformer: two windings and a core",
        geom=[L(-5.0, 0.0, -3.2, 0.0), L(3.2, 0.0, 5.0, 0.0), C(-1.6, 0.0, 1.6),
              C(1.6, 0.0, 1.6), L(0.0, -2.4, 0.0, 2.4),
              L(-1.6, 2.4, -1.6, 3.4), L(0.0, 2.4, 0.0, 3.4), L(1.6, 2.4, 1.6, 3.4)],
    ),
    dict(
        name="GENERATOR", cls="Generator", attrs=A_DEVICE,
        desc="Generator: circle with G",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              T("G", -1.0, -0.9, 1.8)],
    ),
    dict(
        name="UPS", cls="Power Supply", attrs=A_DEVICE,
        desc="Uninterruptible supply: box with the battery symbol",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0)] + rect(-3.0, -2.5, 3.0, 2.5)
        + [T("UPS", -2.2, 0.6, 1.2), L(-2.0, -1.4, -0.4, -1.4), L(-2.0, -2.0, -0.4, -2.0)],
    ),
    dict(
        name="ENERGY_METER", cls="Meter", attrs=A_DEVICE,
        desc="Energy meter: circle with kWh",
        geom=[L(-5.0, 0.0, -2.8, 0.0), L(2.8, 0.0, 5.0, 0.0), C(0.0, 0.0, 2.8),
              T("kWh", -1.9, -0.7, 1.1)],
    ),
    dict(
        name="SERVO_MOTOR", cls="Motor", attrs=A_DEVICE,
        desc="Servo motor: motor with an encoder box on top",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              T("M", -1.0, -0.9, 1.8)] + rect(-1.2, 3.0, 1.2, 4.6),
    ),
    dict(
        name="GEAR_MOTOR", cls="Motor", attrs=A_DEVICE,
        desc="Gear motor: motor circle with a gear ring",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              T("M", -1.0, -1.2, 1.8),
              L(0.0, 3.0, 0.0, 4.2), L(1.5, 2.6, 2.1, 3.6), L(2.6, 1.5, 3.6, 2.1),
              L(3.0, 0.0, 4.2, 0.0), L(2.6, -1.5, 3.6, -2.1), L(1.5, -2.6, 2.1, -3.6),
              L(0.0, -3.0, 0.0, -4.2), L(-1.5, -2.6, -2.1, -3.6), L(-2.6, -1.5, -3.6, -2.1),
              L(-3.0, 0.0, -4.2, 0.0), L(-2.6, 1.5, -3.6, 2.1), L(-1.5, 2.6, -2.1, 3.6)],
    ),
    dict(
        name="SOFT_STARTER", cls="Drive", attrs=A_DEVICE,
        desc="Soft starter: box with SS",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0)] + rect(-3.0, -2.5, 3.0, 2.5)
        + [T("SS", -1.2, -0.7, 1.2)],
    ),
    # ---- sensors ---------------------------------------------------------------
    dict(
        name="PHOTO_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Photoelectric sensor: body with an incoming beam",
        geom=leads(inner=1.0) + rect(-1.0, -2.0, 3.0, 2.0)
        + [L(-1.0, 0.4, -3.6, 0.4), L(-3.6, 0.4, -2.8, 0.9), L(-3.6, 0.4, -2.8, -0.1)],
    ),
    dict(
        name="CAPACITIVE_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Capacitive sensor: body with two capacitor plates",
        geom=leads(inner=1.0) + rect(-1.0, -2.0, 3.0, 2.0)
        + [L(0.4, 0.9, 0.4, -0.9), L(1.2, 0.9, 1.2, -0.9)],
    ),
    dict(
        name="ULTRASONIC_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Ultrasonic sensor: body with two sound arcs",
        geom=leads(inner=1.0) + rect(-1.0, -2.0, 3.0, 2.0)
        + [A(2.4, 0.0, 1.0, -55.0, 55.0), A(2.4, 0.0, 1.8, -55.0, 55.0)],
    ),
    dict(
        name="TEMPERATURE_SENSOR", cls="Sensor", attrs=A_2TERM,
        desc="Temperature probe: bulb and stem",
        geom=leads(inner=2.0) + rect(-1.4, -1.6, 1.4, 1.6)
        + [L(0.0, 1.6, 0.0, 3.4), C(0.0, 3.9, 0.6)],
    ),
    dict(
        name="ENCODER", cls="Sensor", attrs=A_2TERM,
        desc="Rotary encoder: body with a shaft disc",
        geom=leads(inner=2.0) + rect(-2.5, -2.0, 2.5, 2.0)
        + [L(0.0, 2.0, 0.0, 3.0), C(0.0, 3.8, 0.8), T("E", -0.6, -0.7, 1.2)],
    ),
    # ---- PLC family ------------------------------------------------------------
    dict(
        name="PLC-AI", cls="PLC Analog Input", attrs=A_PLC,
        desc="PLC analog input module",
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02")],
        geom=rect(-4.0, -3.0, 4.0, 3.0)
        + [L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0), T("AI", -1.2, 1.0, 1.2)],
    ),
    dict(
        name="PLC-AO", cls="PLC Analog Output", attrs=A_PLC,
        desc="PLC analog output module",
        pins=[(5.0, 1.0, "X1TERM01"), (5.0, -1.0, "X1TERM02")],
        geom=rect(-4.0, -3.0, 4.0, 3.0)
        + [L(4.0, 1.0, 5.0, 1.0), L(4.0, -1.0, 5.0, -1.0), T("AO", -1.2, 1.0, 1.2)],
    ),
    dict(
        name="PLC-CPU", cls="PLC", attrs=A_PLC,
        desc="PLC processor with a programming port",
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02"), (5.0, 0.0, "X1TERM03")],
        geom=rect(-4.0, -3.0, 4.0, 3.0)
        + [L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0), L(4.0, 0.0, 5.0, 0.0),
           T("CPU", -1.8, -0.6, 1.2)],
    ),
    dict(
        name="PLC-PSU", cls="PLC", attrs=A_PLC,
        desc="PLC power supply module",
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02")],
        geom=rect(-4.0, -3.0, 4.0, 3.0)
        + [L(-5.0, 1.0, -4.0, 1.0), L(-5.0, -1.0, -4.0, -1.0), T("24V", -1.7, 1.0, 1.1)],
    ),
    dict(
        name="PLC-COUPLER", cls="PLC", attrs=A_PLC,
        desc="PLC bus coupler with two field leads",
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02"),
              (5.0, 1.0, "X1TERM01"), (5.0, -1.0, "X1TERM02")],
        geom=rect(-3.5, -2.5, 3.5, 2.5)
        + [L(-5.0, 1.0, -3.5, 1.0), L(-5.0, -1.0, -3.5, -1.0),
           L(3.5, 1.0, 5.0, 1.0), L(3.5, -1.0, 5.0, -1.0), T("CPL", -1.4, -0.7, 1.2)],
    ),
    # ---- terminals and connectors ----------------------------------------------
    dict(
        name="TERMINAL_GROUND", cls="Terminal", attrs=A_TERMINAL,
        desc="Earth terminal: terminal circle over the earth bars",
        pins=PINS_EARTH,
        geom=[L(0.0, 3.5, 0.0, 1.2), C(0.0, 0.0, 1.2), L(-2.5, -1.4, 2.5, -1.4),
              L(-1.7, -2.3, 1.7, -2.3), L(-0.9, -3.2, 0.9, -3.2)],
    ),
    dict(
        name="TERMINAL_END_STOP", cls="Terminal", attrs=A_LABEL,
        desc="End stop / end bracket for a DIN rail",
        pins=PINS_NONE,
        geom=rect(-1.0, -2.6, 1.0, 2.6)
        + [L(-1.0, -2.6, -2.6, -3.4), L(1.0, -2.6, 2.6, -3.4), L(-2.6, 0.0, 2.6, 0.0)],
    ),
    dict(
        name="TERMINAL_SEPARATOR", cls="Terminal", attrs=A_LABEL,
        desc="Group separator plate",
        pins=PINS_NONE,
        geom=[L(-1.2, -2.6, -1.2, 2.6), L(1.2, -2.6, 1.2, 2.6)],
    ),
    dict(
        name="CONNECTOR_2P", cls="Connector", attrs=A_DEVICE,
        desc="Two-pole plug connector",
        pins=[(-5.0, 1.0, "X4TERM01"), (-5.0, -1.0, "X4TERM02"),
              (5.0, 1.0, "X1TERM01"), (5.0, -1.0, "X1TERM02")],
        geom=rect(-3.0, -2.0, 3.0, 2.0)
        + [L(-5.0, 1.0, -3.0, 1.0), L(-5.0, -1.0, -3.0, -1.0),
           L(3.0, 1.0, 5.0, 1.0), L(3.0, -1.0, 5.0, -1.0), T("X", -0.6, -0.7, 1.4)],
    ),
    dict(
        name="CONNECTOR_4P", cls="Connector", attrs=A_DEVICE,
        desc="Four-pole plug connector",
        pins=[(-5.0, 2.0, "X4TERM01"), (-5.0, 0.0, "X4TERM02"), (-5.0, -2.0, "X4TERM03"),
              (5.0, 1.0, "X1TERM01"), (5.0, -1.0, "X1TERM02")],
        geom=rect(-3.5, -3.0, 3.5, 3.0)
        + [L(-5.0, 2.0, -3.5, 2.0), L(-5.0, 0.0, -3.5, 0.0), L(-5.0, -2.0, -3.5, -2.0),
           L(3.5, 1.0, 5.0, 1.0), L(3.5, -1.0, 5.0, -1.0), T("X4", -1.4, -0.7, 1.2)],
    ),
    dict(
        name="TEST_POINT", cls="Test Point", attrs=A_TERMINAL,
        desc="Test point: probe circle on the conductor",
        pins=[(-5.0, 0.0, "X4TERM01")],
        geom=[L(-5.0, 0.0, -1.6, 0.0), C(0.0, 0.0, 1.6), T("TP", -0.9, -0.5, 1.0)],
    ),
    # ---- field devices and panel hardware --------------------------------------
    dict(
        name="HEATER", cls="Heater", attrs=A_2TERM,
        desc="Heater element: rectangle with a zig-zag",
        geom=leads(inner=2.5) + rect(-2.5, -2.0, 2.5, 2.0)
        + [P([(-1.8, 0.0), (-0.9, 1.3), (0.0, 0.0), (0.9, 1.3), (1.8, 0.0)])],
    ),
    dict(
        name="FAN", cls="Fan", attrs=A_DEVICE,
        desc="Cooling fan: circle with three blades",
        geom=[L(-5.0, 0.0, -3.0, 0.0), L(3.0, 0.0, 5.0, 0.0), C(0.0, 0.0, 3.0),
              L(0.0, 0.0, 0.0, 2.4), L(0.0, 0.0, 2.1, -1.2), L(0.0, 0.0, -2.1, -1.2)],
    ),
    dict(
        name="BELL", cls="Bell", attrs=A_DEVICE,
        desc="Audible bell",
        geom=[L(-5.0, 0.0, -2.6, 0.0), L(2.6, 0.0, 5.0, 0.0),
              A(0.0, 0.0, 2.6, 0.0, 180.0), L(-2.6, 0.0, 2.6, 0.0)],
    ),
    dict(
        name="BUZZER", cls="Buzzer", attrs=A_DEVICE,
        desc="Buzzer: circle with sound ticks",
        geom=[L(-5.0, 0.0, -2.2, 0.0), L(2.2, 0.0, 5.0, 0.0), C(0.0, 0.0, 2.2),
              L(2.6, 1.4, 3.4, 2.0), L(2.6, -1.4, 3.4, -2.0)],
    ),
    dict(
        name="VALVE_2WAY", cls="Valve", attrs=A_2TERM,
        desc="Two-way valve: bow-tie body",
        geom=leads(inner=2.5)
        + [P([(-2.5, 0.0), (0.0, 2.0), (2.5, 0.0), (0.0, -2.0)], closed=True)],
    ),
    dict(
        name="VALVE_3WAY", cls="Valve", attrs=A_DEVICE,
        desc="Three-way valve: two bow-ties and a stem",
        geom=[L(-5.0, 0.0, -3.2, 0.0), L(3.2, 0.0, 5.0, 0.0), L(0.0, 2.4, 0.0, 4.0),
              L(-1.2, 4.0, 1.2, 4.0),
              P([(-3.2, 0.0), (-0.4, 1.8), (-0.4, -1.8)], closed=True),
              P([(3.2, 0.0), (0.4, 1.8), (0.4, -1.8)], closed=True)],
    ),
    dict(
        name="DIN_RAIL", cls="Rail", attrs=A_LABEL,
        desc="DIN rail section",
        pins=PINS_NONE,
        geom=[L(-5.0, -1.0, -4.0, -1.0), L(-4.0, -1.0, -4.0, 0.6), L(-4.0, 0.6, -3.6, 1.0),
              L(-3.6, 1.0, 3.6, 1.0), L(3.6, 1.0, 4.0, 0.6), L(4.0, 0.6, 4.0, -1.0),
              L(4.0, -1.0, 5.0, -1.0)],
    ),
    dict(
        name="WIRE_DUCT", cls="Wire Duct", attrs=A_LABEL,
        desc="Slotted wire duct",
        pins=PINS_NONE,
        geom=rect(-4.0, -2.0, 4.0, 2.0)
        + [L(-4.0, 0.6, 4.0, 0.6), L(-4.0, -0.6, 4.0, -0.6),
           L(-2.4, 0.6, -2.4, 2.0), L(-0.8, 0.6, -0.8, 2.0),
           L(0.8, 0.6, 0.8, 2.0), L(2.4, 0.6, 2.4, 2.0),
           L(-2.4, -2.0, -2.4, -0.6), L(-0.8, -2.0, -0.8, -0.6),
           L(0.8, -2.0, 0.8, -0.6), L(2.4, -2.0, 2.4, -0.6)],
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
        elif kind == "text":
            _, value, x, y, height, rotation = op
            scaled.append(("text", value, x * factor, y * factor, height * factor, rotation))
        else:  # pragma: no cover
            raise ValueError(f"unknown geometry op {kind!r}")
    return scaled


def build_symbol(entry):
    """Return (dxf text, manifest row tuple) for one catalogue entry."""
    name = entry["name"]
    geom = scale_ops(entry["geom"], UNIT_SCALE)
    attrs = entry["attrs"]
    pins = entry.get("pins", PINS_2TERM)

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
    return doc, (name, entry["cls"], "|".join(documented), str(LIBRARY_SCALE))


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
    files = {}
    rows = []
    for entry in SYMBOLS:
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
    args = parser.parse_args(argv)

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
