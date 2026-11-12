# OpenCADElectrical — end-to-end QA verification (2026-11-11)

Driven as an electrical engineer would: start a new DOL/motor-branch controls drawing,
place symbols, wire pin-to-pin, number wires, schedule cables, run reports, then edit
the result. Everything below was run against the shipped binary
`target\release\OpenCADStudio.exe` (159,340,544 B, built 2026-11-09 11:24:35, HEAD 9b1225a,
working tree clean).

**A fix pass followed this report.** All seventeen defects below are now fixed in the
working tree and re-verified against a rebuilt binary — see
[Fix pass (2026-11-12)](#fix-pass-2026-11-12--what-changed-and-how-it-was-verified) at the
end. D6 and the second half of D13 needed a decision about touching shipped library data;
that decision was taken and both are covered here too.

## Verdict

The drafting core works. The regression suites pass, the MCP surface answers correctly,
and a from-scratch controls circuit drafted in one session survives save/reload and
reports the right nets, wire numbers, BOM, cable schedule, panel and report files.

Sixteen defects are real, four of them serious enough to corrupt a real job (a
seventeenth, D17, was found while verifying the fixes — see below):

| # | Severity | Defect |
|---|---|---|
| D1 | **critical** | A save on any pre-AC1018 drawing is silently discarded in headless/batch mode — and **every shipped template is AC1009** |
| D2 | **high** | `ELECSETUP` writes its contract to a place the reader ignores, so tag/wire numbering settings silently revert (confirmed in-session, no reopen needed) |
| D3 | **high** | Opening an existing drawing loses the symbol attribute template: placed symbols get **no pins** and **duplicate designators** |
| D4 | **high** | Placed symbols stack every attribute — including the `X<n>TERM<nn>` pins — in a column below the insertion point instead of at the symbol's own connection points, so wires land ~31 units away from the device body |
| D5 | medium | `ELECWIRELIST` net labels come from a colliding id space (`net 1, net 2, net 4`, duplicates possible) and disagree with the MCP/report labels `N1..Nn` |
| D6 | medium | Starter library numbers both ends of 2-terminal devices as pin 1, so `-F1:1` legitimately appears in two different nets |
| D7 | medium | The automation (MCP) save path uses a different, weaker loss detector than the GUI and reports success while silently dropping the whole electrical setup contract |
| D8 | medium | Any open modal silently swallows automation `run` commands: the call returns `ok:true` and nothing happens |
| D9 | low | `cargo test --release --lib` does not compile |
| D10 | low | `docs/automation/guides/ecad.md:48` documents `ELECJUNCTION`; the command is `ELECJUNCTIONS` |
| D11 | low | `ELECEXPORT` is missing from the early-arg list, so it runs twice — the bare probe pass emits a spurious usage error |
| D12 | low | `ELECREV SNAPSHOT` with no project loaded writes an **empty** snapshot (`0 file(s), 0.0 KB`) and reports success, where every sibling command refuses |
| D13 | medium | `ELECMAKESYMBOL` / `ELECMAKEMODULE` append a **4-field row to a 5-field manifest** — scale `1` instead of `25.4`, no standard column — and write into the shipped `library\starter` catalogue |
| D14 | medium | `docs/automation/guides/ecad.md:65` documents `ELECZONE`, which is not a command at all |
| D15 | low | `ELECMAKEMODULE` places the module with an **empty `TAG1`**, so it has no designator and is invisible to every tag-based command |
| D16 | low | The conversion-loss warning names the **wrong target format** — `format_for_version` falls through to `"2018"` for AC1009/AC1012/AC1013 — so the modal reads "Saving as DXF 2018 … DXF files older than 2004 … will be lost" |
| D17 | medium | Found while verifying the fixes: a symbol that carries a designator but **no pin attributes** is invisible to tag allocation, so the next placement is handed the **same tag** — two devices on one sheet under one designator |

---

## D1 — silent save loss on pre-AC1018 drawings (critical)

**Symptom.** A batch row `QSAVE` after `ELECSETUP` writes nothing, prints no error, and
the process exits 0. The file is byte-identical. The whole session's work is gone.

**Mechanism.** `queue_native_save` (src\app\update\file.rs:3420-3439) refuses to write
when `conversion_losses_to_confirm` finds losses, and calls `ask_conversion_loss`
(file.rs:4625-4634), which opens `ModalKind::ConversionLoss` and returns `Task::none()`.
In `--each` / `--batch` nobody can answer a modal, so the pending save is dropped.

`drawing_properties_lost_on_save` (src\io\mod.rs:1795) returns 0 for
`version >= AC1018`, otherwise counts six summary fields plus every custom property.
`write_setup_properties` (src\app\commands\elec.rs:511-562) writes **sixteen** custom
properties (`ELEC_TAGFMT`, `ELEC_TAGMODE`, `ELEC_TAG_START`, `ELEC_TAG_RSUF`,
`ELEC_WIREMODE`, `ELEC_WIRE_START`, `ELEC_XREFFMT`, `ELEC_SHEET`, `ELEC_TAGINC`,
`ELEC_TAGFALL`, `ELEC_WIREINC`, `ELEC_WIREKIND`, `ELEC_WIREEMPTY`, `ELEC_TAGUNIQ`,
`ELEC_WIREWEB`, `ELEC_WIREFMT`). So on a pre-AC1018 drawing, `ELECSETUP` guarantees the
modal fires.

**Why the suites miss it.** All shipped templates are AC1009 (R12):
`templates\iec\A0..B1_landscape/portrait.dxf` and `templates\nec\ANSI_*.dxf`.
`run_features.py` uses `QSAVE` 13× but `ELECSETUP` 0×; `run_smoke.py` uses `ELECSETUP` 9×
but routes every save through `SAVEAS` 9×. `save_as_from_command_line`
(src\app\commands\fileops.rs:806, :838) takes the headless path
`save_tab_synchronously_protected_as(..., true)`, which bypasses the confirm and prints
`Saved:`. No harness combines `ELECSETUP` + `QSAVE` on a pre-AC1018 drawing.

**Repro (decisive pair).** Same script, two starting drawings:

```
ELECSETUP TAGINC=2
ELECINSERT CONTACT_NO
QSAVE
```

* AC1009 drawing → no `Saved:` line, file byte-identical, exit 0.
* AC1027 copy of `import\Prints\Prints\DXF\FC1000495-002.dxf` → `Saved:`, file rewritten.

`--allow-lossy` is only accepted with `--export`; `--allow-lossy --each` is rejected by
the CLI parser, so there is no headless escape hatch.

**Fix direction.** In headless mode (`main_window.is_none()`) treat the conversion-loss
question as already answered — either proceed, or fail the row loudly and exit non-zero.
Silently dropping a save is the worst of the three.

---

## D2 — `ELECSETUP` does not persist where it reads (high)

`read_setup` (src\app\commands\elec.rs:400-454) reads the contract from the first `WD_M`
block insert's attributes (`TAGFMT`, `TAGMODE`, `TAG-START`, `TAG-RSUF`, `WIREFMT`,
`WIREMODE`, `WIRE-START`, `XREFFMT`, `SHEET`) and only falls back to custom properties
when no `WD_M` exists — comment at elec.rs:437-438: *"A real WD_M always wins over them."*
`write_setup_properties` never writes the `WD_M` block.

**Proof.** `templates\iec\A4_landscape.dxf` exported to AC1032
(`target\qa\drive\tpl2018.dxf`) so D1 could not interfere:

```
ELECSETUP TAGINC=7 TAGSTART=101   -> "start 101 step 7 ... saved on this drawing", Saved:
(reopen) ELECSETUP                -> "from this drawing's WD_M block ... start 1 step 1"
```

It does not even need a reopen. On a plain copy of the shipped template
(`target\qa\drive5\ins2.txt`, log `ins2.log`, exit 0):

```
ELECLIB IEC                       -> loaded 78 symbol(s) ... standard IEC
ELECINSERT CONTACT_NO             -> placed CONTACT_NO as -K1 at 0.00,0.00.
ELECLIST                          -> Contactor  -K1
ELECSETUP TAGINC=3 TAGSTART=101   -> "... start 101 step 3 (unique Global) ... saved on this drawing."
ELECSETUP                         -> "from this drawing's WD_M block / tag %F%N mode S start 1 step 1 ..."
```

The command reports success and the very next read reports the old values. Every shipped
template carries a `WD_M` block, so on template-based sheets — i.e. every sheet a user
actually drafts — `ELECSETUP` is a no-op. The same run then `QSAVE`d and lost everything
(D1): the file on disk is still AC1009 with 0 `CONTACT_NO` matches.

---

## D3 — placed symbols lose their pins after OPEN (high)

`elec_place` takes the attribute template from `library_index()` and falls back to
`vec!["TAG1","CAT"]` when the entry has none (src\app\commands\elec.rs:4423-4427). The
index is filled from `library.tsv` only by `ensure_library_index` / `ELECLIB` / `ELECNEW`
(elec.rs:216-236, 243-292). A plain `OPEN` does not restore the template, so the fallback
fires and the insert gets **no `X<n>TERM<nn>` pin attributes at all**.

Pins are the only connectivity contract (crates\elec\src\pins.rs:1-46: the attribute's
insertion point *is* the pin). A symbol without them is invisible to the wire graph.

**Matrix** (all AC1032, same symbol, same point):

| session | command | result |
|---|---|---|
| blank + `ELECNEW` | `ELECPLACE FUSE` | 2 pins |
| `ELECLIB` in-session | `ELECPLACE FUSE` | 2 pins |
| reopen a drawing that already has the 78-symbol library | `ELECPLACE FUSE` | **0 pins** |
| reopen a drawing that already has the library | `ELECINSERT FUSE` | **0 pins** |
| reopen + `ELECNEW` | `ELECPLACE FUSE` | 2 pins |
| plain drawing, no `ELECNEW`/`ELECLIB` | `ELECPLACE FUSE` | **0 pins** |

App-level proof, not just my probe: on the reopen drawing the same wiring that yields
`ELECWIRELIST: 4 wire segment(s), 12 pin(s) -> 3 net(s)` in an `ELECLIB` session yields
`2 wire segment(s), 0 pin(s)` without it.

**Second-order failure: duplicate designators.** `elec_place` derives the next tag from
existing anchors (elec.rs:4410-4417). A pin-less device is not an anchor, so it is not
counted. Placing a second fuse into a drawing whose first fuse is pin-less gives:

```
ELECLIST
   Fuse  -F1
   Fuse  -F1
```

Two devices, one reference. This is the *editing an existing drawing* path — the common
case — and it silently produces unnumberable, unconnected, duplicate-tagged hardware.

**Fix direction.** Persist the attribute template (and definition-point offsets) with the
drawing's library table, or re-read `library.tsv` on open for any symbol whose entry has
an empty template.

---

## D4 — placed symbols put their pins in a label column, not on the symbol (high)

The symbol's own file puts its connection points on its drawn line ends —
`library\starter\FUSE.dxf` defines `X4TERM01` at (-0.5, 0) and `X1TERM01` at (0.5, 0),
which are exactly the two ends of the LINE from (-0.5,0) to (0.5,0); the same holds for
`CONTACTOR_COIL` and `PILOT_LAMP` (2/2 pins on a line end in each).

The placed instance does not honour them. `ELECPLACE FUSE` at (60,40) at the library scale
25.4 writes:

```
INSERT FUSE at (60,40) scale 25.4
   TAG1 (60,36.825)  CAT (60,32.856)  MFG (60,28.888)  DESC1 (60,24.919)
   LOC (60,20.950)   TERM01 (60,16.981) TERM02 (60,13.013)
   X4TERM01 (60,9.044)   X1TERM01 (60,5.075)
```

Every attribute — including both pins — sits in one vertical column at the insert's own x,
spaced 3.96875 apart. That is exactly `place.rs:69`'s fallback branch
`at.y - height - row * step` with `height = 0.125 * 25.4 = 3.175`, `step = 1.25 * height`:
the `offset` was `None` for all nine attributes, even though `ELECLIB IEC` had just
reported *"679 attribute definition point(s)"* and `elec_place` passes
`entry.attribute_offsets` into the command (src\app\commands\elec.rs:4434-4451) with the
comment *"placing the insert puts each attribute on its own definition point instead of
stacking them"* (elec.rs:4432-4433). The measurement pass that follows
(elec.rs:4241-4265) also left `label_height` at the 0.125 default for
`CONTACTOR_COIL`, whose span is 0.7 and should give 0.0875 — the per-symbol data measured
during `ELECLIB` is not reaching the index `ELECPLACE` reads.

**Consequence for a real drawing.** The symbol body renders centred on the insert point
(the block is defined with base point ZERO, elec.rs:4198-4202), so its drawn connection
stubs are at ±12.7 units, while the pins the netlist uses are 31–35 units below. A wire
drawn pin-to-pin therefore attaches to empty space under the device — visible in
`target\qa\gui\04-elecsetup.png`, where the green `WIRES` segments run well below every
symbol body. The two ends of a device that should be 25.4 units apart are 3.97 apart, so
`ELECWIRELIST` reports 12 pins for 5 devices and a wire between "adjacent" pins is a
vertical stub in the label column. The netlist is internally consistent (nets do merge),
which is why the suites pass while the drawing is wrong.

**Root cause (pinned).** `ELECLIB` measures the definition points only for symbols it
actually (re)defines. On a drawing that already contains the blocks — which is every
drawing after its first save, because `ELECLIB` skips blocks that are already defined — it
reports:

```
ELECLIB: loaded 0 symbol(s) from ...\library\starter; 78 already in this drawing, 0 failed,
78 in the catalogue, 0 attribute definition point(s), 1525 preview segment(s), standard IEC.
```

`0 attribute definition point(s)` is the tell: `measured_offsets` (elec.rs:4211) stays
empty, the merged index entry (elec.rs:4229-4236) carries no `attribute_offsets`, and
`place.rs:69` takes the stacking fallback. On a drawing where the symbols really are
defined in that session, the same command reports `679 attribute definition point(s)` and
placement is correct. So the defect is not the measurement pass, it is that the offsets are
never recovered from the block definitions already in the drawing.

**Fix direction.** Assert in a test that a placed symbol's `X<n>TERM<nn>` ATTRIB points
coincide with the block's own ATTDEF points scaled by `entry.scale`; today nothing checks
it. Then measure the offsets from the existing block records when `ELECLIB` skips a symbol
as already defined.

---

## D5 — `ELECWIRELIST` net labels collide and disagree (medium)

`ELECWIRELIST` prints `net {net.id}` straight from the netlist
(src\app\commands\elec.rs:2876-2891). `netlist_from_geometry_impl` assigns
`id = wire_ids.first()` for wire-only nets (crates\elec\src\geometry.rs:310) but
`id = wire_ids.first().or(anchor_ids.first())` for anchored nets (geometry.rs:321-325) —
two different id spaces, so labels repeat and skip:

```
ELECWIRELIST: 4 wire segment(s), 12 pin(s) -> 3 net(s)
  net 1: 1 wire(s) -> -X1:1, -F1:1
  net 2: 2 wire(s) -> -F1:1, -K1:1, -K2:2
  net 4: 1 wire(s) -> -K2:1, -H1:2
```

Three nets, labelled 1, 2, 4. On an earlier drawing the same label appeared twice
(`net 1` twice, no `net 5`), and the CSV export confirms it: both rows carry `Net=1`.
Meanwhile `graph_electrical` — the MCP `ocs_read op:electrical`, the node graph and the
reports — renumbers with `.enumerate()` to `N1..Nn` (elec.rs:1086). The doc comment at
elec.rs:1056 claims the two never disagree; the *label* does.

---

## D6 — starter symbols number both terminals "1" (medium)

`library\starter\library.tsv` gives `FUSE`, `CONTACTOR_COIL`, `CONTACT_NO`, `CONTACT_NC`,
`PUSHBUTTON_NO` the set `...|TERM01|TERM02|X4TERM01|X1TERM01`, and `MOTOR` /
`TERMINAL_BLOCK` `X4TERM01|X1TERM01`. The direction digit is not part of the identity
(pins.rs), so both connection points map to bare pin **1**. Consequence in a correct
series chain: `-F1:1` is a member of `net 1` *and* `net 2`. A fuse, a contact or a coil
whose two ends share one reference cannot be reported, cross-checked or terminated
correctly. `PILOT_LAMP` (`X4TERM01|X1TERM02`) and `THERMAL_OVERLOAD` (1/2 both sides)
show the intended pattern.

**Fixed in the fix pass**, at the generator rather than by hand-patching 60 files — see
[D6 — the shipped library, renumbered at the generator](#d6--the-shipped-library-renumbered-at-the-generator).

---

## D7 — the automation save uses a weaker loss detector and reports success while discarding the setup contract (medium)

The GUI and the automation path ask two different questions about a save.

The GUI path: `queue_native_save` → `conversion_losses_to_confirm` →
`drawing_properties_lost_on_save` (src\io\mod.rs:1795), which counts the six summary
fields plus **every custom property** on a pre-AC1018 target and opens
`ModalKind::ConversionLoss`.

The automation path: `ocs_execute op:save` (src\app\automation.rs:1335-1408) consults
`SaveLosses::of` (automation.rs:620-662), whose `lossless()` is
`dropped == 0 && mtext.is_empty()` — `dropped` counts only unsupported passthrough
RECORDS (`crate::io::dropped_on_save_count`) and `mtext` only MTEXT export problems. The
drawing summary and custom properties are **not** in that set, so the refusal at
automation.rs:1386-1388 never fires for the electrical contract.

**Proof.** `target\qa\loss_probe.py` against `templates\iec\A4_landscape.dxf` through the
live editor: `ELECLIB IEC`, `ELECSETUP TAGINC=3 TAGSTART=101`, `ELECINSERT CONTACT_NO`,
then `op:save` → `{"saved": ..., "target_version": "AC1009", "dropped_on_save": 0,
"mtext_losses": []}` with no refusal. The written file contains **0 `ELEC_` matches and 0
`CONTACT_NO` matches** — the sixteen setup properties the session just wrote are gone, and
the client was told the save succeeded.

Two detectors that disagree also means the `allow_lossy` flag an automation client sets
in good faith is not the flag the GUI would have asked about.

**Fix direction.** Have `SaveLosses::of` include
`drawing_properties_lost_on_save(...)`, so an automation save into a lossy target either
refuses with the same count the GUI shows or honours `allow_lossy` consistently.

---

## D8 — an open modal silently swallows automation commands (medium)

With `state.modal` set (`ConversionLoss`, `ElecReport`, …) `ocs_execute op:run` still
returns `ok:true`, but `state.geometry_revision` and `state.revision` do not move and
`status` comes back `"waiting_input"`. In `target\qa\loss_probe.py` the `ELECINSERT
CONTACT_NO` after the lossy save attempt never executed — geometry_revision stayed at 41
and the drawing gained no block.

This is also why `ELECLIST` behaves differently in the two harnesses: in headless
`--each` it prints a table, in the live editor it opens an **ElecReport modal** that then
blocks every later command until something answers it.

An automation client has no way to see this from the `run` reply except by polling
`status`/`modal`; the action names exist (`conversion_loss_save_anyway`,
`conversion_loss_back`, `close_modal` — src\app\control\actions.rs:128) so the recovery is
possible, but the failure is silent by default.

**Fix direction.** Return a distinguishable error (or at least
`status:"waiting_input"` with a `blocked_by_modal` field) from `op:run` when a modal owns
input, instead of `ok:true`.

---

## D9 — the documented test command does not build (low)

`cargo test --release --lib` → exit 101:

```
error[E0063]: missing field `standard` in initializer of `elec::LibraryEntry`
  --> src\app\commands\elec.rs:8133:13
```

The round-33 `standard` field was added to `LibraryEntry` (elec.rs:139-157) without
updating the unit-test constructor at elec.rs:8133. 20 warnings also fire (unused import
`Kind` at elec.rs:8223, unused `HostApi` at src\app\commands\mod.rs:1774 and
src\app\plugin_host.rs:16251, unused `part`/`i` at
src\app\block_editor_tests\grip_descriptions.rs:274).
`cargo test --release -p elec` is fine: **131 passed, 0 failed**.

## D10 — doc typo (low)

`docs\automation\guides\ecad.md:48` names `ELECJUNCTION`. The registered command is
`ELECJUNCTIONS` (elec.rs:7144, ELECHELP line 1806; `docs\QUICKSTART.md:61` has it right).
Typing the documented name gives `✕ INVALID: Unknown command: ELECJUNCTION 40,9.044`.

---

## D11 — `ELECEXPORT` runs twice (low)

`ELECEXPORT` is absent from the early-arg list in
src\app\command_driver\utilities.rs:1067-1125 (`ELECPROJECT` is at :1094, `ELECPROJNUM` at
:1103, `ELECREV` at :1116 — `ELECEXPORT` is not there). The driver therefore probes the
command with an empty argument before running it for real, so one batch row produces two
outputs:

```
ELECEXPORT: give a folder, e.g. ELECEXPORT publish dxf  (or ... dwg).
ELECEXPORT: 2 sheet(s) -> ...\drive8\pub (dxf).
```

The export itself is correct — `pub\s1.dxf` 483,029 B and `pub\s2.dxf` 453,946 B were
written. This is the same double-execution class the round-32 work fixed for `ELECSHEET`
("ELECSHEET drew its frame twice", comment at utilities.rs:1099-1101); it is harmless here
only because the bare pass hits an argument guard.

---

## D12 — `ELECREV SNAPSHOT` succeeds with nothing to snapshot (low)

With no project loaded but a project folder remembered from a failed `ELECPROJNEW`
(`target\qa\drive7\cov2.log`):

```
ELECBUILT: no project is loaded — ELECPROJNEW or ELECPROJOPEN first.
ELECOPTION: no project is loaded.
ELECREV: snapshot 001 — 0 file(s), 0.0 KB.
ELECREV: 1 snapshot(s) in ...\drive7\proj\.rev
  001  0 file(s)  0.0 KB
```

Every sibling command refuses; `ELECREV` creates a snapshot directory with an empty
manifest and calls it a success. A user who then trusts `ELECREV LIST` sees a restore point
that restores nothing. With a real project the same command is correct
(`snapshot 001 — 3 file(s), 915.1 KB`).

---

## D13 — authored symbols are appended to the manifest with the wrong columns (medium)

`store_library_symbol` (src\app\commands\elec.rs:7553-7592) writes:

```rust
text.push_str(&format!("{safe}\t{class}\t{}\t1\n", tags.join("|")));   // elec.rs:7589
```

but the shipped manifest is five columns —
`symbol	class	attributes	scale	standard`, e.g.
`WIRE_LABEL	Wire Label	TAG1|DESC1|X4TERM01|X1TERM02	25.4	IEC`.
The row `ELECMAKEMODULE TB1 12` actually appended was:

```
TB1	Module	X4TERM01|X1TERM01|...|X4TERM12|TAG1|CAT|MFG|DESC1	1
```

so the new symbol gets scale **1** instead of 25.4 (it places 25× smaller than every other
IEC symbol) and no `standard` value, which is what the round-33 standard column and the
palette's IEC/NEC filter read. The stale fallback header at elec.rs:7585
(`symbol\tclass\tattributes\tsheets`) confirms the writer was never updated for the
scale/standard layout.

Two further consequences of the same call: it writes into the **shipped** catalogue folder
(`library\starter\TB1.dxf`, and it modified the git-tracked `library\starter\library.tsv` —
`git status` showed ` M library/starter/library.tsv` + `?? library/starter/TB1.dxf` after my
run, which I reverted), and the in-memory index is not refreshed, so the placed module's
class reads back as the title-cased block name `Tb1` rather than `Module`.

**Fixed in the fix pass**, both halves — see
[D13 — authoring no longer writes into the shipped catalogue](#d13--authoring-no-longer-writes-into-the-shipped-catalogue).
Note that the four-field row itself turned out to be harmless; the finding is restated
there rather than quietly dropped.

---

## D14 — the automation guide documents a command that does not exist (medium)

`docs\automation\guides\ecad.md:65` lists, in the read-back table:

```
| `ELECZONE` | the sheet's zone letter/number for cross references |
```

`ELECZONE` appears nowhere else in the repository — not in `ELECHELP`
(src\app\commands\elec.rs:1782-1891), not in the dispatcher. Running it gives
`✕ INVALID: Unknown command: ELECZONE` and, because `--each` exits 1 when a row fails, the
whole batch reports failure. An automation agent following the published guide will treat a
documented verification step as a defect in its own script.

---

## D15 — `ELECMAKEMODULE` places an untagged instance (low)

The placed insert gets its pin attributes with real `x,y` values (elec.rs:5095-5102, which
is the form `insert_pins` parses), but the label attributes are pushed empty:

```rust
for (row, tag) in ["TAG1", "CAT", "MFG", "DESC1"].iter().enumerate() {
    let mut attribute = codec::entities::AttributeEntity::new(tag.to_string(), String::new());
```

(elec.rs:5103-5113). `ELECLIST` after `ELECMAKEMODULE TB1 12` therefore shows a component
with no designator:

```
Tb1                                 -
```

so the module cannot be found by `ELECSETATTR`, `ELECASSIGN`, `ELECHIGHLIGHT`, `ELECGOTO` or
`ELECBOM`'s WHERE USED column, and `next_tag` does not count it.

---



## D16 — the conversion-loss warning names the wrong format (low)

The GUI does prompt honestly (see "The GUI surfaces"), but the prompt is unreadable.
Captured verbatim from `target\qa\gui\06-conversion-loss.png` after `ELECSETUP TAGINC=3
TAGSTART=101` + `ELECINSERT CONTACT_NO` + `QSAVE` on `templates\iec\A4_landscape.dxf`:

```
Save Warning
Saving as "DXF 2018" cannot keep the following, so it will be lost:
 • DXF files older than 2004 do not store the drawing properties; 16 filled-in properties will be lost.
 [Save anyway] [Back]
```

The drawing's own version is AC1009 (shown in the Properties panel) and the file written by
"Save anyway" is **AC1012** — not 2018. The header label comes from
`PendingConversionSave::format_name` (src\app\mod.rs:2226-2244), which for a `.dxf` target
falls through to `crate::io::format_for_version(version, true)` (src\io\mod.rs:1770-1783):

```rust
let year = match version {
    AC1032 => "2018",  AC1027 => "2013",  AC1024 => "2010",  AC1021 => "2007",
    AC1018 => "2004",  AC1015 => "2000",  AC1014 => "R14",
    _ => "2018",          // AC1009 / AC1012 / AC1013 land here
};
```

So every pre-AC1015 target is labelled "2018", producing a self-contradictory message —
"DXF 2018 … files older than 2004 … will be lost". The function's own doc comment says it
exists so a round-trip "preserves it instead of silently offering 'DWG 2018'", and the
catch-all does precisely that. A user facing D1's decision sees a format name that would
lose nothing, and cannot tell what they are actually agreeing to.

---

## D17 — a designator with no pins is invisible to tag allocation (medium)

Found while re-running the D3/D4 probes against the fixed build, so it is not in the
original sixteen-row table order.

`ELECPLACE` allocates the next designator from the drawing's *wire-graph anchors*, and the
anchor list drops every insert that has no pin attributes — `src\app\commands\elec.rs:1070`
inside `fn wire_graph(document: &codec::Document) -> WireGraph` (`:898`):

```rust
let pins = insert_pins(document, insert);
if pins.is_empty() {
    continue;
}
```

A symbol placed by an older build, or one whose pins were stripped by an exploded-block
edit, still carries its `TAG1` and still shows in `ELECLIST`, but it is not an anchor, so
the allocator never sees it. Observed on `target\qa\drive4\lib2018.dxf`, which holds
`FUSE @(60,40) TAG1='-F1' CAT=''` with no pin attributes: two fresh `ELECPLACE FUSE` came
back as **`-F1` (a duplicate) and `-F2`**.

That is worse than a cosmetic clash: the BOM, the cross-reference and the wire list all
key on designator, so two physical devices collapse into one line item and the
cross-reference reports both coil references against whichever insert it reaches first.

The same `pins.is_empty()` filter is why D3's "no pins" symptom also produced "duplicate
designators" — the two D3 symptoms shared this root.

---

## The GUI surfaces

Driving the real window: iced exposes no UIA tree, so `arya_desktop` snapshot returns only
the title bar and ref-based click/type cannot reach the canvas. The live editor was
therefore driven over its MCP bridge (`target\qa\gui_qa.py`, log `target\qa\gui_qa.txt`,
7/10 — the two failures are D7 and D8) and screenshotted with `ocs_capture`.

Verified by looking at the captures (`target\qa\gui\01..04-*.png`, also written to
`docs\screenshots\`):

* The **Electrical ribbon tab** is present in the tab strip, and the icons are real SVGs
  (`src\modules\electrical\mod.rs:50-79`, `IconKind::Svg(include_bytes!("assets/icons/elec_*.svg"))`)
  — the STATUS.md claim about a double-escaped `\u{2795}` glyph is fixed.
* `ELECSTART` opens the template chooser.
* `ELECPALETTE` opens **"Electrical Library — IEC"**: search box, `78 / 78` count, class
  filter chips (All / Bell / Button / Buzzer / Cable / Circuit Breaker / Connector /
  Contactor …), and a live thumbnail per symbol. The round-32 zero-size-row crash
  (widget\src\button.rs:254) does not reproduce.
* A drafted drawing renders with its device bodies and `WIRES` segments, and two drawings
  sit side by side in the tab strip.
* Typing `ELECSETUP` at the command line prints the contract summary; the dialog is
  `ELECSETUPEDIT` (the summary says so). An earlier "ELECSETUP dialog did not crash" check
  was vacuous — no dialog was open.
* `ELECSETUPEDIT` **does** open a working dialog (`target\qa\gui\05-elecsetupedit.png`,
  `state.modal = ElecSetup`): "Electrical setup — Stored on this drawing. A drawing that
  carries an AutoCAD Electrical `WD_M` block keeps using it.", with Devices (tags) Format
  `%F%N` / Preset / Mode `S - per sheet` / Start / Suffixes / Increment / Follow through /
  Unique `Global`, Wires Format `%N` / Mode / Start / Increment / Numbers `Numerical` /
  Number webs / Empty first value, Cross references Format `%S.%N` + Sheet, and Cancel /
  Save. Note the dialog states the WD_M rule out loud, which is exactly why D2's command
  line ("saved on this drawing") is misleading on a template sheet.
* The **conversion-loss modal does appear in the interactive GUI**
  (`target\qa\gui\06-conversion-loss.png`, `state.modal = ConversionLoss`) — so D1 is
  specifically a headless/batch failure, not a general save failure. Answering it through
  automation (`action: conversion_loss_save_anyway`, one of the names in
  src\app\control\actions.rs:128) completed the save: `target\qa\drive9\r12.dxf` grew from
  9,910 B to 468,142 B and contains the placed `CONTACT_NO` (10 hits). The 16 setup
  properties were still dropped (`ELEC_TAGINC` hits: 0, written version AC1012), which
  confirms independently that the electrical contract has nowhere to live on a pre-AC1015
  target — the D2 fix has to be the `WD_M` block, not custom properties.
* Launch shows an "Update Available" modal (installed v2026.44 → v2026.45); Escape closes
  it.

---

## What was verified working

**Suites** (same binary as baseline and current):

* `tests\ecad\run_smoke.py` — **127/127** checks (`target\qa\smoke_full.txt`); 112/112 with `--no-real-sheets`.
* `tests\ecad\run_features.py` — **44/44** checks (`target\qa\features2.txt`).
* `tests\ecad\verify_mcp_ecad.py` — **16/16** MCP checks (`target\qa\mcp.txt`): `initialize` 2024-11-05, `ocs_electrical` advertised, `cad://guides/ecad` resource, electrical read returns 5 nets / 20 pins / 10 components, `N1: -K1:2 -K2:1`, Electrical node-graph category and Netlist node evaluate.
* `cargo test --release -p elec` — **131 passed**.

**My own drive** (`target\qa\drive_qa.py`, 42/48 checks; the 6 failures triaged above):

* Placement + auto-tagging classifies correctly: `-T1 -F1 -F2 -S1 -S2 -K1 -K2 -H1 -X1 -M1`.
* Pin-to-pin wiring: 4 wires → 3 nets with correct device membership.
* T-junction rule is right: a branch is a separate net until `ELECJUNCTIONS` draws the dot, then merges (`2 net(s)` → `1 net(s)`).
* `ELECWIRENUMBER` writes `TEXT` on `WIRENO` and is idempotent on re-run.
* `ELECBOM` (csv), `ELECPANEL` (dxf), `ELECREPORT` (xlsx), `ELECLIST`, `ELECTERMINALS` all produce and persist.
* Cables are project-scoped and correct once a project exists: `ELECPROJNEW` → two `ELECCABLEDRAW` runs → schedule shows cores, part, computed lengths (40.00 / 30.00) and shield flag; `cables.tsv` round-trips.
* `ELECVARIANT NEW REV-B` writes `variant-REV-B.eprj` and lists beside the project.
* `ELECREV SNAPSHOT/LIST/COMPARE/RESTORE`, `ELECSYSTEM INSERT DOL_STARTER`, `ELECPLC -P1 8 I0.0`, `ELECIOIMPORT`, `ELECSETATTR -K1 CAT=... MFG=... LOC=...`, `ELECHIGHLIGHT`, `ELECGOTO` all behave and persist.

**Command-by-command coverage** (`target\qa\drive7\cov.txt`, `cov2.txt`, `target\qa\drive8\cov3.txt`, `cov4.txt`; logs beside them). A project was built the way the product expects — a folder that already holds sheet DXFs — and then driven end to end:

| command | result |
|---|---|
| `ELECSHEET A3 "QA COVERAGE SHEET"` | `A3 fitted to the drawing at 1:1 (420.00x297.00 units); title block shows sheet - / "QA COVERAGE SHEET"` |
| `ELECPART parts.csv` / `ELECPARTS library\parts` / `ELECPARTLIST fuse` | 4 loaded / 65 in the catalogue / `4 of 65 part(s) match`, table printed |
| `ELECASSIGN -K1 LPJ-100SP` | `set CAT, MFG, DESC1, RATING1 on 1 insert(s): 3 value(s) changed, 1 attribute(s) added`; bare `ELECASSIGN -K1` reads it back |
| `ELECNUMBERPREVIEW` / `ELECNUMBER` | `would write 0 tag(s)` then `wrote 0 tag(s)`, grouped by class |
| `ELECPLC -P1 8 I0.0` / `ELECPLCNEXT -P2 4` | `-P1 … I0.0..I0.7`, then `-P2 … I1.0..I1.3` (addressing continues correctly) |
| `ELECIO` | `-P1 (8 point(s)): I0.0 -> not wired …` + `2 module(s), 12 point(s), 0 wired` |
| `ELECTERMINALS` / `ELECTSIDES` | after `ELECSETATTR -X1 TAG1=X1:1` and `-X2 TAG1=X1:2`: `1 strip(s), 2 terminal(s)` / `2 terminal(s); 0 through, 0 single-sided, 2 unconnected` |
| `ELECBOM out.csv` | 11 components → 10 rows, terminals merged by part (`Terminal UK2.5B … 2 X1:1, X1:2`), csv written |
| `ELECBOM out.csv OPTION=OPT1` | `option OPT1 selects 1 of 5 component(s)` → 1 row |
| `ELECPANEL panel.dxf` | `11 device(s); enclosure 484x380 mm; 1 rail(s), 2 door cut-out(s); wrote … ; registered in project.eprj` |
| `ELECREPORT report.xlsx` | `BOM 10 row(s), wire list 4 row(s), terminals 2 row(s), cables 1 row(s)` |
| `ELECPROJNEW <folder>` / `ELECPROJOPEN` / `ELECPROJLIST` | `2 sheet(s) written to project.eprj`; open and list round-trip the sheet order |
| `ELECPROJNUM` / `ELECPROJNUM apply` | `2 sheet(s), 5 device(s); would renumber 0` then `renumbered 0` — preview never writes |
| `ELECPROJECT <folder> out.xlsx` | `2 sheet(s) read (0 failed); 5 component(s) in 5 class(es); 5 device tag(s), 0 used on more than one sheet; 1 distinct part(s)` + xlsx |
| `ELECEXPORT <pub> dxf` | `2 sheet(s) -> ...\pub (dxf)`; both files on disk (483,029 / 453,946 B) |
| `ELECOPTION -H1 OPT1` | `-H1 = OPT1 (1 option code(s) on -H1)`, persisted into `project.eprj` (`option -H1 OPT1`) |
| `ELECBUILT on` / `off` | `project marked built … Existing references are now fixed.` / `… in design … free again.` |
| `ELECCABLE` / `ELECCABLELIST` / `ELECCABLEDEL` | add W11 + W10, schedule prints cores/part/manufacturer/length/shield, delete leaves 1 |
| `ELECVARIANT LIST` / `NEW REV-QA` / `USE REV-QA` | `variant-REV-QA.eprj (2 sheet(s))` created, listed, switched |
| `ELECSYSTEM LIST` / `INSERT DOL_STARTER 200,0` / `SAVE QA_DOL` | 3 shipped systems; insert retags to `-K2 -K3 -M1 -S1`; save writes `proj\systems\QA_DOL.tsv` (11 devices, 4 wires) |
| `ELECREV SNAPSHOT` / `LIST` / `RESTORE 001` | `snapshot 001 — 3 file(s), 915.1 KB`; `.rev\001\{manifest.tsv,project.eprj,s1.dxf,s2.dxf}`; `restored 3 file(s) from 001 — the pre-restore state is snapshot 002-before-restore` |
| `ELECWIRELABELS` | `no wire numbers on this drawing — run ELECWIRENUMBER first` (correct guard) |
| `ELECHIGHLIGHT -K1` / `ELECGOTO -K1` | `selected 1 part(s)` / `centred on -K1 (1 part(s))` |

`--each` exits **1** when any row is invalid and **0** otherwise, so a batch is a usable CI gate.

## Not defects — environment and harness artifacts

* The first `run_features.py` (29/43) and first `run_smoke.py` crash were both the DSH
  sandbox running at `Mandatory Label\Low Mandatory Level`: the child process could not
  `ReplaceFileW` (`Access is denied (os error 5)`) even inside the workspace, and
  `shutil.move` of `%APPDATA%\AryaCAD\elec_library.txt` failed. With normal access both
  suites pass in full.
* One `run_smoke.py` crash inside `tools\dxfio.py:475` (`TypeError: 'list' object is not
  callable`) did not reproduce in three re-runs; the `.pyc` header matches the source
  exactly and no `list =` shadowing exists. Treat as a one-off in the verification
  tooling.
* `ELECPROJ` and `ELECWIRENUMBER apply` are not commands — my own invented syntax.
  `ELECCABLEDRAW` requiring a project is correct behaviour, but
  `docs\automation\guides\ecad.md` lists it in the single-drawing workflow without saying
  so; that is a doc gap worth closing.
* Bare `ELECLIB` does not load a library — it prints `library_summary()`
  (src\app\commands\elec.rs:121-134) and reports the size of the index already in memory,
  which is why it said `0 symbol(s) from ...library\starter — standard not recorded` on a
  freshly opened template. `ELECLIB IEC` / `ELECLIB NEC` / `ELECLIB <folder>` is the
  loading form and loads all 78 correctly there. My first "the template loads no symbols"
  reading was my own misuse of the bare form.
* `--mcp` attaches to the **already running** editor rather than starting its own, so
  probes share one instance: modals persist between probe scripts and concurrent probes
  race each other. Probe scripts must `close_modal` first and must not run in parallel.
* Re-running a drive script over a drawing it already modified doubles every placement
  (duplicate pins at coincident points). Always re-seed before re-running.

## Coverage gaps in the existing suites

1. No test combines `ELECSETUP` with `QSAVE` on a pre-AC1018 drawing (hides D1).
2. No test places a symbol into a drawing that was **opened** rather than created in the
   same session (hides D3, the edit path).
3. No test asserts `ELECSETUP` settings survive a save/reload — or even a second
   `ELECSETUP` in the same session (hides D2).
4. No test asserts a placed symbol's `X<n>TERM<nn>` attributes land on the symbol's own
   definition points (hides D4).
5. No test checks that `ELECWIRELIST` net labels are unique and sequential, or that they
   match the `N1..Nn` labels the MCP read and the reports use (hides D5).
6. No test saves through the automation (`ocs_execute op:save`) path into a lossy target
   and then re-reads the electrical contract (hides D7).
7. No test asserts that `op:run` is refused, or at least reported as blocked, while a
   modal owns input (hides D8).
8. No test drives the GUI at all: the ribbon tab, `ELECSTART`, `ELECPALETTE` and
   `ELECSETUPEDIT` are only covered by headless command rows, so a modal-only command such
   as `ELECLIST` is never exercised in the form a user meets it.
9. No test drives the project lifecycle end to end at project scope — `ELECEXPORT`,
   `ELECREV SNAPSHOT/LIST/RESTORE`, `ELECSYSTEM SAVE`, `ELECBUILT`, `ELECOPTION` are covered
   only by their "no project is loaded" guards (hides D11, D12).
10. No test asserts that the manifest row written by `ELECMAKESYMBOL` / `ELECMAKEMODULE`
    parses back with the same `scale` and `standard` the shipped rows carry, nor that an
    authored instance gets a designator (hides D13, D15).
11. No test checks that every command named in `docs\automation\guides\ecad.md` actually
    resolves — a one-row batch per documented name, asserting no `✕ INVALID: Unknown
    command`, would have caught D10 and D14.
12. `src\app\view\conversion_loss_tests.rs` covers the dialog's layout but not the target
    label, so the `format_for_version` catch-all in D16 is untested; and no test saves a
    drawing through the GUI modal path (`conversion_loss_save_anyway`) and re-reads the
    electrical contract afterwards.

## Suggested order of attack

1. **D1** — headless/batch saves must not be gated on a modal nobody can answer. Either
   auto-answer `ConversionLoss` when `main_window.is_none()` (as `save_as_from_command_line`
   already does at src\app\commands\fileops.rs:838) or print the refusal and exit non-zero.
   This is the one that destroys work.
2. **D2** — make `write_setup_properties` write the `WD_M` attributes it later reads, or make
   `read_setup` prefer the custom properties it wrote. Today the writer and the reader
   disagree, so every template-based sheet silently ignores numbering setup.
3. **D4** — when `ELECLIB` skips an already-defined block, measure its ATTDEF points from the
   existing block record instead of skipping the measurement (the "0 attribute definition
   point(s)" line is the tell).
4. **D3** — repopulate `library_index` from the drawing's own block records on `OPEN`, so a
   reopened sheet places symbols with pins and correct next-tags.
5. **D7 / D8** — one loss detector shared by GUI, batch and automation; and a blocked/`error`
   result from `op:run` while a modal owns input.
6. **D5, D6, D13, D14, D11, D12, D15, D9, D10** — correctness and hygiene, each with a
   regression row added from the coverage table above.

---

## Fix pass (2026-11-12) — what changed and how it was verified

All seventeen defects are fixed in the working tree. Against HEAD 9b1225a the change is
**79 files, +1134 / −370**: 17 source, tooling, test and documentation files
(**+868 / −104**) plus the 62 regenerated library files that make up the D6 pin
renumbering. Every fix was re-run against a rebuilt release binary, and the three shipped
suites were re-run against that binary.

Two of the original findings turned out to be softer than filed once the fix was in place,
and that is recorded in the status table rather than hidden: **D13** (the four-field
manifest row parses fine — the reader treats scale and standard as optional) and **D10**
(the doc sentence was wrong about more than the command name).

### Status

| # | Status | Fix | Verified by |
|---|---|---|---|
| D1 | **fixed** | `queue_native_save` (src\app\update\file.rs) no longer waits on a dialog nobody can answer while a script is driving: every loss is printed with `command_line.push_warning(loss)` and the file is written. The GUI path still gets `ask_conversion_loss`. | probe `target\qa\drive10\d12.txt` on a copy of the shipped `templates\iec\A4_landscape.dxf` (AC1009): `Saved: …\d12.dxf` is printed, stderr carries `*Warning* DXF files older than 2004 do not store the drawing properties; 16 filled-in … will be lost`, exit 0. Before the fix the save vanished with no message. Unit test `a_script_save_names_the_losses_and_writes_instead_of_freezing`. |
| D2 | **fixed** | `read_setup` now overlays the 16 `ELEC_*` custom properties on top of `WD_M` instead of ignoring them when a `WD_M` exists; new `write_setup_contract` writes the properties **and** updates the matching ATTRIBs of every existing `WD_M` insert. | same probe: `ELECSETUP TAGINC=3 TAGSTART=101` then a bare `ELECSETUP` reads "from this drawing's WD_M block … start 101 step 3". Unit test `a_template_sheet_keeps_the_setup_it_was_given`. |
| D3 | **fixed** | `elec_place` calls `ensure_library_index()` first. Root cause was that it read the static `library_index()` without ever loading it, so a process that had only `OPEN`ed a drawing had an empty index and fell back to a `TAG1`/`CAT` template with no pins. | probe `target\qa\drive11\d3.txt` on the reopened `lib2018.dxf`: `ELECLIST` now lists three Fuse devices `-F1 -F2 -F3` and the placements carry pins (proved by d4 below). Previously: 0 pins and a duplicate `-F1`. |
| D4 | **fixed** | new `block_attribute_offsets(document, block)` measures ATTDEF points (relative to the block record's base point) **from the drawing's own block record**, and `elec_place` falls back to those per-placement offsets when `ELECLIB` skipped an already-defined symbol. Deliberately not cached — two drawings can define one block name differently. | probe `target\qa\drive11\d4.txt`: two `ELECPLACE FUSE` at 160,40 and 200,40, then `ELECWIRE 172.7,40 → 187.3,40` → `ELECWIRELIST: 1 wire segment(s), 4 pin(s) -> 1 net(s)` and `net 1: 1 wire(s) -> -F2:1, -F3:1`. The placed pins land exactly on the shipped block's own ATTDEF points (`FUSE` base point (−0.5,0) × scale 25.4 → `X4TERM01@(147.30,40.00)`, `X1TERM01@(172.70,40.00)`). Before the fix the pins were ~31 units below the body and the wire joined nothing. (The pin numbers in that transcript are as they read **before** the D6 renumbering; the same geometry is re-verified after it by `target\qa\d6\d6.log`, where the identical coordinates give `net 1: 1 wire(s) -> -F1:2, -F2:1` — the ATTDEF points did not move, only their tag text.) |
| D5 | **fixed** | `netlist_from_geometry_impl` (crates\elec\src\geometry.rs) renumbers `net.id = index + 1` after sorting, so the two id spaces (wire ids vs anchor ids) can no longer collide. `net.id` is display/lookup only and is never persisted, so `ELECWIRELIST`, the CSV rows, `ELECWIRENUMBER` and the MCP `N1..Nn` labels now agree. | probe `target\qa\drive11\d5.txt`: a loose wire plus the anchored net → `net 1: 1 wire(s) -> -F1:1, -F2:1` and `net 2: 1 wire(s) -> (no devices)` — distinct labels where both were previously `net 1`. Unit test `a_wire_run_and_a_pin_only_net_do_not_share_a_label`. |
| D6 | **fixed** | Renumbered at the generator, not by hand: `tools\qet_import.py` now numbers the right-hand group `start=len(left) + 1`, `tools\neca_symbols.py` emits `X1TERM02` for the second end, `tools\make_library.py` gained `canonical_pins(pins)` (group by side digit, order X4 then X1, sort each group by (−y, x), one running counter) applied to whatever pin source a symbol uses, and the app's own minters were fixed the same way (`ELECMAKEMODULE` offsets the right edge by the channel count; `ELECMAKESYMBOL PINS=n` counts across both edges). Both catalogues were regenerated. Full write-up: [D6 — the shipped library, renumbered at the generator](#d6--the-shipped-library-renumbered-at-the-generator). | `py -3.13 tools\pin_check.py` → `pin_check: 146 pinned symbol(s), every connection point uniquely numbered`, exit 0 (was 60 colliding symbols). `py -3.13 tools\make_library.py --check` and `--standard nec --check` both still in sync. Probe `target\qa\d6\d6.log`: two `ELECPLACE FUSE` wired pin-to-pin → `net 1: 1 wire(s) -> -F1:2, -F2:1` and `unconnected: -F1:1, -F2:2` — the two ends of a fuse are now different pins. `tests\ecad\run_smoke.py:656` asserts the module numbering `{X4TERM01, X4TERM06, X1TERM07, X1TERM12}`. |
| D7 | **fixed** | `SaveLosses` (src\app\automation.rs) gained a `properties` field fed by `crate::io::drawing_properties_lost_on_save`, the refusal names the property losses, and the JSON reports `properties_lost_on_save`. The native-save arm refuses up front only when `main_window.is_none()` — a windowed session's contract is the dialog plus `conversion_loss_save_anyway`. The `#[cfg(wasm32)]` arm keeps its unconditional refusal on purpose: it has no `queue_native_save` path (`save_to_bytes` + base64), so no dialog is possible there. | `tests\ecad\verify_mcp_ecad.py` 16/16 against the rebuilt binary, including the lossy-save assertions. |
| D8 | **fixed** | the automation `run` result now carries `"modal_open": <Modal name>`, and a pre-existing modal is reported as a blocker only when `main_window.is_some()`. Headless, a modal does **not** swallow the command line — the earlier belief that it did was wrong, and the failing test proved the line really ran. | unit test `a_modal_left_open_is_named_instead_of_faking_a_blocker`. |
| D9 | **fixed** | the `LibraryEntry` literal in `mod library_manifest_tests` gained the `standard` field that D9's compile error named. | `cargo test --release --lib` compiles and runs: **4445 passed / 0 failed / 34 ignored**. |
| D10 | **fixed** | `docs\automation\guides\ecad.md:48` rewritten. The original sentence was wrong on two counts: there is no `ELECJUNCTION` command (it is `ELECJUNCTIONS`, src\app\commands\elec.rs:7310), and the dots it draws are **circles on the `JUNCTIONS` layer**, not inserts — `wire_graph` collects junction points only from those circles (`:978-982`), so `DOT`/`WDDOT`/`JUNCTION` inserts (recognized by `crates\elec\src\pins.rs:81-82` as `is_non_device_block`) never join wires, they are only never mistaken for devices. | doc read-back; the rewritten sentence states the command, the entity type, the layer, and what the imported inserts actually do. |
| D11 | **fixed** | `ELECEXPORT` added to the early-arg list (src\app\command_driver\utilities.rs), so it no longer runs twice. | probe `target\qa\drive11\hyg.log`: the usage line `ELECEXPORT: give a folder, e.g. ELECEXPORT publish dxf` appears **once**, where the original run printed it twice. |
| D12 | **fixed** | `write_revision` (src\app\commands\elec_revision.rs) collects `revision_files(folder)` up front and errors with `no project files in <folder> to snapshot — save the sheets first` before any directory is created. | unit test `an_empty_project_folder_is_refused_before_anything_is_written` — asserts the error and that `read_revisions` stays empty, i.e. no order number is burned. The original repro (a loaded project whose folder holds no sheets) is now also blocked one step earlier by the pre-existing "no project is loaded" guard at `:89`. |
| D13 | **fixed** | `store_library_symbol` now **replaces** an existing row for the symbol instead of appending a second one, and its fallback header is the 5-column form. The row itself is still four fields (`symbol`/`class`/`attributes`/`scale`) — re-verified after the fix, the authored row reads `QA_D13MOD`, `Module`, `<16 attribute tags>`, `1`. | probe `target\qa\drive11\hyg.log` + manifest read-back. **This weakens the original finding**: `read_library_manifest` (src\app\commands\elec.rs:243-292) already treats the scale and standard columns as optional — a missing scale parses to `1.0` ("as drawn", which is correct for a symbol the user drew at working size) and a missing standard parses to empty. So the four-field row is not corrupting the load; the harm that is real is the **duplicate row on re-author** (now fixed) and writing into the **shipped** catalogue, which is also fixed — see [D13 — authoring no longer writes into the shipped catalogue](#d13--authoring-no-longer-writes-into-the-shipped-catalogue). |
| D14 | **fixed (doc)** | the phantom `ELECZONE` row deleted from `docs\automation\guides\ecad.md:65`. | doc read-back; the command still does not exist, which is now consistent. |
| D15 | **fixed** | `ELECMAKEMODULE` computes the class, calls `::elec::next_tag` and writes the designator into `TAG1`. | probe `target\qa\drive11\hyg.log`: after `ELECMAKEMODULE QA_D13MOD 6` the `ELECLIST` census shows `Module  -M1` — the instance carries a designator and is visible to every tag-based command. Previously it appeared as `Tb1` with an empty tag. |
| D16 | **fixed** | new `written_format(version, is_dxf)` in src\io\mod.rs:1793 (`AC1009`+dxf → "DXF R13", `AC1009`+dwg → "DWG R11/R12", `AC1012` → R13) feeds `PendingConversionSave::format_name` (src\app\mod.rs:2246) and therefore the modal title. `format_for_version` was left alone deliberately: it feeds the Save-dialog dropdown and `parse_save_format` maps any unrecognised string to AC1032, so emitting "R13" there would silently *upgrade* files. The acadrust DXF writer itself maps AC1009→AC1012 (R12 predates the handle object model); DWG keeps AC1009. | unit tests `mod written_format_tests` (src\io\mod.rs:1804-1831) assert the exact labels for AC1009/AC1012 in both containers and every modern version. **Note:** the headless probe cannot show this — after D1's fix the headless path prints only the loss warning (`*Warning* DXF files older than 2004 do not store the drawing properties; 16 filled-in properties will be lost`), and the format name appears solely in the windowed modal that headless no longer opens. |
| D17 | **fixed** | new `drawing_tags(document)` walks model-space inserts, skips the setup block, reads the first tag ATTRIB and dedupes; `elec_place` builds the family census from it instead of from the wire-graph anchors. | probe `target\qa\drive11\d4.txt`: the seed's pin-less `-F1` is now counted, so the two new placements are `-F2` and `-F3`, not a duplicate `-F1`. Unit test `a_pin_less_symbol_still_owns_its_tag`. |

### Regression evidence on the rebuilt binary

- `cargo test --release --lib` — **4445 passed, 0 failed, 34 ignored** in 31.60s
  (was: did not compile). The new electrical regression tests are included.
- `cargo test --release -p elec` — **132 passed** (was 131; the new net-label test).
- `cargo build --release` — Finished in 6m 04s.
- `tests\ecad\run_smoke.py` — **134/134** (was 127/127; the seven new checks are the
  authored-catalogue section below).
- `tests\ecad\run_features.py` — **44/44**, exit 0. (The 14 failures in the original run were
  the Low-mandatory-level sandbox, not defects; with a normal-integrity child process they
  pass, which confirms the "not defects" call in that section.)
- `tests\ecad\verify_mcp_ecad.py` — **16/16**, exit 0.
- `py -3.13 tools\make_library.py --check` — in sync, 78 symbols;
  `--standard nec --check` — in sync, 414 symbols.
- `py -3.13 tools\pin_check.py` (new) — `146 pinned symbol(s), every connection point
  uniquely numbered`, exit 0.
- Probes on the rebuilt binary: `target\qa\drive10\d12.log` (D1+D2 on a shipped AC1009
  template), `target\qa\drive11\d3.log`, `d4.log`, `d5.log` (D3/D4/D5/D17),
  `target\qa\drive11\hyg.log` (D11, D15, and the D13 manifest row),
  `target\qa\drive11\rev.log` + `p1.log`/`p2.log` (D12, and a normal snapshot still works:
  `snapshot 001 — 3 file(s), 886.7 KB`), `target\qa\d6\d6.log` (D6 pin numbering),
  `target\qa\d6\d13a.log` / `d13b.log` (D13 authoring and reload).
- New unit tests: `a_script_save_names_the_losses_and_writes_instead_of_freezing` (D1),
  `a_template_sheet_keeps_the_setup_it_was_given` (D2),
  `a_wire_run_and_a_pin_only_net_do_not_share_a_label` (D5, in `crates\elec`),
  `a_modal_left_open_is_named_instead_of_faking_a_blocker` (D8),
  `an_r12_target_is_named_by_what_the_writer_emits` and
  `an_offered_target_keeps_the_save_dialog_wording` (D16),
  `a_pin_less_symbol_still_owns_its_tag` (D17),
  `an_empty_project_folder_is_refused_before_anything_is_written` (D12).
- New shipped-suite coverage: `run_smoke.py` section **17. the authored catalogue: never
  the shipped folder, loaded on top of it** (seven checks, D13), and the module pin-number
  assertion in section 5 was rewritten to `{X4TERM01, X4TERM06, X1TERM07, X1TERM12}` (D6).

### D6 — the shipped library, renumbered at the generator

The libraries are **generated**, so the fix belongs in the generator. Before touching
anything, `py -3.13 tools\make_library.py --check` proved the committed files match the
generators (`library is in sync: 78 symbols, 78 manifest rows` and
`library is in sync: 414 symbols, 414 manifest rows`, exit 0). That makes regeneration
provably safe, and `--check` afterwards proves only the intended lines moved.

The bug is one line in `tools\qet_import.py:244-253`: the left and right connection points
are sorted into two groups and **each group is numbered from 1**, so every two-sided symbol
gets `X4TERM01` and `X1TERM01`. `tools\make_library.py` preferred that imported pin list
over its own hand-authored constants, and its own multi-pin constants had the same shape
(`PINS_NETWORK_SWITCH`, `PINS_HMI`, `PINS_IO_MODULE`, and the inline lists for
`CONNECTOR_2P` / `CONNECTOR_4P`). `tools\neca_symbols.py:269` emitted
`X4TERM01` + `X1TERM01` for every 2-pin NEC symbol.

The rule applied everywhere, identically:

1. keep each pin's **side digit** (`X4` = left, `X1` = right — the only two digits the
   library uses, at x = −0.5 and +0.5 respectively);
2. order the groups X4 then X1, sort each group by (−y, x);
3. run **one counter across both groups**, so the right-hand pins continue the numbering.

Applied as:

- `tools\qet_import.py` — the right group is now `enumerate(right, start=len(left) + 1)`;
- `tools\neca_symbols.py` — the second end is `X1TERM02`;
- `tools\make_library.py` — the five colliding pin constants were renumbered, and a new
  `canonical_pins(pins)` (group by side digit, order X4 → X1 → other → non-pin, sort by
  (−y, x), one running counter, list returned in its original order so only tag text moves)
  is applied in `build_symbol` to **whatever** pin source a symbol uses, so a stale
  intermediate can never ship two ends both numbered 1;
- the app's own minters, which had the same bug: `ELECMAKEMODULE` now offsets the right
  edge by the channel count, and `ELECMAKESYMBOL PINS=n` counts across both edges.

Both catalogues were regenerated. The diff audit is the proof that nothing else moved:
across the 62 changed library files there are **532 changed lines — 290 pin-tag lines,
122 prompt lines, 120 manifest rows, 0 other**. No geometry line changed in either set.
60 symbols were touched, which is exactly the 54 starter + 6 NEC collisions found by the
census (`target\qa\d6\census.txt` before, `census_after.txt` after: 146 pinned symbols,
`0 symbol(s) collide on a TERM number`).

Two intermediates were deliberately **not** patched: `tools\qet_symbols.json` (59,639 B)
and `tools\neca_shapes.json` (746,574 B) still carry their original colliding tags. They
are caches of external CAD sets, `make_library.py` now canonicalises whatever it is given,
and the two importers are fixed, so a fresh import emits unique numbers anyway. Patching
them would be churn in a cache.

One convention nuance worth recording: real AutoCAD Electrical numbers terminals **per
direction** — `reports\real-sheet-model.md:218-219` shows symbols carrying `X8TERM01`,
`X2TERM01`, `X1TERM01` and `X4TERM01` together — which is why the shipped library mirrored
that. This app's netlist does not: `crates\elec\src\pins.rs:22-28` reads the digits after
`TERM` as the pin identity and the digit after `X` as only the approach side. Within this
app the numbers must therefore be unique per symbol, or the wire list prints
`-K1:1 -> -K1:1`. `tools\pin_check.py` (new, in the pattern of `tools\parts_check.py`,
listed in README) enforces that across both catalogues and also checks that each symbol's
pin tags agree with its `library.tsv` row.

### D13 — authoring no longer writes into the shipped catalogue

`ELECMAKESYMBOL` and `ELECMAKEMODULE` wrote into whatever catalogue `ELECLIB` had last
loaded, which in normal use is `library\starter` or `library\nec`. That mutates delivered
data, dirties a git-tracked manifest, and lets an authored symbol **replace a shipped one**
of the same name.

The fix is a per-user overlay:

- `shipped_library_root()` / `is_shipped_library()` identify anything under `<cwd>\library`;
- `user_library_folder()` is `%APPDATA%\AryaCAD\elec_user_library`;
- `authored_library_folder()` — what both authoring commands now use — writes to the loaded
  folder when the user chose a catalogue outside the shipped tree, and to the overlay
  otherwise;
- `ELECLIB` merges the overlay **on top of** the shipped folder it is loading, so an authored
  symbol is available in the next session without the user doing anything;
- `store_library_symbol` creates the destination folder if it does not exist yet.

The merge is gated on the loaded folder being shipped, so a harness that loads a temp copy
of the library is unaffected.

Verified end to end (`target\qa\d6\d13a.log`, `d13b.log`): authoring reports
`written to ...\Roaming\AryaCAD\elec_user_library\QA_D13MOD.dxf`, the shipped
`library\starter\library.tsv` stays at its delivered 79 lines with no `QA_` row and no new
`.dxf`, and in a **later** session `ELECLIB IEC` reports `loaded 79 symbol(s) ... 79 in the
catalogue` and `ELECINSERT QA_D13MOD` places it as `-M1`. `run_smoke.py` section 17 now
locks all of that in as seven shipped checks.

The four-field manifest row described in the original finding is still four fields, and
still harmless for the reason given in the status table — `read_library_manifest` treats
scale and standard as optional. The duplicate row on re-author is fixed: two consecutive
`ELECMAKEMODULE` runs leave exactly one row.

### One residual limitation, not a defect

D2 is fixed for everything the shipped `WD_M` block can hold. The shipped schema has nine
ATTRIBs — `TAGFMT TAGMODE TAG-START TAG-RSUF WIREFMT WIREMODE WIRE-START XREFFMT SHEET` —
and **no field for the increment**. The increment lives only in the `ELEC_TAGINC` /
`ELEC_WIREINC` custom properties, and pre-AC1018 DXF cannot store custom properties at all.
So on a shipped AC1009 template, after save and reopen, `ELECSETUP` reads back
"start 101 **step 1**": start, format, mode and suffixes persist, the step does not. On a
modern AC1032 drawing the step round-trips.

This was measured, not inferred (`target\qa\drive10\d12b.log`: save `d12.dxf`, reopen, read
`WD_M`). It is left as a documented limitation rather than fixed by inventing
`TAG-STEP`/`WIRE-STEP` ATTRIBs, because that would change the schema of a block every
shipped template carries and that AutoCAD Electrical also reads. If the increment is to
survive on old templates, the honest fix is to add the fields to the shipped `WD_M` blocks
and to the harvest profile, not to smuggle them through an ATTRIB the reader does not
expect. Note that real AE `WD_M` does carry `TAGINCR`/`WIREINCR`; this app's does not.

### Still open

No defect from this report is left unfixed. What remains is follow-on work, in rough order
of value:

- **The side digit is inconsistent across the codebase.** The shipped library uses `X4` for
  the left edge, `docs\QUICKSTART.md:188` says the left edge uses `X2TERMnn`, and
  `ELECMAKESYMBOL PINS=n` mints `X2` on the left. Nothing in the tests asserts `X2TERM`, and
  `pin_number` ignores the digit, so this is cosmetic today — but three different spellings
  for one side will confuse anyone authoring symbols by hand. Pick one and align the docs,
  the generator and the app.
- **The coverage gaps listed above are still gaps.** The new unit tests cover D1, D2, D5, D8
  and D17, and `run_smoke.py` section 17 covers the authored catalogue, but items 9–11
  (project-scope lifecycle, manifest round-trip, documented-command resolution) have no test
  driving them. Item 11 in particular is cheap: every command named in
  `docs\automation\guides\ecad.md` could be fed to the command driver and asserted to
  resolve, which is exactly how D14 was found by hand.
- **The `WD_M` increment limitation** described just above, if old-template support matters.
- **The harness now parks `%APPDATA%\AryaCAD\elec_user_library`** for the duration of a run
  (both `run_smoke.py` and `run_features.py`, restoring it in `finally`), because the exact
  catalogue counts in those suites are facts about the shipped library and a per-user
  overlay legitimately changes them. Any future suite that counts catalogue entries needs the
  same isolation; the alternative — loosening the assertions to `>=` — would have hidden real
  regressions.

