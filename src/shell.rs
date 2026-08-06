//! The per-shell dialects: init snippets and prompt escaping.
//!
//! Each shell measures prompt width differently. Bash needs ANSI sequences
//! wrapped in the readline ignore bytes 0x01 and 0x02, or lines redraw wrong
//! once they wrap. Zsh needs %{ %} and treats % as special everywhere in
//! PROMPT. Fish measures the prompt itself and takes raw ANSI.

use clap::ValueEnum;

/// The shells with a dialect here. Also the accepted values for `init` and
/// `--shell`: clap derives them from these variants, lowercased.
#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// The snippet `init` prints, meant for eval or source.
    pub fn snippet(self) -> &'static str {
        match self {
            // promptvars is on by default, so the substitution reruns at
            // every prompt. The single quotes keep it from running at eval
            // time. Colors carry their own ignore bytes, so no \[ \] here.
            Shell::Bash => "PS1='$(youarehere prompt --shell bash)'\n",
            // PROMPT is percent-expanded unconditionally, so the %{ %}
            // markers work without PROMPT_SUBST. Leaving PROMPT_SUBST off
            // keeps a $ or backtick in a directory name inert.
            Shell::Zsh => concat!(
                "autoload -Uz add-zsh-hook\n",
                "__youarehere_precmd() {\n",
                "    PROMPT=\"$(youarehere prompt --shell zsh)\"\n",
                "}\n",
                "add-zsh-hook precmd __youarehere_precmd\n",
            ),
            Shell::Fish => concat!(
                "function fish_prompt\n",
                "    youarehere prompt --shell fish\n",
                "end\n",
            ),
        }
    }
}

/// Dynamic text made inert under the shell's prompt expansion. Zsh expands
/// % escapes in PROMPT unconditionally. Bash never re-scans substitution
/// output. Fish prints verbatim.
pub fn escape_text(shell: Option<Shell>, text: &str) -> String {
    match shell {
        Some(Shell::Zsh) => text.replace('%', "%%"),
        _ => text.to_string(),
    }
}

/// `text` wrapped in an SGR color and the shell's zero-width markers.
pub fn color(shell: Option<Shell>, sgr: &str, text: &str) -> String {
    match shell {
        // 0x01 and 0x02 are what readline compiles \[ and \] to. Emitted
        // raw because PS1 backslash escapes are processed before command
        // substitution runs, so a literal \[ from here would print.
        Some(Shell::Bash) => format!("\x01\x1b[{sgr}m\x02{text}\x01\x1b[0m\x02"),
        Some(Shell::Zsh) => format!("%{{\x1b[{sgr}m%}}{text}%{{\x1b[0m%}}"),
        Some(Shell::Fish) | None => format!("\x1b[{sgr}m{text}\x1b[0m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_doubles_percent_and_others_do_not() {
        assert_eq!(escape_text(Some(Shell::Zsh), "50%"), "50%%");
        assert_eq!(escape_text(Some(Shell::Bash), "50%"), "50%");
        assert_eq!(escape_text(Some(Shell::Fish), "50%"), "50%");
        assert_eq!(escape_text(None, "50%"), "50%");
    }

    #[test]
    fn bash_colors_carry_readline_ignore_bytes() {
        assert_eq!(
            color(Some(Shell::Bash), "35", "x"),
            "\x01\x1b[35m\x02x\x01\x1b[0m\x02"
        );
    }

    #[test]
    fn zsh_colors_carry_width_markers() {
        assert_eq!(
            color(Some(Shell::Zsh), "35", "x"),
            "%{\x1b[35m%}x%{\x1b[0m%}"
        );
    }

    #[test]
    fn fish_and_raw_colors_are_bare_ansi() {
        assert_eq!(color(Some(Shell::Fish), "35", "x"), "\x1b[35mx\x1b[0m");
        assert_eq!(color(None, "35", "x"), "\x1b[35mx\x1b[0m");
    }

    // Each snippet must call back with its own dialect, or escaping and
    // width accounting drift apart.
    #[test]
    fn snippets_call_prompt_with_their_own_shell() {
        assert!(
            Shell::Bash
                .snippet()
                .contains("youarehere prompt --shell bash")
        );
        assert!(
            Shell::Zsh
                .snippet()
                .contains("youarehere prompt --shell zsh")
        );
        assert!(
            Shell::Fish
                .snippet()
                .contains("youarehere prompt --shell fish")
        );
    }

    #[test]
    fn the_zsh_snippet_leaves_prompt_subst_off() {
        assert!(!Shell::Zsh.snippet().contains("PROMPT_SUBST"));
    }
}
