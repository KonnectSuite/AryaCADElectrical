# Draft reply for Issue #78

Paste as a GitHub comment on https://github.com/HakanSeven12/OpenCADStudio/issues/78

---

@HakanSeven12 @schoeller — following up with a concrete architecture proposal and an implementation ready for review.

### Proposal

I've drafted a **QGIS-style add-on model** on my fork:

- **Spec:** [`docs/plugin-architecture.md`](https://github.com/mf4633/OpenCADStudio/blob/feature/plugin-host/docs/plugin-architecture.md)
- **Scaffold:** [`docs/plugin-template/`](https://github.com/mf4633/OpenCADStudio/tree/feature/plugin-host/docs/plugin-template)
- **Framework PR branch:** [`feature/plugin-host`](https://github.com/mf4633/OpenCADStudio/tree/feature/plugin-host) — **host only, no Storm Sewer in core**

**Three layers:** host core → add-on package (`plugin.toml`, ribbon, commands) → optional headless engine crate. Domain data lives on DWG entities (XDATA), not a proprietary project DB.

**Phase 1 (in this PR):** in-process plugins via `inventory::submit!(PluginRegistration)`, `HostSession` API, per-document plugin state, command routing without editing `commands.rs`.

**Phase 2:** user install folder + dynamic `.dll`/`.so` with the same `plugin.toml`.

### PR ready for review

I can open a PR against upstream with **only the generic plugin host** — no civil/hydraulics tab in core. Storm Sewer stays on a separate branch/repo as the reference consumer: [`feature/storm-sewer-module`](https://github.com/mf4633/OpenCADStudio/tree/feature/storm-sewer-module).

### Re: script languages (@schoeller, #29)

Agree this shouldn't be either/or. Suggested sequencing:

1. Native Rust add-ons + stable `HostSession` / `ocs_plugin_api`
2. Python (or similar) as **bindings over that same API** — one extension surface, two authoring paths

Phase 1 defers embedded scripting until the native API is stable.

### Questions for maintainers

1. `ocs_plugin_api` as a workspace crate with semver — OK?
2. Should the main repo ship **zero** discipline modules, or optional built-ins for dev?
3. Priority: extract API crate (1b) vs dynamic loading (2)?

Happy to open the framework PR whenever timing works. Storm Sewer can follow as a separate installable add-on once the host lands.

— Michael