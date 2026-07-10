---
title: Building & Export
description: Deterministic DXF compilation, watch mode, validation, and DXF import.
order: 6
---

# Building & Export

## Compile to DXF

```bash
cadspec build                       # default output.dxf
cadspec build --output plano.dxf    # custom output path
cadspec build --layer muros         # compile a single layer
cadspec build --check               # validate only, no DXF
```

The output is deterministic: identical input produces a bit-identical
DXF. Export uses proper AutoCAD-compatible entities — LWPOLYLINE for
polylines and rectangles, LINE/CIRCLE/ARC for open geometry, TEXT for
annotations, DIMENSION for measured dims, and SOLID (fan-triangulated)
for filled regions. Hatch patterns are emitted as clipped LINE segments
(the `dxf` crate has no native HATCH entity). All entities carry full
layer/color/lineweight mapping.

## Drawing sheets (planos)

A *plano* is a named view of the model rendered onto a paper-sized sheet
with a frame and a title block (rótulo). Declare sheets in `project.toml`
and render them with `cadspec preview --plano <name>`:

```toml
[[plano]]
name = "general"
view = "plan"            # plan | iso | front | back | left | right | top | section
scale = "1:100"          # label shown in the title block
size = [420.0, 297.0]    # paper size in mm (default A3 landscape)
title = "Planta general" # defaults to the plano name

[[plano]]
name = "corte-a"
view = "section"
cut_axis = "x"           # section only: x | y | z
cut_at = 4.0             # position of the cut plane along cut_axis
keep = "min"             # which side to keep: min (default) | max
```

```bash
cadspec preview --plano general              # render one sheet
cadspec preview --plano corte-a --format svg
```

The title block is generated from the project metadata; set `rotulo =
"titleblock.cf"` on a plano to draw a custom one with the same `.cf`
primitives as the rest of the drawing. The live preview (`cadspec
serve`) lists all declared planos in a panel.

## Validation

```bash
cadspec check          # geometry + constraints, human-readable
cadspec check --json   # machine-readable report
cadspec layers         # list layers with entity counts and colors
cadspec layers --json  # machine-readable layer listing
```

## Watch mode

```bash
cadspec watch
```

Monitors `.cf` and `.toml` files and rebuilds the DXF on changes with a
300 ms debounce.

## Formatting

```bash
cadspec fmt           # normalize .cf files in place
cadspec fmt --check   # CI-friendly check mode
```

## Importing existing drawings

```bash
cadspec import plano.dxf                 # all layers
cadspec import plano.dxf --layer muros   # a single DXF layer
cadspec import plano.dxf --output casa   # into a specific directory
```

Import migrates an existing DXF into `.cf` layer files plus a
`project.toml`, so legacy drawings can join the declarative workflow.
Lines, polylines, circles, arcs, text, points and dimensions map back to
their `.cf` primitives. Filled and hatched regions are reconstructed too:
adjacent SOLID entities on a layer (such as the fan-triangulated fills that
`cadspec build` emits) are fused back into a single `[[fill]]`, and the
clipped LINE segments a hatch expands into — marked with `CADSPEC_HATCH`
XDATA on export — are re-fused into a single `[[hatch]]` carrying its region
and pattern/angle/scale. So an export/import round-trip does not inflate the
entity count, and only lines this tool emitted are ever folded back into a
hatch (a foreign DXF's ordinary lines are left untouched).

## Pipeline

```
.cf file (TOML)
    │
    ▼
 Parser ──▶ Resolver ──▶ Compiler ──▶ DXF Emit
                │
                ▼
          Boundary Resolver ──▶ Preview PNG/SVG + JSON meta
```
