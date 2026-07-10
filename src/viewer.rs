//! External viewer launcher for DXF outputs.

use crate::compiler::compile_project;
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn view_project(project_dir: &Path, layer_filter: Option<&str>) -> Result<()> {
    let output = if let Some(layer) = layer_filter {
        let sanitized = layer
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        std::env::temp_dir().join(format!("cadspec-view-{}.dxf", sanitized))
    } else {
        project_dir.join("output.dxf")
    };

    compile_project(project_dir, layer_filter, Some(&output))?;
    open_file(&output)?;
    println!("✓ Viewer opened with {}", output.display());
    Ok(())
}

fn open_file(path: &Path) -> Result<()> {
    if let Ok(custom) = std::env::var("CADSPEC_VIEWER_CMD") {
        let status = Command::new(&custom)
            .arg(path)
            .status()
            .with_context(|| format!("Failed to run CADSPEC_VIEWER_CMD='{}'", custom))?;
        if !status.success() {
            return Err(anyhow!(
                "Custom viewer command failed for {} (exit code: {:?})",
                path.display(),
                status.code()
            ));
        }
        return Ok(());
    }

    let program = opener_program();
    let status = if cfg!(target_os = "windows") {
        Command::new(program.0)
            .args([program.1, program.2, path.to_string_lossy().as_ref()])
            .status()
            .with_context(|| format!("Failed to run opener for {}", path.display()))?
    } else {
        Command::new(program.0)
            .arg(path)
            .status()
            .with_context(|| format!("Failed to run opener for {}", path.display()))?
    };

    if !status.success() {
        return Err(anyhow!(
            "Viewer command failed for {} (exit code: {:?})",
            path.display(),
            status.code()
        ));
    }
    Ok(())
}

fn opener_program() -> (&'static str, &'static str, &'static str) {
    if cfg!(target_os = "macos") {
        ("open", "", "")
    } else if cfg!(target_os = "windows") {
        ("cmd", "/C", "start")
    } else {
        ("xdg-open", "", "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opener_program_is_resolved() {
        let (cmd, _, _) = opener_program();
        assert!(!cmd.is_empty());
    }
}
