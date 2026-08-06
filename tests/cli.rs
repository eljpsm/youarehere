//! End-to-end tests of the binary: the prompt line as each shell receives
//! it, and the init snippets.
//!
//! Small .git trees pin HEAD parsing. Real repositories cover exact tags,
//! which need git to answer.
//!
//! Every test owns a temp tree and runs the binary with a scrubbed
//! environment, so they are order independent and safe to run in parallel.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const SHA1: &str = "1111111111111111111111111111111111111111";

/// An isolated HOME-shaped directory the prompt runs inside.
struct TempTree {
    root: TempDir,
}

impl TempTree {
    /// `name` only labels the temp directory, to make a leaked one on a
    /// failing run traceable back to its test.
    fn new(name: &str) -> Self {
        let root = tempfile::Builder::new()
            .prefix(&format!("youarehere-cli-{name}-"))
            .tempdir()
            .unwrap();
        TempTree { root }
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.root.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// A working tree on branch main at SHA1, under `rel`. Hand written, so
    /// it costs no git process. Enough for anything that does not need a tag.
    fn repo(&self, rel: &str) {
        self.write(&format!("{rel}/.git/HEAD"), "ref: refs/heads/main\n");
        self.write(&format!("{rel}/.git/refs/heads/main"), &format!("{SHA1}\n"));
    }

    /// A repository git itself made, with one empty commit. Needed wherever a
    /// tag has to resolve. HOME points at the temp root throughout, so the
    /// developer's ~/.gitconfig cannot change the result.
    fn real_repo(&self, rel: &str) {
        let path = self.root.path().join(rel);
        std::fs::create_dir_all(&path).unwrap();
        let output = Command::new("git")
            .args(["init", "--quiet", "--initial-branch=main"])
            .arg(&path)
            .env("HOME", self.root.path())
            .output()
            .unwrap();
        assert_eq!(
            code(&output),
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.commit(rel, "initial");
    }

    /// Identity passed per invocation with -c. A machine running the tests
    /// need not have one configured.
    fn commit(&self, rel: &str, message: &str) {
        self.git_ok(
            rel,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                message,
            ],
        );
    }

    fn git(&self, rel: &str, args: &[&str]) -> Output {
        Command::new("git")
            .arg("-C")
            .arg(self.root.path().join(rel))
            .args(args)
            .env("HOME", self.root.path())
            .output()
            .unwrap()
    }

    /// git, asserting it worked. Setup failing silently would surface as a
    /// confusing assertion on the prompt text much later.
    fn git_ok(&self, rel: &str, args: &[&str]) -> Output {
        let output = self.git(rel, args);
        assert_eq!(
            code(&output),
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// The binary under test, run inside `rel`. HOME is the temp root, so
    /// output paths start with a tilde. PWD is removed so the syscall cwd
    /// wins over whatever shell ran the tests; NO_COLOR so the developer's
    /// setting cannot leak in.
    fn cmd(&self, rel: &str, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_youarehere"));
        cmd.args(args)
            .current_dir(self.root.path().join(rel))
            .env("HOME", self.root.path())
            .env("USER", "user")
            .env("HOSTNAME", "host")
            .env_remove("PWD")
            .env_remove("NO_COLOR");
        cmd
    }

    fn run(&self, rel: &str, args: &[&str]) -> Output {
        self.cmd(rel, args).output().unwrap()
    }

    fn prompt(&self, rel: &str) -> String {
        let output = self.run(rel, &["prompt"]);
        assert_eq!(code(&output), 0);
        stdout(&output)
    }
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The escape-free text of a prompt line, so a test asserts on what the user
/// reads rather than on a wall of escapes.
///
/// Drops ANSI up to its terminating m, plus the bash ignore bytes. It does
/// not undo zsh's %% doubling or strip %{ %}, so zsh output is checked for
/// its markers directly instead of going through here.
fn plain(prompt: &str) -> String {
    let mut out = String::new();
    let mut in_escape = false;
    for c in prompt.chars() {
        match c {
            '\x1b' => in_escape = true,
            'm' if in_escape => in_escape = false,
            _ if in_escape => {}
            '\x01' | '\x02' => {}
            _ => out.push(c),
        }
    }
    out
}

#[test]
fn outside_a_repo_the_prompt_is_the_path_alone() {
    let tree = TempTree::new("no-repo");
    std::fs::create_dir_all(tree.root.path().join("docs")).unwrap();

    let prompt = tree.prompt("docs");

    assert_eq!(plain(&prompt), "user@host ~/docs ");
    // A trailing newline would put the cursor on the line below the prompt.
    assert!(!prompt.ends_with('\n'));
    // The default output is raw ANSI, and the path color is fixed.
    assert!(prompt.contains("\x1b[1;36m"));
}

#[test]
fn a_branch_shows_in_parens() {
    let tree = TempTree::new("branch");
    tree.repo("proj");

    assert_eq!(plain(&tree.prompt("proj")), "user@host ~/proj (main) ");
}

#[test]
fn a_lightweight_tag_at_head_joins_the_branch() {
    let tree = TempTree::new("tag");
    tree.real_repo("proj");
    tree.git_ok("proj", &["tag", "v0.1.0"]);

    assert_eq!(
        plain(&tree.prompt("proj")),
        "user@host ~/proj (main v0.1.0) "
    );
}

// An annotated tag is its own object, and repacking moves it out of
// refs/tags into packed-refs. Both are why exact_tag asks git instead of
// reading the ref files.
#[test]
fn a_packed_annotated_tag_is_shown() {
    let tree = TempTree::new("annotated");
    tree.real_repo("proj");
    tree.git_ok(
        "proj",
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "tag",
            "-a",
            "v1",
            "-m",
            "v1",
        ],
    );
    tree.git_ok("proj", &["repack", "-ad"]);
    tree.git_ok("proj", &["prune-packed"]);

    assert_eq!(plain(&tree.prompt("proj")), "user@host ~/proj (main v1) ");
}

#[test]
fn a_detached_head_shows_seven_sha_characters() {
    let tree = TempTree::new("detached");
    tree.write("proj/.git/HEAD", &format!("{SHA1}\n"));

    assert_eq!(plain(&tree.prompt("proj")), "user@host ~/proj (1111111) ");
}

#[test]
fn a_detached_head_on_a_tag_shows_the_tag() {
    let tree = TempTree::new("detached-tag");
    tree.real_repo("proj");
    tree.git_ok("proj", &["tag", "v2"]);
    tree.git_ok("proj", &["checkout", "--quiet", "--detach"]);

    assert_eq!(plain(&tree.prompt("proj")), "user@host ~/proj (v2) ");
}

// A worktree keeps its HEAD in the main repository's .git and shares its
// refs. pack-refs first, since packed refs are the state a long lived
// repository settles into.
#[test]
fn a_linked_worktree_finds_branch_and_packed_tag() {
    let tree = TempTree::new("worktree");
    tree.real_repo("main");
    tree.git_ok("main", &["tag", "v1"]);
    tree.git_ok("main", &["pack-refs", "--all"]);
    tree.git_ok(
        "main",
        &["worktree", "add", "--quiet", "-b", "feat", "../wt"],
    );

    assert_eq!(plain(&tree.prompt("wt")), "user@host ~/wt (feat v1) ");
}

// Tagged in reverse order, so passing by accident is unlikely: the answer
// is the sort, not the order the refs were written.
#[test]
fn the_first_exact_tag_in_refname_order_wins() {
    let tree = TempTree::new("tag-order");
    tree.real_repo("proj");
    tree.git_ok("proj", &["tag", "zeta"]);
    tree.git_ok("proj", &["tag", "alpha"]);

    assert_eq!(
        plain(&tree.prompt("proj")),
        "user@host ~/proj (main alpha) "
    );
}

#[test]
fn a_tag_name_with_slashes_is_shown() {
    let tree = TempTree::new("tag-slash");
    tree.real_repo("proj");
    tree.git_ok("proj", &["tag", "release/1.0"]);

    assert_eq!(
        plain(&tree.prompt("proj")),
        "user@host ~/proj (main release/1.0) "
    );
}

#[test]
fn a_tag_away_from_head_is_not_shown() {
    let tree = TempTree::new("old-tag");
    tree.real_repo("proj");
    tree.git_ok("proj", &["tag", "v1"]);
    tree.commit("proj", "next");

    assert_eq!(plain(&tree.prompt("proj")), "user@host ~/proj (main) ");
}

// The scale bench/prompt.sh times, asserted for correctness here. All 10,000
// tags point at HEAD, so the answer pins that --sort orders every match
// before --count=1 truncates. Written through one update-ref --stdin; ten
// thousand git processes would dominate the suite.
#[test]
fn ten_thousand_tags_keep_the_first_exact_tag() {
    let tree = TempTree::new("many-tags");
    tree.real_repo("proj");
    let head = stdout(&tree.git_ok("proj", &["rev-parse", "HEAD"]));
    let mut child = Command::new("git")
        .arg("-C")
        .arg(tree.root.path().join("proj"))
        .args(["update-ref", "--stdin"])
        .env("HOME", tree.root.path())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    for index in 0..10_000 {
        writeln!(stdin, "create refs/tags/tag-{index:05} {}", head.trim()).unwrap();
    }
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());

    assert_eq!(
        plain(&tree.prompt("proj")),
        "user@host ~/proj (main tag-00000) "
    );
}

// git is optional. An empty PATH makes the spawn fail, which is the same
// path through the code as a machine without git installed.
#[test]
fn a_missing_git_command_only_drops_the_tag() {
    let tree = TempTree::new("missing-git");
    tree.repo("proj");

    let output = tree
        .cmd("proj", &["prompt"])
        .env("PATH", "")
        .output()
        .unwrap();

    assert_eq!(plain(&stdout(&output)), "user@host ~/proj (main) ");
}

#[test]
fn no_color_strips_every_escape() {
    let tree = TempTree::new("no-color");
    tree.repo("proj");

    let output = tree
        .cmd("proj", &["prompt"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();

    let prompt = stdout(&output);
    assert_eq!(prompt, "user@host ~/proj (main) ");
    assert!(!prompt.contains('\x1b'));
}

#[test]
fn the_bash_dialect_wraps_colors_in_ignore_bytes() {
    let tree = TempTree::new("bash");
    tree.repo("proj");

    let prompt = stdout(&tree.run("proj", &["prompt", "--shell", "bash"]));

    assert!(prompt.contains('\x01') && prompt.contains('\x02'));
    assert_eq!(plain(&prompt), "user@host ~/proj (main) ");
}

#[test]
fn the_zsh_dialect_wraps_colors_in_width_markers() {
    let tree = TempTree::new("zsh");
    tree.repo("proj");

    let prompt = stdout(&tree.run("proj", &["prompt", "--shell", "zsh"]));

    assert!(prompt.contains("%{") && prompt.contains("%}"));
}

// $PWD is the path as typed; when it and the syscall cwd name the same
// directory, the prompt shows the typed form.
#[test]
fn pwd_wins_when_it_names_the_current_directory() {
    let tree = TempTree::new("pwd");
    std::fs::create_dir_all(tree.root.path().join("real")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(tree.root.path().join("real"), tree.root.path().join("link"))
        .unwrap();
    // Nothing to test without symlinks. The early return keeps the test off
    // non-unix rather than the whole file.
    #[cfg(not(unix))]
    return;

    let output = tree
        .cmd("real", &["prompt"])
        .current_dir(tree.root.path().join("link"))
        .env("PWD", tree.root.path().join("link"))
        .output()
        .unwrap();

    assert_eq!(plain(&stdout(&output)), "user@host ~/link ");
}

// The other half of the split: the display follows the link, discovery does
// not. Walking up from ~/link would leave the repository entirely and lose
// the branch.
#[test]
fn git_discovery_uses_the_physical_path() {
    let tree = TempTree::new("physical-git");
    tree.repo("repo");
    std::fs::create_dir_all(tree.root.path().join("repo/sub")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        tree.root.path().join("repo/sub"),
        tree.root.path().join("link"),
    )
    .unwrap();
    #[cfg(not(unix))]
    return;

    let output = tree
        .cmd("repo/sub", &["prompt"])
        .current_dir(tree.root.path().join("link"))
        .env("PWD", tree.root.path().join("link"))
        .output()
        .unwrap();

    assert_eq!(plain(&stdout(&output)), "user@host ~/link (main) ");
}

#[test]
fn a_missing_user_uses_a_question_mark() {
    let tree = TempTree::new("missing-user");
    tree.repo("proj");

    let output = tree
        .cmd("proj", &["prompt"])
        .env_remove("USER")
        .output()
        .unwrap();

    assert_eq!(plain(&stdout(&output)), "?@host ~/proj (main) ");
}

#[test]
fn a_missing_hostname_uses_the_system_hostname() {
    let tree = TempTree::new("missing-hostname");
    tree.repo("proj");

    let output = tree
        .cmd("proj", &["prompt"])
        .env_remove("HOSTNAME")
        .output()
        .unwrap();

    let hostname = hostname::get().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        plain(&stdout(&output)),
        format!("user@{hostname} ~/proj (main) ")
    );
}

#[test]
fn init_prints_a_snippet_for_each_shell() {
    let tree = TempTree::new("init");
    for shell in ["bash", "zsh", "fish"] {
        let output = tree.run(".", &["init", shell]);
        assert_eq!(code(&output), 0);
        assert!(stdout(&output).contains(&format!("youarehere prompt --shell {shell}")));
    }
}

// 2 is clap's usage error. `eval "$(youarehere init tcsh)"` must not put an
// empty PS1 into a live shell.
#[test]
fn init_rejects_an_unknown_shell() {
    let tree = TempTree::new("init-unknown");
    assert_eq!(code(&tree.run(".", &["init", "tcsh"])), 2);
}
