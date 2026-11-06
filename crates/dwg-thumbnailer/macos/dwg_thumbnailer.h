/* Bridging header for the macOS QuickLook extension.
 * Exposes the Rust core's C ABI (built as `libdwg_thumbnailer.a`). */
#ifndef DWG_THUMBNAILER_H
#define DWG_THUMBNAILER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Extract a DWG preview, encoded as PNG. On success writes a malloc'd buffer to
 * *out_ptr / *out_len (free with dwg_thumbnail_free) and returns true. */
bool dwg_thumbnail_png(const char *path_utf8, uint32_t max_dim,
                       uint8_t **out_ptr, size_t *out_len);

/* Free a buffer returned by dwg_thumbnail_png. */
void dwg_thumbnail_free(uint8_t *ptr, size_t len);

#endif /* DWG_THUMBNAILER_H */
