//! youarehere prints a one-line shell prompt: who and where you are, plus the
//! git branch and tag when you are in a repository.
//!
//! The pipeline: cli parses the subcommand, app renders the line, path finds
//! both directory identities, git reads HEAD and asks git for exact tags, shell
//! owns the per-shell dialects.

mod app;
mod cli;
mod git;
mod path;
mod shell;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    app::run(cli::Cli::parse())
}
