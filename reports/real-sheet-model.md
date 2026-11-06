# Real ACADE sheet model — what is actually inside `import/Prints/Prints/DXF` (34 sheets)

Scope: the 34 ASCII DXF files `import/Prints/Prints/DXF/FC1000495-0xx.dxf`, plus the DWG and ACD
trees beside them. Read-only analysis; the only file written is this report.

Method and coverage:

* A dependency-free DXF reader was written for this analysis (scratch copy outside the repo, under
  `%TEMP%\acade_rsa`). It reads group-code pairs, HEADER variables, TABLES (LAYER, BLOCK_RECORD),
  BLOCKS (with ATTRIB/SEQEND folding), and model/paper-space ENTITIES (LINE, LWPOLYLINE with all
  vertices, POLYLINE/VERTEX, TEXT, MTEXT, INSERT+ATTRIB, CIRCLE, ARC, ELLIPSE, HATCH, SOLID, POINT,
  DIMENSION, VIEWPORT, SPLINE, LEADER). All 34 files parsed with 0 failures.
* The repo reader `tools/dxfio.py` was read first. It is fine for the R12 round-trip it was written
  for, but it discards exactly the facts this task needs: entity owner `330`, paper-space flag `67`,
  LWPOLYLINE vertices beyond the first, HEADER values beyond `$ACADVER`, and TABLE records other than
  LAYER. Every count below comes from the group-code parser, not from `dxfio`.
* Not interpreted: MTEXT text bodies (31 MTEXT entities on 032/035), exact SPLINE/ELLIPSE
  geometry (bbox approximation only), HATCH pattern internals, DIMENSION measurement values, and any
  proxy/custom objects (none were observed). All entity kinds that matter for tags/wires are read.
* All drawing content is model space. Paper space holds exactly one `VIEWPORT` per sheet (34 total)
  and nothing else. Title blocks are model-space `TEXT`/`LINE`/`HATCH`/`LWPOLYLINE` on layer
  `TITLE_BLOCK`.
* DWG: 33 files `import/Prints/Prints/DWG/20136-0xx.dwg` are binary AC1032 (the header bytes are
  `AC1032`). They use the **older project number 20136**, 046 is missing, and no DWG reader exists on
  this machine (no libredwg/ODA/ezdxf/`dwgread`). They are therefore unmeasured, not "the same".
* ACD: `import/Prints/Prints/PLC/FC1000495_AFR.ACD` is **binary** RSLogix 5000 V37.00.00
  (4,800,528 bytes; only a text banner and `Rx*` class names are plain text). No module/tag table is
  recoverable without Logix Designer or an L5X export, so the brief's "ASCII text" assumption is wrong.

---

## 0. Bottom line

| fact | measured value |
|---|---|
| model-space INSERT entities across 34 sheets | **999** (931 non-`WD_M`) |
| distinct block definitions across the set / block names actually inserted | **114** definitions; **79** distinct block names are referenced by the 999 inserts (the other definitions are 24 `*D*` dimension blocks referenced only by `DIMENSION`, plus unused `*_COPY`/`*X` defs) |
| INSERTs carrying ATTRIB children | **92** = 68 `WD_M` + **24 components**; 24/931 = **2.6 %** of non-setup inserts |
| sheets where the current `components_of()` finds a component | **3 of 34** (003: 4, 042: 7, 044: 13 = **24 components**) |
| sheets that contain *no* attached component attributes | **31 of 34** |
| device-tag TEXTs recoverable from the other sheets | **212 occurrences** (168 number-first, 35 family-first, 9 placeholders) |
| distinct device tags per sheet (tag TEXT ∪ TAG1/TAG2) | **218**; project-wide distinct **140** |
| sheets where a tag can be recovered | **28 of 34**; the other 6 are index/BOM/mechanical/hand-drawn sheets with genuinely no tags |
| wires | `LINE`; only **4 sheets** keep the ACADE `WIRES` layer, the other 30 carry wires as `LINE` on layer `0` |
| wire-number text | plain `TEXT`, layer `0`, height 0.15 or 0.125, 0.06–0.26 units off the wire |
| terminal strip/number | **not represented at all** (111 `TERMINAL` + 175 `BOX` inserts, 0 attributes, no `-X1:1` text anywhere) |
| cross-references | layers/attributes exist but **every XREF value is empty**; no xref text |

Root cause of "0 components on 30 sheets" is now exact: the symbols were **exploded** on 31 sheets,
so their `TAG1/TAG2/DESC` attributes were converted to plain `TEXT` (or lost); only 003, 042 and 044
retain a handful of attributed symbol inserts, and `components_of()` requires attributes.

Reconciliation with earlier runs: the shipped `ELECPROJECT` smoke test reported "34 sheet(s) read;
24 component(s) in 12 class(es)" — that is exactly the 4+7+13 attribute components found here, so the
app is reading the files correctly; the data is the problem. The brief's "0 on 30 of 34" needs one
correction: attached-attribute components exist on **3** sheets, and sheet 045 yields 0 even though it
contains 190 attribute entities (they are detached from any INSERT and every value is empty), so a
pass that counted 4 "sheets with attributes" counted attribute *entities*, not attached component
attributes.

---

## 1. Sheet inventory (all 34 sheets)

Titles are copied from the title-block title field. `kind` is my classification from title + content.
Counts: INSERTs = model-space INSERT entities; `sym` = inserts of electrical-symbol-class blocks,
**including** the `attr comps` subset (003: 4/4, 042: 7/21, 044: 13/19); `term` = `TERMINAL`+`BOX`;
`decor` = `DOT`/`WDDOT`/`GROUND`/`ELGND`/`TY`/`JUMPER`/`ARROW-*`/`MUFFLER`/`CYL-RET`/`PL`;
`anon/other` = `*X`, `*D`, `A$C*`, `*_COPY`, `SI`, `PT`, RJ45/connector blocks, empty modules;
`TEXT tags D/L/P` = distinct number-first / family-first / placeholder strings (see §3);
`tags+attr` = distinct device tags including non-empty `TAG1/TAG2`;
`app today` = components the current `components_of()` returns (attribute-carrying inserts);
`TEXT` = non-title-block TEXT entities on the sheet (5828 across the set; 6674 including title
block); `LINE` = all LINE entities (24083 across the set).

| sheet | title (title block) | kind | INSERTs | WD_M | attr comps | sym | term | decor | anon/other | TEXT tags D/L/P | tags+attr | app today | TEXT | LINE |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|
| 001 | INDEX SHEET | index | 3 | 2 | 0 | 0 | 0 | 0 | 1 | 0/0/0 | **0** | 0 | 162 | 36 |
| 002 | 480VAC POWER DIST. | schematic | 53 | 2 | 0 | 7 | 20 | 15 | 9 | 13/0/0 | **13** | 0 | 240 | 249 |
| 003 | 480VAC POWER DIST. | schematic | 33 | 2 | 4 | 4 | 12 | 15 | 0 | 6/0/0 | **8** | 4 | 115 | 127 |
| 004 | 480VAC SPARE POWER DISTRIBUTION | spare | 2 | 2 | 0 | 0 | 0 | 0 | 1 | 1/0/0 | **1** | 0 | 48 | 37 |
| 007 | CUTTER DRIVE POWERFLEX 70 | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 217 | 231 |
| 008 | CRIMP DRIVE POWERFLEX 70 | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 216 | 230 |
| 009 | SLITTER DRIVE POWERFLEX 70 | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 217 | 230 |
| 010 | INTERMEDIATE CONV. POWERFLEX 70 | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 214 | 234 |
| 011 | CUTTER IN FEED NIP POWERFLEX 70 | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 214 | 233 |
| 012 | AFR INFEED CONV. POWERFLEX 70 | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 182 | 181 |
| 013 | DORNER OUTFEED CONV. POWERFLEX | schematic | 23 | 2 | 0 | 2 | 9 | 9 | 1 | 2/3/0 | **5** | 0 | 182 | 180 |
| 014 | ANGLED CONV. DRIVE POWERFLEX 70 | schematic | 19 | 2 | 0 | 2 | 7 | 7 | 1 | 2/3/0 | **5** | 0 | 180 | 179 |
| 015 | CRIMP ROLL HEATERS | schematic | 40 | 2 | 0 | 6 | 10 | 22 | 0 | 2/0/0 | **2** | 0 | 166 | 199 |
| 020 | 120VAC POWER DISTRIBUTION | schematic | 47 | 2 | 0 | 8 | 6 | 16 | 15 | 9/0/0 | **9** | 0 | 138 | 303 |
| 022 | 24VDC POWER DISTRIBUTION | schematic | 63 | 2 | 0 | 8 | 16 | 21 | 16 | 8/0/0 | **8** | 0 | 144 | 111 |
| 025 | SAFETY RELAY CIRCUIT | schematic | 89 | 4 | 0 | 28 | 18 | 18 | 21 | 6/0/0 | **6** | 0 | 219 | 260 |
| 027 | ETHERNET SWITCH LAYOUT | panel/network layout | 33 | 4 | 0 | 0 | 0 | 0 | 29 | 0/10/0 | **10** | 0 | 150 | 81 |
| 031 | MAIN PANEL LAYOUT | panel layout | 4 | 2 | 0 | 0 | 0 | 2 | 0 | 38/0/0 | **38** | 0 | 321 | 8168 |
| 032 | MAIN PANEL EXTERIOR | mechanical | 8 | 2 | 0 | 3 | 0 | 1 | 2 | 1/0/0 | **1** | 0 | 48 | 430 |
| 035 | HMI ENCLOSURE LAYOUT | physical layout | 12 | 2 | 0 | 5 | 0 | 1 | 4 | 3/0/0 | **3** | 0 | 48 | 5877 |
| 036 | PUSHBUTTON BOX LAYOUTS | physical layout | 16 | 2 | 0 | 6 | 0 | 0 | 8 | 4/0/0 | **4** | 0 | 89 | 432 |
| 037 | FIELD JUNCTION BOX | mechanical | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0/0/0 | **0** | 0 | 3 | 1755 |
| 038 | FIELD JUNCTION BOX | mechanical | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0/0/0 | **0** | 0 | 67 | 1000 |
| 040 | PLC POWER SUPPLY AND PROCESSOR | hand-drawn schematic | 6 | 2 | 0 | 0 | 0 | 0 | 4 | 0/0/0 | **0** | 0 | 46 | 34 |
| 041 | 5069-IB16 INPUT MODULES (X2) | schematic | 94 | 2 | 0 | 18 | 45 | 29 | 0 | 15/0/0 | **15** | 0 | 322 | 196 |
| 042 | 5069-OB16 OUTPUT MODULES (X2) | schematic | 88 | 1 | 7 | 21 | 40 | 23 | 3 | 22/0/0 | **24** | 7 | 337 | 181 |
| 043 | HONEYWELL 8 POINT ANALOG INPUT | schematic | 46 | 2 | 0 | 12 | 8 | 24 | 0 | 2/0/0 | **2** | 0 | 202 | 117 |
| 044 | HONEYWELL 8 POINT ANALOG INPUT | schematic, native | 52 | 1 | 13 | 19 | 7 | 25 | 0 | 2/0/0 | **13** | 13 | 205 | 126 |
| 045 | HONEYWELL 8 POINT ANALOG INPUT | schematic, exploded | 22 | 1 | 0 | 0 | 2 | 0 | 19 | 2/0/0 | **2** | 0 | 184 | 85 |
| 046 | INTERLOCK SHEET | schematic | 42 | 2 | 0 | 8 | 32 | 0 | 0 | 3/0/2 | **5** | 0 | 41 | 72 |
| 047 | ELECTRICAL BILL OF MATERIAL | BOM list | 8 | 1 | 0 | 0 | 0 | 0 | 7 | 0/0/0 | **0** | 0 | 509 | 432 |
| 049 | PNEUMATIC LAYOUT | pneumatic layout | 34 | 2 | 0 | 1 | 0 | 10 | 21 | 4/0/0 | **4** | 0 | 86 | 371 |
| 050 | HEYPAC UNIT | pneumatic layout | 4 | 2 | 0 | 0 | 0 | 1 | 1 | 10/0/0 | **10** | 0 | 30 | 1272 |
| 051 | PNEUMATIC BILL OF MATERIAL | BOM list | 16 | 2 | 0 | 0 | 0 | 0 | 14 | 0/0/0 | **0** | 0 | 286 | 434 |
| **TOTAL** | | | **999** | **68** | **24** | **170** | **286** | **293** | **182** | 167/34/2 | **218** | **24** | 5828 | 24083 |

Notes: 6674 TEXT total, 846 of them on `TITLE_BLOCK`; 5828 non-title. `*D` blocks are defined but not
inserted — `DIMENSION` entities reference them (23 dimensions on 031/032/035). `LINE` totals include
mechanical line-work on layouts (031 has 8168, 035 has 5877, 037 1755, 050 1272, 038 1000).

---

## 2. INSERT inventory — every block, with count, layer, size and ATTRIB status

All 999 model-space inserts. "ATTRIB?" = inserts that carry at least one ATTRIB child, `x/y`.
"core bbox" excludes nested INSERT children (DOT/WDDOT/TY are concentric circles, so their bbox is
0 — their real radii are 0.019–0.062). "nested ins" counts INSERT children inside the block
definition. Layers: `SYMS`/`VP`/`SYMBOLS`/`TUBING_TAGS` appear only on native/layout sheets; every
other insert is on layer `0` (the flattening moved all layers to 0 on 30 sheets).

| block | ins | sh | layer | ATTRIB? | core bbox (no nested inserts) | nested ins | class | ATTDEFs |
|---|---:|---:|---|---|---|---:|---|---|
| `*D1187` | 0 | 0 |  | no | 3.305x18.000 | 0 | dimension anon | — |
| `*D12407` | 0 | 0 |  | no | 18.000x1.305 | 0 | dimension anon | — |
| `*D12421` | 0 | 0 |  | no | 7.500x0.680 | 0 | dimension anon | — |
| `*D12584` | 0 | 0 |  | no | 1.430x2.000 | 0 | dimension anon | — |
| `*D12598` | 0 | 0 |  | no | 1.430x3.188 | 0 | dimension anon | — |
| `*D12612` | 0 | 0 |  | no | 2.367x4.250 | 0 | dimension anon | — |
| `*D12626` | 0 | 0 |  | no | 1.930x2.875 | 0 | dimension anon | — |
| `*D12640` | 0 | 0 |  | no | 0.867x2.250 | 0 | dimension anon | — |
| `*D1549` | 0 | 0 |  | no | 0.992x1.750 | 0 | dimension anon | — |
| `*D1563` | 0 | 0 |  | no | 2.680x1.747 | 0 | dimension anon | — |
| `*D352` | 0 | 0 |  | no | 0.711x2.250 | 0 | dimension anon | — |
| `*D8951` | 0 | 0 |  | no | 4.735x4.737 | 0 | dimension anon | — |
| `*D8966` | 0 | 0 |  | no | 4.779x4.777 | 0 | dimension anon | — |
| `*D8983` | 0 | 0 |  | no | 1.943x1.942 | 0 | dimension anon | — |
| `*D9556` | 0 | 0 |  | no | 2.309x0.867 | 0 | dimension anon | — |
| `*D9575` | 0 | 0 |  | no | 1.657x0.237 | 0 | dimension anon | — |
| `*D9589` | 0 | 0 |  | no | 0.774x1.022 | 0 | dimension anon | — |
| `*D9604` | 0 | 0 |  | no | 4.425x1.367 | 0 | dimension anon | — |
| `*D9618` | 0 | 0 |  | no | 3.932x3.675 | 0 | dimension anon | — |
| `*D9636` | 0 | 0 |  | no | 0.867x0.518 | 0 | dimension anon | — |
| `*D9680` | 0 | 0 |  | no | 2.249x1.701 | 0 | dimension anon | — |
| `*D9693` | 0 | 0 |  | no | 9.000x1.206 | 0 | dimension anon | — |
| `*D9707` | 0 | 0 |  | no | 4.491x1.263 | 0 | dimension anon | — |
| `*X0` | 2 | 2 | VP | no | 1.052x0.500 | 0 | hatch/anon | — |
| `*X1` | 2 | 2 | VP | no | 0.250x0.250 | 0 | hatch/anon | — |
| `*X10` | 1 | 1 | VP | no | 0.437x2.250 | 0 | hatch/anon | — |
| `*X11` | 1 | 1 | VP | no | 1.938x0.895 | 0 | hatch/anon | — |
| `*X12` | 1 | 1 | VP | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X13` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X14` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X16` | 3 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X19` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X2` | 2 | 2 | VP | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X20` | 3 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X22` | 4 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X23` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X24` | 2 | 2 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X25` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X26` | 4 | 2 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X27` | 4 | 2 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X28` | 2 | 2 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X29` | 2 | 2 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X3` | 2 | 2 | VP | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X30` | 2 | 2 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X31` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X32` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X35` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X36` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X4` | 2 | 2 | VP | no | 1.938x0.895 | 0 | hatch/anon | — |
| `*X40` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X43` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X49` | 1 | 1 | 0 | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X5` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X6` | 0 | 0 |  | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X8` | 4 | 2 | 0,VP | no | 0.100x0.075 | 0 | hatch/anon | — |
| `*X9` | 1 | 1 | VP | no | 0.100x0.075 | 0 | hatch/anon | — |
| `24VDCPRX` | 5 | 1 | 0 | no | 1.238x0.707 | 2 | electrical symbol | — |
| `A$C14231A43` | 3 | 1 | 0 | no | 0.000x0.000 | 0 | anon wrapper | — |
| `A$C5DBF6971` | 8 | 8 | 0 | no | empty | 1 | data connector | — |
| `ARROW-L` | 6 | 3 | 0 | no | 0.000x0.000 | 0 | junction/decoration | — |
| `ARROW-S` | 2 | 2 | 0 | no | 0.000x0.000 | 0 | junction/decoration | — |
| `ARYA_MODULE_ACT050_42L_S_6BE` | 1 | 1 | SYMBOLS | no | 38.552x23.000 | 0 | PLC/module | — |
| `BOX` | 175 | 19 | 0 | no | 0.625x0.250 | 0 | terminal | — |
| `BOX_COPY` | 0 | 0 |  | no | 0.625x0.250 | 0 | duplicate block | — |
| `CBL CNCTR` | 10 | 1 | 0 | no | 0.375x0.375 | 0 | other | — |
| `COIL` | 8 | 2 | 0 | no | 1.000x0.008 | 0 | electrical symbol | — |
| `CONT-NC` | 14 | 2 | 0 | no | 0.875x0.250 | 0 | electrical symbol | — |
| `CONT-NO` | 38 | 14 | 0 | no | 0.875x0.250 | 0 | electrical symbol | — |
| `CYL-RET` | 2 | 1 | 0 | no | 0.375x1.125 | 0 | junction/decoration | — |
| `DOT` | 143 | 19 | 0 | no | 0.000x0.000 | 1 | junction/decoration | — |
| `DOT_COPY` | 14 | 1 | 0 | no | 0.000x0.000 | 1 | duplicate block | — |
| `ELCB` | 11 | 2 | 0 | no | 0.375x0.500 | 0 | electrical symbol | — |
| `ELGND` | 6 | 2 | 0 | no | 0.250x0.375 | 0 | junction/decoration | — |
| `ELLIPSE` | 1 | 1 | 0 | no | 0.000x0.000 | 0 | other | — |
| `FAN` | 3 | 1 | 0 | no | 9.646x9.646 | 0 | electrical symbol | — |
| `FGA1010 FRONT` | 2 | 1 | 0 | no | 12.875x11.625 | 0 | other | — |
| `GROUND` | 56 | 11 | 0 | no | 0.312x0.375 | 0 | junction/decoration | — |
| `HA1S1` | 1 | 1 | SYMS | yes 1/1 | 0.113x0.444 | 0 | electrical symbol | DESC1, X4TERM01, SIGCODE, WIRENO… |
| `HCR1` | 2 | 1 | SYMS | yes 2/2 | 1.559x1.078 | 0 | electrical symbol | INST, DESC3, ASSYCODE, LOC… |
| `HCR21` | 2 | 1 | SYMS | yes 2/2 | 0.250x1.195 | 0 | electrical symbol | INST, DESC3, X8LINK, X2LINK… |
| `HDV1TFL` | 6 | 1 | SYMS | yes 6/6 | 0.688x1.353 | 0 | electrical symbol | INST, ASSYCODE, RATING2, RATING1… |
| `HDV1TFL_COPY` | 0 | 0 |  | no | 0.688x1.353 | 0 | duplicate block | — |
| `HLT1G` | 1 | 1 | SYMS | yes 1/1 | 1.311x0.841 | 0 | electrical symbol | WDTAGALT, INST, ASSYCODE, X1TERM02… |
| `HMO13` | 1 | 1 | SYMS | yes 1/1 | 0.568x1.278 | 0 | electrical symbol | WDTAGALT, INST, DESC3, ASSYCODE… |
| `HMS1` | 3 | 1 | SYMS | yes 1/3 | 1.559x1.078 | 0 | electrical symbol | INST, DESC3, ASSYCODE, LOC… |
| `HMS21` | 3 | 1 | SYMS | yes 3/3 | 0.250x1.217 | 0 | electrical symbol | INST, DESC3, X8LINK, X2LINK… |
| `HPX11` | 2 | 1 | SYMS | yes 2/2 | 0.500x1.310 | 0 | electrical symbol | WDTAGALT, INST, TERM03, LOC… |
| `HSV1` | 1 | 1 | SYMS | yes 1/1 | 1.383x1.080 | 0 | electrical symbol | WDTAGALT, XREF, INST, DESC3… |
| `HT0_01` | 2 | 1 | SYMS | yes 2/2 | 0.125x0.125 | 0 | electrical symbol | X8TERM01, X2TERM01, X1TERM01, X4TERM01 |
| `HT0_02` | 4 | 1 | SYMS | yes 2/4 | 0.140x0.140 | 0 | electrical symbol | X8TERM01, X2TERM01, X1TERM01, X4TERM01 |
| `JUMPER` | 5 | 2 | 0 | no | 0.875x0.312 | 0 | junction/decoration | — |
| `MODULE` | 3 | 2 | 0 | no | empty | 0 | PLC/module | — |
| `MUFFLER` | 2 | 1 | 0 | no | 0.469x0.188 | 0 | junction/decoration | — |
| `PB-NC` | 12 | 4 | 0 | no | 0.875x0.094 | 0 | electrical symbol | — |
| `PB-NO` | 11 | 4 | 0 | no | 0.875x0.219 | 0 | electrical symbol | — |
| `PHONEJACK` | 0 | 0 |  | no | 0.375x0.375 | 0 | data connector | — |
| `PL` | 2 | 2 | 0 | no | 1.000x0.359 | 0 | junction/decoration | — |
| `PLUG` | 24 | 4 | 0 | no | 0.219x0.250 | 0 | electrical symbol | — |
| `PLUG_COPY` | 4 | 1 | 0 | no | 0.219x0.250 | 0 | duplicate block | — |
| `PRESS-NO` | 1 | 1 | 0 | no | 0.875x0.363 | 0 | electrical symbol | — |
| `PRESS-SW` | 1 | 1 | 0 | no | 0.375x0.906 | 1 | electrical symbol | — |
| `PT` | 59 | 6 | 0 | no | 0.089x0.089 | 0 | other (connection point) | — |
| `RJ45 8 FCNCTR` | 21 | 2 | 0 | no | 0.433x0.276 | 0 | other (data connector) | — |
| `SI` | 1 | 1 | 0 | no | 9.375x20.312 | 0 | title block logo | — |
| `SOL` | 8 | 1 | 0 | no | 1.000x0.375 | 0 | electrical symbol | — |
| `SURGE` | 6 | 1 | 0 | no | 0.875x0.375 | 2 | electrical symbol | — |
| `SW_TABLEANNOTATION_0` | 1 | 1 | TUBING_TAGS | no | 7.211x0.933 | 0 | other | — |
| `TERMINAL` | 111 | 6 | 0 | no | 0.108x0.062 | 0 | terminal | — |
| `TY` | 46 | 3 | 0 | no | 0.000x0.000 | 0 | junction/decoration | — |
| `WDDOT` | 23 | 2 | SYMS | no | 0.000x0.000 | 0 | junction/decoration | — |
| `WDDOT_COPY` | 0 | 0 |  | no | 0.000x0.000 | 0 | duplicate block | — |
| `WD_M` | 68 | 34 | 0 | yes 68/68 | 0.014x1.402 | 0 | setup | RUNGHORV, REFNUMS, RUNGDIST, DLADW… |
| `WEB MODULE` | 3 | 2 | 0 | no | empty | 0 | PLC/module (empty) | — |
| `_X19_COPY` | 0 | 0 |  | no | 0.100x0.075 | 0 | duplicate block | — |

Facts that matter for writing the recognizer:

1. **`WD_M`** is the only 77-ATTRIB block and the only setup block. 68 inserts, all on layer 0 at
   (0,0) or one of two stray offsets. Always skip it as a component.
2. **`*X0`–`*X49`** are anonymous **hatch/decoration** blocks: the common definition is 7 parallel
   `LINE`s in a 0.100x0.075 lens shape; `*X0`–`*X11` are hatch fragments of 10–64 lines used inside
   title-block notes and inside symbol definitions. **They are not components.** Several symbol
   definitions (DOT, SURGE, 24VDCPRX, PRESS-SW) contain a nested `*X` insert whose offset is
   (-3.25,-1.15) or similar, i.e. 3–4 units away from the symbol origin: any code that flattens
   block definitions will pick up **phantom strokes far outside the symbol**. That is a data defect
   in these files, not a symbol feature.
3. **`*D1187`–`*D9707`** (24 definitions) are **anonymous dimension blocks**; no INSERT references
   them — `DIMENSION` entities do, via group 2. Do not scan them for components.
4. **`*_COPY`** (`BOX_COPY`, `DOT_COPY`, `PLUG_COPY`, `WDDOT_COPY`, `HDV1TFL_COPY`, `_X19_COPY`) are
   duplicate definitions created by copy operations. `DOT_COPY` (14 inserts, 045) and `PLUG_COPY`
   (4, 045) are inserted; the others are unreferenced. `HDV1TFL_COPY` is unusual: its block
   definition contains **24 ATTRIB entities**, not ATTDEFs. Treat all `*_COPY` as the same class as
   their base block.
5. **`MODULE`** and **`WEB MODULE`** have **empty definitions** (0 entities) and are inserted at
   (0,0) on 047/051. They are no-ops; the actual module information on those sheets is plain text
   (BOM rows), not geometry.
6. **`A$C5DBF6971`** is a 1-INSERT wrapper around `PHONEJACK` (scale 0.5, inserted at scale 2 /
   rot 270) — i.e. the Ethernet-port symbol on 007–014. **`A$C14231A43`** is a 1-TEXT wrapper whose
   text is `*, %%U2513` — an ACADE wire/cable marker, used 3× on 025.
7. `ARYA_MODULE_ACT050_42L_S_6BE` (1 insert, 045, layer `SYMBOLS`, scale 0.075) is a block whose name
   matches this app's own naming, i.e. 045 was at some point touched by AryaCAD/this project. Treat
   045 as contaminated evidence.
8. Terminal graphics are `TERMINAL` (0.108x0.062 circle+line, 111 inserts on 6 sheets) and `BOX`
   (0.625x0.250 4-line rectangle, 175 inserts on 19 sheets). **Neither carries attributes.**

---

## 3. TAG RECOVERY — where the tags actually are

### 3.1 Three storage forms, by sheet class

| form | sheets | count | evidence |
|---|---|---|---|
| `ATTRIB` on an INSERT (`TAG1`/`TAG2`), attributes on layer `TAGS` | **003, 042, 044** | 24 inserts, 19 with a non-empty tag, 16 distinct tag values project-wide | e.g. 003 `HMS21` TAG2=`M326`, `HMO13` TAG1=`HYD. UNIT`; 042 `HMS1` TAG1=`M326`, `HLT1G` TAG1=`START PB`, `HCR21` TAG2=`4220CON`/`4270CON`; 044 `HDV1TFL` TAG1=`CT1`…`CT6`, `HPX11` TAG1=`SM1`/`SM2`, `HCR1` TAG1=`BOT`/`CR.MS`, `HSV1` TAG1=`MAIN_SOL` |
| exploded `TEXT`, layer `0` | 24 other sheets | 200 of 212 tag TEXTs | verified per-sheet in §3.4 |
| exploded `TEXT`, layer `TUBING_TAGS` | 049, 050 | 12 | pneumatic valve tags `4254SV`, `4256SV`, `4258SV`, `4260SV`, `4262SV`… |
| `ATTRIB` present but **detached from any INSERT** | 045 | 190 ATTRIB entities, **all 190 values empty** | layers `LOC` 22, `DESC` 23, `MISC` 120, `TERMS` 12, `XREF` 13; no INSERT precedes them |

On native sheets the attribute layers are the ACADE standards from `WD_M`: `TAGS`, `DESC`,
`DESCCHILD`, `TERMS`, `LOC`, `XREF`, `XREFCHILD`, `MISC`, `SYMS`, `WIRES`. On the 30 flattened
sheets **all of these layers are gone** and everything is on layer `0` (plus `TITLE_BLOCK`), except
the pneumatic tags on `TUBING_TAGS` and 23 wire-spec texts on layer `TEXT` on 002/022/025/027.

### 3.2 Observed tag formats (this project's actual convention)

`WD_M.TAGFMT` is `%F%N` (family then number) but **the tags actually written are number-first**:
`264CB`, `4214CON`, `4104PB`, `4268SV`. Do not re-render stored tags through `TAGFMT` — store the
string that is in the drawing.

* number-first: `^\d{3,5}[A-Z]{2,4}$` — **168 occurrences, 167 distinct per sheet, 27 sheets**.
  Families observed (occurrences): `CB` 38, `CR` 33, `PB` 15, `PRX` 13, `CON` 10, `SV` 9, `DRV` 8,
  `PDB` 7, `FB` 7, `SR` 7, `NF` 5, `PS` 4, `SCR` 4, `SVA` 2, `SVB` 2, `MDS` 1, `TR` 1, `LT` 1,
  `PE` 1 (`4164PE`). Also `SV`, `SVA`, `SVB` are one device family with suffixes.
* family-first: `^[A-Z]{2,4}\d{1,4}$` — **35 occurrences, 34 distinct per sheet, 9 sheets**:
  `BR1`/`BR2` (bridge rectifier labels, 007–014, 16 occurrences — hand-drawn, not ACADE symbols),
  `TB3` (I/O terminal block label, 007–014, 8), `MPE102`…`MPE208` (Ethernet jacks, 027, 10 distinct
  / 11 occurrences).
* placeholders: `^X{3,}[A-Z]*$` — **9 occurrences on 046**: `XXXXX` ×6, `XXXXCR` ×3. These are
  ACADE's "tag not assigned" forms; list them as components with `tag = ""` / `unassigned`.
* attribute tags: 19 values on 24 inserts; distinct per sheet 17, project-wide 16. Values:
  `HYD. UNIT`, `M326`, `START PB`, `4220CON`, `4270CON`, `BOT`, `SM1`, `SM2`, `CR.MS`, `MAIN_SOL`,
  `CT1`…`CT6`.

### 3.3 The rule (and what it gets)

```
TAG(t):
  if t matches ^\d{3,5}[A-Z]{2,4}$ and suffix not in {VAC,VDC,AMP,HZ,AWG}:        -> accept (number-first)
  if t matches ^[A-Z]{2,4}\d{1,4}$ and not prefixed IN/OUT/CH:                   -> accept (family-first)
  if t matches ^X{3,}[A-Z]*$:                                                    -> accept as UNASSIGNED
  else: reject
Reject before matching:
  layer == TITLE_BLOCK ; entity is not TEXT ; entity is in paper space
  pure digits ^\d{1,6}$            (wire numbers)
  phase/leg labels  L1 L2 L3, 2L1..3L3, 264L1, 733T1 (digit+letters+digit)
  channel labels    IN-0..IN-15, OUT-0..OUT-15, CH-0..CH-7
  ratings/units     ^#?[\d.]+ ?(AWG)?$, 120VAC/480VAC/600VAC/24VDC/+24VDC/+5VDC/+12VDC, 60HZ, 20AMP, 20/30/63/4/6/8/2A, 480W, 2.2kW, 20A, 24VDC
  part numbers      strings containing '-' such as LPJ-100SP, 1783-USP16T, FAZ-C1/1-NA-SP, EAP-6-221-D, A-202006LP
  notes/addresses   'SHEET #3', 'R.P. 10.0.1.20', 'TYP.', '%%UNOTE:'
```

Measured result of the rule (manual audit of every hit; none of the rejected classes was mislabelled):

| tier | occurrences | distinct per sheet | distinct project-wide | sheets |
|---|---:|---:|---:|---:|
| attribute tags (`TAG1`/`TAG2` non-empty) | 19 (on 24 component inserts) | 17 | 16 | 3 |
| number-first TEXT | **168** | 167 | 111 | 27 |
| family-first TEXT | 35 | 34 | 13 (`BR1`,`BR2`,`TB3`, `MPE102,103,104,105,202,203,204,205,206,208`) | 9 |
| placeholders | 9 | 2 | 2 | 1 |
| **total (union)** | **231 tag-bearing instances** | **218** | **140** | **28** |

Sanity check of the union: 126 distinct TEXT tags (111+13+2) union 16 distinct attribute tags, minus
the 2 tags that occur both as attribute and as TEXT (`4220CON`, `4270CON` on 042) = **140** distinct
project-wide. The 218 per-sheet figure counts a tag once per sheet, so cross-sheet repeats
(e.g. `2517CR` on 022/025/044, `1522SR` on 015/043/045, `M326` on 003/042) are separate there.

Precision: 0 false positives were found among the 203 distinct TEXT hits (all 168 number-first hits
audited against a symbol or panel-layout context; the 35 family-first hits are `BR`/`TB`/`MPE`
labels, which are legitimately tagged items). Recall cannot be proved from the DXF alone; three
quantified gaps:

* 6 sheets have no tags at all and should report 0: 001 (index), 037, 038 (junction-box mechanical),
  047, 051 (BOM text lists), 040 (PLC power supply drawing is hand-drawn; its wire labels 4050–4090
  are pure digits and correctly rejected).
* 045's device tags are unrecoverable: 190 detached ATTRIBs, every value empty; only the two TEXT
  tags `1522SR`/`1572SR` survive.
* 5 attribute-carrying components have no tag value even on native sheets: `HA1S1` (042), `HT0_02`×2
  (042), `HT0_01`×2 (044); plus every surviving non-attribute `CONT-NO`/`COIL`/`ELCB` part on the
  flattened sheets is unnamed (those symbols carry contact/coil graphics, not the device tag).

### 3.4 Per-sheet recovery counted sheet by sheet

`D` = number-first tags (distinct), `L` = family-first, `P` = placeholders, `A` = non-empty
`TAG1/TAG2` on inserts; `all` = distinct tag set. Full lists are in Appendix A.

| sheet | D | L | P | A | all | sheet | D | L | P | A | all |
|---|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|
| 001 | 0 | 0 | 0 | 0 | 0 | 025 | 6 | 0 | 0 | 0 | 6 |
| 002 | 13 | 0 | 0 | 0 | 13 | 027 | 0 | 10 | 0 | 0 | 10 |
| 003 | 6 | 0 | 0 | 2 | 8 | 031 | 38 | 0 | 0 | 0 | 38 |
| 004 | 1 | 0 | 0 | 0 | 1 | 032 | 1 | 0 | 0 | 0 | 1 |
| 007 | 2 | 3 | 0 | 0 | 5 | 035 | 3 | 0 | 0 | 0 | 3 |
| 008 | 2 | 3 | 0 | 0 | 5 | 036 | 4 | 0 | 0 | 0 | 4 |
| 009 | 2 | 3 | 0 | 0 | 5 | 037 | 0 | 0 | 0 | 0 | 0 |
| 010 | 2 | 3 | 0 | 0 | 5 | 038 | 0 | 0 | 0 | 0 | 0 |
| 011 | 2 | 3 | 0 | 0 | 5 | 040 | 0 | 0 | 0 | 0 | 0 |
| 012 | 2 | 3 | 0 | 0 | 5 | 041 | 15 | 0 | 0 | 0 | 15 |
| 013 | 2 | 3 | 0 | 0 | 5 | 042 | 22 | 0 | 0 | 4 | 24 |
| 014 | 2 | 3 | 0 | 0 | 5 | 043 | 2 | 0 | 0 | 0 | 2 |
| 015 | 2 | 0 | 0 | 0 | 2 | 044 | 2 | 0 | 0 | 11 | 13 |
| 020 | 9 | 0 | 0 | 0 | 9 | 045 | 2 | 0 | 0 | 0 | 2 |
| 022 | 8 | 0 | 0 | 0 | 8 | 046 | 3 | 0 | 2 | 0 | 5 |
| | | | | | | 047 | 0 | 0 | 0 | 0 | 0 |
| | | | | | | 049 | 4 | 0 | 0 | 0 | 4 |
| | | | | | | 050 | 10 | 0 | 0 | 0 | 10 |
| | | | | | | 051 | 0 | 0 | 0 | 0 | 0 |

### 3.5 Tag vs description vs part number (they are separate TEXTs)

* On native sheets `DESC1/2/3` are **separate ATTRIB entities on layer `DESC`/`DESCCHILD`**, and
  `CAT`/`MFG`/`FAMILY` are invisible attributes (height 0.01–0.0125) on layer `MISC`. So tag,
  description and part are distinct fields by construction.
* Canonical attribute offsets measured on the 24 native inserts (relative to the insert origin):

  | attribute | dx | dy | height |
  |---|---|---|---|
  | `TAG1`/`TAG2` | −0.47 … −0.14 | +0.08 … +0.42 | 0.125 |
  | `DESC1` | 0 or +0.376 or +1.125 | −0.45 or +0.19 or 0 | 0.125 |
  | `DESC2`, `DESC3` | same as DESC1 | DESC1 − 0.156 each | 0.125 |
  | `LOC` | ≈0 | +0.23 … +0.67 | 0.125 |
  | `INST` | ≈0 | +0.40 … +0.84 | 0.125 |
  | `XREF` | 0 or +1.13 | −0.30 … −0.61 | 0.125 |
  | `TERM01`…`TERM04` | −0.27 … +0.06 | −0.16 | 0.0625 … 0.125 |
  | `FAMILY`,`CAT`,`MFG`,`ASSYCODE` | 0 | +0.10 … +0.16 | 0.01 |

* On flattened sheets the description is the **nearest non-tag TEXT**: measured for the 212 TEXT
  tags, 91 are within 0.5 units of a non-tag TEXT, 145 within 1.0, 190 within 1.5, 205 within 2.0.
  Those description texts have height 0.125 (142 cases), 0.15 (35), 0.1875 (10), 0.1 (5), and sit
  on layer 0 (189), `TUBING_TAGS` (12), `TITLE_BLOCK` (3), `TEXT` (1). Examples: `63A CB`, `8A CB`,
  `1.0-1.6A CB`, `DISCONNECT SWITCH`, `100 AMP FUSED`, `SAFETY MAT 1`, `HEATER LEG 1 AMPS`.
* Part numbers are also free TEXT and are **not reliably tied to a tag**: `LPJ-100SP` next to the
  disconnect switch on 002, `ITV3030-04N4BL4` on 044, `FAZ-C1/1-NA-SP` / `5069-IB16/A` on 027,
  `EAP-6-221-D`, `SCE936B`, `1783-USP16T` on the layout sheets. Only on native sheets is the part
  in a proper `CAT` attribute (all `CAT` values are empty there, so even native sheets have no part
  data in attributes).
* Tag heights across the 212 TEXT tags: 0.125 (176), 0.15 (23), 0.1875 (4), 0.25 (4), 0.1 (3), 0.165
  (1), 0.0938 (1). Justification is mixed: (72,73) = (4,0) middle 67×, (1,0) centre 55×, (0,0)
  default 43×, (1,2) 23×, (2,0) right 12×, (0,1) 12×. **Use group code 11/21 as the anchor when
  72/73 is set** — otherwise tag offsets are off by up to half the text width (e.g. `264CB`:
  insertion point (20.258,15.562) but the real anchor is (20.562,15.562), centred over the symbol).

### 3.6 Multi-part devices — do not double-count

The same device tag legitimately appears on several parts:

* 003: `M326` on **3** `HMS21` inserts (three contacts of one motor starter) → one device.
* 042: `4220CON` and `4270CON` each appear as an attribute on an `HCR21` insert **and** as a tag
  TEXT elsewhere on the sheet → one device per tag, not two.
* 046: `XXXXX`/`XXXXCR` placeholders: 9 unassigned contact positions.

So per-sheet counts in §3.4/Appendix A are **distinct tags**, not inserts.

---

## 4. WIRE STRUCTURE — how wires and nets are actually stored

### 4.1 Storage

* Wires are `LINE` entities. **Only 4 sheets keep an ACADE `WIRES` layer:**

  | sheet | WIRES-layer entities |
  |---|---|
  | 003 | 8 LINE |
  | 042 | 14 LINE |
  | 044 | 61 LINE + **11 ARC** (the hop arcs) |
  | 045 | 3 LWPOLYLINE |

  On the other 30 sheets there is no `WIRES` layer at all; wires are `LINE` on layer `0`, mixed with
  exploded symbol strokes. Wire length is the only usable separator: on schematics, symbol strokes
  are mostly < 0.35 units while wire runs are 0.5–5 units (non-title layer-0 lines ≥ 0.5: 002 117 of
  240, 046 53 of 63, 041 167 of 187, 043 79 of 108). A threshold of **0.5 units on layer 0** is a good wire
  filter for schematics; 0.35 catches more wires but starts including symbol strokes.
* No polyline wires on schematic sheets except 045 (3 LWPOLYLINE on `WIRES`) and 027 (network layout
  cable runs). Split LWPOLYLINE into segments by consecutive vertex pairs.
* Entity kinds on schematic sheets: `LINE`, `ARC` (wire hops), `CIRCLE`/`ELLIPSE` (symbol bodies,
  terminals), `TEXT`, `INSERT`, `HATCH`/`SOLID` (fills, arrowheads).

### 4.2 Wire numbers

Wire number texts are plain `TEXT`, **layer `0`**, not attributes, not associated by handle.

* Numbering format is `WIREFMT=%N` → pure digits: `738`, `739`, `1500`, `2000`, `22280`, `41040`,
  `42040`, `25055`. Three-phase wire numbers repeat the same number on L1/L2/L3 (e.g. `1500`
  appears 3× on 002).
* A second style carries the phase/leg as a suffix: `264L1`, `264L2`, `264L3`, `733T1`, `1517L1`,
  `2L1`. These are still wire numbers, not tags — the pattern `digit+letter+digit` is why the tag
  regex must not accept them.
* Height: 0.15 on 002/007–025 (dominant), 0.125 on 041–046. Offset from the wire (signed
  perpendicular, measured over all pure-digit texts within 1.0 of a wire): **+0.19 (575), −0.06
  (211), +0.06 (146), −0.19 (115), +0.12 (89)**; per sheet the dominant offset is +0.19 on
  007–014 and +0.06 on 020/022/025/041/043/044/046 (negative values are the same offsets on
  vertical wires, where the offset is horizontal). Practically: wire number = pure-digit TEXT
  0.05–0.30 units from a wire segment, height 0.125/0.15, layer 0.
* 3035 pure-digit TEXT entities exist across the set; 66 % of them are within 1.0 of a wire
  candidate. The rest are zone labels (left margin column `200`,`201`,… and a right-hand column
  `260`,`261`,… at 0.15 height), title-block numbers, BOM quantities (047/051: 248/201 pure-digit
  texts, 0.19 above the row rules — these are quantities, not wires), and table/index numbers.

### 4.3 What makes two wire segments one net — measured gaps

Endpoint coincidence is **exact** where ACADE joined wires: corner joins are 0.000 gaps (measured
merges at tol 1e-6: 002 13 merges among 117 candidate segments, 041 37/167, 044 32/61, 046 16/53,
022 38/94). Use **tol = 1e-6 (or 0.001 to be safe)** for the union — no need to use 0.05, because
every non-zero gap in this data is deliberate:

| measured gap | meaning | where | test |
|---|---|---|---|
| **0.000** | corner / same-net join | everywhere; many per sheet | endpoint distance ≤ 0.001 |
| **0.0625** | half-gap from a wire end to a `TERMINAL` centre (terminal circle radius) | 046 (wire starts at x=8.062, terminal at x=8.001), 041 | endpoint-to-terminal-centre 0.06–0.07 |
| **0.100** | full hop gap: `ARC` radius 0.05, semicircle, centred on the gap | 044 (11 ARCs on `WIRES`, r=0.05, 90°–270°) | arc centre ≈ gap midpoint, radius ≈ gap/2 |
| **0.125** | full hop gap: `ARC` radius 0.0625, semicircle | 002 (27), 003 (15), 007–013 (1 each) | arc centre ≈ gap midpoint, r≈0.0625 |
| **0.125** | same-size gap with **no** terminal and **no** hop arc — component-pin or symbol-boundary gap; keep nets apart | 020 (7), 043 (12), 046 (several) | no terminal/arc at midpoint |
| **0.125** | pass-through `TERMINAL`: two wires end on opposite sides of one terminal insert | 025 (8), 041 (25), 042 (17) | `TERMINAL` insert within 0.09 of the gap midpoint |
| **0.125** | contact/symbol pin gap: a wire stops 0.125 short of a relay/contact pin (the device itself is the boundary, so the nets must stay separate) | 046 (`CONT-NO` pins at ±0.4375 from insert origin; wire starts 0.125 beyond the pin) | no terminal/arc at midpoint → **do not merge** |

Verified example (002): a vertical wire runs at x=19.000 from y=17.25 to 1.974. The horizontal wire
at y=15.000 is cut at 18.937–19.062 and a semicircular `ARC` of radius 0.0625 centred at (19.000,
15.000) spans 0°–180°. This is a **wire hop**, a crossing that must not connect the two nets.
The same structure appears on 044 with r=0.05 and 90°–270° (vertical hop over horizontal wire).

**T-junctions (junction dots).** `DOT`/`WDDOT` inserts mark a wire endpoint that lands on another
wire's interior (not on its endpoint). Measured: 002 11 of 12 junction dots have exactly 1 wire
endpoint at the dot centre and 1 wire passing through the interior; 022 16/16; 020 12/13; 015 has
2–3 endpoints at dots. So the net test additionally needs **endpoint-to-segment-interior distance
≤ 0.02 units**.

**Net sizes after these rules** (wire candidates = `WIRES` layer if present, else layer-0 lines
≥ 0.5; terminal bridge as above; hop gaps skipped):

| sheet | segments | nets | multi-segment nets | largest |
|---|---:|---:|---:|---:|
| 002 | 117 | 106 | 6 | 4 |
| 003 (WIRES) | 8 | 5 | 3 | 2 |
| 015 | 76 | 58 | 10 | 4 |
| 020 | 95 | 62 | 18 | 4 |
| 022 | 94 | 61 | 19 | 5 |
| 025 | 158 | 129 | 17 | 8 |
| 041 | 167 | 110 | 41 | 4 |
| 042 (WIRES) | 14 | 10 | 2 | 3 |
| 043 | 79 | 58 | 7 | 4 |
| 044 (WIRES) | 61 | 38 | 14 | 3 |
| 045 | 66 | 48 | 10 | 4 |
| 046 | 53 | 39 | 10 | 4 |

Most nets are 1–4 segments; that is normal for point-to-point ACADE wiring. The wire graph itself
does **not** tell you which device pin a net belongs to — that needs the symbol association pass (§9).

---

## 5. TERMINALS — what the `TERMINAL` inserts are, and what is missing

* `TERMINAL` block definition: one `CIRCLE` r≈0.054 + one short `LINE`; bbox 0.108×0.062; base
  (0,0); **no ATTDEFs**. 111 inserts, on 6 sheets: 022 (2), 025 (8), 041 (31), 042 (32), 043 (6),
  046 (32). Every one has **0 ATTRIB children**.
* `BOX` block: 4-line rectangle 0.625×0.250; 175 inserts on 19 sheets; also 0 ATTRIBs. Used as
  multi-pole terminal-block graphics (e.g. 002: three `BOX` at x=25.188, y=14.625/15.000/15.250 where
  three wires end).
* Geometry: a `TERMINAL` sits at a wire end, with the wire stopping 0.0625 from its centre (the
  circle radius). Example 046: wire `(8.062,15.500)-(16.500,15.500)`, terminal at `(8.001,15.500)`.
* **Strip designation and terminal number are not represented** in these files.
  * Searched all 6674 TEXT values: no `-X1:1`-style designator, no text containing `:` outside the
    title block (`CHECKED BY:` etc.), no `TERM`-layer text.
  * The only terminal-ish labels that exist are hand text: `TB3` on 007–014 (I/O terminal block
    table label, height 0.125, layer 0), `TS-1 TERMINAL STRIP` … `TS-4 TERMINAL STRIP` on 031 (panel
    layout), `X1`–`X4` on 020 (hand-drawn terminal block), `S10`–`S21`/`A1`/`A2` on 025 (layer
    `TEXT`), and part descriptions on the BOM sheets 047/051 (`TERMINAL 2 CONDUCTOR GREY`,
    `TERMINAL JUMPER 4 WAY`).
  * The numbers placed near terminals (041: `41040`, `41060`, …; 042: `42040`, …; 046: `22280`,
    `41640`, …) are **wire numbers**, not terminal numbers.
* 045 is the smoking gun for the loss: its 190 detached ATTRIBs include 12 on layer `TERMS` with
  tags `TERM01`/`TERM02`, plus the `X*TERM` invisible attributes — i.e. terminal numbers existed as
  attributes, and in 045 they were detached and emptied.

Practical consequence: a terminal can be counted (286 terminal-graphic inserts), and it can be
associated with the adjacent wire/net, but it cannot be named. Any "terminal strip report" must ask
the user for the strip designation, or infer it from the hand labels (`TB3`, `TS-1`…).

---

## 6. CROSS-REFERENCES — present in the schema, empty in the data

* `WD_M` defines `XREF_LAY=XREF`, `CXREF_LAY=XREFCHILD`, `XREFFMT=%N`, `ALT_XREFFMT=%N`,
  `XREF_STYLE=0`, `XREF_GRAPHIC=0`, `XREF_TBLFLDNAMS=W1,T1,TYPE,T2,W2,REF,SH,SHDWGNAM,FILENAME,FULLFILENAME`.
* Native sheets 003/042/044 define `XREF`/`XREFCHILD` layers and carry `XREF`/`XREFNC`/`XREFNO`
  attributes — **every value is empty** (003: 3 `XREF` attributes, all on layer `XREFCHILD`;
  042: 5 XREF-family attributes; 044: 13). No TEXT exists on `XREF`/`XREFCHILD` on any sheet.
* 045 has 13 detached ATTRIBs on layer `XREF`, all empty.
* The only cross-reference-like text is manual annotation: `CONTINUED ON SHEET #3` and `SHEET #20`
  (002), `FROM SHEET 49` (050), `SLOT 1 (LOCAL:1)` / `SLOT 2 (LOCAL:2)` / `SLOT 4` / `SLOT 5` (041/042
  PLC slots), and the unassigned `XXXXX`/`XXXXCR` placeholders (046). None encodes sheet **and**
  zone; the `%X` zone part of `XREFFMT` is not present at all.
* So: **sheet+zone cross-references cannot be recovered from these files.** The format can only be
  stated from `WD_M` (`%N` = sheet number only). A real xref report requires the original DWG/ACADE
  project (or the DWGs converted with ODA, since they are a different revision — see §7).

---

## 7. UNITS AND SCALE — measured extent, text heights, and the A3 claim

### 7.1 Measured model-space extent

The sheet frame is an LWPOLYLINE rectangle at `(0.4876,0.5373)–(35.4876,23.5373)` = **35.0 x 23.0
drawing units** on every sheet. Model-space content reaches **35.5 x 23.51** on 31 sheets (the extra
0.5 is the title-block/frame line-work that runs to 35.5 and 23.51).

Deviations (all are stray entities, not bigger drawings):

| sheet | measured bbox | what extends it |
|---|---|---|
| 020, 022 | (0, −13.0)–(35.5, 23.51) | one stray `WD_M` insert at (3.06,−13.0) |
| 025, 027 | (−11.125, −8.9375)–(35.5, 23.51) | two stray `WD_M` inserts at (−11.12,−8.94) each |
| 037 | (0, −7.55)–(43.47, 23.51) | one stray `ARC` at (43.5,−7.6) |
| 042 | (−1.19, 0)–(35.51, 23.51) | one stray `*X40` insert at (−1.19,2.9) |
| 035 | (0,0)–(35.51, 25.03) | one `MTEXT` at y=25.0 |
| 049 | (0,0)–(35.5, 24.94) | two stray `*X26`/`*X27` inserts at y=24.9 |

There is **no second copy of any sheet**: the off-frame entities are only stray `WD_M` copies, empty
`TEXT`s and anonymous hatch fragments. That matters — a net collector can safely clip to
0 ≤ x ≤ 35.6, 0 ≤ y ≤ 23.6 and lose nothing but junk.

### 7.2 Text and symbol sizes

* 6674 TEXT total; 5828 non-title. Heights: **0.125 → 3653**, **0.15 → 2135**, 0.08 → 348 (title
  block labels), 0.0938 → 114, 0.25 → 75, 0.1875 → 57, 0.1 → 54, 0.0234 → 40, 0.3 → 36, 0.13 → 34,
  0.1128 → 34, 0.075 → 20, 0.0312 → 18, remainder < 20 each. Non-title heights: 0.125 → 3619,
  0.15 → 1833, 0.25 → 75, 0.1875 → 57. This reproduces the prior report's 3653/2135/75 exactly.
* Symbol sizes (block geometry): `CONT-NO`/`CONT-NC` 0.875×0.25; `COIL` 1.0×0.008 core + circle
  r=0.25; `ELCB` 0.375×0.5; `PLUG` 0.219×0.25; `PB-NO` 0.875×0.219; `PB-NC` 0.875×0.094;
  `TERMINAL` 0.108×0.062; `BOX` 0.625×0.25; `DOT` circle r=0.05; `TY` circles r=0.019–0.062;
  `GROUND` 0.312×0.375; native master symbols `HCR1` 1.559×1.078, `HCR21` 0.25×1.195,
  `HMS21` 0.25×1.217, `HDV1TFL` 0.688×1.353, `HPX11` 0.5×1.31, `HMO13` 0.568×1.278.
  Rule of thumb: a device symbol occupies 0.1–1.6 units; a whole sheet is 35×23.

### 7.3 The claim "~35.5 × 25.1 units per A3, text 0.125"

**Confirmed:** width 35.5 units; text 0.125 dominant (3653 occurrences).
**Corrected:** the height is **23.5 units, not 25.1**. 25.1 is not in the data — it is exactly
`297 / (420/35.5)` i.e. the A3 proportion computed from width, not a measured extent. The frame is
35.0×23.0 units — aspect 1.522, whereas A3 is 1.414. So the sheet is **not A3-proportioned**:

* if the width 420 mm ↔ 35.5 units, then 1 unit = 11.83 mm and the frame height 23.0 units =
  272 mm, not 297 mm;
* if the frame height 297 mm ↔ 23.0 units, then 1 unit = 12.91 mm and the width 35.0 units =
  452 mm, not 420 mm;
* the only pair that makes the frame 420×276 mm is 1 unit = 12.0 mm exactly (35×12=420, 23×12=276),
  which would be A3 width with the 21 mm of A3's extra height split as top/bottom margins.

Neither the DXF nor the title block records the plot scale: `$INSUNITS=0` (unitless),
`$MEASUREMENT=1` (metric flag), `$LTSCALE=1.0`, `$TEXTSIZE=2.5` (unused default), no `$EXTMIN`/
`$EXTMAX`, and the title block `SCALE:` field is **empty on all 34 sheets** (only the label exists).
The DWGs might carry the real paper setup, but they are unreadable here.

**Recommendation for the code:** treat drawing units as abstract. Do not convert to mm and do not
hard-code an A3 frame. Use the measured unit tolerances in §9 (0.0625/0.125/0.25) and take any
print scale from the user or from a future DWG/ACADE source.

---

## 8. EXCEPTIONS / data-quality findings

### 8.1 `WD_M` — the true number

* **34 of 34 sheets carry `WD_M` inserts with attributes.** 68 inserts total, all with exactly **77
  ATTRIBs**; all 68 are identical (0 disagreeing attribute values across 68 inserts), owner
  `*Model_Space`.
* Distribution: **28 sheets × 2 inserts**, **025 and 027 × 4 inserts**, **042, 044, 045, 047 × 1
  insert** (28·2 + 2·4 + 4·1 = 68).
* **No sheet lacks `WD_M` attributes.** The earlier "2 of 34 lacked them" claim is unsupported. The
  most likely origin of that disagreement is an earlier pass counting sheets with two inserts: 30
  sheets carry two, the other four carry one. (A reader that de-duplicates per sheet sees 34/34.)
* 11 of the 77 attributes are empty on every insert: `DATUMX`, `DATUMY`, `SHEET`, `SHEETDWGNAME`,
  `IEC_LOC`, `IEC_INST`, `IEC_PROJ`, `MISC_DATA`, `SORTMODE`, `WNUM_GAP`, `XREF_FILLWITH`.
* Settings that matter to the app: `TAGMODE=R`, `WIREMODE=R` (existing references are fixed — do not
  renumber), `TAGFMT=%F%N`, `WIREFMT=%N`, `XREFFMT=%N`, `TAG_START=1`, `WIRE_START=1`,
  `UNIT_SCL=1.0`, `FEATURE_SCL=1.0`, `PLC_STYLE=1`, `XREF_STYLE=0`, `XREF_GRAPHIC=0`,
  `RUNGDIST=0.75`, `DISTH=2.0`, `DISTV=2.0`, and the layer names
  `TAGS/WIRES/SYMS/TERMS/DESC/XREF/LOC/MISC/WIRENO`.

### 8.2 DXF versions

**16 sheets are AC1027** (AutoCAD 2013): 002, 004, 007, 008, 009, 010, 011, 012, 013, 014, 015,
020, 022, 025, 027, 031. **18 sheets are AC1032** (AutoCAD 2018+): 001, 003, 032, 035, 036, 037, 038,
040, 041, 042, 043, 044, 045, 046, 047, 049, 050, 051.

### 8.3 Sheets that are structurally different

* **003, 042, 044** — the only native ACADE sheets: layers `SYMS/WIRES/TAGS/DESC/TERMS/LOC/XREF/
  DESCCHILD/XREFCHILD`, real symbol blocks with ATTDEFs and attached ATTRIBs. 003 is a hybrid: it
  also has 6 exploded fuse-block tags on layer 0.
* **045** — exploded with the attributes detached: 190 orphan `ATTRIB` entities (all values empty),
  20 of 22 inserts without a `330` owner handle, duplicated block definitions (`DOT_COPY`,
  `Plug_COPY`, `BOX_COPY`, `WDDOT_COPY`, `HDV1TFL_COPY`, `_X19_COPY`), and one insert of
  `ARYA_MODULE_ACT050_42L_S_6BE` (this app's own block-naming pattern, layer `SYMBOLS`, scale 0.075).
  Treat 045 as evidence that the files were touched by an earlier importer/session; its original
  component attributes are lost.
* **025, 027** — 4 × `WD_M` each; content extends off-frame only by those copies (no second drawing).
* **020, 022** — one stray `WD_M` at (3.06,−13.0) each.
* **047, 051** — BOM text sheets (`ELECTRICAL BILL OF MATERIAL`, `PNEUMATIC BILL OF MATERIAL`):
  8/16 inserts, mostly empty `MODULE`/`WEB MODULE` blocks at (0,0); 509/286 TEXT (part descriptions
  and quantities). They contain **no device tags** but are the only source of catalog descriptions.
* **001** — sheet index: 186 TEXT listing all sheets and titles; 3 inserts, 2 of them `WD_M`.
* **040** — hand-drawn PLC power-supply sheet: 46 TEXT, wire numbers `4050`–`4090`, text
  `ROCKWELL PROCESSOR`, `5069-L320ER`, plus 2 `RJ45` and 2 `PT` inserts. No component tags.
* **004** — spare power-distribution sheet, only 1 tag `2009PDB`, no symbol inserts.
* **031/032/035/036/037/038/049/050** — panel/mechanical/pneumatic layouts: 4–34 inserts, thousands
  of LINE/ARC/CIRCLE for enclosure art; tags exist on 031/032/035/036/049/050 but there are no ACADE
  symbol inserts for them.

### 8.4 Owner handles (a latent importer bug)

34 of 999 inserts have **no `330` owner handle**: 027 (2/33), 040 (2/6), 042 (10/88), 045 (20/22).
The 24 attributed components are all owned (`*Model_Space`), so today's `components_of()` still finds
them; but `components_of()` filters `owner_handle != model_space` — any future insert without a 330
(on these files or another export) is silently dropped. Many non-INSERT entities are ownerless too
(031: 1008, 042: 156, 041: 170, 045: 333). Recommendation: when the owner is absent, fall back to
the paper-space flag `67==0` rather than discarding.

### 8.5 Symbol definitions with phantom geometry

Several ACADE symbol definitions embed a nested anonymous `*X` insert placed 3–4 units away from the
symbol origin: `DOT` (nested `*X9` at (−3.25,−1.15)), `SURGE` (2 × `*X13` at −2.812/−3.688),
`24VDCPRX` (2 nested), `PRESS-SW` (1 nested). If the importer flattens block definitions without
clipping to the symbol's visible geometry (the repo's `harvest_library.py` does flatten), it will
harvest phantom strokes far from the symbol. This also explains why naive block bboxes look absurd
(e.g. DOT 3.25×1.15 instead of r=0.05).

### 8.6 DWG / ACD

* 33 DWGs, names `20136-0xx.dwg`, binary AC1032, 046 missing. They use a different (older) project
  number; they may be a different revision of the same sheets. No reader available → not measured.
  **If the authoritative tags/parts/terminals are needed, convert these DWGs with ODA File Converter
  and re-run; that is the single highest-value follow-up and it would also give the true `WD_M`
  per-sheet distribution at the source revision.**
* `FC1000495_AFR.ACD` is binary RSLogix 5000 V37.00.00, not ASCII text. No PLC module/tag table is
  recoverable with tools available here. Use Logix Designer → L5X export if PLC I/O tags are needed.

---

## 9. IMPLEMENTATION SPEC — what to build, ranked by expected yield

### 9.1 Ranked heuristics

**H1 — keep/repair the attribute path (yield 24 components on 3 sheets; ~10 lines).**
For every model-space INSERT: if it has ATTRIB children and block != `WD_M`, make a component.
Map attributes:
`TAG1`/`TAG2`/`TAG`/`NAME`/`TAGSTR` → tag; `DESC1..DESC3`/`DESC`/`DESCRIPTION` → description
(join with space or keep lines); `CAT`/`PART`/`PARTNUMBER`/`CATALOG` → part; `MFG` → manufacturer;
`LOC`/`INST` → location; `TERM01..TERM04`/`X*TERM*` → terminals; `XREF`/`XREFNC`/`XREFNO` → xref
(all empty today; read if ever filled). Skip `WD_M`. Dedupe rows whose non-empty tag is identical
within the sheet (multi-part devices). This preserves today's 24 components and will immediately
recover more the moment a native sheet is added.

**H2 — TEXT tag scan (the main win: +212 tag instances, 28 sheets).**
Classify every `TEXT` in model space:
* skip `layer == TITLE_BLOCK` (846 texts) and paper space;
* skip if the effective string is empty;
* accept if matches `^\d{3,5}[A-Z]{2,4}$` with suffix not in
  `{VAC,VDC,AMP,HZ,AWG,WATT}`, or `^[A-Z]{2,4}\d{1,4}$` with prefix not `IN/OUT/CH`, or
  `^X{3,}[A-Z]*$` (unassigned);
* reject pure digits, `L1/L2/L3`, `\d+[A-Z]\d+` (phase suffixes), `#…AWG`, ratings/voltages
  (`120VAC`, `24VDC`, `+24VDC`, `60HZ`, `20AMP`, `20A`, `2.2kW`, `480W`), IP addresses, anything
  containing `-` (part numbers), `TYP.`, `%%U…`, `SHEET #…`, and text whose height is outside
  `[0.09, 0.26]` (keeps title/footnote text out).
Merge with H1 by tag string per sheet. Expected yields: 203 distinct TEXT tags + 17 attr tags = 218
per-sheet distinct tags on 28/34 sheets; 231 tag-bearing instances.

**H3 — description / part association (secondary, noisy: use for BOM only with a confidence flag).**
For each tag, search `TEXT` within 2.0 units (prefer 0.2–1.5): skip other tags, pure digits, phase
labels, wire specs. Take the nearest; if it is a word/description-shaped string, set `description`.
Part numbers can be suggested by `^[A-Z0-9][A-Z0-9./-]{3,}$` with at least one digit and a letter
(`LPJ-100SP`, `FAZ-C1/1-NA-SP`, `ITV3030-04N4BL4`), but do not claim them as authoritative: only a
minority of tags has one, and the same text can be a note. On native sheets read `CAT`/`MFG` instead
(they are empty here). Measured anchor: 205/212 tags have a non-tag TEXT within 2.0; 145/212 within
1.0. Prefer a description **below** the tag (native DESC1 sits at dy=−0.45 for most blocks).

**H4 — wire/net collector (yield: recoverable nets on ~24 schematic sheets).**
1. Wire candidates: if the sheet has `line` entities on layer `WIRES`, use those (003/042/044/045);
   otherwise use `LINE` on layer 0 with length ≥ 0.50 (schematics only; skip 031/032/035/036/037/
   038/047/049/050/051 mechanical/BOM/pneumatic sheets by title/entity mix). Split `LWPOLYLINE`
   into segments; include `ARC` only as hop evidence, never as a wire.
2. Union endpoints with tol **0.001** (exact joins).
3. T-junction: if an endpoint lies within **0.02** of another segment's interior, union.
4. Terminal pass-through: for two endpoints with gap ≤ **0.14**, if a `TERMINAL` insert lies within
   **0.09** of the midpoint, union.
5. Hop reject: for two endpoints with gap ≤ **0.14**, if an `ARC` with `|radius − gap/2| ≤ 0.012`
   and centre within 0.06 of the midpoint exists, do **not** union.
6. Symbol pin gap: gaps of 0.125 with no terminal/arc are device boundaries; leave the nets apart.
   Associate each net endpoint with the nearest component insert/pin (within 0.2) afterwards.
7. Net id can be the wire-number text attached to the net (nearest pure-digit TEXT within 0.30 of
   any segment of the net, height 0.125/0.15); a net may have several wire numbers (3-phase).

**H5 — terminal inventory (yield 286 graphics, no names).**
Count `TERMINAL` and `BOX` inserts as terminal graphics. Attach the nearest wire-number TEXT
(0.0625–0.35 away) as the terminal's only identifier. Report `strip=""`, `number=""` and mark
`synthetic=true`. Do not try to reconstruct `-X1:1`; it is not in the data.

**H6 — symbol inventory for class inference (yield 170 symbol inserts).**
Classify non-attribute symbol inserts by block name (table in §2) to infer `class` for BOM
(`CONT-NO`→Contactor, `COIL`→Relay coil, `ELCB`→Circuit breaker, `PB-*`→Pushbutton,
`24VDCPRX`→Proximity sensor, `SURGE`→Surge suppressor, `SOL`/`SV`→Solenoid valve,
`HDV1TFL`→Variable frequency drive, etc.). Family letters in the tag are a second signal
(`CB`, `CR`, `PB`, `PRX`, `CON`, `SV`, `DRV`, `FB`, `PDB`, `NF`, `PS`, `SCR`, `SR`, `TR`, `LT`).

**H7 — exclusion list (must).**
Never emit components for: `WD_M`; `DOT`, `WDDOT`, `GROUND`, `ELGND`, `TY`, `JUMPER`, `ARROW-L`,
`ARROW-S`, `MUFFLER`, `CYL-RET`, `PL`, `PT`, `BOX`; `*X*`, `*D*`, `A$C*`, `*_COPY`; `SI`; empty
`MODULE`/`WEB MODULE`; layer `TITLE_BLOCK`; paper space; pure-digit text; `XXXXX` is a placeholder
(emit with empty tag, flagged unassigned, not dropped).

**H8 — owner fallback (robustness).**
`components_of()` currently requires `owner_handle == model_space`. 34/999 inserts have no 330
owner; use `67 == 0` (model space) as a fallback so future exports do not lose components.

### 9.2 Reference pseudo-code

```rust
let mut comps = Vec::new();
for e in doc.entities() {
    if e.space != Space::Model { continue; }
    match e.kind {
        Insert { block, attrs, .. } if !block.eq_ignore_ascii_case("WD_M") => {
            if !attrs.is_empty() {
                comps.push(component_from_attrs(block, attrs));   // H1
            }
        }
        _ => {}
    }
}
for t in doc.texts() {
    if t.layer == "TITLE_BLOCK" || t.space != Space::Model { continue; }
    let s = t.value.trim();
    if is_tag(s) { comps.push(Component { tag: s, .. }); }        // H2
}
dedupe_by_tag_per_sheet(&mut comps);                               // multi-part devices
```

`is_tag`:
```rust
fn is_tag(s: &str) -> bool {
    let bytes = s.as_bytes();
    let num_first = regex_like_number_first(s) && !["VAC","VDC","AMP","HZ","AWG"].contains(&s[digits..]);
    let fam_first = regex_like_family_first(s) && !s.starts_with("IN") && !s.starts_with("OUT")
                   && !s.starts_with("CH");
    num_first || fam_first || s.chars().all(|c| c == 'X' || c.is_ascii_uppercase())
        && s.starts_with("XXX")
}
```

(Write the two regexes by hand or with a small crate; the alphabet is ASCII only in all 22 753
TEXT values.)

### 9.3 Tolerances to implement (drawing units)

| purpose | tolerance |
|---|---|
| wire endpoint join (same net) | 0.001 (data is exact; 1e-6 also works) |
| endpoint on another wire's interior (T-junction dot) | 0.02 |
| wire end ↔ `TERMINAL` centre | 0.06–0.07 (bridge if gap ≤0.14 and terminal at midpoint ±0.09) |
| hop detection | `abs(arc.radius − gap/2) ≤ 0.012`, arc centre within 0.06 of gap midpoint |
| symbol pin gap (do not merge) | 0.125 ± 0.02, no terminal/arc present |
| tag TEXT ↔ symbol insert (mixed sheets) | 0.50 preferred, 1.0 fallback (measured 041 14/15 ≤0.75; 025 6/6 =0.21; 035 3/3 =0.25–0.27; 036 4/4 =0.50; 044 2/2 =0.40) |
| tag ↔ description text | 2.0 (prefer 0.2–1.5; 91/212 within 0.5) |
| wire number ↔ wire | 0.30 |
| sheet clip | 0 ≤ x ≤ 35.6, 0 ≤ y ≤ 23.6 (drops stray off-frame `WD_M`s, empty texts, anon hatches) |
| text height of a tag | 0.09–0.26 |

### 9.4 What will still be wrong / needs a human

1. **Terminals are unnamed.** No strip/number in any file. A user must supply the strip mapping, or
   accept wire numbers as terminal identifiers.
2. **045 loses its components.** 190 empty detached ATTRIBs; only 2 text tags survive. Nothing can
   recover the lost tags from this DXF; the DWG for 045 (20136-045) may have them if converted.
3. **Cross-references are empty.** No sheet/zone data exists; a cross-reference feature cannot be
   honest on this data.
4. **Multi-part devices** (003 `M326` ×3, 042 `4220CON` attr+text, 046 placeholders) are ambiguous:
   count devices by distinct tag (recommended) or count parts (needs a part index that is not in the
   DXF). Document the choice.
5. **Tag-vs-label ambiguity in a few families**: `BR1`/`BR2` (007–014) label a hand-drawn bridge
   rectifier, `TB3` labels an I/O terminal table — they behave like ACADE tags but are not symbols.
   `MPE1xx/2xx` (027) label Ethernet jacks in a layout drawing. All are "tagged items", not
   "attributed symbol inserts"; classify them as `tagged, no symbol`.
6. **Class inference from family letters is a guess** for `SR`, `PE`, `MDS`, `PDB`, `FB`, `NF`, `PS`
   (`SR` may be a "slip ring" on 015/043/045 — the text next to `1522SR` reads `SLIP RING`; `PDB`
   panel distribution block; `FB` fuse block; `NF` noise filter; `MDS` motor disconnect switch).
   Map only where a description near the tag confirms; otherwise emit the family as `class_hint`.
7. **Part/BOM data is thin.** Only the BOM sheets (047/051) and a handful of free texts carry part
   descriptions; native `CAT`/`MFG` attributes are empty. Do not promise a BOM from tags alone.
8. **Mechanical/layout sheets** (031/032/035/036/037/038/049/050) have tags for physical devices but
   no ACADE symbols; a "components_of" that scans only symbols will report nothing there. The
   tag-only pass is what makes those sheets yield (31: 38 tags, 050: 10, 049: 4, 035: 3, 036: 4,
   032: 1). This is probably desired for a panel-layout BOM, but label the source as "text tag only".
9. **Wire threshold 0.5 is a heuristic.** On dense schematics a few short wires (< 0.5) will be
   missed; lowering to 0.35 recovers them but adds symbol strokes. If net counts are used for
   renumbering, make the threshold configurable.
10. **DWG/ACD not read.** The 33 DWGs (different project number, 046 missing) and the binary ACD
    remain unmeasured. Converting them is the only way to check whether the flattened DXF lost data
    relative to the source.

---

## Appendix A — per-sheet tag lists (distinct strings recovered)

`A:` = attribute tag values; `D:` = number-first TEXT; `L:` = family-first TEXT; `P:` = placeholder.

* **001** — (none; index sheet)
* **002** — D: `203PDB`, `207PDB`, `261PDB`, `208CB`, `212FB`, `218FB`, `224CB`, `264CB`, `270CB`,
  `276CB`, `282CB`, `4214CON`, `4216CON`
* **003** — A: `HYD. UNIT`, `M326`; D: `261PDB`, `302FB`, `308FB`, `314FB`, `320FB`, `326FB`
* **004** — D: `2009PDB`
* **007** — D: `2521CR`, `2525CR`; L: `BR1`, `BR2`, `TB3`
* **008** — same as 007 (`2521CR`, `2525CR`; `BR1`, `BR2`, `TB3`)
* **009** — same as 007
* **010** — same as 007
* **011** — D: `2523CR`, `2527CR`; L: `BR1`, `BR2`, `TB3`
* **012** — same as 011
* **013** — same as 011
* **014** — same as 011
* **015** — D: `1522SR`, `1572SR` (next to `SLIP RING` text)
* **020** — D: `2007CB`, `2012CB`, `2015CB`, `2018CB`, `2020NF`, `2021CB`, `2024CB`, `2028CB`,
  `4208CR`
* **022** — D: `2202PS`, `2210CB`, `2220CB`, `2223CB`, `2223NF`, `2226CB`, `2517CR`, `2519CR`
* **025** — D: `2517CR`, `2519CR`, `2521CR`, `2523CR`, `2525CR`, `2527CR`
* **027** — L: `MPE102`, `MPE103`, `MPE104`, `MPE105`, `MPE202`, `MPE203`, `MPE204`, `MPE205`,
  `MPE206`, `MPE208`
* **031** — D (38): `1000MDS`, `1002DRV`, `1102DRV`, `1202DRV`, `1302DRV`, `1402DRV`, `1512SCR`,
  `1562SCR`, `2007CB`, `2012CB`, `2015CB`, `2018CB`, `2020NF`, `2021CB`, `2024CB`, `207PDB`, `208CB`,
  `214CB`, `220CB`, `2210CB`, `2217CB`, `2220CB`, `2223CB`, `2223NF`, `2226CB`, `224CB`, `226CB`,
  `232CB`, `261PDB`, `2703SR`, `302CB`, `308CB`, `3100PS`, `314CB`, `320CB`, `702DRV`, `802DRV`,
  `902DRV`
* **032** — D: `2402TR`
* **035** — D: `4104PB`, `4118PB`, `4120PB`
* **036** — D: `4110PB`, `4112PB`, `4114PB`, `4116PB`
* **037** — (none)
* **038** — (none)
* **040** — (none; hand-drawn PLC labels only)
* **041** — D: `4104PB`, `4108PS`, `4110PB`, `4112PB`, `4114PB`, `4116PB`, `4118PB`, `4120PB`,
  `4122PB`, `4124PRX`, `4126PRX`, `4128PRX`, `4130PRX`, `4154PRX`, `4164PE`
* **042** — A: `M326`, `START PB`, `4220CON`, `4270CON`; D (22): `4204LT`, `4206CR`, `4208CR`,
  `4210SCR`, `4212SCR`, `4214CON`, `4216CON`, `4218CON`, `4220CON`, `4222CON`, `4254SVA`, `4256SVB`,
  `4258SVA`, `4260SVB`, `4262SV`, `4264SV`, `4266SV`, `4268SV`, `4270CON`, `4272CON`, `4412CON`,
  `4438CR` (after dedupe with A: **24 distinct**)
* **043** — D: `1522SR`, `1572SR`
* **044** — A: `BOT`, `SM1`, `SM2`, `CR.MS`, `MAIN_SOL`, `CT1`…`CT6`; D: `2517CR`, `2519CR`
* **045** — D: `1522SR`, `1572SR` (its 190 detached ATTRIBs are all empty)
* **046** — D: `4218CR`, `4220CR`, `4222CR`; P: `XXXXX`, `XXXXCR`
* **047** — (none; BOM text)
* **049** — D: `41080PS`, `4124PRX`, `4126PRX`, `4266SV` (last also on layer `TUBING_TAGS`)
* **050** — D (layer `TUBING_TAGS`): `4128PRX`, `4130PRX`, `4132PRX`, `4134PRX`, `4156PRX`,
  `4158PRX`, `4254SV`, `4256SV`, `4258SV`, `4260SV`
* **051** — (none; BOM text)

## Appendix B — measurement artifacts (so the Lead can sanity-check)

* All tag/wire/terminal counts exclude paper space and layer `TITLE_BLOCK`.
* A "distinct" tag is the exact string after `trim()`; no case folding is applied (all are
  uppercase in this data).
* Wire-candidate definition used for the measurements: layer `WIRES` if the sheet has it, otherwise
  `LINE` on layer 0 with length ≥ 0.5 (for the 0.35-based length distribution see §4.1).
* Net counts in §4.3 are union-find results with exact-endpoint joins + T-junction (0.02) +
  terminal bridges; hop gaps are skipped; no symbol pin assignment is included.
* "Text anchor" = group code 11/21 when a justification (72/73) is set, otherwise 10/20. Using 10/20
  naively shifts tags by up to half their width; e.g. `264CB` code-10 x=20.258, true anchor x=20.562.
* The per-sheet INSERT/TEXT/LINE counts come from the ENTITIES section only; block definitions are
  counted separately in §2. A block can be defined without being inserted (`BOX_COPY`, `WDDOT_COPY`,
  `HDV1TFL_COPY`, `_X19_COPY`, `*D*`) — these are still listed with 0 inserts.
