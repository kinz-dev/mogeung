//! The window updating itself. `R-J105`,
//! [feature 0049](../../../docs/features/0049-update-in-place.md),
//! [ADR-0041](../../../docs/decisions/0041-the-window-updates-itself-through-the-os.md).
//!
//! Four steps, each a command the window calls and none of them automatic:
//!
//! 1. **Look** (`update_check`): where is the branch this build follows, in the
//!    checkout it was installed from, and what has landed since this build?
//!    One `git rev-parse` and a `git log` — milliseconds, never a build. The
//!    window decides how often.
//! 2. **Build** (`update_build`): check the commit out in a worktree of our own
//!    — `~/.mogeung/update/tree`, detached, never the checkout agents are
//!    editing — and run `install.sh --build-only` there, as you, streaming its
//!    output. It keeps its own `target/`, so every build after the first is
//!    incremental.
//! 3. **Install** (`update_install`): `install.sh --no-build --elevate pkexec`
//!    from the same tree. The one root step is `dpkg -i`, and its password is
//!    typed into the desktop's own dialog: **this process never sees it**.
//!    On macOS nothing needs root and the script copies the app into place.
//! 4. **Restart** (`update_restart`): exit, and have a small detached waiter
//!    start the installed binary once this process is gone.
//!
//! ## Why the restart is not Tauri's
//!
//! `AppHandle::restart` spawns the new process and *then* exits. This window
//! usually hosts the daemon in-process (ADR-0009), bound to its port, so the
//! new window would come up while the old one still held it, find the port
//! taken, attach to a daemon about to die, and be left with nothing. So the
//! relaunch waits for this pid to disappear before it starts anything.
//!
//! ## What it never does
//!
//! Stop an agent: agents live in tmux, and every agent pane is a `tmux attach`
//! client, so a restart detaches and reattaches (`A46`). Restart a daemon this
//! window did not start: the window says so and leaves it (`R-J101`'s rule).
//! Hold a password: see the ADR.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// What `install.sh` records after a successful install.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstallRecord {
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub at: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub window: bool,
}

/// What the window needs to know before it looks for an update.
#[derive(Debug, Clone, Serialize)]
pub struct Info {
    /// The commit this binary was built from, or empty.
    pub commit: String,
    /// An installed build, rather than one run from a checkout's `target/` —
    /// only an installed build offers updates, since a development build is
    /// already the code.
    pub installed: bool,
    pub record: Option<InstallRecord>,
    pub check: bool,
    pub branch: String,
    pub interval: u64,
    pub source: Option<String>,
    pub platform: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

/// What the branch looks like from here.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Check {
    /// The branch's commit, when it could be read.
    pub head: Option<String>,
    /// When that commit was made, in seconds — for the window's "wait until
    /// it has been still a while".
    pub head_time: Option<i64>,
    /// What landed since this build, newest first, at most 50.
    pub commits: Vec<Commit>,
    /// How many commits behind, when this build's commit is an ancestor of the
    /// branch; `None` when it is not (a rewritten history, an unknown build).
    pub behind: Option<u32>,
    /// Why nothing could be read, in a sentence.
    pub error: Option<String>,
}

const PLATFORM: &str = if cfg!(target_os = "macos") { "macos" } else { "linux" };

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
}

fn record_path() -> PathBuf {
    home().join(".mogeung").join("install.json")
}

fn update_dir() -> PathBuf {
    home().join(".mogeung").join("update")
}

/// The worktree updates are built in.
fn tree_dir() -> PathBuf {
    update_dir().join("tree")
}

pub fn read_record(path: &Path) -> Option<InstallRecord> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// A build is *installed* when it is a release build that is not running out
/// of a `target/` directory — `target/release/mogeung-desktop` is a checkout
/// testing itself, and offering it an update would build the code it already
/// is.
pub fn is_installed(exe: &Path, debug: bool) -> bool {
    !debug && !exe.components().any(|c| c.as_os_str() == "target")
}

/// The checkout to update from: the file's say-so first, then the record.
pub fn choose_source(configured: Option<&Path>, record: Option<&InstallRecord>) -> Option<PathBuf> {
    if let Some(p) = configured {
        return Some(p.to_path_buf());
    }
    record.filter(|r| !r.source.is_empty()).map(|r| PathBuf::from(&r.source))
}

fn settings() -> (mogeung_core::config::Config, Option<InstallRecord>) {
    // The window's own update settings fall back to the defaults on a file
    // that cannot be read whole; the fault is shown where it matters, as the
    // reason the window hosts no daemon (`daemon::acquire`, ADR-0045).
    let cfg = mogeung_core::config::Config::load().unwrap_or_default();
    (cfg, read_record(&record_path()))
}

fn exe() -> PathBuf {
    tauri::utils::platform::current_exe().unwrap_or_default()
}

#[tauri::command]
pub fn update_info() -> Info {
    let (cfg, record) = settings();
    let source = choose_source(cfg.update_source.as_deref(), record.as_ref());
    Info {
        commit: mogeungd::version::COMMIT.to_string(),
        installed: is_installed(&exe(), cfg!(debug_assertions)),
        check: cfg.update_check.unwrap_or(true),
        branch: cfg.update_branch.clone().unwrap_or_else(|| "main".into()),
        interval: cfg.update_interval.unwrap_or(60).max(10),
        source: source.map(|p| p.display().to_string()),
        record,
        platform: PLATFORM,
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(mogeungd::env::which("git"))
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("PATH", mogeungd::env::path())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// `%H<tab>%s` lines, newest first.
pub fn parse_log(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|l| {
            let (sha, subject) = l.split_once('\t')?;
            (!sha.is_empty()).then(|| Commit { sha: sha.to_string(), subject: subject.to_string() })
        })
        .collect()
}

/// Where `branch` is in `source`, against `running`.
pub fn check_branch(source: &Path, branch: &str, running: &str) -> Check {
    let head = match git(source, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}^{{commit}}")]) {
        Ok(h) if !h.is_empty() => h,
        Ok(_) | Err(_) => {
            return Check {
                error: Some(format!("There is no branch {branch} in {}.", source.display())),
                ..Check::default()
            }
        }
    };
    let head_time = git(source, &["log", "-1", "--format=%ct", &head]).ok().and_then(|t| t.parse().ok());
    if head == running {
        return Check { head: Some(head), head_time, behind: Some(0), ..Check::default() };
    }
    let ancestor = !running.is_empty()
        && Command::new(mogeungd::env::which("git"))
            .arg("-C")
            .arg(source)
            .args(["merge-base", "--is-ancestor", running, &head])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    let (range, behind) = if ancestor {
        let n = git(source, &["rev-list", "--count", &format!("{running}..{head}")]).ok().and_then(|n| n.parse().ok());
        (format!("{running}..{head}"), n)
    } else {
        (head.clone(), None)
    };
    let log = git(source, &["log", "-50", "--format=%H%x09%s", &range]).unwrap_or_default();
    Check { head: Some(head), head_time, commits: parse_log(&log), behind, error: None }
}

#[tauri::command]
pub fn update_check() -> Check {
    let (cfg, record) = settings();
    let branch = cfg.update_branch.clone().unwrap_or_else(|| "main".into());
    let Some(source) = choose_source(cfg.update_source.as_deref(), record.as_ref()) else {
        return Check {
            error: Some(
                "mogeung does not know which checkout it was installed from — run ./scripts/install.sh from it once, or set update_source in ~/.mogeung/config.toml."
                    .into(),
            ),
            ..Check::default()
        };
    };
    if !source.join(".git").exists() {
        return Check { error: Some(format!("{} is not a git checkout.", source.display())), ..Check::default() };
    }
    check_branch(&source, &branch, mogeungd::version::COMMIT)
}

// --- the build and the install ----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Build,
    Install,
}

#[derive(Debug, Clone, Serialize)]
struct LogLine {
    stage: Stage,
    line: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Done {
    pub stage: Stage,
    pub ok: bool,
    pub code: Option<i32>,
    pub cancelled: bool,
    /// What went wrong, in a sentence, when something did.
    pub message: Option<String>,
}

/// The job in flight, if any: its process group, so a cancel reaches every
/// cargo and npm it started, and whether a cancel was asked for.
#[derive(Default)]
pub struct Job(Mutex<Option<(u32, bool)>>);

/// What a finished stage means, from `install.sh`'s exit status.
pub fn outcome(stage: Stage, code: Option<i32>, cancelled: bool) -> Done {
    let ok = code == Some(0) && !cancelled;
    let message = if ok {
        None
    } else if cancelled {
        Some("Cancelled — the installed version is as it was.".into())
    } else {
        Some(match (stage, code) {
            (Stage::Install, Some(3)) => "The password dialog was dismissed — nothing was installed.".into(),
            (Stage::Install, Some(4)) => "Not authorised to install — nothing was installed.".into(),
            (Stage::Build, Some(c)) => format!("The build failed (exit {c}) — the log above says why. Nothing was installed."),
            (Stage::Install, Some(c)) => format!("The install failed (exit {c}) — the log above says why."),
            (_, None) => "It was stopped before it finished.".into(),
        })
    };
    Done { stage, ok, code, cancelled, message }
}

/// The login shell's `PATH`, with the places a profile puts toolchains added.
///
/// A window started from the dock has almost no `PATH` (`R-J87`), and a build
/// needs `cargo` and `npm` — which live where *your* profile says, often a
/// version manager's directory no list could guess. So the login shell is
/// asked once, the way an editor resolves its environment, and the answer is
/// fenced with markers because a profile may print anything on the way.
fn build_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let asked = login_shell_path(&shell).unwrap_or_default();
        let base = if asked.is_empty() { mogeungd::env::path().to_string() } else { asked };
        let cargo = home().join(".cargo").join("bin").display().to_string();
        let mut dirs: Vec<String> = std::env::split_paths(&base).map(|p| p.display().to_string()).collect();
        for extra in std::iter::once(cargo).chain(std::env::split_paths(mogeungd::env::path()).map(|p| p.display().to_string())) {
            if !dirs.contains(&extra) {
                dirs.push(extra);
            }
        }
        std::env::join_paths(dirs).map(|p| p.to_string_lossy().into_owned()).unwrap_or(base)
    })
}

/// The `PATH` between the markers in a login shell's output.
pub fn fenced_path(out: &str) -> Option<String> {
    let start = out.rfind("__MOGEUNG_PATH_START__")? + "__MOGEUNG_PATH_START__".len();
    let end = out[start..].find("__MOGEUNG_PATH_END__")? + start;
    let p = out[start..end].trim();
    (!p.is_empty()).then(|| p.to_string())
}

fn login_shell_path(shell: &str) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '__MOGEUNG_PATH_START__%s__MOGEUNG_PATH_END__' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A profile that waits for input must not hang the update.
    for _ in 0..100 {
        if let Ok(Some(_)) = child.try_wait() {
            let mut out = String::new();
            std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
            return fenced_path(&out);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    None
}

/// Point the update worktree at `commit`, making it if it is not there yet.
pub fn prepare_tree(source: &Path, tree: &Path, commit: &str) -> Result<(), String> {
    // A tree deleted by hand leaves a registration behind that refuses a new
    // one at the same path.
    let _ = git(source, &["worktree", "prune"]);
    if tree.join(".git").exists() {
        git(tree, &["checkout", "--quiet", "--detach", "--force", commit])?;
    } else {
        if let Some(parent) = tree.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        git(source, &["worktree", "add", "--quiet", "--detach", &tree.display().to_string(), commit])?;
    }
    Ok(())
}

/// Run `install.sh` with `args` in the update tree, streaming every line to
/// the window and to `~/.mogeung/update/<stage>.log`, then say how it ended.
fn run_stage(app: AppHandle, job: &'static Job, stage: Stage, prepare: Option<(PathBuf, String)>, args: Vec<String>) {
    std::thread::spawn(move || {
        let dir = update_dir();
        let _ = std::fs::create_dir_all(&dir);
        let log_path = dir.join(match stage {
            Stage::Build => "build.log",
            Stage::Install => "install.log",
        });
        let mut log = std::fs::File::create(&log_path).ok();
        let mut emit = |line: String| {
            if let Some(f) = log.as_mut() {
                let _ = writeln!(f, "{line}");
            }
            let _ = app.emit("update:log", LogLine { stage, line });
        };
        let finish = |done: Done| {
            *job.0.lock().unwrap() = None;
            let _ = app.emit("update:done", done);
        };

        let tree = tree_dir();
        let cancelled = || job.0.lock().unwrap().map(|(_, c)| c).unwrap_or(false);
        if let Some((source, commit)) = prepare {
            emit(format!("▸ checking out {} in {}", &commit[..commit.len().min(7)], tree.display()));
            if let Err(e) = prepare_tree(&source, &tree, &commit) {
                emit(e.clone());
                finish(Done { stage, ok: false, code: None, cancelled: false, message: Some(format!("Could not prepare the build: {e}")) });
                return;
            }
        }

        if cancelled() {
            finish(outcome(stage, None, true));
            return;
        }
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg("exec bash scripts/install.sh \"$@\" 2>&1")
            .arg("install.sh")
            .args(&args)
            .current_dir(&tree)
            .env("PATH", build_path())
            // No colour codes in a log meant for reading.
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                finish(Done { stage, ok: false, code: None, cancelled: false, message: Some(format!("Could not start install.sh: {e}")) });
                return;
            }
        };
        {
            // A cancel asked for while the tree was being prepared is kept, and
            // acted on now that there is a process to act on.
            let mut guard = job.0.lock().unwrap();
            let asked = guard.map(|(_, c)| c).unwrap_or(false);
            *guard = Some((child.id(), asked));
            if asked {
                let _ = Command::new("kill").args(["-TERM", "--", &format!("-{}", child.id())]).status();
            }
        }
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                emit(line);
            }
        }
        let code = child.wait().ok().and_then(|s| s.code());
        finish(outcome(stage, code, cancelled()));
    });
}

fn job() -> &'static Job {
    static JOB: OnceLock<Job> = OnceLock::new();
    JOB.get_or_init(Job::default)
}

fn busy() -> Result<(), String> {
    if job().0.lock().unwrap().is_some() {
        Err("An update is already running.".into())
    } else {
        Ok(())
    }
}

/// Build `commit` in the update tree. Returns at once; `update:log` and
/// `update:done` say how it goes.
#[tauri::command]
pub fn update_build(app: AppHandle, commit: String) -> Result<(), String> {
    busy()?;
    if !commit.chars().all(|c| c.is_ascii_hexdigit()) || commit.len() < 7 {
        return Err("That is not a commit.".into());
    }
    let (cfg, record) = settings();
    let source = choose_source(cfg.update_source.as_deref(), record.as_ref())
        .ok_or("mogeung does not know which checkout it was installed from.")?;
    // Claimed before the thread starts, so two clicks cannot start two builds.
    *job().0.lock().unwrap() = Some((0, false));
    run_stage(app, job(), Stage::Build, Some((source, commit)), vec!["--build-only".into()]);
    Ok(())
}

/// Install what `update_build` built.
#[tauri::command]
pub fn update_install(app: AppHandle) -> Result<(), String> {
    busy()?;
    if !tree_dir().join("scripts").join("install.sh").exists() {
        return Err("There is nothing built to install.".into());
    }
    let args: Vec<String> = if PLATFORM == "macos" {
        vec!["--no-build".into(), "--install-app".into()]
    } else {
        vec!["--no-build".into(), "--elevate".into(), "pkexec".into()]
    };
    *job().0.lock().unwrap() = Some((0, false));
    run_stage(app, job(), Stage::Install, None, args);
    Ok(())
}

/// Stop the build or install in flight — every process it started.
#[tauri::command]
pub fn update_cancel() -> Result<(), String> {
    let mut guard = job().0.lock().unwrap();
    let Some((pid, _)) = *guard else { return Ok(()) };
    *guard = Some((pid, true));
    if pid != 0 {
        // The negative pid is the process group `process_group(0)` made.
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
    }
    Ok(())
}

/// The command that waits for `pid` to be gone and then starts `exe` — or, on
/// macOS, opens the app bundle `exe` lives in, which is how a Mac app is
/// launched properly.
pub fn relaunch_command(pid: u32, exe: &Path, macos: bool) -> Vec<String> {
    let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"));
    let start = match (macos, bundle) {
        (true, Some(app)) => format!("exec open -n \"{}\"", app.display()),
        _ => "exec \"$2\"".to_string(),
    };
    vec![
        "/bin/sh".into(),
        "-c".into(),
        format!("while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; {start}"),
        "relaunch".into(),
        pid.to_string(),
        exe.display().to_string(),
    ]
}

/// Exit, and come back as whatever is installed now.
#[tauri::command]
pub fn update_restart(app: AppHandle) -> Result<(), String> {
    let exe = exe();
    if exe.as_os_str().is_empty() {
        return Err("Could not tell which program this is, so it cannot restart itself — quit and start it again.".into());
    }
    let argv = relaunch_command(std::process::id(), &exe, PLATFORM == "macos");
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn().map_err(|e| format!("Could not start the relaunch: {e}"))?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, script: &str) {
        let ok = Command::new("sh").arg("-c").arg(script).current_dir(dir).status().unwrap().success();
        assert!(ok, "{script}");
    }

    fn repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mogeung-update-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        sh(
            &dir,
            "git init -q -b main . && git config user.email t@t && git config user.name t && \
             git commit -q --allow-empty -m one && git commit -q --allow-empty -m two",
        );
        dir
    }

    fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn a_build_run_from_target_is_not_an_installed_one() {
        assert!(is_installed(Path::new("/usr/bin/mogeung-desktop"), false));
        assert!(is_installed(Path::new("/Applications/mogeung.app/Contents/MacOS/mogeung-desktop"), false));
        assert!(!is_installed(Path::new("/home/k/mogeung/desktop/src-tauri/target/release/mogeung-desktop"), false));
        assert!(!is_installed(Path::new("/usr/bin/mogeung-desktop"), true), "a debug build is never installed");
    }

    #[test]
    fn the_file_names_the_source_first_then_the_record() {
        let r = InstallRecord { source: "/from/record".into(), ..Default::default() };
        assert_eq!(choose_source(Some(Path::new("/from/file")), Some(&r)), Some(PathBuf::from("/from/file")));
        assert_eq!(choose_source(None, Some(&r)), Some(PathBuf::from("/from/record")));
        assert_eq!(choose_source(None, Some(&InstallRecord::default())), None);
        assert_eq!(choose_source(None, None), None);
    }

    #[test]
    fn the_record_install_sh_writes_reads_back() {
        let dir = std::env::temp_dir().join(format!("mogeung-record-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("install.json");
        std::fs::write(
            &p,
            r#"{"commit":"abc","source":"/home/k/mogeung","branch":"main","at":"2026-09-24T12:00:00Z","platform":"linux","window":true}"#,
        )
        .unwrap();
        let r = read_record(&p).unwrap();
        assert_eq!((r.commit.as_str(), r.source.as_str(), r.window), ("abc", "/home/k/mogeung", true));
        std::fs::write(&p, "not json").unwrap();
        assert!(read_record(&p).is_none());
    }

    #[test]
    fn behind_a_branch_is_the_commits_since_this_build_newest_first() {
        let dir = repo("behind");
        let running = git(&dir, &["rev-parse", "HEAD~1"]).unwrap();
        sh(&dir, "git commit -q --allow-empty -m three");
        let c = check_branch(&dir, "main", &running);
        assert_eq!(c.head.as_deref(), Some(head(&dir).as_str()));
        assert_eq!(c.behind, Some(2));
        assert_eq!(c.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["three", "two"]);
        assert!(c.head_time.unwrap() > 1_700_000_000);

        let same = check_branch(&dir, "main", &head(&dir));
        assert_eq!((same.behind, same.commits.len()), (Some(0), 0));

        // A build from a commit the branch does not contain: no count, recent history shown.
        let unknown = check_branch(&dir, "main", "0123456789012345678901234567890123456789");
        assert_eq!(unknown.behind, None);
        assert_eq!(unknown.commits.len(), 3);

        let missing = check_branch(&dir, "nope", &running);
        assert!(missing.error.unwrap().contains("no branch nope"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_update_tree_is_made_once_and_moved_after() {
        let dir = repo("tree");
        let tree = dir.with_extension("tree");
        let _ = std::fs::remove_dir_all(&tree);
        let first = git(&dir, &["rev-parse", "HEAD~1"]).unwrap();
        prepare_tree(&dir, &tree, &first).unwrap();
        assert_eq!(head(&tree), first);
        let second = head(&dir);
        prepare_tree(&dir, &tree, &second).unwrap();
        assert_eq!(head(&tree), second);
        // Deleted by hand: made again rather than refused.
        std::fs::remove_dir_all(&tree).unwrap();
        prepare_tree(&dir, &tree, &first).unwrap();
        assert_eq!(head(&tree), first);
        // The checkout itself is untouched: still on main.
        assert_eq!(git(&dir, &["symbolic-ref", "--short", "HEAD"]).unwrap(), "main");
        let _ = std::fs::remove_dir_all(&tree);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_sh_exit_codes_read_as_sentences() {
        assert!(outcome(Stage::Build, Some(0), false).ok);
        assert!(outcome(Stage::Install, Some(3), false).message.unwrap().contains("dismissed"));
        assert!(outcome(Stage::Install, Some(4), false).message.unwrap().contains("Not authorised"));
        assert!(outcome(Stage::Build, Some(1), false).message.unwrap().contains("build failed"));
        let c = outcome(Stage::Build, Some(143), true);
        assert!(!c.ok && c.cancelled && c.message.unwrap().contains("Cancelled"));
    }

    #[test]
    fn the_login_shells_path_is_read_between_its_markers() {
        assert_eq!(
            fenced_path("welcome!\n__MOGEUNG_PATH_START__/a:/b__MOGEUNG_PATH_END__"),
            Some("/a:/b".to_string())
        );
        assert_eq!(fenced_path("no markers"), None);
        assert_eq!(fenced_path("__MOGEUNG_PATH_START____MOGEUNG_PATH_END__"), None);
    }

    #[test]
    fn the_relaunch_waits_for_this_process_before_starting_the_new_one() {
        let linux = relaunch_command(4242, Path::new("/usr/bin/mogeung-desktop"), false);
        assert_eq!(linux[4], "4242");
        assert!(linux[2].starts_with("while kill -0 \"$1\""));
        assert!(linux[2].ends_with("exec \"$2\""));
        assert_eq!(linux[5], "/usr/bin/mogeung-desktop");
        let mac = relaunch_command(7, Path::new("/Applications/mogeung.app/Contents/MacOS/mogeung-desktop"), true);
        assert!(mac[2].ends_with("exec open -n \"/Applications/mogeung.app\""));

        // And it really does wait: a pid that is alive holds it; one that is gone lets it through.
        let mut sleeper = Command::new("sleep").arg("1").spawn().unwrap();
        let argv = relaunch_command(sleeper.id(), Path::new("/bin/true"), false);
        let mut waiter = Command::new(&argv[0]).args(&argv[1..]).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(waiter.try_wait().unwrap().is_none(), "still waiting while the old process lives");
        let _ = sleeper.wait();
        let status = waiter.wait().unwrap();
        assert!(status.success(), "and then it started the program");
    }
}
