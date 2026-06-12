//! Schema — the `.cf` language reference, printable via `cadforge schema`.
//!
//! This is the self-discovery entry point for AI agents: one command dumps the
//! complete format so any agent can generate valid `.cf` files without prior
//! training. The same text is embedded into AGENTS.md by `cadforge new`.

/// Complete `.cf` + `project.toml` reference in markdown.
pub const CF_REFERENCE: &str = r##"# CADforge `.cf` Language Reference

CADforge projects are plain TOML. A project is a directory with a `project.toml`
plus one `.cf` file per layer. Geometry is declared, never drawn: the same files
always compile to the same DXF.

## project.toml

```toml
[project]
name = "Vivienda Lote 12"
scale = "1:100"        # drawing scale (metadata)
units = "m"            # unit label used in dimension labels
strict = false         # true: constraint violations fail the build

[layers]               # order defines draw order (later = on top)
muros = { file = "muros.cf", locked = false }
puertas = { file = "puertas.cf", locked = false }

[constraints]          # optional, validated on build/check
puertas.parent = "muros"        # child bbox must fit inside parent bbox
cotas.belongs_to = "muros"      # child primitives reference parent ids via belongs_to
"muros → puertas" = "spatial_dependency"  # movement warning (informational)
```

## Layer files (`.cf`)

Each `.cf` may start with optional layer metadata:

```toml
[layer]
name = "muros"
color = "#FFFFFF"      # default color for the layer
line_weight = 0.35     # default stroke weight in mm
visible = true
locked = false
```

### Common attributes (valid on every primitive, all optional)

```toml
id = "ln-001"          # unique within the layer; needed for hatch boundaries / belongs_to
color = "#FF5050"      # overrides layer color
weight = 0.50          # line weight in mm (0.13 thin … 0.70 thick)
style = "solid"        # solid | dashed | dotted | dashdot
visible = true
locked = false
belongs_to = "id"      # reference to a primitive id in the parent layer
```

### Primitives

```toml
[[line]]               # straight segment
from = [0.0, 0.0]
to = [8.5, 0.0]

[[polyline]]           # multi-vertex path; closed = true makes it a polygon
points = [[0.0, 0.0], [8.5, 0.0], [8.5, 6.0], [0.0, 6.0]]
closed = true

[[rect]]               # axis-aligned rectangle from bottom-left origin
origin = [1.0, 1.0]
width = 3.5
height = 4.0

[[circle]]
center = [4.0, 3.0]
radius = 0.5

[[arc]]                # angles in degrees, counterclockwise from +X
center = [2.0, 2.0]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

[[text]]
position = [4.0, 3.0]
content = "SALA"
size = 0.25            # text height in world units
align = "center"       # left | center | right

[[point]]              # reference marker (drawn as a cross)
position = [3.0, 3.0]

[[dim]]                # dimension; the measured distance is labeled automatically
type = "linear"        # linear | angular | radial
from = [0.0, 0.0]
to = [8.5, 0.0]
offset = -0.8          # distance from the measured element (sign = side)
text_size = 0.25       # label height in world units (optional)
precision = 2          # decimals in the measured value (default 2)
show_units = true      # append project units to the label (default true)

[[hatch]]              # pattern fill inside a closed boundary
boundary = "pl-001"    # id of a closed polyline or rect in the same file
pattern = "ansi31"     # ansi31 | ansi32 | ansi33 | ansi34 | solid | none
scale = 1.0
angle = 45.0

[[fill]]               # solid fill; boundary id or inline points
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
color = "#808080"

[[group]]              # logical grouping of primitives by id
members = ["ln-001", "rc-001"]
```

### Construction tools

Arrays and mirrors expand into concrete primitives at build time. Copies get
derived ids — `pl-001@1`, `pl-001@2`, … (array) and `pl-001@m` (mirror) — so
they can be highlighted or referenced. Edit the base entity or the
`[[array]]`/`[[mirror]]` block to change all copies at once.

```toml
[[array]]              # repeat targets in a line or around a center
target = "pl-huella"   # or targets = ["id-a", "id-b"]
mode = "polar"         # polar: spiral stairs, gear teeth, radial columns
count = 16             # total instances, including the original
center = [0.0, 0.0]    # polar: rotation center
step_angle = 22.5      # polar: degrees per copy, counterclockwise
rotate_items = true    # polar: false = orbit only, keep orientation

[[array]]
target = "rc-banco"
mode = "linear"        # linear: equally spaced series
count = 3
offset = [1.3, 0.0]    # displacement per copy

[[mirror]]             # mirror targets across an axis (two points)
targets = ["pl-nave", "ar-puerta"]
axis = [[2.5, 0.0], [2.5, 1.0]]
```

## Conventions

- Coordinates are world units (see `units`), Y grows upward, origin at [0, 0].
- Use floats (`8.5`, `0.0`) for all coordinates.
- Give every primitive a short prefixed id: `ln-` lines, `pl-` polylines,
  `rc-` rects, `ci-` circles, `ar-` arcs, `tx-` text, `dm-` dims, `ht-` hatches.
- Walls are typically `weight = 0.50`, furniture `0.25`, annotations `0.18`.

## Workflow

```bash
cadforge serve            # live preview in the browser (auto-reloads on save)
cadforge build            # compile to output.dxf
cadforge check --json     # machine-readable validation report
cadforge layers --json    # machine-readable layer listing
cadforge preview          # PNG + metadata JSON (--format svg for vector)
cadforge preview --highlight ln-001,tx-002   # amber markers around those ids
cadforge fmt              # normalize .cf formatting
```

The feedback loop for agents: edit `.cf` → run `cadforge check --json` to
validate → run `cadforge preview` and **look at `preview.png`** — it is a
faithful render (real text, measured dimension labels, hatches, line styles).
`preview.meta.json` maps every entity id to world and pixel bounding boxes.
After editing specific entities, re-render with
`cadforge preview --highlight <ids>` to visually confirm the change landed
where intended (highlighted entities get labeled amber markers).

For humans, `cadforge serve` adds: click any entity to inspect its source
TOML block (copyable as an agent prompt for targeted edits), a layer panel
with on/ghost/off states (trace one floor over another), and a 3D stacked
view of the layers.
"##;

/// Print the `.cf` language reference to stdout.
pub fn print_schema() {
    println!("{}", CF_REFERENCE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_covers_all_primitives() {
        for primitive in [
            "[[line]]",
            "[[polyline]]",
            "[[rect]]",
            "[[circle]]",
            "[[arc]]",
            "[[text]]",
            "[[point]]",
            "[[dim]]",
            "[[hatch]]",
            "[[fill]]",
            "[[group]]",
            "[[array]]",
            "[[mirror]]",
        ] {
            assert!(CF_REFERENCE.contains(primitive), "missing {}", primitive);
        }
    }

    #[test]
    fn reference_examples_are_valid_toml() {
        // Every fenced toml block in the reference must parse.
        let mut in_block = false;
        let mut block = String::new();
        for line in CF_REFERENCE.lines() {
            if line.starts_with("```toml") {
                in_block = true;
                block.clear();
            } else if line.starts_with("```") && in_block {
                in_block = false;
                let parsed: Result<toml::Value, _> = toml::from_str(&block);
                assert!(parsed.is_ok(), "invalid TOML block:\n{}", block);
            } else if in_block {
                block.push_str(line);
                block.push('\n');
            }
        }
    }
}
