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
polylines, HATCH for solid fills, MTEXT for annotations — with full
layer/color/lineweight mapping.

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
```

Import migrates an existing DXF into `.cf` layer files plus a
`project.toml`, so legacy drawings can join the declarative workflow.

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
