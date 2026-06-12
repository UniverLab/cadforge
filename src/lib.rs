//! cadforge — deterministic geometry engine for reproducible architectural design.
//!
//! Pipeline: `.cf` (TOML) → intermediate model → DXF output.

pub mod color;
pub mod compiler;
pub mod config;
pub mod dxf_writer;
pub mod fmt;
pub mod importer;
pub mod model;
pub mod parser;
pub mod preview;
pub mod scaffold;
pub mod schema;
pub mod serve;
pub mod svg;
pub mod transform;
pub mod viewer;
pub mod watch;
