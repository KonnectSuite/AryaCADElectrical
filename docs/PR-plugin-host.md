# PR: Add plugin host (Phase 1)

**Target:** `HakanSeven12/OpenCADStudio`  
**Branch:** `feature/plugin-host` on `mf4633/OpenCADStudio`  
**Related:** Issue #78 (Storm Sewer interest check / extension architecture)

## Summary

Introduces a QGIS-style add-on architecture so discipline-specific tools (storm sewer, sanitary, geotech, …) can ship **outside** the core application. Core ribbon tabs (Home, Model, View, …) are unchanged. Add-on packages register ribbon + commands through a single `PluginRegistration` hook.

**This PR contains only the generic host** — no Storm Sewer tab, no `stormsewer` engine crate, no civil/hydraulics code in `src/modules/`.

## What's included

| Area | Change |
|------|--------|
| `src/plugin/` | Manifest, registry, `BuiltinPlugin` trait, `try_dispatch` |
| `src/app/plugin_host.rs` | `HostSession` adapter (document, undo, tab state, command line) |
| `src/app/document.rs` | Per-tab `plugin_state` map keyed by plugin id |
| `src/app/commands.rs` | Plugin dispatch before legacy command match |
| `src/command/mod.rs` | Generic `ObjectPickHit` + acquisition hooks on `CadCommand` |
| `build.rs` | Skips dirs with `plugin.toml` (add-ons register via plugin host) |
| `src/ui/ribbon/mod.rs` | `all_ribbon_modules()` = core tabs + plugin tabs |
| `docs/plugin-architecture.md` | Accepted architecture spec |
| `docs/plugin-template/` | Scaffold for new add-on authors |

## Design principles

1. **`src/plugin/` must not import domain modules** — keeps core mergeable.
2. **One package, one registration** — `plugin.toml` + `register.rs` + `BuiltinPlugin`.
3. **DWG round-trip** — plugins persist domain data on entity XDATA (documented per add-on in `PLUGIN.md`).
4. **Phase 2 ready** — same `plugin.toml` layout for future dynamic `.dll` loading.

## How to add an add-on (after merge)

Copy `docs/plugin-template/` → `src/modules/<name>/`, implement `BuiltinPlugin`, add `pub mod <name>;` to `modules/mod.rs`. No edits to `commands.rs`.

External repos: depend on extracted `ocs_plugin_api` (Phase 1b, follow-up PR).

## Follow-up (not in this PR)

- `ocs_plugin_api` workspace crate (semver-stable host surface)
- Dynamic plugin loading (`%APPDATA%/OpenCADStudio/plugins/`)
- Python/scripting bindings over host API (#29)
- Storm Sewer add-on — separate repo/PR: `mf4633` branch `feature/storm-sewer-module`

## Testing

```powershell
cargo build
cargo test --lib
```

A minimal **`demo_plugin`** add-on registers at compile time for smoke tests (`DP_HELLO` command, Demo Plugin ribbon tab). Remove or gate it before release if maintainers prefer zero in-tree add-ons.

## Review questions

1. OK to add `inventory`-based plugin registration (same pattern as `CommandRegistration`)?
2. Should built-in add-ons ever live in the main repo, or only the host + template?
3. Priority for Phase 1b (`ocs_plugin_api` crate) vs Phase 2 (dynamic loading)?