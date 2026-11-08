# ECAD feature matrix — the SkyCAD-style table against what we ship

The request was "everything in Advance" from a feature table with three columns
(STANDARD / ADVANCED / PRO). This page maps every row of that table to the
command that implements it, and to the check that proves it still works. It is
the audit the handoff has been asking for since Round 30.

Verification legend:

| Mark | Meaning |
| --- | --- |
| **S** | `py -3.13 tests\ecad\run_smoke.py` — the drafting/library suite (127 checks) |
| **F** | `py -3.13 tests\ecad\run_features.py` — the feature suite added in this round (35 checks) |
| **U** | `cargo test --release -p elec` — 131 unit tests in the domain crate |
| **M** | Manual, GUI-only: the command opens a dialog, so a headless assertion would only prove the dialog opened |
| **—** | Not applicable / not implemented (see *Gaps*) |

## The table

| # | Feature (as printed in the table) | Tier | Command surface | Proof | Status |
| --- | --- | --- | --- | --- | --- |
| 1 | Automated numbering | all | `ELECNUMBER`, `ELECNUMBERPREVIEW`, `ELECPROJNUM [apply]`, `ELECSETUP` / `ELECSETUPEDIT` | S, F §9, U | done |
| 2 | BOMs and parts-lists management | all | `ELECBOM [out.csv]`, `ELECPART`, `ELECPARTS`, `ELECPARTLIST`, `ELECASSIGN`, `ELECREPORT` | S, `tools/parts_check.py` | done |
| 3 | Automated cross-references | all | `ELECXREF`, `ELECXREFWRITE` | F §1, U | done — labels now `-K1:1` / `-K1:2` |
| 4 | License sharing | all | — | — | not applicable: local desktop app, no licence server |
| 5 | No training required | all | `ELECHELP`, `ELECPALETTE`, `ELECSETUP` | M | UX claim, not a testable behaviour |
| 6 | PLCs and drives automatically generated | all | `ELECPLC`, `ELECPLCNEXT`, `ELECIOIMPORT` | F §3, §4 | done |
| 7 | Navigation tools | all | `ELECGOTO`, `ELECHIGHLIGHT`, `ELECXREF` | F §9 | done |
| 8 | Terminal strip management | all | `ELECTERMINALS`, `ELECTSIDES`, `strip:index` tags | F §2, U | done — counting fixed this round |
| 9 | Revision control and rollback | all | `ELECREV SNAPSHOT\|LIST\|COMPARE\|RESTORE` | F §8 | done — restore now reloads the open sheet |
| 10 | Print to PDF | all | `EXPORTPDF` (plot dialog) | M | GUI-only; interactive dialog |
| 11 | Panel layout design | adv | `ELECPANEL <out.dxf>`, `library/panel/footprints.tsv` | F §6 | done |
| 12 | Wire processing | adv | `ELECWIRE`, `ELECJUNCTIONS`, `ELECWIRELIST`, `ELECWIRENUMBER`, `ELECWIRELABELS` | S | done |
| 13 | Automated PLC drawings from I/O list | adv | `ELECIOIMPORT <xlsx> [x,y]` | F §4 | done |
| 14 | Cabling and cable schedules | adv | `ELECCABLE`, `ELECCABLEDRAW`, `ELECCABLELIST`, `ELECCABLEDEL` | F §5 | done |
| 15 | Built project modification control | adv | `ELECBUILT on\|off` | F §9 | done |
| 16 | Export to DWG, DXF and intelligent PDF | adv | `ELECEXPORT <folder> [dxf\|dwg]`, `EXPORTPDF` | S (DXF round trip), M (PDF) | DXF done; PDF GUI-only |
| 17 | Harness design | pro | — | — | not implemented |
| 18 | Configuration management | pro | `ELECVARIANT`, `ELECOPTION`, `ELECREV` | F §7, §8 | partial: project variants + snapshots, not full config management |
| 19 | Design by systems / sub-assemblies | pro | `ELECSYSTEM [LIST\|INSERT]`, `library/systems/*.tsv` | F §7 | done |

## What the symbol library carries

Both the control blocks and the construction symbols are part of the same
mechanism: `ELECLIB <folder>` loads a folder of DXFs plus its `library.tsv`
manifest, `ELECINSERT` places by block name, and the pins come from the block's
`X4TERMnn` / `X1TERMnn` attribute definitions.

* `library/starter` — 78 IEC/EN 60617 control blocks (contacts, coils, motors,
  drives, PLC modules, terminals, sensors, …) with 679 attribute tags.
* `library/nec` — 414 symbols: the same 78 control blocks with ANSI geometry,
  plus 336 NECA 100-2024 construction symbols named from the Standard's own text
  and grouped into palette classes (`NECA Wiring Methods`, `NECA Motor Controls`,
  `NECA Fire Alarm`, `NECA Power Distribution`, `NECA Ground Fault`, …).
* `library/panel/footprints.tsv`, `library/parts/parts.tsv`,
  `library/systems/*.tsv` — panel footprints, part catalogue, sub-assemblies.

Generation, when a symbol is not in a published set:

* `ELECMAKEMODULE <name> <channels> [SIDES=both] [PITCH=n] [CLASS=x] [AT=x,y]`
  writes an N-channel module (body, one lead per channel, invisible pin ATTDEFs,
  `TAG1`/`CAT`/`MFG`/`DESC1`) into the loaded library and places one instance.
* `py -3.13 tools/symbol_maker.py --out <folder> NAME:CHANNELS[:SIDES] …` does
  the same in bulk, offline, for a whole family of modules.

## Defects found and fixed by this audit

1. **Terminal reports counted a terminal once per pin.** Two terminal blocks with
   two pins each reported "1 strip(s), 6 terminal(s)" and listed the non-terminal
   contact four times. `ELECTERMINALS` now groups anchors by label before
   counting; `ELECTSIDES` merges the side reports of one terminal instead of
   emitting one entry per pin. (F §2, U)
2. **A renumbered project left open sheets stale.** `ELECPROJNUM apply` wrote the
   new tags to disk but the tab kept the old ones, so a following `QSAVE` was
   refused as an outside change and `ELECGOTO` could not find the new tag. The
   command now reloads every open sheet it rewrote and says so.
3. **Revision restore could not replace an open drawing**, because the tab holds
   the file's edit lease. Restore now stages through the Save path
   (`crate::io::replace_file_from_source`) and reloads the open tab
   (`reload_tab_from_disk`), reporting "Reloaded <name>.".
4. **Snapshot ids had to be typed exactly.** `ELECREV RESTORE 002` failed against
   snapshot `002-A`; `COMPARE` had the same defect. Both now share
   `find_revision`, which accepts the printed name or the bare order number.
5. **Cross-reference labels printed insertion coordinates** (`40,0`) when a part
   had no contact identifier — unstable after any drag. A part without an
   identifier is now referenced as `<tag>:<ordinal>` (`-K1:2`).

## Gaps, stated plainly

* **Harness design (PRO)** is not implemented: there is no harness model, no
  bundle/breakout representation and no harness report.
* **Configuration management (PRO)** exists only as project variants
  (`ELECVARIANT NEW|LIST|USE`, `ELECOPTION`) plus revision snapshots; a full
  option/configuration matrix with per-variant BOMs is not there.
* **License sharing** has no meaning for this application: it is a local
  desktop editor with no licence server.
* **Print to PDF** and the plot path open a dialog; they are exercised by hand
  and in the screenshots, not by the headless suites.
* **Intelligent PDF** means searchable text (already produced by the plot path);
  clickable cross-reference links are still missing.
