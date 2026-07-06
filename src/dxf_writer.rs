//! DXF generation module — converts geometric primitives to DXF entities.

use anyhow::Result;
use dxf::entities::{Entity, EntityType, LwPolyline};
use dxf::enums::AcadVersion;
use dxf::tables::{AppId, Layer};
use dxf::{Color, Drawing, LwPolylineVertex, Point, XData, XDataItem};
use std::path::Path;

/// XDATA application name stamped on the pattern lines a hatch expands into, so
/// the DXF importer can recognize and re-fuse them back into a single `[[hatch]]`
/// (instead of dozens of stray `[[line]]`s). Only lines this tool emits carry
/// the tag, so import never mistakes a foreign DXF's lines for a hatch.
pub const HATCH_XDATA_APP: &str = "CADSPEC_HATCH";

/// Optional visual attributes applied to any entity.
#[derive(Default, Clone)]
pub struct EntityStyle {
    pub color_24bit: Option<i32>,
    pub lineweight: Option<i16>,
    pub line_type: Option<String>,
}

impl EntityStyle {
    pub fn is_empty(&self) -> bool {
        self.color_24bit.is_none() && self.lineweight.is_none() && self.line_type.is_none()
    }
}

/// Builder for constructing a DXF drawing from primitives.
pub struct DxfWriter {
    drawing: Drawing,
    /// Monotonic id assigned to each hatch so its expanded pattern lines can be
    /// grouped back together on import.
    hatch_group_seq: i32,
}

impl DxfWriter {
    pub fn new() -> Self {
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2004;

        // Register standard line types
        drawing.add_line_type(Self::make_line_type("DASHED", &[0.5, -0.25]));
        drawing.add_line_type(Self::make_line_type("DOTTED", &[0.0, -0.25]));
        drawing.add_line_type(Self::make_line_type("DASHDOT", &[0.5, -0.25, 0.0, -0.25]));

        // Register the APPID our hatch pattern lines stamp XDATA under, so
        // strict readers don't have to fall back on undeclared-app tolerance.
        drawing.add_app_id(AppId {
            name: HATCH_XDATA_APP.to_string(),
            ..Default::default()
        });

        Self {
            drawing,
            hatch_group_seq: 0,
        }
    }

    fn make_line_type(name: &str, pattern: &[f64]) -> dxf::tables::LineType {
        let mut lt = dxf::tables::LineType {
            name: name.to_string(),
            ..Default::default()
        };
        lt.element_count = pattern.len() as i32;
        lt.total_pattern_length = pattern.iter().map(|v| v.abs()).sum();
        lt.dash_dot_space_lengths = pattern.to_vec();
        lt
    }

    /// Add a named layer with an ACI color index (1-255).
    /// Re-adding an existing layer updates its color instead of duplicating
    /// the LAYER table record.
    pub fn add_layer(&mut self, name: &str, color_index: u8) {
        if let Some(existing) = self.drawing.layers_mut().find(|l| l.name == name) {
            existing.color = Color::from_index(color_index);
            return;
        }
        let layer = Layer {
            name: name.to_string(),
            color: Color::from_index(color_index),
            ..Default::default()
        };
        self.drawing.add_layer(layer);
    }

    // ── Single entry point for adding entities ─────────────────────────

    /// Build a styled entity (layer + color/weight/line-type) without adding it.
    fn styled_entity(entity_type: EntityType, layer: &str, style: &EntityStyle) -> Entity {
        let mut entity = Entity::new(entity_type);
        entity.common.layer = layer.to_string();
        if let Some(c) = style.color_24bit {
            entity.common.color_24_bit = c;
        }
        if let Some(lw) = style.lineweight {
            entity.common.lineweight_enum_value = lw;
        }
        if let Some(lt) = &style.line_type {
            entity.common.line_type_name = lt.clone();
        }
        entity
    }

    fn add_entity(&mut self, entity_type: EntityType, layer: &str, style: &EntityStyle) {
        self.drawing
            .add_entity(Self::styled_entity(entity_type, layer, style));
    }

    // ── Public primitive methods ───────────────────────────────────────

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, layer: &str, style: &EntityStyle) {
        let line = dxf::entities::Line::new(Point::new(x1, y1, 0.0), Point::new(x2, y2, 0.0));
        self.add_entity(EntityType::Line(line), layer, style);
    }

    pub fn circle(&mut self, cx: f64, cy: f64, radius: f64, layer: &str, style: &EntityStyle) {
        let circle = dxf::entities::Circle {
            center: Point::new(cx, cy, 0.0),
            radius,
            ..Default::default()
        };
        self.add_entity(EntityType::Circle(circle), layer, style);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        layer: &str,
        style: &EntityStyle,
    ) {
        let arc = dxf::entities::Arc {
            center: Point::new(cx, cy, 0.0),
            radius,
            start_angle,
            end_angle,
            ..Default::default()
        };
        self.add_entity(EntityType::Arc(arc), layer, style);
    }

    pub fn polyline(
        &mut self,
        points: &[(f64, f64)],
        closed: bool,
        layer: &str,
        style: &EntityStyle,
    ) {
        let mut poly = LwPolyline {
            flags: i32::from(closed),
            ..Default::default()
        };
        for &(x, y) in points {
            poly.vertices.push(LwPolylineVertex {
                x,
                y,
                ..Default::default()
            });
        }
        self.add_entity(EntityType::LwPolyline(poly), layer, style);
    }

    pub fn rect(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        layer: &str,
        style: &EntityStyle,
    ) {
        let points = [
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
        ];
        self.polyline(&points, true, layer, style);
    }

    pub fn text(
        &mut self,
        x: f64,
        y: f64,
        height: f64,
        content: &str,
        layer: &str,
        style: &EntityStyle,
    ) {
        let text = dxf::entities::Text {
            location: Point::new(x, y, 0.0),
            text_height: height,
            value: content.to_string(),
            ..Default::default()
        };
        self.add_entity(EntityType::Text(text), layer, style);
    }

    pub fn point(&mut self, x: f64, y: f64, layer: &str, style: &EntityStyle) {
        let pt = dxf::entities::ModelPoint {
            location: Point::new(x, y, 0.0),
            ..Default::default()
        };
        self.add_entity(EntityType::ModelPoint(pt), layer, style);
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn dim_linear(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        offset: f64,
        label: &str,
        text_height: f64,
        layer: &str,
        style: &EntityStyle,
    ) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            return;
        }
        // Offset along the normal, matching the preview renderer
        let (nx, ny) = (-dy / len, dx / len);
        let (ax, ay) = (x1 + nx * offset, y1 + ny * offset);
        let (bx, by) = (x2 + nx * offset, y2 + ny * offset);

        let dim = dxf::entities::RotatedDimension {
            definition_point_2: Point::new(x1, y1, 0.0),
            definition_point_3: Point::new(x2, y2, 0.0),
            insertion_point: Point::new((ax + bx) / 2.0, (ay + by) / 2.0, 0.0),
            rotation_angle: dy.atan2(dx).to_degrees(),
            ..Default::default()
        };
        self.add_entity(EntityType::RotatedDimension(dim), layer, style);

        // Also emit dimension lines and text as explicit entities for compatibility
        // Extension lines
        self.line(x1, y1, ax, ay, layer, style);
        self.line(x2, y2, bx, by, layer, style);
        // Dimension line
        self.line(ax, ay, bx, by, layer, style);
        // Dimension text, raised half its height off the dimension line
        let text_style = EntityStyle::default();
        self.text(
            (ax + bx) / 2.0 + nx * text_height * 0.5,
            (ay + by) / 2.0 + ny * text_height * 0.5,
            text_height,
            label,
            layer,
            &text_style,
        );
    }

    /// Save the drawing to a DXF file.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.drawing
            .save_file(path.to_str().unwrap_or("output.dxf"))?;
        Ok(())
    }

    /// Fill a polygon with solid color using DXF Solid entities (fan triangulation).
    pub fn solid_fill(&mut self, points: &[(f64, f64)], layer: &str, style: &EntityStyle) {
        if points.len() < 3 {
            return;
        }
        // Fan triangulation from first vertex
        let (ax, ay) = points[0];
        for i in 1..points.len() - 1 {
            let (bx, by) = points[i];
            let (cx, cy) = points[i + 1];
            let solid = dxf::entities::Solid::new(
                Point::new(ax, ay, 0.0),
                Point::new(bx, by, 0.0),
                Point::new(cx, cy, 0.0),
                Point::new(cx, cy, 0.0), // 4th = 3rd for triangle
            );
            self.add_entity(EntityType::Solid(solid), layer, style);
        }
    }

    /// Generate hatch pattern lines within a boundary polygon.
    /// `boundary` is a list of (x,y) points forming a closed polygon.
    /// `angle` is in degrees, `spacing` is distance between lines.
    ///
    /// `scale` and `pattern` are the source `[[hatch]]` values, carried on each
    /// generated line as XDATA (alongside the boundary and a group id) so the
    /// importer can re-fuse the lines into one `[[hatch]]` on a DXF round-trip.
    #[allow(clippy::too_many_arguments)]
    pub fn hatch(
        &mut self,
        boundary: &[(f64, f64)],
        angle: f64,
        spacing: f64,
        scale: f64,
        pattern: &str,
        layer: &str,
        style: &EntityStyle,
    ) {
        if boundary.len() < 3 {
            return;
        }

        // XDATA stamped on every pattern line of this hatch: group id, source
        // angle/scale/pattern, then the boundary polygon vertices. All lines of
        // one hatch share the same payload, so any of them can rebuild it.
        self.hatch_group_seq += 1;
        let mut items = vec![
            XDataItem::Long(self.hatch_group_seq),
            XDataItem::Real(angle),
            XDataItem::Real(scale),
            XDataItem::Str(pattern.to_string()),
        ];
        for &(x, y) in boundary {
            items.push(XDataItem::WorldSpacePosition(Point::new(x, y, 0.0)));
        }
        let xdata = XData {
            application_name: HATCH_XDATA_APP.to_string(),
            items,
        };

        // Compute bounding box
        let (min_x, max_x, min_y, max_y) = bounding_box(boundary);

        // Generate parallel lines at the given angle that cover the bbox
        let rad = angle.to_radians();
        let cos_a = rad.cos();
        let sin_a = rad.sin();

        // Diagonal of bbox determines how many lines we need
        let diag = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt();
        let cx = (min_x + max_x) / 2.0;
        let cy = (min_y + max_y) / 2.0;

        let num_lines = (diag / spacing).ceil() as i32;

        for i in -num_lines..=num_lines {
            let offset = i as f64 * spacing;
            // Line perpendicular offset from center
            let px = cx + offset * sin_a;
            let py = cy - offset * cos_a;
            // Line endpoints extending beyond bbox
            let x1 = px - diag * cos_a;
            let y1 = py - diag * sin_a;
            let x2 = px + diag * cos_a;
            let y2 = py + diag * sin_a;

            // Clip line to boundary polygon
            if let Some((cx1, cy1, cx2, cy2)) = clip_line_to_polygon(x1, y1, x2, y2, boundary) {
                let line =
                    dxf::entities::Line::new(Point::new(cx1, cy1, 0.0), Point::new(cx2, cy2, 0.0));
                let mut entity = Self::styled_entity(EntityType::Line(line), layer, style);
                entity.common.x_data.push(xdata.clone());
                self.drawing.add_entity(entity);
            }
        }
    }
}

impl Default for DxfWriter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Geometry helpers for hatch ──────────────────────────────────────────

fn bounding_box(pts: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for &(x, y) in pts {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    (min_x, max_x, min_y, max_y)
}

/// Clip a line segment to a convex/concave polygon using Cyrus-Beck-like approach.
/// Returns the clipped segment or None if fully outside.
fn clip_line_to_polygon(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    polygon: &[(f64, f64)],
) -> Option<(f64, f64, f64, f64)> {
    // Find all intersections of the line with polygon edges
    let mut params: Vec<f64> = Vec::new();
    let n = polygon.len();

    for i in 0..n {
        let j = (i + 1) % n;
        let (ex1, ey1) = polygon[i];
        let (ex2, ey2) = polygon[j];

        if let Some(t) = line_segment_intersection(x1, y1, x2, y2, ex1, ey1, ex2, ey2) {
            params.push(t);
        }
    }

    if params.len() < 2 {
        return None;
    }

    params.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let t_min = params[0];
    let t_max = params[params.len() - 1];

    let dx = x2 - x1;
    let dy = y2 - y1;

    Some((
        x1 + t_min * dx,
        y1 + t_min * dy,
        x1 + t_max * dx,
        y1 + t_max * dy,
    ))
}

/// Find parameter t where line (x1,y1)→(x2,y2) intersects segment (ex1,ey1)→(ex2,ey2).
#[allow(clippy::too_many_arguments)]
fn line_segment_intersection(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    ex1: f64,
    ey1: f64,
    ex2: f64,
    ey2: f64,
) -> Option<f64> {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let edx = ex2 - ex1;
    let edy = ey2 - ey1;

    let denom = dx * edy - dy * edx;
    if denom.abs() < 1e-10 {
        return None; // parallel
    }

    let t = ((ex1 - x1) * edy - (ey1 - y1) * edx) / denom;
    let u = ((ex1 - x1) * dy - (ey1 - y1) * dx) / denom;

    if (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn generates_basic_dxf() {
        let mut w = DxfWriter::new();
        let s = EntityStyle::default();
        w.add_layer("MUROS", 7);
        w.line(0.0, 0.0, 10.0, 0.0, "MUROS", &s);
        w.circle(5.0, 5.0, 2.0, "MUROS", &s);
        w.rect(1.0, 1.0, 3.0, 4.0, "MUROS", &s);

        let path = PathBuf::from("/tmp/cadspec_test_basic.dxf");
        w.save(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn entity_style_applies_color_and_weight() {
        let mut w = DxfWriter::new();
        w.add_layer("TEST", 7);
        let style = EntityStyle {
            color_24bit: Some(0xFF0000),
            lineweight: Some(50),
            line_type: None,
        };
        w.line(0.0, 0.0, 1.0, 1.0, "TEST", &style);

        let path = PathBuf::from("/tmp/cadspec_test_styled.dxf");
        w.save(&path).unwrap();
        assert!(path.exists());
    }
}
