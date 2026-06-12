//! Scaffold — generates a new CADforge project structure.

use crate::schema::CF_REFERENCE;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Create a new CADforge project in the given directory.
pub fn create_project(name: &str, parent: &Path) -> Result<()> {
    let project_dir = parent.join(name);
    if project_dir.exists() {
        bail!("Directory '{}' already exists", project_dir.display());
    }

    fs::create_dir_all(&project_dir)?;
    write_project_files(&project_dir, name)?;

    println!("✓ Project '{}' created at {}", name, project_dir.display());
    println!("  → project.toml");
    println!("  → muros.cf");
    println!("  → puertas.cf");
    println!("  → mobiliario.cf");
    println!("  → cotas.cf");
    println!("  → AGENTS.md");
    println!("  → .gitignore");
    println!(
        "\n  Run `cadforge serve --path {}` for a live preview,",
        name
    );
    println!("  or `cadforge build --path {}` to compile to DXF.", name);
    Ok(())
}

/// Initialize a CADforge project in the current directory.
pub fn init_project(dir: &Path) -> Result<()> {
    if dir.join("project.toml").exists() {
        bail!("project.toml already exists in '{}'", dir.display());
    }

    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");
    write_project_files(dir, name)?;

    println!("✓ Initialized CADforge project in {}", dir.display());
    println!("  → project.toml");
    println!("  → muros.cf");
    println!("  → puertas.cf");
    println!("  → mobiliario.cf");
    println!("  → cotas.cf");
    println!("  → AGENTS.md");
    println!("  → .gitignore");
    println!("\n  Run `cadforge serve` for a live preview.");
    Ok(())
}

fn write_project_files(project_dir: &Path, name: &str) -> Result<()> {
    let project_toml = format!(
        r#"[project]
name = "{name}"
scale = "1:100"
units = "m"

[layers]
muros = {{ file = "muros.cf", locked = false }}
puertas = {{ file = "puertas.cf", locked = false }}
mobiliario = {{ file = "mobiliario.cf", locked = false }}
cotas = {{ file = "cotas.cf", locked = false }}
"#
    );
    fs::write(project_dir.join("project.toml"), project_toml)?;

    let gitignore = "# CADforge output\noutput.dxf\npreview.png\npreview.svg\npreview.meta.json\n\n# Rust build artifacts\ntarget/\n";
    fs::write(project_dir.join(".gitignore"), gitignore)?;

    let agents_md = format!(
        r#"# {name} — Agent Guide

This is a CADforge project: geometry declared as TOML, compiled to DXF.
Edit the `.cf` layer files listed in `project.toml`; never edit `output.dxf`
or `preview.*` (generated).

Feedback loop:

1. Edit `.cf` files (format reference below).
2. `cadforge check --json` — validate and read constraint issues.
3. `cadforge preview` — render `preview.png` (faithful: real text, measured
   dimensions, hatches) + `preview.meta.json` (entity bounding boxes in world
   and pixel coordinates). Look at the image to verify your work.
4. `cadforge preview --highlight <id1,id2>` — re-render with labeled amber
   markers around the entities you just touched, to confirm the change landed
   where intended.
5. If a human is watching, `cadforge serve` gives them a live browser preview
   that refreshes automatically on every save.

{reference}"#,
        name = name,
        reference = CF_REFERENCE
    );
    fs::write(project_dir.join("AGENTS.md"), agents_md)?;

    let muros_cf = r##"[layer]
name = "muros"
color = "#FFFFFF"
line_weight = 0.50

# Perímetro exterior
[[polyline]]
id = "pl-perimetro"
points = [[0.0, 0.0], [8.0, 0.0], [8.0, 6.0], [0.0, 6.0]]
closed = true
weight = 0.50

# Muro divisorio horizontal
[[line]]
id = "ln-div-h"
from = [0.0, 3.5]
to = [5.0, 3.5]
weight = 0.35

# Muro divisorio vertical
[[line]]
id = "ln-div-v"
from = [5.0, 0.0]
to = [5.0, 6.0]
weight = 0.35
"##;
    fs::write(project_dir.join("muros.cf"), muros_cf)?;

    let puertas_cf = r##"[layer]
name = "puertas"
color = "#00CC44"

# Puerta principal
[[arc]]
id = "ar-puerta-principal"
center = [0.0, 2.5]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

# Puerta interior
[[arc]]
id = "ar-puerta-int"
center = [5.0, 4.5]
radius = 0.8
from_angle = 90.0
to_angle = 180.0
"##;
    fs::write(project_dir.join("puertas.cf"), puertas_cf)?;

    let mobiliario_cf = r##"[layer]
name = "mobiliario"
color = "#4488FF"

# Mesa sala
[[rect]]
id = "rc-mesa"
origin = [1.5, 4.5]
width = 2.0
height = 1.0

# Cama dormitorio
[[rect]]
id = "rc-cama"
origin = [5.5, 4.0]
width = 2.0
height = 1.5

# Etiquetas
[[text]]
id = "tx-sala"
position = [2.0, 5.0]
content = "SALA"
size = 0.25

[[text]]
id = "tx-dorm"
position = [6.0, 5.0]
content = "DORMITORIO"
size = 0.20

[[text]]
id = "tx-cocina"
position = [2.0, 1.5]
content = "COCINA"
size = 0.20
"##;
    fs::write(project_dir.join("mobiliario.cf"), mobiliario_cf)?;

    let cotas_cf = r##"[layer]
name = "cotas"
color = "#FF4444"

# Cota horizontal total
[[dim]]
id = "dm-ancho"
type = "linear"
from = [0.0, 0.0]
to = [8.0, 0.0]
offset = -0.8

# Cota vertical total
[[dim]]
id = "dm-alto"
type = "linear"
from = [0.0, 0.0]
to = [0.0, 6.0]
offset = -0.8
"##;
    fs::write(project_dir.join("cotas.cf"), cotas_cf)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn creates_project_structure() {
        let tmp = PathBuf::from("/tmp/cadforge_test_new");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        create_project("mi-proyecto", &tmp).unwrap();

        let project_dir = tmp.join("mi-proyecto");
        assert!(project_dir.join("project.toml").exists());
        assert!(project_dir.join("muros.cf").exists());
        assert!(project_dir.join("puertas.cf").exists());
        assert!(project_dir.join("mobiliario.cf").exists());
        assert!(project_dir.join("cotas.cf").exists());
        assert!(project_dir.join("AGENTS.md").exists());
        assert!(project_dir.join(".gitignore").exists());

        let content = fs::read_to_string(project_dir.join("project.toml")).unwrap();
        assert!(content.contains("mi-proyecto"));
        assert!(content.contains("muros.cf"));

        let gitignore = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
        assert!(gitignore.contains("output.dxf"));
        assert!(gitignore.contains("preview.svg"));
        assert!(gitignore.contains("target/"));

        let agents = fs::read_to_string(project_dir.join("AGENTS.md")).unwrap();
        assert!(agents.contains("mi-proyecto"));
        assert!(agents.contains("[[line]]"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fails_if_dir_exists() {
        let tmp = PathBuf::from("/tmp/cadforge_test_exists");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("existing")).unwrap();

        let result = create_project("existing", &tmp);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_in_existing_dir() {
        let tmp = PathBuf::from("/tmp/cadforge_test_init");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        init_project(&tmp).unwrap();

        assert!(tmp.join("project.toml").exists());
        assert!(tmp.join("muros.cf").exists());
        assert!(tmp.join("puertas.cf").exists());
        assert!(tmp.join("mobiliario.cf").exists());
        assert!(tmp.join("cotas.cf").exists());
        assert!(tmp.join("AGENTS.md").exists());
        assert!(tmp.join(".gitignore").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_fails_if_project_exists() {
        let tmp = PathBuf::from("/tmp/cadforge_test_init_exists");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("project.toml"), "").unwrap();

        let result = init_project(&tmp);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }
}
