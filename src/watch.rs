//! Watch — monitors project files and auto-rebuilds on changes.

use crate::compiler::compile_project;
use crate::parser::parse_project;
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(300);

/// Watch project files and auto-rebuild on changes.
pub fn watch_project(project_dir: &Path) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    println!("Watching project: {}", project.project.name);
    println!("  Directory: {}", project_dir.display());
    println!("  Layers: {}", project.layers.len());
    println!();
    println!("Press Ctrl+C to stop.");
    println!();

    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    )?;

    watcher.watch(project_dir, RecursiveMode::NonRecursive)?;

    let mut last_build = Instant::now();

    loop {
        match rx.recv() {
            Ok(event) => {
                if !is_relevant(&event) {
                    continue;
                }
                if last_build.elapsed() < DEBOUNCE {
                    continue;
                }

                let changed_files: Vec<String> = event
                    .paths
                    .iter()
                    .filter_map(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                    })
                    .collect();

                println!("⟳ Change detected: {}", changed_files.join(", "));

                match compile_project(project_dir, None, None) {
                    Ok(()) => println!("  ✓ Rebuild complete\n"),
                    Err(e) => println!("  ✗ Build failed: {}\n", e),
                }

                last_build = Instant::now();
            }
            Err(e) => {
                anyhow::bail!("Watch error: {}", e);
            }
        }
    }
}

fn is_relevant(event: &Event) -> bool {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
        _ => return false,
    }

    event.paths.iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "cf" || e == "toml")
    })
}
