# CADforge

Architecture as Code — deterministic geometry engine for reproducible architectural design.

## Quick Start

```bash
# Create a new project
cadforge new mi-proyecto
cd mi-proyecto

# Edit .cf files (TOML format)
# Then compile to DXF
cadforge build

# Validate without generating output
cadforge check

# List layers
cadforge layers
```

## Commands

| Command | Description |
|---|---|
| `cadforge new <name>` | Create a new project directory |
| `cadforge init` | Initialize in current directory |
| `cadforge build` | Compile .cf → DXF |
| `cadforge build --layer <name>` | Compile a single layer |
| `cadforge check` | Validate project without output |
| `cadforge layers` | List layers with entity count |

## .cf Format

Files use TOML with array-of-tables for primitives:

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
origin = [1.0, 1.0]
width = 3.5
height = 4.0

[[circle]]
center = [4.0, 3.0]
radius = 0.5

[[arc]]
center = [2.0, 2.0]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

[[text]]
position = [4.0, 3.0]
content = "SALA"
size = 0.2
```

## Supported Primitives

`line`, `polyline`, `rect`, `circle`, `arc`, `text`, `point`, `dim`

## License

MIT
