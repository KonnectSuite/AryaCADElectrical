# AryaCADElectrical — electrical-layer code audit

**Auditor:** code-auditor (delegated, read-only)
**Date:** 2026-11-05
**Repo:** `C:\Users\KNPhu\AryaCode\KonnectSuite\AryaCADElectrical` (single git commit `fdfe8b4`, clean tree)
**Method:** source inspection only (grep/read of the checkout, direct `rustc` build of the dependency-free `elec` crate). No source file was modified; this report is the only file written.
**Question:** verify or refute `handoff.md`, and measure how much of an "electrical CAD" layer is actually implemented and integrated.

---

## 0. Verdict summary

The electrical layer is **real code, cleanly written, but much thinner and less integrated than `handoff.md` implies**. It is a *command-line-only* library of drawing conventions (layers + block attributes + plain text) with a solid, tested domain crate for the arithmetic (numbering, nets, terminals, xrefs). It is not an ACADE/SkyCAD-class application layer: there is no pin model, no persistent electrical object data, and — critically — **the app's test target does not compile**.

| # | Handoff claim | Verdict | Evidence |
|---|---|---|---|
| 1 | `src/app/commands/elec.rs` "~4,200 lines" | **True** (4,487 physical lines / 4,196 non-blank) | §2 |
| 2 | The 1.56 GB corruption is gone; file is sane | **True** — 187,886 bytes, pure ASCII, max line 190 | §2 |
| 3 | `crates/elec` "3,366 lines, **121 unit tests**" | **Tests true (121/121 pass); line count low** — actual 3,706 lines | §3 |
| 4 | ELECSHEET fit-to-extent rewrite is "uncommitted and **unverified**" | **Present, committed, and readable** — but *not* runnable here, and it **double-executes** when given arguments | §2.4 |
| 5 | Commands that take a path are registered in the early-argument list | **Mostly true, 4 exceptions** — `ELECSHEET`, `ELECPROJNUM`, `ELECPROJNEW`, `ELECPROJOPEN` missing; `ELECSHEET` visibly breaks | §1.3 |
| 6 | Command table lists every electrical command | **Incomplete** — 3 commands exist that the handoff omits: `ELECWIRENUMBER`, `ELECPART`, `ELECATTR`; 0 commands claimed are missing | §1.1 |
| 7 | Electrical ribbon tab / palette modal wired | **True** — tab registered unconditionally, modal enum + view present | §4 |
| 8 | "App-level tests not re-run since the corruption" | **Worse than stated** — the app test target *cannot compile*: 3 independent errors in `src/modules/electrical/` | §2.3 |
| 9 | `ELECPANEL` fixed 14×10 boxes, alphabetical | **True** | §6 |
| 10 | Data folder needed at runtime | **Partly true** — no hard-coded data paths anywhere; the folder is argument-driven, and it is **absent from this checkout** | §5 |
| 11 | `ELECWIRE` is "polyline wire with junction dots" | **Overstated** — it is a chain of `LINE` segments; junctions are a separate command | §1.2 |
| 12 | `crates/elec` tests can be run with `cargo test -p elec` | **False on this machine** — workspace resolution needs the `acadrust` git rev `d6d7e132`, which is not cached and cannot be fetched | §3.2 |

Environment blocker the Lead must know about (not a code defect): **this machine cannot build or test the app crate.** See §5.4.

---

## 1. INVENTORY — every `ELEC*` command string in the tree

Search basis: all `"ELEC…"` string literals in `src/` (199 lines in `elec.rs`, plus ribbon and modal references). All electrical commands live in **`src/app/commands/elec.rs`** (4,487 lines, 63 `fn`s, one `impl OpenCADStudio` block) and dispatch through **`dispatch_elec` at `src/app/commands/elec.rs:4043`**, called from the shared dispatcher at **`src/app/commands/mod.rs:758`**.

"Registered" below means "listed in the autocomplete inventory": `inventory::submit!(crate::command::CommandRegistration { … })` at `src/app/commands/mod.rs:911`, electrical block at **`src/app/commands/mod.rs:1291-1322`**. Registration affects command-line completion, alias validation and suggestion only (`crate::command::all_registered_command_names()` consumers at `src/app/alias.rs:327`, `src/app/control/mod.rs:747`, `src/ui/ribbon/app_menu.rs:235`); it is **not** required for dispatch.

### 1.1 The table (32 command names found)

| Command | Implemented at | Dispatch arm | Registered | Early-arg list | What the code actually does |
|---|---|---|---|---|---|
| `ELECBOM` | `elec.rs:235` | `4045` (bare), `4103` (`ELECBOM <csv>`) | `mod.rs:1292` | yes `utilities.rs:1085` | Bill of material of the active drawing; prints lines, optional CSV with the shipped templates' headers (`:4276`). |
| `ELECLIST` | `elec.rs:300` | `4046` | `1293` | n/a | Lists the components the extractor found (one line each). |
| `ELECNUMBER` | `elec.rs:332` | `4097` | `1294` | n/a | Renumbers device tags on the current drawing: grouped by class, reading order, prefix inferred or from IEC class letter; writes tag attributes; undo snapshot at `:499`. |
| `ELECNUMBERPREVIEW` | `elec.rs:332` (`dry_run=true`) | `4101` | `1295` | n/a | Same as `ELECNUMBER` but reports without writing. |
| `ELECWIRELIST` | `elec.rs:522` | `4047` | `1296` | n/a | Derives nets from drawn LINE/LWPOLYLINE geometry and prints the wire list. |
| `ELECWIRENUMBER` | `elec.rs:657` | `4048` | `1297` | n/a | Numbers nets in reading order and writes a `TEXT` label on layer `WIRENO` at the net's first wire midpoint; updates labels in place on re-run. **Not in the handoff.** |
| `ELECTERMINALS` | `elec.rs:902` | `4049` | `1298` | n/a | Groups components tagged `strip:index` (`X1:12`) into terminal strips; prints strips + reachable devices. |
| `ELECTSIDES` | `elec.rs:1786` | `4050` | `1299` | n/a | Per-terminal side A/B report (walks wires without crossing the terminal). |
| `ELECIO` | `elec.rs:2994` | `4051` | `1300` | n/a | Per PLC point: address → device on that conductor, using `X1TERMnn`/`X2TERMnn` coordinates recorded by `ELECPLC`. |
| `ELECXREF` | `elec.rs:982` | `4095` | `1301` | n/a | Reports multi-part devices (same tag, several parts) with class and position. |
| `ELECXREFWRITE` | `elec.rs:1154` | `4096` | `1303` | n/a | Writes cross-reference `TEXT` on layer `XREF` beside each part; in-place update on re-run; undo snapshot `:1225`. |
| `ELECPART` | `elec.rs:1081` | `4109` (`ELECPART <csv>`) | `1302` | yes `:1086` | Loads a parts-catalogue CSV into a session-global `Catalogue`. **Not in the handoff.** |
| `ELECREPORT` | `elec.rs:1250` | `4115` (`ELECREPORT <xlsx>`) | `1304` | yes `:1087` | Writes a 3-sheet xlsx (BOM / wire list / terminals) via the hand-rolled writer at `:4330`. |
| `ELECINSERT` | `elec.rs:1329` | `4253` (`ELECINSERT <block> [part]`) | `1305` | yes `:1088` | Inserts an existing block definition, tags it from the class, fills part/manufacturer/description from the catalogue. **Bare form is unhandled → "Unknown command"** (see §4.3). |
| `ELECPALETTE` | inline `elec.rs:4077-4094` | `4077` | `1306` | n/a | Opens `ModalKind::ElectricalLibrary`; auto-loads the remembered library if none is loaded. |
| `ELECPLACE` | `elec.rs:1865` | `4066` (bare → returns `None`), `4134` (`ELECPLACE <block>`) | `1307` | yes `:1095` | Starts `PlaceSymbolCommand` click-to-place with the library attribute template, next free tag, catalogue values, label height. |
| `ELECSETATTR` | `elec.rs:1949` | `4127` (`ELECSETATTR <tag> k=v`) | `1308` | yes `:1096` | Sets attributes on every insert carrying that tag; undo snapshot `:2004`. |
| `ELECNEW` | `elec.rs:2490` | `4058` (bare), `4241` (`ELECNEW <template.dxf>`) | `1309` | yes `:1090` | Prepares a new electrical drawing: layers, setup, optional template DXF copy, auto-loads remembered library. |
| `ELECPLC` | `elec.rs:2827` | `4187` (`ELECPLC <tag> <points> <base> [x,y]`) | `1310` | yes `:1091` | Builds a generic `PLC-<n>` block on the fly (body + one pin per point, `:2855-2890`), inserts it, writes `ADDR01..ADDRnn` and `X1TERMnn`/`X2TERMnn`; max 128 points. |
| `ELECPLCNEXT` | `elec.rs:3165` | `4165` | `1311` | yes `:1092` | Continues I/O addressing across the project's sheets (highest address already used). |
| `ELECPANEL` | `elec.rs:3263` | `4159` (`ELECPANEL <out.dxf>`) | `1312` | yes `:1093` | Writes a separate panel DXF: one 14×10 box per tagged device on a 20-unit grid, alphabetical (`:3285`), optional class text. |
| `ELECPROJNEW` | `elec.rs:3362` | `4152` (`ELECPROJNEW <folder> [file]`) | `1313` | **no** (harmless; no bare arm) | Writes `project.eprj` (plain text) listing the folder's DXFs. |
| `ELECPROJOPEN` | `elec.rs:3422` | `4146` (`ELECPROJOPEN <file.eprj>`) | `1314` | **no** (harmless) | Opens a project file into the session. |
| `ELECPROJLIST` | `elec.rs:3457` | `4143` | `1315` | n/a | Lists the open project's sheets. |
| `ELECPROJNUM` | `elec.rs:3576` | `4214` (bare = preview), `4217` (`ELECPROJNUM apply`) | `1316` | **no** → preview runs twice for `apply` | Project-wide renumber; with `apply` writes tags back per sheet and copies each to `.dxf.bak` (`:3781`). |
| `ELECPROJECT` | `elec.rs:2607` | `4221` (open project), `4225` (`<folder> [out.xlsx]`) | `1318` | yes `:1094` | Scans every sheet, aggregates devices/parts/classes, optionally writes the project workbook (`write_project_workbook:2777`). |
| `ELECSHEET` | `elec.rs:3814` | `4205` (arg form), `4211` (bare) | `1317` | **no** → **double frame** (see §2.4) | Draws paper border + inner frame + title block sized to the drawing's extent on layer `ELEC-FRAME`. |
| `ELECWIRE` | `elec.rs:2073` | `4059` | `1319` | n/a | Starts `WireCommand` (`src/modules/electrical/wire.rs:48`): chained `LINE` segments committed on layer `WIRES`. |
| `ELECJUNCTIONS` | `elec.rs:2089` | `4060` | `1321` | n/a | Places `CIRCLE` dots of r=1 on layer `JUNCTIONS` wherever ≥3 wire ends meet (or an end lands on a wire); dedupes existing dots. |
| `ELECMAKESYMBOL` | `elec.rs:2229` | `4054` (bare = hint), `4121` (`<name>`) | `1320` | yes `:1097` | Defines a block from the selection, writes `library/<name>.dxf` and appends to `library.tsv`; originals are not consumed. |
| `ELECLIB` | `elec.rs:1614` | `4247` (`ELECLIB <folder>`) | `1322` | yes `:1089` | Loads every `*.dxf` in the folder as a block definition, reads `library.tsv`, fabricates missing attribute definitions, measures label heights, remembers the folder. |
| `ELECATTR` | none (message only) | `4061` | **not registered** | n/a | Prints "use ELECSETATTR". **Not in the handoff; not a real command.** |

### 1.2 Counts and reachability

* **32** distinct `ELEC*` names are handled by `dispatch_elec`.
* **31** are registered for autocomplete (`ELECATTR` is not).
* **31** are real features (`ELECATTR` is the only hint stub; `ELECPALETTE` counts as real even though it is implemented inline in the dispatcher rather than as its own method), and those 31 are exactly the 31 registered names.
* **22** names have a bare dispatch arm. Of those, **19 perform their function with no arguments**; 2 only print a usage hint (`ELECMAKESYMBOL` bare, `ELECATTR`); 1 loads the library and then returns `None` (bare `ELECPLACE`, so typing it alone reports "Unknown command").
* **10** names have **no bare arm at all** and are reachable only with arguments: `ELECLIB`, `ELECINSERT`, `ELECPART`, `ELECREPORT`, `ELECSETATTR`, `ELECPANEL`, `ELECPLC`, `ELECPLCNEXT`, `ELECPROJNEW`, `ELECPROJOPEN`. (22 + 10 = 32.)
* **Ribbon** raises 14 tools (`src/modules/electrical/mod.rs:26-128`): 12 are `ELEC*`, 2 are the generic `ATTDEF`/`ATTEDIT`. **13 of 14 work**; `ELECINSERT` is broken (§4.3).
* **19** working commands have **no ribbon button at all**: `ELECLIB`, `ELECPLACE`, `ELECPART`, `ELECREPORT`, `ELECPROJECT`, `ELECPROJNEW/OPEN/LIST/NUM`, `ELECPANEL`, `ELECPLC`, `ELECPLCNEXT`, `ELECIO`, `ELECSHEET`, `ELECWIRE`, `ELECJUNCTIONS`, `ELECSETATTR`, `ELECNEW`, `ELECMAKESYMBOL`. (The one ribbon button for a command with no bare form, `ELECINSERT`, is broken — §4.3.)

### 1.3 Handoff cross-check

**Commands the handoff claims that are NOT found in code: none.** All 29 names it lists exist in the dispatcher.

**Commands in code that are NOT in the handoff (3):**

| Command | Why it matters |
|---|---|
| `ELECWIRENUMBER` | A real feature (net labels on layer `WIRENO`) and a ribbon button. The handoff's §10 "wire numbering formats" next step never mentions it exists. |
| `ELECPART` | A real feature (parts catalogue CSV) with a dispatch arm, registration and early-arg entry. |
| `ELECATTR` | Hint-only stub, unregistered — correctly omitted, but it is still a dispatch arm a reader will meet. |

**Overstatements in the handoff's descriptions:**

* `ELECWIRE` "polyline wire with junction dots" — the tool commits **individual `LINE` segments** (`src/modules/electrical/wire.rs:32-39, 60-72`); no LWPOLYLINE is created, and junction dots are a separate command (`ELECJUNCTIONS`).
* `ELECSHEET` is described as "frame + title block"; the code also fits the frame to the drawing extent and pulls sheet number/title from the project file — i.e. it does more than the row says, while doing it in a way that is broken for argument invocations.
* `crates/elec` is "3,366 lines"; the real figure is **3,706** lines across 11 modules (`crates/elec/src/*.rs`).

### 1.4 Argument-handling mechanism (and where it fails)

`run_command_line` (`src/app/command_driver/utilities.rs:1015-1017`) calls `run_command_line_impl(cmd, finish=true, chain=true)`. For a line with more than one token, if the verb is **not** in the early-argument allow-list at `utilities.rs:1067-1099`, the driver first **probes the bare verb** (`utilities.rs:1103`:

```rust
let Some(start_task) = self.try_dispatch_command(tokens[0], false) else { … };
```

), and only then dispatches the full line (`utilities.rs:1113`). Any command that both (a) has a bare-form arm and (b) is missing from that list executes twice.

Electrical entries in the list: `ELECBOM, ELECPART, ELECREPORT, ELECINSERT, ELECLIB, ELECNEW, ELECPLC, ELECPLCNEXT, ELECPANEL, ELECPROJECT, ELECPLACE, ELECSETATTR, ELECMAKESYMBOL` (`utilities.rs:1085-1097`).

**Missing but argument-taking:** `ELECSHEET`, `ELECPROJNUM`, `ELECPROJNEW`, `ELECPROJOPEN`.

Consequences, by code path:

* `ELECSHEET A3 Motor panel` → probe draws the **default A4 frame** (`dispatch_elec` `cmd == "ELECSHEET"` → `elec_sheet(i,"A4","")` at `:4211`), then the full line draws the **A3 frame** (`:4205`). Two frames, two undo snapshots, and the first one is measured as content by the second (it is on `ELEC-FRAME` only for the *extent* filter, not for the writing). **This is the one destructive instance.**
* `ELECPROJNUM apply` → dry-run preview executes, then "apply"; wasteful (two project scans) but not destructive.
* `ELECPROJNEW` / `ELECPROJOPEN` → probe returns `None`, so the full line dispatches normally: no double execution, just a wasted probe.
* Dead code: `elec.rs:4140-4142` re-tests `cmd == "ELECNEW"` inside the `_` arm although `"ELECNEW"` is already matched at `:4058`; unreachable.

---

## 2. QUALITY CHECK — `src/app/commands/elec.rs`

### 2.1 Size and corruption

| Metric | Value |
|---|---|
| Bytes | **187,886** (0.19 MB) |
| Physical lines | **4,487** (4,196 non-blank) |
| `fn` declarations | **63** (one `impl OpenCADStudio` block) |
| `#[test]` in the file | **0** |
| Longest line | **190** chars |
| Non-ASCII bytes | **0** (pure ASCII) |
| Mojibake (`Ã¢â‚¬`) | **0** |

This is the decisive answer to the 1.56 GB / 736-million-character-line incident described in `handoff.md` §8: the file is now **three orders of magnitude smaller than the corrupt version** and shows no residue. Repo-wide, the longest source line is 6,057 chars in `src/mcp.rs` (pre-existing upstream code), then 1,751 in `src/io/tds_legacy.rs`; nothing near a corruption signature. Note that the *base* OpenCAD sources do contain pre-existing mojibake in comments (e.g. `src/app/commands/mod.rs:168` "auditsâ€¦"), but `elec.rs` itself is clean.

### 2.2 Stubs, duplication, dead code

* **No `TODO`, `TODO(`, `FIXME`, `XXX`, `HACK`, `stub`, `todo!()`, `unimplemented!()` or `panic!` anywhere in `elec.rs`** (grep: 0 matches). The only panic-like sites are `unwrap_or*` fallbacks and the `expect`-free style; the file is written in a defensive "log to the command line and return" style throughout.
* **No duplicated function definitions** (63 declarations, 63 distinct names).
* **Dead code:** unreachable `if cmd == "ELECNEW"` in the `_` arm (`elec.rs:4140-4142`).
* **Stale/duplicated doc comments:** `elec.rs:2454-2456` carries two consecutive doc paragraphs, one describing the older `(name, class)` return shape; `elec.rs:2372-2378` repeats the doc of the palette helper.
* **Odd but legal structure:** `fn components_of` is written as an *associated function inside* `impl OpenCADStudio` (`elec.rs:178-223`), with the doc block that reads as if it belonged to `elec_components` (`:226`), which itself has no doc.
* **Bug — wrong block-ownership field:** `elec_lib`'s fallback for symbol files whose geometry lives inside a block uses
  ```rust
  let handle = record.block_entity_handle;
  entities = document.entities()
      .filter(|entity| entity.common().owner_handle == handle)   // elec.rs:1669-1673
  ```
  but the same file documents the correct rule 700 lines later:
  ```rust
  // Block contents are owned by the block **record** (or listed in its
  // entity_handles); `block_entity_handle` is the BLOCK entity itself. …
  let record_handle = record.handle;
  let entity_handles = record.entity_handles.clone();
  ```
  (`elec.rs:2386-2394`, used correctly in `elec_symbol_segments`). So for "harvested" symbol DXFs that keep geometry in a block, `ELECLIB` will find zero entities and report `"<stem>: no geometry"` (`:1678-1680`), while the palette thumbnail path would find them. This is exactly the class of bug `handoff.md` §9 warns about, present in the code.
* **Over-broad filter:** `is_setup_block` (`elec.rs:4318-4321`) treats **any** block whose name starts with `WD_` as drawing setup:
  ```rust
  name == "WD_M" || name.starts_with("WD_")
  ```
  used at 11 call sites (`:196, 353, 568, 696, 876, 994, 1118, 1349, 3633, 3743`). AutoCAD Electrical library block names frequently begin `WD_`, so such symbols would silently vanish from components, BOM, wire nets and numbering.
* **`is_setup_block` is the *only* use of `WD_M` in the layer** — the code never *reads* the electrical settings stored there (tag format, wire-number format, ladder config). The doc comment (`:4315-4317`) implies reading; the code only filters.

### 2.3 Does it compile conceptually?

**Symbol resolution looks sound.** Every app-side symbol `elec.rs` names exists in this checkout:

* `super::{Message, OpenCADStudio}` and `::elec::{bill_of_materials, Component}` (`:11-12`); all 14 `::elec::…` item paths used (`Catalogue`, `BomRow`, `Cell`, `Sheet`, `PlacedPart`, `junction_points`, `terminal_sides`, `terminal_strips`, `parse_terminal_tag`, `next_tag`, `default_prefix`, `infer_tag_prefix`, `reading_order_key`, `geometry::…`) are exported by `crates/elec/src/lib.rs:25-38`.
* `codec::entities::Line::from_points`, `codec::entities::Text::new`, `codec::entities::AttributeDefinition::new`, `codec::entities::AttributeEntity::new`, `codec::types::Vector3::new`, `codec::tables::Layer::new` are used hundreds of times elsewhere in the crate (340 / 8 / 501 hits respectively), and `common.layer = "…".to_string()` matches the convention in `src/app/commands/blocks.rs:2551`.
* `self.push_undo_snapshot(i, "ELECSHEET")` matches `src/app/history.rs:453` (`impl Into<HistoryLabel>`); `self.tabs[i].scene.add_entity(...)`, `scene.define_block_from_owned_entities(...)`, `reset_modal_geometry()` (`src/app/update/mod.rs:154`), `ModalKind::ElectricalLibrary` (`src/app/mod.rs:3047`) all exist.
* The file references `zip::ZipWriter` (`:4334`) and `zip` is a dependency (`Cargo.toml:59`), and `codec` is the `opencaddocument` git dependency (`Cargo.toml:42`).

**But the app's `#[cfg(test)]` surface does not compile.** Three independent, unambiguous errors, all in the electrical modules:

1. `src/modules/electrical/place.rs:102` and `:132` call
   ```rust
   PlaceSymbolCommand::new("HCR1".to_string(), vec![ … ])
   ```
   while the only constructor takes four arguments (`place.rs:35-40`: `block, attributes, height, scale`). → **E0061, wrong number of arguments.**
2. `src/modules/electrical/place.rs:133` asserts `CmdResult::Finished`, a variant that **does not exist**. `CmdResult` (`src/command.rs:1813`) has `NeedPoint`, `CommitEntity`, `CommitAndExit`, `Done` (the only "finished" token, `:2394`) and many payload variants; `Finished` appears in the crate only inside a doc comment at `src/command.rs:1965`. → **E0599, no variant or associated item.**
3. `src/modules/electrical/wire.rs:86-89` and `:98-114` write `assert_eq!(command.on_point(...), CmdResult::NeedPoint)` and `… "got {other:?}"`. `CmdResult` is declared with `#[allow(dead_code)]` and **no derives at all** (`src/command.rs:1812-1813`), and there is no manual `impl PartialEq` / `impl Debug` for it anywhere in `src/`. → **E0369 (binary operation `==` cannot be applied) and E0277 (`CmdResult: Debug` not satisfied).**

These are test-only (`#[cfg(test)] mod tests`, `place.rs:96`, `wire.rs:79`), so `cargo build` is unaffected — which is exactly why the handoff could believe "tests not re-run" while the truth is "tests cannot be built". This also refutes the handoff's §10 step 5 ("re-run `cargo test --release` for the app and record the result"): it will fail at compile time in its current state.

### 2.4 `ELECSHEET` — exact behaviour and verdict

**Location:** `src/app/commands/elec.rs:3814-4038`. **State:** the fit-to-extent rewrite is *in the tree and committed* (git tree clean; the single import commit contains it), not "uncommitted and unverified" as the handoff states.

**Extent scan (`:3826-3877`):** walks `doc.entities()`, keeps only entities whose `owner_handle == doc.header.model_space_block_handle` (`:3840`, block geometry excluded), skips anything on layer `ELEC-FRAME` (`:3845`), and accumulates a bounding box over `Line` (both endpoints), `Circle`/`Arc` (centre ± radius, so arcs are over-estimated by their full circle), `Text` (insertion point only, no height), `Insert` (insertion point only, no block extents) and `LwPolyline` (vertices). Everything else is ignored — no Hatch, Spline, Ellipse, MText, Dimension, Leader, Solid, Ray/Xline.

**Scale logic (`:3879-3898`):**
```rust
let scale = ((width * 1.15) / paper_w).max((height * 1.15) / paper_h);
let frame_w = paper_w * scale;
let frame_h = paper_h * scale;
let mm = |value: f64| value * scale;      // 1 mm of paper, in drawing units
```
So it is a uniform paper→drawing-unit multiplier with 15 % padding, using the larger of the two ratios (frame fits the content). A4 = 297×210 (`:3820`), A3 = 420×297, A2 = 594×420, A1 = 841×594, A0 = 1189×841. Empty or degenerate (width==0 or height==0) drawings fall back to **1:1 around the origin** (`.max(1e-9)` at `:3880-3881` guards div-by-zero; the branch at `:3889-3892` returns `paper_w/2, paper_h/2, 1.0`).

**Geometry actually drawn (`:3936-4008`), layer `ELEC-FRAME` (`:4016`, `:4024`):**

| Item | Coordinates |
|---|---|
| Paper edge | one rectangle, `(origin_x, origin_y)` … `+frame_w, +frame_h` (`:3942-3945`) |
| Inner frame | 10 mm inset rectangle (`:3946-3950`) |
| Title block | 120 × 32 mm box at bottom-right inside the inset (`:3953-3960`) |
| Title-block rules | three horizontal lines at +20 / +12 / +6 mm, plus one vertical split at 65 % of the width down to the +6 mm rule (`:3961-3968`) |
| Texts (5) | `SHEET <n>` at 6 mm height; sheet title (uppercased) at 4 mm; project name (or `ARYACAD ELECTRICAL`) at 3 mm; `"<SIZE>  W x H"` at 2.5 mm; `1:<N>` at 2.5 mm (`:3970-4008`) |

**Sheet metadata (`:3900-3934`):** if a project file is open, the current tab's file name is matched against `project.eprj` entries; `number` falls back to the 1-based index, `title` to the project title. Otherwise the `title` argument is used, else `UNTITLED` / `SHEET`.

**Verdict:** the rewrite is **implemented, self-consistent and sane**, and it does solve the stated problem (a 35×25-unit drawing gets a ~40.7×28.7-unit frame at 1:7 instead of a 297×210 monster). It is **not verified**: this checkout cannot build (§5.4), so no runtime observation exists in the record, and the handoff's own note that "the full build was interrupted" still stands. Three real defects remain in it: (a) the argument form **draws two frames** because it is missing from the early-argument list (§1.4); (b) the written sheet identity is **plain `TEXT`, not attributes**, so a re-run or any report cannot recover it, and nothing persists the sheet number in the drawing; (c) nothing integrates with page setup / plot scale, so the "1:N" label is decorative and printing the frame at true size is not possible.

---

## 3. `crates/elec` — the domain crate

### 3.1 Modules, public surface, tests

`crates/elec/Cargo.toml` has **no dependencies**, and the crate builds standalone (verified, see §3.2).

| Module | Lines | `pub fn` | `pub struct/enum` | `#[test]` | Purpose (from module docs) |
|---|---|---|---|---|---|
| `catalogue.rs` | 340 | 5 | 3 | **14** | Parts catalogue CSV (part/manufacturer/description) for BOM enrichment |
| `component.rs` | 296 | 9 | 2 | **8** | `Component` + `bill_of_materials` roll-up |
| `expr.rs` | 660 | 3 | 9 | **16** | `<r:…>` label/expression template language |
| `geometry.rs` | 351 | 5 | 6 | **9** | Geometric netlist: wires → nets, component anchors |
| `junction.rs` | 178 | 1 | 0 | **8** | Junction-dot detection (≥3 meeting, or endpoint on wire) |
| `netlist.rs` | 305 | 5 | 4 | **11** | Explicit `Wire`/`Port`/`Net` netlist builder |
| `numbering.rs` | 538 | 11 | 6 | **20** | Tag/wire numbering formats, prefixes, renumber, reading order |
| `report.rs` | 234 | 9 | 2 | **7** | Report sheets (`Cell`/`Sheet`), BOM/wire/terminal sheets |
| `sides.rs` | 277 | 3 | 2 | **7** | Terminal side A/B nets |
| `terminal.rs` | 181 | 2 | 3 | **8** | Terminal-strip grouping from `strip:index` tags |
| `xref.rs` | 299 | 7 | 3 | **13** | Cross references / multi-part devices |
| `lib.rs` | 47 | — | — | 0 | re-exports of all of the above |
| **Total** | **3,706** | 60 | 44 | **121** | |

`lib.rs:13-38` exports: `Catalogue, CatalogueError, PartRecord; bill_of_materials, BomRow, Component; netlist_from_geometry, wire_rows, Anchor, GeoNet, GeoNetlist, GeoWireRow, Point, WireSeg; junction_points; build_nets, wire_list, Net, Port, Wire, WireRow; terminal_sides, SideNet, TerminalSides; bom_sheet, terminal_sheet, wire_sheet, wire_sheet_from, Cell, Sheet; Branch, Compare, Conditional, Join, Part, Ref, Resolve, Step, Template, Value; cross_references, multi_part, DeviceRef, PartRef, PlacedPart; parse_terminal_tag, terminal_strips, StripTerminal, TerminalRef, TerminalStrip; default_prefix, infer_tag_prefix, next_tag, reading_order_key, renumber, Alphabet, Kind, Numbered, NumberingFormat, Renumbered, Uniqueness`.

**Only one file in the app consumes this crate:** all 39 `::elec::` references are in `src/app/commands/elec.rs`. Consequences:

* **`expr.rs` is completely unwired.** Neither `::elec::expr` nor any of its exports (`Template`, `Resolve`, `Branch`, `Step`, `Conditional`, …) is referenced anywhere in `src/`, `plugins/`, `benches/` or `tests/` (grep: 0). 660 lines and 16 passing tests of label-language implementation that no command can reach. The handoff presents labels as part of the product.
* **`netlist.rs`'s `build_nets` / `wire_list` are unused**; the app uses `geometry::netlist_from_geometry` instead (5 call sites).
* `report::wire_sheet` is unused (`wire_sheet_from` is used); `xref::DeviceRef` / `PartRef` are unused (the app uses `PlacedPart` + `multi_part`); `numbering::{Alphabet, Kind, Uniqueness}` are not selected by the UI — only `NumberingFormat::tag()` / `::wire()` constructors are.

### 3.2 Test run — result and the command the task specified

The requested command **fails before it compiles anything**, because `cargo test -p elec` still resolves the whole workspace and its git dependencies:

```
$env:CARGO_TARGET_DIR="$env:TEMP\elec-audit-target"; cargo test -p elec
    Updating crates.io index
    Updating git repository `https://github.com/HakanSeven12/acadrust.git`
error: failed to get `acadrust` as a dependency of package `opencaddocument v0.1.0`
  ...
  Unable to update https://github.com/HakanSeven12/acadrust.git?rev=d6d7e132#d6d7e132
  Caused by: revision d6d7e13224af4f3b6978cb3a3ccc8c4cce8b3b11 not found
  Caused by: failed to send request: No credentials were available in the client certificate.
```
`cargo test --offline -p elec` also fails (`error: no matching package named 'bincode' found` — the registry cache holds 50 crates against a 790-package lockfile). This is an environment/dependency-cache problem, **not a defect in `elec`**.

Because `crates/elec` is dependency-free, the suite was built and run directly with the compiler instead:

```
rustc --edition 2021 --test crates\elec\src\lib.rs -o %TEMP%\elec-audit-tests.exe
(EXIT=0, 1,807,872-byte test harness)
… 121 tests …
test result: ok. 121 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Result: 121 passed, 0 failed, 0 ignored — exactly the handoff's number, verified.** Per-module counts in §3.1. (The crate compiles cleanly under rustc 1.99.1 with no warnings surfaced in this build.)

---

## 4. INTEGRATION — is the layer reachable at runtime?

### 4.1 Ribbon tab — reachable, unconditional

`src/modules/mod.rs:30` declares `pub mod electrical;`. `src/modules/registry.rs:16-29` instantiates the tab last:

```rust
pub fn all_modules() -> Vec<Box<dyn CadModule>> {
    vec![ …, Box::new(super::electrical::ElectricalModule) ]
}
```

`ElectricalModule` (`src/modules/electrical/mod.rs:12-129`) implements `CadModule` with three groups — Numbering (2 tools), Components (5), Reports (7). **No `#[cfg]` gate, no feature flag, no runtime enable check** anywhere in `elec.rs`, `modules/electrical/*` or `registry.rs` (grep for `#[cfg` in the electrical files finds only the two `#[cfg(test)]` module markers). The tab appears for every user on every build.

### 4.2 Palette modal — fully wired

| Link | Location |
|---|---|
| `ModalKind::ElectricalLibrary` variant | `src/app/mod.rs:3047` (with doc comment "The electrical symbol library, browsed and inserted from.") |
| Window title | `src/app/view/modal.rs:29` |
| View dispatch (520×620 flow) | `src/app/view/modal.rs:718-723` |
| View body | `src/ui/window/elec_library.rs:113-153` — scrollable list grouped by class, each row a 64×40 canvas thumbnail + name + `"<class> · <n> seg"` |
| Module declaration | `src/ui/window/mod.rs:23` |
| Open path | `dispatch_elec` `:4077-4094` sets `active_modal` and resets modal geometry |
| Click → insert | `elec_library.rs:21-23`: `Message::Command(format!("ELECPLACE {symbol}"))` |

The thumbnail is real geometry: `elec_symbol_segments` (`elec.rs:2379-2450`) walks the block record's owned entities and poly-lines Line/Circle/Arc/LwPolyline into ≤400 segments.

**Gaps in the modal:** no search/filter box, no metadata (part, description), no multi-select, no drag-and-drop onto the canvas, and the row shows a debug counter (`"· n seg"`, `elec_library.rs:95`) with a comment admitting "while previews are being finished".

### 4.3 Dispatch reachability defects

1. **`ELECINSERT` bare is unhandled.** The match has no `"ELECINSERT"` arm; only `strip_prefix("ELECINSERT ")` at `elec.rs:4253`. The ribbon's "Insert Symbol" tool dispatches exactly `"ELECINSERT"` (`src/modules/electrical/mod.rs:59-64`), which falls through `dispatch_elec` → `None` → `dispatch_command_inner` prints `Unknown command: ELECINSERT` (`src/app/commands/mod.rs:197-204`). The button is dead. (A user who types `ELECINSERT BLOCKNAME` gets the working path.)
2. **Bare `ELECPLACE` returns `None`** (`:4066-4076`) after opportunistically loading the library. Typed alone it reports "Unknown command". Harmless for the palette (which always appends a symbol) but confusing.
3. **Bare `ELECLIB`, `ELECPART`, `ELECREPORT`, `ELECPANEL`, `ELECPLC`, `ELECPLCNEXT`, `ELECPROJNEW`, `ELECPROJOPEN`, `ELECSETATTR` all return `None`** with no hint of the required syntax (contrast `ELECMAKESYMBOL`/`ELECATTR`, which do print usage hints). A user who types `ELECLIB` gets "Unknown command: ELECLIB" rather than "ELECLIB <folder>".
4. `ELECSHEET` / `ELECPROJNUM` double-dispatch — §1.4.

### 4.4 Command registry, locale, and the Start tab

* **Registration is not required to run a command**, and **no locale/i18n string is needed for a command to appear.** The `CommandRegistration` list is a plain `&[&str]` of names consumed by autocomplete (`src/app/control/mod.rs:747`), alias validation (`src/app/alias.rs:327`) and the ribbon's applicable-command table (`src/ui/ribbon/app_menu.rs:235`). There is no test that enforces dispatchability of registered names.
* **No electrical strings are localized.** `src/locale_catalog.rs` has **zero** `ELEC*` entries (the 550 apparent grep hits are the substring inside "SELECT"). Ribbon labels are hard-coded English `&'static str` (`src/modules/electrical/mod.rs:32, 38, 49, …`), which matches the convention in every other module (`src/modules/draw/mod.rs:57…`), so the Electrical tab is English-only in all locales. Command-line prompts from `WireCommand` do use `t!()` (`src/modules/electrical/wire.rs:55-56`), but those keys have no catalogue entry either, so `t!` falls back to the key.
* **Start tab gate:** `start_allowed` (`src/app/commands/mod.rs:870-902`) allows only document lifecycle / app-wide verbs; no `ELEC*` verb is listed, so electrical commands are **refused while the Start (welcome) tab is active**. This is correct behaviour, but it means the tab's buttons are dead until a drawing is open — a first-run UX trap.
* **Icon bug:** the Components group's Insert Symbol tool uses the string literal `"\\u{2795}"` (`src/modules/electrical/mod.rs:62`) — a backslash-`u` escape inside a normal string, so the button renders the literal text `\u{2795}` instead of ➕. Every neighbouring glyph uses a single backslash and renders correctly.

---

## 5. RUNTIME DATA DEPENDENCIES AND HARD-CODED PATHS

### 5.1 Hard-coded absolute paths — **none**

A full grep of `src/app/commands/elec.rs`, `src/modules/electrical/**`, `src/ui/window/elec_library.rs` and `crates/elec/**` for `[A-Za-z]:\`, `/Users/`, `HomeAdmin`, `APPDATA`, `LOCALAPPDATA`, `ProgramData` returns **no data paths**. The only machine-bound path is a config pointer:

```rust
// src/app/commands/elec.rs:41-44
fn library_config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)?;
    Some(base.join("AryaCAD").join("elec_library.txt"))
}
```

`%APPDATA%\AryaCAD\elec_library.txt` stores one line — the last `ELECLIB` folder (`remember_library:46-54`, `remembered_library:56-65`). It is a *cache*, not a requirement: if absent, `ELECPALETTE`/`ELECPLACE`/`ELECNEW` simply do not auto-load, and the palette shows "No symbols loaded. Load a library with ELECLIB <folder>…" (`src/ui/window/elec_library.rs:114-130`). That string is the only place the data folder is named in prose ("the harvester writes one into AyraCADElectrical\library"), and it is display text, not a path lookup.

### 5.2 What the code needs at runtime (all argument-driven)

| Feature | Needs | Looked for | If missing |
|---|---|---|---|
| `ELECLIB <folder>` | folder containing `*.dxf`; optionally `library.tsv` (tab-separated `symbol \t class \t attrA\|attrB \t scale`, `elec.rs:120-161`) | exactly the path given (`:1615-1620`) | `ELECLIB: <folder> is not a folder.` — clean refusal |
| `ELECINSERT` / `ELECPLACE` | a block definition, normally created by `ELECLIB` | session state | `ELECINSERT: no block named "…" in this drawing (available: …). Load a symbol library with ELECLIB <folder>.` (`:1411`) |
| `ELECPART <file.csv>` | catalogue CSV (`Part number, Manufacturer, Description`, extra columns ignored) | path given | error line, session catalogue unchanged |
| `ELECNEW [template.dxf]` | optional template drawing | path given | works without it |
| `ELECPROJECT <folder> [out.xlsx]`, `ELECPROJNEW`, `ELECPROJNUM apply` | a folder of `*.dxf` / an `.eprj` / `.dxf.bak` write access | path given | `ELECPROJECT: <folder> is not a folder.` / `no .dxf drawings in …` |
| `ELECREPORT <xlsx>`, `ELECBOM <csv>`, `ELECPANEL <dxf>` | an output path | path given | write error printed |
| `ELECSHEET`, `ELECNUMBER`, `ELECWIRENUMBER`, `ELECJUNCTIONS`, `ELECXREF*`, `ELECLIST`, `ELECWIRELIST`, `ELECTERMINALS`, `ELECTSIDES`, `ELECIO`, `ELECPLC*` | **nothing external** — drawing contents only | — | — |

So: **the data folder is not a build-time or startup dependency.** The app starts and every non-library command runs on a bare machine. Only the library/catalogue/project workflows need external files, and they fail with printed messages rather than panics — code that reads files uses `.ok()`/`match` and reports through `command_line.push_output`.

### 5.3 What is actually missing from this checkout

| Handoff's data folder item | Present here? |
|---|---|
| `library\` (51 symbol DXFs + `library.tsv`) | **No** — no `library/` directory exists |
| `tests\` electrical fixtures and batches (`e2e_batch.txt`, `plc_batch.txt`, `io6_batch.txt`, `pn3.txt`, `meta2_batch.txt`, `sheet_batch.txt`, `read_addrs.py`, `read_tags.py`, `read_panel.py`) | **No** — `tests/` contains only OpenCAD's own 27 `.rs` integration tests and its `fixtures/` (3 DXFs), no `.txt`, no Python, nothing electrical |
| `import\Prints\DXF\` (34 real sheets) | **No** — no `import/` directory |
| `tools\` (`harvest_library.py`, `make_*_test.py`) | **No** — no `tools/` directory |
| `reports\` (`ecad-verification.md`, screenshots) | Only `reports/_tmp/` — 50 text files of the *previous session's* tool logs (`apptest.txt`, `e0786.txt`, `acade.txt`, `timeline.txt`, …), not the verification report |
| Any `.eprj` project file | **No** — zero `.eprj` in the tree |
| Any electrical `.dxf` fixture | **No** — the only 3 DXFs are OpenCAD's dimension/plot/image fixtures |

**Conclusion for the Lead: the entire electrical data set must be copied from the other machine; nothing that the handoff's §6 verification recipes reference exists in this checkout.** Also note the handoff's data-folder spelling (`AyraCADElectrical`, "Ayra") differs from this repo (`AryaCADElectrical`), so whatever copy step is used must be checked by content, not by name.

### 5.4 Environment blocker: the app cannot be built or tested on this machine

Independently of the data folder, the checkout cannot be compiled here:

* **`target/` does not exist**, and no `cargo`/`rustc` process was running during this audit (so a release build here is a clean 35–40-minute build from zero, as `handoff.md` says).
* **One git dependency is not cached:** `~/.cargo/git/db/acadrust-34ed2f71ff2db249` exists but is an **empty bare repo** — the required revision `d6d7e13224af4f3b6978cb3a3ccc8c4cce8b3b11` (`Cargo.lock:100-102`) is absent. Every fetch attempt fails with Windows Schannel:
  ```
  fatal: unable to access 'https://github.com/HakanSeven12/acadrust.git/':
  schannel: AcquireCredentialsHandle failed: SEC_E_NO_CREDENTIALS (0x8009030e)
  ```
  (reproduced with `git ls-remote`; the other git deps — `iced`, `iced_aw`, `opencaddocument`, `opencadgraph`, `opencadkernel` — *are* cached and populated.)
* **The crates.io cache is incomplete:** `~/.cargo/registry` holds 50 crate archives / 52 index entries against **790** packages in `Cargo.lock`, which is why `--offline` resolution dies on `bincode`.

**What must therefore be copied or fixed before any build/run verification is possible:** (1) the electrical data folder (§5.3); (2) either the populated `~/.cargo/git/db/acadrust-*` (or `target/`) from the original machine, or working HTTPS to `github.com` (the Schannel failure is a machine TLS/credential problem, not a proxy settings issue — no proxy variables are set; `git -c http.sslBackend=openssl` could not be tested because Git's helper process is blocked by the sandbox). Only `crates/elec` (zero dependencies) builds here.

---

## 6. GAP ANALYSIS VS AN ACADE/SKCAD-CLASS PRODUCT

### 6.1 Capability matrix

| Capability | Status | Where / why |
|---|---|---|
| **Symbol library** (folder of DXFs + class/attribute manifest) | **Partial** | `ELECLIB` `elec.rs:1614-1780`; `library.tsv` reader `:120-161`; attribute *definitions* are **fabricated** at load because harvested symbols carry attributes on the insert (`:1694-1709`). No pin/connection-point definitions, no footprint, no multi-state/variant symbols, no manufacturer linkage, no library editor. |
| **Device tagging** | **Present (fixed format)** | Class-based prefix inference `numbering.rs:215-334`, `ELECNUMBER` `elec.rs:332-521`, project-wide renumber `:3576-3795` with `.bak` backups (`:3781`). Format is whatever `NumberingFormat::tag(&prefix)` returns (`:442`); no user-editable tag format, prefix table or uniqueness scope in the app (`Uniqueness` exists in the crate but is never selected). |
| **Wire numbering** | **Partial** | `ELECWIRENUMBER` `elec.rs:657-833` writes `TEXT` on layer `WIRENO` at the first wire's midpoint; format hard-coded via `NumberingFormat::wire()` (`:751`). No per-sheet/project formats (handoff's own next step 4), no wire-number attributes, no source/destination arrow system, no cable/core numbering. |
| **Terminal strips** | **Partial** | Derived from the `strip:index` tag convention (`crates/elec/src/terminal.rs`, `ELECTERMINALS` `elec.rs:902-980`), side analysis `sides.rs` / `ELECTSIDES` `:1786`. No strip definition, no terminal symbol family, no accessories/end plates/jumpers, no terminal table placed on the drawing. |
| **Cross references** | **Partial** | Report `xref.rs` / `ELECXREF` `:982`, written as `TEXT` on layer `XREF` (`:1154-1240`). Contact identity only via optional `PIN`/`CONTACT`/`CONTACTNUMBER`/`PARTNUMBER2` attributes (`:1133`); references are position-only (`PlacedPart::new(block, tag, x, y)` `:1134-1139`) so **no sheet qualification** — a multi-sheet device cannot say "sheet 4". No in-symbol contact xrefs, no automatic update on edit. |
| **PLC I/O** | **Partial / synthetic** | `ELECPLC` builds a generic rectangle-plus-pins block at runtime (`:2855-2890`), writes `ADDR01..ADDRnn` (max 128 points, `:2842`), `X1TERMnn`/`X2TERMnn`; `ELECPLCNEXT` continues across sheets `:3165`; `ELECIO` maps address → device `:2994`. No PLC module library, no rack/slot/chassis, no I/O type/description, no bus or network addressing. |
| **Project / sheet management** | **Minimal** | `project.eprj` plain text (`elec_proj_new:3362`, `read_project:3517`), sheet list `:3457`, project BOM workbook `:2607-2777`. No sheet properties in the drawing, no drawing-number/title-block attributes, no revision/approval, no sheet templates, no layouts, no UI beyond one command. |
| **Reports / BOM** | **Partial** | BOM/CSV (`:235`, header literal `:4276`), 3-sheet xlsx via a hand-written OOXML writer (`:4330-4429`), project workbook (`:2777`). No report templates, styles, formulas, no BOM table placed on the sheet/title block, no user-defined report formats. |
| **Panel layout** | **Prototype** | `ELECPANEL` `:3263-3355`: fixed `BOX_W=14`, `BOX_H=10` in a 20-unit grid, alphabetical order, no footprints, dimensions, DIN rail, enclosure, or wire routing. |
| **Drawing frames / title blocks** | **Geometry only** | `ELECSHEET` `:3814-4038`: border + inner frame + a fixed 120×32 mm box with 5 plain `TEXT`s. No frame library, no title-block *attributes*, no revision/appr/date fields, no plot-scale or page-setup integration. |
| **DXF/DWG round-trip of electrical data** | **Weakest link** | Everything is **layer convention + block ATTRIB + plain TEXT**. `XDATA` appears **0 times** in the electrical code (repo-wide XDATA exists in `src/scene/selection.rs` but is unrelated); no extension dictionary, no custom objects, no per-object electrical identity. See §6.2. |
| **Connectivity model** | **Symbol-level only** | One anchor per symbol, at its **insertion point** (`elec_collect` `:875-891`; `elec_wirenumber` `:695-711`). Multi-pin symbols cannot be netted per pin; pins exist only as PLC-module synthetic attributes. |

### 6.2 How the electrical model is stored in DXF (this determines interop)

| Data | Encoding | Survives a DXF round-trip? |
|---|---|---|
| Components | `INSERT` of a named block + attached `ATTRIB`s (`TAG1/CAT/MFG/DESC/LOC` or SkyCAD-style names, `field_for_attribute:167-176`) | Yes |
| Device class | from `library.tsv`, else block name (`elec_insert:1380-1382`) | Class is **not in the DXF** — a symbol placed from a library loses its class if the library is not loaded |
| Wires | `LINE` entities on layer `WIRES` (`wire.rs:16, 37`) | Yes (no wire semantics beyond the layer) |
| Junctions | `CIRCLE` r=1 on layer `JUNCTIONS` (`:2091-2092`) | Yes |
| Wire numbers | `TEXT` on layer `WIRENO` (`:662`), linked to the wire only by **position** | Text survives; the link does not |
| Cross references | `TEXT` on layer `XREF` (`:1157`), linked by position | Text survives; the link does not |
| Frame / title block | `LINE`/`TEXT` on layer `ELEC-FRAME` (`:4016`, `:4024`) | Yes, as dumb geometry |
| Panel | separate DXF, `PANEL`/`PANEL-TEXT` layers (`:3294-3295`) | Yes |
| PLC point addresses | `ADDRnn` + `X1TERMnn`/`X2TERMnn` attributes on the module insert (`:2897-2920`) | Yes |
| Terminal membership | the **tag string** `X1:12` | Yes, but only if third-party software understands the convention |
| Project membership, sheet number/title | **outside the DXF** in `project.eprj`; `ELECSHEET` bakes a snapshot into `TEXT` | **Lost** without the `.eprj` |

Consequence: a drawing opened in any other CAD tool is plain geometry with attributes; nothing on the wire/junction/frame/text objects marks them as electrical. ACADE at least stores wire/ladder settings in `WD_M` and uses XDATA on some objects; SkyCAD keeps a separate project database. This layer has neither — the *only* database is the side-car `.eprj` plus `library.tsv`.

### 6.3 The ten biggest functional gaps

1. **No pin-level connectivity.** Every symbol contributes exactly one anchor at its insertion point (`src/app/commands/elec.rs:875-891`). A contactor, relay or terminal block cannot be connected pin-by-pin, so nets, wire lists and terminal reports are only as precise as "symbol touches wire here". ACADE is pin/terminal-point driven; this cannot be fixed by adding commands — it needs a pin model in the symbol library and the drawing.
2. **No persistent electrical object data in the DXF.** 0 XDATA, no custom objects, no extension dictionary (§6.2). Device class, sheet membership, net identity and xref links are recomputed or lost. Round-tripping a project through another tool destroys the model, and opening someone else's ACADE drawing yields only generic blocks and text.
3. **Symbol library is a folder of DXFs with a 4-column TSV** (`elec.rs:120-161, 1614-1780`) — no pin definitions, no footprints, no multi-state symbols, no terminal/PLC/cable symbol families, no attribute templates in the drawing (they are synthesised at load, `:1694-1709`), no library editor, no catalogue-to-symbol link. The `block_entity_handle` bug (`:1669-1673`) additionally breaks the "geometry inside a block" symbol layout.
4. **Tagging/numbering is not configurable.** Only `NumberingFormat::tag(prefix)` and `::wire()` exist as used (`:442`, `:751`); no UI or command to set format, start value, increment, prefix table, per-sheet prefixes or uniqueness scope. `numbering.rs` implements those knobs (20 tests) but nothing selects them. ACADE's `%F%N` drawing settings and SkyCAD's numbering-format editor have no equivalent.
5. **Wire numbering is decoration, not data.** A `TEXT` on `WIRENO` matched by position (`elec.rs:657-833`), one hard-coded format, no wire-number attributes, no source/destination arrows, no cable/core numbers, no renumber propagation when geometry moves.
6. **Cross references are same-sheet and position-based.** `PlacedPart` carries `(block, tag, x, y)` only (`elec.rs:1134-1139`), so a device split across sheets cannot be referenced by sheet; `ELECXREFWRITE` drops `TEXT` at a fixed offset (`:1154-1240`) which drifts as soon as the symbol moves. No contact-level references (13-14 / 21-22 style), no automatic refresh.
7. **Project/sheet management is a plain-text file and one command.** `project.eprj` (`:3362, 3517`) knows path/number/title; nothing else is per-sheet (paper size, drawing number, revision, description, status), nothing is stored in the drawing, `ELECSHEET` writes text not attributes (`:4019-4026`), and there is no project tree UI.
8. **Reports are a fixed CSV header plus a minimal OOXML writer** (`:4276`, `:4330-4429`): no templates, no styles, no formulas, no BOM table on the drawing, no title-block-driven data extraction, no user-defined report configurations. The project workbook is the only multi-sheet output.
9. **Panel layout is a prototype.** Fixed 14×10 boxes on a 20-unit grid, alphabetical (`:3263-3355`): no footprints, dimensions, DIN rail, enclosure library, wire routing, or terminal-strip layout — the handoff itself lists this as unfinished (its §7.2).
10. **PLC I/O is synthetic geometry with attributes.** A runtime-built `PLC-<n>` body (`:2855-2890`) with `ADDR01..` strings; no rack/slot/chassis, no module families, no signal types, no I/O descriptions, no addressing beyond the `I0.0`-style increment (`plc_addresses:2943`), no bus networks.

**Also absent (smaller but real):** no audit/ERC (no "wire ends in air", "duplicate tag", "unconnected terminal" checks); no ladder/rung drafting aids; no wire type/colour/gauge/cable data; no device class rules or installation/location hierarchy beyond free-text attributes; no interoperability with ACADE's `WD_M` settings (read never happens — only `WD_*` blocks are filtered out, `:4318-4321`); no ribbon access for 19 of the 31 working commands (and the `ELECINSERT` button that does exist is broken); `expr.rs` label language entirely unreachable; English-only UI for a feature whose stated model is reverse-engineered from two non-English-first products.

---

## Appendix A — How to reproduce the findings

```powershell
# 1. File size / sanity
(Get-Item src\app\commands\elec.rs).Length                      # 187886
([System.IO.File]::ReadAllText('src\app\commands\elec.rs') -split "`n").Count   # 4488

# 2. No stubs / no duplicates
Select-String -Path src\app\commands\elec.rs -Pattern 'todo!\(|unimplemented!\(|panic!|TODO|FIXME'

# 3. Command inventory
Select-String -Path src\app\commands\elec.rs -Pattern '"(ELEC[A-Z0-9_]*)"' -AllMatches |
  % { $_.Matches } | % { $_.Groups[1].Value } | Group-Object | Sort-Object Name
Select-String -Path src\app\commands\mod.rs -Pattern 'ELEC'      # registration block 1291-1322
Select-String -Path src\app\command_driver\utilities.rs -Pattern 'ELEC'   # early-arg list 1085-1097

# 4. Test-compile errors in the electrical modules
Select-String -Path src\modules\electrical\place.rs -Pattern 'PlaceSymbolCommand::new|CmdResult::Finished'
Select-String -Path src\modules\electrical\wire.rs  -Pattern 'assert_eq!.*CmdResult'
Select-String -Path src\command.rs -Pattern 'pub enum CmdResult|Done,' -Context 6,2

# 5. elec crate test suite (cargo cannot resolve here — see §3.2)
$env:CARGO_TARGET_DIR="$env:TEMP\elec-audit-target"; cargo test -p elec      # fails: acadrust rev missing
rustc --edition 2021 --test crates\elec\src\lib.rs -o "$env:TEMP\elec-tests.exe"; & "$env:TEMP\elec-tests.exe"

# 6. Wiring
Select-String -Path src\modules\registry.rs -Pattern 'electrical'
Select-String -Path src\app\view\modal.rs  -Pattern 'ElectricalLibrary'
Select-String -Path src\app\mod.rs         -Pattern 'ElectricalLibrary'
```

## Appendix B — Artifacts consulted (non-source)

`reports/_tmp/*.txt` are the previous session's raw tool logs (tool call/result transcripts, including a `cargo test -p elec` history showing transient 1-test failures at 25/44/49/93/105-test stages on 2026-11-04). They were read only as corroboration; every claim in this report is backed by the current source, and the current crate test run (121/121) supersedes those logs. No `reports/ecad-verification.md` exists in this checkout.
