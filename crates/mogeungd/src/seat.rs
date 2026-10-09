//! Starting a seat: the launcher. `R-P4`, feature 0051.
//!
//! The one place mogeung starts an agent of its own
//! ([ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md), amended
//! 2026-10-09): a real interactive CLI, in a tmux session of its own, in a
//! worktree or the team's checkout, configured by what the CLI accepts at
//! launch and nothing written under the user's home
//! ([ADR-0043](../../../docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)).
//!
//! # The match is the contract
//!
//! [`compose`] turns a [`Seat`] into an argv and an environment through one
//! `match` per CLI, with no wildcard: a fifth `SessionSource` is a compile
//! error here, which is what
//! [ADR-0029](../../../docs/decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)
//! asks for and ADR-0025's amendment leans on. Nothing on the wire reaches
//! this function but a definition the daemon already holds.
//!
//! # What each CLI is handed, and where the guidance goes
//!
//! | CLI | session id | mode | guidance | own home |
//! |---|---|---|---|---|
//! | Claude Code | `--session-id` | `--permission-mode` | `--append-system-prompt-file` under `~/.mogeung` | `CLAUDE_CONFIG_DIR` |
//! | Qwen Code | `--session-id` | `--approval-mode` | `--append-system-prompt` as argv | `QWEN_HOME` |
//! | Codex | none — the pane is the key | `-a` and `-s workspace-write`; no plan mode, refused | a marked block in the worktree's `AGENTS.md`, removed at stop; in a home of its own, that home's `AGENTS.md` | `CODEX_HOME` |
//! | opencode | none | `--agent plan`, `--auto`, or a `permission` in the inline config | `instructions` in `OPENCODE_CONFIG_CONTENT`, pointing under `~/.mogeung` | the XDG pair |
//!
//! The Codex row is the one that writes into a worktree, and it is allowed
//! by [ADR-0019](../../../docs/decisions/0019-a-viewer-not-an-editor.md)'s
//! amendment — a generated file in a marked block, in a worktree the daemon
//! made, never committed — and taken back at stop so the worktree is as clean
//! as the seat left it. A Codex seat in the team's *checkout* with guidance
//! and no home of its own is refused, in words: there is nowhere to put it
//! that is not the user's.

use anyhow::{anyhow, bail, Context, Result};
use mogeung_core::session::SessionSource;
use mogeung_core::team::{PermissionMode, Seat, SeatLaunch, Team, Workplace};
use std::path::{Path, PathBuf};

/// Where a seat's generated files live: `~/.mogeung/seats/<team id>/<seat>/`.
pub fn seats_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("seats")
}

/// The team's name as a path and session-name segment.
pub fn slug(team: &Team) -> String {
    crate::team::slug(&team.name)
}

/// `geung-<team>-<seat>`: the tmux session the seat runs in, alone.
pub fn tmux_session_name(team: &Team, seat: &Seat) -> String {
    format!("geung-{}-{}", slug(team), seat.name)
}

/// `geung/<team>/<seat>`: the branch a seat's worktree is cut on.
pub fn worktree_branch(team: &Team, seat: &Seat) -> String {
    format!("geung/{}/{}", slug(team), seat.name)
}

/// Beside the worktrees `LaunchTerminal` makes, named for the team and seat
/// rather than a stamp, so a second boot finds the first boot's worktree.
pub fn worktree_dir(repo: &Path, team: &Team, seat: &Seat) -> PathBuf {
    let repo_name = repo
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into());
    crate::git::worktree_root()
        .join(repo_name)
        .join(format!("{}-{}", slug(team), seat.name))
}

/// The two layers, in the order the launcher applies them: the team's, then
/// the seat's own. Never merged anywhere else.
pub fn guidance_text(team: &Team, seat: &Seat) -> String {
    let mut out = String::new();
    let team_g = team.guidance.trim();
    let seat_g = seat.guidance.trim();
    if !team_g.is_empty() {
        out.push_str(&format!("# Team {}\n\n{team_g}\n", team.name));
    }
    if !seat_g.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("# Seat {}\n\n{seat_g}\n", seat.name));
    }
    out
}

pub fn claude_mode(m: PermissionMode) -> &'static str {
    match m {
        PermissionMode::Default => "default",
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::Plan => "plan",
        PermissionMode::BypassPermissions => "bypassPermissions",
    }
}

pub fn qwen_mode(m: PermissionMode) -> &'static str {
    match m {
        PermissionMode::Default => "default",
        PermissionMode::AcceptEdits => "auto-edit",
        PermissionMode::Plan => "plan",
        PermissionMode::BypassPermissions => "yolo",
    }
}

/// Codex's `--ask-for-approval`. Never its bypass flag: `never` with the
/// `workspace-write` sandbox is the recipe `LaunchTerminal` and `codexmo`
/// already use, and the sandbox is not something a definition gets to drop.
pub fn codex_approval(m: PermissionMode) -> Result<&'static str> {
    match m {
        PermissionMode::Default => Ok("on-request"),
        PermissionMode::AcceptEdits => Ok("on-failure"),
        PermissionMode::Plan => {
            bail!("Codex has no plan mode — give this seat another mode, or another CLI")
        }
        PermissionMode::BypassPermissions => Ok("never"),
    }
}

const BLOCK_OPEN: &str = "<!-- mogeung:seat — generated for a seat; never commit this block -->";
const BLOCK_CLOSE: &str = "<!-- /mogeung:seat -->";

/// `existing` with one marked block carrying `body`, replacing any earlier.
pub fn with_block(existing: &str, body: &str) -> String {
    let base = without_block(existing);
    let mut out = base.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(BLOCK_OPEN);
    out.push('\n');
    out.push_str(body.trim_end());
    out.push('\n');
    out.push_str(BLOCK_CLOSE);
    out.push('\n');
    out
}

/// `text` with the marked block gone, and nothing else touched.
pub fn without_block(text: &str) -> String {
    let Some(start) = text.find(BLOCK_OPEN) else {
        return text.to_string();
    };
    let Some(end_rel) = text[start..].find(BLOCK_CLOSE) else {
        return text.to_string();
    };
    let end = start + end_rel + BLOCK_CLOSE.len();
    let mut out = text[..start].trim_end().to_string();
    let rest = text[end..].trim_start_matches('\n');
    if !rest.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(rest);
    } else if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Where each CLI is on this machine. A struct so a test can compose a
/// command without the resolvers walking the real `PATH`.
#[derive(Debug, Clone)]
pub struct Binaries {
    pub claude: String,
    pub qwen: String,
    pub codex: String,
    pub opencode: String,
}

impl Binaries {
    pub fn installed() -> Self {
        Binaries {
            claude: crate::state::claude_binary(),
            qwen: crate::state::qwen_binary(),
            codex: crate::state::codex_binary(),
            opencode: crate::state::opencode_binary(),
        }
    }
}

/// What a seat is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composed {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    /// The worktree file a marked block was written into, if one was —
    /// Codex's `AGENTS.md` — so `stop` can take it back.
    pub wrote: Option<PathBuf>,
}

/// Compose the command, writing the generated files it names. Pure but for
/// those writes, which all land under `files` — or, for Codex in a worktree,
/// in `dir`, marked.
pub fn compose(
    team: &Team,
    seat: &Seat,
    files: &Path,
    dir: &Path,
    session_id: Option<&str>,
    bins: &Binaries,
) -> Result<Composed> {
    std::fs::create_dir_all(files)?;
    let guidance = guidance_text(team, seat);
    let guidance_path = files.join("guidance.md");
    std::fs::write(&guidance_path, &guidance)?;
    let home = files.join("home");
    if seat.own_home {
        std::fs::create_dir_all(&home)?;
    }
    let mode = team.permission_for(seat);
    let label = format!("{}/{}", team.name, seat.name);
    let mut env = Vec::new();
    let mut wrote = None;
    let argv = match seat.cli {
        SessionSource::ClaudeCode => {
            let mut a = vec![
                bins.claude.clone(),
                "--session-id".into(),
                session_id.ok_or_else(|| anyhow!("a Claude seat needs a session id"))?.into(),
                "--name".into(),
                label.clone(),
                "--permission-mode".into(),
                claude_mode(mode).into(),
            ];
            if !guidance.is_empty() {
                a.push("--append-system-prompt-file".into());
                a.push(guidance_path.to_string_lossy().into_owned());
            }
            if let Some(m) = &seat.model {
                a.push("--model".into());
                a.push(m.clone());
            }
            if seat.own_home {
                env.push(("CLAUDE_CONFIG_DIR".into(), home.to_string_lossy().into_owned()));
            }
            a
        }
        SessionSource::QwenCode => {
            let mut a = vec![
                bins.qwen.clone(),
                "--session-id".into(),
                session_id.ok_or_else(|| anyhow!("a Qwen seat needs a session id"))?.into(),
                "--approval-mode".into(),
                qwen_mode(mode).into(),
            ];
            if !guidance.is_empty() {
                a.push("--append-system-prompt".into());
                a.push(guidance.clone());
            }
            if let Some(m) = &seat.model {
                a.push("-m".into());
                a.push(m.clone());
            }
            if seat.own_home {
                env.push(("QWEN_HOME".into(), home.to_string_lossy().into_owned()));
            }
            a
        }
        SessionSource::Codex => {
            let approval = codex_approval(mode)?;
            let mut a = vec![
                bins.codex.clone(),
                "-a".into(),
                approval.into(),
                "-s".into(),
                "workspace-write".into(),
            ];
            if let Some(m) = &seat.model {
                a.push("-m".into());
                a.push(m.clone());
            }
            if seat.own_home {
                env.push(("CODEX_HOME".into(), home.to_string_lossy().into_owned()));
                if !guidance.is_empty() {
                    std::fs::write(home.join("AGENTS.md"), &guidance)?;
                }
            } else if !guidance.is_empty() {
                match seat.works_in {
                    Workplace::Worktree => {
                        let path = dir.join("AGENTS.md");
                        let existing = std::fs::read_to_string(&path).unwrap_or_default();
                        std::fs::write(&path, with_block(&existing, &guidance))?;
                        wrote = Some(path);
                    }
                    Workplace::Checkout => bail!(
                        "a Codex seat in the team's checkout cannot be given guidance without \
                         writing there — put it in a worktree, or give it a home of its own"
                    ),
                }
            }
            a
        }
        SessionSource::OpenCode => {
            let mut a = vec![bins.opencode.clone()];
            if let Some(m) = &seat.model {
                a.push("--model".into());
                a.push(m.clone());
            }
            let mut config = serde_json::Map::new();
            if !guidance.is_empty() {
                config.insert(
                    "instructions".into(),
                    serde_json::json!([guidance_path.to_string_lossy()]),
                );
            }
            match mode {
                PermissionMode::Default => {}
                PermissionMode::AcceptEdits => {
                    config.insert("permission".into(), serde_json::json!({ "edit": "allow" }));
                }
                PermissionMode::Plan => {
                    a.push("--agent".into());
                    a.push("plan".into());
                }
                PermissionMode::BypassPermissions => a.push("--auto".into()),
            }
            if !config.is_empty() {
                env.push((
                    "OPENCODE_CONFIG_CONTENT".into(),
                    serde_json::Value::Object(config).to_string(),
                ));
            }
            if seat.own_home {
                env.push(("XDG_CONFIG_HOME".into(), home.join("config").to_string_lossy().into_owned()));
                env.push(("XDG_DATA_HOME".into(), home.join("data").to_string_lossy().into_owned()));
            }
            a
        }
    };
    Ok(Composed { argv, env, wrote })
}

/// The worktree for a seat: found from an earlier boot, or made. A stop
/// leaves worktrees behind on purpose, so the second boot works where the
/// first did, on the same branch.
fn ensure_worktree(repo: &Path, team: &Team, seat: &Seat) -> Result<PathBuf> {
    let dir = worktree_dir(repo, team, seat);
    if dir.join(".git").exists() {
        return Ok(dir);
    }
    if dir.exists() {
        bail!(
            "{} exists and is not a worktree — move it aside, or remove it",
            dir.display()
        );
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let branch = worktree_branch(team, seat);
    let dir_s = dir.to_string_lossy().to_string();
    let has_branch = crate::git::run_git(
        repo,
        &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")],
    )
    .is_ok();
    if has_branch {
        crate::git::run_git(repo, &["worktree", "add", &dir_s, &branch])
    } else {
        crate::git::run_git(repo, &["worktree", "add", "-b", &branch, &dir_s, "HEAD"])
    }
    .with_context(|| format!("making the worktree at {dir_s}"))?;
    Ok(dir)
}

/// The argv for the tmux start, pure so a test can pin its shape. `-P -F`
/// prints the pane id and the target so the record is exact from birth; `-e`
/// sets the seat's own environment without touching the server's.
pub fn tmux_argv(name: &str, dir: &Path, env: &[(String, String)], argv: &[String]) -> Vec<String> {
    let mut cmd = vec![
        "new-session".to_string(),
        "-d".into(),
        "-P".into(),
        "-F".into(),
        "#{pane_id} #{session_name}:#{window_index}.#{pane_index}".into(),
        "-s".into(),
        name.to_string(),
        "-c".into(),
        dir.to_string_lossy().into_owned(),
    ];
    for (k, v) in env {
        cmd.push("-e".into());
        cmd.push(format!("{k}={v}"));
    }
    cmd.extend(argv.iter().cloned());
    cmd
}

fn start_in_tmux(name: &str, dir: &Path, env: &[(String, String)], argv: &[String]) -> Result<(String, String)> {
    let cmd = tmux_argv(name, dir, env, argv);
    let out = match crate::env::command("tmux").args(&cmd).output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("a seat needs tmux, and tmux is not installed (ADR-0011: no tmux, no seats)")
        }
        Err(e) => bail!("could not run tmux: {e}"),
        Ok(out) => out,
    };
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let first = why.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no message");
        bail!("tmux would not create the session — {first}");
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.trim();
    let (pane, target) = line
        .split_once(' ')
        .ok_or_else(|| anyhow!("tmux did not name the pane: `{line}`"))?;
    for opts in [
        vec!["set-option", "-w", "-t", &format!("={name}:"), "window-size", "latest"],
        vec!["set-option", "-t", name, "status", "off"],
    ] {
        let owned: Vec<String> = opts.iter().map(|s| s.to_string()).collect();
        if let Err(e) = crate::env::command("tmux").args(&owned).output() {
            tracing::warn!("tmux option not applied to {name}: {e}");
        }
    }
    Ok((pane.to_string(), target.to_string()))
}

/// A session name no other tmux session holds: the base, else `-2`, `-3`…
fn unique_tmux_name(base: &str) -> String {
    let taken = |name: &str| {
        crate::env::command("tmux")
            .args(["has-session", "-t", &format!("={name}")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !taken(base) {
        return base.to_string();
    }
    for n in 2..100 {
        let cand = format!("{base}-{n}");
        if !taken(&cand) {
            return cand;
        }
    }
    format!("{base}-{}", std::process::id())
}

/// Start one seat. Refusals come before anything is made: no repository, no
/// CLI, a mode the CLI lacks. Then the worktree, the files, the process.
pub fn start(team: &Team, seat: &Seat, seats_root: &Path, bins: &Binaries) -> Result<SeatLaunch> {
    if team.repo.trim().is_empty() {
        bail!("the team names no repository");
    }
    let repo = PathBuf::from(&team.repo);
    if !repo.is_dir() {
        bail!("the team's repository {} does not exist", repo.display());
    }
    if seat.works_in == Workplace::Worktree && !crate::git::is_repo(&repo) {
        bail!("{} is not a git repository, so no worktree can be made in it", repo.display());
    }
    let binary = match seat.cli {
        SessionSource::ClaudeCode => &bins.claude,
        SessionSource::QwenCode => &bins.qwen,
        SessionSource::Codex => &bins.codex,
        SessionSource::OpenCode => &bins.opencode,
    };
    if !Path::new(binary).is_file() && crate::env::which(binary) == *binary && !Path::new(binary).is_absolute() {
        bail!("{} is not installed on this machine", seat.cli.label());
    }
    let dir = match seat.works_in {
        Workplace::Worktree => ensure_worktree(&repo, team, seat)?,
        Workplace::Checkout => repo.clone(),
    };
    let files = seats_root.join(&team.id).join(&seat.name);
    let session_id = match seat.cli {
        SessionSource::ClaudeCode | SessionSource::QwenCode => Some(uuid::Uuid::new_v4().to_string()),
        SessionSource::Codex | SessionSource::OpenCode => None,
    };
    let composed = compose(team, seat, &files, &dir, session_id.as_deref(), bins)?;
    let name = unique_tmux_name(&tmux_session_name(team, seat));
    let (pane_id, tmux_target) = start_in_tmux(&name, &dir, &composed.env, &composed.argv)?;
    Ok(SeatLaunch {
        team_id: team.id.clone(),
        team: team.name.clone(),
        seat: seat.name.clone(),
        cli: seat.cli,
        session_id,
        pane_id,
        tmux_target,
        tmux_session: name,
        dir: dir.to_string_lossy().into_owned(),
        worktree: seat.works_in == Workplace::Worktree,
        files_dir: files.to_string_lossy().into_owned(),
        started: chrono::Utc::now().timestamp(),
    })
}

/// End the seat's process and take back what was written into its
/// worktree. The worktree itself stays.
pub fn stop(launch: &SeatLaunch) {
    let _ = crate::env::command("tmux")
        .args(["kill-session", "-t", &format!("={}", launch.tmux_session)])
        .output();
    if launch.worktree && launch.cli == SessionSource::Codex {
        let path = Path::new(&launch.dir).join("AGENTS.md");
        if let Ok(text) = std::fs::read_to_string(&path) {
            let rest = without_block(&text);
            if rest.trim().is_empty() {
                let _ = std::fs::remove_file(&path);
            } else if rest != text {
                let _ = std::fs::write(&path, rest);
            }
        }
    }
}

/// Remove a stopped seat's worktree, only when it is clean. The branch stays:
/// its commits are the seat's work, and deleting them is not what *remove the
/// worktree* says.
pub fn remove_worktree(team: &Team, seat: &Seat) -> Result<PathBuf> {
    let repo = PathBuf::from(&team.repo);
    let dir = worktree_dir(&repo, team, seat);
    if !dir.join(".git").exists() {
        bail!("there is no worktree for seat `{}` at {}", seat.name, dir.display());
    }
    let status = crate::git::run_git(&dir, &["status", "--porcelain"])?;
    let dirty: Vec<&str> = status.lines().filter(|l| !l.trim().is_empty()).collect();
    if !dirty.is_empty() {
        let shown: Vec<&str> = dirty.iter().take(5).copied().collect();
        bail!(
            "the worktree for `{}` is not clean — {} path(s), e.g. {} — commit or discard first",
            seat.name,
            dirty.len(),
            shown.join(", ")
        );
    }
    let dir_s = dir.to_string_lossy().to_string();
    crate::git::run_git(&repo, &["worktree", "remove", &dir_s])
        .with_context(|| format!("removing the worktree at {dir_s}"))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::team::Policy;

    fn bins() -> Binaries {
        Binaries {
            claude: "/bin/claude".into(),
            qwen: "/bin/qwen".into(),
            codex: "/bin/codex".into(),
            opencode: "/bin/opencode".into(),
        }
    }

    fn team() -> Team {
        Team {
            id: "t1".into(),
            name: "Port OpenRig".into(),
            repo: "/home/you/mogeung".into(),
            guidance: "Read the spec.".into(),
            permission: PermissionMode::AcceptEdits,
            policy: Policy::default(),
            seats: vec![],
            paused: false,
            created: 0,
            updated: 0,
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-seat-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names_follow_the_team_and_the_seat() {
        let t = team();
        let s = Seat::new("builder", SessionSource::Codex);
        assert_eq!(tmux_session_name(&t, &s), "geung-port-openrig-builder");
        assert_eq!(worktree_branch(&t, &s), "geung/port-openrig/builder");
        assert!(worktree_dir(Path::new("/home/you/mogeung"), &t, &s)
            .ends_with("worktrees/mogeung/port-openrig-builder"));
    }

    /// The two layers, in order, and apart.
    #[test]
    fn guidance_is_the_teams_then_the_seats() {
        let t = team();
        let mut s = Seat::new("reviewer", SessionSource::ClaudeCode);
        s.guidance = "Review only.".into();
        let g = guidance_text(&t, &s);
        let team_at = g.find("Read the spec.").unwrap();
        let seat_at = g.find("Review only.").unwrap();
        assert!(team_at < seat_at, "{g}");
        assert!(g.starts_with("# Team Port OpenRig"), "{g}");
        s.guidance.clear();
        assert!(!guidance_text(&t, &s).contains("# Seat"));
    }

    #[test]
    fn a_claude_seat_is_named_moded_and_guided_by_flags_only() {
        let d = scratch("claude");
        let t = team();
        let mut s = Seat::new("reviewer", SessionSource::ClaudeCode);
        s.model = Some("haiku".into());
        s.permission = Some(PermissionMode::Plan);
        let c = compose(&t, &s, &d.join("files"), &d, Some("abc"), &bins()).unwrap();
        let a = c.argv.join(" ");
        assert!(a.starts_with("/bin/claude --session-id abc --name Port OpenRig/reviewer --permission-mode plan"), "{a}");
        assert!(a.contains("--append-system-prompt-file"), "{a}");
        assert!(a.ends_with("--model haiku"), "{a}");
        assert!(c.env.is_empty(), "no home of its own unless asked");
        assert!(d.join("files/guidance.md").is_file());
        s.own_home = true;
        let c = compose(&t, &s, &d.join("files"), &d, Some("abc"), &bins()).unwrap();
        assert_eq!(c.env[0].0, "CLAUDE_CONFIG_DIR");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_qwen_seat_takes_the_guidance_as_argv_and_the_teams_mode() {
        let d = scratch("qwen");
        let t = team();
        let s = Seat::new("tester", SessionSource::QwenCode);
        let c = compose(&t, &s, &d.join("files"), &d, Some("q1"), &bins()).unwrap();
        assert_eq!(&c.argv[..5], &["/bin/qwen", "--session-id", "q1", "--approval-mode", "auto-edit"]);
        assert!(c.argv.iter().any(|x| x.contains("Read the spec.")));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_codex_seat_has_no_plan_mode_and_writes_its_guidance_as_a_marked_block() {
        let d = scratch("codex");
        let t = team();
        let mut s = Seat::new("builder", SessionSource::Codex);
        s.permission = Some(PermissionMode::Plan);
        let why = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap_err().to_string();
        assert!(why.contains("no plan mode"), "{why}");

        s.permission = Some(PermissionMode::BypassPermissions);
        std::fs::write(d.join("AGENTS.md"), "# Repo rules\n\nBe kind.\n").unwrap();
        let c = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap();
        assert_eq!(&c.argv[..5], &["/bin/codex", "-a", "never", "-s", "workspace-write"]);
        assert!(!c.argv.iter().any(|x| x.contains("dangerously")), "{:?}", c.argv);
        let agents = std::fs::read_to_string(d.join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# Repo rules"), "{agents}");
        assert!(agents.contains(BLOCK_OPEN) && agents.contains("Read the spec."), "{agents}");
        assert_eq!(c.wrote, Some(d.join("AGENTS.md")));
        // Taken back whole: the repository's own file is what it was.
        assert_eq!(without_block(&agents), "# Repo rules\n\nBe kind.\n");

        // In the checkout, with nowhere of mogeung's to put it: refused.
        s.works_in = Workplace::Checkout;
        s.permission = None;
        let why = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap_err().to_string();
        assert!(why.contains("checkout"), "{why}");
        // With a home of its own, the home's AGENTS.md is the place.
        s.own_home = true;
        let c = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap();
        assert!(c.env.iter().any(|(k, _)| k == "CODEX_HOME"));
        assert!(d.join("files/home/AGENTS.md").is_file());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_opencode_seat_is_configured_inline_and_never_in_its_home() {
        let d = scratch("opencode");
        let mut t = team();
        let mut s = Seat::new("tester", SessionSource::OpenCode);
        let c = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap();
        assert_eq!(c.argv, vec!["/bin/opencode".to_string()]);
        let cfg = &c.env.iter().find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT").unwrap().1;
        assert!(cfg.contains("\"instructions\"") && cfg.contains("\"edit\":\"allow\""), "{cfg}");
        t.permission = PermissionMode::BypassPermissions;
        s.permission = Some(PermissionMode::Plan);
        let c = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap();
        assert_eq!(&c.argv[1..], &["--agent", "plan"]);
        s.permission = None;
        let c = compose(&t, &s, &d.join("files"), &d, None, &bins()).unwrap();
        assert_eq!(&c.argv[1..], &["--auto"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_marked_block_is_added_once_and_removed_whole() {
        let once = with_block("", "hello");
        assert!(once.starts_with(BLOCK_OPEN), "{once}");
        let twice = with_block(&once, "again");
        assert_eq!(twice.matches(BLOCK_OPEN).count(), 1, "{twice}");
        assert!(twice.contains("again") && !twice.contains("hello"), "{twice}");
        assert_eq!(without_block(&twice), "");
        let mixed = with_block("# Keep\n", "x");
        assert_eq!(without_block(&mixed), "# Keep\n");
        assert_eq!(without_block("untouched"), "untouched");
    }

    /// `-e` sets the seat's environment on its own session, and the format
    /// prints what the record needs — nothing is read from a shell.
    #[test]
    fn the_tmux_start_names_the_pane_and_carries_the_environment() {
        let argv = tmux_argv(
            "geung-t-s",
            Path::new("/w"),
            &[("CLAUDE_CONFIG_DIR".into(), "/h".into())],
            &["/bin/claude".into(), "--name".into(), "t/s".into()],
        );
        assert_eq!(&argv[..3], &["new-session", "-d", "-P"]);
        assert!(argv.contains(&"#{pane_id} #{session_name}:#{window_index}.#{pane_index}".to_string()));
        let e = argv.iter().position(|x| x == "-e").unwrap();
        assert_eq!(argv[e + 1], "CLAUDE_CONFIG_DIR=/h");
        assert_eq!(argv[argv.len() - 3..], ["/bin/claude", "--name", "t/s"]);
    }
}
