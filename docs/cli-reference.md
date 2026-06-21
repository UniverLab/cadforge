---
title: CLI Reference
description: Every cadspec command and flag.
order: 8
---

# CLI Reference

```
cadspec <command> [options]
```

## Project lifecycle

| Command | Description |
|---------|-------------|
| `cadspec new <name>` | Create a new project with multi-layer scaffold |
| `cadspec init` | Initialize cadspec in the current directory |
| `cadspec build` | Compile project to DXF |
| `cadspec build --check` | Validate project and constraints without generating DXF |
| `cadspec build --output <path>` | Compile to custom output path |
| `cadspec build --layer <name>` | Compile a specific layer only |
| `cadspec watch` | Auto-rebuild on file changes (300 ms debounce) |

## Preview

| Command | Description |
|---------|-------------|
| `cadspec serve` | Live preview server — browser auto-reloads on save |
| `cadspec serve --open --port <p>` | Open browser automatically on a custom port |
| `cadspec preview` | Faithful PNG render + `preview.meta.json` |
| `cadspec preview --format svg` | Vector SVG preview (same renderer) |
| `cadspec preview --highlight <ids>` | Amber markers around specific entities |
| `cadspec preview --width <w> -H <h>` | Custom resolution |
| `cadspec preview --layer <name>` | Preview a specific layer only |
| `cadspec view` | Open the project in the configured viewer |
| `cadspec view --layer <name>` | Open only one layer in the viewer |

## Inspection & quality

| Command | Description |
|---------|-------------|
| `cadspec check` | Validate with project metadata and layer colors |
| `cadspec check --json` | Machine-readable validation report |
| `cadspec layers` | List layers with entity counts and colors |
| `cadspec layers --json` | Machine-readable layer listing |
| `cadspec schema` | Print the full `.cf` language reference (markdown) |
| `cadspec fmt` | Format `.cf` files (normalize whitespace) |
| `cadspec fmt --check` | Check formatting without modifying (CI) |

## Import & config

| Command | Description |
|---------|-------------|
| `cadspec import <file.dxf>` | Import DXF into `.cf` layers + `project.toml` |
| `cadspec import <file.dxf> --layer <name>` | Import only one DXF layer |
| `cadspec config set <key> <value>` | Set global defaults (`author`, `units`) |
| `cadspec config show` | Show global defaults |

## Output files

| File | Description |
|------|-------------|
| `output.dxf` | Default build output |
| `preview.png` / `preview.svg` | Preview renders |
| `preview.meta.json` | Per-entity bounding boxes (world + pixel coordinates) |
