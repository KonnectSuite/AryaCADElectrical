# AryaCADStudio — Electrical CAD handoff

State of the work, what another machine needs to build it, and where the sharp edges are.
Written 2026-11-05. Repo has **14 commits**; one file is uncommitted (see *Current state*).

---

## 1. What this is

`AryaCADStudio` is a fork of **Open CAD Studio 2026.44** (Rust, `iced` 0.15-dev, wgpu,
~300k LOC). On top of it sits an **electrical CAD layer** modelled on SkyCAD and
AutoCAD Electrical: a symbol library, placement, tagging, wiring, terminals, cross
references, PLC I/O, project files and reports.

Two directories matter:

| Path | What it is |
|---|---|
| `C:\Users\HomeAdmin\AryaCode\KonnectSuite\AryaCADStudio` | the code (git repo, ~33 GB incl. `target/`) |
| `C:\Users\HomeAdmin\AryaCode\KonnectSuite\AyraCADElectrical` | the *data*: library, fixtures, real drawings, reports |

The data folder is not in the repo. Copy it separately.

```
AyraCADElectrical\
  library\        51 symbol DXFs + library.tsv (symbol, class, attributes, sheets, scale)
  tests\          ~70 fixture DXFs and command batches (see §6)
  import\Prints\DXF\   34 real sheets (FC1000495-xxx) - plot exports, no attributes
  reports\        ecad-verification.md + screenshots
  tools\          harvest_library.py, make_*_test.py, DXF readers
```

---

## 2. What must be installed to build

Verified on the machine this was written on:

| Component | Version used | Notes |
|---|---|---|
| **Rust** (`rustup`) | **1.99.1** stable | toolchain `stable-x86_64-pc-windows-msvc` |
| **cargo** | 1.99.1 | comes with rustup |
| **Visual Studio 2022 Build Tools** | MSVC **14.44.35207** | needs the *Desktop development with C++* workload |
| **Windows 10/11 SDK** | **10.0.26100.0** | the MSVC linker needs it |
| **Git** | any recent | the repo history is the only record of the work |
| **GPU driver** | DX12-capable | the app runs `[gpu] backend: dx12`; Intel HD 530 worked |

Install with:

```powershell
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e `
  --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then confirm:

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustc -vV    # host: x86_64-pc-windows-msvc
```

**Not needed:** Node/pnpm, Python, .NET SDK, Docker, WSL.
Python 3 *is* used by the helper scripts in `tools/` (harvesting symbols, reading DXFs
back for verification), but never by the build.

### Build

```powershell
cd C:\Users\HomeAdmin\AryaCode\KonnectSuite\AryaCADStudio
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release -j 4
```

- **Incremental: ~12 minutes.** **Clean: 35–40 minutes** (this crate is huge; be patient).
- Output: `target\release\OpenCADStudio.exe` (~150 MB).
- Run: `.\target\release\OpenCADStudio.exe` — add a drawing path to open one.

Two settings in the repo exist because this crate is at the edge of what the compiler
likes:

- `.cargo/config.toml` sets `RUST_MIN_STACK = 67108864` ("rustc 1.99 overflows the
  default 8 MB compiling this crate in release").
- `Cargo.toml` sets `[profile.dev.package.meshopt] debug = 0`. **With `debug = 1` every
  dev build fails** (MSVC `cl D8050: cannot execute c1xx.dll`). Release builds were never
  affected.

---

## 3. Current state (read this before building)

- Committed and working: everything through **`e3fe933cf` "ELECSHEET: drawing frame and
  title block"**.
- **Uncommitted:** `src/app/commands/elec.rs` — a rewrite of `ELECSHEET` so the frame fits
  the drawing's own extent instead of being drawn at 297×210 mm in units where the content
  is only ~35×25. `cargo check --release` passed at the time of writing; the full build was
  interrupted, so **this change is unverified**. Either finish it or `git checkout` it.

```
git status          # one modified file
git log --oneline   # 14 commits
```

---

## 4. The electrical commands

All live in `src/app/commands/elec.rs` (~4,200 lines) and dispatch through
`dispatch_elec`. Domain logic is in `crates/elec` (3,366 lines, dependency-free, **121 unit
tests**).

| Command | Does |
|---|---|
| `ELECLIB <folder>` | load a folder of symbol DXFs (+ `library.tsv`); remembers the path in `%APPDATA%\AryaCAD\elec_library.txt` |
| `ELECPALETTE` | modal symbol palette with drawn thumbnails |
| `ELECINSERT` / `ELECPLACE <symbol>` | place a library symbol, class-based tagging |
| `ELECSETATTR <tag> k=v,...` | edit attributes at the library's label height |
| `ELECWIRE` | polyline wire with junction dots |
| `ELECJUNCTIONS` | place junction dots where wires meet |
| `ELECTERMINALS`, `ELECTSIDES` | terminal strips, side A/B assignment |
| `ELECXREF`, `ELECXREFWRITE` | cross references between parts of one device |
| `ELECLIST`, `ELECBOM [csv]`, `ELECWIRELIST`, `ELECREPORT <xlsx>` | listings and reports |
| `ELECNUMBER` / `ELECNUMBERPREVIEW` | tag devices on the current drawing |
| `ELECNEW [template.dxf]` | new drawing, optional template |
| `ELECMAKESYMBOL <name>` | turn selected geometry into a library symbol |
| `ELECPROJECT <folder> [out.xlsx]` | scan every sheet, project BOM workbook |
| `ELECPROJNEW/OPEN/LIST` | project file (`project.eprj`, plain text) |
| `ELECPROJNUM [apply]` | renumber devices across the project (preview by default, `.dxf.bak` backup) |
| `ELECPLC <tag> <points> <base> [x,y]` | place a PLC module with `ADDR01..ADDRnn` |
| `ELECPLCNEXT <tag> <points> [prefix] [x,y]` | continue I/O addressing across sheets |
| `ELECIO` | per-point cross references (address → device on that conductor) |
| `ELECPANEL <out.dxf>` | panel arrangement drawing |
| `ELECSHEET [A4|A3|A2|A1] [title]` | frame + title block (see the caveat in §7) |

---

## 5. Project file format

`project.eprj`, plain text, relative paths, hand-editable:

```
# AryaCAD electrical project
sheet sheet-01.dxf
number 1
title Feeder panel
sheet sheet-02.dxf
number 2
title Motor starter
```

`number` and `title` belong to the `sheet` line above them. The project file is the single
source of truth for sheet membership and order — `ELECPROJECT` (no argument), `ELECPLCNEXT`
and `ELECPROJNUM` all read it.

---

## 6. How to verify anything (do not skip this)

The app is a GUI, but every command runs headless:

```powershell
$exe = "target\release\OpenCADStudio.exe"
& $exe --each tests\e2e_batch.txt tests\e2e_test.dxf
```

- `--each <batch.txt> <drawing.dxf>` runs one command per line, then exits.
- Blank line = Enter (ends the current interactive command).
- Point input is `x,y` on one line — **not** separate x and y lines (silently ignored).
- `--script <file> <drawing>` runs a script at GUI startup.
- Read results back **from the saved DXF**, not from the command's own output; the
  Python readers in `tests\` (`read_addrs.py`, `read_tags.py`, `read_panel.py`) do this.

Batches worth knowing: `e2e_batch.txt` (full draft + reports), `plc_batch.txt`,
`io6_batch.txt`, `pn3.txt` (project numbering), `meta2_batch.txt` (metadata + workbook),
`sheet_batch.txt`.

---

## 7. Known problems / sharp edges

1. **`ELECSHEET` scale (in progress).** Drawing frames at true paper size does not fit
   drawings whose content is ~35×25 units; symbols end up microscopic. The uncommitted
   change fits the frame to the drawing extent.
2. **`ELECPANEL` has no footprints** — fixed 14×10 boxes, no dimensions, no DIN rail.
   Ordering is alphabetical by tag.
3. **Library is thin** — 51 symbols, all authored symbols get class `Symbol` (`-S1`).
4. **Your exported sheets carry no component attributes** — they live only on `WD_M`, so
   project BOMs come out empty on them (24 components across 34 sheets).
5. **App-level tests not re-run since a source-corruption incident** (§8). `crates/elec`
   tests pass; `cargo test --release` for the app has not been verified end to end.
6. **Single-instance edit lock** — a second session cannot write a drawing the GUI has
   open, so headless batches fail on files the app is showing.

---

## 8. The incident that must not repeat

`src/app/commands/elec.rs` once reached **1.56 GB**, with a single **736-million-character
line**: a PowerShell `Get-Content`/`Set-Content` round trip had mojibake-doubled a doc
comment, and each later edit doubled it again. That one literal inflated the crate's
artifacts — the library rlib hit **4.65 GB with 4.19 GB of metadata**, over the 4 GiB
archive-member limit — and every binary failed to link with:

```
error[E0786]: corrupt metadata encountered in libOpenCADStudio-*.rlib
```

Hours were spent treating it as a compiler limit (`codegen-units`, `opt-level`,
pipelining, `-j 1`, full clean) before the real cause was found.

**Rules from that:**

- **Never** edit source files with PowerShell `Set-Content`, `-replace`, or `>`. Use the
  file-editing tool, or a Python script that writes bytes exactly once.
- If a file grows unexpectedly or a build fails with E0786, check the largest source file
  first: `Get-ChildItem src -Recurse -Include *.rs | Sort-Object Length -Descending | Select -First 5`.
- `src/lib.rs.disabled`, `src/main_lib_linked.rs.disabled` and
  `src/bin/ocs_launcher.rs.disabled` are leftovers from the workaround; the real crate
  layout (`lib.rs` + thin `main.rs` + `ocs_launcher`) is restored and should stay.

---

## 9. Where the code lives

```
crates/elec/src/            domain: catalogue, component, expr, geometry, junction,
                            netlist, numbering, report, sides, terminal, xref (+ tests)
src/app/commands/elec.rs    every ECAD command, library statics, project handling
src/modules/electrical/     ribbon tab, PlaceSymbolCommand, WireCommand
src/ui/window/elec_library.rs   palette modal with thumbnails
src/app/commands/mod.rs     command registry (add new names here)
src/app/command_driver/utilities.rs  argument-taking commands must be listed here
```

Gotchas that cost time already:

- Commands that take a path must be registered in the **early-argument list** in
  `command_driver/utilities.rs`, or they run twice / execute the argument as a command.
- Block contents are owned by `record.handle` (or `record.entity_handles`) — **not**
  `block_entity_handle`; model space is `doc.header.model_space_block_handle`.
- Electrical collectors must skip entities whose `owner_handle != model_space`, or block
  geometry is counted as wires.
- Label height comes from the library entry (`label_height`), not a constant.

---

## 10. Suggested next steps

1. Finish or revert the uncommitted `ELECSHEET` fit; verify with
   `sheet_batch.txt` on `tests\proj2\sheet-01.dxf` and a screenshot.
2. Project-wide BOM that walks `project_sheets()` (the per-drawing `ELECBOM` does not yet).
3. Footprints for `ELECPANEL` (sizes per part) + DIN rail.
4. Wire numbering formats per sheet/project.
5. Re-run `cargo test --release` for the app and record the result in
   `reports\ecad-verification.md`.

---

## 11. Handy one-liners

```powershell
# build
cargo build --release -j 4

# version + graphics backend
.\target\release\OpenCADStudio.exe --version

# full headless draft of a test drawing
.\target\release\OpenCADStudio.exe --each tests\e2e_batch.txt tests\e2e_test.dxf

# electrical crate tests
cargo test --release -p elec
```

---

## 12. Round 24 (2026-11-07): 78-symbol library, palette keys, manager dropdowns

Done on top of `fee94a0`, verified against this tree (not committed yet).

**Symbol library: 30 -> 78 symbols.** `tools/make_library.py` gained 48 entries, so
`library/starter` now covers the device families a real panel needs:

- Relays/timers: `RELAY_CONTACT_NO/NC`, `TIMER_COIL`, `TIMER_CONTACT_NO/NC`.
- Switching/protection: `SWITCH_DISCONNECT`, `EMERGENCY_STOP`, `FOOT_SWITCH`,
  `LIMIT_SWITCH_ROLLER`, `LEVEL_SWITCH`, `PRESSURE_SWITCH`, `FLOW_SWITCH`, `MCCB`,
  `RCD`, `THERMAL_MAGNETIC`, `SURGE_PROTECTOR`, `FUSE_3PHASE`.
- Power: `TRANSFORMER_3PHASE`, `GENERATOR`, `UPS`, `ENERGY_METER`, `SOFT_STARTER`,
  `SERVO_MOTOR`, `GEAR_MOTOR`.
- Sensing: `PHOTO_SENSOR`, `CAPACITIVE_SENSOR`, `ULTRASONIC_SENSOR`,
  `TEMPERATURE_SENSOR`, `ENCODER`.
- PLC: `PLC-AI`, `PLC-AO`, `PLC-CPU`, `PLC-PSU`, `PLC-COUPLER` (the DI/DO pair existed).
- Wiring hardware: `TERMINAL_GROUND`, `TERMINAL_END_STOP`, `TERMINAL_SEPARATOR`,
  `CONNECTOR_2P`, `CONNECTOR_4P`, `TEST_POINT`, `DIN_RAIL`, `WIRE_DUCT`, `WIRE_LABEL`.
- Field devices: `HEATER`, `FAN`, `BELL`, `BUZZER`, `VALVE_2WAY`, `VALVE_3WAY`.

The library is generated data: edit `SYMBOLS` in `tools/make_library.py`, then

```powershell
py -3.13 tools\make_library.py            # rewrites library/starter
py -3.13 tools\make_library.py --check    # proves the committed files match
```

New classes were added to `library/panel/footprints.tsv` (Timer, Switch, Surge
Protection, Meter, Test Point, Heater, Rail, Wire Duct, Generator, Fan, Valve, Bell,
Buzzer, PLC Analog Output) so `ELECPANEL` sizes them instead of boxing 14 x 10.

**Palette keyboard navigation** (`src/ui/window/elec_library.rs`). Arrow Up/Down move
the highlighted row through the filtered list, Enter places the highlighted symbol,
typing/Backspace clear the highlight. The highlighted row is drawn as a translucent
band behind the row inside the list's clip layer.

**Symbol Manager dropdowns** (`src/ui/window/elec_lib_manager.rs`). The details panel
now offers a "Pick an existing class..." `pick_list` beside the free-text class field,
and the add-row has a class dropdown plus a pins dropdown (0..=16). Typed values still
work, so a brand-new class or an unusual pin count is not blocked.

**Verification on this tree**

| Check | Result |
|---|---|
| `cargo check --release` | clean, 56.87 s, no errors |
| `cargo build --release -j 4` | finished 5 m 18 s, `target\release\OpenCADStudio.exe` 158 MB |
| `py -3.13 tests\ecad\run_smoke.py` | 92/99 |
| same suite, `ECAD_LIBRARY_DIR` = the committed 30-symbol library | 92/99, the same 7 checks |

The 7 failures are therefore **pre-existing** and not library-size related:
`ELECLIST printed the placed components` (the check still looks for ACADE class names
such as `Contactor` while the listing deliberately prints block names, since `ea53801`),
`ELECBOM wrote one row per (class, part)` (expects 5 rows; the batch places 6
components), and five checks about recovering tags/parts from the harvested real
sheets' TEXT, where the recovery currently finds the tag but no part number.

**Still open** (in the order they hurt most): a Numbering Format dialog (SkyCAD keeps
`NumberingFormat` with type/template/start/increment/uniqueness scope; this app still
only reads the drawing's `WD_M` settings through `ELECSETUP`), a class filter in the
palette, and catalogue rows in `library/parts/parts.tsv` for the new classes so BOM rows
carry manufacturer and description.

## 13. Round 25 (2026-11-07): numbering setup dialog, palette paging, parts catalogue

**Numbering setup dialog** (`ELECSETUPEDIT`, ribbon *Electrical -> Project & Sheet ->
Drawing -> Numbering Setup*). SkyCAD keeps numbering in a `NumberingFormat` object
(type, template, start, increment, uniqueness scope); this app only read an AutoCAD
Electrical `WD_M` block. The dialog now edits the same contract - tag format, preset and
mode, start, suffixes; wire format, preset, mode and start; cross-reference format and
sheet - and stores it as custom properties on the drawing:

| Property | Meaning |
|---|---|
| `ELEC_TAGFMT` | device tag template (`%F` family, `%N` number, `%S` sheet) |
| `ELEC_TAGMODE` | `C` continuous, `S` per sheet, `R` fixed |
| `ELEC_TAG_START` | first tag number |
| `ELEC_TAG_RSUF` | comma-separated per-sheet suffixes |
| `ELEC_WIREFMT`, `ELEC_WIREMODE`, `ELEC_WIRE_START` | wire numbering |
| `ELEC_XREFFMT`, `ELEC_SHEET` | cross references and the sheet label |

`read_setup` (`src/app/commands/elec.rs:263`) still reads the first `WD_M` block and
**only falls back to the properties when the drawing has none**, so a real AutoCAD
Electrical drawing keeps renumbering the way its template says. Applying is one undo
step (`ELECSETUPEDIT`) and refreshes fields that display a custom property.

Files: `src/ui/window/elec_setup.rs` (new view: `State`, `SetupMsg`, `mode_label`,
`mode_letter`), `src/app/commands/elec.rs` (`read_setup` fallback, `elec_setup_edit`,
`on_elec_setup`, `apply_elec_setup`), `ModalKind::ElecSetup` plus `Message::ElecSetup(..)`,
the view/title/close arms, the command registry entry, the help line and the ribbon item.

**Palette paging** (`src/ui/window/elec_library.rs`). Home/End jump to the first/last
match and Page Up/Page Down move a whole window, keeping the highlight inside the
visible rows.

**Parts catalogue** (`library/parts/parts.tsv`). About 40 rows for the Round 24 classes,
so `ELECBOM` fills manufacturer and description for them: timers, MCCB/RCD/thermal-magnetic
breakers, surge protector, 3-phase fuse and transformer, UPS, soft starter, meter,
generator, servo and gear motor, photo/capacitive/ultrasonic/temperature sensors, PLC
CPU/PSU/coupler/AI/AO, ground/end-stop/separator terminals, test point, 2P/4P connectors,
DIN rail, wire duct, valves, heater, fan, bell, buzzer. `py -3.13 tools\parts_check.py`
passes.

**Verification**

| Check | Result |
|---|---|
| `cargo check --release` | clean |
| `cargo test --release --lib elec_setup` | 3 passed |
| `cargo build --release -j 4` | finished 4 m 47 s |
| `py -3.13 tests\ecad\run_smoke.py` | 92/99, the same 7 pre-existing checks |

Unit tests added in `src/app/commands/elec.rs`:
`a_drawing_without_wd_m_reads_the_setup_properties`,
`the_dialog_stores_what_elecnumber_reads`,
`a_bad_counter_keeps_the_dialog_open_and_says_why`.

**Still open**: a class filter or grouping row in the symbol palette (the palette draws
its own rows, so grouping needs slot arithmetic; typing a class name in the search box
already filters), and the SkyCAD `NumberingFormat` uniqueness-scope and increment fields,
which the dialog does not expose yet.

## 14. Round 26 (2026-11-07): smoke 108/108, unique handles, numbering parity

This round closed the four gaps Round 25 left open, and fixed a save-visible bug it exposed.

**The smoke suite was measuring an old binary.** `tests/ecad/run_smoke.py` copies the
editor to `%TEMP%\ocs_baseline.exe` (sections 1-12) and `%TEMP%\ocs_current.exe` (real
sheets). Both copies were from 5 November, so the five checks that read tags back from
TEXT-recovered sheets failed against a build that did not have the recovery. The runner
now refreshes either copy when `target\release\OpenCADStudio.exe` is newer and prints
`refreshed the stale <name> copy from target/release`. Two further checks hard-coded
uppercase module block names (`HMS21`/`HMO13`) while the customer sheet spells them
`Hms21`/`Hmo13`; those compare case-insensitively now.

**`ELECINSERT` reused one handle per symbol.** The command clones the first instance of a
block on the sheet and only moved its insertion point, so three placed contacts were three
objects under one handle (`5B2`), and `ELECNUMBER` - which applies a label through
`doc.get_entity_mut(handle)` - wrote the last label to every part. The clone now starts
without identity (insert, attributes and embedded mtext all `Handle::NULL`) and the scene
allocates fresh handles through `adopt_attributes`. Verified: three contacts save as
`5B2`, `5BD`, `5C8` with `-K1`, `-K2`, `-F1` intact. This was a real drawing-integrity bug,
not only a test problem.

**SkyCAD `NumberingFormat` parity.** The core crate already carried every field; the app was
only reading the format/mode/start triple.

| SkyCAD field | Where it lives now |
|---|---|
| Order number format, mode, start, prefix | `ELEC_TAGFMT`, `ELEC_TAGMODE`, `ELEC_TAG_START`, `ELEC_TAG_RSUF` |
| Order number increment | `ELEC_TAGINC` -> `NumberingFormat::increment` |
| Follow through (number a device once) | `ELEC_TAGFALL` -> parts of one tag share a label; off numbers each symbol |
| Wire numbering format/mode/start | `ELEC_WIREFMT`, `ELEC_WIREMODE`, `ELEC_WIRE_START` |
| Wire numbers: numerical/alphabetical | `ELEC_WIREKIND` -> `Kind::Alphabetical` |
| Empty first value | `ELEC_WIREEMPTY` -> the first net gets no number |
| Uniqueness scope | `ELEC_TAGUNIQ` -> `Global` skips tags used on the project's other sheets (Round 27) |

Both `ELECNUMBER` and `ELECWIRENUMBER` used to apply the contract only when a `WD_M` block
was present, so the dialog-written properties were silently ignored; they apply the values
unconditionally now. `ELECSETUP` grew a headless argument form (`ELECSETUP TAGINC=2
TAGFALL=No WIREKIND=Alphabetical ...`) with the same validation as the dialog, so scripts
and tests do not have to click. `write_setup_properties` is the one writer of the `ELEC_*`
keys, used by the dialog and the command.

**Saving these properties needs SAVEAS.** `tools/dxfio.py` writes DXF R12, which cannot
carry custom properties; `QSAVE` therefore raises the conversion-loss confirmation and a
headless batch silently drops the save. The numbering batches in the smoke suite use
`SAVEAS <sheet>` and the sheet is written as AC1012. Worth knowing for any future test that
stores `ELEC_*`.

**Palette class chips.** `src/ui/window/elec_library.rs` grew a chip strip under the search
field: `All` plus one chip per class, hit-tested by measured text width, with the row area
shifted by `CHIP_H = 26.0` and `capacity` recomputed. The starter library has forty-two
classes, which is more than fits one strip, so the wheel over the chips scrolls the strip
sideways (`State::chip_scroll`, clamped to the measured total) and `<` / `>` chevrons mark
the hidden side; the strip is painted inside its own layer so a scrolled chip cannot spill
onto the search field or the list. Keyboard: Home/End and Page Up/Page Down move by a
window. Searching a class name still works and composes with the chip. After the scrolling
change: `cargo build --release -j 4` 4 m 23 s, smoke 108/108, `ecad-palette.png`
re-captured with the `<` / `>` chevrons visible.

**Screenshots.** `scripts/capture_ecad_screenshots.py` launches the editor per picture with
`--http PORT`, runs the commands, and calls `capture` over the GUI REST bridge (bearer token
from `OCS_API_TOKEN`, pinned by the script). Six new pictures in `docs/screenshots`:
`ecad-palette`, `ecad-symbol-manager`, `ecad-numbering-setup`, `ecad-bill-of-material`,
`ecad-terminal-strips`, `ecad-wire-list`; `README.md` links the first three. The same round
fixed `POST /api/v1/commands` to forward `document_id` - the documented route was unusable
against a GUI with more than one tab.

**Verification**

| Check | Result |
|---|---|
| `cargo check --release` | clean |
| `cargo test --release --lib elec` | 353 passed, 3 ignored |
| `cargo test --release -p elec` | 131 passed |
| `cargo build --release -j 4` | finished 5 m 05 s |
| `py -3.13 tests\ecad\run_smoke.py` | **108/108 checks passed** |
| `py -3.13 tools\make_library.py --check` | in sync, 78 symbols |
| `py -3.13 tools\parts_check.py` | passes |

**Still open**: `ELECLIST` still prints the block name as the class for library-less
symbols, which the reports render as-is.

## 15. Round 27 (2026-11-07): uniqueness scope, connection webs, smoke 113/113

The last SkyCAD `NumberingFormat` field is now honoured end to end. ELECPANEL already read
`library/panel/footprints.tsv`, so panel layout was not a gap.

**Uniqueness scope.** `ElecSetup` gained `tag_unique` (`Global` default, plus `Local` and
`None`, normalised by `canonical_uniqueness`), stored as `ELEC_TAGUNIQ` - the fifteenth
`ELEC_*` property. With `Global`, `elec_number` first scans `project_sheets()` (the project
file or, failing that, the folder set by `ELECPROJECT <dir>`), loads every other sheet, and
collects the upper-cased tags it finds. Those become `reserved`; the assignment loop walks
the ordinal forward until a label is free, so a new sheet continues past `-K1`/`-K2`
instead of reusing them. `Local` and `None` skip the scan.

**Everywhere it is exposed.** `ELECSETUP TAGUNIQ=Global|Local|None` (unknown values are
refused with "TAGUNIQ must be Global, Local or None."), the `ELECSETUP` printout gained
`unique <scope>` on the tag line, and the dialog has a `Unique` pick list under `Follow
through` with the hint "Global skips numbers used on other sheets."

**Number connection webs.** `ElecSetup` gained `wire_number_web` (default on, property
`ELEC_WIREWEB`). `ELECWIRENUMBER` now numbers either one number per net - a web is one
electrical node, so every conductor of it shares the number - or, with the option off, one
number per drawn segment, which is what a panel builder wants for wire tags. The summary
line says which unit it counted (`net(s)` / `segment(s)`). Exposed as `ELECSETUP WIREWEB=No`
and as a `Number webs` pick list in the dialog.

**Tests.** Two unit assertions (`ELEC_TAGUNIQ` round-trip; the dialog stores `None`) in
`mod elec_setup_tests` - 4 passed. New smoke section 16 (`run_uniqueness_scope`) numbers a
first sheet `-K1`/`-K2`, then numbers a second sheet with `TAGUNIQ=Global` (expects `-K3`)
and a third with `TAGUNIQ=Local` (expects `-K1`). Two more checks draw a corner (two
segments, one net) plus a loose conductor and assert `WIREWEB=Yes` labels twice while
`WIREWEB=No` labels three times.

| Check | Result |
|---|---|
| `cargo check --release` | clean |
| `cargo test --release --lib elec_setup` | 4 passed |
| `cargo build --release -j 4` | finished in 8 m 13 s |
| `py -3.13 tests\ecad\run_smoke.py` | **113/113 checks passed** |
| `docs/screenshots/ecad-numbering-setup.png` | re-captured with the `Unique` row |

**Still open**: `ELECLIST` prints the block name as the class for library-less symbols.
