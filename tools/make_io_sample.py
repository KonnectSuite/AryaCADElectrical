#!/usr/bin/env python3
"""Write the sample I/O list the ELECIOIMPORT test imports.

Run with the bundled interpreter:

    python tools/make_io_sample.py

The workbook is what an electrical designer would hand over: a title block row,
a header row, then one row per I/O point grouped in cards. It deliberately
contains the awkward cases the importer has to survive:

* a title row and a blank row *before* the header, so the header has to be found,
* mixed spellings of the I/O type column values (DI / Input / di / DO / Output / AI),
* one completely blank row in the middle,
* one junk footer row that is not an I/O point ("Total: 18 I/O points"),
* a second worksheet ("Notes") that must be ignored: the importer reads the first.

Addresses use the three families the importer knows: I (digital in), Q (digital
out) and %AI (analog in, Siemens style with the percent sign). Every card is a
byte: I0.0..I0.7, Q0.0..Q0.5, %AI0.0..%AI0.3.
"""

from pathlib import Path

from openpyxl import Workbook
from openpyxl.styles import Alignment, Font, PatternFill
from openpyxl.utils import get_column_letter

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "tests" / "ecad" / "io_list_sample.xlsx"

HEADERS = ["I/O Type", "Address", "Description", "Terminal", "Module"]

# (type, address, description, terminal, module)
POINTS = [
    # Card 1 - digital inputs, I0.0 .. I0.7
    ("DI", "I0.0", "Start pushbutton", "X1:1", "Slot 1"),
    ("DI", "I0.1", "Stop pushbutton (NC)", "X1:2", "Slot 1"),
    ("Input", "I0.2", "Guard door closed", "X1:3", "Slot 1"),
    ("di", "I0.3", "Motor overload trip", "X1:4", "Slot 1"),
    ("DI", "I0.4", "Tank high level", "X1:5", "Slot 1"),
    ("DI", "I0.5", "Pump running feedback", "X1:6", "Slot 1"),
    ("DI", "I0.6", "Emergency stop auxiliary", "X1:7", "Slot 1"),
    ("DI", "I0.7", "Mode selector auto", "X1:8", "Slot 1"),
    # Card 2 - digital outputs, Q0.0 .. Q0.5
    ("DO", "Q0.0", "Motor starter coil", "X2:1", "Slot 2"),
    ("DO", "Q0.1", "Stack light green", "X2:2", "Slot 2"),
    ("Output", "Q0.2", "Stack light amber", "X2:3", "Slot 2"),
    ("DO", "Q0.3", "Stack light red", "X2:4", "Slot 2"),
    ("DO", "Q0.4", "Solenoid valve fill", "X2:5", "Slot 2"),
    ("DO", "Q0.5", "Solenoid valve drain", "X2:6", "Slot 2"),
    # Card 3 - analog inputs, %AI0.0 .. %AI0.3
    ("AI", "%AI0.0", "Tank level 4-20 mA", "X3:1", "Slot 3"),
    ("AI", "%AI0.1", "Line pressure 4-20 mA", "X3:2", "Slot 3"),
    ("AI", "%AI0.2", "Motor current 0-10 V", "X3:3", "Slot 3"),
    ("AI", "%AI0.3", "Ambient temperature", "X3:4", "Slot 3"),
]

# The footer row is junk: it has no address, so the importer must skip it and say why.
FOOTER = ["", "", "Total: 18 I/O points", "", "Issued for construction"]


def build() -> Workbook:
    book = Workbook()
    sheet = book.active
    # A sheet title cannot contain "/", so the tab is "IO List" while the cells say I/O.
    sheet.title = "IO List"

    # Title row, then a blank row, so the header is on row 4 and has to be found.
    sheet["A1"] = "I/O List - Line 1 Packing"
    sheet["A1"].font = Font(bold=True, size=14)
    sheet["A2"] = "One row per I/O point; terminal strip and module are the electrician's columns."
    sheet["A2"].font = Font(italic=True)

    header_row = 4
    for column, name in enumerate(HEADERS, start=1):
        cell = sheet.cell(row=header_row, column=column, value=name)
        cell.font = Font(bold=True)
        cell.fill = PatternFill("solid", fgColor="DDEBF7")
        cell.alignment = Alignment(horizontal="center")

    row = header_row + 1
    for index, point in enumerate(POINTS):
        # A completely blank row between the second and third card: the importer must
        # count it as a blank row, not as a malformed I/O point.
        if index == 14:
            row += 1
        for column, value in enumerate(point, start=1):
            sheet.cell(row=row, column=column, value=value)
        row += 1

    for column, value in enumerate(FOOTER, start=1):
        sheet.cell(row=row, column=column, value=value)
    sheet.cell(row=row, column=3).font = Font(italic=True)

    widths = [10, 12, 34, 12, 12]
    for column, width in enumerate(widths, start=1):
        sheet.column_dimensions[get_column_letter(column)].width = width
    sheet.freeze_panes = "A5"

    # A second worksheet that must be ignored: only the first sheet is an I/O list.
    notes = book.create_sheet("Notes")
    notes["A1"] = "Notes"
    notes["A1"].font = Font(bold=True, size=14)
    notes["A2"] = "This sheet is documentation. The importer reads the first worksheet only."
    notes["A3"] = "Rev A - 2026-11-06 - issued for construction"
    return book


def main() -> int:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    book = build()
    book.save(OUT)
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")
    print(f"  {len(POINTS)} I/O points in 3 cards, 1 blank row, 1 junk footer row, 2 worksheets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
