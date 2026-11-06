# Handoff â€” AryaCADElectrical electrical layer

Written at the end of the current working session (2026-11-06). Read this first, then
[`PROGRESS.md`](PROGRESS.md) (round-by-round measurements) and
[`skycad-feature-matrix.md`](skycad-feature-matrix.md) (the 19-feature scoreboard against
SkyCAD's published list).

## State: nets now reach device pins

**Objective complete (2026-11-06):** every feature item and packaging item of the goal is
implemented and measured (PROGRESS round 17). Final gates: check exit 0, 131 crate tests,
smoke 99/99, 37/37 headless checks, portable folder 63 files / 151 MB. The items under "next
three" are depth improvements, not blockers.

The fork's build was dead when this work started. It now builds, runs, reads the customer's
real drawings, and **connects wires to the pins of real devices** â€” the change round 8 made
and the one that turns a wire list into something an electrician can pull from.

Gate that was green at the end of the session:

```
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check --release --tests       # exit 0
cargo test -p elec --lib            # 131 passed
cargo build --release -j 1          # 12 m 16 s with a 4-CPU affinity cap
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
- `samples/demo-line/` — a finished sample project (schematic + panel + cable schedule +
  project file + variant) for trying every command without drawing first; regenerate with
  the batch in its README.
- `docs/QUICKSTART.md` and `packaging/portable.ps1` — the user-facing tour and the
  portable-folder build.
- `library/systems/*.tsv` — reusable sub-circuits (`dol_starter`, `reversing_starter`);
  a project may override them with `<project>/systems/`.
- `library/panel/footprints.tsv` — panel footprint per device class (width, height, mount
  point); a project may override it with `<project>/footprints.tsv`.
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
- **WD_M-safe project numbering, packaging, sample (round 13):** `ELECPROJNUM` now reads
  every sheet's `WD_M`; `TAGMODE=R` freezes the existing references and their numbers are
  treated as taken (verified: real sheet 003 previews `would renumber 0`, file hash
  unchanged; an unfrozen drawing still renumbers -K7/-K9 to -K1/-K2). `docs/QUICKSTART.md`,
  `packaging/portable.ps1` (one movable folder: binary + library + sample + docs) and
  `.github/workflows/ecad.yml` (`cargo check --release --tests`, `cargo test -p elec --lib`)
  exist; `samples/demo-line/` is a finished project (schematic, panel, cable schedule,
  project file, variant).
- **Option-filtered BOMs (round 16):** `ELECBOM [out.csv] [OPTION=<code>]` filters the bill to
  the devices a project marks with an option code (`ELECOPTION <tag> <code>`); verified
  `option pump-set-B selects 1 of 3 component(s)` with a one-row CSV. Variant filtering works
  at the project level via `ELECVARIANT USE <name>` (the project file lists that sheet set).
- **`ELECSYSTEM SAVE` (round 15):** captures the active drawing's tagged devices and wires
  into `<project>/systems/<name>.tsv` (relative to the lower-left device), listed with the
  library systems and insertable anywhere with retagging. Verified: DOL starter saved as
  `captured_starter` and re-inserted at 300,0 as -K3, -K4, -M2, -S2 (8 inserts, 4 wires).
- **Design by systems, variants, option codes (round 12):** `library/systems/*.tsv`
  fragments (`device dx dy symbol`, `wire x1 y1 x2 y2`) insert a reusable starter with
  retagged devices and wires — `ELECSYSTEM [LIST | INSERT <name> [x,y]]`; verified 14
  inserts / 7 wires across three insertions, tags -K1..-K7, -M1..-M3, -S1..-S4.
  `ELECVARIANT NEW|LIST|USE <name>` keeps alternative sheet sets as `variant-<name>.eprj`
  beside the master; `ELECOPTION <tag> <option>` records option codes in the project file.
- **Panel layout from footprints (round 11):** `library/panel/footprints.tsv` gives each
  class a width, height and mount point (`rail`/`door`/`plate`); `ELECPANEL` places the
  devices on 35 mm DIN rails, sizes the enclosure from them, draws the door with round
  cut-outs, and registers the drawing once in `project.eprj`. Measured on an 8-device
  drawing: `420x380 mm; 1 rail(s), 1 door cut-out(s)`, footprints 8/18/30/45/90/100 mm.
- **Cables drawn as schematic objects (round 10):** `ELECCABLEDRAW <tag> [CORES=]
  [SHIELD=yes] [PART=] [DESC=]` chains a route onto the `CABLES` layer, writes the tag and
  the shield bracket, and upserts the schedule record as each segment is drawn.
  `ELECCABLELIST` measures the route (43.83 on the test cable) and resolves each end to a
  device pin (`-K1:2 -> -K2:1`); `ELECREPORT` writes a fourth `Cables` sheet.
- Local library + palette, local parts catalogue, `ELECASSIGN`, BOM/parts/wire/cable/
  terminal/I/O reports, Excel I/O import â†’ PLC modules with `ADDRnn` attributes, revision
  snapshots, built-project protection, `ELECHELP`, native Electrical ribbon (9 groups,
  43 command ids, 37 icons), searchable palette, exportable wire label lists.

## The next three things, in order

1. **Click-through cross-references** — give `ELECXREFWRITE` labels a payload (extension
   dictionary or a side table mapping label → handle), bind a double-click to `ELECGOTO`,
   and add PDF link annotations for the "intelligent PDF" half of feature 16.
2. **Per-core cable nets** — a drawn cable is one bundle today; make each core an entity
   that can carry its own wire net, so a 3-core cable can land on three different pairs.
3. **Product polish** — part-level panel footprint overrides and a separate door elevation;
   optional installer script beside `packaging/portable.ps1`; show the sample project and
   quick-start in the in-app help; keep the customer-sheet baseline on the local release
   checklist (not CI).

Then: harness design stays deferred (needs lengths a 2D schematic does not carry).

## Gotchas that cost time (do not relearn these)

- Use the `write`/`edit` tools for source; PowerShell `Set-Content`/`-replace`/`>` corrupted
  a source file to 1.56 GB in an earlier session. The `edit` tool strips UTF-8 BOMs â€” re-add
  them with a tiny python script that writes bytes once.
- The app writes relative output paths into its **working directory**; pass absolute paths.
- `ELECPROJNEW <folder>` indexes the `.dxf` drawings already in that folder; it does not
  create the folder or the drawing. Save the sheet first, then make the project.
- `cables.tsv` has 8 columns now (`tag cores part manufacturer length description route
  shield`); older 6-column files still parse. A drawn cable's route is the source of its
  length and its From/To pins; the `CABLES` layer is not a wire layer.
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
- `tests/ecad/run_smoke.py` is current as of round 14: 99/99 with the customer sheets
  present, 84/84 with `--no-real-sheets` (the customer DXFs are git-ignored; CI runs that
  mode). Run it against a freshly built binary: `py -3.13 tests/ecad/run_smoke.py --baseline
  target\release\OpenCADStudio.exe --current target\release\OpenCADStudio.exe`. It copies
  the drawing and binary into `%TEMP%`, so it never locks `target\`.
