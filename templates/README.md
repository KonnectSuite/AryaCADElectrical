# Starting templates — IEC and NEC

Fifteen ready-to-use sheet templates, one per standard size, each with a frame, zone
references, a standard title block and the numbering contract the electrical commands read.
Units are millimetres (1 drawing unit = 1 mm); the NEC sheets use the ASME Y14.1 inch sizes
converted to mm.

## Start a drawing from one

```
ELECNEW templates\iec\A3_landscape.dxf
ELECNEW templates\nec\ANSI_B_tabloid.dxf
```

The app's startup chooser (**Electrical drawing set**) does this for you: pick the standard,
size, project/title/company/drawing number, sheet count and folder, and it creates the set
from these templates with the title block filled and a `project.eprj` beside it. `ELECSTART`
reopens the chooser.

`ELECNEW <template.dxf>` copies the template's layers and every frame/title-block entity into
the drawing, then creates/keeps the electrical layers and loads the remembered symbol
library. Save the drawing under its own name afterwards — the template itself is never
modified.

## What is in every template

| layer | contents |
|---|---|
| `BORDER` | paper edge and inner frame, with zone tick marks |
| `ZONES` | zone numbers (1..n) and letters (A..) around the frame |
| `TITLE_BLOCK` | title block lines and its field texts |
| `WIRES`, `JUNCTIONS`, `WIRENO`, `XREF`, `ELEC-SYMBOLS` | the electrical layers with house colours |

A `WD_M` insert carries the numbering contract that `ELECSETUP` prints and `ELECNUMBER` /
`ELECPROJNUM` obey:

| setting | IEC | NEC (ASME) |
|---|---|---|
| `TAGFMT` / `TAGMODE` | `%F%N` / `S` (sequential; set `R` when released) | `%F%N` / `S` |
| `TAG-START`, `TAG-RSUF` | `1`, `A,B,C,…Y` | `1`, `A,B,C,…Y` |
| `WIREFMT` / `WIREMODE` | `%N` / `S` | `%N` / `S` |
| `XREFFMT` | `%S.%N` (sheet.number) | `%N` |
| `SHEET` | empty — fill it for a fixed sheet reference | empty |

The title block differs by standard: **IEC** uses an ISO 7200-style block (company, drawing
title, project, drawing number, revision, scale, size, sheet, date, drawn/checked/approved,
first-angle projection, `IEC 61082 / ISO 5457`), **NEC** uses an ASME Y14.1-style block
(company, title, project, drawing number, revision, scale, size, sheet, date,
drawn/checked/approved, third-angle projection, `ASME Y14.1 / NEC`).

## Sizes

| standard | files | sheet (mm) |
|---|---|---|
| IEC (ISO 5457) | `A4_landscape`, `A4_portrait`, `A3_landscape`, `A3_portrait`, `A2_landscape`, `A1_landscape`, `A0_landscape`, `B1_landscape` | 297×210, 210×297, 420×297, 297×420, 594×420, 841×594, 1189×841, 1000×707 |
| NEC (ASME Y14.1) | `ANSI_A_letter`, `ANSI_A_letter_portrait`, `ANSI_Legal`, `ANSI_B_tabloid`, `ANSI_C`, `ANSI_D`, `ANSI_E` | 279.4×215.9, 215.9×279.4, 355.6×215.9, 431.8×279.4, 558.8×431.8, 863.6×558.8, 1117.6×863.6 |

## Customising for your company

The title-block fields are ordinary `TEXT` entities, so you can edit them like any other
text in a drawing and save the result as your own template. For a permanent house template,
change the constants in `tools/make_templates.py` (company name, drawing-number prefix,
projection symbol, note text, margins, title-block proportions) and regenerate:

```powershell
py -3.13 tools\make_templates.py          # rewrites every file under templates\
```

To change the numbering contract instead, edit `IEC_SETUP` / `NEC_SETUP` in the same script
(`TAGMODE` `R` freezes references on released sheets, `TAGFMT` accepts `%F`, `%N`, `%S`,
`%X`), regenerate, and use the new template. `ELECSETUP` prints what a drawing's `WD_M`
currently says; the attribute manager (`ATTMAN`) is the in-app view of an insert's
attributes.

## Verification

Every template is checked by `target\tmp\pinprobe\verify9.py` (kept out of the repository
because it is a scratch script): open a drawing from the template, save, read it back, and
assert the paper extent, the standard title-block text, the `WD_M` values and that
`ELECSETUP` reads them. All fifteen pass — `15/15 templates verified`.
