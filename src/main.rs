mod discover;
mod github;
mod lint;
mod reference;

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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Lint => lint::run(Path::new(".")),
    }
}
