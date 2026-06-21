//! 3D view renderer — extrudes `.cf` geometry into solids and projects them.
//!
//! cadspec geometry is declared in 2D plan coordinates. Any primitive can
//! carry an `extrude` height (and optional `elevation`): a closed shape becomes
//! a solid prism, a line or open polyline becomes a vertical wall. This module
//! projects that geometry with a fixed axonometric (isometric) camera, sorts
//! the faces back-to-front (painter's algorithm) and shades them, producing a
//! single self-contained SVG — the same backend story as the 2D renderer, so
//! it rasterizes to PNG and streams to the live viewer identically.

use crate::mesh;
use crate::model::{CfFile, CommonAttrs};
use crate::svg::layer_display_color;
use std::fmt::Write as _;

const BG_COLOR: &str = "#141414";
const PADDING_PX: f64 = 48.0;
const CIRCLE_SEGMENTS: usize = 40;

/// A rendered 3D SVG plus its pixel size (for the PNG rasterizer).
pub struct Render3d {
    pub svg: String,
    pub width_px: f64,
    pub height_px: f64,
}

/// One projected, shaded polygon ready to emit, with its camera depth.
struct Face {
    /// Projected screen-space points (pre-fit): (horizontal, vertical-up).
    proj: Vec<(f64, f64)>,
    fill: Option<String>,
    stroke: String,
    stroke_w: f64,
    /// Larger = closer to the camera (drawn later, on top).
    depth: f64,
    layer: String,
    id: Option<String>,
}

/// An orthographic/axonometric camera: screen-horizontal (`right`), screen-up
/// (`up`) and the viewing direction (`view`, pointing from the scene toward the
/// camera — used for depth sorting and back-face culling). Project a world
/// point by dotting it against `right`/`up`.
#[derive(Clone, Copy)]
pub struct Camera {
    right: V3,
    up: V3,
    view: V3,
}

impl Camera {
    /// Standard 2:1 isometric (the default 3D look).
    pub fn iso() -> Camera {
        const C: f64 = 0.866_025_403_784_438_6;
        const S: f64 = 0.5;
        Camera {
            right: [C, -C, 0.0],
            up: [-S, -S, 1.0],
            view: [0.577_350_27, 0.577_350_27, 0.577_350_27],
        }
    }
    /// Looking straight down (-Z): a plan / planta.
    pub fn top() -> Camera {
        Camera {
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            view: [0.0, 0.0, 1.0],
        }
    }
    pub fn front() -> Camera {
        Camera {
            right: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            view: [0.0, -1.0, 0.0],
        }
    }
    pub fn back() -> Camera {
        Camera {
            right: [-1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            view: [0.0, 1.0, 0.0],
        }
    }
    pub fn right_side() -> Camera {
        Camera {
            right: [0.0, -1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            view: [1.0, 0.0, 0.0],
        }
    }
    pub fn left_side() -> Camera {
        Camera {
            right: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            view: [-1.0, 0.0, 0.0],
        }
    }
    fn project(&self, p: V3) -> (f64, f64) {
        (v_dot(p, self.right), v_dot(p, self.up))
    }
    fn depth(&self, p: V3) -> f64 {
        v_dot(p, self.view)
    }
}

/// A section cut: keep one half-space of the model, revealing the interior at
/// the cut plane. `axis` is 0=x, 1=y, 2=z; `keep_max` keeps the side where the
/// coordinate is ≥ `at` (otherwise ≤ `at`).
#[derive(Clone, Copy)]
pub struct Cut {
    pub axis: usize,
    pub at: f64,
    pub keep_max: bool,
}

impl Cut {
    /// A huge box covering the discarded half-space (to subtract from solids).
    fn discard_box(&self) -> mesh::Mesh {
        const BIG: f64 = 1.0e5;
        let mut at = [-BIG, -BIG, -BIG];
        let mut size = [2.0 * BIG, 2.0 * BIG, 2.0 * BIG];
        if self.keep_max {
            at[self.axis] = -BIG; // discard coord < at
            size[self.axis] = self.at + BIG;
        } else {
            at[self.axis] = self.at; // discard coord > at
            size[self.axis] = BIG;
        }
        mesh::box_solid(at, size)
    }
}

/// Render already-parsed layers to an isometric 3D [`Render3d`].
pub fn render_scene_3d(layers: &[(String, CfFile)], width: u32) -> Render3d {
    render_view(layers, width, &Camera::iso(), None)
}

/// Render already-parsed layers through an arbitrary [`Camera`], optionally
/// sectioned by `cut`.
pub fn render_view(
    layers: &[(String, CfFile)],
    width: u32,
    cam: &Camera,
    cut: Option<&Cut>,
) -> Render3d {
    let mut faces: Vec<Face> = Vec::new();

    for (idx, (layer_name, cf)) in layers.iter().enumerate() {
        let visible = cf.layer_meta.as_ref().map(|m| m.visible).unwrap_or(true);
        if !visible {
            continue;
        }
        let layer_color = layer_display_color(cf, idx);
        collect_layer_faces(&mut faces, layer_name, cf, &layer_color, cam, cut);
    }

    // Project bounds across every face to size the canvas.
    let (mut min_h, mut min_v, mut max_h, mut max_v) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for f in &faces {
        for &(h, v) in &f.proj {
            min_h = min_h.min(h);
            min_v = min_v.min(v);
            max_h = max_h.max(h);
            max_v = max_v.max(v);
        }
    }
    if faces.is_empty() {
        min_h = 0.0;
        min_v = 0.0;
        max_h = 10.0;
        max_v = 10.0;
    }

    let span_h = (max_h - min_h).max(1e-6);
    let span_v = (max_v - min_v).max(1e-6);
    let width_px = width as f64;
    let scale = (width_px - 2.0 * PADDING_PX).max(1.0) / span_h;
    let height_px = span_v * scale + 2.0 * PADDING_PX;

    // World→screen: flip vertical (SVG y grows down) so up is up.
    let to_px = |(h, v): (f64, f64)| -> (f64, f64) {
        (
            (h - min_h) * scale + PADDING_PX,
            (max_v - v) * scale + PADDING_PX,
        )
    };

    // Painter's algorithm: farthest first.
    faces.sort_by(|a, b| {
        a.depth
            .partial_cmp(&b.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = String::with_capacity(16 * 1024 + faces.len() * 96);
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" data-view="3d">"#,
        w = width_px,
        h = height_px,
    );
    let _ = write!(
        out,
        r#"<rect width="100%" height="100%" fill="{}"/>"#,
        BG_COLOR
    );

    for f in &faces {
        let pts: String = f
            .proj
            .iter()
            .map(|&p| {
                let (x, y) = to_px(p);
                format!("{x:.2},{y:.2}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        let layer_attr = format!(r#" data-layer="{}""#, xml_escape(&f.layer));
        let id_attr = match &f.id {
            Some(id) => format!(r#" data-id="{}""#, xml_escape(id)),
            None => String::new(),
        };
        match &f.fill {
            Some(fill) => {
                let _ = write!(
                    out,
                    r#"<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="{sw:.2}" stroke-linejoin="round"{layer_attr}{id_attr}/>"#,
                    stroke = f.stroke,
                    sw = f.stroke_w,
                );
            }
            None => {
                let _ = write!(
                    out,
                    r#"<polyline points="{pts}" fill="none" stroke="{stroke}" stroke-width="{sw:.2}" stroke-linejoin="round"{layer_attr}{id_attr}/>"#,
                    stroke = f.stroke,
                    sw = f.stroke_w,
                );
            }
        }
    }

    out.push_str("</svg>");
    Render3d {
        svg: out,
        width_px,
        height_px,
    }
}

/// A 2D footprint extracted from a primitive, ready to extrude.
struct Footprint<'a> {
    points: Vec<[f64; 2]>,
    closed: bool,
    common: &'a CommonAttrs,
}

fn collect_layer_faces(
    faces: &mut Vec<Face>,
    layer_name: &str,
    cf: &CfFile,
    layer_color: &str,
    cam: &Camera,
    cut: Option<&Cut>,
) {
    let mut prints: Vec<Footprint> = Vec::new();

    for e in &cf.rects {
        let [ox, oy] = e.origin;
        prints.push(Footprint {
            points: vec![
                [ox, oy],
                [ox + e.width, oy],
                [ox + e.width, oy + e.height],
                [ox, oy + e.height],
            ],
            closed: true,
            common: &e.common,
        });
    }
    for e in &cf.polylines {
        prints.push(Footprint {
            points: e.points.clone(),
            closed: e.closed,
            common: &e.common,
        });
    }
    for e in &cf.lines {
        prints.push(Footprint {
            points: vec![e.from, e.to],
            closed: false,
            common: &e.common,
        });
    }
    for e in &cf.circles {
        prints.push(Footprint {
            points: polygonize_arc(e.center, e.radius, 0.0, 360.0, CIRCLE_SEGMENTS),
            closed: true,
            common: &e.common,
        });
    }
    for e in &cf.arcs {
        let segs = ((e.to_angle - e.from_angle).abs() / 360.0 * CIRCLE_SEGMENTS as f64)
            .ceil()
            .max(2.0) as usize;
        prints.push(Footprint {
            points: polygonize_arc(e.center, e.radius, e.from_angle, e.to_angle, segs),
            closed: false,
            common: &e.common,
        });
    }

    for fp in prints {
        if fp.points.len() < 2 {
            continue;
        }
        let base = fp.common.elevation.unwrap_or(0.0);
        let h = fp.common.extrude.unwrap_or(0.0);
        let color = fp
            .common
            .color
            .clone()
            .unwrap_or_else(|| layer_color.to_string());
        let id = fp.common.id.clone();

        if h > 0.0 {
            // Extruded footprint → a real prism/wall mesh.
            let mesh = apply_cut(mesh::prism(&fp.points, base, h, fp.closed), cut);
            emit_mesh(faces, &mesh, &color, layer_name, &id, cam);
        } else if cut.is_none() {
            // Flat footprint lying on the ground plane (the plan, projected).
            // Skipped in section views (there is no solid to cut).
            emit_outline(faces, &fp, base, &color, layer_name, &id, cam);
        }
    }

    collect_solid_faces(faces, layer_name, cf, layer_color, cam, cut);
}

/// Subtract the cut's discarded half-space from `m`, if a cut is active.
fn apply_cut(m: mesh::Mesh, cut: Option<&Cut>) -> mesh::Mesh {
    match cut {
        Some(c) => m.difference(&c.discard_box()),
        None => m,
    }
}

/// Build the named [`mesh`] solids, apply each `[[boolean]]`, and emit the
/// results. Solids consumed by a boolean are not drawn on their own.
fn collect_solid_faces(
    faces: &mut Vec<Face>,
    layer_name: &str,
    cf: &CfFile,
    layer_color: &str,
    cam: &Camera,
    cut: Option<&Cut>,
) {
    use std::collections::{HashMap, HashSet};

    let mut meshes: HashMap<&str, mesh::Mesh> = HashMap::new();
    let mut colors: HashMap<&str, String> = HashMap::new();
    for s in &cf.solids {
        meshes.insert(s.id.as_str(), build_solid(s));
        colors.insert(
            s.id.as_str(),
            s.color.clone().unwrap_or_else(|| layer_color.to_string()),
        );
    }

    let mut consumed: HashSet<&str> = HashSet::new();
    for b in &cf.booleans {
        consumed.insert(b.base.as_str());
        for t in &b.tools {
            consumed.insert(t.as_str());
        }
    }

    for b in &cf.booleans {
        let Some(base) = meshes.get(b.base.as_str()) else {
            continue; // unknown base — skip rather than fail the whole render
        };
        let mut acc = base.clone();
        for t in &b.tools {
            if let Some(tool) = meshes.get(t.as_str()) {
                acc = match b.op.as_str() {
                    "union" => acc.union(tool),
                    "intersection" => acc.intersection(tool),
                    _ => acc.difference(tool), // default: difference (cut)
                };
            }
        }
        let color = b
            .color
            .clone()
            .or_else(|| colors.get(b.base.as_str()).cloned())
            .unwrap_or_else(|| layer_color.to_string());
        emit_mesh(faces, &apply_cut(acc, cut), &color, layer_name, &b.id, cam);
    }

    for s in &cf.solids {
        if consumed.contains(s.id.as_str()) {
            continue;
        }
        let (Some(m), Some(c)) = (meshes.get(s.id.as_str()), colors.get(s.id.as_str())) else {
            continue;
        };
        emit_mesh(
            faces,
            &apply_cut(m.clone(), cut),
            c,
            layer_name,
            &Some(s.id.clone()),
            cam,
        );
    }
}

fn build_solid(s: &crate::model::CfSolid) -> mesh::Mesh {
    let at = s.at.unwrap_or([0.0, 0.0, 0.0]);
    match s.shape.as_str() {
        "cylinder" => mesh::cylinder_solid(
            at,
            s.radius.unwrap_or(1.0),
            s.height.unwrap_or(1.0),
            s.segments.unwrap_or(40),
        ),
        _ => mesh::box_solid(at, s.size.unwrap_or([1.0, 1.0, 1.0])),
    }
}

// ── Mesh → shaded faces ────────────────────────────────────────────────

/// Light direction (from the upper front).
const LIGHT: V3 = [0.32, 0.24, 0.92];
const AMBIENT: f64 = 0.40;
const DIFFUSE: f64 = 0.60;

type V3 = [f64; 3];

fn v_sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn v_cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn v_dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn v_norm(a: V3) -> V3 {
    let l = v_dot(a, a).sqrt();
    if l < 1e-12 {
        a
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// Project a mesh: cull back faces, flat-shade by the surface normal, and push
/// one [`Face`] per visible triangle.
fn emit_mesh(
    faces: &mut Vec<Face>,
    m: &mesh::Mesh,
    color: &str,
    layer: &str,
    id: &Option<String>,
    cam: &Camera,
) {
    for t in &m.tris {
        let normal = v_norm(v_cross(v_sub(t[1], t[0]), v_sub(t[2], t[0])));
        if v_dot(normal, cam.view) <= 0.0 {
            continue; // back face — hidden
        }
        let lit = AMBIENT + DIFFUSE * v_dot(normal, LIGHT).max(0.0);
        let proj: Vec<(f64, f64)> = t.iter().map(|&p| cam.project(p)).collect();
        let cx = (t[0][0] + t[1][0] + t[2][0]) / 3.0;
        let cy = (t[0][1] + t[1][1] + t[2][1]) / 3.0;
        let cz = (t[0][2] + t[1][2] + t[2][2]) / 3.0;
        // Stroke == fill: coplanar triangles (same shade) merge seamlessly so
        // the internal triangulation never shows; the small width closes the
        // anti-alias cracks between adjacent triangles. Face-to-face contrast
        // (different normals → different shade) still defines the real edges.
        let fill = scale_color(color, lit.clamp(0.0, 1.0));
        faces.push(Face {
            proj,
            stroke: fill.clone(),
            fill: Some(fill),
            stroke_w: 1.0,
            depth: cam.depth([cx, cy, cz]),
            layer: layer.to_string(),
            id: id.clone(),
        });
    }
}

/// A flat (non-extruded) footprint, drawn as an outline on the ground plane.
fn emit_outline(
    faces: &mut Vec<Face>,
    fp: &Footprint,
    base: f64,
    color: &str,
    layer: &str,
    id: &Option<String>,
    cam: &Camera,
) {
    let mut ring: Vec<[f64; 3]> = fp.points.iter().map(|p| [p[0], p[1], base]).collect();
    if fp.closed {
        if let Some(&first) = ring.first() {
            ring.push(first); // close the outline visually
        }
    }
    let mut proj = Vec::with_capacity(ring.len());
    let mut d = f64::MIN;
    for &p in &ring {
        proj.push(cam.project(p));
        d = d.max(cam.depth(p));
    }
    faces.push(Face {
        proj,
        fill: None,
        stroke: color.to_string(),
        stroke_w: 1.4,
        depth: d - 0.01, // ground outlines sit just behind solids at the same spot
        layer: layer.to_string(),
        id: id.clone(),
    });
}

/// Sample points along an arc (degrees, counterclockwise). For a full circle
/// pass 0→360; the duplicate end point is dropped for closed footprints.
fn polygonize_arc(center: [f64; 2], radius: f64, from: f64, to: f64, segs: usize) -> Vec<[f64; 2]> {
    let segs = segs.max(2);
    let full = (to - from).abs() >= 359.999;
    let count = if full { segs } else { segs + 1 };
    let mut pts = Vec::with_capacity(count);
    for i in 0..count {
        let t = i as f64 / segs as f64;
        let ang = (from + (to - from) * t).to_radians();
        pts.push([
            center[0] + radius * ang.cos(),
            center[1] + radius * ang.sin(),
        ]);
    }
    pts
}

/// Multiply each channel of `#RRGGBB` by `factor`, clamped to a valid color.
fn scale_color(hex: &str, factor: f64) -> String {
    let h = hex.trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(180);
    let (r, g, b) = if h.len() == 6 {
        (parse(&h[0..2]), parse(&h[2..4]), parse(&h[4..6]))
    } else {
        (180, 180, 180)
    };
    let scale = |c: u8| ((c as f64) * factor).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", scale(r), scale(g), scale(b))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CfFile;

    #[test]
    fn extruded_rect_produces_solid_faces() {
        let cf: CfFile = toml::from_str(
            r#"
[[rect]]
id = "rc-1"
origin = [0.0, 0.0]
width = 4.0
height = 3.0
extrude = 2.5
"#,
        )
        .unwrap();
        let r = render_scene_3d(&[("l".to_string(), cf)], 800);
        // A box is 12 triangles; back-face culling leaves the visible front
        // faces (top + the sides facing the camera) — several shaded polygons.
        let polys = r.svg.matches("<polygon").count();
        assert!((3..=12).contains(&polys), "got {polys} polygons");
        assert!(r.svg.contains(r#"data-id="rc-1""#));
        assert!(r.svg.contains(r#"data-view="3d""#));
        assert!(r.width_px > 0.0 && r.height_px > 0.0);
    }

    #[test]
    fn flat_geometry_renders_as_ground_outline() {
        let cf: CfFile = toml::from_str(
            r#"
[[rect]]
origin = [0.0, 0.0]
width = 4.0
height = 3.0
"#,
        )
        .unwrap();
        let r = render_scene_3d(&[("l".to_string(), cf)], 800);
        assert_eq!(r.svg.matches("<polygon").count(), 0);
        assert_eq!(r.svg.matches("<polyline").count(), 1);
    }

    #[test]
    fn color_scaling_clamps() {
        assert_eq!(scale_color("#FFFFFF", 0.5), "#808080");
        assert_eq!(scale_color("#808080", 2.0), "#FFFFFF");
        assert_eq!(scale_color("#000000", 1.0), "#000000");
    }
}
