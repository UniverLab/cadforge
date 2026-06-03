//! DXF importer — converts DXF layers/entities into CADforge `.cf` + `project.toml`.

use anyhow::{anyhow, Context, Result};
use dxf::entities::EntityType;
use dxf::Drawing;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct LayerFile {
    entities: Vec<String>,
    counters: BTreeMap<&'static str, usize>,
}

impl LayerFile {
    fn next_id(&mut self, prefix: &'static str) -> String {
        let n = self
            .counters
            .entry(prefix)
            .and_modify(|v| *v += 1)
            .or_insert(1);
        format!("{prefix}-{n:03}")
    }
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
    let mut unsupported = 0usize;

    match Drawing::load_file(input) {
        Ok(drawing) => {
            for entity in drawing.entities() {
                let layer_name = normalize_layer_name(&entity.common.layer);
                if let Some(filter) = layer_filter {
                    if filter != layer_name {
                        continue;
                    }
                }

                let layer = layers.entry(layer_name.clone()).or_default();
                match &entity.specific {
                    EntityType::Line(e) => {
                        let id = layer.next_id("ln");
                        layer.entities.push(format!(
                            "[[line]]\nid = \"{}\"\nfrom = [{}, {}]\nto = [{}, {}]\n",
                            id,
                            n(e.p1.x),
                            n(e.p1.y),
                            n(e.p2.x),
                            n(e.p2.y)
                        ));
                    }
                    EntityType::LwPolyline(e) => {
                        if e.vertices.len() >= 2 {
                            let id = layer.next_id("pl");
                            let points = e
                                .vertices
                                .iter()
                                .map(|v| format!("[{}, {}]", n(v.x), n(v.y)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            layer.entities.push(format!(
                                "[[polyline]]\nid = \"{}\"\npoints = [{}]\nclosed = {}\n",
                                id,
                                points,
                                e.is_closed()
                            ));
                        }
                    }
                    EntityType::Circle(e) => {
                        let id = layer.next_id("ci");
                        layer.entities.push(format!(
                            "[[circle]]\nid = \"{}\"\ncenter = [{}, {}]\nradius = {}\n",
                            id,
                            n(e.center.x),
                            n(e.center.y),
                            n(e.radius)
                        ));
                    }
                    EntityType::Arc(e) => {
                        let id = layer.next_id("ar");
                        layer.entities.push(format!(
                            "[[arc]]\nid = \"{}\"\ncenter = [{}, {}]\nradius = {}\nfrom_angle = {}\nto_angle = {}\n",
                            id,
                            n(e.center.x),
                            n(e.center.y),
                            n(e.radius),
                            n(e.start_angle),
                            n(e.end_angle)
                        ));
                    }
                    EntityType::Text(e) => {
                        let id = layer.next_id("tx");
                        layer.entities.push(format!(
                            "[[text]]\nid = \"{}\"\nposition = [{}, {}]\ncontent = \"{}\"\nsize = {}\n",
                            id,
                            n(e.location.x),
                            n(e.location.y),
                            escape_string(&e.value),
                            n(e.text_height.max(0.1))
                        ));
                    }
                    EntityType::ModelPoint(e) => {
                        let id = layer.next_id("pt");
                        layer.entities.push(format!(
                            "[[point]]\nid = \"{}\"\nposition = [{}, {}]\n",
                            id,
                            n(e.location.x),
                            n(e.location.y)
                        ));
                    }
                    EntityType::RotatedDimension(e) => {
                        let id = layer.next_id("dm");
                        let from_x = e.definition_point_2.x;
                        let from_y = e.definition_point_2.y;
                        let to_x = e.definition_point_3.x;
                        let to_y = e.definition_point_3.y;
                        let offset = e.insertion_point.y - (from_y + to_y) / 2.0;
                        layer.entities.push(format!(
                            "[[dim]]\nid = \"{}\"\ntype = \"linear\"\nfrom = [{}, {}]\nto = [{}, {}]\noffset = {}\n",
                            id,
                            n(from_x),
                            n(from_y),
                            n(to_x),
                            n(to_y),
                            n(offset)
                        ));
                    }
                    _ => {
                        unsupported += 1;
                    }
                }
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
    for (layer_name, layer_file) in &layers {
        imported_layers += 1;
        let file_name = format!("{}.cf", sanitize_for_filename(layer_name));
        let mut cf = format!(
            "[layer]\nname = \"{}\"\ncolor = \"#FFFFFF\"\n\n",
            escape_string(layer_name)
        );
        if layer_file.entities.is_empty() {
            cf.push_str("[[line]]\nfrom = [0.0, 0.0]\nto = [1.0, 0.0]\n");
        } else {
            for e in &layer_file.entities {
                cf.push_str(e);
                cf.push('\n');
            }
        }
        fs::write(output_dir.join(&file_name), cf)
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
