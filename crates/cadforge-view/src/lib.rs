//! cadforge-view — vector viewer for cadforge projects.
//!
//! Stub crate; the wgpu-backed renderer lands in a follow-up commit.

use anyhow::Result;
use std::path::Path;

pub fn run_viewer(project_dir: &Path, layer: Option<&str>) -> Result<()> {
    let _ = (project_dir, layer);
    Ok(())
}
