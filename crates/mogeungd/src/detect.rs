//! What this project already knows how to build and test. `R-N3`.
//!
//! [ADR-0026](../../../docs/decisions/0026-other-peoples-run-configurations.md)
//! made this **the source** rather than a fallback, on a measurement rather
//! than a preference: `R-N1` found that 35 of the 58 repositories on this
//! machine carry no run configuration of any kind, and mogeung's own repository
//! is one of them. A feature resting on parsed configuration files would be
//! inert in the repository where it is developed.
//!
//! What every one of those repositories *does* have is a manifest that already
//! says how the project is built and tested. This module reads those.
//!
//! # A closed set, which is also the security argument
//!
//! Every command here is **compiled in**. A manifest supplies a *name* — a
//! workspace member, an npm script — and never a program or an argument
//! shape. That is [ADR-0025](../../../docs/decisions/0025-run-a-process-you-named-never-an-agent.md)
//! clause 1 satisfied for free: there is no file whose contents become a
//! command line, so nothing a client sends and nothing a repository contains
//! can influence what gets executed beyond choosing among these.
//!
//! It is also why detection cannot produce an agent. `npm run dev` is `npm`
//! however the script is written — and ADR-0025 is explicit that clause 2
//! checks the program, not what it goes on to do.
//!
//! # A wrong inference is a first impression
//!
//! ADR-0026 named this cost when it promoted detection, so two rules follow.
//! **`cargo run -p X` is offered only for a member that has a binary**, which
//! is the good answer to *"why is `cargo run -p mogeung-tray` in my list"* and
//! the reason `mogeung-core` is not. And **nothing is invented**: an npm script
//! is offered because a human wrote it in their own `package.json`, which makes
//! it the least inferred thing here despite arriving through a detector.

use mogeung_core::run::{Origin, RunConfig};
use std::path::{Path, PathBuf};

/// How deep to look for nested manifests.
///
/// Three, which reaches `desktop/package.json` here and the ordinary
/// `frontend/`, `server/`, `packages/x/` shapes elsewhere, without walking a
/// monorepo's every leaf. The cost of missing one is an entry you do not get;
/// the cost of walking everything is a panel that takes a second to open.
const MAX_DEPTH: u8 = 3;

/// Everything this repository could be asked to run, newest opinion first.
///
/// Ordered so the most useful entries lead: tests before builds before
/// long-running processes, and the root before its subdirectories. The panel
/// shows this order, and the first three lines are what anyone judges it by.
pub fn detect(repo: &Path) -> Vec<RunConfig> {
    let mut out = Vec::new();
    let dirs = manifest_dirs(repo);
    // **A workspace member is not its own project.** `cargo test` at the root
    // already covers every one of them, and offering each separately turned
    // this repository's list into twenty entries where four were wanted —
    // three of them for a library crate that cannot be run at all. Found by
    // the acceptance test on the first run, which is what it is for.
    let members = workspace_members(&dirs);
    for dir in &dirs {
        let rel = relative(repo, dir);
        if !members.contains(&real(dir)) {
            cargo(dir, &rel, &mut out);
        }
        npm(dir, &rel, &mut out);
        python(dir, &rel, &mut out);
        jvm(dir, &rel, &mut out);
    }
    // Two manifests can name the same command — a nested Cargo.toml inside a
    // workspace is the ordinary case — and one entry is one thing to run.
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out.sort_by_key(rank);
    out
}

/// Tests, then builds, then anything that keeps running. Within a kind, the
/// root before its subdirectories.
fn rank(c: &RunConfig) -> (u8, usize, String) {
    let line = c.command_line();
    let kind = if line.contains("test") || line.contains("check") {
        0
    } else if line.contains("build") {
        1
    } else {
        2
    };
    (kind, c.dir.matches('/').count() + usize::from(!c.dir.is_empty()), c.id.clone())
}

/// Directories worth looking in: the root, and anything below it holding a
/// manifest we understand.
fn manifest_dirs(repo: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
        if has_manifest(dir) {
            out.push(dir.to_path_buf());
        }
        if depth >= MAX_DEPTH {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut kids: Vec<PathBuf> = Vec::new();
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            // `is_dir()` follows symlinks, which would let one loop the walk.
            if !ft.is_dir() {
                continue;
            }
            let name = e.file_name();
            let name = name.to_string_lossy();
            // The build outputs, which contain manifests by the thousand and
            // nothing anybody wants to run.
            if name.starts_with('.') || matches!(&*name, "node_modules" | "target" | "build" | "dist" | "vendor" | "venv") {
                continue;
            }
            kids.push(e.path());
        }
        kids.sort();
        for k in kids {
            walk(&k, out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(repo, &mut out, 0);
    out
}

/// Every directory that is a member of some workspace we found.
///
/// Collected across all of them before anything is offered, because a member
/// can be walked before the workspace that claims it — `crates/` sorts before
/// the root's own manifest is read in any order that matters.
fn workspace_members(dirs: &[PathBuf]) -> std::collections::HashSet<PathBuf> {
    let mut out = std::collections::HashSet::new();
    for dir in dirs {
        let Ok(body) = std::fs::read_to_string(dir.join("Cargo.toml")) else { continue };
        let Ok(doc) = toml::from_str::<toml::Value>(&body) else { continue };
        let Some(ws) = doc.get("workspace") else { continue };
        // **Only the explicit list.** `members()` falls back to "the directory
        // itself" for a lone crate, which is right where it is used and wrong
        // here: an empty `[workspace]` is the ordinary way a nested crate
        // detaches from its parent — Tauri writes one into every `src-tauri` —
        // and taking the fallback made such a crate a member of itself, so it
        // excluded itself and disappeared from the panel entirely.
        let Some(list) = ws.get("members").and_then(|m| m.as_array()) else { continue };
        for m in list.iter().filter_map(|v| v.as_str()).filter(|m| !m.contains('*')) {
            out.insert(real(&dir.join(m)));
        }
    }
    out
}

/// A path two spellings of the same directory agree on.
fn real(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn has_manifest(dir: &Path) -> bool {
    ["Cargo.toml", "package.json", "pyproject.toml", "pom.xml", "build.gradle", "build.gradle.kts"]
        .iter()
        .any(|f| dir.join(f).is_file())
        || is_python_project(dir)
}

/// Is there Python here, rather than merely a directory called `tests`?
///
/// One of the declared markers, or a `tests/` directory with at least one
/// `.py` file in it. The second half is what a project with no packaging at
/// all looks like, and the marker files are what everything else looks like.
fn is_python_project(dir: &Path) -> bool {
    if ["pyproject.toml", "setup.py", "setup.cfg", "pytest.ini", "tox.ini"]
        .iter()
        .any(|f| dir.join(f).is_file())
    {
        return true;
    }
    let Ok(rd) = std::fs::read_dir(dir.join("tests")) else { return false };
    rd.flatten()
        .any(|e| e.path().extension().is_some_and(|x| x == "py"))
}

fn relative(repo: &Path, dir: &Path) -> String {
    dir.strip_prefix(repo)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn add(out: &mut Vec<RunConfig>, name: &str, program: &str, args: &[&str], dir: &str) {
    out.push(RunConfig::new(
        Origin::Detected,
        name,
        program,
        args.iter().map(|s| s.to_string()).collect(),
        dir,
    ));
}

/// `cargo test` / `cargo build`, and `cargo run -p X` per member that has a
/// binary.
fn cargo(dir: &Path, rel: &str, out: &mut Vec<RunConfig>) {
    let path = dir.join("Cargo.toml");
    let Ok(body) = std::fs::read_to_string(&path) else { return };
    let Ok(doc) = toml::from_str::<toml::Value>(&body) else {
        // A manifest we cannot read is not a reason to offer nothing: the
        // degrade-never-panic rule, applied to a format that is at least
        // specified.
        return;
    };

    let workspace = doc.get("workspace");
    if workspace.is_some() {
        add(out, "cargo test --workspace", "cargo", &["test", "--workspace"], rel);
        add(out, "cargo build", "cargo", &["build"], rel);
    } else if doc.get("package").is_some() {
        add(out, "cargo test", "cargo", &["test"], rel);
        add(out, "cargo build", "cargo", &["build"], rel);
    } else {
        return;
    }

    // `cargo run -p X`, and **only** where there is something to run. ADR-0026
    // asked for a good answer to "why is this in my list"; "it has a binary" is
    // that answer, and it is why a library-only member does not appear.
    for member in members(dir, workspace) {
        let Some(name) = package_name(&member) else { continue };
        if !has_binary(&member) {
            continue;
        }
        add(out, &format!("cargo run -p {name}"), "cargo", &["run", "-p", &name], rel);
    }
}

/// The workspace's member directories, or the crate itself.
fn members(dir: &Path, workspace: Option<&toml::Value>) -> Vec<PathBuf> {
    let Some(ws) = workspace else { return vec![dir.to_path_buf()] };
    let Some(list) = ws.get("members").and_then(|m| m.as_array()) else {
        return vec![dir.to_path_buf()];
    };
    let mut out = Vec::new();
    for m in list.iter().filter_map(|v| v.as_str()) {
        // Globs are left alone rather than half-expanded: `crates/*` is common
        // and a wrong expansion would offer a run for something that is not a
        // crate. The literal members are what this cut covers.
        if m.contains('*') {
            continue;
        }
        out.push(dir.join(m));
    }
    out
}

fn package_name(crate_dir: &Path) -> Option<String> {
    let body = std::fs::read_to_string(crate_dir.join("Cargo.toml")).ok()?;
    let doc = toml::from_str::<toml::Value>(&body).ok()?;
    Some(doc.get("package")?.get("name")?.as_str()?.to_string())
}

fn has_binary(crate_dir: &Path) -> bool {
    if crate_dir.join("src/main.rs").is_file() || crate_dir.join("src/bin").is_dir() {
        return true;
    }
    std::fs::read_to_string(crate_dir.join("Cargo.toml"))
        .ok()
        .and_then(|b| toml::from_str::<toml::Value>(&b).ok())
        .is_some_and(|d| d.get("bin").is_some())
}

/// Every script in a `package.json`.
///
/// Offered whole rather than filtered to the ones that look like tests: a
/// script is something a human wrote down in their own manifest, which makes
/// this the least inferred thing detection produces. Choosing among them for
/// them would be the wrong kind of opinion.
fn npm(dir: &Path, rel: &str, out: &mut Vec<RunConfig>) {
    let Ok(body) = std::fs::read_to_string(dir.join("package.json")) else { return };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&body) else { return };
    let Some(scripts) = doc.get("scripts").and_then(|s| s.as_object()) else { return };

    let mut names: Vec<&String> = scripts.keys().collect();
    names.sort();
    for name in names {
        // `npm test` and `npm start` are the spellings npm itself blesses and
        // the ones a person recognises; everything else needs `run`.
        let args: Vec<&str> = if name == "test" || name == "start" {
            vec![name.as_str()]
        } else {
            vec!["run", name.as_str()]
        };
        let label = format!("npm {}", args.join(" "));
        add(out, &label, "npm", &args, rel);
    }
}

/// `pytest`, from a Python project.
///
/// **A `tests/` directory is not enough on its own**, which ADR-0026's wording
/// (*"a `pyproject.toml` or a `tests/` directory"*) would allow and the first
/// run disproved: every Rust crate in this workspace has one, so `mogeungd`
/// was offered `python -m pytest`. That is exactly the wrong-inference-as-
/// first-impression the ADR warned about, caught by its own acceptance test.
/// So the directory counts only when it actually holds Python.
fn python(dir: &Path, rel: &str, out: &mut Vec<RunConfig>) {
    if !is_python_project(dir) {
        return;
    }
    // `python -m pytest` rather than a bare `pytest`, so the interpreter on
    // `PATH` decides — a bare `pytest` resolves to whichever virtualenv was
    // active when it was installed, which is rarely the one the project wants.
    add(out, "python -m pytest", "python", &["-m", "pytest"], rel);
}

/// Gradle or Maven, preferring the wrapper the project checked in.
///
/// The wrapper is the point: a project that ships `./gradlew` has pinned its
/// build tool version, and running the machine's `gradle` against it is how you
/// get an error nobody can reproduce.
fn jvm(dir: &Path, rel: &str, out: &mut Vec<RunConfig>) {
    if dir.join("gradlew").is_file() {
        add(out, "./gradlew test", "./gradlew", &["test"], rel);
        add(out, "./gradlew build", "./gradlew", &["build"], rel);
    } else if dir.join("build.gradle").is_file() || dir.join("build.gradle.kts").is_file() {
        add(out, "gradle test", "gradle", &["test"], rel);
    }

    if dir.join("mvnw").is_file() {
        add(out, "./mvnw test", "./mvnw", &["test"], rel);
    } else if dir.join("pom.xml").is_file() {
        add(out, "mvn test", "mvn", &["test"], rel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        // `CARGO_MANIFEST_DIR` is `crates/mogeungd`.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
    }

    fn lines(cs: &[RunConfig]) -> Vec<String> {
        cs.iter()
            .map(|c| {
                if c.dir.is_empty() {
                    c.command_line()
                } else {
                    format!("{} ({})", c.command_line(), c.dir)
                }
            })
            .collect()
    }

    /// **The acceptance criterion, against this repository itself.**
    ///
    /// Feature 0035 asks for exactly this — *"with no configuration file
    /// anywhere, opening mogeung on this repository offers `cargo test
    /// --workspace`, `cargo build`, and `npm test` in `desktop/`"* — and
    /// mogeung carries none of the files `R-N1` looks for, which is precisely
    /// why ADR-0026 promoted detection.
    #[test]
    fn mogeung_can_run_its_own_suite_with_no_configuration_file() {
        let got = lines(&detect(&repo_root()));
        assert!(got.iter().any(|l| l == "cargo test --workspace"), "{got:#?}");
        assert!(got.iter().any(|l| l == "cargo build"), "{got:#?}");
        assert!(got.iter().any(|l| l == "npm test (desktop)"), "{got:#?}");
    }

    /// ADR-0026's *"why is `cargo run -p mogeung-tray` in my list"* — the
    /// question it said had to have a good answer. The answer is that it has a
    /// binary, and this pins the other half: a library-only member has none, so
    /// it is not offered.
    #[test]
    fn only_a_member_with_a_binary_is_offered_a_run() {
        let got = lines(&detect(&repo_root()));
        assert!(got.iter().any(|l| l == "cargo run -p mogeungd"), "{got:#?}");
        assert!(got.iter().any(|l| l == "cargo run -p mogeung-tray"), "{got:#?}");
        assert!(
            !got.iter().any(|l| l.contains("mogeung-core")),
            "a library has nothing to run:\n{got:#?}"
        );
    }

    /// Tests lead. The first lines are what anyone judges the panel by, and
    /// `R-N7`'s whole point is putting a run in front of a claim about tests.
    #[test]
    fn the_test_commands_come_first() {
        let got = detect(&repo_root());
        let first = got.first().expect("something is detected");
        assert!(
            first.command_line().contains("test") || first.command_line().contains("check"),
            "got {first:?}"
        );
    }

    /// Every entry says it was inferred, because ADR-0026 made detection the
    /// first thing anyone sees and a guess presented as a fact is the failure
    /// it warned about.
    #[test]
    fn everything_detected_is_labelled_inferred() {
        for c in detect(&repo_root()) {
            assert_eq!(c.origin, Origin::Detected);
            assert_eq!(c.origin.label(), "inferred");
            assert!(c.runnable(), "a detected entry is always runnable: {c:?}");
        }
    }

    /// ADR-0025 clause 1 is satisfied by construction, and this is the test
    /// that would catch it being loosened: every program is from the closed
    /// set, whatever the manifests happen to contain.
    #[test]
    fn every_detected_program_is_from_the_compiled_in_set() {
        const CLOSED: &[&str] =
            &["cargo", "npm", "python", "./gradlew", "gradle", "./mvnw", "mvn"];
        for c in detect(&repo_root()) {
            assert!(CLOSED.contains(&c.program.as_str()), "unexpected program: {c:?}");
            assert!(!mogeung_core::run::is_agent(&c.program), "{c:?}");
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-detect-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// The population this feature exists for: `R-N1` found 35 of 58
    /// repositories carrying no configuration file at all. A bare npm project
    /// has to come out with something usable.
    #[test]
    fn a_project_with_only_a_package_json_still_gets_entries() {
        let d = scratch("npm");
        write(&d, "package.json", r#"{"scripts":{"test":"vitest","dev":"vite","build":"vite build"}}"#);
        let got = lines(&detect(&d));
        assert!(got.contains(&"npm test".to_string()), "{got:#?}");
        assert!(got.contains(&"npm run dev".to_string()), "{got:#?}");
        assert!(got.contains(&"npm run build".to_string()), "{got:#?}");
    }

    /// `node_modules` holds thousands of `package.json` files and nothing
    /// anybody wants to run. Without this the panel would be unusable in every
    /// JavaScript project — which is most of them.
    #[test]
    fn manifests_inside_build_output_are_not_offered() {
        let d = scratch("skip");
        write(&d, "package.json", r#"{"scripts":{"test":"vitest"}}"#);
        write(&d, "node_modules/left-pad/package.json", r#"{"scripts":{"test":"nope"}}"#);
        write(&d, "target/debug/package.json", r#"{"scripts":{"test":"nope"}}"#);
        let got = detect(&d);
        assert_eq!(got.len(), 1, "only the project's own manifest:\n{:#?}", lines(&got));
    }

    /// A manifest that does not parse costs its entries and nothing else — the
    /// degrade-never-panic rule, on a format that at least has a specification.
    #[test]
    fn a_broken_manifest_does_not_cost_the_rest() {
        let d = scratch("broken");
        write(&d, "Cargo.toml", "this is not toml [[[");
        write(&d, "desktop/package.json", r#"{"scripts":{"test":"vitest"}}"#);
        let got = lines(&detect(&d));
        assert_eq!(got, vec!["npm test (desktop)".to_string()]);
    }

    /// A wrapper means the project pinned its build tool, and running the
    /// machine's own is how you get an error nobody can reproduce.
    #[test]
    fn a_gradle_wrapper_wins_over_the_machines_gradle() {
        let d = scratch("gradle");
        write(&d, "build.gradle", "");
        write(&d, "gradlew", "#!/bin/sh\n");
        let got = lines(&detect(&d));
        assert!(got.contains(&"./gradlew test".to_string()), "{got:#?}");
        assert!(!got.iter().any(|l| l == "gradle test"), "{got:#?}");
    }

    /// A workspace member is covered by `cargo test` at the root, so offering
    /// it again under its own directory is noise — twenty entries where four
    /// were wanted, three of them for a crate that cannot be run.
    #[test]
    fn a_workspace_member_is_not_offered_as_its_own_project() {
        let d = scratch("members");
        write(&d, "Cargo.toml", "[workspace]\nmembers = [\"crates/a\"]\n");
        write(&d, "crates/a/Cargo.toml", "[package]\nname = \"a\"\n");
        write(&d, "crates/a/src/main.rs", "fn main() {}");
        let got = lines(&detect(&d));
        assert!(got.contains(&"cargo test --workspace".to_string()), "{got:#?}");
        assert!(got.contains(&"cargo run -p a".to_string()), "{got:#?}");
        assert!(
            !got.iter().any(|l| l.contains("(crates/a)")),
            "the member must not be offered separately:\n{got:#?}"
        );
    }

    /// **An empty `[workspace]` is a crate detaching from its parent**, which
    /// is what Tauri writes into every `src-tauri` — and reading it as "a
    /// workspace whose sole member is itself" made the crate exclude itself and
    /// vanish. Caught here because `desktop/src-tauri` disappeared from this
    /// repository's own list between two runs.
    #[test]
    fn a_crate_that_detaches_with_an_empty_workspace_still_appears() {
        let d = scratch("detached");
        write(&d, "Cargo.toml", "[workspace]\nmembers = [\"crates/a\"]\n");
        write(&d, "crates/a/Cargo.toml", "[package]\nname = \"a\"\n");
        write(&d, "nested/Cargo.toml", "[workspace]\n\n[package]\nname = \"nested\"\n");
        write(&d, "nested/src/main.rs", "fn main() {}");

        let got = lines(&detect(&d));
        assert!(
            got.iter().any(|l| l == "cargo build (nested)"),
            "a detached crate is its own project:\n{got:#?}"
        );
    }

    /// Nothing to detect is an ordinary answer and must be empty rather than
    /// anything that looks like a failure.
    #[test]
    fn a_directory_with_no_manifest_yields_nothing() {
        let d = scratch("empty");
        std::fs::create_dir_all(d.join("src")).unwrap();
        assert!(detect(&d).is_empty());
    }
}

