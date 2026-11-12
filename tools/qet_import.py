"""Convert QElectroTech elements into the shapes ``make_library.py`` emits.

QElectroTech (https://qelectrotech.org) publishes a large, maintained library of
real electrical symbols - including the EN 60617 (IEC) collections - as XML
``.elmt`` files.  This tool reads a chosen element, keeps its drawable geometry
and its terminals, normalises it onto the ten-unit design canvas this project
authors symbols on, and writes ``tools/qet_symbols.json``.  ``make_library.py``
then uses those shapes in place of the hand-drawn ones, so the blocks in
``library/starter`` carry the published geometry rather than an approximation.

Only the shapes are taken from QElectroTech; block names, classes, attribute
sets and pin naming stay this project's, so the manifest, the parts catalogue
and the reports keep working unchanged.

Usage::

    py -3.13 tools/qet_import.py --source <elements checkout>/10_electric

The checkout is the ``qelectrotech-elements`` repository (GPL-2.0-or-later);
its element licence note is at http://qelectrotech.org/wiki/doc/elements_license.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import xml.etree.ElementTree as ET

# Our block name -> the QElectroTech element that draws it.  Paths are relative
# to the ``10_electric`` folder of the elements repository.
MAPPING = {
    "CONTACTOR_COIL": "10_allpole/310_relays_contactors_contacts/01_coils/bobine3.elmt",
    "RELAY_COIL": "10_allpole/310_relays_contactors_contacts/01_coils/comm_courant_alter.elmt",
    "TIMER_COIL": "10_allpole/310_relays_contactors_contacts/01_coils/bobine_tempo_travail.elmt",
    "CONTACT_NO": "10_allpole/310_relays_contactors_contacts/02_contacts_cross_referencing/01_auxiliary_contacts/con_simple.elmt",
    "CONTACT_NC": "10_allpole/310_relays_contactors_contacts/02_contacts_cross_referencing/01_auxiliary_contacts/con_simple_nf.elmt",
    "RELAY_CONTACT_NO": "10_allpole/310_relays_contactors_contacts/03_contacts/act_electromagnetique_no.elmt",
    "RELAY_CONTACT_NC": "10_allpole/310_relays_contactors_contacts/03_contacts/act_electromagnetique_nf.elmt",
    "TIMER_CONTACT_NO": "10_allpole/310_relays_contactors_contacts/02_contacts_cross_referencing/11_delayed_contacts/con_simple_tmp_t.elmt",
    "TIMER_CONTACT_NC": "10_allpole/310_relays_contactors_contacts/02_contacts_cross_referencing/11_delayed_contacts/con_simple_nf_tmp_t.elmt",
    "PUSHBUTTON_NO": "10_allpole/380_signaling_operating/20_push_buttons/poussoir.elmt",
    "PUSHBUTTON_NC": "10_allpole/380_signaling_operating/20_push_buttons/poussoir_nf.elmt",
    "EMERGENCY_STOP": "10_allpole/380_signaling_operating/20_push_buttons/arret-urgence_no.elmt",
    "SELECTOR_SWITCH": "10_allpole/380_signaling_operating/21_selector_switches/commut_2_position_fixe.elmt",
    "LIMIT_SWITCH": "91_en_60617/en_60617_07/en_60617_07_08/en_60617_07_08_01.elmt",
    "LIMIT_SWITCH_ROLLER": "91_en_60617/en_60617_07/en_60617_07_08/en_60617_07_08_03.elmt",
    "SWITCH_DISCONNECT": "10_allpole/200_fuses_protective_gears/20_disconnecting_switches/sectionneur_general.elmt",
    "FOOT_SWITCH": "10_allpole/380_signaling_operating/20_push_buttons/foot_no.elmt",
    "LEVEL_SWITCH": "10_allpole/390_sensors_instruments/12_sensors_level/niv_liquide_no.elmt",
    "PRESSURE_SWITCH": "10_allpole/390_sensors_instruments/13_sensors_pressure/pressure_switch_no.elmt",
    "FLOW_SWITCH": "10_allpole/390_sensors_instruments/11_sensors_flow/debit_fluide_no.elmt",
    "FUSE": "10_allpole/200_fuses_protective_gears/10_fuses/pojistka1p.elmt",
    "FUSE_3PHASE": "10_allpole/200_fuses_protective_gears/10_fuses/pojistka3p.elmt",
    "MCB": "10_allpole/200_fuses_protective_gears/11_circuit_breakers/disjonct-m_1f.elmt",
    "MCCB": "10_allpole/200_fuses_protective_gears/12_magneto_thermal_circuit_breakers/fa4202_disjoncteur_moteur_3p.elmt",
    "THERMAL_MAGNETIC": "10_allpole/200_fuses_protective_gears/12_magneto_thermal_circuit_breakers/dis_mag_term_2f-1.elmt",
    "RCD": "10_allpole/200_fuses_protective_gears/50_residual_current_circuit_breaker/ddr2.elmt",
    "THERMAL_OVERLOAD": "10_allpole/200_fuses_protective_gears/30_thermal_relays/relais_mono.elmt",
    "SURGE_PROTECTOR": "10_allpole/200_fuses_protective_gears/90_overvoltage_protections/parafoudre.elmt",
    "MOTOR": "10_allpole/391_consumers_actuators/10_engines/motor2.elmt",
    "MOTOR_3PHASE": "10_allpole/500_home_installation/20_home_appliances/moteur_tri.elmt",
    "GENERATOR": "10_allpole/392_generators_sources/10_generators/gene_tri.elmt",
    "TRANSFORMER": "10_allpole/330_transformers_power_supplies/10_transformers/trafo1.elmt",
    "TRANSFORMER_3PHASE": "10_allpole/330_transformers_power_supplies/10_transformers/transformateur_triphase.elmt",
    "POWER_SUPPLY": "10_allpole/330_transformers_power_supplies/30_power_supplies/power_supply_1_phase_ac_dc.elmt",
    "VFD": "10_allpole/340_converters_inverters/10_converters/convertisseur_tri_mono.elmt",
    "SOFT_STARTER": "10_allpole/340_converters_inverters/10_converters/convertisseur_tri_ac_dc.elmt",
    "ENERGY_METER": "91_en_60617/en_60617_08/en_60617_08_04/en_60617_08_04_03.elmt",
    "HEATER": "10_allpole/391_consumers_actuators/50_heatings/resistance_chauffante.elmt",
    "FAN": "91_en_60617/en_60617_11/en_60617_11_16/en_60617_11_16_02.elmt",
    "BELL": "10_allpole/380_signaling_operating/12_acoustic_signaling/sonnerie.elmt",
    "BUZZER": "10_allpole/380_signaling_operating/12_acoustic_signaling/buzzer.elmt",
    "SOLENOID_VALVE": "10_allpole/391_consumers_actuators/20_valves/electrovanne.elmt",
    "PROXIMITY_SENSOR": "91_en_60617/en_60617_07/en_60617_07_19/en_60617_07_19_01.elmt",
    "PHOTO_SENSOR": "10_allpole/390_sensors_instruments/04_sensors_optical/cellule_photoelectrique_recepteur.elmt",
    "CAPACITIVE_SENSOR": "10_allpole/390_sensors_instruments/01_sensors_capacitive/capteur_capacitif_3.elmt",
    "ULTRASONIC_SENSOR": "10_allpole/390_sensors_instruments/16_sensors_ultrasonic/capteurs_ultrasons.elmt",
    "TEMPERATURE_SENSOR": "10_allpole/390_sensors_instruments/15_sensors_temperature/sonde_temperature.elmt",
    "ENCODER": "10_allpole/390_sensors_instruments/80_encoder/codeur.elmt",
    "TERMINAL": "10_allpole/130_terminals_terminal_strips/borne_2.elmt",
    "TERMINAL_BLOCK": "10_allpole/130_terminals_terminal_strips/bornedouble.elmt",
    "TERMINAL_GROUND": "10_allpole/130_terminals_terminal_strips/90_terminal_strips_diagram/90-10-0121.elmt",
    "EARTH": "91_en_60617/en_60617_02/en_60617_02_15/en_60617_02_15_01.elmt",
    "TEST_POINT": "91_en_60617/en_60617_02/en_60617_02_17/en_60617_02_17_05.elmt",
}

# The ten-unit canvas ``make_library.py`` authors on.
CANVAS = 10.0


def _floats(node, names):
    return [float(node.get(name, "0") or 0) for name in names]


def parse_element(path):
    """Return (ops, terminals) in the element's own coordinates."""
    root = ET.parse(path).getroot()
    description = root.find("description")
    ops = []
    terminals = []
    if description is None:
        return ops, terminals

    for node in description.iter():
        tag = node.tag
        if tag == "line":
            x1, y1, x2, y2 = _floats(node, ("x1", "y1", "x2", "y2"))
            ops.append(("line", x1, y1, x2, y2))
        elif tag == "rect":
            x, y, w, h = _floats(node, ("x", "y", "width", "height"))
            ops += [
                ("line", x, y, x + w, y),
                ("line", x + w, y, x + w, y + h),
                ("line", x + w, y + h, x, y + h),
                ("line", x, y + h, x, y),
            ]
        elif tag == "circle":
            cx, cy, diameter = _floats(node, ("x", "y", "diameter"))
            ops.append(("circle", cx, cy, diameter / 2.0))
        elif tag == "ellipse":
            x, y, w, h = _floats(node, ("x", "y", "width", "height"))
            if abs(w - h) < 1e-6:
                ops.append(("circle", x + w / 2.0, y + h / 2.0, w / 2.0))
            # A true ellipse has no DXF-circle equivalent here; skip it.
        elif tag == "arc":
            x, y, w, h = _floats(node, ("x", "y", "width", "height"))
            start, angle = _floats(node, ("start", "angle"))
            if abs(w - h) < 1e-6:
                ops.append(
                    ("arc", x + w / 2.0, y + h / 2.0, w / 2.0, start, start + angle)
                )
        elif tag in ("polygon", "polyline"):
            points = []
            index = 1
            while node.get(f"x{index}") is not None:
                points.append(
                    (float(node.get(f"x{index}", "0")), float(node.get(f"y{index}", "0")))
                )
                index += 1
            if len(points) >= 2:
                ops.append(("poly", points, tag == "polygon"))
        elif tag == "terminal":
            tx, ty = _floats(node, ("x", "y"))
            terminals.append((tx, ty))
    return ops, terminals


def transform(ops, terminals):
    """Rotate a vertical element, scale to the canvas and centre it."""
    xs, ys = [], []
    for op in ops:
        kind = op[0]
        if kind == "line":
            xs += [op[1], op[3]]
            ys += [op[2], op[4]]
        elif kind in ("circle", "arc"):
            xs += [op[1] - op[3], op[1] + op[3]]
            ys += [op[2] - op[3], op[2] + op[3]]
        elif kind == "poly":
            xs += [p[0] for p in op[1]]
            ys += [p[1] for p in op[1]]
    xs += [t[0] for t in terminals]
    ys += [t[1] for t in terminals]
    if not xs:
        return [], []

    # Decide the orientation from the terminal axis, not from the bounding box:
    # a generator circle hangs off to one side of its leads, so its box is wide
    # while its two pins still run vertically.
    pin_dx = (max(t[0] for t in terminals) - min(t[0] for t in terminals)) if terminals else 0.0
    pin_dy = (max(t[1] for t in terminals) - min(t[1] for t in terminals)) if terminals else 0.0
    rotate = pin_dy > pin_dx

    def place(x, y):
        if rotate:
            x, y = y, -x
        return x, y

    placed = []
    for op in ops:
        kind = op[0]
        if kind == "line":
            ax, ay = place(op[1], op[2])
            bx, by = place(op[3], op[4])
            placed.append(("line", ax, ay, bx, by))
        elif kind == "circle":
            cx, cy = place(op[1], op[2])
            placed.append(("circle", cx, cy, op[3]))
        elif kind == "arc":
            cx, cy = place(op[1], op[2])
            start, end = op[4], op[5]
            if rotate:
                start, end = start - 90.0, end - 90.0
            placed.append(("arc", cx, cy, op[3], start, end))
        elif kind == "poly":
            placed.append(("poly", [place(x, y) for x, y in op[1]], op[2]))
    pins = [place(x, y) for x, y in terminals]

    xs = []
    ys = []
    for op in placed:
        if op[0] == "line":
            xs += [op[1], op[3]]
            ys += [op[2], op[4]]
        elif op[0] in ("circle", "arc"):
            xs += [op[1] - op[3], op[1] + op[3]]
            ys += [op[2] - op[3], op[2] + op[3]]
        elif op[0] == "poly":
            xs += [p[0] for p in op[1]]
            ys += [p[1] for p in op[1]]
    xs += [p[0] for p in pins]
    ys += [p[1] for p in pins]

    # Fit the whole element - leads included - into the design canvas.  Scaling
    # by the terminal spacing instead would blow up symbols whose published
    # drawing puts long leads between the pins (a generator's circle then sat
    # three canvases off the wire).
    span = max(max(xs) - min(xs), max(ys) - min(ys), 1e-6)
    scale = CANVAS / span
    cx = (max(xs) + min(xs)) / 2.0
    cy = (max(ys) + min(ys)) / 2.0

    def fit(x, y):
        return round((x - cx) * scale, 4), round((y - cy) * scale, 4)

    final = []
    for op in placed:
        kind = op[0]
        if kind == "line":
            ax, ay = fit(op[1], op[2])
            bx, by = fit(op[3], op[4])
            final.append(("line", ax, ay, bx, by))
        elif kind == "circle":
            ox, oy = fit(op[1], op[2])
            final.append(("circle", ox, oy, round(op[3] * scale, 4)))
        elif kind == "arc":
            ox, oy = fit(op[1], op[2])
            final.append(("arc", ox, oy, round(op[3] * scale, 4), op[4], op[5]))
        elif kind == "poly":
            final.append(("poly", [fit(x, y) for x, y in op[1]], op[2]))

    # Pins: left-hand ones are X4 (AutoCAD Electrical's left terminal), right-hand
    # ones X1.  The number after TERM is the pin number and is unique across the
    # whole symbol - the left-hand column is numbered 1..n top to bottom and the
    # right-hand column continues from there - because the electrical layer reads
    # that number as the pin, and two pins sharing it read as one connection.
    fitted = [fit(x, y) for x, y in pins]
    left = sorted([p for p in fitted if p[0] < 0], key=lambda p: (-p[1], p[0]))
    right = sorted([p for p in fitted if p[0] >= 0], key=lambda p: (-p[1], p[0]))
    named = []
    for index, (x, y) in enumerate(left, start=1):
        named.append((x, y, f"X4TERM{index:02d}"))
    for index, (x, y) in enumerate(right, start=len(left) + 1):
        named.append((x, y, f"X1TERM{index:02d}"))
    return final, named


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True,
                        help="path to the 10_electric folder of qelectrotech-elements")
    parser.add_argument("--out", default=os.path.join("tools", "qet_symbols.json"),
                        help="where to write the shapes make_library.py reads")
    args = parser.parse_args(argv)

    shapes = {}
    missing = []
    for name, relative in sorted(MAPPING.items()):
        path = os.path.join(args.source, relative.replace("/", os.sep))
        if not os.path.exists(path):
            missing.append((name, relative))
            continue
        ops, terminals = parse_element(path)
        geom, pins = transform(ops, terminals)
        if not geom:
            missing.append((name, relative + " (no drawable shapes)"))
            continue
        shapes[name] = {
            "source": "qelectrotech-elements/" + relative,
            "geom": [list(op) for op in geom],
            "pins": [list(pin) for pin in pins],
        }

    with open(args.out, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(shapes, handle, indent=1, sort_keys=True)
        handle.write("\n")

    print(f"wrote {len(shapes)} symbol shape(s) to {args.out}")
    for name, reason in missing:
        print(f"  skipped {name}: {reason}")
    return 0 if not missing else 1


if __name__ == "__main__":
    raise SystemExit(main())
