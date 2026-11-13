mod discover;
mod edit;
mod github;
mod lint;
mod pin;
mod reference;
mod resolve;
mod write;

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
    /// Preview pins for unpinned references; edit files only with --write
    Pin {
        /// Apply the pins, saving each original file to a numbered .bkp backup first
        #[arg(long)]
        write: bool,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Lint => lint::run(Path::new(".")),
        Command::Pin { write } => pin::run(Path::new("."), write),
    }
}
