██████   ████████ ██████   ████████ ██      ██    ██ ███████ ████████
██░░███ ░░███░░███░░░░░███ ░░███░░███░███    ░███  ██░███░░░░░░░███░
░███ ░░░  ░███ ░███  ███████  ░███ ░░░ ░███    ░██████░░█████  ░███
░███  ███ ░███ ░███ ███░░███  ░███     ░███    ░███░░░ ░███░░█  ░███
░░██████ ░███████░░████████ ░███     ░███████████   ███████  ░████████
 ░░░░░░   ░░░░░░░  ░░░░░░░░  ░░░      ░░░░░░░░░░░    ░░░░░░   ░░░░░░░

<p align="center">
  <a href="https://github.com/UniverLab/cadforge/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/UniverLab/cadforge/ci.yml?branch=main&style=for-the-badge&label=CI" alt="CI"/></a>
  <a href="https://crates.io/crates/cadforge"><img src="https://img.shields.io/crates/v/cadforge?style=for-the-badge&logo=rust&logoColor=white" alt="Crates.io"/></a>
  <img src="https://img.shields.io/badge/Status-Active-27AE60?style=for-the-badge" alt="Status"/>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-2E8B57?style=for-the-badge" alt="License"/></a>
</p>

cadforge is an **Architecture as Code** CLI tool and Rust library for declarative 2D CAD modeling. Write geometry as code in `.cf` TOML format, compile to DXF, and generate PNG previews for AI agents.

---

## Features

### 🎯 Core Platform

- **📐 Declarative Geometry** — Define architectural elements (lines, rects, circles, arcs, polylines, text, dimensions) in TOML `.cf` files. Deterministic, reproducible, version-controlled.
- **🔗 Layer System** — Organize geometry by layer with custom names, colors, and line weights. Compile single layers or full projects.
- **📄 DXF Export** — Compile `.cf` → DXF (AutoCAD-compatible). Full layer support, LWPOLYLINE for polylines, HATCH for solid fills, MTEXT for annotations.
- **🖼️ PNG Preview** — Generate raster previews with metadata JSON for AI agent integration. Renders fills, hatches, strokes, and text with boundary resolution. Configurable resolution and layer filtering.
- **✅ Validation Engine** — `cadforge check` validates geometry without generating output. Shows project metadata, layer colors, and entity counts.

### 🏗️ Project Management

- **Project Scaffolding** — `cadforge new` creates a complete multi-layer project (muros, puertas, mobiliario, cotas) with meaningful architectural examples.
- **Multi-Layer Compilation** — Compile all layers or target specific layers with `--layer`. Custom output path with `--output`.
- **Auto-Rebuild** — `cadforge watch` monitors `.cf` and `.toml` files and auto-rebuilds on changes with 300ms debounce.
- **Code Formatting** — `cadforge fmt` normalizes `.cf` files. `--check` mode for CI validation.
- **Boundary Resolution** — Automatic detection of closed boundaries for hatch generation. Shared boundary resolution across overlapping entities.
- **Polyline Support** — Full LWPOLYLINE support with bulge factors for arcs. Proper vertex handling and closure detection.

### 🔧 Architecture

- **Compiler Pipeline** — Parse → Resolve → Compile → Emit. Modular design for easy extension.
- **DXF Writer** — Direct DXF entity writing with proper AutoCAD compatibility. Layer/color/lineweight mapping.
- **Preview Renderer** — Tiny-skia based raster rendering with anti-aliasing. PNG + JSON metadata output.
- **Error Reporting** — Structured errors with file, line, and context. Fast-fail on validation errors.

---

## Commands

| Command | Description |
|---------|-------------|
| `cadforge new <name>` | Create a new project with multi-layer scaffold |
| `cadforge init` | Initialize CADforge in current directory |
| `cadforge build` | Compile project to DXF |
| `cadforge build --check` | Validate project and constraints without generating DXF |
| `cadforge build --output <path>` | Compile to custom output path |
| `cadforge build --layer <name>` | Compile specific layer only |
| `cadforge check` | Validate with project metadata and layer colors |
| `cadforge layers` | List layers with entity counts and colors |
| `cadforge preview` | Generate PNG preview + metadata JSON |
| `cadforge preview --width 1024 --height 768` | Custom resolution preview |
| `cadforge preview --layer <name>` | Preview specific layer only |
| `cadforge fmt` | Format .cf files (normalize whitespace) |
| `cadforge fmt --check` | Check formatting without modifying (CI) |
| `cadforge watch` | Auto-rebuild on file changes |
| `cadforge import <file.dxf>` | Import DXF into `.cf` layers + `project.toml` |
| `cadforge import <file.dxf> --layer <name>` | Import only one DXF layer |
| `cadforge view` | Open the project in the configured viewer |
| `cadforge view --layer <name>` | Open only one layer in the viewer |
| `cadforge config set <key> <value>` | Set global defaults (`author`, `units`) |
| `cadforge config show` | Show global defaults |

### Viewer controls (MVP)

- HUD flotante en pantalla con proyecto, vista, distancia, capas, selección y ayuda de atajos
- `T` / `F` / `V` / `R` → top / front / right / isometric preset views
- `Q` / `E` / `W` / `S` → orbit camera
- Mouse left-drag → orbit
- Mouse right-drag / arrows → pan
- Mouse wheel / `+` / `-` → zoom
- `1`..`9` → toggle layer visibility
- Click entity edge → select primitive id
- Selected entity is highlighted in amber in the viewport HUD context
- `C` → copy selected id to clipboard

---

## .cf Format

```toml
[layer]
name = "muros"
color = "#FFFFFF"

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
vertices = [[0, 0], [5, 0], [5, 3], [0, 3]]
closed = true

[[text]]
id = "tx-001"
position = [4.0, 3.0]
content = "SALA"
size = 0.2

[[dim]]
id = "dm-001"
from = [0, 0]
to = [5, 0]
offset = 0.5
```

### Supported Primitives

`line`, `polyline`, `rect`, `circle`, `arc`, `text`, `point`, `dim`, `hatch`, `solid`

---

## Architecture Overview

```
.cf file (TOML)
    │
    ▼
┌─────────┐    ┌──────────┐    ┌─────────┐    ┌─────────┐
│ Parser  │───▶│ Resolver  │───▶│Compiler │───▶│ DXF Emit│
└─────────┘    └──────────┘    └─────────┘    └─────────┘
                     │
                     ▼
              ┌──────────┐    ┌─────────────┐
              │ Boundary │───▶│ Preview PNG │
              │ Resolver │    │ + JSON meta │
              └──────────┘    └─────────────┘
```

- **Parser** — TOML parsing with custom array-of-tables detection, primitive validation
- **Resolver** — Layer dependency resolution, coordinate validation, boundary detection
- **Compiler** — Entity compilation to DXF format, hatch generation, polyline closure
- **DXF Writer** — Direct DXF entity emission with proper layer/color/lineweight mapping
- **Preview Renderer** — Tiny-skia raster rendering with hatch/fill support

---

## Main Modules

- `compiler/` — Project compilation pipeline, layer targeting, validation, build stats
- `dxf_writer/` — DXF entity writing, LWPOLYLINE, HATCH, MTEXT generation
- `preview/` — PNG rendering with configurable resolution, layer filtering, metadata JSON
- `parser/` — TOML parsing, primitive extraction, array-of-tables handling
- `model/` — Data structures: Layer, Primitive, Project
- `scaffold/` — Multi-layer project creation with architectural examples
- `fmt/` — .cf file formatting and normalization
- `watch/` — File system watcher with auto-rebuild and debounce
- `color/` — Color parsing and DXF color mapping

---

## Data Storage

| Data | Location | Format |
|------|----------|--------|
| Project files | `./` | TOML (`.cf` + `project.toml`) |
| Build output | `output/` | DXF |
| Preview output | `output/preview.png` | PNG |
| Preview metadata | `output/preview.json` | JSON |
| Build cache | `target/` | Cargo build |

---

## Usage

**Create a new project:**
```bash
cadforge new mi-proyecto
cd mi-proyecto
```

**Edit `.cf` files** (TOML format with your geometry)

**Format and validate:**
```bash
cadforge fmt           # normalize .cf files
cadforge check         # validate without generating DXF
```

**Compile to DXF:**
```bash
cadforge build                        # default output.dxf
cadforge build --output plano.dxf     # custom output path
cadforge build --layer muros          # compile single layer
```

**Preview:**
```bash
cadforge preview                          # default 2048x1536
cadforge preview --width 1024 --height 768  # custom resolution
cadforge preview --layer muros            # single layer preview
```

**Auto-rebuild on changes:**
```bash
cadforge watch       # monitors .cf and .toml files
```

---

## Tech Stack

| Rust 2021 | clap | toml | toml_edit | tiny-skia | dxf | notify | anyhow | serde |

---

## License

MIT — see [LICENSE](LICENSE) for details.

---

Made with ❤️ by [JheisonMB](https://github.com/JheisonMB) and [UniverLab](https://github.com/UniverLab)