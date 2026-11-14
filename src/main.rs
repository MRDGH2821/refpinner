mod discover;
mod edit;
mod github;
mod gitlab;
mod gitlab_resolve;
mod image;
mod lint;
mod pin;
mod reference;
mod registry;
mod resolve;
mod settings;
mod update;
mod version;
mod write;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(flatten)]
    actions: settings::Actions,
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
    /// Preview updates of pinned references within version limits; edit files only with --write
    #[command(group = clap::ArgGroup::new("limit").args(["major", "minor", "patch"]))]
    Update {
        /// Allow updates to a new major version, moving the track with it
        #[arg(long)]
        major: bool,
        /// Stay within the current major version (the default)
        #[arg(long)]
        minor: bool,
        /// Stay within the current minor version
        #[arg(long)]
        patch: bool,
        /// Admit all prereleases as well as stable releases
        #[arg(long, conflicts_with = "rc")]
        pre: bool,
        /// Admit release candidates as well as stable releases
        #[arg(long)]
        rc: bool,
        /// Follow the default branch for pins without an annotation
        #[arg(long)]
        follow_default_branch: bool,
        /// Fail instead of updating, because updates need network access
        #[arg(long)]
        offline: bool,
        /// Apply the updates, saving each original file to a numbered .bkp backup first
        #[arg(long)]
        write: bool,
        #[command(flatten)]
        scan: discover::Scan,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Lint(scan) => lint::run(&scan),
        Command::Pin { write, scan } => pin::run(&scan, write, &cli.actions),
        Command::Update { offline: true, .. } => {
            lint::fail("update needs network access and cannot run offline")
        }
        Command::Update {
            major,
            patch,
            pre,
            rc,
            follow_default_branch,
            write,
            scan,
            ..
        } => {
            let limit = match (major, patch) {
                (true, _) => version::Limit::Major,
                (_, true) => version::Limit::Patch,
                _ => version::Limit::Minor,
            };
            let releases = match (pre, rc) {
                (true, _) => version::Releases::Pre,
                (_, true) => version::Releases::Rc,
                _ => version::Releases::Stable,
            };
            update::run(
                &scan,
                write,
                limit,
                releases,
                follow_default_branch,
                &cli.actions,
            )
        }
    }
}
