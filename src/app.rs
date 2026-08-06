//! Subcommand dispatch and prompt rendering. The one place that prints.
//!
//! Rendering never fails. Each layer degrades instead: an unreadable repo
//! drops the git span, an unreadable cwd falls back to "?". A prompt that
//! prints errors at every keystroke is broken software.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::cli::{Cli, Command};
use crate::shell::{self, Shell};
use crate::{git, path};

/// Path in bold cyan, git span in magenta. Fixed on purpose.
const PATH_SGR: &str = "1;36";
const GIT_SGR: &str = "35";

/// Runs the parsed command. The exit code is always success. A shell hook has
/// nothing useful to do with a failure, and a nonzero status would show up in
/// the next `$?`.
pub fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Command::Init { shell } => print!("{}", shell.snippet()),
        Command::Prompt { shell } => print!("{}", prompt(shell)),
    }
    ExitCode::SUCCESS
}

/// The full prompt line: identity, path, optional git span, one trailing space.
fn prompt(shell: Option<Shell>) -> String {
    // https://no-color.org: any nonempty value disables color.
    let color = std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
    let cwd = path::current();
    // Discovery walks up from the physical path, so a symlinked cwd finds the
    // repository it actually lives in rather than the one above the link.
    let git = cwd.as_ref().and_then(|cwd| git::info(&cwd.physical));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let user = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "?".to_string());
    let hostname = hostname(std::env::var_os("HOSTNAME"), hostname::get().ok());
    let display = match &cwd {
        Some(cwd) => path::display(&cwd.logical, home.as_deref()),
        None => "?".to_string(),
    };
    render(&user, &hostname, &display, git.as_ref(), shell, color)
}

/// The environment wins. An exported HOSTNAME is the name the shell was given,
/// which is the one the user recognizes when the system name differs, as it
/// does inside a container. "?" when neither answers.
fn hostname(environment: Option<OsString>, system: Option<OsString>) -> String {
    [environment, system]
        .into_iter()
        .flatten()
        .filter_map(|hostname| hostname.into_string().ok())
        .find(|hostname| !hostname.is_empty())
        .unwrap_or_else(|| "?".to_string())
}

/// Assembles the line from parts already resolved. Taking every input as an
/// argument keeps it free of the environment, so the tests below can pin the
/// exact bytes.
fn render(
    user: &str,
    hostname: &str,
    path: &str,
    git: Option<&git::GitInfo>,
    shell: Option<Shell>,
    color: bool,
) -> String {
    let mut line = paint(shell, PATH_SGR, &format!("{user}@{hostname} {path}"), color);
    if let Some(info) = git {
        line.push(' ');
        line.push_str(&paint(
            shell,
            GIT_SGR,
            &format!("({})", git_span(info)),
            color,
        ));
    }
    line.push(' ');
    line
}

/// One colored span. Escaping runs first, over the dynamic text alone.
/// Escaping the colored string instead would double the percent signs in zsh's
/// own %{ %} markers and print them.
fn paint(shell: Option<Shell>, sgr: &str, text: &str, color: bool) -> String {
    let text = shell::escape_text(shell, text);
    if color {
        shell::color(shell, sgr, &text)
    } else {
        text
    }
}

/// "main v0.1.0", "main", "v1.2.0", or "3f2a1b9". A detached HEAD sitting
/// on a tag is named by the tag; the sha adds nothing.
fn git_span(info: &git::GitInfo) -> String {
    match (&info.head, &info.tag) {
        (git::Head::Branch(branch), Some(tag)) => format!("{branch} {tag}"),
        (git::Head::Branch(branch), None) => branch.clone(),
        (git::Head::Detached(_), Some(tag)) => tag.clone(),
        (git::Head::Detached(sha), None) => sha.chars().take(7).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{GitInfo, Head};

    fn on_branch(branch: &str, tag: Option<&str>) -> GitInfo {
        GitInfo {
            head: Head::Branch(branch.to_string()),
            tag: tag.map(String::from),
        }
    }

    #[test]
    fn path_only_outside_a_repo() {
        assert_eq!(
            render("user", "host", "~/docs", None, None, false),
            "user@host ~/docs "
        );
    }

    #[test]
    fn hostname_prefers_the_environment() {
        assert_eq!(
            hostname(
                Some(OsString::from("shell")),
                Some(OsString::from("system"))
            ),
            "shell"
        );
    }

    #[test]
    fn hostname_falls_back_to_the_system() {
        assert_eq!(hostname(None, Some(OsString::from("system"))), "system");
        assert_eq!(hostname(None, None), "?");
    }

    #[test]
    fn branch_and_tag_share_the_parens() {
        let info = on_branch("main", Some("v0.1.0"));
        assert_eq!(
            render("user", "host", "~", Some(&info), None, false),
            "user@host ~ (main v0.1.0) "
        );
    }

    // The reset after each span is what keeps the color from bleeding into
    // whatever the user types next.
    #[test]
    fn colors_wrap_the_path_and_the_whole_git_span() {
        let info = on_branch("main", None);
        assert_eq!(
            render("user", "host", "~", Some(&info), None, true),
            "\x1b[1;36muser@host ~\x1b[0m \x1b[35m(main)\x1b[0m "
        );
    }

    #[test]
    fn detached_head_shows_seven_sha_characters() {
        let info = GitInfo {
            head: Head::Detached("0123456789abcdef0123456789abcdef01234567".to_string()),
            tag: None,
        };
        assert_eq!(git_span(&info), "0123456");
    }

    #[test]
    fn detached_head_prefers_the_tag() {
        let info = GitInfo {
            head: Head::Detached("0123456789abcdef0123456789abcdef01234567".to_string()),
            tag: Some("v1.2.0".to_string()),
        };
        assert_eq!(git_span(&info), "v1.2.0");
    }

    // A directory named 1% must not become a prompt escape in zsh.
    #[test]
    fn zsh_text_is_percent_escaped_even_without_color() {
        assert_eq!(
            render("u%", "h%", "~/1%", None, Some(Shell::Zsh), false),
            "u%%@h%% ~/1%% "
        );
    }
}
