//! SVG renderer — full-fidelity vector rendering of a project.
//!
//! Renders real text, dimension lines with measured values, hatch patterns
//! clipped to their boundary, line styles, and optional highlight markers.
//! It is the single rendering backend: `cadspec serve` displays the SVG
//! directly and the PNG preview rasterizes it.

use crate::compiler::resolve_boundary;
use crate::model::{CfFile, CommonAttrs, LineStyle, TextAlign};
use crate::parser::{parse_cf, parse_project};
use crate::transform::expand_cf;
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::path::Path;

const PADDING: f64 = 1.0; // world units around content
const MAX_HEIGHT_PX: f64 = 4096.0;
const BG_COLOR: &str = "#141414";
const GRID_COLOR: &str = "#232323";
const AXIS_COLOR: &str = "#333333";

const LAYER_PALETTE: &[&str] = &[
    "#FFFFFF", "#FF5050", "#50FF50", "#50C8FF", "#FFC850", "#C878FF", "#FF9632",
];

// ── Public API ──────────────────────────────────────────────────────────

/// A rendered SVG plus the world→pixel transform used to produce it, so
/// consumers (PNG rasterizer, metadata) can map world coordinates to pixels.
pub struct Scene {
    pub svg: String,
    /// Pixels per world unit.
    pub px_per_unit: f64,
    /// World X of the left canvas edge.
    pub offset_x: f64,
    /// World Y of the bottom canvas edge.
    pub offset_y: f64,
    /// Canvas height in world units (used for the Y flip).
    pub world_h: f64,
    pub width_px: f64,
    pub height_px: f64,
    /// Content bounds (without padding): [min_x, min_y, max_x, max_y].
    pub world_bounds: [f64; 4],
}

impl Scene {
    /// Map a world coordinate to image pixels.
    pub fn world_to_px(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.offset_x) * self.px_per_unit,
            (self.world_h - (y - self.offset_y)) * self.px_per_unit,
        )
    }
}

/// Parse `project.toml` and its (filtered) layer files in one pass, with
/// `[[array]]`/`[[mirror]]` constructions expanded into concrete primitives.
pub fn load_project_layers(
    project_dir: &Path,
    layer_filter: Option<&str>,
) -> Result<(crate::parser::ProjectFile, Vec<(String, CfFile)>)> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    let layers: Vec<(String, CfFile)> = project
        .layers
        .iter()
        .filter(|(name, _)| layer_filter.is_none_or(|f| f == *name))
        .map(|(name, entry)| {
            let cf = parse_cf(&project_dir.join(&entry.file))
                .with_context(|| format!("Failed to parse layer '{}'", name))?;
            Ok((name.clone(), expand_cf(&cf)))
        })
        .collect::<Result<_>>()?;

    Ok((project, layers))
}

/// Render already-parsed layers to a [`Scene`], optionally highlighting ids.
pub fn render_scene_from(
    project_name: &str,
    units: &str,
    layers: &[(String, CfFile)],
    width: u32,
    highlight: &[String],
) -> Scene {
    render_layers(project_name, units, layers, width, highlight)
}

/// Render the project to a [`Scene`], optionally highlighting entities by id.
pub fn render_scene(
    project_dir: &Path,
    layer_filter: Option<&str>,
    width: u32,
    highlight: &[String],
) -> Result<Scene> {
    let (project, layers) = load_project_layers(project_dir, layer_filter)?;
    Ok(render_layers(
        &project.project.name,
        &project.project.units,
        &layers,
        width,
        highlight,
    ))
}

/// Render the project to an SVG string.
pub fn render_svg(project_dir: &Path, layer_filter: Option<&str>, width: u32) -> Result<String> {
    Ok(render_scene(project_dir, layer_filter, width, &[])?.svg)
}

/// Display color of a layer: its declared color, or a palette color by index.
pub fn layer_display_color(cf: &CfFile, index: usize) -> String {
    cf.layer_meta
        .as_ref()
        .and_then(|m| m.color.clone())
        .unwrap_or_else(|| LAYER_PALETTE[index % LAYER_PALETTE.len()].to_string())
}

// ── Canvas ──────────────────────────────────────────────────────────────

struct Canvas {
    out: String,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    world_h: f64,
    width_px: f64,
    height_px: f64,
    clip_seq: usize,
}

impl Canvas {
    fn world_to_px(&self, x: f64, y: f64) -> (f64, f64) {
        let px = (x - self.offset_x) * self.scale;
        let py = (self.world_h - (y - self.offset_y)) * self.scale;
        (px, py)
    }

    fn points_attr(&self, points: &[(f64, f64)]) -> String {
        let mut s = String::with_capacity(points.len() * 16);
        for (i, &(x, y)) in points.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            let (px, py) = self.world_to_px(x, y);
            let _ = write!(s, "{:.2},{:.2}", px, py);
        }
        s
    }
}

#[derive(Clone)]
struct Style {
    color: String,
    width_px: f64,
    dash: Option<&'static str>,
}

fn resolve_style(common: &CommonAttrs, layer_color: &str, default_weight: f64) -> Style {
    let color = common
        .color
        .clone()
        .unwrap_or_else(|| layer_color.to_string());
    let weight = common.weight.unwrap_or(default_weight);
    let width_px = ((weight / 0.35) * 1.4).clamp(0.6, 6.0);
    let dash = match common.style {
        Some(LineStyle::Dashed) => Some("8,6"),
        Some(LineStyle::Dotted) => Some("1.5,5"),
        Some(LineStyle::Dashdot) => Some("10,4,1.5,4"),
        _ => None,
    };
    Style {
        color,
        width_px,
        dash,
    }
}

fn stroke_attrs(s: &Style) -> String {
    let mut a = format!(
        r#"stroke="{}" stroke-width="{:.2}" fill="none""#,
        s.color, s.width_px
    );
    if let Some(dash) = s.dash {
        let _ = write!(a, r#" stroke-dasharray="{}""#, dash);
    }
    a
}

fn id_attr(common: &CommonAttrs) -> String {
    match &common.id {
        Some(id) => format!(r#" data-id="{}""#, xml_escape(id)),
        None => String::new(),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── Bounds ──────────────────────────────────────────────────────────────

struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min_x: f64::MAX,
            min_y: f64::MAX,
            max_x: f64::MIN,
            max_y: f64::MIN,
        }
    }

    fn add(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn is_empty(&self) -> bool {
        self.min_x > self.max_x
    }
}

fn compute_bounds(layers: &[(String, CfFile)]) -> Bounds {
    let mut b = Bounds::empty();
    for (_, cf) in layers {
        for e in &cf.lines {
            b.add(e.from[0], e.from[1]);
            b.add(e.to[0], e.to[1]);
        }
        for e in &cf.polylines {
            for p in &e.points {
                b.add(p[0], p[1]);
            }
        }
        for e in &cf.rects {
            b.add(e.origin[0], e.origin[1]);
            b.add(e.origin[0] + e.width, e.origin[1] + e.height);
        }
        for e in &cf.circles {
            b.add(e.center[0] - e.radius, e.center[1] - e.radius);
            b.add(e.center[0] + e.radius, e.center[1] + e.radius);
        }
        for e in &cf.arcs {
            b.add(e.center[0] - e.radius, e.center[1] - e.radius);
            b.add(e.center[0] + e.radius, e.center[1] + e.radius);
        }
        for e in &cf.texts {
            b.add(e.position[0], e.position[1]);
        }
        for e in &cf.points {
            b.add(e.position[0], e.position[1]);
        }
        for e in &cf.dims {
            b.add(e.from[0], e.from[1]);
            b.add(e.to[0], e.to[1]);
        }
        for e in &cf.fills {
            if let Some(points) = &e.points {
                for p in points {
                    b.add(p[0], p[1]);
                }
            }
        }
    }
    if b.is_empty() {
        Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        }
    } else {
        b
    }
}

// ── Rendering ───────────────────────────────────────────────────────────

fn render_layers(
    project_name: &str,
    units: &str,
    layers: &[(String, CfFile)],
    width: u32,
    highlight: &[String],
) -> Scene {
    let bounds = compute_bounds(layers);
    let world_w = bounds.max_x - bounds.min_x + 2.0 * PADDING;
    let world_h = bounds.max_y - bounds.min_y + 2.0 * PADDING;

    let width_px = width as f64;
    let height_px = (width_px * world_h / world_w).min(MAX_HEIGHT_PX);
    let scale = (width_px / world_w).min(height_px / world_h);

    let mut canvas = Canvas {
        out: String::with_capacity(16 * 1024),
        scale,
        offset_x: bounds.min_x - PADDING,
        offset_y: bounds.min_y - PADDING,
        world_h,
        width_px,
        height_px,
        clip_seq: 0,
    };

    let _ = write!(
        canvas.out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" data-project="{name}">"#,
        w = canvas.width_px,
        h = canvas.height_px,
        name = xml_escape(project_name),
    );
    let _ = write!(
        canvas.out,
        r#"<rect width="100%" height="100%" fill="{}"/>"#,
        BG_COLOR
    );

    draw_grid(&mut canvas, &bounds);

    for (idx, (layer_name, cf)) in layers.iter().enumerate() {
        let layer_color = layer_display_color(cf, idx);
        let default_weight = cf
            .layer_meta
            .as_ref()
            .and_then(|m| m.line_weight)
            .unwrap_or(0.35);
        let visible = cf.layer_meta.as_ref().map(|m| m.visible).unwrap_or(true);
        if !visible {
            continue;
        }
        let _ = write!(canvas.out, r#"<g data-layer="{}">"#, xml_escape(layer_name));
        render_layer(&mut canvas, cf, &layer_color, default_weight, units);
        canvas.out.push_str("</g>");
    }

    if !highlight.is_empty() {
        draw_highlights(&mut canvas, layers, highlight);
    }

    canvas.out.push_str("</svg>");
    Scene {
        px_per_unit: canvas.scale,
        offset_x: canvas.offset_x,
        offset_y: canvas.offset_y,
        world_h: canvas.world_h,
        width_px: canvas.width_px,
        height_px: canvas.height_px,
        world_bounds: [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y],
        svg: canvas.out,
    }
}

// ── Highlights (visual verification markers for agents) ────────────────

const HIGHLIGHT_COLOR: &str = "#FFB300";

fn draw_highlights(c: &mut Canvas, layers: &[(String, CfFile)], highlight: &[String]) {
    c.out.push_str(r#"<g data-highlights="true">"#);
    for (_, cf) in layers {
        for rec in enumerate_entities(cf) {
            let Some(id) = &rec.id else { continue };
            if !highlight.iter().any(|h| h == id) {
                continue;
            }
            let (x1, y1) = c.world_to_px(rec.bbox[0], rec.bbox[3]); // top-left
            let (x2, y2) = c.world_to_px(rec.bbox[2], rec.bbox[1]); // bottom-right
            let margin = 8.0;
            let _ = write!(
                c.out,
                r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" stroke="{}" stroke-width="1.8" stroke-dasharray="6,4" fill="none" data-highlight="{}"/>"#,
                x1 - margin,
                y1 - margin,
                (x2 - x1) + 2.0 * margin,
                (y2 - y1) + 2.0 * margin,
                HIGHLIGHT_COLOR,
                xml_escape(id),
            );
            let _ = write!(
                c.out,
                r#"<text x="{:.2}" y="{:.2}" font-size="14" font-family="monospace" fill="{}">{}</text>"#,
                x1 - margin,
                y1 - margin - 6.0,
                HIGHLIGHT_COLOR,
                xml_escape(id),
            );
        }
    }
    c.out.push_str("</g>");
}

// ── Entity enumeration (shared with the PNG preview metadata) ───────────

/// A primitive with its world-space bounding box, for metadata and highlights.
pub struct EntityRecord {
    pub id: Option<String>,
    pub kind: &'static str,
    /// [min_x, min_y, max_x, max_y] in world units.
    pub bbox: [f64; 4],
    /// Text content, for `text` entities.
    pub content: Option<String>,
}

/// Enumerate the drawable primitives of a layer with their world bounds.
pub fn enumerate_entities(cf: &CfFile) -> Vec<EntityRecord> {
    fn bbox_of(points: &[(f64, f64)]) -> [f64; 4] {
        let mut b = Bounds::empty();
        for &(x, y) in points {
            b.add(x, y);
        }
        [b.min_x, b.min_y, b.max_x, b.max_y]
    }

    let mut out = Vec::new();
    for e in &cf.lines {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "line",
            bbox: bbox_of(&[(e.from[0], e.from[1]), (e.to[0], e.to[1])]),
            content: None,
        });
    }
    for e in &cf.polylines {
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "polyline",
            bbox: bbox_of(&pts),
            content: None,
        });
    }
    for e in &cf.rects {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "rect",
            bbox: [
                e.origin[0],
                e.origin[1],
                e.origin[0] + e.width,
                e.origin[1] + e.height,
            ],
            content: None,
        });
    }
    for e in &cf.circles {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "circle",
            bbox: [
                e.center[0] - e.radius,
                e.center[1] - e.radius,
                e.center[0] + e.radius,
                e.center[1] + e.radius,
            ],
            content: None,
        });
    }
    for e in &cf.arcs {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "arc",
            bbox: bbox_of(&arc_points(
                e.center[0],
                e.center[1],
                e.radius,
                e.from_angle,
                e.to_angle,
            )),
            content: None,
        });
    }
    for e in &cf.texts {
        // Approximate extent from monospace glyph proportions.
        let w = 0.6 * e.size * e.content.chars().count() as f64;
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "text",
            bbox: [
                e.position[0],
                e.position[1],
                e.position[0] + w,
                e.position[1] + e.size,
            ],
            content: Some(e.content.clone()),
        });
    }
    for e in &cf.points {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "point",
            bbox: [
                e.position[0] - 0.05,
                e.position[1] - 0.05,
                e.position[0] + 0.05,
                e.position[1] + 0.05,
            ],
            content: None,
        });
    }
    for e in &cf.dims {
        let dx = e.to[0] - e.from[0];
        let dy = e.to[1] - e.from[1];
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (nx, ny) = (-dy / len, dx / len);
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "dim",
            bbox: bbox_of(&[
                (e.from[0], e.from[1]),
                (e.to[0], e.to[1]),
                (e.from[0] + nx * e.offset, e.from[1] + ny * e.offset),
                (e.to[0] + nx * e.offset, e.to[1] + ny * e.offset),
            ]),
            content: None,
        });
    }
    for e in &cf.hatches {
        let pts = if let Some(boundary_id) = &e.boundary {
            resolve_boundary(boundary_id, cf)
        } else {
            e.points
                .as_ref()
                .map(|p| p.iter().map(|v| (v[0], v[1])).collect())
        };
        if let Some(pts) = pts {
            out.push(EntityRecord {
                id: e.common.id.clone(),
                kind: "hatch",
                bbox: bbox_of(&pts),
                content: None,
            });
        }
    }
    for e in &cf.fills {
        let pts = if let Some(boundary_id) = &e.boundary {
            resolve_boundary(boundary_id, cf)
        } else {
            e.points
                .as_ref()
                .map(|p| p.iter().map(|v| (v[0], v[1])).collect())
        };
        if let Some(pts) = pts {
            out.push(EntityRecord {
                id: e.common.id.clone(),
                kind: "fill",
                bbox: bbox_of(&pts),
                content: None,
            });
        }
    }
    out
}

fn grid_step(world_w: f64) -> f64 {
    const STEPS: &[f64] = &[0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 500.0];
    for &s in STEPS {
        if world_w / s <= 40.0 {
            return s;
        }
    }
    1000.0
}

fn draw_grid(c: &mut Canvas, bounds: &Bounds) {
    let step = grid_step(bounds.max_x - bounds.min_x + 2.0 * PADDING);
    let x0 = ((bounds.min_x - PADDING) / step).floor() * step;
    let x1 = bounds.max_x + PADDING;
    let y0 = ((bounds.min_y - PADDING) / step).floor() * step;
    let y1 = bounds.max_y + PADDING;

    c.out.push_str(r#"<g data-grid="true">"#);
    let mut x = x0;
    while x <= x1 {
        let (px, _) = c.world_to_px(x, 0.0);
        let color = if x.abs() < 1e-9 {
            AXIS_COLOR
        } else {
            GRID_COLOR
        };
        let _ = write!(
            c.out,
            r#"<line x1="{px:.2}" y1="0" x2="{px:.2}" y2="{h:.2}" stroke="{color}" stroke-width="1"/>"#,
            h = c.height_px,
        );
        x += step;
    }
    let mut y = y0;
    while y <= y1 {
        let (_, py) = c.world_to_px(0.0, y);
        let color = if y.abs() < 1e-9 {
            AXIS_COLOR
        } else {
            GRID_COLOR
        };
        let _ = write!(
            c.out,
            r#"<line x1="0" y1="{py:.2}" x2="{w:.2}" y2="{py:.2}" stroke="{color}" stroke-width="1"/>"#,
            w = c.width_px,
        );
        y += step;
    }
    c.out.push_str("</g>");
}

fn render_layer(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64, units: &str) {
    for e in cf.lines.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (x1, y1) = c.world_to_px(e.from[0], e.from[1]);
        let (x2, y2) = c.world_to_px(e.to[0], e.to[1]);
        let _ = write!(
            c.out,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" {}{}/>"#,
            x1,
            y1,
            x2,
            y2,
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }

    for e in cf.polylines.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        let tag = if e.closed { "polygon" } else { "polyline" };
        let _ = write!(
            c.out,
            r#"<{} points="{}" {}{}/>"#,
            tag,
            c.points_attr(&pts),
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }

    for e in cf.rects.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.origin[0], e.origin[1] + e.height);
        let _ = write!(
            c.out,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" {}{}/>"#,
            px,
            py,
            e.width * c.scale,
            e.height * c.scale,
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }

    for e in cf.circles.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.center[0], e.center[1]);
        let _ = write!(
            c.out,
            r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" {}{}/>"#,
            px,
            py,
            e.radius * c.scale,
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }

    for e in cf.arcs.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let pts = arc_points(e.center[0], e.center[1], e.radius, e.from_angle, e.to_angle);
        let _ = write!(
            c.out,
            r#"<polyline points="{}" {}{}/>"#,
            c.points_attr(&pts),
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }

    // Fills and hatches go before text so labels stay readable on top.
    for e in cf.fills.iter().filter(|e| e.common.visible) {
        let pts = if let Some(boundary_id) = &e.boundary {
            resolve_boundary(boundary_id, cf)
        } else {
            e.points
                .as_ref()
                .map(|p| p.iter().map(|v| (v[0], v[1])).collect())
        };
        if let Some(pts) = pts {
            let s = resolve_style(&e.common, layer_color, default_weight);
            let _ = write!(
                c.out,
                r#"<polygon points="{}" fill="{}" fill-opacity="0.35" stroke="none"{}/>"#,
                c.points_attr(&pts),
                s.color,
                id_attr(&e.common)
            );
        }
    }

    for e in cf.hatches.iter().filter(|e| e.common.visible) {
        let boundary = if let Some(boundary_id) = &e.boundary {
            resolve_boundary(boundary_id, cf)
        } else {
            e.points
                .as_ref()
                .map(|p| p.iter().map(|v| (v[0], v[1])).collect())
        };
        if let Some(boundary) = boundary {
            let s = resolve_style(&e.common, layer_color, default_weight);
            draw_hatch(
                c,
                &boundary,
                e.pattern.as_str(),
                e.angle,
                0.1 * e.scale,
                &s,
                &e.common,
            );
        }
    }

    for e in cf.dims.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        draw_dim(c, e, &s, units);
    }

    for e in cf.points.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.position[0], e.position[1]);
        let _ = write!(
            c.out,
            r#"<path d="M {x0:.2} {y:.2} H {x1:.2} M {x:.2} {y0:.2} V {y1:.2}" {attrs}{id}/>"#,
            x0 = px - 4.0,
            x1 = px + 4.0,
            y0 = py - 4.0,
            y1 = py + 4.0,
            x = px,
            y = py,
            attrs = stroke_attrs(&s),
            id = id_attr(&e.common)
        );
    }

    for e in cf.texts.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.position[0], e.position[1]);
        let anchor = match e.align {
            Some(TextAlign::Center) => "middle",
            Some(TextAlign::Right) => "end",
            _ => "start",
        };
        let font_px = (e.size * c.scale).max(1.0);
        let _ = write!(
            c.out,
            r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="monospace" text-anchor="{}" fill="{}"{}>{}</text>"#,
            px,
            py,
            font_px,
            anchor,
            s.color,
            id_attr(&e.common),
            xml_escape(&e.content)
        );
    }
}

fn arc_points(cx: f64, cy: f64, radius: f64, from_deg: f64, to_deg: f64) -> Vec<(f64, f64)> {
    const STEPS: usize = 48;
    let start = from_deg.to_radians();
    let delta = (to_deg.to_radians() - start) / STEPS as f64;
    (0..=STEPS)
        .map(|i| {
            let a = start + delta * i as f64;
            (cx + radius * a.cos(), cy + radius * a.sin())
        })
        .collect()
}

fn draw_dim(c: &mut Canvas, dim: &crate::model::CfDim, s: &Style, units: &str) {
    let (from, to, offset) = (dim.from, dim.to, dim.offset);
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);

    // Dimension line endpoints, offset along the normal
    let a = (from[0] + nx * offset, from[1] + ny * offset);
    let b = (to[0] + nx * offset, to[1] + ny * offset);

    let (ax, ay) = c.world_to_px(a.0, a.1);
    let (bx, by) = c.world_to_px(b.0, b.1);
    let (fx, fy) = c.world_to_px(from[0], from[1]);
    let (tx, ty) = c.world_to_px(to[0], to[1]);

    let _ = write!(c.out, r#"<g data-dim="true"{}>"#, id_attr(&dim.common));
    // Extension lines + dimension line
    let _ = write!(
        c.out,
        r#"<path d="M {fx:.2} {fy:.2} L {ax:.2} {ay:.2} M {tx:.2} {ty:.2} L {bx:.2} {by:.2} M {ax:.2} {ay:.2} L {bx:.2} {by:.2}" stroke="{color}" stroke-width="{w:.2}" fill="none"/>"#,
        color = s.color,
        w = (s.width_px * 0.8).max(0.6),
    );
    // Tick marks (45° slashes) at both ends
    let tick = 5.0;
    for &(px, py) in &[(ax, ay), (bx, by)] {
        let _ = write!(
            c.out,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{:.2}"/>"#,
            px - tick,
            py + tick,
            px + tick,
            py - tick,
            s.color,
            (s.width_px * 0.8).max(0.6),
        );
    }
    // Measured value label at the midpoint
    let text_size = dim.text_size.unwrap_or(0.0);
    let label_gap = if text_size > 0.0 {
        text_size * 0.6
    } else {
        0.15
    };
    let mid = (
        (a.0 + b.0) / 2.0 + nx * label_gap,
        (a.1 + b.1) / 2.0 + ny * label_gap,
    );
    let (mx, my) = c.world_to_px(mid.0, mid.1);
    let font_px = if text_size > 0.0 {
        (text_size * c.scale).max(1.0)
    } else {
        (0.22 * c.scale).clamp(9.0, 28.0)
    };
    let label = format_dim_label(
        len,
        dim.precision.unwrap_or(2) as usize,
        dim.show_units,
        units,
    );
    let _ = write!(
        c.out,
        r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="monospace" text-anchor="middle" fill="{}">{}</text>"#,
        mx,
        my,
        font_px,
        s.color,
        xml_escape(&label),
    );
    c.out.push_str("</g>");
}

/// Format a dimension label: measured value with the configured precision,
/// optionally followed by the project units.
pub fn format_dim_label(
    len: f64,
    precision: usize,
    show_units: Option<bool>,
    units: &str,
) -> String {
    if show_units.unwrap_or(true) {
        format!("{:.prec$} {}", len, units, prec = precision)
    } else {
        format!("{:.prec$}", len, prec = precision)
    }
}

fn draw_hatch(
    c: &mut Canvas,
    boundary: &[(f64, f64)],
    pattern: &str,
    angle_deg: f64,
    spacing: f64,
    s: &Style,
    common: &CommonAttrs,
) {
    if boundary.is_empty() || spacing <= 0.0 {
        return;
    }

    if pattern == "solid" {
        let _ = write!(
            c.out,
            r#"<polygon points="{}" fill="{}" fill-opacity="0.5" stroke="none"{}/>"#,
            c.points_attr(boundary),
            s.color,
            id_attr(common)
        );
        return;
    }

    // Bounding box of the boundary
    let mut b = Bounds::empty();
    for &(x, y) in boundary {
        b.add(x, y);
    }
    let cx = (b.min_x + b.max_x) / 2.0;
    let cy = (b.min_y + b.max_y) / 2.0;
    let half_diag = (((b.max_x - b.min_x).powi(2) + (b.max_y - b.min_y).powi(2)).sqrt()) / 2.0;

    let theta = angle_deg.to_radians();
    let (dx, dy) = (theta.cos(), theta.sin());
    let (nx, ny) = (-dy, dx);

    let n = ((half_diag / spacing).ceil() as i64).min(2000);

    c.clip_seq += 1;
    let clip_id = format!("hatch-clip-{}", c.clip_seq);
    let _ = write!(
        c.out,
        r#"<clipPath id="{}"><polygon points="{}"/></clipPath>"#,
        clip_id,
        c.points_attr(boundary)
    );
    let _ = write!(
        c.out,
        r#"<g clip-path="url(#{})"{}>"#,
        clip_id,
        id_attr(common)
    );
    let mut path = String::new();
    for k in -n..=n {
        let ox = cx + nx * spacing * k as f64;
        let oy = cy + ny * spacing * k as f64;
        let (x1, y1) = c.world_to_px(ox - dx * half_diag, oy - dy * half_diag);
        let (x2, y2) = c.world_to_px(ox + dx * half_diag, oy + dy * half_diag);
        let _ = write!(path, "M {:.2} {:.2} L {:.2} {:.2} ", x1, y1, x2, y2);
    }
    let _ = write!(
        c.out,
        r#"<path d="{}" stroke="{}" stroke-width="0.8" fill="none"/>"#,
        path.trim_end(),
        s.color
    );
    c.out.push_str("</g>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_layers() -> Vec<(String, CfFile)> {
        let toml = r##"
[layer]
name = "test"
color = "#FFFFFF"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [10.0, 0.0]
style = "dashed"

[[rect]]
id = "rc-1"
origin = [1.0, 1.0]
width = 3.0
height = 2.0

[[circle]]
center = [5.0, 5.0]
radius = 1.5

[[arc]]
center = [2.0, 2.0]
radius = 1.0
from_angle = 0.0
to_angle = 90.0

[[text]]
position = [5.0, 5.0]
content = "SALA <principal>"
size = 0.3
align = "center"

[[dim]]
from = [0.0, 0.0]
to = [10.0, 0.0]
offset = -0.8

[[polyline]]
id = "pl-room"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
closed = true

[[hatch]]
boundary = "pl-room"
pattern = "ansi31"
"##;
        let cf: CfFile = toml::from_str(toml).unwrap();
        vec![("test".to_string(), cf)]
    }

    #[test]
    fn renders_all_primitives() {
        let svg = render_layers("demo", "m", &sample_layers(), 1200, &[]).svg;
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("<line"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("<polyline")); // arc
        assert!(svg.contains("<polygon")); // closed polyline
        assert!(svg.contains("clipPath")); // hatch
        assert!(svg.contains("stroke-dasharray")); // dashed line
        assert!(svg.contains(r#"data-id="ln-1""#));
    }

    #[test]
    fn escapes_text_content() {
        let svg = render_layers("demo", "m", &sample_layers(), 1200, &[]).svg;
        assert!(svg.contains("SALA &lt;principal&gt;"));
        assert!(!svg.contains("SALA <principal>"));
    }

    #[test]
    fn dim_label_shows_measured_length() {
        let svg = render_layers("demo", "m", &sample_layers(), 1200, &[]).svg;
        assert!(svg.contains("10.00 m"));
    }

    #[test]
    fn empty_project_renders_default_viewport() {
        let svg = render_layers("empty", "m", &[], 800, &[]).svg;
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r#"width="800""#));
    }

    #[test]
    fn highlight_draws_marker_for_matching_id() {
        let scene = render_layers(
            "demo",
            "m",
            &sample_layers(),
            1200,
            &["rc-1".to_string(), "missing-id".to_string()],
        );
        assert!(scene.svg.contains(r#"data-highlight="rc-1""#));
        assert!(scene.svg.contains(">rc-1</text>"));
        assert!(!scene.svg.contains(r#"data-highlight="missing-id""#));
    }

    #[test]
    fn enumerate_entities_covers_bounds_and_content() {
        let layers = sample_layers();
        let records = enumerate_entities(&layers[0].1);
        // line, rect, circle, arc, text, dim, polyline, hatch
        assert_eq!(records.len(), 8);
        let text = records.iter().find(|r| r.kind == "text").unwrap();
        assert_eq!(text.content.as_deref(), Some("SALA <principal>"));
        let rect = records.iter().find(|r| r.kind == "rect").unwrap();
        assert_eq!(rect.bbox, [1.0, 1.0, 4.0, 3.0]);
    }

    #[test]
    fn scene_world_to_px_is_consistent_with_canvas() {
        let scene = render_layers("demo", "m", &sample_layers(), 1200, &[]);
        // Bottom-left content corner with padding maps inside the canvas
        let (px, py) = scene.world_to_px(scene.world_bounds[0], scene.world_bounds[1]);
        assert!(px > 0.0 && px < scene.width_px);
        assert!(py > 0.0 && py <= scene.height_px);
    }

    #[test]
    fn grid_step_scales_with_world_size() {
        assert_eq!(grid_step(10.0), 0.5);
        assert_eq!(grid_step(30.0), 1.0);
        assert_eq!(grid_step(300.0), 10.0);
    }
}
