//! Global configuration for cadforge CLI defaults.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::PathBuf;
use toml_edit::{value, DocumentMut, Item, Table};

const SUPPORTED_KEYS: &[&str] = &["author", "units"];

fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".cadforge").join("config.toml"))
}

fn load_document(path: &PathBuf) -> Result<DocumentMut> {
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
    if !SUPPORTED_KEYS.contains(&key) {
        let options = SUPPORTED_KEYS.join(", ");
        return Err(anyhow!(
            "Unsupported config key '{}'. Supported keys: {}",
            key,
            options
        ));
    }

    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create {}", parent.display()))?;
    }

    let mut doc = load_document(&path)?;
    let defaults = ensure_defaults_table(&mut doc)?;
    defaults[key] = value(val);

    fs::write(&path, doc.to_string())
        .with_context(|| format!("Cannot write {}", path.display()))?;
    println!("✓ config {} = {}", key, val);
    println!("  {}", path.display());
    Ok(())
}

pub fn config_show() -> Result<()> {
    let path = config_path()?;
    let doc = load_document(&path)?;
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
