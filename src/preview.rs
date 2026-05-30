//! Preview — renders project to PNG + metadata JSON for multimodal AI agents.

use crate::model::{CfFile, CommonAttrs};
use crate::parser::{parse_cf, parse_project};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

// ── Configuration ───────────────────────────────────────────────────────

const DEFAULT_WIDTH: u32 = 2048;
const DEFAULT_HEIGHT: u32 = 1536;
const PADDING: f64 = 0.5; // world units padding around content
const STROKE_WIDTH: f32 = 1.5;
const TEXT_MARKER: f64 = 0.05;

fn bg_color() -> Color {
    Color::from_rgba8(20, 20, 20, 255)
}

// ── Bounds accumulator (DRY: one place to track min/max) ────────────────

#[derive(Serialize, Clone, Copy)]
pub struct WorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl WorldBounds {
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

    fn as_bbox(&self) -> [f64; 4] {
        [self.min_x, self.min_y, self.max_x, self.max_y]
    }
}

/// Compute the bounding box of a set of points.
fn points_bounds(points: &[(f64, f64)]) -> WorldBounds {
    let mut b = WorldBounds::empty();
    for &(x, y) in points {
        b.add(x, y);
    }
    b
}

// ── Metadata structures (for the agent) ─────────────────────────────────

#[derive(Serialize)]
pub struct PreviewMeta {
    pub project_name: String,
    pub image_file: String,
    pub width_px: u32,
    pub height_px: u32,
    pub world_bounds: WorldBounds,
    pub scale: f64,
    pub layers: Vec<LayerInfo>,
    pub entities: Vec<EntityInfo>,
}

#[derive(Serialize)]
pub struct LayerInfo {
    pub name: String,
    pub entity_count: usize,
    pub color: String,
}

#[derive(Serialize)]
pub struct EntityInfo {
    pub id: Option<String>,
    pub entity_type: String,
    pub layer: String,
    pub bbox: [f64; 4],       // [min_x, min_y, max_x, max_y] in world coords
    pub pixel_bbox: [u32; 4], // [x, y, w, h] in image coords
}

// ── Renderer ────────────────────────────────────────────────────────────

struct Renderer {
    pixmap: Pixmap,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    world_height: f64,
}

impl Renderer {
    fn new(width: u32, height: u32, bounds: &WorldBounds) -> Result<Self> {
        let world_w = bounds.max_x - bounds.min_x + 2.0 * PADDING;
        let world_h = bounds.max_y - bounds.min_y + 2.0 * PADDING;

        let scale = (width as f64 / world_w).min(height as f64 / world_h);

        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| anyhow::anyhow!("Invalid image dimensions {}x{}", width, height))?;
        pixmap.fill(bg_color());

        Ok(Self {
            pixmap,
            scale,
            offset_x: bounds.min_x - PADDING,
            offset_y: bounds.min_y - PADDING,
            world_height: world_h,
        })
    }

    fn world_to_px(&self, x: f64, y: f64) -> (f32, f32) {
        let px = ((x - self.offset_x) * self.scale) as f32;
        // Flip Y: world Y goes up, pixel Y goes down
        let py = ((self.world_height - (y - self.offset_y)) * self.scale) as f32;
        (px, py)
    }

    /// Single stroke entry point — all draw_* methods funnel through here (DRY).
    fn stroke(&mut self, path: tiny_skia::Path, color: Color, width: f32) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: Color, width: f32) {
        let (px1, py1) = self.world_to_px(x1, y1);
        let (px2, py2) = self.world_to_px(x2, y2);
        let mut pb = PathBuilder::new();
        pb.move_to(px1, py1);
        pb.line_to(px2, py2);
        if let Some(path) = pb.finish() {
            self.stroke(path, color, width);
        }
    }

    fn draw_circle(&mut self, cx: f64, cy: f64, radius: f64, color: Color, width: f32) {
        let (pcx, pcy) = self.world_to_px(cx, cy);
        let pr = (radius * self.scale) as f32;
        let mut pb = PathBuilder::new();
        pb.push_circle(pcx, pcy, pr);
        if let Some(path) = pb.finish() {
            self.stroke(path, color, width);
        }
    }

    fn draw_arc(&mut self, arc: ArcSpec, color: Color, width: f32) {
        const STEPS: usize = 32;
        let start = arc.start_deg.to_radians();
        let delta = (arc.end_deg.to_radians() - start) / STEPS as f64;

        let mut pb = PathBuilder::new();
        for i in 0..=STEPS {
            let angle = start + delta * i as f64;
            let (px, py) = self.world_to_px(
                arc.cx + arc.radius * angle.cos(),
                arc.cy + arc.radius * angle.sin(),
            );
            if i == 0 {
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        if let Some(path) = pb.finish() {
            self.stroke(path, color, width);
        }
    }

    fn draw_polyline(&mut self, points: &[(f64, f64)], closed: bool, color: Color, width: f32) {
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        let mut pb = PathBuilder::new();
        let (px, py) = self.world_to_px(first.0, first.1);
        pb.move_to(px, py);
        for &(x, y) in rest {
            let (px, py) = self.world_to_px(x, y);
            pb.line_to(px, py);
        }
        if closed {
            pb.close();
        }
        if let Some(path) = pb.finish() {
            self.stroke(path, color, width);
        }
    }

    fn save_png(&self, path: &Path) -> Result<()> {
        self.pixmap
            .save_png(path)
            .map_err(|e| anyhow::anyhow!("Failed to save PNG: {}", e))
    }

    fn entity_info(
        &self,
        common: &CommonAttrs,
        entity_type: &str,
        layer: &str,
        bounds: WorldBounds,
    ) -> EntityInfo {
        let (px1, py1) = self.world_to_px(bounds.min_x, bounds.max_y); // top-left
        let (px2, py2) = self.world_to_px(bounds.max_x, bounds.min_y); // bottom-right
        EntityInfo {
            id: common.id.clone(),
            entity_type: entity_type.to_string(),
            layer: layer.to_string(),
            bbox: bounds.as_bbox(),
            pixel_bbox: [
                px1 as u32,
                py1 as u32,
                (px2 - px1) as u32,
                (py2 - py1) as u32,
            ],
        }
    }
}

#[derive(Clone, Copy)]
struct ArcSpec {
    cx: f64,
    cy: f64,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
}

// ── Layer color mapping ─────────────────────────────────────────────────

fn layer_color(index: usize) -> Color {
    const PALETTE: &[(u8, u8, u8)] = &[
        (255, 255, 255), // white
        (255, 80, 80),   // red
        (80, 255, 80),   // green
        (80, 200, 255),  // cyan
        (255, 200, 80),  // yellow
        (200, 120, 255), // purple
        (255, 150, 50),  // orange
    ];
    let (r, g, b) = PALETTE[index % PALETTE.len()];
    Color::from_rgba8(r, g, b, 255)
}

fn color_to_hex(c: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c.red() * 255.0) as u8,
        (c.green() * 255.0) as u8,
        (c.blue() * 255.0) as u8,
    )
}

// ── Public API ──────────────────────────────────────────────────────────

/// Generate a preview PNG + metadata JSON for the project.
pub fn generate_preview(project_dir: &Path) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    // Parse all layer files once
    let layers: Vec<(String, CfFile)> = project
        .layers
        .iter()
        .map(|(name, entry)| {
            let cf = parse_cf(&project_dir.join(&entry.file))
                .with_context(|| format!("Failed to parse layer '{}'", name))?;
            Ok((name.clone(), cf))
        })
        .collect::<Result<_>>()?;

    let bounds = compute_bounds(&layers);
    let mut renderer = Renderer::new(DEFAULT_WIDTH, DEFAULT_HEIGHT, &bounds)?;
    let mut entities: Vec<EntityInfo> = Vec::new();
    let mut layer_infos: Vec<LayerInfo> = Vec::new();

    for (idx, (layer_name, cf)) in layers.iter().enumerate() {
        let color = layer_color(idx);
        let count = render_layer(&mut renderer, cf, layer_name, color, &mut entities);
        layer_infos.push(LayerInfo {
            name: layer_name.clone(),
            entity_count: count,
            color: color_to_hex(color),
        });
    }

    let png_path = project_dir.join("preview.png");
    renderer.save_png(&png_path)?;

    let meta = PreviewMeta {
        project_name: project.project.name,
        image_file: "preview.png".to_string(),
        width_px: DEFAULT_WIDTH,
        height_px: DEFAULT_HEIGHT,
        world_bounds: bounds,
        scale: renderer.scale,
        layers: layer_infos,
        entities,
    };

    let json_path = project_dir.join("preview.meta.json");
    std::fs::write(&json_path, serde_json::to_string_pretty(&meta)?)?;

    println!("✓ Preview: {}", png_path.display());
    println!("✓ Metadata: {}", json_path.display());
    Ok(())
}

// ── Rendering per layer ──────────────────────────────────────────────────

fn render_layer(
    r: &mut Renderer,
    cf: &CfFile,
    layer: &str,
    color: Color,
    out: &mut Vec<EntityInfo>,
) -> usize {
    let mut count = 0;

    for e in &cf.lines {
        r.draw_line(e.from[0], e.from[1], e.to[0], e.to[1], color, STROKE_WIDTH);
        let bounds = points_bounds(&[(e.from[0], e.from[1]), (e.to[0], e.to[1])]);
        out.push(r.entity_info(&e.common, "line", layer, bounds));
        count += 1;
    }

    for e in &cf.polylines {
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        r.draw_polyline(&pts, e.closed, color, STROKE_WIDTH);
        out.push(r.entity_info(&e.common, "polyline", layer, points_bounds(&pts)));
        count += 1;
    }

    for e in &cf.rects {
        let pts = rect_points(e.origin[0], e.origin[1], e.width, e.height);
        r.draw_polyline(&pts, true, color, STROKE_WIDTH);
        out.push(r.entity_info(&e.common, "rect", layer, points_bounds(&pts)));
        count += 1;
    }

    for e in &cf.circles {
        r.draw_circle(e.center[0], e.center[1], e.radius, color, STROKE_WIDTH);
        out.push(r.entity_info(
            &e.common,
            "circle",
            layer,
            circle_bounds(e.center, e.radius),
        ));
        count += 1;
    }

    for e in &cf.arcs {
        r.draw_arc(
            ArcSpec {
                cx: e.center[0],
                cy: e.center[1],
                radius: e.radius,
                start_deg: e.from_angle,
                end_deg: e.to_angle,
            },
            color,
            STROKE_WIDTH,
        );
        out.push(r.entity_info(&e.common, "arc", layer, circle_bounds(e.center, e.radius)));
        count += 1;
    }

    for e in &cf.texts {
        // Render text position as a small marker
        r.draw_line(
            e.position[0] - TEXT_MARKER,
            e.position[1],
            e.position[0] + TEXT_MARKER,
            e.position[1],
            color,
            1.0,
        );
        let mut b = WorldBounds::empty();
        b.add(e.position[0], e.position[1]);
        b.add(e.position[0] + 0.5, e.position[1] + 0.2);
        out.push(r.entity_info(&e.common, "text", layer, b));
        count += 1;
    }

    count
}

// ── Geometry helpers ─────────────────────────────────────────────────────

fn rect_points(x: f64, y: f64, w: f64, h: f64) -> [(f64, f64); 4] {
    [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
}

fn circle_bounds(center: [f64; 2], radius: f64) -> WorldBounds {
    let mut b = WorldBounds::empty();
    b.add(center[0] - radius, center[1] - radius);
    b.add(center[0] + radius, center[1] + radius);
    b
}

fn compute_bounds(layers: &[(String, CfFile)]) -> WorldBounds {
    let mut b = WorldBounds::empty();

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
    }

    if b.is_empty() {
        WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        }
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_accumulates_correctly() {
        let mut b = WorldBounds::empty();
        assert!(b.is_empty());
        b.add(1.0, 2.0);
        b.add(5.0, -1.0);
        assert_eq!(b.as_bbox(), [1.0, -1.0, 5.0, 2.0]);
        assert!(!b.is_empty());
    }

    #[test]
    fn circle_bounds_is_square() {
        let b = circle_bounds([5.0, 5.0], 2.0);
        assert_eq!(b.as_bbox(), [3.0, 3.0, 7.0, 7.0]);
    }

    #[test]
    fn color_to_hex_formats() {
        assert_eq!(color_to_hex(Color::from_rgba8(255, 0, 128, 255)), "#FF0080");
    }
}
