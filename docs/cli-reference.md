---
title: CLI Reference
description: Every cadforge command and flag.
order: 8
---

# CLI Reference

```
cadforge <command> [options]
```

## Project lifecycle

| Command | Description |
|---------|-------------|
| `cadforge new <name>` | Create a new project with multi-layer scaffold |
| `cadforge init` | Initialize cadforge in the current directory |
| `cadforge build` | Compile project to DXF |
| `cadforge build --check` | Validate project and constraints without generating DXF |
| `cadforge build --output <path>` | Compile to custom output path |
| `cadforge build --layer <name>` | Compile a specific layer only |
| `cadforge watch` | Auto-rebuild on file changes (300 ms debounce) |

## Preview

| Command | Description |
|---------|-------------|
| `cadforge serve` | Live preview server — browser auto-reloads on save |
| `cadforge serve --open --port <p>` | Open browser automatically on a custom port |
| `cadforge preview` | Faithful PNG render + `preview.meta.json` |
| `cadforge preview --format svg` | Vector SVG preview (same renderer) |
| `cadforge preview --highlight <ids>` | Amber markers around specific entities |
| `cadforge preview --width <w> -H <h>` | Custom resolution |
| `cadforge preview --layer <name>` | Preview a specific layer only |
| `cadforge view` | Open the project in the configured viewer |
| `cadforge view --layer <name>` | Open only one layer in the viewer |

## Inspection & quality

| Command | Description |
|---------|-------------|
| `cadforge check` | Validate with project metadata and layer colors |
| `cadforge check --json` | Machine-readable validation report |
| `cadforge layers` | List layers with entity counts and colors |
| `cadforge layers --json` | Machine-readable layer listing |
| `cadforge schema` | Print the full `.cf` language reference (markdown) |
| `cadforge fmt` | Format `.cf` files (normalize whitespace) |
| `cadforge fmt --check` | Check formatting without modifying (CI) |

## Import & config

| Command | Description |
|---------|-------------|
| `cadforge import <file.dxf>` | Import DXF into `.cf` layers + `project.toml` |
| `cadforge import <file.dxf> --layer <name>` | Import only one DXF layer |
| `cadforge config set <key> <value>` | Set global defaults (`author`, `units`) |
| `cadforge config show` | Show global defaults |

## Output files

| File | Description |
|------|-------------|
| `output.dxf` | Default build output |
| `preview.png` / `preview.svg` | Preview renders |
| `preview.meta.json` | Per-entity bounding boxes (world + pixel coordinates) |
