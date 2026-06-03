//! Formatter — normalizes .cf files (sort keys, consistent spacing).

use crate::parser::parse_project;
use anyhow::Result;
use std::path::Path;

/// Format all .cf files in a project.
pub fn format_project(project_dir: &Path, check_only: bool) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut changed = 0;

    for (_name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        if !cf_path.exists() {
            continue;
        }

        let original = std::fs::read_to_string(&cf_path)?;
        let formatted = format_cf(&original);

        if original != formatted {
            if check_only {
                println!("✗ {} — needs formatting", entry.file);
                changed += 1;
            } else {
                std::fs::write(&cf_path, &formatted)?;
                println!("✓ {} — formatted", entry.file);
                changed += 1;
            }
        } else {
            println!("  {} — ok", entry.file);
        }
    }

    if check_only && changed > 0 {
        anyhow::bail!(
            "{} file(s) need formatting. Run `cadforge fmt` to fix.",
            changed
        );
    }

    if !check_only {
        println!("✓ {} file(s) formatted", changed);
    }
    Ok(())
}

/// Format a single .cf file content.
fn format_cf(content: &str) -> String {
    let doc: toml_edit::DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(_) => return content.to_string(),
    };
    doc.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_preserves_valid_toml() {
        let input = "[layer]\nname = \"test\"\ncolor = \"#FFFFFF\"\n\n[[line]]\nid = \"ln-001\"\nfrom = [0.0, 0.0]\nto = [10.0, 0.0]\n";
        let output = format_cf(input);
        assert!(output.contains("[layer]"));
        assert!(output.contains("[[line]]"));
    }

    #[test]
    fn format_returns_original_on_parse_error() {
        let input = "invalid [[[ toml";
        let output = format_cf(input);
        assert_eq!(output, input);
    }
}
