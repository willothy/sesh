use sesh_cli::Cli;
use std::path::PathBuf;

const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const LIGHT_BLUE: &str = "\x1b[94m";

const BG_DARK_GRAY: &str = "\x1b[100m";

const RESET: &str = "\x1b[0m";

const BOLD: &str = "\x1b[1m";
const UNDERLINE: &str = "\x1b[4m";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Make sure protoc is installed
    which::which("protoc").map_err(|_|
        anyhow::anyhow!(
        "{RED}Protobuf installation not found{RESET}.

{BOLD}{GREEN}protoc{RESET} (the {BOLD}Protobuf{RESET} compiler) is required to build {GREEN}{BOLD}Sesh{RESET} from source.

Please install it, and try again. Go to {UNDERLINE}{LIGHT_BLUE}https://grpc.io/docs/protoc-installation/{RESET} for instructions.
    
With apt on Ubuntu, you can install it with:
    {BG_DARK_GRAY}\n
    sudo apt install protobuf-compiler
    {RESET}

On Arch Linux, you can install it with:
    {BG_DARK_GRAY}\n
    sudo pacman -S protobuf
    {RESET}

On MacOS, you can install it with:
    {BG_DARK_GRAY}\n
    brew install protobuf
    {RESET}
"    ))?;
    let markdown: String = clap_markdown::help_markdown::<Cli>();
    let workspace_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    std::fs::write(
        PathBuf::from(workspace_dir).join("MANUAL.md"),
        markdown.as_bytes(),
    )?;
    Ok(())
}
