use anyhow::{bail, Result};
use cadforge::compiler::{check_project, compile_project, list_layers};
use cadforge::preview::generate_preview;
use cadforge::scaffold::{create_project, init_project};
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
    /// Create a new CADforge project
    New {
        /// Project name (creates a directory with this name)
        name: String,
    },
    /// Initialize CADforge in the current directory
    Init,
    /// Compile project (.cf files) → DXF output
    Build {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Compile only a specific layer
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Validate project without generating DXF
    Check {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
    /// List project layers with status
    Layers {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
    /// Generate PNG preview + metadata JSON for AI agents
    Preview {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => create_project(&name, &PathBuf::from(".")),
        Commands::Init => init_project(&PathBuf::from(".")),
        Commands::Build { path, layer } => {
            let dir = resolve_project_dir(path)?;
            compile_project(&dir, layer.as_deref(), None)
        }
        Commands::Check { path } => {
            let dir = resolve_project_dir(path)?;
            check_project(&dir)?;
            Ok(())
        }
        Commands::Layers { path } => {
            let dir = resolve_project_dir(path)?;
            list_layers(&dir)
        }
        Commands::Preview { path } => {
            let dir = resolve_project_dir(path)?;
            generate_preview(&dir, 2048, 1536, None)
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
