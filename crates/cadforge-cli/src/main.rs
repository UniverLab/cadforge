use anyhow::{bail, Result};
use cadforge::compiler::{check_project, compile_project, list_layers};
use cadforge::config::{config_set, config_show};
use cadforge::fmt::format_project;
use cadforge::importer::import_dxf;
use cadforge::preview::generate_preview;
use cadforge::scaffold::{create_project, init_project};
use cadforge::watch::watch_project;
use cadforge_view::run_viewer;
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
        /// Output file path (defaults to output.dxf in project dir)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Validate constraints and geometry without generating DXF
        #[arg(long)]
        check: bool,
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
        /// Image width in pixels
        #[arg(short, long, default_value = "2048")]
        width: u32,
        /// Image height in pixels
        #[arg(short, long, default_value = "1536")]
        height: u32,
        /// Render only a specific layer
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Format .cf files (sort keys, normalize whitespace)
    Fmt {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,
    },
    /// Watch project files and auto-rebuild on changes
    Watch {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
    /// Import a DXF file into CADforge project files
    Import {
        /// Input DXF file
        input: PathBuf,
        /// Output directory for generated project (defaults to current dir)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Import only one DXF layer
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Open project output in external viewer
    View {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// View only one layer
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Global cadforge configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Set global default value
    Set {
        /// Config key (author | units)
        key: String,
        /// Value to store
        value: String,
    },
    /// Show global configuration
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => create_project(&name, &PathBuf::from(".")),
        Commands::Init => init_project(&PathBuf::from(".")),
        Commands::Build {
            path,
            layer,
            output,
            check,
        } => {
            let dir = resolve_project_dir(path)?;
            if check {
                check_project(&dir)?;
                Ok(())
            } else {
                compile_project(&dir, layer.as_deref(), output.as_deref())
            }
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
        Commands::Preview {
            path,
            width,
            height,
            layer,
        } => {
            let dir = resolve_project_dir(path)?;
            generate_preview(&dir, width, height, layer.as_deref())
        }
        Commands::Fmt { path, check } => {
            let dir = resolve_project_dir(path)?;
            format_project(&dir, check)
        }
        Commands::Watch { path } => {
            let dir = resolve_project_dir(path)?;
            watch_project(&dir)
        }
        Commands::Import {
            input,
            output,
            layer,
        } => {
            let out_dir = output.unwrap_or_else(|| PathBuf::from("."));
            import_dxf(&input, &out_dir, layer.as_deref())
        }
        Commands::View { path, layer } => {
            let dir = resolve_project_dir(path)?;
            run_viewer(&dir, layer.as_deref())
        }
        Commands::Config { command } => match command {
            ConfigCommands::Set { key, value } => config_set(&key, &value),
            ConfigCommands::Show => config_show(),
        },
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
