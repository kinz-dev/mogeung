//! Folders on this machine, for choosing where a launch starts. `R-J142`.
//!
//! The New session window had the OS folder picker, which can only show the
//! disk of the computer the window runs on — so a launch on a followed
//! machine, or from a window watching a daemon on another box, was browsed on
//! the wrong one. This answers the same question from the daemon whose disk
//! it is, and the window draws the picker.
//!
//! Folder names only, never files, never their contents: a launch starts in
//! a folder, and that is all this is for. It is still a wider read than
//! anything else on the wire, which is why `api::is_write` gates it and the
//! hop needs a follow's consent to changes (`federation::hop`).

use mogeung_core::ServerMsg;
use std::path::{Path, PathBuf};

/// Past this a folder is a dump, not somewhere you start a session — and the
/// list a window would have to draw.
const CAP: usize = 2000;

/// The answer to `BrowseFolders { path }`, `home` standing in for `~`.
///
/// A path that does not exist is answered at its nearest folder that does,
/// so the half of a path you have typed opens where the typing stopped
/// making sense, not on an error. A relative one is refused: relative to
/// what would be the daemon's working directory, which nobody chose.
pub fn folders(asked: &str, home: Option<&Path>) -> ServerMsg {
    let refuse = |error: String| ServerMsg::Folders {
        machine: None,
        asked: asked.to_string(),
        path: String::new(),
        parent: None,
        dirs: Vec::new(),
        truncated: false,
        error: Some(error),
    };
    let want = match resolve(asked, home) {
        Ok(p) => p,
        Err(e) => return refuse(e),
    };
    let Some(dir) = want.ancestors().find(|a| a.is_dir()).map(Path::to_path_buf) else {
        return refuse(format!("nothing on this machine is at {}", want.display()));
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) => return refuse(format!("{}: {e}", dir.display())),
    };
    let mut dirs: Vec<String> = entries
        .flatten()
        // `metadata` follows a symlink, so a linked folder is offered as the
        // folder it is; a dangling link is not a folder.
        .filter(|e| std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort_by_key(|d| d.to_lowercase());
    let truncated = dirs.len() > CAP;
    dirs.truncate(CAP);
    ServerMsg::Folders {
        machine: None,
        asked: asked.to_string(),
        path: dir.display().to_string(),
        parent: dir.parent().map(|p| p.display().to_string()),
        dirs,
        truncated,
        error: None,
    }
}

fn resolve(asked: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let asked = asked.trim();
    let home = || home.map(Path::to_path_buf).ok_or_else(|| "this machine has no home folder set".to_string());
    if asked.is_empty() || asked == "~" {
        return home();
    }
    if let Some(rest) = asked.strip_prefix("~/") {
        return Ok(home()?.join(rest));
    }
    let p = PathBuf::from(asked);
    if p.is_absolute() {
        Ok(p)
    } else {
        Err(format!("{asked} is not an absolute path — start it with / or ~"))
    }
}

/// `$HOME`, as `state::shellexpand` reads it for the launch itself — so the
/// `~` you browse is the `~` you start in.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home of its own per test, removed when the test lets go of it.
    struct Tree(PathBuf);
    impl Tree {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tree(tag: &str) -> Tree {
        let t = Tree(std::env::temp_dir().join(format!("mogeung-browse-{tag}-{}", std::process::id())));
        let _ = std::fs::remove_dir_all(t.path());
        for d in ["projects/b", "projects/A", "projects/.hidden", "projects/c/deep"] {
            std::fs::create_dir_all(t.path().join(d)).unwrap();
        }
        std::fs::write(t.path().join("projects/notes.txt"), "x").unwrap();
        t
    }

    fn listed(msg: ServerMsg) -> (String, Option<String>, Vec<String>, Option<String>) {
        match msg {
            ServerMsg::Folders { path, parent, dirs, error, .. } => (path, parent, dirs, error),
            other => panic!("not a listing: {other:?}"),
        }
    }

    #[test]
    fn folders_only_sorted_without_case_and_hidden_ones_kept() {
        let t = tree("sorted");
        let at = t.path().join("projects");
        let (path, parent, dirs, error) = listed(folders(at.to_str().unwrap(), None));
        assert_eq!(error, None);
        assert_eq!(path, at.display().to_string());
        assert_eq!(parent.as_deref(), Some(t.path().to_str().unwrap()));
        // No `notes.txt`: a session does not start in a file. The hidden
        // folder is the window's to hide, not the daemon's.
        assert_eq!(dirs, [".hidden", "A", "b", "c"]);
    }

    #[test]
    fn tilde_and_empty_are_the_home_folder() {
        let t = tree("tilde");
        let home = Some(t.path());
        assert_eq!(listed(folders("", home)).0, t.path().display().to_string());
        assert_eq!(listed(folders("~", home)).0, t.path().display().to_string());
        assert_eq!(listed(folders("~/projects/c", home)).2, ["deep"]);
    }

    /// The half-typed path: the picker opens where the typing stops being a
    /// folder, which is the one place you can carry on from.
    #[test]
    fn a_path_that_does_not_exist_opens_at_its_nearest_folder() {
        let t = tree("nearest");
        let (path, _, dirs, error) = listed(folders("~/projects/mog", Some(t.path())));
        assert_eq!(error, None);
        assert_eq!(path, t.path().join("projects").display().to_string());
        assert!(dirs.contains(&"c".to_string()));
        // A file is not a folder either: its own folder is where you are.
        let (path, ..) = listed(folders("~/projects/notes.txt", Some(t.path())));
        assert_eq!(path, t.path().join("projects").display().to_string());
    }

    #[test]
    fn a_relative_path_or_no_home_is_refused_in_words() {
        let (_, _, _, error) = listed(folders("projects", None));
        assert!(error.unwrap().contains("not an absolute path"));
        let (_, _, _, error) = listed(folders("~", None));
        assert!(error.unwrap().contains("no home folder"));
    }

    #[test]
    fn the_root_has_no_parent() {
        assert_eq!(listed(folders("/", None)).1, None);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_is_a_folder_and_a_dangling_link_is_not() {
        let t = tree("link");
        let p = t.path().join("projects");
        std::os::unix::fs::symlink(p.join("c"), p.join("link")).unwrap();
        std::os::unix::fs::symlink(p.join("gone"), p.join("dangling")).unwrap();
        let dirs = listed(folders(p.to_str().unwrap(), None)).2;
        assert!(dirs.contains(&"link".to_string()));
        assert!(!dirs.contains(&"dangling".to_string()));
    }
}
