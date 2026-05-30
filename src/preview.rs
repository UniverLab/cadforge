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

fn bg_color() -> Color {
    Color::from_rgba8(20, 20, 20, 255)
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
pub struct WorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
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
    // World → pixel transform
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    world_height: f64,
}

impl Renderer {
    fn new(width: u32, height: u32, bounds: &WorldBounds) -> Self {
        let world_w = bounds.max_x - bounds.min_x + 2.0 * PADDING;
        let world_h = bounds.max_y - bounds.min_y + 2.0 * PADDING;

        let scale_x = width as f64 / world_w;
        let scale_y = height as f64 / world_h;
        let scale = scale_x.min(scale_y);

        let offset_x = bounds.min_x - PADDING;
        let offset_y = bounds.min_y - PADDING;

        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(bg_color());

        Self {
            pixmap,
            scale,
            offset_x,
            offset_y,
            world_height: world_h,
        }
    }

    fn world_to_px(&self, x: f64, y: f64) -> (f32, f32) {
        let px = ((x - self.offset_x) * self.scale) as f32;
        // Flip Y: world Y goes up, pixel Y goes down
        let py = ((self.world_height - (y - self.offset_y)) * self.scale) as f32;
        (px, py)
    }

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: Color, width: f32) {
        let (px1, py1) = self.world_to_px(x1, y1);
        let (px2, py2) = self.world_to_px(x2, y2);

        let mut pb = PathBuilder::new();
        pb.move_to(px1, py1);
        pb.line_to(px2, py2);
        let path = match pb.finish() {
            Some(p) => p,
            None => return,
        };

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

    fn draw_circle(&mut self, cx: f64, cy: f64, radius: f64, color: Color, width: f32) {
        let (pcx, pcy) = self.world_to_px(cx, cy);
        let pr = (radius * self.scale) as f32;

        let mut pb = PathBuilder::new();
        // Approximate circle with 4 cubic beziers
        let k: f32 = 0.552_284_8; // magic number for cubic bezier circle
        let kr = pr * k;
        pb.move_to(pcx + pr, pcy);
        pb.cubic_to(pcx + pr, pcy - kr, pcx + kr, pcy - pr, pcx, pcy - pr);
        pb.cubic_to(pcx - kr, pcy - pr, pcx - pr, pcy - kr, pcx - pr, pcy);
        pb.cubic_to(pcx - pr, pcy + kr, pcx - kr, pcy + pr, pcx, pcy + pr);
        pb.cubic_to(pcx + kr, pcy + pr, pcx + pr, pcy + kr, pcx + pr, pcy);
        pb.close();
        let path = match pb.finish() {
            Some(p) => p,
            None => return,
        };

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

    #[allow(clippy::too_many_arguments)]
    fn draw_arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
        color: Color,
        width: f32,
    ) {
        let steps = 32;
        let start_rad = start_deg.to_radians();
        let end_rad = end_deg.to_radians();
        let delta = (end_rad - start_rad) / steps as f64;

        let mut pb = PathBuilder::new();
        for i in 0..=steps {
            let angle = start_rad + delta * i as f64;
            let x = cx + radius * angle.cos();
            let y = cy + radius * angle.sin();
            let (px, py) = self.world_to_px(x, y);
            if i == 0 {
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        let path = match pb.finish() {
            Some(p) => p,
            None => return,
        };

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

    fn draw_polyline(&mut self, points: &[(f64, f64)], closed: bool, color: Color, width: f32) {
        if points.is_empty() {
            return;
        }
        let mut pb = PathBuilder::new();
        let (px, py) = self.world_to_px(points[0].0, points[0].1);
        pb.move_to(px, py);
        for &(x, y) in &points[1..] {
            let (px, py) = self.world_to_px(x, y);
            pb.line_to(px, py);
        }
        if closed {
            pb.close();
        }
        let path = match pb.finish() {
            Some(p) => p,
            None => return,
        };

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

    fn save_png(&self, path: &Path) -> Result<()> {
        self.pixmap
            .save_png(path)
            .map_err(|e| anyhow::anyhow!("Failed to save PNG: {}", e))
    }
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

// ── Public API ──────────────────────────────────────────────────────────

/// Generate a preview PNG + metadata JSON for the project.
pub fn generate_preview(project_dir: &Path) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    // Collect all entities and compute world bounds
    let mut all_entities: Vec<(String, usize, CfFile)> = Vec::new();
    for (i, (name, entry)) in project.layers.iter().enumerate() {
        let cf_path = project_dir.join(&entry.file);
        let cf = parse_cf(&cf_path).with_context(|| format!("Failed to parse layer '{}'", name))?;
        all_entities.push((name.clone(), i, cf));
    }

    let bounds = compute_bounds(&all_entities);

    let mut renderer = Renderer::new(DEFAULT_WIDTH, DEFAULT_HEIGHT, &bounds);
    let mut meta_entities: Vec<EntityInfo> = Vec::new();
    let mut layer_infos: Vec<LayerInfo> = Vec::new();

    for (layer_name, layer_idx, cf) in &all_entities {
        let color = layer_color(*layer_idx);
        let stroke_w = 1.5_f32;

        let mut count = 0;

        for e in &cf.lines {
            renderer.draw_line(e.from[0], e.from[1], e.to[0], e.to[1], color, stroke_w);
            meta_entities.push(entity_info(
                &e.common,
                "line",
                layer_name,
                line_bbox(e.from[0], e.from[1], e.to[0], e.to[1]),
                &renderer,
            ));
            count += 1;
        }

        for e in &cf.polylines {
            let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
            renderer.draw_polyline(&pts, e.closed, color, stroke_w);
            let bb = poly_bbox(&pts);
            meta_entities.push(entity_info(
                &e.common, "polyline", layer_name, bb, &renderer,
            ));
            count += 1;
        }

        for e in &cf.rects {
            let pts = [
                (e.origin[0], e.origin[1]),
                (e.origin[0] + e.width, e.origin[1]),
                (e.origin[0] + e.width, e.origin[1] + e.height),
                (e.origin[0], e.origin[1] + e.height),
            ];
            renderer.draw_polyline(&pts, true, color, stroke_w);
            meta_entities.push(entity_info(
                &e.common,
                "rect",
                layer_name,
                [
                    e.origin[0],
                    e.origin[1],
                    e.origin[0] + e.width,
                    e.origin[1] + e.height,
                ],
                &renderer,
            ));
            count += 1;
        }

        for e in &cf.circles {
            renderer.draw_circle(e.center[0], e.center[1], e.radius, color, stroke_w);
            meta_entities.push(entity_info(
                &e.common,
                "circle",
                layer_name,
                [
                    e.center[0] - e.radius,
                    e.center[1] - e.radius,
                    e.center[0] + e.radius,
                    e.center[1] + e.radius,
                ],
                &renderer,
            ));
            count += 1;
        }

        for e in &cf.arcs {
            renderer.draw_arc(
                e.center[0],
                e.center[1],
                e.radius,
                e.from_angle,
                e.to_angle,
                color,
                stroke_w,
            );
            meta_entities.push(entity_info(
                &e.common,
                "arc",
                layer_name,
                [
                    e.center[0] - e.radius,
                    e.center[1] - e.radius,
                    e.center[0] + e.radius,
                    e.center[1] + e.radius,
                ],
                &renderer,
            ));
            count += 1;
        }

        for e in &cf.texts {
            // Text rendered as a small marker
            let (px, py) = renderer.world_to_px(e.position[0], e.position[1]);
            renderer.draw_line(
                e.position[0] - 0.05,
                e.position[1],
                e.position[0] + 0.05,
                e.position[1],
                color,
                1.0,
            );
            let _ = px + py; // suppress unused
            meta_entities.push(entity_info(
                &e.common,
                "text",
                layer_name,
                [
                    e.position[0],
                    e.position[1],
                    e.position[0] + 0.5,
                    e.position[1] + 0.2,
                ],
                &renderer,
            ));
            count += 1;
        }

        let layer_color_hex = format!(
            "#{:02X}{:02X}{:02X}",
            (color.red() * 255.0) as u8,
            (color.green() * 255.0) as u8,
            (color.blue() * 255.0) as u8,
        );
        layer_infos.push(LayerInfo {
            name: layer_name.clone(),
            entity_count: count,
            color: layer_color_hex,
        });
    }

    // Save PNG
    let png_path = project_dir.join("preview.png");
    renderer.save_png(&png_path)?;

    // Save metadata JSON
    let meta = PreviewMeta {
        project_name: project.project.name,
        image_file: "preview.png".to_string(),
        width_px: DEFAULT_WIDTH,
        height_px: DEFAULT_HEIGHT,
        world_bounds: bounds,
        scale: renderer.scale,
        layers: layer_infos,
        entities: meta_entities,
    };

    let json_path = project_dir.join("preview.meta.json");
    let json = serde_json::to_string_pretty(&meta)?;
    std::fs::write(&json_path, json)?;

    println!("✓ Preview: {}", png_path.display());
    println!("✓ Metadata: {}", json_path.display());
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn compute_bounds(layers: &[(String, usize, CfFile)]) -> WorldBounds {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for (_, _, cf) in layers {
        for e in &cf.lines {
            expand(
                &mut min_x, &mut min_y, &mut max_x, &mut max_y, e.from[0], e.from[1],
            );
            expand(
                &mut min_x, &mut min_y, &mut max_x, &mut max_y, e.to[0], e.to[1],
            );
        }
        for e in &cf.polylines {
            for p in &e.points {
                expand(&mut min_x, &mut min_y, &mut max_x, &mut max_y, p[0], p[1]);
            }
        }
        for e in &cf.rects {
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.origin[0],
                e.origin[1],
            );
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.origin[0] + e.width,
                e.origin[1] + e.height,
            );
        }
        for e in &cf.circles {
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.center[0] - e.radius,
                e.center[1] - e.radius,
            );
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.center[0] + e.radius,
                e.center[1] + e.radius,
            );
        }
        for e in &cf.arcs {
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.center[0] - e.radius,
                e.center[1] - e.radius,
            );
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.center[0] + e.radius,
                e.center[1] + e.radius,
            );
        }
        for e in &cf.texts {
            expand(
                &mut min_x,
                &mut min_y,
                &mut max_x,
                &mut max_y,
                e.position[0],
                e.position[1],
            );
        }
    }

    if min_x == f64::MAX {
        return WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        };
    }

    WorldBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

fn expand(min_x: &mut f64, min_y: &mut f64, max_x: &mut f64, max_y: &mut f64, x: f64, y: f64) {
    *min_x = min_x.min(x);
    *min_y = min_y.min(y);
    *max_x = max_x.max(x);
    *max_y = max_y.max(y);
}

fn entity_info(
    common: &CommonAttrs,
    entity_type: &str,
    layer: &str,
    bbox: [f64; 4],
    renderer: &Renderer,
) -> EntityInfo {
    let (px1, py1) = renderer.world_to_px(bbox[0], bbox[3]); // top-left
    let (px2, py2) = renderer.world_to_px(bbox[2], bbox[1]); // bottom-right
    EntityInfo {
        id: common.id.clone(),
        entity_type: entity_type.to_string(),
        layer: layer.to_string(),
        bbox,
        pixel_bbox: [
            px1 as u32,
            py1 as u32,
            (px2 - px1) as u32,
            (py2 - py1) as u32,
        ],
    }
}

fn line_bbox(x1: f64, y1: f64, x2: f64, y2: f64) -> [f64; 4] {
    [x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)]
}

fn poly_bbox(pts: &[(f64, f64)]) -> [f64; 4] {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for &(x, y) in pts {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x, max_y]
}
