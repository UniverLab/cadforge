use anyhow::Result;
use cadforge::compiler::compile_project;
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { path } => {
            let dir = path.unwrap_or_else(|| PathBuf::from("."));
            compile_project(&dir)
        }
    }
}
