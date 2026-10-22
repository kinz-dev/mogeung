//! Running the Ask agent. `R-O14`,
//! [ADR-0046](../../../docs/decisions/0046-an-ask-may-be-answered-by-a-sandboxed-agent.md).
//!
//! The policy is in `mogeung_core::askagent`: the argv, the config, the
//! parser. What is here finds the two binaries, checks that a namespace will
//! start, and runs one ask.
//!
//! **Fail closed** (clause 5). Every way this can find itself unable to build
//! the sandbox is an [`Unavailable`], and the caller answers as plain chat.
//! Nothing here has a path that starts opencode without `bwrap` in front of it.

use mogeung_core::askagent::{self as policy, AgentStep, Event, Sandbox, Unavailable};
use mogeung_core::model::ChatTurn;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

/// Everything needed to run an ask, found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub sandbox: Sandbox,
    /// opencode's own `--version`, for the line under the answer.
    pub version: String,
}

impl Ready {
    pub fn provenance(&self) -> String {
        policy::provenance(&self.version)
    }
}

/// Can an ask be answered by the agent here, now?
///
/// Blocking: it starts `bwrap` once, and on a binary it has not seen,
/// `opencode --version`. Called per ask on a blocking thread, because
/// installing `bubblewrap` should not need a daemon restart to be noticed.
pub fn probe(enabled: bool) -> Result<Ready, Unavailable> {
    if !enabled {
        return Err(Unavailable::Off);
    }
    if !cfg!(target_os = "linux") {
        return Err(Unavailable::Platform);
    }
    let bwrap = crate::env::which("bwrap");
    if !Path::new(&bwrap).is_file() {
        return Err(Unavailable::NoBwrap);
    }
    namespace_starts(&bwrap)?;
    let found = crate::state::opencode_binary();
    let opencode = std::fs::canonicalize(&found).map_err(|_| Unavailable::NoOpencode)?;
    if !opencode.is_file() {
        return Err(Unavailable::NoOpencode);
    }
    Ok(Ready {
        sandbox: Sandbox {
            sized: sized(&bwrap),
            bwrap,
            opencode: opencode.to_string_lossy().into_owned(),
            keep: keep(),
        },
        version: version(&opencode),
    })
}

/// A namespace that will not start is the AppArmor case on Ubuntu 23.10 and
/// later, or user namespaces switched off. `bwrap`'s own words say which.
fn namespace_starts(bwrap: &str) -> Result<(), Unavailable> {
    let out = std::process::Command::new(bwrap)
        .args(["--ro-bind", "/", "/", "--unshare-pid", "--new-session", "--die-with-parent", "--", "true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Unavailable::BwrapFails(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let line = said.lines().find(|l| !l.trim().is_empty()).unwrap_or("it exited with an error");
    Err(Unavailable::BwrapFails(line.trim().trim_start_matches("bwrap: ").to_string()))
}

/// `--size` arrived in bubblewrap 0.10.
fn sized(bwrap: &str) -> bool {
    let Ok(out) = std::process::Command::new(bwrap).arg("--version").output() else {
        return false;
    };
    sized_from(&String::from_utf8_lossy(&out.stdout))
}

/// `bubblewrap 0.11.1` → yes. Anything unreadable is no: an uncapped tmpfs
/// still works, while a flag `bwrap` does not know stops the ask.
pub fn sized_from(version: &str) -> bool {
    let v = version.split_whitespace().last().unwrap_or("");
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(major), Some(minor)) => major > 0 || minor >= 10,
        _ => false,
    }
}

/// Where `/etc/resolv.conf` really is, when that is under a path the sandbox
/// hides: `/run/systemd/resolve` with systemd-resolved, `/run/NetworkManager`
/// or `/run/resolvconf` elsewhere. Without it the agent has no DNS.
fn keep() -> Vec<String> {
    let Ok(real) = std::fs::canonicalize("/etc/resolv.conf") else {
        return Vec::new();
    };
    keep_for(&real)
}

pub fn keep_for(resolv: &Path) -> Vec<String> {
    let hidden = policy::HIDDEN.iter().any(|h| resolv.starts_with(h));
    match resolv.parent().filter(|_| hidden) {
        Some(dir) => vec![dir.to_string_lossy().into_owned()],
        None => Vec::new(),
    }
}

/// `opencode --version`, remembered per binary and modification time: it
/// costs a second of start-up, and the answer changes only on an upgrade.
fn version(bin: &Path) -> String {
    static SEEN: OnceLock<Mutex<HashMap<(PathBuf, Option<SystemTime>), String>>> = OnceLock::new();
    let key = (bin.to_path_buf(), std::fs::metadata(bin).and_then(|m| m.modified()).ok());
    let seen = SEEN.get_or_init(Default::default);
    if let Some(v) = seen.lock().expect("version lock").get(&key) {
        return v.clone();
    }
    let v = std::process::Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| v.len() <= 40 && !v.contains('\n'))
        .unwrap_or_default();
    seen.lock().expect("version lock").insert(key, v.clone());
    v
}

/// What one ask came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub text: String,
    pub steps: Vec<AgentStep>,
    /// Ended by the stop bell. `text` is what had arrived.
    pub stopped: bool,
}

/// Today, for the agent's instructions, in the daemon's own time zone: the
/// date the person asking would say.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d (%A)").to_string()
}

/// Run one ask. `on_text` gets each new part of the answer, already prefixed
/// with the blank line that joins it to the last; `on_step` gets each tool
/// step as it lands. Neither is called after this returns.
///
/// `Err` is a sentence for a human. Stopping is not an error.
pub async fn run(
    ready: &Ready,
    base_url: &str,
    model: Option<&str>,
    turns: &[ChatTurn],
    on_text: &mut (dyn FnMut(String) + Send),
    on_step: &mut (dyn FnMut(AgentStep) + Send),
    stop: Option<&Notify>,
) -> Result<Outcome, String> {
    let config = policy::opencode_config(base_url, model, &today()).to_string();
    let argv = policy::sandbox_argv(&ready.sandbox, &config);
    run_argv(&argv, &policy::render_message(turns), Duration::from_secs(policy::TIMEOUT_SECS), on_text, on_step, stop).await
}

/// [`run`] on an argv already built. Separate so the tests can put a fake
/// opencode inside the real sandbox.
pub async fn run_argv(
    argv: &[String],
    message: &str,
    limit: Duration,
    on_text: &mut (dyn FnMut(String) + Send),
    on_step: &mut (dyn FnMut(AgentStep) + Send),
    stop: Option<&Notify>,
) -> Result<Outcome, String> {
    let (bin, args) = argv.split_first().ok_or("nothing to run")?;
    let mut child = tokio::process::Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The stop bell and the time limit both end here, and a dropped future
        // (the socket went away) must not leave an agent running.
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start the sandbox: {e}"))?;

    // On stdin, and from a task of its own: a long thread is more than a pipe
    // holds, and opencode reads it only once it has started.
    let mut stdin = child.stdin.take().ok_or("the sandbox has no stdin")?;
    let message = message.to_string();
    tokio::spawn(async move {
        let _ = stdin.write_all(message.as_bytes()).await;
        let _ = stdin.shutdown().await;
    });
    let mut stderr = child.stderr.take().ok_or("the sandbox has no stderr")?;
    let said = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = (&mut stderr).take(16 * 1024).read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });
    let mut lines = BufReader::new(child.stdout.take().ok_or("the sandbox has no stdout")?).lines();

    let deadline = tokio::time::Instant::now() + limit;
    let started = Instant::now();
    let mut parts: Vec<String> = Vec::new();
    let mut steps = Vec::new();
    let mut error: Option<String> = None;
    let mut stopped = false;
    let never = Notify::new();
    let bell = stop.unwrap_or(&never);
    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(l)) => match policy::parse_line(&l) {
                    Some(Event::Text(t)) => {
                        on_text(if parts.is_empty() { t.clone() } else { format!("\n\n{t}") });
                        parts.push(t);
                    }
                    Some(Event::Step(s)) => {
                        on_step(s.clone());
                        steps.push(s);
                    }
                    Some(Event::Error(e)) => error = Some(e),
                    None => {}
                },
                // EOF, or a read error, which ends the stream the same way.
                Ok(None) | Err(_) => break,
            },
            _ = bell.notified() => {
                stopped = true;
                let _ = child.start_kill();
                break;
            }
            _ = tokio::time::sleep_until(deadline) => {
                let _ = child.start_kill();
                return Err(format!(
                    "the agent was still working after {} minutes and was stopped",
                    started.elapsed().as_secs() / 60
                ));
            }
        }
    }
    let status = child.wait().await.ok();
    let text = policy::join_text(&parts);
    if stopped || !text.is_empty() {
        return Ok(Outcome { text, steps, stopped });
    }
    if let Some(e) = error {
        return Err(format!("the agent could not answer: {e}"));
    }
    let said = said.await.unwrap_or_default();
    let why = said.lines().rev().find(|l| !l.trim().is_empty()).map(str::trim);
    Err(match (status.map(|s| s.success()), why) {
        (Some(true), _) => "the agent finished without writing an answer".to_string(),
        (_, Some(w)) => format!("the agent stopped without an answer: {w}"),
        _ => "the agent stopped without an answer, and said nothing about why".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_used_from_bubblewrap_0_10() {
        assert!(sized_from("bubblewrap 0.11.1\n"));
        assert!(sized_from("bubblewrap 0.10.0"));
        assert!(sized_from("bubblewrap 1.0"));
        assert!(!sized_from("bubblewrap 0.9.0"));
        assert!(!sized_from("bubblewrap 0.8.0"));
        assert!(!sized_from(""));
        assert!(!sized_from("bubblewrap x.y"));
    }

    #[test]
    fn dns_is_mounted_back_only_when_the_sandbox_hides_it() {
        assert_eq!(keep_for(Path::new("/run/systemd/resolve/stub-resolv.conf")), vec!["/run/systemd/resolve"]);
        assert_eq!(keep_for(Path::new("/run/NetworkManager/resolv.conf")), vec!["/run/NetworkManager"]);
        assert!(keep_for(Path::new("/etc/resolv.conf")).is_empty());
    }

    #[test]
    fn switched_off_never_looks_for_anything() {
        assert_eq!(probe(false), Err(Unavailable::Off));
    }

    /// The real sandbox, when this machine can build one. Says so and passes
    /// when it cannot: CI without `bwrap` is not evidence either way, and the
    /// probe's refusal is what the product does there.
    fn sandbox_here() -> Option<Sandbox> {
        match probe(true) {
            Ok(r) => Some(r.sandbox),
            Err(Unavailable::NoOpencode) => {
                // The containment test needs `bwrap`, not opencode: the
                // program inside is a script.
                let bwrap = crate::env::which("bwrap");
                namespace_starts(&bwrap).ok()?;
                Some(Sandbox { sized: sized(&bwrap), bwrap, opencode: "/bin/sh".into(), keep: keep() })
            }
            Err(u) => {
                eprintln!("skipped: no sandbox on this machine ({u:?})");
                None
            }
        }
    }

    /// The daemon's own argv, with a script where opencode would be.
    fn argv_running(sandbox: &Sandbox, script: &str) -> Vec<String> {
        let mut argv = policy::sandbox_argv(sandbox, "{}");
        let dash = argv.iter().position(|x| x == "--").unwrap();
        argv.truncate(dash + 1);
        argv.extend(["/bin/sh".to_string(), "-c".to_string(), script.to_string()]);
        argv
    }

    async fn output_of(argv: &[String]) -> String {
        let out = tokio::process::Command::new(&argv[0]).args(&argv[1..]).output().await.unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// ADR-0046 clause 2, and A55: the test that would have failed on the
    /// first profile tried, which left `docker.sock` reachable.
    #[tokio::test]
    async fn nothing_outside_the_tmpfs_is_writable_or_readable() {
        let Some(sandbox) = sandbox_here() else { return };
        let home = std::env::var("HOME").unwrap();
        // Under `/var/tmp`, which is writable outside and on the read-only
        // root inside. Not under `/tmp`: the sandbox replaces that, so a
        // canary there fails for the wrong reason (feature 0053, *Notes*).
        let canary_dir = PathBuf::from(format!("/var/tmp/mogeung-canary-{}", std::process::id()));
        std::fs::create_dir_all(&canary_dir).unwrap();
        let canary = canary_dir.join("written-from-inside");
        let runtime = format!("/run/user/{}", unsafe { libc::getuid() });
        // Every real entry in HOME, except the one directory `bwrap` makes as
        // the mount point for opencode's own (`~/.local`, for an npm
        // install): that is an empty skeleton, not your files.
        let skeleton = Path::new(&sandbox.opencode)
            .strip_prefix(&home)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| c.as_os_str().to_owned());
        let looks: String = std::fs::read_dir(&home)
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name()).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|n| Some(n) != skeleton.as_ref())
            .take(40)
            .map(|n| format!("[ -e '{home}/{}' ] && echo SAW_HOME\n", n.to_string_lossy().replace('\'', "")))
            .collect::<String>();
        // And the repository this test is in, which is your code.
        let repo = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let script = format!(
            r#"
            touch '{canary}' 2>/dev/null && echo WROTE_OUTSIDE
            touch /home/x /run/x /var/x 2>/dev/null && echo WROTE_HIDDEN
            {looks}
            [ -r '{repo}' ] && echo SAW_REPO
            [ -e '{runtime}/bus' ] && echo SAW_SESSION_BUS
            [ -e /run/docker.sock ] && echo SAW_DOCKER
            [ -e /var/run/docker.sock ] && echo SAW_DOCKER
            [ -e /run/dbus/system_bus_socket ] && echo SAW_SYSTEM_BUS
            [ -n "$SSH_AUTH_SOCK$DBUS_SESSION_BUS_ADDRESS$DISPLAY" ] && echo LEAKED_ENV
            echo hi > /tmp/work/inside && echo TMPFS_WRITABLE
            echo PID_$$
            "#,
            canary = canary.display(),
        );
        let out = output_of(&argv_running(&sandbox, &script)).await;
        for bad in ["WROTE_OUTSIDE", "WROTE_HIDDEN", "SAW_HOME", "SAW_REPO", "SAW_SESSION_BUS", "SAW_DOCKER", "SAW_SYSTEM_BUS", "LEAKED_ENV"] {
            assert!(!out.contains(bad), "{bad}: the sandbox let it through\n{out}");
        }
        let escaped = canary.exists();
        let _ = std::fs::remove_dir_all(&canary_dir);
        assert!(!escaped);
        assert!(out.contains("TMPFS_WRITABLE"), "the agent needs somewhere to work\n{out}");
        // Its own pid namespace: the shell is one of the first few processes.
        let pid: u32 = out.split("PID_").nth(1).and_then(|p| p.trim().parse().ok()).unwrap();
        assert!(pid < 10, "pid {pid} — not a fresh pid namespace");
    }

    /// The network is the point, so it must still be there: DNS resolves.
    #[tokio::test]
    async fn the_network_and_dns_still_work() {
        let Some(sandbox) = sandbox_here() else { return };
        let out = output_of(&argv_running(&sandbox, "getent hosts localhost >/dev/null && echo DNS_OK")).await;
        assert!(out.contains("DNS_OK"), "{out}");
    }

    /// A fake opencode, inside the real sandbox, speaking the stream.
    fn fake(sandbox: &Sandbox, lines: &[&str], then: &str) -> Vec<String> {
        let body: String = lines.iter().map(|l| format!("printf '%s\\n' '{l}'\n")).collect();
        argv_running(sandbox, &format!("cat >/dev/null\n{body}{then}"))
    }

    #[tokio::test]
    async fn text_and_steps_arrive_as_they_happen_and_the_answer_is_whole() {
        let Some(sandbox) = sandbox_here() else { return };
        let argv = fake(
            &sandbox,
            &[
                r#"{"type":"step_start","part":{}}"#,
                r#"{"type":"text","part":{"text":"Let me read it."}}"#,
                r#"{"type":"tool_use","part":{"tool":"webfetch","state":{"status":"completed","input":{"url":"https://example.com"}}}}"#,
                r#"{"type":"text","part":{"text":"It is a placeholder page."}}"#,
            ],
            "",
        );
        let mut chunks = Vec::new();
        let mut seen = Vec::new();
        let out = run_argv(&argv, "q", Duration::from_secs(30), &mut |t| chunks.push(t), &mut |s| seen.push(s), None)
            .await
            .unwrap();
        assert_eq!(out.text, "Let me read it.\n\nIt is a placeholder page.");
        assert_eq!(chunks.concat(), out.text, "the chunks add up to the answer");
        assert_eq!(seen.len(), 1);
        assert_eq!(out.steps, seen);
        assert!(!out.stopped);
    }

    #[tokio::test]
    async fn an_error_with_no_answer_says_what_opencode_said() {
        let Some(sandbox) = sandbox_here() else { return };
        let argv = fake(
            &sandbox,
            &[r#"{"type":"error","error":{"name":"APIError","data":{"message":"Cannot connect to API"}}}"#],
            "exit 1",
        );
        let e = run_argv(&argv, "q", Duration::from_secs(30), &mut |_| {}, &mut |_| {}, None).await.unwrap_err();
        assert!(e.contains("Cannot connect to API"), "{e}");
    }

    #[tokio::test]
    async fn silence_is_an_error_not_an_empty_answer() {
        let Some(sandbox) = sandbox_here() else { return };
        let e = run_argv(&fake(&sandbox, &[], ""), "q", Duration::from_secs(30), &mut |_| {}, &mut |_| {}, None)
            .await
            .unwrap_err();
        assert!(e.contains("without writing an answer"), "{e}");
    }

    #[tokio::test]
    async fn a_stop_keeps_what_arrived_and_ends_the_sandbox() {
        let Some(sandbox) = sandbox_here() else { return };
        let argv = fake(&sandbox, &[r#"{"type":"text","part":{"text":"half an answer"}}"#], "sleep 60");
        let bell = std::sync::Arc::new(Notify::new());
        let ring = bell.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            ring.notify_one();
        });
        let started = Instant::now();
        let out = run_argv(&argv, "q", Duration::from_secs(30), &mut |_| {}, &mut |_| {}, Some(&bell))
            .await
            .unwrap();
        assert!(out.stopped);
        assert_eq!(out.text, "half an answer");
        assert!(started.elapsed() < Duration::from_secs(10), "the stop did not end it");
    }

    #[tokio::test]
    async fn the_time_limit_ends_a_run_that_never_finishes() {
        let Some(sandbox) = sandbox_here() else { return };
        let e = run_argv(&fake(&sandbox, &[], "sleep 60"), "q", Duration::from_millis(500), &mut |_| {}, &mut |_| {}, None)
            .await
            .unwrap_err();
        assert!(e.contains("still working"), "{e}");
    }
}
