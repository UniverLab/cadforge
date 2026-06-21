//! Intermediate model — structs that represent `.cf` file contents.

use serde::Deserialize;

/// Common visual attributes shared by all primitives.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommonAttrs {
    pub id: Option<String>,
    pub color: Option<String>,
    pub weight: Option<f64>,
    pub style: Option<LineStyle>,
    pub layer: Option<String>,
    pub belongs_to: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    /// Extrusion height in world units for the 3D view. `None`/0 stays flat;
    /// a closed shape becomes a solid, a line/open polyline becomes a wall.
    pub extrude: Option<f64>,
    /// Base elevation (Z) for the 3D view. Defaults to 0 (the ground plane).
    pub elevation: Option<f64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    Dashdot,
}

// ── Primitives ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CfLine {
    pub from: [f64; 2],
    pub to: [f64; 2],
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfPolyline {
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub closed: bool,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfRect {
    pub origin: [f64; 2],
    pub width: f64,
    pub height: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfCircle {
    pub center: [f64; 2],
    pub radius: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfArc {
    pub center: [f64; 2],
    pub radius: f64,
    pub from_angle: f64,
    pub to_angle: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfText {
    pub position: [f64; 2],
    pub content: String,
    #[serde(default = "default_text_size")]
    pub size: f64,
    pub align: Option<TextAlign>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

fn default_text_size() -> f64 {
    2.5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfPoint {
    pub position: [f64; 2],
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfDim {
    #[serde(rename = "type")]
    pub dim_type: Option<DimType>,
    pub from: [f64; 2],
    pub to: [f64; 2],
    #[serde(default = "default_offset")]
    pub offset: f64,
    /// Label height in world units (default 0.25).
    pub text_size: Option<f64>,
    /// Decimal places for the measured value (default 2).
    pub precision: Option<u32>,
    /// Append the project units to the label (default true).
    pub show_units: Option<bool>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

fn default_offset() -> f64 {
    0.5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimType {
    Linear,
    Angular,
    Radial,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfHatch {
    pub boundary: String,
    #[serde(default = "default_pattern")]
    pub pattern: String,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default = "default_angle")]
    pub angle: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

fn default_pattern() -> String {
    "ansi31".to_string()
}
fn default_scale() -> f64 {
    1.0
}
fn default_angle() -> f64 {
    45.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfGroup {
    pub members: Vec<String>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArrayMode {
    Linear,
    Polar,
}

/// Repeats target primitives: linear (offset per copy) or polar (rotation
/// around a center — spiral stairs, gear teeth, radial columns).
#[derive(Debug, Clone, Deserialize)]
pub struct CfArray {
    /// Single target id (alternative to `targets`).
    pub target: Option<String>,
    /// Multiple target ids.
    pub targets: Option<Vec<String>>,
    pub mode: ArrayMode,
    /// Total number of instances, including the original.
    pub count: usize,
    /// Linear: displacement per copy.
    pub offset: Option<[f64; 2]>,
    /// Polar: rotation center.
    pub center: Option<[f64; 2]>,
    /// Polar: degrees per copy (counterclockwise).
    pub step_angle: Option<f64>,
    /// Polar: rotate each copy's geometry (true) or only orbit it (false).
    #[serde(default = "default_true")]
    pub rotate_items: bool,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

/// Mirrors target primitives across an axis defined by two points.
#[derive(Debug, Clone, Deserialize)]
pub struct CfMirror {
    /// Single target id (alternative to `targets`).
    pub target: Option<String>,
    /// Multiple target ids.
    pub targets: Option<Vec<String>>,
    /// Mirror axis: two points [[x1, y1], [x2, y2]].
    pub axis: [[f64; 2]; 2],
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfFill {
    /// Reference to a closed polyline or rect id, or inline points.
    pub boundary: Option<String>,
    /// Inline points (alternative to boundary reference).
    pub points: Option<Vec<[f64; 2]>>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

// ── 3D solids (CSG) ────────────────────────────────────────────────────

/// A named 3D primitive solid for the extruded/CSG view. Referenced by id from
/// `[[boolean]]`. 3D-only — solids do not appear in the 2D plan or DXF.
#[derive(Debug, Clone, Deserialize)]
pub struct CfSolid {
    pub id: String,
    /// `box` | `cylinder`.
    pub shape: String,
    /// Placement: box minimum corner, or cylinder base-circle center. Default origin.
    pub at: Option<[f64; 3]>,
    /// Box dimensions [sx, sy, sz].
    pub size: Option<[f64; 3]>,
    /// Cylinder radius.
    pub radius: Option<f64>,
    /// Cylinder height.
    pub height: Option<f64>,
    /// Cylinder facet count (default 40).
    pub segments: Option<usize>,
    pub color: Option<String>,
}

/// A CSG operation combining named solids. The result is rendered; the solids
/// it consumes (`base` + `tools`) are not drawn on their own.
#[derive(Debug, Clone, Deserialize)]
pub struct CfBoolean {
    pub id: Option<String>,
    /// `difference` | `union` | `intersection`.
    pub op: String,
    pub base: String,
    #[serde(default)]
    pub tools: Vec<String>,
    pub color: Option<String>,
}

// ── Layer-level metadata ───────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LayerMeta {
    pub name: Option<String>,
    pub color: Option<String>,
    pub line_weight: Option<f64>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
}

// ── Top-level .cf file ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CfFile {
    #[serde(rename = "layer")]
    pub layer_meta: Option<LayerMeta>,
    #[serde(default, rename = "line")]
    pub lines: Vec<CfLine>,
    #[serde(default, rename = "polyline")]
    pub polylines: Vec<CfPolyline>,
    #[serde(default, rename = "rect")]
    pub rects: Vec<CfRect>,
    #[serde(default, rename = "circle")]
    pub circles: Vec<CfCircle>,
    #[serde(default, rename = "arc")]
    pub arcs: Vec<CfArc>,
    #[serde(default, rename = "text")]
    pub texts: Vec<CfText>,
    #[serde(default, rename = "point")]
    pub points: Vec<CfPoint>,
    #[serde(default, rename = "dim")]
    pub dims: Vec<CfDim>,
    #[serde(default, rename = "hatch")]
    pub hatches: Vec<CfHatch>,
    #[serde(default, rename = "fill")]
    pub fills: Vec<CfFill>,
    #[serde(default, rename = "group")]
    pub groups: Vec<CfGroup>,
    #[serde(default, rename = "array")]
    pub arrays: Vec<CfArray>,
    #[serde(default, rename = "mirror")]
    pub mirrors: Vec<CfMirror>,
    #[serde(default, rename = "solid")]
    pub solids: Vec<CfSolid>,
    #[serde(default, rename = "boolean")]
    pub booleans: Vec<CfBoolean>,
}
