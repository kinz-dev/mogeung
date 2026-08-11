//! The run-configuration sweep says what it found, and its exit code means
//! something. `R-N1`.
//!
//! `tests/sweep.rs` is the template and the reasons carry over: the property
//! that makes a check usable is the **exit code**, because a check you have to
//! read is a check nobody runs twice.
//!
//! What is different here, and is the half worth guarding, is that this tool
//! reports two kinds of thing with two different weights. VS Code's types are a
//! **verdict** — unclassified means somebody has to decide, and the run fails.
//! IntelliJ's are an **inventory** — measured because
//! [ADR-0026](../../../docs/decisions/0026-other-peoples-run-configurations.md)
//! deferred them rather than refusing them, and a deferral with no measurement
//! behind it becomes permanent by forgetting. Confusing the two in either
//! direction breaks the tool: fail on IntelliJ and it can never pass, ignore
//! VS Code and it can never fail.

use std::path::Path;
use std::process::Command;

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-runconfigs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A repository is a directory with a `.git`, which is all the sweep looks for.
fn repo(root: &Path, name: &str) -> std::path::PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join(".git")).unwrap();
    p
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// The binary, pointed at a corpus built for the test. Roots are argv, so no
/// test-only switch is needed and the invocation is the documented one.
fn run(root: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_runconfigs"))
        .arg(root)
        .output()
        .expect("runconfigs runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

const LLDB_LAUNCH: &str = r#"{
  "version": "0.2.0",
  "configurations": [
    // The real one, from github/codex — comments and all.
    { "type": "lldb", "request": "launch", "name": "Cargo launch", },
  ]
}"#;

#[test]
fn a_corpus_of_classified_configurations_passes_quietly() {
    let root = scratch("clean");
    write(&repo(&root, "a").join(".vscode/launch.json"), LLDB_LAUNCH);

    let (ok, out) = run(&root);
    assert!(ok, "a classified corpus must exit zero:\n{out}");
    assert!(out.contains("Every configuration type on this machine is classified"), "{out}");
    // The comment and the trailing comma above are the point of this fixture:
    // if the tolerant reader regressed, the entry would vanish rather than fail.
    assert!(out.contains("lldb/launch"), "the entry must be read:\n{out}");
}

/// **The point of the tool.** A `type` nobody has decided about has to be
/// named, counted, shown with a place to look, and to fail the run.
#[test]
fn an_unclassified_type_is_named_and_fails_the_run() {
    let root = scratch("drift");
    write(
        &repo(&root, "a").join(".vscode/launch.json"),
        r#"{"configurations":[{"type":"holodeck","request":"launch","name":"Deck 5"}]}"#,
    );

    let (ok, out) = run(&root);
    assert!(!ok, "an unclassified type must fail the run:\n{out}");
    assert!(out.contains("UNCLASSIFIED"), "{out}");
    assert!(out.contains("holodeck/launch"), "it must name the type:\n{out}");
    // Named, so the decision can be made from the file rather than the count.
    assert!(out.contains("Deck 5"), "it must point at an example:\n{out}");
}

/// ADR-0026's cut, in the one place it could silently be undone.
///
/// IntelliJ's namespace is extended by every plugin — this corpus alone holds
/// `CargoCommandRunConfiguration`, `GradleRunConfiguration`, `docker-deploy`
/// and six more. If those counted as verdicts the sweep could never exit zero,
/// and the first thing anyone would do is stop running it.
#[test]
fn intellij_types_are_inventoried_and_never_fail_the_run() {
    let root = scratch("intellij");
    let r = repo(&root, "a");
    write(
        &r.join(".run/App.run.xml"),
        r#"<component name="ProjectRunConfigurationManager">
             <configuration name="App" type="Application" factoryName="Application" />
           </component>"#,
    );
    write(
        &r.join(".idea/workspace.xml"),
        r#"<project>
             <component name="RunManager" selected="Docker.thing">
               <configuration name="up" type="docker-deploy" factoryName="docker-compose.yml" />
             </component>
           </project>"#,
    );

    let (ok, out) = run(&root);
    assert!(ok, "a deferred format must not fail the run:\n{out}");
    assert!(out.contains("inventory"), "{out}");
    assert!(out.contains("Application"), "the .run type must be counted:\n{out}");
    assert!(out.contains("docker-deploy"), "RunManager's type must be counted:\n{out}");
    // Counted, never judged — an inventory row must not be reported as a
    // decision anyone has taken.
    assert!(!out.contains("UNCLASSIFIED"), "inventory is not a verdict:\n{out}");
}

/// **The number the whole pillar turns on.**
///
/// ADR-0026 chose detection over parsing because most repositories carry no
/// shared configuration at all, and said in terms that the table *"should be
/// re-measured rather than remembered"*. If this count ever stops being
/// reported, the argument for `R-N3`'s rank loses its evidence.
#[test]
fn repositories_with_nothing_are_counted_and_named() {
    let root = scratch("bare");
    write(&repo(&root, "has-one").join(".vscode/launch.json"), LLDB_LAUNCH);
    repo(&root, "bare-one");
    repo(&root, "bare-two");

    let (ok, out) = run(&root);
    assert!(ok, "{out}");
    assert!(
        out.contains("repositories with no configuration of any kind: 2 of 3"),
        "the count detection has to cover must be reported:\n{out}"
    );
    assert!(out.contains("bare-one"), "and the repos named:\n{out}");
}

/// A deleted checkout in the wastebasket answered as a repository the first
/// time this was run against a real home directory. A measurement that counts
/// your rubbish reads low on the one number it exists to produce.
#[test]
fn repositories_inside_dot_directories_are_not_counted() {
    let root = scratch("hidden");
    repo(&root, "real");
    repo(&root, ".local/share/Trash/files/deleted-checkout");

    let (_, out) = run(&root);
    assert!(out.contains("1 git repositor"), "only the live one counts:\n{out}");
    assert!(!out.contains("deleted-checkout"), "{out}");
}

/// A checkout can legitimately contain another one — `github/aeron-fork/aeron`
/// in the corpus this was built against — so finding one must not stop the walk.
#[test]
fn a_repository_nested_in_another_is_still_found() {
    let root = scratch("nested");
    let outer = repo(&root, "outer");
    std::fs::create_dir_all(outer.join("vendor/inner/.git")).unwrap();

    let (_, out) = run(&root);
    assert!(out.contains("2 git repositor"), "both must be found:\n{out}");
}
