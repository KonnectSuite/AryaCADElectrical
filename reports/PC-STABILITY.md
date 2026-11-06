# PC stability — why this machine restarts during builds

Written 2026-11-06 after the second unexpected reboot of the day, while
`cargo build --release` was compiling this repository. Evidence is from the
Windows event log and the dump files on this machine; it is not a guess.

## The machine

| | |
|---|---|
| System | CyberPowerPC "GamingPC" prebuilt |
| Motherboard | ASUS PRIME Z790-V WIFI |
| BIOS | 1666 (2024-09-26; this is the release that added Intel microcode 0x12B) |
| CPU | Intel Core i9-14900**KF**, 24 cores / 32 threads, 3.2 GHz base |
| CPU microcode reported | `0x12B` (Update Revision `2B 01 00 00`) |
| RAM | 63.8 GB, 49.0 GB free at the time of the crash — **not** out of memory |
| Disk | 616 GB free on C: |
| OS | Windows 11 Home 10.0.26200 |

## What the logs say

**Kernel bugchecks (unexpected reboots):**

| When | Bugcheck | Note |
|---|---|---|
| 2026-11-06 01:59:57 | `0x00020001` (`HYPERVISOR_ERROR`) | dump `110626-12125-01.dmp` |
| 2026-11-06 08:26:05 | `0x0000001e` (`KMODE_EXCEPTION_NOT_HANDLED`) | arg1 `0xffffffffc0000096` = privileged instruction, dump `110626-10031-01.dmp` |
| 2026-10-19 18:24 | (earlier dump) | `101926-8218-01.dmp` |
| 2026-10-14 13:39 | (earlier dump) | `101426-9812-01.dmp` |
| 2026-09-02 17:42 | (earlier dump) | `090226-9546-01.dmp` |

**Kernel-Power 41 ("rebooted without shutting down") events in the last 30 days: 8.**
Different bugcheck codes each time is itself a symptom: a single failing driver
usually repeats one code.

**User-mode crashes of the compiler** — `rustc.exe` 1.99.1.0, exception
`0xc0000005` (access violation) in `rustc_driver-58995fd3f1c8ecc6.dll` — **8 times
today**, at 01:52, 06:48, 08:18 and 08:20 (two each), in addition to the two
kernel bugchecks. No WHEA (machine-check) records were logged.

## Interpretation

This is the documented **Intel Core 13th/14th-generation "Vmin Shift"
instability**, not a fault in this repository, and not a memory shortage:

* The 14900KF is one of the affected parts. Intel's own guidance describes random
  crashes, BSODs with *different* stop codes and failures under heavy sustained
  load — exactly the pattern above.
* The processor already carries microcode `0x12B` (BIOS 1666). Intel later shipped
  **`0x12F`** (2025) as a further mitigation; this board has not been updated to it.
  Microcode prevents further degradation; it does not repair a CPU that is already
  unstable.
* A prebuilt may also be running an aggressive board profile (ASUS
  "Multi-Core Enhancement"/performance defaults, XMP) that the failure is very
  sensitive to.
* `rustc` is the most demanding sustained all-core load on the machine. Every crash
  today happened while compiling the ~150 MB `OpenCADStudio` release build; normal
  desktop use has not reproduced it.

Reference material (external):
- Intel microcode 0x12F supplement: <https://community.intel.com/t5/Mobile-and-Desktop-Processors/Intel-Core-13th-and-14th-Gen-Vmin-Shift-Instabilty-Update-New/m-p/1686948>
- ASUS PRIME Z790-V WIFI BIOS list (1666 is the 0x12B release): <https://www.asus.com/motherboards-components/motherboards/prime/prime-z790-v-wifi/helpdesk_bios?model2Name=PRIME-Z790-V-WIFI>

## What to do, in order

1. **Update the BIOS** to the latest release for PRIME Z790-V WIFI (the one with
   Intel microcode 0x12F or newer). After flashing, **load the "Intel Default
   Settings" / "Baseline" profile** — not "Performance" — and save.
2. In the BIOS, while testing: disable Multi-Core Enhancement and any ASUS
   performance/overclock preset, disable XMP/EXPO (run RAM at JEDEC defaults),
   and keep Intel's power limits (PL1/PL2) at the default values.
3. Re-run the workload that reproduces it. If it still bugchecks or `rustc` still
   faults, open an **Intel warranty claim** (13th/14th-gen boxed CPUs have an
   extended 5-year warranty); the CPU is the suspect, not the code.
4. Until that is done, do not run long all-core builds on this machine. Options:
   * build with the compiler restricted to ~4 cores (`-j 1` plus an affinity mask)
     — slower, much lower current draw, far less likely to trip it;
   * or build on another machine/CI and copy `target\release\OpenCADStudio.exe`
     back for headless verification.

## Work protection

The pin-anchoring work was committed as `d161ed7` before attempting another
build, so a reboot cannot lose source changes. The build product
(`target\release\OpenCADStudio.exe`) still needs one successful build to verify
the change; `cargo check --release --tests` passed and the 130 crate tests pass.
