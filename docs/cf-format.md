---
title: The .cf Format
description: Layers, primitives, styled dimensions, construction tools and constraints.
order: 4
---

# The .cf Format

A cadspec project is a `project.toml` plus one `.cf` file per layer.
`.cf` files are TOML: a `[layer]` header followed by arrays of entity
tables.

> The authoritative reference is always `cadspec schema` — it prints the
> complete language specification (markdown) for the exact version you
> have installed.

## Layers

```toml
[layer]
name = "muros"
color = "#FFFFFF"
```

Layers organize geometry with custom names, colors and line weights. You
can compile or preview single layers (`--layer muros`) or the whole
project.

## Primitives

Supported primitives: `line`, `polyline`, `rect`, `circle`, `arc`,
`text`, `point`, `dim`, `hatch`, `fill`, `group`.

```toml
[[line]]
id = "ln-001"
from = [0.0, 0.0]
to = [8.5, 0.0]
weight = 0.50

[[rect]]
id = "rc-001"
origin = [1.0, 1.0]
width = 3.5
height = 4.0

[[circle]]
id = "ci-001"
center = [4.0, 3.0]
radius = 0.5

[[arc]]
id = "ac-001"
center = [2.0, 2.0]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

[[polyline]]
id = "pl-001"
points = [[0.0, 0.0], [5.0, 0.0], [5.0, 3.0], [0.0, 3.0]]
closed = true

[[text]]
id = "tx-001"
position = [4.0, 3.0]
content = "SALA"
size = 0.2
```

Every entity has a stable `id` — that is what makes plans diffable and
lets agents target precise edits.

## Filled and hatched regions

`fill` (solid color) and `hatch` (parallel pattern lines) mark an area. The
region can come from either a reference to a closed `polyline`/`rect` `id` in
the **same layer file** (`boundary`) or inline `points`:

```toml
[[fill]]
id = "fl-closet"
points = [[0.0, 7.0], [1.2, 7.0], [1.2, 9.0], [0.0, 9.0]]
color = "#E0E0E0"

[[hatch]]
id = "ht-bano"
boundary = "pl-bano"        # id of a closed polyline/rect in this file
pattern = "ansi31"
scale = 2.0
angle = 45.0
```

Boundaries resolve per layer file: a `boundary` id defined in another layer
will not resolve (the build warns and skips the region); use inline `points`
to cross that boundary.

At build time a `hatch` expands into DXF `LINE`s (there is no native HATCH
entity in the DXF format this tool targets). Those pattern lines carry a
`CADSPEC_HATCH` marker so `cadspec import` re-fuses them back into a single
`[[hatch]]` — with inline `points` — instead of dozens of stray `[[line]]`s.
Only lines this tool emits carry the marker, so importing a foreign DXF never
mistakes ordinary lines for a hatch.

## Styled dimensions

`dim` entities are auto-measured — the label is computed from the
geometry, never typed by hand:

```toml
[[dim]]
id = "dm-001"
from = [0, 0]
to = [5, 0]
offset = 0.5
```

Configurable per dimension: `text_size`, `precision`, `show_units`,
`offset`.

## Construction tools

`[[array]]` (linear and polar — spiral staircases, gear teeth, repeated
columns) and `[[mirror]]` expand into concrete primitives at build time.
Copies get derived ids: `base@1`, `base@2`, … and `base@m` for mirrors.

## Constraints

Layers can declare relationships: `parent`, `belongs_to` and
`spatial_dependency`. Violations are warnings by default and
build-blocking with `strict = true` in `project.toml`.
