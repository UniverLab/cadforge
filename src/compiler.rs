//! Compiler — transforms the intermediate model into a DXF file via DxfWriter.

use crate::color::{hex_to_24bit, hex_to_aci, weight_to_dxf};
use crate::dxf_writer::{DxfWriter, EntityStyle};
use crate::model::{CfFile, CommonAttrs, LineStyle};
use crate::parser::{parse_cf, parse_project, LayerEntry, ProjectFile};
use crate::transform::expand_cf;
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn new(x: f64, y: f64) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    fn include_point(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn contains(&self, other: &Bounds) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
    }
}

#[derive(Default)]
struct ConstraintRules {
    parent: Vec<(String, String)>,
    belongs_to: Vec<(String, String)>,
    spatial_dependency: Vec<(String, String)>,
    strict: bool,
}

// ── Style resolution (DRY: one place to convert CommonAttrs → EntityStyle) ──

fn resolve_style(common: &CommonAttrs) -> EntityStyle {
    EntityStyle {
        color_24bit: common.color.as_deref().map(hex_to_24bit),
        lineweight: common.weight.map(weight_to_dxf),
        line_type: common.style.as_ref().map(line_style_to_dxf_name),
    }
}

fn line_style_to_dxf_name(style: &LineStyle) -> String {
    match style {
        LineStyle::Solid => "CONTINUOUS".to_string(),
        LineStyle::Dashed => "DASHED".to_string(),
        LineStyle::Dotted => "DOTTED".to_string(),
        LineStyle::Dashdot => "DASHDOT".to_string(),
    }
}

fn resolve_layer<'a>(common: &'a CommonAttrs, default: &'a str) -> &'a str {
    common.layer.as_deref().unwrap_or(default)
}

fn load_layers(
    project_dir: &Path,
    layers: &IndexMap<String, LayerEntry>,
) -> Result<IndexMap<String, CfFile>> {
    let mut loaded = IndexMap::with_capacity(layers.len());
    for (name, entry) in layers {
        let cf_path = project_dir.join(&entry.file);
        let cf = parse_cf(&cf_path).with_context(|| format!("Failed to parse layer '{}'", name))?;
        loaded.insert(name.clone(), expand_cf(&cf));
    }
    Ok(loaded)
}

fn extract_constraint_rules(project: &ProjectFile) -> ConstraintRules {
    let mut rules = ConstraintRules::default();
    let Some(toml::Value::Table(table)) = project.constraints.as_ref() else {
        return rules;
    };

    for (key, value) in table {
        if key == "strict" {
            if let toml::Value::Boolean(strict) = value {
                rules.strict = *strict;
            }
            continue;
        }

        if key.contains('→') {
            if let toml::Value::String(kind) = value {
                if kind == "spatial_dependency" {
                    let mut parts = key.split('→').map(|s| s.trim().to_string());
                    if let (Some(from), Some(to)) = (parts.next(), parts.next()) {
                        rules.spatial_dependency.push((from, to));
                    }
                }
            }
            continue;
        }

        if let toml::Value::Table(child_rules) = value {
            if let Some(toml::Value::String(parent)) = child_rules.get("parent") {
                rules.parent.push((key.clone(), parent.clone()));
            }
            if let Some(toml::Value::String(parent)) = child_rules.get("belongs_to") {
                rules.belongs_to.push((key.clone(), parent.clone()));
            }
        }
    }

    rules
}

fn layer_bbox(cf: &CfFile) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    let mut include = |x: f64, y: f64| {
        if let Some(b) = bounds.as_mut() {
            b.include_point(x, y);
        } else {
            bounds = Some(Bounds::new(x, y));
        }
    };

    for e in &cf.lines {
        include(e.from[0], e.from[1]);
        include(e.to[0], e.to[1]);
    }
    for e in &cf.polylines {
        for p in &e.points {
            include(p[0], p[1]);
        }
    }
    for e in &cf.rects {
        include(e.origin[0], e.origin[1]);
        include(e.origin[0] + e.width, e.origin[1] + e.height);
    }
    for e in &cf.circles {
        include(e.center[0] - e.radius, e.center[1] - e.radius);
        include(e.center[0] + e.radius, e.center[1] + e.radius);
    }
    for e in &cf.arcs {
        include(e.center[0] - e.radius, e.center[1] - e.radius);
        include(e.center[0] + e.radius, e.center[1] + e.radius);
    }
    for e in &cf.texts {
        include(e.position[0], e.position[1]);
    }
    for e in &cf.points {
        include(e.position[0], e.position[1]);
    }
    for e in &cf.dims {
        include(e.from[0], e.from[1]);
        include(e.to[0], e.to[1]);
    }
    for e in &cf.fills {
        if let Some(points) = &e.points {
            for p in points {
                include(p[0], p[1]);
            }
        }
    }

    bounds
}

fn collect_layer_ids(cf: &CfFile) -> HashSet<String> {
    let mut ids = HashSet::new();
    for_each_common(cf, |common| {
        if let Some(id) = &common.id {
            ids.insert(id.clone());
        }
    });
    ids
}

fn for_each_common(cf: &CfFile, mut f: impl FnMut(&CommonAttrs)) {
    for e in &cf.lines {
        f(&e.common);
    }
    for e in &cf.polylines {
        f(&e.common);
    }
    for e in &cf.rects {
        f(&e.common);
    }
    for e in &cf.circles {
        f(&e.common);
    }
    for e in &cf.arcs {
        f(&e.common);
    }
    for e in &cf.texts {
        f(&e.common);
    }
    for e in &cf.points {
        f(&e.common);
    }
    for e in &cf.dims {
        f(&e.common);
    }
    for e in &cf.hatches {
        f(&e.common);
    }
    for e in &cf.fills {
        f(&e.common);
    }
    for e in &cf.groups {
        f(&e.common);
    }
}

fn validate_constraints(project: &ProjectFile, layers: &IndexMap<String, CfFile>) -> Vec<String> {
    let rules = extract_constraint_rules(project);
    let mut issues = Vec::new();

    for (child, parent) in &rules.parent {
        match (layers.get(child), layers.get(parent)) {
            (Some(child_cf), Some(parent_cf)) => {
                let child_bbox = layer_bbox(child_cf);
                let parent_bbox = layer_bbox(parent_cf);
                match (child_bbox, parent_bbox) {
                    (Some(c), Some(p)) => {
                        if !p.contains(&c) {
                            issues.push(format!(
                                "Layer '{}' violates parent='{}': child bbox [{:.2}, {:.2}]->[{:.2}, {:.2}] is outside parent bbox [{:.2}, {:.2}]->[{:.2}, {:.2}]",
                                child, parent, c.min_x, c.min_y, c.max_x, c.max_y, p.min_x, p.min_y, p.max_x, p.max_y
                            ));
                        }
                    }
                    _ => {
                        issues.push(format!(
                            "Layer '{}' parent='{}' cannot be validated because one layer has no measurable geometry",
                            child, parent
                        ));
                    }
                }
            }
            _ => issues.push(format!(
                "Invalid parent constraint: '{}' or '{}' layer does not exist",
                child, parent
            )),
        }
    }

    for (child, parent) in &rules.belongs_to {
        match (layers.get(child), layers.get(parent)) {
            (Some(child_cf), Some(parent_cf)) => {
                let parent_ids = collect_layer_ids(parent_cf);
                let mut total = 0usize;
                let mut referenced = 0usize;

                for_each_common(child_cf, |common| {
                    total += 1;
                    if let Some(reference) = &common.belongs_to {
                        referenced += 1;
                        if !parent_ids.contains(reference) {
                            issues.push(format!(
                                "Layer '{}' has belongs_to='{}' but id does not exist in parent layer '{}'",
                                child, reference, parent
                            ));
                        }
                    }
                });

                if total > 0 && referenced == 0 {
                    issues.push(format!(
                        "Layer '{}' has belongs_to='{}' constraint but no primitives define belongs_to references",
                        child, parent
                    ));
                }
            }
            _ => issues.push(format!(
                "Invalid belongs_to constraint: '{}' or '{}' layer does not exist",
                child, parent
            )),
        }
    }

    for (from, to) in &rules.spatial_dependency {
        if layers.contains_key(from) && layers.contains_key(to) {
            issues.push(format!(
                "spatial_dependency '{}' -> '{}' registered; dynamic movement tracking is not implemented yet (warning only)",
                from, to
            ));
        } else {
            issues.push(format!(
                "Invalid spatial_dependency '{}' -> '{}': one layer does not exist",
                from, to
            ));
        }
    }

    issues
}

fn is_strict(project: &ProjectFile) -> bool {
    project.project.strict || extract_constraint_rules(project).strict
}

fn print_constraint_issues(issues: &[String]) {
    for issue in issues {
        println!("warning CONSTRAINT VIOLATION");
        println!("  Detail: {}", issue);
        println!("  Action: build continues with warning (set strict = true to fail)");
        println!();
    }
}

// ── Public API ──────────────────────────────────────────────────────────

/// Compile a full project (project.toml + .cf files) into a single DXF.
pub fn compile_project(
    project_dir: &Path,
    layer_filter: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut writer = DxfWriter::new();

    for name in project.layers.keys() {
        writer.add_layer(name, 7);
    }

    let loaded_layers = load_layers(project_dir, &project.layers)?;
    let issues = validate_constraints(&project, &loaded_layers);
    let strict = is_strict(&project);
    if !issues.is_empty() {
        print_constraint_issues(&issues);
        if strict {
            bail!(
                "Build blocked: {} constraint violation(s) with strict = true",
                issues.len()
            );
        }
    }

    let mut total_entities = 0usize;
    let mut layer_stats: Vec<(String, usize)> = Vec::new();
    for name in project.layers.keys() {
        if layer_filter.is_none_or(|f| f == name) {
            let cf = loaded_layers
                .get(name)
                .with_context(|| format!("Failed to load layer '{}'", name))?;
            let count = entity_count(cf);
            compile_cf(&mut writer, cf, name);
            total_entities += count;
            layer_stats.push((name.to_string(), count));
        }
    }

    let output_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| project_dir.join("output.dxf"));
    writer.save(&output_path)?;

    println!("✓ DXF generado: {}", output_path.display());
    println!(
        "  {} entidades en {} capas",
        total_entities,
        layer_stats.len()
    );
    for (name, count) in &layer_stats {
        println!("    {}: {} entidades", name, count);
    }
    Ok(())
}

/// Validate a project without generating DXF output.
pub fn check_project(project_dir: &Path) -> Result<usize> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let loaded_layers = load_layers(project_dir, &project.layers)?;
    let issues = validate_constraints(&project, &loaded_layers);
    let strict = is_strict(&project);
    let mut total = 0usize;

    println!("Project: {}", project.project.name);
    println!(
        "Scale: {}  Units: {}",
        project.project.scale, project.project.units
    );
    println!();

    for (name, entry) in &project.layers {
        let cf = loaded_layers
            .get(name)
            .with_context(|| format!("Failed to load layer '{}'", name))?;
        let count = entity_count(cf);
        let color = cf
            .layer_meta
            .as_ref()
            .and_then(|m| m.color.as_deref())
            .unwrap_or("#FFFFFF");
        println!("  ✓ {} — {} entities [{}]", entry.file, count, color);
        total += count;
    }

    if !issues.is_empty() {
        println!();
        print_constraint_issues(&issues);
        if strict {
            bail!(
                "Check failed: {} constraint violation(s) with strict = true",
                issues.len()
            );
        }
    }

    println!();
    println!(
        "✓ Valid: {} layers, {} total entities",
        project.layers.len(),
        total
    );
    Ok(total)
}

/// List layers in a project with their status.
pub fn list_layers(project_dir: &Path) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    println!("Project: {}", project.project.name);
    println!(
        "Scale: {}  Units: {}",
        project.project.scale, project.project.units
    );
    println!();
    println!("{:<20} {:<25} {:<10} Color", "Layer", "File", "Entities");
    println!("{}", "-".repeat(65));

    for (name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        let (status, color) = if cf_path.exists() {
            let cf = expand_cf(&parse_cf(&cf_path)?);
            let count = entity_count(&cf);
            let col = cf
                .layer_meta
                .as_ref()
                .and_then(|m| m.color.as_deref())
                .unwrap_or("#FFFFFF");
            (format!("{}", count), col.to_string())
        } else {
            ("⚠ missing".to_string(), "-".to_string())
        };
        let lock = if entry.locked { " [locked]" } else { "" };
        println!(
            "{:<20} {:<25} {:<10} {}{}",
            name, entry.file, status, color, lock
        );
    }
    Ok(())
}

// ── Machine-readable report (for AI agents / tooling) ──────────────────

#[derive(Serialize)]
pub struct LayerReport {
    pub name: String,
    pub file: String,
    pub entities: Option<usize>,
    pub color: Option<String>,
    pub locked: bool,
    pub missing: bool,
}

#[derive(Serialize)]
pub struct ProjectReport {
    pub name: String,
    pub scale: String,
    pub units: String,
    pub strict: bool,
    pub total_entities: usize,
    pub layers: Vec<LayerReport>,
    pub issues: Vec<String>,
}

/// Build a structured validation report of the project (used by `--json` flags).
pub fn project_report(project_dir: &Path) -> Result<ProjectReport> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut loaded: IndexMap<String, CfFile> = IndexMap::new();
    let mut layers = Vec::with_capacity(project.layers.len());
    let mut total = 0usize;

    for (name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        if cf_path.exists() {
            let cf = expand_cf(
                &parse_cf(&cf_path).with_context(|| format!("Failed to parse layer '{}'", name))?,
            );
            let count = entity_count(&cf);
            total += count;
            layers.push(LayerReport {
                name: name.clone(),
                file: entry.file.clone(),
                entities: Some(count),
                color: cf.layer_meta.as_ref().and_then(|m| m.color.clone()),
                locked: entry.locked,
                missing: false,
            });
            loaded.insert(name.clone(), cf);
        } else {
            layers.push(LayerReport {
                name: name.clone(),
                file: entry.file.clone(),
                entities: None,
                color: None,
                locked: entry.locked,
                missing: true,
            });
        }
    }

    let issues = validate_constraints(&project, &loaded);
    Ok(ProjectReport {
        name: project.project.name.clone(),
        scale: project.project.scale.clone(),
        units: project.project.units.clone(),
        strict: is_strict(&project),
        total_entities: total,
        layers,
        issues,
    })
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
        + cf.fills.len()
        + cf.groups.len()
}

/// Compile a single .cf file into the DxfWriter (public for integration tests).
pub fn compile_cf_public(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    compile_cf(writer, cf, default_layer);
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
        let dist = ((e.to[0] - e.from[0]).powi(2) + (e.to[1] - e.from[1]).powi(2)).sqrt();
        let label =
            crate::svg::format_dim_label(dist, e.precision.unwrap_or(2) as usize, e.show_units, "")
                .trim_end()
                .to_string();
        writer.dim_linear(
            e.from[0],
            e.from[1],
            e.to[0],
            e.to[1],
            e.offset,
            &label,
            e.text_size.unwrap_or(0.25),
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }

    // Hatches: resolve boundary by id, generate pattern lines
    for e in &cf.hatches {
        let layer = resolve_layer(&e.common, default_layer);
        let style = resolve_style(&e.common);
        let spacing = 0.1 * e.scale; // base spacing scaled

        if let Some(boundary) = resolve_boundary(&e.boundary, cf) {
            writer.hatch(&boundary, e.angle, spacing, layer, &style);
        }
    }

    // Solid fills
    for e in &cf.fills {
        let layer = resolve_layer(&e.common, default_layer);
        let style = resolve_style(&e.common);

        let pts = if let Some(ref boundary_id) = e.boundary {
            resolve_boundary(boundary_id, cf)
        } else {
            e.points
                .as_ref()
                .map(|p| p.iter().map(|v| (v[0], v[1])).collect())
        };

        if let Some(pts) = pts {
            writer.solid_fill(&pts, layer, &style);
        }
    }
}

/// Resolve a boundary id to a list of (x,y) points from polylines or rects in the file.
pub fn resolve_boundary(id: &str, cf: &CfFile) -> Option<Vec<(f64, f64)>> {
    // Search polylines
    for poly in &cf.polylines {
        if poly.common.id.as_deref() == Some(id) && poly.closed {
            return Some(poly.points.iter().map(|p| (p[0], p[1])).collect());
        }
    }
    // Search rects
    for rect in &cf.rects {
        if rect.common.id.as_deref() == Some(id) {
            let (x, y) = (rect.origin[0], rect.origin[1]);
            return Some(vec![
                (x, y),
                (x + rect.width, y),
                (x + rect.width, y + rect.height),
                (x, y + rect.height),
            ]);
        }
    }
    None
}
