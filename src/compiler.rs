//! Compiler — transforms the intermediate model into a DXF file via DxfWriter.

use crate::color::{hex_to_24bit, hex_to_aci, weight_to_dxf};
use crate::dxf_writer::{DxfWriter, EntityStyle};
use crate::model::{CfFile, CommonAttrs};
use crate::parser::{parse_cf, parse_project, LayerEntry};
use anyhow::{Context, Result};
use std::path::Path;

// ── Style resolution (DRY: one place to convert CommonAttrs → EntityStyle) ──

fn resolve_style(common: &CommonAttrs) -> EntityStyle {
    EntityStyle {
        color_24bit: common.color.as_deref().map(hex_to_24bit),
        lineweight: common.weight.map(weight_to_dxf),
    }
}

fn resolve_layer<'a>(common: &'a CommonAttrs, default: &'a str) -> &'a str {
    common.layer.as_deref().unwrap_or(default)
}

// ── Layer iteration (DRY: shared between compile/check/list) ────────────

struct LayerVisitor<'a> {
    project_dir: &'a Path,
}

impl<'a> LayerVisitor<'a> {
    fn new(project_dir: &'a Path) -> Self {
        Self { project_dir }
    }

    fn visit_each<F>(&self, layers: &indexmap::IndexMap<String, LayerEntry>, mut f: F) -> Result<()>
    where
        F: FnMut(&str, &LayerEntry, &CfFile) -> Result<()>,
    {
        for (name, entry) in layers {
            let cf_path = self.project_dir.join(&entry.file);
            let cf =
                parse_cf(&cf_path).with_context(|| format!("Failed to parse layer '{}'", name))?;
            f(name, entry, &cf)?;
        }
        Ok(())
    }
}

// ── Public API ──────────────────────────────────────────────────────────

/// Compile a full project (project.toml + .cf files) into a single DXF.
pub fn compile_project(project_dir: &Path, layer_filter: Option<&str>) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut writer = DxfWriter::new();

    for name in project.layers.keys() {
        writer.add_layer(name, 7);
    }

    let visitor = LayerVisitor::new(project_dir);
    visitor.visit_each(&project.layers, |name, _entry, cf| {
        if layer_filter.is_none_or(|f| f == name) {
            compile_cf(&mut writer, cf, name);
        }
        Ok(())
    })?;

    let output = project_dir.join("output.dxf");
    writer.save(&output)?;
    println!("✓ DXF generado: {}", output.display());
    Ok(())
}

/// Validate a project without generating DXF output.
pub fn check_project(project_dir: &Path) -> Result<usize> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut total = 0;

    let visitor = LayerVisitor::new(project_dir);
    visitor.visit_each(&project.layers, |_name, entry, cf| {
        let count = entity_count(cf);
        println!("  ✓ {} — {} entities", entry.file, count);
        total += count;
        Ok(())
    })?;

    println!(
        "✓ Project valid: {} layers, {} total entities",
        project.layers.len(),
        total
    );
    Ok(total)
}

/// List layers in a project with their status.
pub fn list_layers(project_dir: &Path) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

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

// ── Internal ────────────────────────────────────────────────────────────

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
fn compile_cf(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    if let Some(meta) = &cf.layer_meta {
        if let Some(color) = &meta.color {
            writer.add_layer(default_layer, hex_to_aci(color));
        }
    }

    for e in &cf.lines {
        let style = resolve_style(&e.common);
        writer.line(
            e.from[0],
            e.from[1],
            e.to[0],
            e.to[1],
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.polylines {
        let style = resolve_style(&e.common);
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        writer.polyline(
            &pts,
            e.closed,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.rects {
        let style = resolve_style(&e.common);
        writer.rect(
            e.origin[0],
            e.origin[1],
            e.width,
            e.height,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.circles {
        let style = resolve_style(&e.common);
        writer.circle(
            e.center[0],
            e.center[1],
            e.radius,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.arcs {
        let style = resolve_style(&e.common);
        writer.arc(
            e.center[0],
            e.center[1],
            e.radius,
            e.from_angle,
            e.to_angle,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.texts {
        let style = resolve_style(&e.common);
        writer.text(
            e.position[0],
            e.position[1],
            e.size,
            &e.content,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.points {
        let style = resolve_style(&e.common);
        writer.point(
            e.position[0],
            e.position[1],
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    for e in &cf.dims {
        let style = resolve_style(&e.common);
        writer.dim_linear(
            e.from[0],
            e.from[1],
            e.to[0],
            e.to[1],
            e.offset,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}
