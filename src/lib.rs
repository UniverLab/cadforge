//! cadforge — deterministic geometry engine for reproducible architectural design.
//!
//! Pipeline: `.cf` (TOML) → intermediate model → DXF output.

pub mod compiler;
pub mod dxf_writer;
pub mod model;
pub mod parser;
