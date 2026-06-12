use anyhow::{bail, Result};
use cadforge::compiler::{check_project, compile_project, list_layers, project_report};
use cadforge::config::{config_set, config_show};
use cadforge::fmt::format_project;
use cadforge::importer::import_dxf;
use cadforge::preview::{generate_preview, PreviewOutputs};
use cadforge::scaffold::{create_project, init_project};
use cadforge::schema::print_schema;
use cadforge::serve::serve_project;
use cadforge::viewer::view_project;
use cadforge::watch::watch_project;
use clap::{Parser, Subcommand, ValueEnum};
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
        /// Emit a machine-readable JSON report
        #[arg(long)]
        json: bool,
    },
    /// List project layers with status
    Layers {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Emit a machine-readable JSON report
        #[arg(long)]
        json: bool,
    },
    /// Generate preview (PNG + metadata JSON, or SVG) for AI agents
    Preview {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Image width in pixels
        #[arg(short, long, default_value = "1600")]
        width: u32,
        /// Image height in pixels (PNG only; SVG derives it from content)
        #[arg(short = 'H', long, default_value = "1200")]
        height: u32,
        /// Render only a specific layer
        #[arg(short, long)]
        layer: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value_t = PreviewFormat::Png)]
        format: PreviewFormat,
        /// Highlight entities by id (comma-separated) with labeled markers
        #[arg(long, value_delimiter = ',')]
        highlight: Vec<String>,
    },
    /// Live preview server — browser auto-reloads when .cf files change
    Serve {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Port to listen on
        #[arg(long, default_value = "4377")]
        port: u16,
        /// Open the browser automatically
        #[arg(long)]
        open: bool,
    },
    /// Print the .cf language reference (markdown, for humans and AI agents)
    Schema,
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

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PreviewFormat {
    /// Raster PNG + preview.meta.json
    Png,
    /// Vector SVG (real text, dimensions, hatches)
    Svg,
    /// Both PNG and SVG
    All,
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
        Commands::Check { path, json } => {
            let dir = resolve_project_dir(path)?;
            if json {
                let report = project_report(&dir)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if report.strict && !report.issues.is_empty() {
                    bail!(
                        "Check failed: {} constraint violation(s) with strict = true",
                        report.issues.len()
                    );
                }
                Ok(())
            } else {
                check_project(&dir)?;
                Ok(())
            }
        }
        Commands::Layers { path, json } => {
            let dir = resolve_project_dir(path)?;
            if json {
                let report = project_report(&dir)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                Ok(())
            } else {
                list_layers(&dir)
            }
        }
        Commands::Preview {
            path,
            width,
            height,
            layer,
            format,
            highlight,
        } => {
            let dir = resolve_project_dir(path)?;
            let outputs = PreviewOutputs {
                png: matches!(format, PreviewFormat::Png | PreviewFormat::All),
                svg: matches!(format, PreviewFormat::Svg | PreviewFormat::All),
            };
            generate_preview(&dir, width, height, layer.as_deref(), &highlight, outputs)
        }
        Commands::Serve { path, port, open } => {
            let dir = resolve_project_dir(path)?;
            serve_project(&dir, port, open)
        }
        Commands::Schema => {
            print_schema();
            Ok(())
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
            view_project(&dir, layer.as_deref())
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
