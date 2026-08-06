//! Where you are. $PWD when it names the current directory, so a path
//! entered through a symlink displays as typed; the syscall answer otherwise.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The two identities of one directory. They differ only when the user
/// reached it through a symlink.
pub struct Current {
    /// The kernel's answer, symlinks resolved. What git discovery walks up.
    pub physical: PathBuf,
    /// The path as typed. What gets displayed.
    pub logical: PathBuf,
}

/// Both identities of the current directory. None when the current directory
/// is gone or unreadable, which the caller renders as "?".
pub fn current() -> Option<Current> {
    let physical = env::current_dir().ok()?;
    // PWD is inherited, so it can be stale or belong to another directory
    // entirely. It is only trusted after it checks out against the syscall.
    if let Some(pwd) = env::var_os("PWD").map(PathBuf::from) {
        if pwd.is_absolute() && same_dir(&pwd, &physical) {
            return Some(Current {
                physical,
                logical: pwd,
            });
        }
    }
    Some(Current {
        logical: physical.clone(),
        physical,
    })
}

/// Two paths name the same directory when their canonical forms agree.
/// An unreadable path is not a match. A wrong no only loses the symlink
/// spelling. A wrong yes displays a directory you are not in.
fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Render with the home directory abbreviated to ~. The prefix match works
/// on components, so a sibling like /home/userx keeps its full path.
pub fn display(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home {
        if let Ok(rest) = path.strip_prefix(home) {
            return if rest.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", rest.display())
            };
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Option<&'static Path> {
        Some(Path::new("/home/user"))
    }

    #[test]
    fn home_itself_is_a_tilde() {
        assert_eq!(display(Path::new("/home/user"), home()), "~");
    }

    #[test]
    fn a_subdirectory_hangs_off_the_tilde() {
        assert_eq!(display(Path::new("/home/user/src"), home()), "~/src");
    }

    // The regression a string prefix check would introduce.
    #[test]
    fn a_sibling_of_home_is_not_abbreviated() {
        assert_eq!(
            display(Path::new("/home/userx/src"), home()),
            "/home/userx/src"
        );
    }

    #[test]
    fn no_home_means_the_full_path() {
        assert_eq!(display(Path::new("/home/user"), None), "/home/user");
    }

    #[test]
    fn root_stays_root() {
        assert_eq!(display(Path::new("/"), home()), "/");
    }
}
