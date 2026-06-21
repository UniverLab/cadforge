---
title: Quick Start
description: Scaffold a project, preview it live, and compile to DXF.
order: 3
---

# Quick Start

## 1. Create a project

```bash
cadspec new my-house
cd my-house
```

`cadspec new` scaffolds a complete multi-layer project (walls, doors,
furniture, dimensions) with meaningful architectural examples. To adopt
cadspec in an existing directory use `cadspec init`.

## 2. Preview it live

```bash
cadspec serve --open
```

A local server opens the plan in your browser. Edit any `.cf` file and
the view refreshes on save; build errors render as an overlay with
file/line detail, so the loop never breaks. See
[Live Preview](live-preview.md) for the full controls.

## 3. Edit geometry

Geometry lives in `.cf` files — TOML with one table per entity:

```toml
[layer]
name = "muros"
color = "#FFFFFF"

[[line]]
id = "ln-001"
from = [0.0, 0.0]
to = [8.5, 0.0]
weight = 0.50
```

Run `cadspec schema` for the complete language reference, or read
[The .cf Format](cf-format.md).

## 4. Validate and format

```bash
cadspec check     # validate geometry and constraints, no output files
cadspec fmt       # normalize .cf files
```

## 5. Compile to DXF

```bash
cadspec build                       # default output.dxf
cadspec build --output plano.dxf    # custom output path
cadspec build --layer muros         # single layer
```

The output is deterministic and AutoCAD-compatible. `cadspec watch`
rebuilds automatically while you edit.
