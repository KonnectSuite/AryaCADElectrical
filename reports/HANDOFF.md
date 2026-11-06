# Handoff — AryaCADElectrical electrical layer

Written at the end of the current working session (2026-11-06). Read this first, then
[`PROGRESS.md`](PROGRESS.md) (round-by-round measurements) and
[`skycad-feature-matrix.md`](skycad-feature-matrix.md) (the 19-feature scoreboard against
SkyCAD's published list).

## State: it builds, it runs, it reads the customer's real drawings

Seven commits on top of the imported fork (`git log --oneline`): dispatch/loader fixes,
real-sheet recognition, WD_M-driven numbering, native ribbon + palette, local parts library,
Excel I/O → PLC drawings, revisions + built-project control, cable schedules, wire nets with
T-junction merging, `ELECHELP`.

Gate that was green at the end of the session:

```
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release -j 2          # ~5-8 min incremental, clean ~35-40 min
cargo check --release --tests       # exit 0
cargo test -p elec --lib            # 121 passed
```
**Build note:** a clean dependency graph needs the `[patch]` block in `Cargo.toml` — the
`acadrust` repository moved from `HakanSeven12` to `hakanaktt` on GitHub. Do not remove it.
One `rustc` run in this session died with `STATUS_ACCESS_VIOLATION`; retrying with `-j 2`
succeeded. That is this crate's known flakiness, not a code fault.

## How to test anything (headless)

```
target\release\OpenCADStudio.exe --each <batch.txt> <drawing.dxf>
```
- One command per line; a blank line ends an interactive command; point input is `x,y` on
  one line; **the drawing is only written if the batch ends with `SAVE`**.
- Child processes in this session were denied writes under `%TEMP%`; use a scratch folder
  under `target\tmp\`.
- Read results back from the saved DXF/CSV with `py -3.13` (see `tests/ecad/read_io.py`).
- `python` (3.11) on PATH is broken here (`sys.prefix` resolves to the CWD); use `py -3.13`
  or the bundled DSH python.

Fixtures and data:
- `import/Prints/Prints/DXF/FC1000495-0xx.dxf` — 34 real AutoCAD Electrical sheets (not
  committed; git-ignored). `FC1000495-003` has 4 attribute devices + 132 wire segments;
  `-046` has 77 wire segments and the cleanest net structure.
- `library/` — 18 symbol DXFs harvested from those sheets + `library.tsv`
  (`symbol \t class \t attributes \t scale`). `library/starter/` — 27 generated IEC symbols.
- `library/parts/parts.tsv` — the local parts catalogue (29 parts, 13 classes).
- `tools/` — `dxfio.py` (DXF reader/writer), `harvest_library.py`, `make_library.py`,
  `wd_m_profile.py`, `parts_import.py`, `parts_check.py`, `make_io_sample.py`.
- `tests/ecad/` — headless batches for every feature and the sample I/O workbook.

## What works (verified, with the numbers in PROGRESS.md)

- Reading real sheets: `ELECLIST` finds 13 components on `-002`, 57 nets on `-003` (was 0 / "no wires found").
- `ELECSETUP` reads the drawing's `WD_M` contract (`TAGFMT=%F%N`, `TAGMODE=R`, `WIREFMT=%N`);
  `ELECNUMBER` honours it and freezes existing references; `ELECWIRENUMBER` numbers real nets.
- Local library + palette, local parts catalogue, `ELECASSIGN`, BOM/parts/wire/cable/terminal/I/O
  reports, Excel I/O import → PLC modules with `ADDRnn` attributes, revision snapshots,
  built-project protection, `ELECHELP`, native Electrical ribbon (7 groups, 31 command ids,
  35 icons) and a searchable symbol palette.

## The next three things, in order

1. **Pin-level anchoring** — nets connect to each other but not to devices. `components_of()`
   recovers tags from TEXT (the sheets were exploded); anchors still use the insert's
   insertion point, so a net has no device. Derive a pin position per recovered device (e.g.
   the wire endpoint nearest the tag TEXT, or the symbol bbox edge from the block geometry)
   and let `Anchor` carry it. Expected payoff: pin-to-pin wire lists, real I/O cross-references.
2. **Draw a cable as a schematic object** — `cables.tsv` holds the schedule (tag, cores, part,
   length, description); drawing the cable and storing its conductors would let the schedule be
   derived from geometry instead of typed. Start from the `ELECPLACE`/insert machinery.
3. **Navigation tools** — `ELECXREF` writes reference TEXT; add a command that selects every
   part of one device (`ELECHIGHLIGHT <tag>`) and one that jumps to a referenced position. The
   scene selection API is in `src/scene/selection.rs` (`select_entity`, `selected_entities`).

Then: panel layout with real footprints/cut-outs (`elec_panel` currently draws fixed 14×10
boxes), configuration management / design-by-systems, and `ELECPROJNUM` still ignores the
per-sheet `WD_M` freeze (it respects `ELECBUILT` only).

## Gotchas that cost time (do not relearn these)

- Use the `write`/`edit` tools for source; PowerShell `Set-Content`/`-replace`/`>` corrupted a
  source file to 1.56 GB in an earlier session. The `edit` tool strips UTF-8 BOMs — re-add them
  with a tiny python script that writes bytes once.
- The app writes relative output paths into its **working directory**; pass absolute paths.
- `--each` does **not** save automatically; add `SAVE`.
- Block contents are owned by the block **record**; `block_entity_handle` is the BLOCK marker.
- Collectors must use `drawing_spaces()` (model + every `*Paper_Space*`), not model space alone.
- Do not run `ELECNUMBER`/`ELECPROJNUM` apply on the customer sheets without checking
  `ELECSETUP` first — the `WD_M` freeze protects tags, `ELECBUILT` blocks renumbering entirely.
