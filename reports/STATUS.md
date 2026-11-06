# AryaCADElectrical — status review

**Date:** 2026-11-05 · **Machine:** `C:\Users\KNPhu` (new box; previous work was done on `C:\Users\HomeAdmin`)
**Scope:** code review of the electrical layer, verification of the handoff claims, build/dependency audit, and the SkyCAD question.
**Companion reports:** [`session-review.md`](session-review.md) (transcript forensics, 12,354 records) · [`code-audit.md`](code-audit.md) (line-level code audit).

---

## 0. Verdict

The electrical layer is **real, working code** — not a mock-up. `crates/elec` is a clean dependency-free domain crate with **121 passing unit tests**, and `src/app/commands/elec.rs` is **32 commands / 187,886 bytes / 4,487 lines of pure ASCII** with no corruption residue. The reporting half (BOM, wire list, terminals, cross-references, numbering) was **verified headless against the user's 34 real production sheets**, with results read back from the saved DXF rather than trusted from stdout.

It now **builds and runs on this machine** — verified today (`cargo build --release` finished in **11 m 36 s**, `OpenCADStudio.exe` 150.2 MB, and a headless `ELECLIST` / `ELECSHEET` run produced real output). One dead git dependency had to be patched first (§4).

But it is a **convention-on-DXF command library, not yet an AutoCAD-Electrical/SkyCAD-class layer**, and two things are blocking progress right now:

1. **The data folder is gone** — the symbol library, the 34 real sheets, the test batches and the verification reports lived *outside* the repo and are not on this PC.
2. **The app-level test suite does not compile** (three trivial type errors), so only `crates/elec` is proven by tests today.

---

## 1. What exists

Timeline of the previous session (2026-11-04 02:16 → 2026-11-05 20:59, 56 turns, 1,657 tool calls, 3 goals):

| Phase | Outcome |
|---|---|
| Goal 1 — reporting | `crates/elec` + the `ELEC*` reporting commands; verified on all 34 real sheets (exit 0, DXF read-back) |
| Goal 2 — drafting | 51-symbol library harvested from the real drawings, palette with thumbnails, placement, attributes, wires, junctions, symbol authoring; verified in the GUI with screenshots |
| Goal 3 — parity | PLC I/O, per-point cross-references, panel arrangement, `.eprj` project files, project-wide renumber, sheet frame + title block |
| Incident | `elec.rs` reached **1.56 GB / one 736-million-character line** (PowerShell mojibake doubling) → every link failed `E0786`; ~5 h lost before the file was found; repaired; build back to ~11 min incremental |
| Publish | 14 commits, history rewritten twice to satisfy GitHub's 100 MB limit, pushed to `KonnectSuite/AryaCADStudio` |

**The history is not in this checkout.** `git log` here shows one squashed commit ("Initial import … renamed from AryaCADStudio"). The 14-commit history lives on GitHub per the transcript; confirm it from a logged-in machine before trusting anything else about provenance.

---

## 2. What works (verified, not claimed)

| Area | Evidence |
|---|---|
| **Reports** — `ELECBOM` (+CSV), `ELECWIRELIST`, `ELECTERMINALS`, `ELECREPORT` (xlsx) | exact outputs quoted in session review; ran over 34 production sheets |
| **Numbering** — `ELECNUMBER`, `ELECNUMBERPREVIEW` | idempotent; preview proven non-mutating (DXF hash unchanged); 20 numbering tests in the crate |
| **Cross-references** — `ELECXREF`, `ELECXREFWRITE` | contact-level refs (`-K1 (3 part(s)): 13-14, 21-22`), labels written on layer `XREF`, idempotent |
| **Drafting path** — `ELECLIB` → `ELECPLACE`/`ELECINSERT` → `ELECSETATTR` → `ELECWIRE` → `ELECJUNCTIONS` → `ELECMAKESYMBOL` | 50/51 symbols loaded, 2,053 palette thumbnail segments, correct T-junction rule, wrote a new `MYCOIL.dxf` (50→51) |
| **PLC / project** — `ELECPLC`, `ELECIO`, `ELECPLCNEXT`, `ELECPROJNEW/OPEN/LIST`, `ELECPROJNUM` | `I0.0..I0.7`, rollover `O0.6..O1.1`, `I0.0 -> -K1`, preview/apply with `.dxf.bak` |
| **Domain crate** | **121 passed / 0 failed**, 11 modules, 3,706 lines — re-verified on this machine via `rustc --test` |
| **UI integration** | `Electrical` ribbon tab registered unconditionally ([registry.rs:27](src/modules/registry.rs:27)); palette modal fully wired (`ModalKind::ElectricalLibrary`, view + body + click → `ELECPLACE <sym>`) |
| **Code health** | `elec.rs` 187,886 B / 4,487 lines, max line 190, pure ASCII, **0** TODO/FIXME/`todo!`/`unimplemented!`/`panic!`, 0 duplicate fns, 0 mojibake |

---

## 3. What is broken or unverified

| # | Problem | Evidence | Severity |
|---|---|---|---|
| 1 | **`ELECSHEET <size> <title>` runs twice.** It is missing from the early-argument list in [utilities.rs:1085-1097](src/app/command_driver/utilities.rs:1085) — the same list whose comment documents this exact hazard | **reproduced on this machine**: one `ELECSHEET A3 Test Sheet` line drew the default A4 frame *and* the A3 frame | high |
| 2 | **App tests do not compile.** `PlaceSymbolCommand::new` takes 4 args, tests pass 2 ([place.rs:102](src/modules/electrical/place.rs:102)); tests use `CmdResult::Finished` and `{:?}`/`assert_eq!` on `CmdResult`, which has **no derives** ([command.rs:1813](src/command.rs:1813)) | confirmed by reading both files; refutes the handoff's "not re-run since" | high |
| 3 | **`ELECSIDES` does not exist** — registry says `ELECSIDES`, dispatcher implements `ELECTSIDES` | typed name → `✕ INVALID: Unknown command` | medium |
| 4 | **`ELECINSERT` ribbon button is dead** — the button sends bare `ELECINSERT`; only `ELECINSERT <block>` is handled ([elec.rs:4253](src/app/commands/elec.rs:4253)) | verified in dispatch | low |
| 5 | **`ELECPANEL` is a skeleton** — fixed 14×10 boxes, no footprints, no DIN rail, alphabetical order | [elec.rs:3263-3355](src/app/commands/elec.rs:3263) | medium |
| 6 | **`ELECBOM` is empty on the real sheets** — their attributes live only on the `WD_M` setup block of the plot exports; there is no AutoCAD-Electrical `.ACD` import path | data limitation, not a bug | medium |
| 7 | **`ELECSHEET` fit-to-extent rewrite is present but runtime-unverified** (compile-checked only in the old session; nothing on this machine could build it until now) | [elec.rs:3814-4038](src/app/commands/elec.rs:3814) | medium |
| 8 | **`WD_*` symbols are dropped everywhere.** `is_setup_block` treats *any* block starting `WD_` as setup ([elec.rs:4318](src/app/commands/elec.rs:4318)) | verified | medium |
| 9 | **Block-embedded symbols report "no geometry"** — the loader filters on `record.block_entity_handle` instead of `record.handle` / `entity_handles` ([elec.rs:1669](src/app/commands/elec.rs:1669)) | verified against the file's own later convention at `:2386` | medium |
| 10 | **`ELECPROJNEW/OPEN/NUM` are also missing from the early-argument list** (`NUM apply` runs preview then apply) | same list | low/medium |
| 11 | `expr.rs` (660 lines, 16 tests — the `<r:…>` label language) is **completely unwired**; 0 references anywhere | audit | low |
| 12 | Ribbon icon uses `"\\u{2795}"` (double escape) → renders the literal text | [electrical/mod.rs:62](src/modules/electrical/mod.rs:62) | cosmetic |

---

## 4. Dependencies — what is installed, what was fixed, what is missing

### Present on this machine
| Component | State |
|---|---|
| Rust / cargo | **1.99.1**, host `x86_64-pc-windows-msvc` ✔ |
| VS 2022 Build Tools + MSVC | ✔ `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools` |
| Windows SDK | ✔ 10.0.26100.0 (and 10.0.22621.0) |
| Git | ✔ 2.54.0 |
| Python | ⚠ `py -3.13` works; **bare `python` (3.11) is broken** — it resolves its prefix to the CWD (`No module named 'encodings'`) |
| Node | ✔ v26.1.0 (needed by REA) |
| Disk | ✔ 614 GB free |
| SkyCAD Electrical | ✔ **1.3.65 installed today** (see §5) |

### Fixed today
**`acadrust` moved on GitHub.** The build died on:
`failed to get acadrust … unable to update https://github.com/HakanSeven12/acadrust.git?rev=d6d7e132 … 404`.
The repository now lives at **`hakanaktt/acadrust`** and the pinned commit `d6d7e132…` still exists there. A `[patch]` block in [Cargo.toml](Cargo.toml) redirects the source without editing the transitive `opencaddocument` manifest. After that patch, dependency resolution succeeded and the release build started compiling.

This affects **every fresh clone on every machine** — it is not local to this PC. Treat third-party git revisions (5 of them, all `HakanSeven12/*`) as a supply-chain risk: vendor them (`cargo vendor`) before the next milestone.

### Still missing
1. ~~The build is unproven~~ — **proven 2026-11-05 23:46**: clean `cargo build --release -j 4` → `Finished release profile [optimized] target(s) in 11m 36s`, exit 0, `target\release\OpenCADStudio.exe` = 150.2 MB, `--version` → `OpenCADStudio 2026.44+gfdfe8b47.dirty`.
2. **The data folder** (see §6).
3. Nothing else. No other software dependency is unaccounted for.

### Verified by running it
```
ELECSHEET: A4 fitted to the drawing at 1:1 (297.00x210.00 units); title block shows sheet - / -
ELECSHEET: A3 fitted to the drawing at 1:1 (420.00x297.00 units); title block shows sheet - / Test Sheet.
✕ INVALID: Unknown command: ELECSIDES
✕ INVALID: Unknown command: ELECINSERT
ELECATTR: use ELECSETATTR <tag> NAME=value
```
That single headless run **proves four of the findings in §3 at once**: `ELECSHEET` executes twice (bare default frame, then the argument frame — two frames, two undo steps), `ELECSIDES` and bare `ELECINSERT` are unknown commands, and `ELECATTR` is a stub. It also shows the fit-to-extent title block path itself works and reports its scale.

---

## 5. The SkyCAD question, answered

**Short answer: SkyCAD is not needed to build, run, or continue developing AryaCADElectrical. It is worth having anyway — as a data and specification source, not as a runtime dependency.**

### How the previous session actually reverse-engineered it
- SkyCAD Electrical **was installed** on the old machine (`C:\Program Files\SkyCAD Electrical`, data in `C:\SkyCAD Environments\Standard Environment`).
- It was **never launched** by the agent — a search of all 1,657 tool calls finds zero `Start-Process SkyCad*`. A stray SkyCAD process was only stopped once.
- Method was **static**: managed-metadata analysis of `SkyCadKernel.dll` / `SkyCadControls.dll` (241 classes, ~645 property definitions, ~214 queries) plus reading the on-disk environment data. No web research, no user screenshots, no decompiled C#.
- Concepts came from that metadata + the on-disk data + **the user's own drawings** (34 DXF + 33 DWG + one AutoCAD Electrical `FC1000495_AFR.ACD`).

**Conclusion:** the specs are already distilled into `crates/elec` and its 121 tests. Re-installing SkyCAD is *not* required to resume. It becomes useful again when a *new* feature needs the reference model.

### What was done today
Installed **SkyCAD Electrical 1.3.65** (installer is Authenticode-valid, signed *SkyCAD Systems (9323-3617 Québec inc.)*) — the EULA was accepted to complete the install. Then inventoried what it ships:

| Asset | Size / count | Value to us |
|---|---|---|
| `C:\SkyCAD Environments\Standard Environment` | **2,241 files / 286.5 MB** | the real environment layout |
| `*.SkyCadFile` | **1,647 files / 195 MB** | .NET-serialized class + object model (catalogue, compositions, queries) |
| Catalogue class tree | `Graphical Element / graphical block` (Connection, Connection point, Cross-reference, Title block, List item, OPR…), `Net`, `Net branch`, `Library`, `work field classes` (Component, Accessory, Terminal…) | the object model an ECAD needs — closest thing to a written spec |
| `Template files\*.xlsx` | **11 report templates** | exact column sets a commercial ECAD ships (below) |
| `Icons\`, `Images\` | 559 images incl. `*_Layout.png` footprints | symbol/panel footprint references |

The report templates alone tell us what our reports must eventually carry:

- **BOM**: `Class name · Part number · Manufacturer · Description · Qty · Where used`
- **Parts List**: `Reference · Located in · Class name · Part Number · Manufacturer · Description`
- **Wire List**: `Number · Net · Length · Wire Size · Source · Destination`
- **Connection nets**: `Class name · Number · Connected elements`
- **IO List**: `Description · Connected Element · Through Terminal Strip · Owner`
- **Cable List**: `Description · Part Number · Manufacturer · Length · Conductor`

**Not present after install:** the six example projects the old session mentioned. They are created/downloaded on first run through the SkyCAD login (`kimpuhung53@live.com`), so if we want them someone must log in once.

**Licence caveat:** the SkyCAD EULA almost certainly forbids reverse-engineering for a competing product. Defensible use is *interoperability and data*: read the shipped class/report data, export from example projects, match file formats. Do not decompile the app to rebuild its internals, and do not ship its assets.

---

## 6. The missing data folder — the biggest practical gap

The previous machine kept a **separate, non-repo folder** `...\AyraCADElectrical\` with everything that makes the electrical commands usable:

```
library\            51 symbol DXFs + library.tsv (symbol, class, attributes, sheets, scale)
tests\              ~70 fixture DXFs + command batches (e2e_batch.txt, plc_batch.txt,
                    io6_batch.txt, pn3.txt, meta2_batch.txt, sheet_batch.txt) + read_*.py
import\Prints\DXF\  34 real sheets (FC1000495-xxx)
reports\            ecad-verification.md + screenshots
tools\              harvest_library.py, make_*_test.py, DXF readers
```

**None of it is on this PC** — searched. Consequences today:
- `ELECLIB` has nothing to load → palette shows "No symbols loaded" (it does *not* crash; the code refuses cleanly).
- Every headless verification batch in the handoff is unrunnable.
- The 34 real sheets that produced the credibility of §2 are gone.

**Good news:** there are **zero hard-coded absolute paths** in the electrical code (only an optional `%APPDATA%\AryaCAD\elec_library.txt` cache). Everything is argument-driven, so restoring the folder to *any* path restores full function — no code change needed.

**Recovery, best to worst:**
1. **Copy the folder from the old PC** (fastest, exact).
2. **Re-export `Prints.zip`** from the user's own ECAD/DWG tool and re-run `harvest_library.py` — the library is *derived*, so it can be regenerated from drawings.
3. **Regenerate from SkyCAD** — the catalogue now installed on this machine is a legitimate source for classes and symbols.

---

## 7. Recommended order of work

1. **Finish the build** ~~and record the result~~ — **done**, see §4. Keep `target\` off the repo and record the version/date in the verification report.
2. **Vendor the git dependencies** (`cargo vendor`) and commit `.cargo/config.toml` — the `acadrust` move must never break the build again.
3. **Restore the data folder** (§6). Until then every claim about the electrical commands is historical.
4. **Fix the app test suite** so `cargo test` exists: 4-arg `PlaceSymbolCommand::new`, delete `CmdResult::Finished` usage, add `#[derive(Debug, PartialEq)]` (or replace the assertions).
5. **Fix the dispatch bugs** (all small, all verified): add `ELECSHEET` / `ELECPROJNUM` / `ELECPROJNEW` / `ELECPROJOPEN` to the early-argument list; rename `ELECSIDES`↔`ELECTSIDES` consistently; give `ELECINSERT` a bare arm; fix the `WD_*` filter; fix the block-geometry ownership field.
6. **Then** the real product gaps, in the order a draftsman would hit them: pin-level connectivity → persistent electrical data in DXF (XDATA/extension dictionary) → configurable numbering → sheet-qualified cross-references → report templates matching the SkyCAD column sets in §5.

---

## 8. Evidence

- [`session-review.md`](session-review.md) — reconstruction from the archived transcript: 12,354 records, every command's verification status with quoted tool output, the corruption incident, the user's verbatim product statements.
- [`code-audit.md`](code-audit.md) — command inventory (32/31/31), line references, the two loader bugs, the 10 biggest gaps versus AutoCAD Electrical/SkyCAD.
- `reports\_tmp\` — the raw extraction scripts used for the transcript analysis (safe to delete).
