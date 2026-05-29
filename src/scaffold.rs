//! Scaffold — generates a new CADforge project structure.

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
    println!("  → planta.cf");
    println!("  → .gitignore");
    println!("\n  Run `cadforge build --path {}` to compile.", name);
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
    println!("  → planta.cf");
    println!("  → .gitignore");
    Ok(())
}

fn write_project_files(project_dir: &Path, name: &str) -> Result<()> {
    let project_toml = format!(
        r#"[project]
name = "{name}"
scale = "1:100"
units = "m"

[layers]
planta = {{ file = "planta.cf", locked = false }}
"#
    );
    fs::write(project_dir.join("project.toml"), project_toml)?;

    let gitignore = "# CADforge output\noutput.dxf\n\n# Rust build artifacts\ntarget/\n";
    fs::write(project_dir.join(".gitignore"), gitignore)?;

    let planta_cf = r##"[layer]
name = "planta"
color = "#FFFFFF"

[[line]]
id = "ln-001"
from = [0.0, 0.0]
to = [10.0, 0.0]
"##;
    fs::write(project_dir.join("planta.cf"), planta_cf)?;
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
        assert!(project_dir.join("planta.cf").exists());
        assert!(project_dir.join(".gitignore").exists());

        let content = fs::read_to_string(project_dir.join("project.toml")).unwrap();
        assert!(content.contains("mi-proyecto"));
        assert!(content.contains("planta.cf"));

        let gitignore = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
        assert!(gitignore.contains("output.dxf"));
        assert!(gitignore.contains("target/"));

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
        assert!(tmp.join("planta.cf").exists());
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
