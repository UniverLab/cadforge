use anyhow::Result;
use cadforge_view::run_viewer;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut path: Option<PathBuf> = None;
    let mut layer: Option<String> = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--path" | "-p" => path = iter.next().map(PathBuf::from),
            "--layer" | "-l" => layer = iter.next(),
            other if other.starts_with("--path=") => {
                path = Some(PathBuf::from(&other["--path=".len()..]));
            }
            other if other.starts_with("--layer=") => {
                layer = Some(other["--layer=".len()..].to_string());
            }
            _ => {}
        }
    }

    let dir = path.unwrap_or_else(|| PathBuf::from("."));
    run_viewer(&dir, layer.as_deref())
}
