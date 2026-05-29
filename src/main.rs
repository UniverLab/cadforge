use anyhow::{bail, Result};
use cadforge::compiler::{check_project, compile_project};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cadforge",
    version,
    about = "Architecture as Code — declarative geometry → DXF"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile project (.cf files) → DXF output
    Build {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
    /// Validate project without generating DXF
    Check {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { path } => {
            let dir = resolve_project_dir(path)?;
            compile_project(&dir)
        }
        Commands::Check { path } => {
            let dir = resolve_project_dir(path)?;
            check_project(&dir)?;
            Ok(())
        }
    }
}

fn resolve_project_dir(path: Option<PathBuf>) -> Result<PathBuf> {
    let dir = path.unwrap_or_else(|| PathBuf::from("."));
    if !dir.join("project.toml").exists() {
        bail!(
            "No project.toml found in '{}'. Run `cadforge new` to create a project.",
            dir.display()
        );
    }
    Ok(dir)
}
