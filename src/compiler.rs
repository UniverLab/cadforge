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
    for (name, entry) in &project.layers {
        let _ = entry; // locked is informational for now
        writer.add_layer(name, 7); // default white, overridden by layer meta
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
        writer.polyline(&points, poly.closed, layer);
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
}
