---
title: Live Preview
description: The cadforge serve loop — pan/zoom, inspector, layer states, 3D view, error overlay.
order: 5
---

# Live Preview

```bash
cadforge serve                       # start the local preview server
cadforge serve --open --port 4377    # open browser, custom port
```

`cadforge serve` runs a zero-config local server with auto-reload on
save (SSE). Edit a `.cf` file and the browser updates instantly.

## Controls

| Action | Effect |
|---|---|
| Scroll | Zoom (centered on cursor) |
| Drag | Pan |
| Double-click / `F` | Fit to content |
| Click an entity | Inspector with its source TOML block |
| Layer panel / keys `1`–`9` | Cycle each layer **on → ghost → off** |
| `3D` button / key `3` | Stacked exploded view of the layers |

## Inspector and "copy for agent"

Clicking any entity opens an inspector showing its source TOML. The
**copy for agent** button produces a ready-made targeted-edit prompt you
can paste into an AI agent — the entity id, its current definition, and
the file it lives in.

## Ghost mode

Cycling a layer to *ghost* renders it as a faint trace — useful for
drawing one floor plan over another, or checking alignment between
structural and furniture layers.

## Error overlay

Build errors (parse errors, constraint violations) render as an overlay
with file and line detail instead of crashing the server. Fix the file,
save, and the view recovers — the loop never breaks.
