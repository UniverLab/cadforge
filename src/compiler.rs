//! Compiler — transforms the intermediate model into a DXF file via DxfWriter.

use crate::dxf_writer::{DxfWriter, LineStyle};
use crate::model::CfFile;
use crate::parser::{parse_cf, parse_project};
use anyhow::{Context, Result};
use std::path::Path;

/// ACI color index from hex string (best-effort mapping).
fn hex_to_aci(hex: &str) -> u8 {
    match hex.to_uppercase().trim_start_matches('#') {
        "FF0000" => 1, // red
        "FFFF00" => 2, // yellow
        "00FF00" => 3, // green
        "00FFFF" => 4, // cyan
        "0000FF" => 5, // blue
        "FF00FF" => 6, // magenta
        "FFFFFF" => 7, // white
        "808080" => 8, // dark grey
        "C0C0C0" => 9, // light grey
        _ => 7,        // default white
    }
}

/// Lineweight in mm → DXF lineweight enum value (hundredths of mm).
fn weight_to_dxf(mm: f64) -> i16 {
    (mm * 100.0) as i16
}

/// Compile a full project (project.toml + .cf files) into a single DXF.
pub fn compile_project(project_dir: &Path) -> Result<()> {
    let project_path = project_dir.join("project.toml");
    let project = parse_project(&project_path)?;

    let mut writer = DxfWriter::new();

    // Register layers
    for name in project.layers.keys() {
        writer.add_layer(name, 7);
    }

    // Process each layer file
    for (layer_name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        let cf = parse_cf(&cf_path)
            .with_context(|| format!("Failed to parse layer '{}'", layer_name))?;
        compile_cf(&mut writer, &cf, layer_name);
    }

    let output = project_dir.join("output.dxf");
    writer.save(&output)?;
    println!("✓ DXF generado: {}", output.display());
    Ok(())
}

/// Validate a project without generating DXF output.
/// Returns the total number of entities found across all layers.
pub fn check_project(project_dir: &Path) -> Result<usize> {
    let project_path = project_dir.join("project.toml");
    let project = parse_project(&project_path)?;

    let mut total_entities = 0;

    for (layer_name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        let cf = parse_cf(&cf_path)
            .with_context(|| format!("Failed to parse layer '{}'", layer_name))?;

        let count = entity_count(&cf);
        println!("  ✓ {} — {} entities", entry.file, count);
        total_entities += count;
    }

    println!(
        "✓ Project valid: {} layers, {} total entities",
        project.layers.len(),
        total_entities
    );
    Ok(total_entities)
}

/// List layers in a project with their status.
pub fn list_layers(project_dir: &Path) -> Result<()> {
    let project_path = project_dir.join("project.toml");
    let project = parse_project(&project_path)?;

    println!("Project: {}", project.project.name);
    println!("Layers:");
    for (name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        let status = if cf_path.exists() {
            let cf = parse_cf(&cf_path)?;
            format!("{} entities", entity_count(&cf))
        } else {
            "⚠ file missing".to_string()
        };
        let lock = if entry.locked { " [locked]" } else { "" };
        println!("  {} → {} ({}){}", name, entry.file, status, lock);
    }
    Ok(())
}

fn entity_count(cf: &CfFile) -> usize {
    cf.lines.len()
        + cf.polylines.len()
        + cf.rects.len()
        + cf.circles.len()
        + cf.arcs.len()
        + cf.texts.len()
        + cf.points.len()
        + cf.dims.len()
        + cf.hatches.len()
        + cf.groups.len()
}

/// Compile a single .cf file into the DxfWriter.
pub fn compile_cf(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    // If layer meta defines a color, update the layer
    if let Some(meta) = &cf.layer_meta {
        if let Some(color) = &meta.color {
            writer.add_layer(default_layer, hex_to_aci(color));
        }
    }

    for line in &cf.lines {
        let layer = line.common.layer.as_deref().unwrap_or(default_layer);
        match line.common.weight {
            Some(w) => writer.line_styled(
                line.from[0],
                line.from[1],
                line.to[0],
                line.to[1],
                layer,
                &LineStyle {
                    color_index: 7,
                    lineweight: weight_to_dxf(w),
                },
            ),
            None => writer.line(line.from[0], line.from[1], line.to[0], line.to[1], layer),
        }
    }

    for poly in &cf.polylines {
        let layer = poly.common.layer.as_deref().unwrap_or(default_layer);
        let points: Vec<(f64, f64)> = poly.points.iter().map(|p| (p[0], p[1])).collect();
        let lw = poly.common.weight.map(weight_to_dxf);
        writer.polyline_styled(&points, poly.closed, layer, lw);
    }

    for rect in &cf.rects {
        let layer = rect.common.layer.as_deref().unwrap_or(default_layer);
        writer.rect(
            rect.origin[0],
            rect.origin[1],
            rect.width,
            rect.height,
            layer,
        );
    }

    for circle in &cf.circles {
        let layer = circle.common.layer.as_deref().unwrap_or(default_layer);
        writer.circle(circle.center[0], circle.center[1], circle.radius, layer);
    }

    for arc in &cf.arcs {
        let layer = arc.common.layer.as_deref().unwrap_or(default_layer);
        writer.arc(
            arc.center[0],
            arc.center[1],
            arc.radius,
            arc.from_angle,
            arc.to_angle,
            layer,
        );
    }

    for text in &cf.texts {
        let layer = text.common.layer.as_deref().unwrap_or(default_layer);
        writer.text(
            text.position[0],
            text.position[1],
            text.size,
            &text.content,
            layer,
        );
    }

    for point in &cf.points {
        let layer = point.common.layer.as_deref().unwrap_or(default_layer);
        writer.point(point.position[0], point.position[1], layer);
    }

    for dim in &cf.dims {
        let layer = dim.common.layer.as_deref().unwrap_or(default_layer);
        writer.dim_linear(
            dim.from[0],
            dim.from[1],
            dim.to[0],
            dim.to[1],
            dim.offset,
            layer,
        );
    }
}
