---
title: Building & Export
description: Deterministic DXF compilation, watch mode, validation, and DXF import.
order: 6
---

# Building & Export

## Compile to DXF

```bash
cadforge build                       # default output.dxf
cadforge build --output plano.dxf    # custom output path
cadforge build --layer muros         # compile a single layer
cadforge build --check               # validate only, no DXF
```

The output is deterministic: identical input produces a bit-identical
DXF. Export uses proper AutoCAD-compatible entities — LWPOLYLINE for
polylines, HATCH for solid fills, MTEXT for annotations — with full
layer/color/lineweight mapping.

## Validation

```bash
cadforge check          # geometry + constraints, human-readable
cadforge check --json   # machine-readable report
cadforge layers         # list layers with entity counts and colors
cadforge layers --json  # machine-readable layer listing
```

## Watch mode

```bash
cadforge watch
```

Monitors `.cf` and `.toml` files and rebuilds the DXF on changes with a
300 ms debounce.

## Formatting

```bash
cadforge fmt           # normalize .cf files in place
cadforge fmt --check   # CI-friendly check mode
```

## Importing existing drawings

```bash
cadforge import plano.dxf                 # all layers
cadforge import plano.dxf --layer muros   # a single DXF layer
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
