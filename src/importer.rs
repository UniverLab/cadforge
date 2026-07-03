//! DXF importer — converts DXF layers/entities into CADspec `.cf` + `project.toml`.

use crate::color::aci_to_hex;
use anyhow::{anyhow, Context, Result};
use dxf::entities::EntityType;
use dxf::Drawing;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Per-entity style recovered from DXF (true color, lineweight, line type).
#[derive(Default, Clone)]
struct StyleAttrs {
    color: Option<String>,
    weight: Option<f64>,
    line_style: Option<&'static str>,
}

impl StyleAttrs {
    fn from_common(common: &dxf::entities::EntityCommon) -> Self {
        let color = if common.color_24_bit > 0 {
            Some(format!("#{:06X}", common.color_24_bit))
        } else {
            common
                .color
                .index()
                .filter(|i| (1..=9).contains(i))
                .map(|i| aci_to_hex(i).to_string())
        };
        let weight = (common.lineweight_enum_value > 0)
            .then(|| f64::from(common.lineweight_enum_value) / 100.0);
        let line_style = match common.line_type_name.to_ascii_uppercase().as_str() {
            "DASHED" => Some("dashed"),
            "DOTTED" => Some("dotted"),
            "DASHDOT" => Some("dashdot"),
            _ => None,
        };
        StyleAttrs {
            color,
            weight,
            line_style,
        }
    }

    fn emit(&self, out: &mut String) {
        if let Some(c) = &self.color {
            out.push_str(&format!("color = \"{}\"\n", c));
        }
        if let Some(w) = self.weight {
            out.push_str(&format!("weight = {}\n", w));
        }
        if let Some(s) = self.line_style {
            out.push_str(&format!("style = \"{}\"\n", s));
        }
    }
}

enum Shape {
    Line {
        from: [f64; 2],
        to: [f64; 2],
    },
    Polyline {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        from_angle: f64,
        to_angle: f64,
    },
    Text {
        position: [f64; 2],
        content: String,
        size: f64,
    },
    Point {
        position: [f64; 2],
    },
    Dim {
        from: [f64; 2],
        to: [f64; 2],
        offset: f64,
    },
    /// A filled region. Imported from one or more SOLID entities; adjacent
    /// SOLID triangles/quads on the same layer are fused into a single fill.
    Fill {
        points: Vec<[f64; 2]>,
    },
}

struct Imported {
    shape: Shape,
    style: StyleAttrs,
}

#[derive(Default)]
struct LayerFile {
    entities: Vec<Imported>,
    counters: BTreeMap<&'static str, usize>,
}

pub fn import_dxf(input: &Path, output_dir: &Path, layer_filter: Option<&str>) -> Result<()> {
    if !input.exists() {
        return Err(anyhow!(
            "Input DXF file does not exist: {}",
            input.display()
        ));
    }

    fs::create_dir_all(output_dir)
        .with_context(|| format!("Cannot create output dir {}", output_dir.display()))?;

    let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
    let mut layer_colors: BTreeMap<String, String> = BTreeMap::new();
    let mut unsupported = 0usize;

    match Drawing::load_file(input) {
        Ok(drawing) => {
            for layer in drawing.layers() {
                if let Some(index) = layer.color.index() {
                    layer_colors.insert(
                        normalize_layer_name(&layer.name),
                        aci_to_hex(index).to_string(),
                    );
                }
            }

            for entity in drawing.entities() {
                let layer_name = normalize_layer_name(&entity.common.layer);
                if let Some(filter) = layer_filter {
                    if filter != layer_name {
                        continue;
                    }
                }

                let style = StyleAttrs::from_common(&entity.common);
                let shape = match &entity.specific {
                    EntityType::Line(e) => Some(Shape::Line {
                        from: [e.p1.x, e.p1.y],
                        to: [e.p2.x, e.p2.y],
                    }),
                    EntityType::LwPolyline(e) => (e.vertices.len() >= 2).then(|| Shape::Polyline {
                        points: e.vertices.iter().map(|v| [v.x, v.y]).collect(),
                        closed: e.is_closed(),
                    }),
                    EntityType::Circle(e) => Some(Shape::Circle {
                        center: [e.center.x, e.center.y],
                        radius: e.radius,
                    }),
                    EntityType::Arc(e) => Some(Shape::Arc {
                        center: [e.center.x, e.center.y],
                        radius: e.radius,
                        from_angle: e.start_angle,
                        to_angle: e.end_angle,
                    }),
                    EntityType::Text(e) => Some(Shape::Text {
                        position: [e.location.x, e.location.y],
                        content: e.value.clone(),
                        size: e.text_height.max(0.1),
                    }),
                    EntityType::ModelPoint(e) => Some(Shape::Point {
                        position: [e.location.x, e.location.y],
                    }),
                    EntityType::RotatedDimension(e) => {
                        let from = [e.definition_point_2.x, e.definition_point_2.y];
                        let to = [e.definition_point_3.x, e.definition_point_3.y];
                        dim_offset(from, to, [e.insertion_point.x, e.insertion_point.y])
                            .map(|offset| Shape::Dim { from, to, offset })
                    }
                    EntityType::Solid(e) => {
                        let ring = solid_ring(e);
                        (ring.len() >= 3).then_some(Shape::Fill { points: ring })
                    }
                    _ => {
                        unsupported += 1;
                        None
                    }
                };
                if let Some(shape) = shape {
                    layers
                        .entry(layer_name)
                        .or_default()
                        .entities
                        .push(Imported { shape, style });
                }
            }

            for layer in layers.values_mut() {
                remove_dim_companions(&mut layer.entities);
                fuse_solids(&mut layer.entities);
            }
        }
        Err(_) => {
            let content = fs::read_to_string(input)
                .with_context(|| format!("Cannot read DXF text: {}", input.display()))?;
            for layer_name in collect_layer_names_from_text(&content) {
                if let Some(filter) = layer_filter {
                    if filter != layer_name {
                        continue;
                    }
                }
                layers.entry(layer_name).or_default();
            }
        }
    }

    if layers.is_empty() {
        let content = fs::read_to_string(input)
            .with_context(|| format!("Cannot read DXF text: {}", input.display()))?;
        for layer_name in collect_layer_names_from_text(&content) {
            if let Some(filter) = layer_filter {
                if filter != layer_name {
                    continue;
                }
            }
            layers.entry(layer_name).or_default();
        }
        if layers.is_empty() {
            for layer_name in collect_layer_names_from_layer_table(&content) {
                if let Some(filter) = layer_filter {
                    if filter != layer_name {
                        continue;
                    }
                }
                layers.entry(layer_name).or_default();
            }
        }
    }

    if layers.is_empty() {
        return Err(anyhow!(
            "No importable entities found in DXF (filter: {})",
            layer_filter.unwrap_or("<none>")
        ));
    }

    let project_name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-project");
    let mut project_toml = format!(
        "[project]\nname = \"{}\"\nscale = \"1:100\"\nunits = \"m\"\n\n[layers]\n",
        escape_string(project_name)
    );

    let mut imported_layers = 0usize;
    for (layer_name, layer_file) in &mut layers {
        imported_layers += 1;
        let file_name = format!("{}.cf", sanitize_for_filename(layer_name));
        let color = layer_colors
            .get(layer_name)
            .map(String::as_str)
            .unwrap_or("#FFFFFF");
        let mut cf = format!(
            "[layer]\nname = \"{}\"\ncolor = \"{}\"\n\n",
            escape_string(layer_name),
            color
        );
        if layer_file.entities.is_empty() {
            cf.push_str("[[line]]\nfrom = [0.0, 0.0]\nto = [1.0, 0.0]\n");
        } else {
            for entity in &layer_file.entities {
                let (prefix, body) = emit_shape(&entity.shape);
                let count = layer_file
                    .counters
                    .entry(prefix)
                    .and_modify(|v| *v += 1)
                    .or_insert(1);
                cf.push_str(&format!(
                    "[[{}]]\nid = \"{prefix}-{count:03}\"\n",
                    header_for(prefix)
                ));
                cf.push_str(&body);
                entity.style.emit(&mut cf);
                cf.push('\n');
            }
        }
        fs::write(
            output_dir.join(&file_name),
            cf.trim_end().to_string() + "\n",
        )
        .with_context(|| format!("Cannot write layer file {}", file_name))?;
        project_toml.push_str(&format!(
            "\"{}\" = {{ file = \"{}\", locked = false }}\n",
            escape_string(layer_name),
            file_name
        ));
    }

    let project_path: PathBuf = output_dir.join("project.toml");
    fs::write(&project_path, project_toml)
        .with_context(|| format!("Cannot write {}", project_path.display()))?;

    println!("✓ Imported DXF: {}", input.display());
    println!("  Layers: {}", imported_layers);
    println!(
        "  Unsupported entities skipped: {} (kept import resilient)",
        unsupported
    );
    println!("  Project: {}", project_path.display());
    Ok(())
}

/// Recover the perpendicular dimension offset from the insertion point.
fn dim_offset(from: [f64; 2], to: [f64; 2], insertion: [f64; 2]) -> Option<f64> {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }
    let (nx, ny) = (-dy / len, dx / len);
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    Some((insertion[0] - mid[0]) * nx + (insertion[1] - mid[1]) * ny)
}

/// Drop the extension/dimension lines and label text that `cadspec build`
/// emits alongside each DIMENSION entity for viewer compatibility; the
/// re-created `[[dim]]` regenerates all of them. Foreign DXFs are unaffected
/// (their dimension graphics live in blocks, not loose entities).
fn remove_dim_companions(entities: &mut Vec<Imported>) {
    const TOL: f64 = 1e-6;
    let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL;

    let dims: Vec<([f64; 2], [f64; 2], f64)> = entities
        .iter()
        .filter_map(|e| match e.shape {
            Shape::Dim { from, to, offset } => Some((from, to, offset)),
            _ => None,
        })
        .collect();

    let mut keep = vec![true; entities.len()];
    for (from, to, offset) in dims {
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let (nx, ny) = (-dy / len, dx / len);
        let a = [from[0] + nx * offset, from[1] + ny * offset];
        let b = [to[0] + nx * offset, to[1] + ny * offset];
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];

        for (cf, ct) in [(from, a), (to, b), (a, b)] {
            mark_line_companion(entities, &mut keep, cf, ct, close);
        }
        mark_text_companion(entities, &mut keep, mid, nx, ny, close);
    }

    let mut it = keep.into_iter();
    entities.retain(|_| it.next().unwrap_or(true));
}

fn mark_line_companion(
    entities: &[Imported],
    keep: &mut [bool],
    from: [f64; 2],
    to: [f64; 2],
    close: impl Fn([f64; 2], [f64; 2]) -> bool,
) {
    if let Some(i) = (0..entities.len()).find(|&i| {
        keep[i]
            && matches!(entities[i].shape, Shape::Line { from: lf, to: lt }
                if (close(lf, from) && close(lt, to)) || (close(lf, to) && close(lt, from)))
    }) {
        keep[i] = false;
    }
}

fn mark_text_companion(
    entities: &[Imported],
    keep: &mut [bool],
    mid: [f64; 2],
    nx: f64,
    ny: f64,
    close: impl Fn([f64; 2], [f64; 2]) -> bool,
) {
    if let Some(i) = (0..entities.len()).find(|&i| {
        keep[i]
            && matches!(entities[i].shape, Shape::Text { position, size, .. }
                if close(position, [mid[0] + nx * size * 0.5, mid[1] + ny * size * 0.5]))
    }) {
        keep[i] = false;
    }
}

/// Recover the polygon ring of a DXF SOLID entity. SOLID vertices render in
/// the order first → second → fourth → third; adjacent duplicates (the writer
/// sets the fourth corner equal to the third for triangles) are collapsed.
fn solid_ring(e: &dxf::entities::Solid) -> Vec<[f64; 2]> {
    let raw = [
        [e.first_corner.x, e.first_corner.y],
        [e.second_corner.x, e.second_corner.y],
        [e.fourth_corner.x, e.fourth_corner.y],
        [e.third_corner.x, e.third_corner.y],
    ];
    let mut ring: Vec<[f64; 2]> = Vec::with_capacity(4);
    for p in raw {
        if ring.last().is_none_or(|q| !pts_eq(*q, p)) {
            ring.push(p);
        }
    }
    // Drop a closing duplicate if the last equals the first.
    if ring.len() > 1 && pts_eq(ring[0], ring[ring.len() - 1]) {
        ring.pop();
    }
    ring
}

/// Quantized key for a point so that geometrically equal vertices from
/// different SOLID entities hash together despite float round-trip noise.
type PtKey = (i64, i64);

fn pt_key(p: [f64; 2]) -> PtKey {
    ((p[0] * 1e6).round() as i64, (p[1] * 1e6).round() as i64)
}

fn pts_eq(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// Undirected edge key with a canonical (min, max) endpoint order.
fn edge_key(a: PtKey, b: PtKey) -> (PtKey, PtKey) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Fuse adjacent SOLID-derived fills into single `[[fill]]` regions.
///
/// `cadspec build` fan-triangulates a solid fill into several SOLID entities;
/// importing each one separately would inflate the entity count and lose the
/// `[[fill]]` semantics. Fills that share an edge belong to the same region:
/// group them, drop the shared (internal) edges, and stitch the remaining
/// boundary edges back into one polygon. Foreign SOLIDs that don't tile a
/// region are left as individual fills.
fn fuse_solids(entities: &mut Vec<Imported>) {
    let fill_positions: Vec<usize> = entities
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.shape, Shape::Fill { .. }))
        .map(|(i, _)| i)
        .collect();
    if fill_positions.len() < 2 {
        return; // nothing to merge
    }

    let ring_of = |pos: usize| -> &[[f64; 2]] {
        match &entities[pos].shape {
            Shape::Fill { points } => points,
            _ => unreachable!("filtered to fills"),
        }
    };

    // Union-find over fills, joined when they share an edge.
    let mut parent: Vec<usize> = (0..fill_positions.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    let mut edge_owner: std::collections::HashMap<(PtKey, PtKey), usize> =
        std::collections::HashMap::new();
    for (idx, &pos) in fill_positions.iter().enumerate() {
        for (a, b) in ring_edges(ring_of(pos)) {
            let key = edge_key(pt_key(a), pt_key(b));
            if let Some(&other) = edge_owner.get(&key) {
                let (ra, rb) = (find(&mut parent, idx), find(&mut parent, other));
                parent[ra] = rb;
            } else {
                edge_owner.insert(key, idx);
            }
        }
    }

    // Group fill indices by component root.
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for idx in 0..fill_positions.len() {
        let root = find(&mut parent, idx);
        components.entry(root).or_default().push(idx);
    }

    // Build replacement fills and track which original positions to drop.
    let mut fused: Vec<Imported> = Vec::new();
    let mut drop: Vec<bool> = vec![false; entities.len()];
    for members in components.values() {
        for &idx in members {
            drop[fill_positions[idx]] = true;
        }
        let style_pos = fill_positions[members[0]];
        let style = entities[style_pos].style.clone();

        if members.len() == 1 {
            // A lone fill: keep its own ring.
            let points = ring_of(fill_positions[members[0]]).to_vec();
            fused.push(Imported {
                shape: Shape::Fill { points },
                style,
            });
            continue;
        }

        let rings: Vec<&[[f64; 2]]> = members
            .iter()
            .map(|&idx| ring_of(fill_positions[idx]))
            .collect();
        match merge_rings(&rings) {
            Some(points) => fused.push(Imported {
                shape: Shape::Fill { points },
                style,
            }),
            None => {
                // Couldn't stitch a clean boundary: keep each fill as-is.
                for &idx in members {
                    fused.push(Imported {
                        shape: Shape::Fill {
                            points: ring_of(fill_positions[idx]).to_vec(),
                        },
                        style: entities[fill_positions[idx]].style.clone(),
                    });
                }
            }
        }
    }

    let mut it = drop.into_iter();
    entities.retain(|_| !it.next().unwrap_or(false));
    entities.extend(fused);
}

/// Iterate the closed ring's undirected edges as consecutive point pairs.
fn ring_edges(ring: &[[f64; 2]]) -> Vec<([f64; 2], [f64; 2])> {
    let n = ring.len();
    (0..n).map(|i| (ring[i], ring[(i + 1) % n])).collect()
}

/// Merge a set of triangle/quad rings that tile one region into a single
/// boundary polygon: edges shared by two rings are interior and dropped; the
/// remaining boundary edges are chained into one ring. Returns `None` if the
/// boundary is not a single simple loop.
fn merge_rings(rings: &[&[[f64; 2]]]) -> Option<Vec<[f64; 2]>> {
    let mut edge_count: std::collections::HashMap<(PtKey, PtKey), usize> =
        std::collections::HashMap::new();
    let mut coord: std::collections::HashMap<PtKey, [f64; 2]> = std::collections::HashMap::new();
    for ring in rings {
        for (a, b) in ring_edges(ring) {
            coord.entry(pt_key(a)).or_insert(a);
            coord.entry(pt_key(b)).or_insert(b);
            *edge_count
                .entry(edge_key(pt_key(a), pt_key(b)))
                .or_insert(0) += 1;
        }
    }

    // Boundary edges appear exactly once.
    let boundary: Vec<(PtKey, PtKey)> = edge_count
        .iter()
        .filter(|(_, &c)| c == 1)
        .map(|(&e, _)| e)
        .collect();
    if boundary.len() < 3 {
        return None;
    }

    // Adjacency; a clean loop has every vertex at degree 2.
    let mut adj: std::collections::HashMap<PtKey, Vec<PtKey>> = std::collections::HashMap::new();
    for (a, b) in &boundary {
        adj.entry(*a).or_default().push(*b);
        adj.entry(*b).or_default().push(*a);
    }
    if adj.values().any(|v| v.len() != 2) {
        return None;
    }

    // Walk the loop from a deterministic start.
    let start = *adj.keys().min()?;
    let mut ring_keys = vec![start];
    let mut prev = start;
    let mut cur = adj[&start][0];
    while cur != start {
        ring_keys.push(cur);
        let nbrs = &adj[&cur];
        let next = if nbrs[0] == prev { nbrs[1] } else { nbrs[0] };
        prev = cur;
        cur = next;
        if ring_keys.len() > boundary.len() {
            return None; // did not close cleanly
        }
    }
    if ring_keys.len() != boundary.len() {
        return None; // disconnected boundary (e.g. a hole)
    }

    Some(ring_keys.iter().map(|k| coord[k]).collect())
}

/// Render a shape body (without `[[header]]`/`id`); returns (id prefix, body).
fn emit_shape(shape: &Shape) -> (&'static str, String) {
    match shape {
        Shape::Line { from, to } => (
            "ln",
            format!(
                "from = [{}, {}]\nto = [{}, {}]\n",
                n(from[0]),
                n(from[1]),
                n(to[0]),
                n(to[1])
            ),
        ),
        Shape::Polyline { points, closed } => {
            let pts = points
                .iter()
                .map(|p| format!("[{}, {}]", n(p[0]), n(p[1])))
                .collect::<Vec<_>>()
                .join(", ");
            ("pl", format!("points = [{}]\nclosed = {}\n", pts, closed))
        }
        Shape::Circle { center, radius } => (
            "ci",
            format!(
                "center = [{}, {}]\nradius = {}\n",
                n(center[0]),
                n(center[1]),
                n(*radius)
            ),
        ),
        Shape::Arc {
            center,
            radius,
            from_angle,
            to_angle,
        } => (
            "ar",
            format!(
                "center = [{}, {}]\nradius = {}\nfrom_angle = {}\nto_angle = {}\n",
                n(center[0]),
                n(center[1]),
                n(*radius),
                n(*from_angle),
                n(*to_angle)
            ),
        ),
        Shape::Text {
            position,
            content,
            size,
        } => (
            "tx",
            format!(
                "position = [{}, {}]\ncontent = \"{}\"\nsize = {}\n",
                n(position[0]),
                n(position[1]),
                escape_string(content),
                n(*size)
            ),
        ),
        Shape::Point { position } => (
            "pt",
            format!("position = [{}, {}]\n", n(position[0]), n(position[1])),
        ),
        Shape::Dim { from, to, offset } => (
            "dm",
            format!(
                "type = \"linear\"\nfrom = [{}, {}]\nto = [{}, {}]\noffset = {}\n",
                n(from[0]),
                n(from[1]),
                n(to[0]),
                n(to[1]),
                n(*offset)
            ),
        ),
        Shape::Fill { points } => {
            let pts = points
                .iter()
                .map(|p| format!("[{}, {}]", n(p[0]), n(p[1])))
                .collect::<Vec<_>>()
                .join(", ");
            ("fl", format!("points = [{}]\n", pts))
        }
    }
}

fn header_for(prefix: &str) -> &'static str {
    match prefix {
        "ln" => "line",
        "pl" => "polyline",
        "ci" => "circle",
        "ar" => "arc",
        "tx" => "text",
        "pt" => "point",
        "dm" => "dim",
        "fl" => "fill",
        _ => "line",
    }
}

fn n(v: f64) -> String {
    format!("{:.4}", v)
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn sanitize_for_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "layer".to_string()
    } else {
        out
    }
}

fn normalize_layer_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn collect_layer_names_from_text(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut lines = content.lines();
    while let Some(code) = lines.next() {
        let Some(value) = lines.next() else {
            break;
        };
        if code.trim() == "8" {
            let layer = normalize_layer_name(value);
            if layer != "0" && !names.iter().any(|existing| existing == &layer) {
                names.push(layer);
            }
        }
    }
    names
}

fn collect_layer_names_from_layer_table(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut lines = content.lines();
    let mut in_layer_record = false;
    while let Some(code) = lines.next() {
        let Some(value) = lines.next() else {
            break;
        };
        let code = code.trim();
        let value = value.trim();
        if code == "100" && value == "AcDbLayerTableRecord" {
            in_layer_record = true;
            continue;
        }
        if in_layer_record && code == "2" {
            let layer = normalize_layer_name(value);
            if layer != "0" && !names.iter().any(|existing| existing == &layer) {
                names.push(layer);
            }
            in_layer_record = false;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(points: Vec<[f64; 2]>) -> Imported {
        Imported {
            shape: Shape::Fill { points },
            style: StyleAttrs::default(),
        }
    }

    fn ring_of(e: &Imported) -> &[[f64; 2]] {
        match &e.shape {
            Shape::Fill { points } => points,
            _ => panic!("expected fill"),
        }
    }

    #[test]
    fn merge_rings_stitches_fan_triangles_into_rectangle() {
        // Fan triangulation of rect [0,0]-[2,2] from vertex [0,0].
        let t1: &[[f64; 2]] = &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]];
        let t2: &[[f64; 2]] = &[[0.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let ring = merge_rings(&[t1, t2]).expect("should stitch a clean loop");
        assert_eq!(ring.len(), 4, "shared diagonal edge is dropped");
        // All four rectangle corners present regardless of winding/start.
        for corner in [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] {
            assert!(
                ring.iter().any(|p| pts_eq(*p, corner)),
                "missing corner {corner:?}"
            );
        }
    }

    #[test]
    fn fuse_solids_merges_adjacent_and_keeps_disjoint_separate() {
        let mut entities = vec![
            // Region A: two triangles tiling rect [0,0]-[2,2].
            fill(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]),
            fill(vec![[0.0, 0.0], [2.0, 2.0], [0.0, 2.0]]),
            // Region B: a lone triangle far away.
            fill(vec![[10.0, 10.0], [11.0, 10.0], [10.5, 11.0]]),
        ];
        fuse_solids(&mut entities);
        let fills: Vec<&Imported> = entities
            .iter()
            .filter(|e| matches!(e.shape, Shape::Fill { .. }))
            .collect();
        assert_eq!(fills.len(), 2, "adjacent pair fuses, disjoint stays");
        let mut lens: Vec<usize> = fills.iter().map(|e| ring_of(e).len()).collect();
        lens.sort_unstable();
        assert_eq!(lens, vec![3, 4], "one rectangle (4) and one triangle (3)");
    }

    #[test]
    fn fuse_solids_leaves_single_fill_untouched() {
        let mut entities = vec![fill(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]])];
        fuse_solids(&mut entities);
        assert_eq!(entities.len(), 1);
        assert_eq!(ring_of(&entities[0]).len(), 3);
    }
}
