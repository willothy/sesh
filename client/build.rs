use std::path::PathBuf;

use sesh_cli::Cli;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let markdown: String = clap_markdown::help_markdown::<Cli>();
    let workspace_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    std::fs::write(
        PathBuf::from(workspace_dir).join("../MANUAL.md"),
        markdown.as_bytes(),
    )?;
    Ok(())
}
