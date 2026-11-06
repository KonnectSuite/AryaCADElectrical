# Migrating model tiles to `iced::pane_grid`

Status: **planned, not started.** This is a design note for a future migration of
OpenCADStudio's custom model-space tiling (`model_tiles`) onto iced's
`pane_grid` widget. It records the motivation, blockers, decisions and a phased
plan so the work can start cold later.

## Goal

Replace the bespoke rect-based tiling with `pane_grid` to get drag-resize,
drag-to-swap, and maximize/restore "for free", and to delete the custom
hit-testing / edge-drag / split math.

Non-goal: **floating viewports.** `pane_grid` is a tiling widget — panes never
overlap or float. If floating palettes/viewports are ever wanted, that is a
separate effort (multi-window via the iced `daemon` API, or a custom z-ordered
overlay stack). Do not pick `pane_grid` expecting floating.

## Current architecture (what we'd replace)

Tiles are normalized rectangles in a flat `Vec`, each carrying its own view
state. Rendering is a **single wgpu shader with one scissor pass per tile**;
overlays are a single canvas spanning the whole viewport, positioned at the
active tile.

Core types / functions (`src/scene/mod.rs`):
- `struct ModelTile { rect, camera, render_mode, grid_on, snap_on }` — per-pane state.
- `model_tiles: RefCell<Vec<ModelTile>>`, `active_model_tile: Cell<usize>`.
- `struct TileEdge { orient, ... }`, `enum TileEdgeOrient` — draggable dividers.
- `split_active_model_tile(horizontal)` — binary split of the active tile.
- `set_model_tile_layout(rects)` — VPORTS presets / reset.
- `set_active_model_tile_at(nx, ny)` — hover/click activation by point.
- `hit_model_tile_edge(...)`, `move_model_tile_edge(...)`, `model_tile_edges()`,
  `collapse_small_model_tiles(...)` — divider drag + cleanup.
- `active_model_tile_bounds(vw, vh)` — active tile's canvas rect (drives grid /
  crosshair / UCS / viewcube placement).
- `model_tile_grid_views(vw, vh)` — per-tile grid params (added for #121).
- `viewports_to_render(...)` — the per-tile scissor passes for the single shader.

DWG round-trip (`src/scene/mod.rs`):
- `save_model_tiles_to_vports()` / `restore_model_tiles_from_vports()` —
  `ModelTile ↔ *Active VPort` entries (rect via `tile_rect_to_vport` /
  `vport_to_tile_rect`, camera via `vport_from_camera` / `camera_from_vport`,
  plus `render_mode`, `grid_on`, `snap_on`).

App glue:
- `src/app/update.rs`: `Message::SplitModelViewport`, hover activation in
  `ViewportMove` (~2481), click activation in the left-press path (~3104),
  edge-drag (~2308), `sync_vport_display` / `adopt_view_display` (grid/snap
  follow the active viewport).
- `src/app/view.rs`: builds the overlay (`tile_edges`, grid, viewcube,
  control chip) at `active_model_tile_bounds`; the top-left `viewport_controls`
  chip emits `SplitModelViewport`.

## Why this is not a drop-in

### 1. Rendering architecture (the hard part)
Today: **one shader, N scissor passes** — shared pipeline/buffers, cheap.
`pane_grid` wants each pane's content to be a normal `Element`, so each pane
would hold its **own `shader` widget** → N independent shader instances. That
changes how cameras, pipelines and GPU buffers are shared, and splits the
single full-canvas overlay into one overlay per pane.

Two viable shapes:
- **(A) Per-pane shader + per-pane overlay.** Cleanest conceptually (a pane is a
  self-contained viewport: shader + its own grid/snap/crosshair/viewcube). Big
  refactor; need to confirm N wgpu shader widgets share resources acceptably and
  perform.
- **(B) Hybrid — `pane_grid` for layout only, keep the single-shader-scissor.**
  Tempting, but `pane_grid` does not hand you the computed pane bounds for an
  external renderer; you'd track split ratios/orientation yourself and recompute
  rects — i.e. re-implement most of what `model_tiles` already does. Low payoff.

Decision lean: **(A)**, gated on a spike (below). If the spike shows shader
sharing/perf is bad, abandon or stay hybrid-custom.

### 2. VPORT round-trip fidelity
The DWG VPORT table stores **arbitrary** per-viewport rects (lower_left /
upper_right). `pane_grid` is a **binary split tree**; its layout is ratios +
orientation, not free rects. Standard VPORTS configs (2H, 3-left, 4-equal) are
tree-expressible, but arbitrary rects saved by other CAD apps may **not** map to
a clean tree → lossy load/save.

Mitigations:
- Keep `ModelTile`-equivalent rects as the **source of truth for I/O**; convert
  tree ↔ rects at the boundary. On load, best-effort fit saved rects to a tree;
  if they don't fit, fall back to the current flat-rect path for that file.
- Store the pane_grid `Configuration` (split ratios/orientation) and reconstruct
  on load; write each leaf's computed rect to its VPort entry on save.

### 3. Overlays
Grid, snap markers, crosshair, UCS icon, viewcube, dynamic-input boxes, and the
control chip are currently one canvas at the active tile. Under (A) each becomes
per-pane. Per-#121 the grid is already per-tile (`model_tile_grid_views`), which
is a good precedent — the rest follow the same shape.

## What `pane_grid` gives us
- Drag-resize dividers, drag-to-swap panes, maximize/restore — deletes
  `TileEdge`, `hit_model_tile_edge`, `move_model_tile_edge`, edge-drag handling,
  `collapse_small_model_tiles`, `set_active_model_tile_at` hit-testing.
- Natural per-pane mouse events (no manual point→tile mapping).
- `pane_grid::State<ModelTile>` carries per-pane state (camera / render_mode /
  grid_on / snap_on) directly.
- Focus model replaces `active_model_tile` (adapt hover-vs-click activation;
  OCS currently activates on hover — decide whether to keep that or move to
  click-to-focus, which is the `pane_grid` default).

## Phased plan

**Phase 0 — Spike (throwaway).** One `pane_grid` with a `shader` widget per
pane rendering the OCS scene with the pane's camera. Answer: does the scene draw
correctly per pane? Do N wgpu shader widgets share resources / perform with 4
panes? This decides go/no-go for shape (A).

**Phase 1 — Layout + rendering.** Introduce `pane_grid::State<ModelTile>`
alongside the existing `model_tiles` behind a flag. Move rendering to per-pane
shader + per-pane overlay. Keep the control chip per pane (top-left). Wire split
to `pane_grid` split, resize/swap/maximize to its built-ins. Remove `TileEdge`
and friends once parity is reached.

**Phase 2 — Activation + grid/snap.** Map focus → active pane. Reroute
`sync_vport_display` / `adopt_view_display` to the focused pane's state. Decide
hover-activation policy.

**Phase 3 — DWG I/O.** Tree ↔ rect conversion in `save_model_tiles_to_vports` /
`restore_model_tiles_from_vports`. Best-effort fit on load; lossless for
tree-expressible configs; document the lossy edge case.

**Phase 4 — Cleanup.** Delete the old flat-rect path and the feature flag once
the new path is at parity and round-trips real multi-viewport DWGs.

## Open questions
- Can iced `shader` widgets share a wgpu pipeline/buffers across panes, or does
  each allocate its own? (Phase 0 answers this — the whole migration hinges on
  it.)
- Keep hover-to-activate, or switch to click-to-focus (`pane_grid` default)?
- Acceptable to be lossy for non-tree VPORT rect configs from foreign CAD apps,
  or keep a flat-rect fallback path indefinitely?

## Recommendation
Worth a Phase-0 spike. If shader-per-pane is clean and fast, the UX win
(resize/swap/maximize) and the code deleted are real. If not, keep the current
rect model — it already maps directly to VPORTs and renders efficiently — and do
not adopt `pane_grid`. Either way, `pane_grid` does **not** unlock floating
viewports.
