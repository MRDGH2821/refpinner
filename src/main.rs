mod discover;
mod edit;
mod github;
mod image;
mod lint;
mod pin;
mod reference;
mod registry;
mod resolve;
mod write;

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
    Lint(discover::Scan),
    /// Preview pins for unpinned references; edit files only with --write
    Pin {
        /// Apply the pins, saving each original file to a numbered .bkp backup first
        #[arg(long)]
        write: bool,
        #[command(flatten)]
        scan: discover::Scan,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Lint(scan) => lint::run(&scan),
        Command::Pin { write, scan } => pin::run(&scan, write),
    }
}
