//! Global configuration for cadspec CLI defaults.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

const SUPPORTED_KEYS: &[&str] = &["author", "units"];

fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".cadspec").join("config.toml"))
}

fn load_document(path: &Path) -> Result<DocumentMut> {
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
    let doc = raw
        .parse::<DocumentMut>()
        .with_context(|| format!("Invalid TOML in {}", path.display()))?;
    Ok(doc)
}

fn ensure_defaults_table(doc: &mut DocumentMut) -> Result<&mut Table> {
    if !doc.as_table().contains_key("defaults") {
        doc["defaults"] = Item::Table(Table::new());
    }
    doc["defaults"]
        .as_table_mut()
        .ok_or_else(|| anyhow!("'defaults' is not a table in global config"))
}

pub fn config_set(key: &str, val: &str) -> Result<()> {
    config_set_at(&config_path()?, key, val)
}

fn config_set_at(path: &Path, key: &str, val: &str) -> Result<()> {
    if !SUPPORTED_KEYS.contains(&key) {
        let options = SUPPORTED_KEYS.join(", ");
        return Err(anyhow!(
            "Unsupported config key '{}'. Supported keys: {}",
            key,
            options
        ));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create {}", parent.display()))?;
    }

    let mut doc = load_document(path)?;
    let defaults = ensure_defaults_table(&mut doc)?;
    defaults[key] = value(val);

    fs::write(path, doc.to_string()).with_context(|| format!("Cannot write {}", path.display()))?;
    println!("✓ config {} = {}", key, val);
    println!("  {}", path.display());
    Ok(())
}

pub fn config_show() -> Result<()> {
    config_show_at(&config_path()?)
}

fn config_show_at(path: &Path) -> Result<()> {
    let doc = load_document(path)?;
    println!("Global config: {}", path.display());
    if let Some(defaults) = doc.get("defaults").and_then(Item::as_table) {
        for key in SUPPORTED_KEYS {
            if let Some(v) = defaults.get(key).and_then(Item::as_str) {
                println!("  {} = {}", key, v);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_path(name: &str) -> PathBuf {
        let dir = PathBuf::from("/tmp").join(name);
        let _ = fs::remove_dir_all(&dir);
        dir.join("config.toml")
    }

    #[test]
    fn config_set_rejects_unsupported_key() {
        let path = scratch_path("cadspec_test_config_bad_key");

        let err = config_set_at(&path, "bogus", "x").unwrap_err();

        assert!(err.to_string().contains("Unsupported config key 'bogus'"));
        assert!(!path.exists());
    }

    #[test]
    fn config_set_creates_parent_dir_and_writes_value() {
        let path = scratch_path("cadspec_test_config_new");

        config_set_at(&path, "author", "Jheison").unwrap();

        let doc = load_document(&path).unwrap();
        assert_eq!(
            doc["defaults"]["author"].as_str(),
            Some("Jheison"),
            "config file: {}",
            fs::read_to_string(&path).unwrap()
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn config_set_preserves_other_keys_and_overwrites_target() {
        let path = scratch_path("cadspec_test_config_overwrite");

        config_set_at(&path, "author", "Jheison").unwrap();
        config_set_at(&path, "units", "mm").unwrap();
        config_set_at(&path, "author", "Someone Else").unwrap();

        let doc = load_document(&path).unwrap();
        assert_eq!(doc["defaults"]["author"].as_str(), Some("Someone Else"));
        assert_eq!(doc["defaults"]["units"].as_str(), Some("mm"));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn config_show_on_missing_file_does_not_error() {
        let path = scratch_path("cadspec_test_config_missing");

        config_show_at(&path).unwrap();

        assert!(!path.exists());
    }
}
