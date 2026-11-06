# Handoff â€” AryaCADElectrical electrical layer

Written at the end of the current working session (2026-11-06). Read this first, then
[`PROGRESS.md`](PROGRESS.md) (round-by-round measurements) and
[`skycad-feature-matrix.md`](skycad-feature-matrix.md) (the 19-feature scoreboard against
SkyCAD's published list).

## State: nets now reach device pins

The fork's build was dead when this work started. It now builds, runs, reads the customer's
real drawings, and **connects wires to the pins of real devices** â€” the change round 8 made
and the one that turns a wire list into something an electrician can pull from.

Gate that was green at the end of the session:

```
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check --release --tests       # exit 0
cargo test -p elec --lib            # 131 passed
cargo build --release -j 1          # 13 m 46 s with a 4-CPU affinity cap
```

**Build note â€” read this before building:** this machine (Intel i9-14900KF, ASUS PRIME
Z790-V WIFI, BIOS 1666) has bugchecked twice *during* a release build. That is the
documented Intel 13th/14th-gen instability, not this repository; the evidence and the fix
list are in [`PC-STABILITY.md`](PC-STABILITY.md). Build the way the round-8 work did:

```
powershell -NoProfile -ExecutionPolicy Bypass -File target\tmp\pinprobe\build_aff.ps1
```

It pins `cargo` to four logical CPUs (`-j 1` + affinity mask `0xF`) and finished both
rebuilds without a crash. Do not run an unconstrained `cargo build --release` until the
BIOS/CPU issue is resolved.

A clean dependency graph also needs the `[patch]` block in `Cargo.toml` â€” the `acadrust`
repository moved from `HakanSeven12` to `hakanaktt` on GitHub. Do not remove it.

## How to test anything (headless)

```
target\release\OpenCADStudio.exe --each <batch.txt> <drawing.dxf>
```
- One command per line; a blank line ends an interactive command; point input is `x,y` on
  one line; **the drawing is only written if the batch ends with `SAVE`/`QSAVE`**.
- Child processes are fine under `%TEMP%` now, but keep scratch work under `target\tmp\`.
- Read results back from the saved DXF/CSV with `py -3.13` (see `tests/ecad/read_io.py`).
- `python` (3.11) on PATH is broken here (`sys.prefix` resolves to the CWD); use `py -3.13`
  or the bundled DSH python.
- `target\tmp\pinprobe\` holds this round's batches and the `verify.py` runner:
  `py -3.13 target\tmp\pinprobe\verify.py` re-runs the real sheets and the app-drawn
  pin-to-pin case and writes `verify_*.log`.

Fixtures and data:
- `import/Prints/Prints/DXF/FC1000495-0xx.dxf` â€” 34 real AutoCAD Electrical sheets (not
  committed; git-ignored). `FC1000495-003` keeps its `WIRES` layer (8 segments) and 4
  attributed devices; `FC1000495-046` is flattened (53 layer-0 wires, 39 nets).
- `library/` â€” 18 symbol DXFs harvested from those sheets + `library.tsv`
  (`symbol \t class \t attributes \t scale`). `library/starter/` â€” 27 generated IEC symbols.
- `library/parts/parts.tsv` â€” the local parts catalogue (29 parts, 13 classes).
- `tools/` â€” `dxfio.py` (DXF reader/writer), `harvest_library.py`, `make_library.py`,
  `wd_m_profile.py`, `parts_import.py`, `parts_check.py`, `make_io_sample.py`.
- `tests/ecad/` â€” headless batches for every feature and the sample I/O workbook.

## What works (verified, numbers in PROGRESS.md)

- Reading real sheets: `ELECLIST` finds 13 components on `-002`; `ELECSETUP` reads the
  drawing's `WD_M` contract; `ELECNUMBER` honours the `TAGMODE=R` freeze.
- **Pin-level connectivity (round 8):** `ELECWIRELIST` on `-003` reports `8 wire
  segment(s), 10 pin(s) -> 4 net(s)` with `net 7 -> M326:2`; on `-046` `53 -> 39 nets`
  with six tagged contacts; on an app-drawn sheet two `ELECINSERT` symbols joined by one
  wire give `net 1 -> -K1:2, -K2:1` and `ELECWIRENUMBER` labels it.
- **Pin columns, library pins, navigation (round 9):** `tools/make_library.py` emits
  invisible `X*TERM*` pin ATTDEFs at the true pin positions (27 starter symbols
  regenerated), `ELECWIRELIST <out.csv>` and the `ELECREPORT` XLSX wire sheet carry
  `From pin`/`To pin`, and `ELECHIGHLIGHT <tag>` / `ELECGOTO <tag>` live in a Navigation
  ribbon group.
- Local library + palette, local parts catalogue, `ELECASSIGN`, BOM/parts/wire/cable/
  terminal/I/O reports, Excel I/O import â†’ PLC modules with `ADDRnn` attributes, revision
  snapshots, built-project protection, `ELECHELP`, native Electrical ribbon (9 groups,
  43 command ids, 37 icons), searchable palette, exportable wire label lists.

## The next three things, in order

1. **Draw a cable as a schematic object** â€” `cables.tsv` holds the schedule (tag, cores,
   part, length, description); drawing the cable and storing its conductors would let the
   schedule be derived from geometry instead of typed. Start from the `ELECPLACE`/insert
   machinery; `ELECCABLELIST` already has the column set, and the pin-anchoring pass gives
   the net each core lands on.
2. **Panel layout with real footprints and cut-outs** â€” `ELECPANEL` still draws fixed
   14Ã—10 boxes on a grid. Add `footprint` columns to `library/parts/parts.tsv`, place the
   footprint blocks on DIN-rail rows, size the enclosure from the union of footprints, and
   register the panel drawing in the project.
3. **Configuration management / design-by-systems** â€” extend `.eprj` with `variant <name>`
   blocks (sheet list + excluded tags) and add `library/systems/` fragments with
   instantiate-and-retag, so a motor starter drawn once can be dropped into a new project.
   Then `ELECPROJNUM` should read each sheet's `WD_M` freeze (it respects `ELECBUILT` only).

Then: harness design stays deferred (needs lengths a 2D schematic does not carry), and the
refreshed smoke suite should replace the seven pre-round-2 expectations.

## Gotchas that cost time (do not relearn these)

- Use the `write`/`edit` tools for source; PowerShell `Set-Content`/`-replace`/`>` corrupted
  a source file to 1.56 GB in an earlier session. The `edit` tool strips UTF-8 BOMs â€” re-add
  them with a tiny python script that writes bytes once.
- The app writes relative output paths into its **working directory**; pass absolute paths.
- `--each` does **not** save automatically; add `SAVE`/`QSAVE`.
- Block contents are owned by the block **record**; `block_entity_handle` is the BLOCK marker.
- Collectors must use `drawing_spaces()` (model + every `*Paper_Space*`), not model space alone.
- **Wires are not every LINE.** The `WIRES` layer when the sheet has one, else layer-0 lines
  â‰¥ 0.5 units. `WIRENO` starts with `WIRE` too â€” match the whole name.
- `doc.entities()` also returns block-definition geometry; filter with `in_drawing_space`.
- A symbol's pins come from `X<n>TERM<nn>` attributes when present, else from the block's
  geometry. A wire that stops 0.125 short of a pin still connects (0.2 association).
- A T-junction only connects where a dot marks it; `ELECJUNCTIONS` draws those dots.
- Do not run `ELECNUMBER`/`ELECPROJNUM` apply on the customer sheets without checking
  `ELECSETUP` first â€” the `WD_M` freeze protects tags, `ELECBUILT` blocks renumbering entirely.
- `tests/ecad/run_smoke.py` is stale: 92/99 pass, and the 7 failures assert pre-round-2
  behaviour (`-002` yields "no components", BOM of 2 rows, class-name listing). Fix the
  expectations when next touching that file; the wire/net checks pass.
