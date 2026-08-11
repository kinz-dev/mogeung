//! What run configurations actually exist on this machine. `R-N1`.
//!
//! [A32](../../../../docs/product/assumptions.md) — *reading the IDEs' run
//! configurations gives us enough to run without a format of our own* — is
//! `AT RISK`, and it got that way from a fifteen-line shell loop somebody
//! happened to write while
//! [ADR-0026](../../../../docs/decisions/0026-other-peoples-run-configurations.md)
//! was being drafted. That loop changed the design: it found **five checked-in
//! configuration files across nineteen repositories**, which turned detection
//! from a fallback into the source and demoted `launch.json` to a bonus.
//!
//! Then it was thrown away. This is that measurement kept, for exactly the
//! reason `R-J28` gives about transcripts: *"a finding that depends on somebody
//! deciding to write a script is a finding you get once."*
//!
//! ```sh
//! cargo run -q -p mogeungd --bin runconfigs            # every repo under $HOME
//! cargo run -q -p mogeungd --bin runconfigs -- ~/work  # somewhere specific
//! cargo run -q -p mogeungd --bin runconfigs -- --quiet # only what is unknown
//! ```
//!
//! **It exits non-zero when a VS Code configuration type is unclassified**, so
//! it is a check rather than a report you have to remember to read.
//!
//! # The number the whole pillar turns on
//!
//! Not the type counts — **`repositories with no configuration of any kind`**.
//! That is the population detection (`R-N3`) has to cover on its own, and if it
//! is most of them then `launch.json` was never going to carry this feature.
//! ADR-0026 reached that conclusion from one machine on one day and said in
//! terms that it *"should be re-measured rather than remembered"*. This is how.
//!
//! # It measures what it does not parse, on purpose
//!
//! IntelliJ's three sources are counted and their types inventoried, and
//! nothing here can run one. ADR-0026 deferred them rather than refusing them,
//! and `R-N12` holds the condition to take them up — *"a deferral with no
//! measurement behind it is a deferral that becomes permanent by forgetting."*
//! So they are reported as an **inventory**: named and counted, never a verdict,
//! and never a reason to exit non-zero.

use mogeungd::runconfig::{self, Source, Verdict};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// How deep to look for repositories. Six is what [`crate`]'s sibling sweep
/// uses, and it reached every repository on both machines this was run on.
const MAX_DEPTH: u8 = 6;

/// How many repository names to print per row before summarising the rest.
/// The list is evidence, not a manifest — twenty-eight paths under one heading
/// is a wall nobody reads.
const NAME_LIMIT: usize = 8;

#[derive(Default)]
struct Report {
    repos: Vec<PathBuf>,
    /// Source label → the repos carrying it.
    by_source: BTreeMap<&'static str, BTreeSet<PathBuf>>,
    /// `type/request` → how many entries, for the two files we parse.
    known: BTreeMap<String, u64>,
    unknown: BTreeMap<String, Unknown>,
    /// IntelliJ's types: counted, never judged.
    inventory: BTreeMap<String, u64>,
    /// Repos with no configuration from any source at all.
    bare: Vec<PathBuf>,
    /// Repos where `R-N3`'s detection finds nothing to offer. **This is the
    /// number the pillar is now betting on**: `bare` says how many projects a
    /// parser cannot help, and this says how many detection cannot help either
    /// — the population that would open an empty panel.
    undetected: Vec<PathBuf>,
    detected_entries: u64,
}

#[derive(Default)]
struct Unknown {
    count: u64,
    /// Where it was seen first, so the decision can be made from the file.
    example: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quiet = args.iter().any(|a| a == "--quiet");
    let roots: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    let roots = if roots.is_empty() {
        vec![PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| ".".into()),
        )]
    } else {
        roots
    };

    let mut r = Report::default();
    for root in &roots {
        find_repos(root, &mut r.repos, 0);
    }
    r.repos.sort();
    r.repos.dedup();
    for repo in r.repos.clone() {
        scan_repo(&repo, &mut r);
    }

    print(&roots, &r, quiet);
    if args.iter().any(|a| a == "--detected") {
        println!("\n=== what detection would offer ===");
        show_detected(&r.repos);
    }

    if r.unknown.is_empty() {
        println!(
            "\nEvery configuration type on this machine is classified. \
             Re-run when an IDE or a project changes."
        );
        return;
    }
    println!(
        "\n{} unclassified type(s). Each is a deliberate decision: add it to\n\
         runconfig::HANDLED if we should run it, to KNOWN_IGNORED if we should not —\n\
         and remember that an unclassified entry is still *listed* in the panel, named\n\
         as unrunnable, because a configuration silently missing reads as \"mogeung did\n\
         not find it\".",
        r.unknown.len()
    );
    std::process::exit(1);
}

/// Every directory holding a `.git`, depth-capped.
///
/// **Dot-directories are not descended into**, which is what keeps `.git`'s own
/// object store, `node_modules` caches and `~/.local/share/Trash` out of the
/// corpus. That last one is not hypothetical: a deleted checkout sitting in the
/// wastebasket answered as a repository the first time this was run, and a
/// measurement that counts your rubbish is a measurement that reads low.
///
/// The known-heavy build directories are skipped by name for speed rather than
/// correctness — a repository nested inside `target/` is not one you run.
fn find_repos(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    if depth > MAX_DEPTH {
        return;
    }
    if dir.join(".git").exists() {
        out.push(dir.to_path_buf());
        // Not returning: a checkout can legitimately contain another one, and
        // `github/aeron-fork/aeron` in this corpus is exactly that.
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        // `is_dir()` follows symlinks and would let one loop the walk.
        if !ft.is_dir() {
            continue;
        }
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        find_repos(&e.path(), out, depth + 1);
    }
}

fn scan_repo(repo: &Path, r: &mut Report) {
    let mut any = false;

    for (source, label, entries) in [
        (Source::Launch, ".vscode/launch.json", runconfig::read_launch(repo)),
        (Source::Tasks, ".vscode/tasks.json", runconfig::read_tasks(repo)),
    ] {
        if entries.is_empty() {
            continue;
        }
        any = true;
        r.by_source.entry(label).or_default().insert(repo.to_path_buf());
        for e in entries {
            let name = format!("{label} {}", e.key);
            match runconfig::classify(source, &e.key) {
                Verdict::Handled | Verdict::Ignored => *r.known.entry(name).or_default() += 1,
                Verdict::Unclassified => {
                    let u = r.unknown.entry(name).or_default();
                    u.count += 1;
                    if u.example.is_empty() {
                        u.example = format!(
                            "{} — {}",
                            repo.display(),
                            if e.name.is_empty() { &e.ty } else { &e.name }
                        );
                    }
                }
            }
        }
    }

    // IntelliJ. Measured, never parsed for running — ADR-0026, `R-N12`.
    for (label, files) in [
        (".run/*.run.xml", xml_files(&repo.join(".run"))),
        (
            ".idea/runConfigurations/*.xml",
            xml_files(&repo.join(".idea/runConfigurations")),
        ),
    ] {
        if files.is_empty() {
            continue;
        }
        any = true;
        r.by_source.entry(label).or_default().insert(repo.to_path_buf());
        for f in files {
            let Ok(body) = std::fs::read_to_string(&f) else { continue };
            for ty in xml_config_types(&body) {
                *r.inventory.entry(format!("{label} {ty}")).or_default() += 1;
            }
        }
    }

    let workspace = repo.join(".idea/workspace.xml");
    if let Ok(body) = std::fs::read_to_string(&workspace) {
        if let Some(section) = run_manager(&body) {
            any = true;
            r.by_source
                .entry(".idea/workspace.xml → RunManager")
                .or_default()
                .insert(repo.to_path_buf());
            for ty in xml_config_types(section) {
                *r.inventory
                    .entry(format!(".idea/workspace.xml {ty}"))
                    .or_default() += 1;
            }
        }
    }

    if !any {
        r.bare.push(repo.to_path_buf());
    }

    // What `R-N3` would offer here. Measured in the same pass because the two
    // numbers only mean anything beside each other: ADR-0026 moved the feature
    // onto detection, so "how many repositories carry no file" is only half the
    // argument — the other half is whether detection reaches the ones it left.
    let found = mogeungd::detect::detect(repo);
    r.detected_entries += found.len() as u64;
    if found.is_empty() {
        r.undetected.push(repo.to_path_buf());
    }
}

fn xml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "xml"))
        .collect();
    out.sort();
    out
}

/// The `<component name="RunManager">` block, or nothing.
///
/// A substring slice rather than an XML parse, deliberately: `workspace.xml` is
/// a live IDE's scratch file, frequently megabytes, and read while RustRover is
/// writing it. Nothing here depends on it being well-formed — this is an
/// inventory, and an approximate count of somebody else's private file is worth
/// more than a parser that refuses half of them.
fn run_manager(body: &str) -> Option<&str> {
    let start = body.find(r#"<component name="RunManager""#)?;
    let rest = &body[start..];
    let end = rest.find("</component>").unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Every `type="…"` on a `<configuration` element.
///
/// IntelliJ's namespace is extended by every plugin — `CargoCommandRunConfiguration`,
/// `SpringBootApplicationConfigurationType`, `docker-deploy` and
/// `MAKEFILE_TARGET_RUN_CONFIGURATION` all appear in this corpus — which is
/// exactly why ADR-0026 declined to parse it and why counting it is still worth
/// doing.
fn xml_config_types(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in body.split("<configuration").skip(1) {
        // Only the element's own attributes, not everything after it.
        let head = &chunk[..chunk.find('>').unwrap_or(chunk.len())];
        if let Some(v) = attr(head, "type") {
            out.push(v);
        }
    }
    out
}

fn attr(head: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let at = head.find(&needle)? + needle.len();
    let rest = &head[at..];
    Some(rest[..rest.find('"')?].to_string())
}

fn print(roots: &[PathBuf], r: &Report, quiet: bool) {
    println!(
        "=== Run configurations — {} ===",
        roots
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("{} git repositor(ies)", r.repos.len());

    if !r.unknown.is_empty() {
        println!("\n  UNCLASSIFIED — listed as unrunnable, and nobody has decided:");
        for (k, v) in &r.unknown {
            println!("    {:<44} {:>6}", k, v.count);
            println!("      e.g. {}", v.example);
        }
    }

    if quiet {
        return;
    }

    println!("\n  sources (the A32 table, re-measured):");
    for (label, repos) in &r.by_source {
        println!("    {:<44} {:>6}", label, repos.len());
        names(repos.iter());
    }

    if !r.known.is_empty() {
        println!("\n  classified:");
        for (k, n) in &r.known {
            println!("    {:<44} {:>6}", k, n);
        }
    }

    if !r.inventory.is_empty() {
        println!("\n  IntelliJ (inventory — measured, never parsed; R-N12):");
        for (k, n) in &r.inventory {
            println!("    {:<44} {:>6}", k, n);
        }
    }

    // **The number this tool exists for.** Detection has to cover these on its
    // own, and if it is most of them then no configuration parser was ever
    // going to carry the feature.
    let pct = if r.repos.is_empty() {
        0
    } else {
        r.bare.len() * 100 / r.repos.len()
    };
    println!(
        "\n  repositories with no configuration of any kind: {} of {} ({pct}%)",
        r.bare.len(),
        r.repos.len()
    );
    println!("    this is the population R-N3's detection has to cover alone");
    names(r.bare.iter());

    // …and whether it does.
    let covered = r.repos.len() - r.undetected.len();
    let cpct = if r.repos.is_empty() {
        0
    } else {
        covered * 100 / r.repos.len()
    };
    println!(
        "\n  detection (R-N3) offers something in: {covered} of {} ({cpct}%), {} entries in total",
        r.repos.len(),
        r.detected_entries
    );
    if r.undetected.is_empty() {
        println!("    every repository here would open a panel with something in it");
    } else {
        println!("    these would open an empty panel:");
        names(r.undetected.iter());
    }
}

/// Print what detection would actually offer, repository by repository.
///
/// ADR-0026 promoted detection to *the source* and named the price in the same
/// breath: *"a wrong inference is now a first impression"*, and **"why is
/// `cargo run -p mogeung-tray` in my list" is a question that has to have a
/// good answer.** A count cannot answer it. This is the flag that shows the
/// list you would actually be looking at, so a bad inference is something you
/// can see rather than something a user reports.
fn show_detected(repos: &[PathBuf]) {
    for repo in repos {
        let found = mogeungd::detect::detect(repo);
        if found.is_empty() {
            continue;
        }
        println!("\n  {}", repo.display());
        for c in found {
            let place = if c.dir.is_empty() { String::new() } else { format!("  ({})", c.dir) };
            println!("    {:<40}{place}", c.command_line());
        }
    }
}

fn names<'a>(mut it: impl Iterator<Item = &'a PathBuf>) {
    let shown: Vec<&PathBuf> = it.by_ref().take(NAME_LIMIT).collect();
    for p in &shown {
        println!("      {}", p.display());
    }
    let rest = it.count();
    if rest > 0 {
        println!("      … and {rest} more");
    }
}
