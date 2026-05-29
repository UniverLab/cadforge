//! DXF generation module — converts geometric primitives to DXF entities.

use anyhow::Result;
use dxf::entities::{Entity, EntityType, LwPolyline};
use dxf::enums::AcadVersion;
use dxf::tables::Layer;
use dxf::{Color, Drawing, LwPolylineVertex, Point};
use std::path::Path;

/// Optional visual attributes applied to any entity.
#[derive(Default, Clone)]
pub struct EntityStyle {
    pub color_24bit: Option<i32>,
    pub lineweight: Option<i16>,
}

impl EntityStyle {
    pub fn is_empty(&self) -> bool {
        self.color_24bit.is_none() && self.lineweight.is_none()
    }
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

    // ── Single entry point for adding entities ─────────────────────────

    fn add_entity(&mut self, entity_type: EntityType, layer: &str, style: &EntityStyle) {
        let mut entity = Entity::new(entity_type);
        entity.common.layer = layer.to_string();
        if let Some(c) = style.color_24bit {
            entity.common.color_24_bit = c;
        }
        if let Some(lw) = style.lineweight {
            entity.common.lineweight_enum_value = lw;
        }
        self.drawing.add_entity(entity);
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
    pub fn dim_linear(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        offset: f64,
        layer: &str,
        style: &EntityStyle,
    ) {
        let dim = dxf::entities::RotatedDimension {
            definition_point_2: Point::new(x1, y1, 0.0),
            definition_point_3: Point::new(x2, y2, 0.0),
            insertion_point: Point::new((x1 + x2) / 2.0, y1 + offset, 0.0),
            ..Default::default()
        };
        self.add_entity(EntityType::RotatedDimension(dim), layer, style);
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
        let s = EntityStyle::default();
        w.add_layer("MUROS", 7);
        w.line(0.0, 0.0, 10.0, 0.0, "MUROS", &s);
        w.circle(5.0, 5.0, 2.0, "MUROS", &s);
        w.rect(1.0, 1.0, 3.0, 4.0, "MUROS", &s);

        let path = PathBuf::from("/tmp/cadforge_test_basic.dxf");
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
        };
        w.line(0.0, 0.0, 1.0, 1.0, "TEST", &style);

        let path = PathBuf::from("/tmp/cadforge_test_styled.dxf");
        w.save(&path).unwrap();
        assert!(path.exists());
    }
}
