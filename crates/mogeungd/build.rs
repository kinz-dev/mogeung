//! Stamp the build with the commit it was built from. `R-J105`, feature 0049.
//!
//! Every build of mogeung was `0.1.0`, so nothing could say how far the window
//! in use was behind `main` — the first thing an update has to know. The
//! commit is read from git at build time and compiled in as `MOGEUNG_COMMIT`
//! (see `version.rs`); the window, which builds this crate as a dependency,
//! reads the same constant.
//!
//! **Re-run only when HEAD moves**: the `HEAD` file (a checkout, and every
//! commit in a detached worktree) and the ref it names (a commit on a branch),
//! plus `packed-refs`. Only paths that exist are named — cargo re-runs a build
//! script every time for a missing one, which would relink this crate on every
//! build of anything that depends on it.
//!
//! No git, or no repository (a source tarball): the commit is empty, and every
//! reader says *unknown* rather than failing the build.

use std::path::Path;
use std::process::Command;

fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).current_dir(dir).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn watch(dir: &str, git_path: &str) {
    if let Some(p) = git(dir, &["rev-parse", "--path-format=absolute", "--git-path", git_path]) {
        if Path::new(&p).exists() {
            println!("cargo:rerun-if-changed={p}");
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let commit = git(&dir, &["rev-parse", "HEAD"]).unwrap_or_default();
    println!("cargo:rustc-env=MOGEUNG_COMMIT={commit}");
    watch(&dir, "HEAD");
    watch(&dir, "packed-refs");
    if let Some(branch) = git(&dir, &["symbolic-ref", "-q", "HEAD"]) {
        watch(&dir, &branch);
    }
}
