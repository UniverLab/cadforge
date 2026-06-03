//! Parser — reads `.cf` and `project.toml` files into the intermediate model.

use crate::model::CfFile;
use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;
use std::path::Path;

// ── project.toml structures ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectFile {
    pub project: ProjectMeta,
    pub layers: IndexMap<String, LayerEntry>,
    #[serde(default)]
    pub constraints: Option<toml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    #[serde(default = "default_scale")]
    pub scale: String,
    #[serde(default = "default_units")]
    pub units: String,
    #[serde(default)]
    pub strict: bool,
    pub author: Option<String>,
    pub version: Option<String>,
}

fn default_scale() -> String {
    "1:100".to_string()
}
fn default_units() -> String {
    "m".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayerEntry {
    pub file: String,
    #[serde(default)]
    pub locked: bool,
}

// ── Parsing functions ──────────────────────────────────────────────────

/// Parse a `project.toml` file.
pub fn parse_project(path: &Path) -> Result<ProjectFile> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("Invalid TOML in {}", path.display()))
}

/// Parse a `.cf` layer file.
pub fn parse_cf(path: &Path) -> Result<CfFile> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
    toml::from_str(&content).with_context(|| {
        format!(
            "Invalid TOML in {}:\n  Check syntax: keys must be quoted, arrays use [[name]], tables use [name]",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cf_file() {
        let toml = r##"
[layer]
name = "muros"
color = "#FFFFFF"

[[line]]
id = "ln-001"
from = [0.0, 0.0]
to = [8.5, 0.0]
weight = 0.50

[[rect]]
id = "rc-001"
origin = [1.0, 1.0]
width = 3.5
height = 4.0

[[circle]]
id = "ci-001"
center = [4.0, 3.0]
radius = 0.5

[[arc]]
id = "ar-001"
center = [2.0, 2.0]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

[[text]]
id = "tx-001"
position = [4.0, 3.0]
content = "SALA"
size = 14.0
"##;
        let cf: CfFile = toml::from_str(toml).unwrap();
        assert_eq!(cf.lines.len(), 1);
        assert_eq!(cf.rects.len(), 1);
        assert_eq!(cf.circles.len(), 1);
        assert_eq!(cf.arcs.len(), 1);
        assert_eq!(cf.texts.len(), 1);
        assert_eq!(cf.layer_meta.unwrap().name.unwrap(), "muros");
    }

    #[test]
    fn parses_project_toml() {
        let toml = r#"
[project]
name = "Vivienda Unifamiliar"
scale = "1:100"
units = "m"
strict = true
author = "Arq. Test"

[layers]
muros = { file = "muros.cf", locked = false }
puertas = { file = "puertas.cf", locked = false }

[constraints]
puertas.parent = "muros"
"#;
        let proj: ProjectFile = toml::from_str(toml).unwrap();
        assert_eq!(proj.project.name, "Vivienda Unifamiliar");
        assert!(proj.project.strict);
        assert_eq!(proj.layers.len(), 2);
        assert_eq!(proj.layers["muros"].file, "muros.cf");
        assert!(proj
            .constraints
            .as_ref()
            .and_then(|v| v.get("puertas"))
            .is_some());
    }
}
