//! Formatter — normalizes `.cf` and `project.toml` files, analogous to
//! `terraform fmt`: canonical `key = value` spacing, exactly one blank line
//! between blocks, tidy arrays and inline tables. Comments are preserved.
//! Files that fail to parse — or whose formatted output would not reparse to
//! the same values — are left untouched.

use crate::parser::parse_project;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use toml_edit::{Array, Decor, DocumentMut, InlineTable, Item, KeyMut, RawString, Table, Value};

/// Format `project.toml` and every `.cf` file in a project.
pub fn format_project(project_dir: &Path, check_only: bool) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    let mut files: BTreeSet<PathBuf> = BTreeSet::new();
    files.insert(project_dir.join("project.toml"));
    for entry in project.layers.values() {
        files.insert(project_dir.join(&entry.file));
    }
    if let Ok(dir) = std::fs::read_dir(project_dir) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "cf") {
                files.insert(path);
            }
        }
    }

    let mut changed = 0;
    for path in files {
        if !path.exists() {
            continue;
        }
        let display = path
            .strip_prefix(project_dir)
            .unwrap_or(&path)
            .display()
            .to_string();
        let original = std::fs::read_to_string(&path)?;
        let formatted = format_source(&original);
        if original == formatted {
            println!("  {display} — ok");
        } else if check_only {
            println!("✗ {display} — needs formatting");
            changed += 1;
        } else {
            std::fs::write(&path, &formatted)?;
            println!("✓ {display} — formatted");
            changed += 1;
        }
    }

    if check_only && changed > 0 {
        anyhow::bail!("{changed} file(s) need formatting. Run `cadforge fmt` to fix.");
    }
    if !check_only {
        println!("✓ {changed} file(s) formatted");
    }
    Ok(())
}

/// Format TOML source, guaranteeing the result parses to the same values.
fn format_source(content: &str) -> String {
    let formatted = format_toml(content);
    let before: Result<toml::Value, _> = toml::from_str(content);
    let after: Result<toml::Value, _> = toml::from_str(&formatted);
    match (before, after) {
        (Ok(b), Ok(a)) if b == a => formatted,
        _ => content.to_string(),
    }
}

fn format_toml(content: &str) -> String {
    let Ok(mut doc) = content.parse::<DocumentMut>() else {
        return content.to_string();
    };

    let mut first = true;
    for (mut key, item) in doc.as_table_mut().iter_mut() {
        match item {
            Item::Table(table) => {
                normalize_header(table.decor_mut(), first);
                normalize_table(table);
            }
            Item::ArrayOfTables(tables) => {
                for table in tables.iter_mut() {
                    normalize_header(table.decor_mut(), first);
                    normalize_table(table);
                    first = false;
                }
            }
            Item::Value(value) => {
                normalize_key(&mut key);
                normalize_value(value);
            }
            Item::None => {}
        }
        first = false;
    }

    let trailing = comments_only(&raw_text(Some(doc.trailing())), true);
    doc.set_trailing(trailing);

    let out = doc.to_string();
    let trimmed = out.trim_end_matches('\n');
    if trimmed.is_empty() {
        out
    } else {
        format!("{trimmed}\n")
    }
}

fn normalize_table(table: &mut Table) {
    for (mut key, item) in table.iter_mut() {
        match item {
            Item::Value(value) => {
                normalize_key(&mut key);
                normalize_value(value);
            }
            Item::Table(child) => {
                if child.is_dotted() {
                    // dotted key (e.g. `cotas.parent = "muros"`): only touch values
                    normalize_dotted(child);
                } else {
                    normalize_header(child.decor_mut(), false);
                    normalize_table(child);
                }
            }
            Item::ArrayOfTables(tables) => {
                for child in tables.iter_mut() {
                    normalize_header(child.decor_mut(), false);
                    normalize_table(child);
                }
            }
            Item::None => {}
        }
    }
}

fn normalize_dotted(table: &mut Table) {
    for (_, item) in table.iter_mut() {
        match item {
            Item::Value(value) => normalize_value(value),
            Item::Table(child) if child.is_dotted() => normalize_dotted(child),
            _ => {}
        }
    }
}

/// `[header]` / `[[header]]` prefix: one blank line between blocks (none for
/// the first), preceding comments preserved one per line.
fn normalize_header(decor: &mut Decor, first: bool) {
    let prefix = comments_only(&raw_text(decor.prefix()), !first);
    let suffix = trailing_comment(decor.suffix());
    decor.set_prefix(prefix);
    decor.set_suffix(suffix);
}

/// Key of a `key = value` pair: comments kept, a single leading blank line
/// kept if the author separated the pair from the previous one.
fn normalize_key(key: &mut KeyMut) {
    let raw = raw_text(key.leaf_decor().prefix());
    let blank = raw.split('#').next().unwrap_or("").contains('\n');
    let prefix = comments_only(&raw, blank);
    let decor = key.leaf_decor_mut();
    decor.set_prefix(prefix);
    decor.set_suffix(" ");
}

fn normalize_value(value: &mut Value) {
    let suffix = trailing_comment(value.decor().suffix());
    match value {
        Value::Array(array) => {
            normalize_array(array);
        }
        Value::InlineTable(table) => {
            normalize_inline_table(table);
        }
        _ => {}
    }
    let decor = value.decor_mut();
    decor.set_prefix(" ");
    decor.set_suffix(suffix);
}

/// Arrays keep the author's single-line vs multi-line choice; multi-line
/// arrays get one element per line, four-space indent and a trailing comma.
/// Arrays containing comments are left untouched.
fn normalize_array(array: &mut Array) {
    if array_has_comment(array) {
        return;
    }
    let multiline = raw_text(Some(array.trailing())).contains('\n')
        || array.iter().any(|v| {
            raw_text(v.decor().prefix()).contains('\n')
                || raw_text(v.decor().suffix()).contains('\n')
        });

    if multiline {
        for value in array.iter_mut() {
            if let Value::Array(inner) = value {
                normalize_inline_array(inner);
            }
            let decor = value.decor_mut();
            decor.set_prefix("\n    ");
            decor.set_suffix("");
        }
        array.set_trailing("\n");
        array.set_trailing_comma(true);
    } else {
        normalize_inline_array(array);
    }
}

fn normalize_inline_array(array: &mut Array) {
    if array_has_comment(array) {
        return;
    }
    let mut first = true;
    for value in array.iter_mut() {
        if let Value::Array(inner) = value {
            normalize_inline_array(inner);
        }
        let decor = value.decor_mut();
        decor.set_prefix(if first { "" } else { " " });
        decor.set_suffix("");
        first = false;
    }
    array.set_trailing("");
    array.set_trailing_comma(false);
}

fn normalize_inline_table(table: &mut InlineTable) {
    let len = table.len();
    for (i, (mut key, value)) in table.iter_mut().enumerate() {
        let decor = key.leaf_decor_mut();
        decor.set_prefix(" ");
        decor.set_suffix(" ");
        let decor = value.decor_mut();
        decor.set_prefix(" ");
        decor.set_suffix(if i + 1 == len { " " } else { "" });
    }
}

fn array_has_comment(array: &Array) -> bool {
    raw_text(Some(array.trailing())).contains('#')
        || array.iter().any(|v| {
            raw_text(v.decor().prefix()).contains('#') || raw_text(v.decor().suffix()).contains('#')
        })
}

/// Extract `#` comment lines from raw decor text, optionally preceded by one
/// blank line; everything else (stray whitespace, extra blanks) is dropped.
fn comments_only(raw: &str, leading_blank: bool) -> String {
    let mut out = String::new();
    if leading_blank {
        out.push('\n');
    }
    for line in raw.lines().map(str::trim).filter(|l| l.starts_with('#')) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Keep a same-line trailing comment (` # like this`), drop plain whitespace.
fn trailing_comment(raw: Option<&RawString>) -> String {
    let text = raw_text(raw);
    if text.contains('#') {
        format!(" {}", text.trim())
    } else {
        String::new()
    }
}

fn raw_text(raw: Option<&RawString>) -> String {
    raw.and_then(|r| r.as_str()).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_spacing_and_blank_lines() {
        let input = "[layer]\nname=\"test\"\ncolor   =   \"#FFFFFF\"\n\n\n\n[[line]]\nid=\"ln-001\"\nfrom=[0.0,0.0]\nto = [ 10.0 ,  0.0 ]\n[[line]]\nid = \"ln-002\"\nfrom = [0.0, 1.0]\nto = [10.0, 1.0]\n";
        let expected = "[layer]\nname = \"test\"\ncolor = \"#FFFFFF\"\n\n[[line]]\nid = \"ln-001\"\nfrom = [0.0, 0.0]\nto = [10.0, 0.0]\n\n[[line]]\nid = \"ln-002\"\nfrom = [0.0, 1.0]\nto = [10.0, 1.0]\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn preserves_comments() {
        let input = "[layer]\nname = \"muros\"\n\n# Perímetro exterior\n[[polyline]]\nid = \"pl-001\" # principal\npoints = [[0.0, 0.0], [5.0, 0.0]]\n";
        let output = format_source(input);
        assert!(output.contains("# Perímetro exterior\n[[polyline]]"));
        assert!(output.contains("id = \"pl-001\" # principal"));
    }

    #[test]
    fn normalizes_multiline_point_arrays() {
        let input = "[[polyline]]\nid = \"pl-001\"\npoints = [\n  [0.30,0.0],\n      [1.55, 0.0],\n  [1.456, 0.530]\n]\nclosed = true\n";
        let expected = "[[polyline]]\nid = \"pl-001\"\npoints = [\n    [0.30, 0.0],\n    [1.55, 0.0],\n    [1.456, 0.530],\n]\nclosed = true\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn normalizes_inline_tables() {
        let input =
            "[project]\nname = \"x\"\n\n[layers]\nmuros = {file=\"muros.cf\",locked=false}\n";
        let output = format_source(input);
        assert!(output.contains("muros = { file = \"muros.cf\", locked = false }"));
    }

    #[test]
    fn keeps_dotted_constraint_keys_intact() {
        let input = "[constraints]\ncotas.parent = \"muros\"\ncotas.belongs_to = \"muros\"\n";
        let output = format_source(input);
        assert!(output.contains("cotas.parent = \"muros\""));
        assert!(output.contains("cotas.belongs_to = \"muros\""));
    }

    #[test]
    fn keeps_blank_line_groups_inside_blocks() {
        let input = "[layer]\nname = \"x\"\n\n\ncolor = \"#FFFFFF\"\n";
        let output = format_source(input);
        assert!(output.contains("name = \"x\"\n\ncolor"));
    }

    #[test]
    fn is_idempotent_and_meaning_preserving() {
        for path in [
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/taller/escalera.cf"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/taller/planta.cf"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/taller/cotas.cf"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/taller/project.toml"),
        ] {
            let original = std::fs::read_to_string(path).unwrap();
            let once = format_source(&original);
            let twice = format_source(&once);
            assert_eq!(once, twice, "fmt not idempotent for {path}");
            let before: toml::Value = toml::from_str(&original).unwrap();
            let after: toml::Value = toml::from_str(&once).unwrap();
            assert_eq!(before, after, "fmt changed meaning of {path}");
        }
    }

    #[test]
    fn returns_original_on_parse_error() {
        let input = "invalid [[[ toml";
        assert_eq!(format_source(input), input);
    }
}
