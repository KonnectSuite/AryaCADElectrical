# DWG thumbnails for OS file managers

`dwg_thumbnailer::extract` reads **only** the embedded preview from a DWG (file
header → preview seeker → parse — never the whole drawing) and returns it as an
`image::RgbaImage`. It's the shared core behind every platform's thumbnail
integration.

```
crates/
  dwg-thumbnailer/        this crate — shared core + macOS extension sources
  dwg-thumbnailer-win/    Windows IThumbnailProvider COM DLL (depends on the core)
```

---

## Linux (COSMIC, GNOME, Nautilus, Nemo, …) — handled by OpenCADStudio itself

No separate binary. OpenCADStudio embeds this core and, on startup, installs a
freedesktop `.thumbnailer` pointing at its own hidden `--dwg-thumbnail` mode
(see `src/io/file_association.rs::install_thumbnailer`). Launch OCS once and file
managers render DWG thumbnails; clear stale "no thumbnail" cache if needed:

```sh
rm -f ~/.cache/thumbnails/fail/*/*.png
```

---

## Windows (Explorer) — authored, build & test on Windows

A COM in-proc server implementing `IThumbnailProvider`. **Not compiled/tested on
the Linux dev host** — build and verify on Windows.

```bat
cd crates\dwg-thumbnailer-win && cargo build --release
regsvr32 target\release\dwg_thumbnailer_win.dll        :: register (elevated)
regsvr32 /u target\release\dwg_thumbnailer_win.dll     :: unregister
ie4uinit.exe -show                                     :: refresh thumbnails
```

If `regsvr32`'s self-registration needs adjusting, the equivalent registry keys
are (replace the path):

```reg
Windows Registry Editor Version 5.00
[HKEY_CLASSES_ROOT\CLSID\{8F2A9C41-3B6E-4E2D-9C7A-1E0B5D6F42AA}]
@="OpenCADStudio DWG Thumbnail Provider"
[HKEY_CLASSES_ROOT\CLSID\{8F2A9C41-3B6E-4E2D-9C7A-1E0B5D6F42AA}\InprocServer32]
@="C:\\path\\to\\dwg_thumbnailer_win.dll"
"ThreadingModel"="Apartment"
[HKEY_CLASSES_ROOT\.dwg\ShellEx\{e357fccd-a995-4576-b01f-234630154e96}]
@="{8F2A9C41-3B6E-4E2D-9C7A-1E0B5D6F42AA}"
```

---

## macOS (Finder) — sources provided, build in Xcode

A QuickLook **Thumbnail Extension** (`macos/ThumbnailProvider.swift`) calls the
Rust core's C ABI, linked as a static library. Requires Xcode + a host app.

1. Build the core as a static lib for your arch(s):
   ```sh
   cargo build -p dwg-thumbnailer --release --target aarch64-apple-darwin
   # → target/aarch64-apple-darwin/release/libdwg_thumbnailer.a
   ```
2. In Xcode, add a **Thumbnail Extension** target to a host app.
   - Use `macos/Info.plist` (lists the `com.autodesk.dwg` UTI).
   - Add `macos/ThumbnailProvider.swift`.
   - Add a bridging header that `#include`s `macos/dwg_thumbnailer.h`.
   - Link `libdwg_thumbnailer.a` (+ system frameworks it needs).
3. Sign, install the host app, and Finder picks up the extension.

C ABI (see `dwg_thumbnailer.h`):
`dwg_thumbnail_png(path, max_dim, &ptr, &len)` / `dwg_thumbnail_free(ptr, len)`.

---

Formats: DWG BMP-DIB and PNG embedded previews decode. WMF previews and files
without a preview (or DXF) produce no thumbnail — the file manager falls back.
