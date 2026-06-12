```text
                        █████ ███████████
                       ░░███ ░░███░░░░░░█
  ██████   ██████    ███████  ░███   █ ░   ██████  ████████   ███████  ██████
 ███░░███ ░░░░░███  ███░░███  ░███████    ███░░███░░███░░███ ███░░███ ███░░███
░███ ░░░   ███████ ░███ ░███  ░███░░░█   ░███ ░███ ░███ ░░░ ░███ ░███░███████
░███  ███ ███░░███ ░███ ░███  ░███  ░    ░███ ░███ ░███     ░███ ░███░███░░░
░░██████ ░░████████░░████████ █████      ░░██████  █████    ░░███████░░██████
 ░░░░░░   ░░░░░░░░  ░░░░░░░░ ░░░░░        ░░░░░░  ░░░░░      ░░░░░███ ░░░░░░
                                                             ███ ░███
                                                            ░░██████
                                                             ░░░░░░
```

<p align="center">
  <a href="https://github.com/UniverLab/cadforge/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/UniverLab/cadforge/ci.yml?branch=main&style=for-the-badge&label=CI" alt="CI"/></a>
  <a href="https://crates.io/crates/cadforge"><img src="https://img.shields.io/crates/v/cadforge?style=for-the-badge&logo=rust&logoColor=white" alt="Crates.io"/></a>
  <img src="https://img.shields.io/badge/Status-Active-27AE60?style=for-the-badge" alt="Status"/>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-2E8B57?style=for-the-badge" alt="License"/></a>
</p>

cadforge is an **Architecture as Code** CLI tool and Rust library for declarative 2D CAD modeling. Write geometry as code in `.cf` TOML format, watch it live in the browser, and compile to DXF — built for humans and AI agents working together.

---

## Vibecoding CAD

The core loop: **describe geometry in TOML, see it instantly, iterate.**

```bash
cadforge new casa && cd casa
cadforge serve --open        # live preview in the browser
```

Now edit any `.cf` file — by hand or by asking an AI agent — and the browser
updates on every save. Parse errors and constraint violations appear as an
overlay instead of a crash. When the design is right, `cadforge build` emits
a deterministic, AutoCAD-compatible DXF.

Agents get first-class support:

- `cadforge schema` — full `.cf` language reference in one command; agents
  self-discover the format without prior training.
- `cadforge check --json` / `cadforge layers --json` — machine-readable
  validation reports.
- `cadforge preview` — a faithful PNG render (real text, measured dimension
  labels, hatches, line styles) + `preview.meta.json` with per-entity bounding
  boxes in world and pixel coordinates, so multimodal agents can *look* at the
  plan and locate every entity in the image.
- `cadforge preview --highlight ln-001,tx-002` — labeled amber markers around
  specific entities, so an agent can visually confirm its edit landed where
  intended.
- `cadforge preview --format svg` — same render as vector SVG.

---

## Features

### 🎯 Core Platform

- **📐 Declarative Geometry** — Define architectural elements (lines, rects, circles, arcs, polylines, text, dimensions) in TOML `.cf` files. Deterministic, reproducible, version-controlled.
- **🛠️ Construction Tools** — `[[array]]` (linear and polar: spiral staircases, gear teeth, repeated columns) and `[[mirror]]` expand into concrete primitives at build time; copies get derived ids (`base@1`, `base@m`).
- **📏 Styled Dimensions** — Auto-measured labels with configurable `text_size`, `precision`, `show_units`, and `offset` per dimension.
- **🔴 Live Preview** — `cadforge serve` runs a local server with pan/zoom, auto-reload on save (SSE), click-to-inspect any entity (copy its source TOML as an agent prompt), per-layer ghost/hide states, a 3D stacked-layers view, and a build-error overlay. Zero config.
- **🔗 Layer System** — Organize geometry by layer with custom names, colors, and line weights. Compile single layers or full projects.
- **📄 DXF Export** — Compile `.cf` → DXF (AutoCAD-compatible). Full layer support, LWPOLYLINE for polylines, HATCH for solid fills, MTEXT for annotations.
- **🖼️ Previews for Agents** — Raster PNG + metadata JSON (entity bounding boxes) and full-fidelity SVG with real text, auto-measured dimensions, line styles, and clipped hatch patterns.
- **✅ Validation Engine** — `cadforge check` validates geometry and constraints without generating output; `--json` for tooling.

### 🏗️ Project Management

- **Project Scaffolding** — `cadforge new` creates a complete multi-layer project (muros, puertas, mobiliario, cotas) with meaningful architectural examples.
- **Multi-Layer Compilation** — Compile all layers or target specific layers with `--layer`. Custom output path with `--output`.
- **Auto-Rebuild** — `cadforge watch` monitors `.cf` and `.toml` files and auto-rebuilds DXF on changes with 300ms debounce.
- **Code Formatting** — `cadforge fmt` normalizes `.cf` files. `--check` mode for CI validation.
- **Constraints** — `parent`, `belongs_to`, and `spatial_dependency` rules between layers; warnings by default, build-blocking with `strict = true`.
- **DXF Import** — `cadforge import plano.dxf` migrates existing drawings into `.cf` layers + `project.toml`.

### 🔧 Architecture

- **Compiler Pipeline** — Parse → Resolve → Compile → Emit. Modular design for easy extension.
- **DXF Writer** — Direct DXF entity writing with proper AutoCAD compatibility. Layer/color/lineweight mapping.
- **Renderer** — One handwritten SVG backend; the PNG preview rasterizes it via resvg with an embedded monospace font (deterministic text on any machine, including fontless containers).
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
| `cadforge serve` | Live preview server — browser auto-reloads on save |
| `cadforge serve --open --port 4377` | Open browser automatically on a custom port |
| `cadforge check` | Validate with project metadata and layer colors |
| `cadforge check --json` | Machine-readable validation report |
| `cadforge layers` | List layers with entity counts and colors |
| `cadforge layers --json` | Machine-readable layer listing |
| `cadforge schema` | Print the full `.cf` language reference (markdown) |
| `cadforge preview` | Faithful PNG render + metadata JSON |
| `cadforge preview --format svg` | Vector SVG preview (same renderer) |
| `cadforge preview --highlight <id1,id2>` | Amber markers around specific entities |
| `cadforge preview --width 1024 -H 768` | Custom resolution preview |
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

### Live preview controls (`cadforge serve`)

- Scroll → zoom (centered on cursor) · drag → pan · double-click / `F` → fit
- **Click an entity** → inspector with its source TOML block; `copy for agent` produces a ready-made targeted-edit prompt
- Layer panel (or keys `1`-`9`) → cycle each layer **on → ghost → off**; ghost mode traces one floor plan over another
- `3D` button (or key `3`) → stacked exploded view of the layers
- Browser auto-reloads on every `.cf` / `project.toml` save (SSE)
- Build errors render as an overlay with file/line detail — the loop never breaks

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
points = [[0.0, 0.0], [5.0, 0.0], [5.0, 3.0], [0.0, 3.0]]
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

`line`, `polyline`, `rect`, `circle`, `arc`, `text`, `point`, `dim`, `hatch`, `fill`, `group`

Run `cadforge schema` for the complete reference with all attributes.

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
- **SVG Renderer** — Single vector backend: text, measured dims, hatches, highlights; PNG previews are resvg rasterizations of it

---

## Main Modules

- `compiler/` — Project compilation pipeline, layer targeting, validation, JSON reports
- `dxf_writer/` — DXF entity writing, LWPOLYLINE, HATCH, MTEXT generation
- `preview/` — PNG rendering with configurable resolution, layer filtering, metadata JSON
- `svg/` — Vector SVG rendering: real text, measured dimensions, hatch clipping, grid
- `serve/` — Live preview server: file watcher + SSE auto-reload + error overlay
- `schema/` — Embedded `.cf` language reference for humans and agents
- `parser/` — TOML parsing, primitive extraction, array-of-tables handling
- `model/` — Data structures: Layer, Primitive, Project
- `scaffold/` — Multi-layer project creation with architectural examples
- `fmt/` — .cf file formatting and normalization
- `watch/` — File system watcher with auto-rebuild and debounce
- `importer/` — DXF → `.cf` migration
- `color/` — Color parsing and DXF color mapping

---

## Data Storage

| Data | Location | Format |
|------|----------|--------|
| Project files | `./` | TOML (`.cf` + `project.toml`) |
| Build output | `./output.dxf` | DXF |
| Preview output | `./preview.png`, `./preview.svg` | PNG / SVG |
| Preview metadata | `./preview.meta.json` | JSON |
| Language reference | `cadforge schema` (stdout) | Markdown |

---

## Usage

**Create a new project:**
```bash
cadforge new mi-proyecto
cd mi-proyecto
```

**Live preview while you edit:**
```bash
cadforge serve --open    # browser refreshes on every save
```

**Edit `.cf` files** (TOML format with your geometry — run `cadforge schema` for the reference)

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
cadforge preview                          # default 1600x1200 (fits content aspect)
cadforge preview --width 1024 --height 768  # custom resolution
cadforge preview --layer muros            # single layer preview
```

**Auto-rebuild on changes:**
```bash
cadforge watch       # monitors .cf and .toml files
```

---

## Tech Stack

| Rust 2021 | clap | toml | toml_edit | resvg | dxf | notify | anyhow | serde |

---

## License

MIT — see [LICENSE](LICENSE) for details.

---

Made with ❤️ by [JheisonMB](https://github.com/JheisonMB) and [UniverLab](https://github.com/UniverLab)