mod discover;
mod edit;
mod github;
mod lint;
mod pin;
mod reference;
mod resolve;

use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report pinned and unpinned references without network access
    Lint,
    /// Preview pins for unpinned references without editing files
    Pin,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Lint => lint::run(Path::new(".")),
        Command::Pin => pin::run(Path::new(".")),
    }
}
