# Credits and acknowledgements

OpenCADElectrical (working name) exists because of the work of the Open CAD Studio project
and the open-source Rust ecosystem around it. This page records that debt honestly.

## Open CAD Studio

**Open CAD Studio is created and maintained by HakanSeven12 and the Open CAD Studio
contributors.** Thank you for building an open, cross-platform CAD application in Rust and
for releasing it under the GNU GPL: DWG/DXF reading and writing, the drawing kernel, blocks
and attributes, layers, printing, plugins and the MCP server are all their work, and the
electrical layer in this repository is built on top of it.

- Upstream repository: <https://github.com/HakanSeven12/OpenCADStudio>
- Upstream web application: <https://www.opencadstudio.com>
- Upstream discussions: <https://github.com/HakanSeven12/OpenCADStudio/discussions>
- Upstream releases: <https://github.com/HakanSeven12/OpenCADStudio/releases>

This repository is an independent fork. It is **not** an official Open CAD Studio release,
and the Open CAD Studio maintainers are not responsible for the electrical additions, for
this fork's bugs, or for any data loss it may cause.

## The electrical layer

The electrical work in this repository (project drawing sets, IEC/NEC templates, symbol
libraries, pin-level connectivity, numbering, BOM and reports, cables, PLC I/O, panel
layout, systems/variants/options, and the `ELEC*` command set) was written for this fork.
It is inspired by the workflow of AutoCAD Electrical, but it is an independent
implementation; no Autodesk code, symbols or data are included. AutoCAD and AutoCAD
Electrical are trademarks of Autodesk, Inc.

The compatibility conventions used here (the `WD_M` drawing contract, `X*TERM*` pin
attributes, tag and wire formats) were derived from studying the format of drawings the
owners of this repository already had, plus public documentation of those conventions.

## Rust and third-party libraries

This project would not build without the Rust community. In particular:

- [`iced`](https://github.com/iced-rs/iced) for the user interface
- [`wgpu`](https://github.com/gfx-rs/wgpu) for rendering
- [`acadrust`](https://github.com/hakanaktt/acadrust) for the DWG/DXF codec work inherited from upstream
- the many crates listed in `Cargo.lock`

Each crate keeps its own copyright and license; see `Cargo.lock` and the crates' own
repositories for details.

## Documentation conventions

Some files under `reports/` are historical engineering records from the build-out of this
fork. They keep the names that were in use at the time (`AryaCADStudio`,
`AryaCADElectrical`) so the record stays accurate. The product, the documentation and new
commits call it **OpenCADElectrical** (working name) until the repository rename is
permitted.

## Contributing

If you are looking for the upstream project, use the links above. If you are contributing to
the electrical layer, note that this fork tracks a pinned Open CAD Studio revision; changes
to shared CAD code should stay minimal and explainable so they can be carried forward when
upstream moves.
