//! Reads enough repository state to name HEAD without starting git.
//!
//! Exact tags use git when it is available. Anything unreadable or malformed
//! degrades to less information so the prompt always renders.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Everything the prompt shows about a repository.
#[derive(Debug, PartialEq)]
pub struct GitInfo {
    pub head: Head,
    /// A tag pointing exactly at HEAD, not the nearest one behind it.
    pub tag: Option<String>,
}

/// What HEAD points at.
#[derive(Debug, PartialEq)]
pub enum Head {
    /// The branch name, slashes kept. Set even before the first commit, when
    /// no ref file exists yet.
    Branch(String),
    /// Holds the full sha; the renderer shortens it.
    Detached(String),
}

/// The branch and exact tag for the repository containing `start`. None when
/// there is no repository above `start`, or when HEAD is unreadable or holds
/// something this does not recognize.
///
/// `start` must be a physical path. Discovery walks its ancestors, and a
/// symlinked path has different ones.
pub fn info(start: &Path) -> Option<GitInfo> {
    let git_dir = discover(start)?;
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    let head = match head.strip_prefix("ref:") {
        Some(target) => Head::Branch(branch_name(target.trim())?),
        None if is_sha(head) => Head::Detached(head.to_string()),
        None => return None,
    };
    Some(GitInfo {
        head,
        tag: exact_tag(start),
    })
}

/// The git directory owning `start`, found by walking up to the filesystem
/// root. No ceiling and no filesystem check, unlike git itself: the prompt
/// crossing a mount point is cheaper than the syscalls to notice it.
fn discover(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            return read_gitfile(&dot_git);
        }
    }
    None
}

/// A .git file holds "gitdir: <path>", written for worktrees and
/// submodules. A relative path is relative to the file's directory.
fn read_gitfile(file: &Path) -> Option<PathBuf> {
    let contents = fs::read_to_string(file).ok()?;
    let target = contents.strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }
    let target = PathBuf::from(target);
    if target.is_absolute() {
        Some(target)
    } else {
        Some(file.parent()?.join(target))
    }
}

/// A target outside refs/heads is rare. Its last component is the least
/// wrong name to show.
fn branch_name(target: &str) -> Option<String> {
    let name = match target.strip_prefix("refs/heads/") {
        Some(rest) => rest,
        None => target.rsplit('/').next().unwrap_or(target),
    };
    (!name.is_empty()).then(|| name.to_string())
}

/// The first exact tag in refname order. No git means no tag.
///
/// This is the one place that shells out. Tags can be loose files or packed,
/// and a worktree shares the main repository's refs, so reading them here
/// would mean reimplementing git's ref store. `-C start` lets git rediscover
/// the repository and handle all of it.
///
/// --sort makes the answer stable when several tags sit on HEAD, and
/// --count=1 keeps the output one line however many there are. The scan
/// stays inside git, which is why ten thousand tags still land inside the
/// bench limit.
fn exact_tag(start: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(start)
        .args([
            "for-each-ref",
            "--count=1",
            "--sort=refname",
            "--points-at",
            "HEAD",
            "--format=%(refname:strip=2)",
            "refs/tags",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .next()
        .map(String::from)
}

/// SHA-1 is the version 0.1 repository contract. Doubles as the check that
/// rejects a HEAD holding neither a ref nor an object id. A SHA-256
/// repository detaches to a 64 character id and falls out here.
fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // These build .git trees by hand instead of running git init. It keeps
    // them fast and lets them hold shapes git will not produce on demand.
    // Nothing here has a tag: exact_tag shells out to git, and these trees
    // hold no refs for it to find. Tags are covered end to end in
    // tests/cli.rs against real repositories.

    const SHA: &str = "1111111111111111111111111111111111111111";

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn basic_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".git/HEAD", "ref: refs/heads/main\n");
        dir
    }

    fn branch(info: &GitInfo) -> Option<&str> {
        match &info.head {
            Head::Branch(name) => Some(name),
            Head::Detached(_) => None,
        }
    }

    #[test]
    fn no_repository_means_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(info(dir.path()), None);
    }

    #[test]
    fn a_branch_with_no_tag() {
        let dir = basic_repo();
        let info = info(dir.path()).unwrap();
        assert_eq!(branch(&info), Some("main"));
        assert_eq!(info.tag, None);
    }

    #[test]
    fn discovery_walks_up_from_a_subdirectory() {
        let dir = basic_repo();
        let sub = dir.path().join("a/b");
        fs::create_dir_all(&sub).unwrap();
        assert_eq!(branch(&info(&sub).unwrap()), Some("main"));
    }

    #[test]
    fn a_branch_name_keeps_its_slashes() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".git/HEAD", "ref: refs/heads/feat/x\n");
        assert_eq!(branch(&info(dir.path()).unwrap()), Some("feat/x"));
    }

    // Pins the fallback in branch_name. The prompt shows a short name, never
    // the whole ref path.
    #[test]
    fn a_symref_outside_refs_heads_shows_its_last_component() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".git/HEAD", "ref: refs/something/else\n");
        assert_eq!(branch(&info(dir.path()).unwrap()), Some("else"));
    }

    // basic_repo writes HEAD and no ref file, which is exactly a fresh repo
    // before its first commit. The assertion is that naming the branch never
    // reads the ref, so the prompt works in an empty repository.
    #[test]
    fn an_unborn_branch_has_a_name_and_no_tag() {
        let dir = basic_repo();
        let info = info(dir.path()).unwrap();
        assert_eq!(branch(&info), Some("main"));
        assert_eq!(info.tag, None);
    }

    #[test]
    fn a_detached_head_keeps_the_full_sha() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".git/HEAD", &format!("{SHA}\n"));
        assert_eq!(
            info(dir.path()).unwrap().head,
            Head::Detached(SHA.to_string())
        );
    }

    // Dropping the whole git span beats printing a line of noise.
    #[test]
    fn garbage_head_means_none() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".git/HEAD", "not a head\n");
        assert_eq!(info(dir.path()), None);
    }

    #[test]
    fn a_gitfile_with_a_relative_path_is_followed() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "gitdirs/wt/HEAD", "ref: refs/heads/main\n");
        write(dir.path(), "tree/.git", "gitdir: ../gitdirs/wt\n");
        assert_eq!(
            branch(&info(&dir.path().join("tree")).unwrap()),
            Some("main")
        );
    }

    #[test]
    fn a_gitfile_with_an_absolute_path_is_followed() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "gitdirs/wt/HEAD", &format!("{SHA}\n"));
        write(
            dir.path(),
            "tree/.git",
            &format!("gitdir: {}\n", dir.path().join("gitdirs/wt").display()),
        );
        assert_eq!(
            info(&dir.path().join("tree")).unwrap().head,
            Head::Detached(SHA.to_string())
        );
    }

    // A worktree has its own HEAD under the main repository's
    // .git/worktrees/<name>. Following the gitfile is what keeps the prompt
    // from showing the main checkout's branch in every worktree.
    #[test]
    fn a_linked_worktree_reads_its_own_head() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "main/.git/worktrees/wt/HEAD",
            "ref: refs/heads/feat\n",
        );
        write(dir.path(), "wt/.git", "gitdir: ../main/.git/worktrees/wt\n");

        assert_eq!(branch(&info(&dir.path().join("wt")).unwrap()), Some("feat"));
    }
}
