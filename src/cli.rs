//! The command line surface. Doc comments here are user-facing: clap prints
//! them as help text.

use clap::{Parser, Subcommand};

use crate::shell::Shell;

/// A minimalist prompt showing the user, hostname, directory, and git branch.
#[derive(Debug, Parser)]
#[command(name = "youarehere", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands. `prompt` is called by the snippet `init` emits.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the shell integration snippet for bash, zsh, or fish.
    Init {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Print one prompt line. Called by the shell hook.
    Prompt {
        /// Escape colors for this shell. Omitted means raw ANSI.
        #[arg(long, value_enum)]
        shell: Option<Shell>,
    },
}

// These cover the argument shapes the shell snippets produce. A parsing
// change that breaks one of them breaks the prompt in a live shell, where
// the failure is silent.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_requires_a_known_shell() {
        assert!(Cli::try_parse_from(["youarehere", "init"]).is_err());
        assert!(Cli::try_parse_from(["youarehere", "init", "tcsh"]).is_err());
        assert!(Cli::try_parse_from(["youarehere", "init", "bash"]).is_ok());
    }

    #[test]
    fn prompt_parses_with_and_without_a_shell() {
        assert!(Cli::try_parse_from(["youarehere", "prompt"]).is_ok());
        let cli = Cli::try_parse_from(["youarehere", "prompt", "--shell", "bash"]).expect("parse");
        match cli.command {
            Command::Prompt { shell } => assert_eq!(shell, Some(Shell::Bash)),
            other => panic!("expected prompt, got {other:?}"),
        }
    }

    #[test]
    fn prompt_rejects_unknown_shells() {
        assert!(Cli::try_parse_from(["youarehere", "prompt", "--shell", "tcsh"]).is_err());
    }
}
