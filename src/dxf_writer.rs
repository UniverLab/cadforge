//! DXF generation module — converts geometric primitives to DXF entities.

use anyhow::Result;
use dxf::entities::{Entity, EntityType, LwPolyline};
use dxf::enums::AcadVersion;
use dxf::tables::Layer;
use dxf::{Color, Drawing, LwPolylineVertex, Point};
use std::path::Path;

/// Style attributes for a line entity.
pub struct LineStyle {
    pub color_index: u8,
    pub lineweight: i16,
}

/// Builder for constructing a DXF drawing from primitives.
pub struct DxfWriter {
    drawing: Drawing,
}

impl DxfWriter {
    pub fn new() -> Self {
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2004;
        Self { drawing }
    }

    /// Add a named layer with an ACI color index (1-255).
    pub fn add_layer(&mut self, name: &str, color_index: u8) {
        let layer = Layer {
            name: name.to_string(),
            color: Color::from_index(color_index),
            ..Default::default()
        };
        self.drawing.add_layer(layer);
    }

    /// Add a line from (x1,y1) to (x2,y2).
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, layer: &str) {
        let line = dxf::entities::Line::new(Point::new(x1, y1, 0.0), Point::new(x2, y2, 0.0));
        let mut entity = Entity::new(EntityType::Line(line));
        entity.common.layer = layer.to_string();
        self.drawing.add_entity(entity);
    }

    /// Add a line with color and lineweight.
    pub fn line_styled(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        layer: &str,
        style: &LineStyle,
    ) {
        let line = dxf::entities::Line::new(Point::new(x1, y1, 0.0), Point::new(x2, y2, 0.0));
        let mut entity = Entity::new(EntityType::Line(line));
        entity.common.layer = layer.to_string();
        entity.common.color = Color::from_index(style.color_index);
        entity.common.lineweight_enum_value = style.lineweight;
        self.drawing.add_entity(entity);
    }

    /// Add a circle at (cx, cy) with given radius.
    pub fn circle(&mut self, cx: f64, cy: f64, radius: f64, layer: &str) {
        let circle = dxf::entities::Circle {
            center: Point::new(cx, cy, 0.0),
            radius,
            ..Default::default()
        };
        let mut entity = Entity::new(EntityType::Circle(circle));
        entity.common.layer = layer.to_string();
        self.drawing.add_entity(entity);
    }

    /// Add an arc at (cx, cy) with radius, from start_angle to end_angle (degrees).
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        layer: &str,
    ) {
        let arc = dxf::entities::Arc {
            center: Point::new(cx, cy, 0.0),
            radius,
            start_angle,
            end_angle,
            ..Default::default()
        };
        let mut entity = Entity::new(EntityType::Arc(arc));
        entity.common.layer = layer.to_string();
        self.drawing.add_entity(entity);
    }

    /// Add a lightweight polyline from a list of (x, y) points.
    pub fn polyline(&mut self, points: &[(f64, f64)], closed: bool, layer: &str) {
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
        let mut entity = Entity::new(EntityType::LwPolyline(poly));
        entity.common.layer = layer.to_string();
        self.drawing.add_entity(entity);
    }

    /// Add a rectangle (as a closed polyline) from origin (x, y) with width and height.
    pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, layer: &str) {
        let points = [
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
        ];
        self.polyline(&points, true, layer);
    }

    /// Add a text entity at (x, y) with given height.
    pub fn text(&mut self, x: f64, y: f64, height: f64, content: &str, layer: &str) {
        let text = dxf::entities::Text {
            location: Point::new(x, y, 0.0),
            text_height: height,
            value: content.to_string(),
            ..Default::default()
        };
        let mut entity = Entity::new(EntityType::Text(text));
        entity.common.layer = layer.to_string();
        self.drawing.add_entity(entity);
    }

    /// Add a point entity at (x, y).
    pub fn point(&mut self, x: f64, y: f64, layer: &str) {
        let pt = dxf::entities::ModelPoint {
            location: Point::new(x, y, 0.0),
            ..Default::default()
        };
        let mut entity = Entity::new(EntityType::ModelPoint(pt));
        entity.common.layer = layer.to_string();
        self.drawing.add_entity(entity);
    }

    /// Save the drawing to a DXF file.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.drawing
            .save_file(path.to_str().unwrap_or("output.dxf"))?;
        Ok(())
    }
}

impl Default for DxfWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn generates_basic_dxf() {
        let mut w = DxfWriter::new();
        w.add_layer("MUROS", 7);
        w.line(0.0, 0.0, 10.0, 0.0, "MUROS");
        w.circle(5.0, 5.0, 2.0, "MUROS");
        w.rect(1.0, 1.0, 3.0, 4.0, "MUROS");

        let path = PathBuf::from("/tmp/cadforge_test_basic.dxf");
        w.save(&path).unwrap();
        assert!(path.exists());
    }
}
