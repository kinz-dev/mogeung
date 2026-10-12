//! A team definition out as a file, and a file in — mogeung's own, or an
//! OpenRig `rig.yaml` read as other people's configuration. `R-P12`,
//! feature 0051.
//!
//! *"UI come first for the team config, later I can import/export the
//! config"* (2026-10-09). The window is still where a team is written; this
//! module is the door a file comes through, both ways.
//!
//! # Export is the definition, whole
//!
//! The team's own JSON — the blob the store keeps, pretty-printed — with the
//! id, the stamps and the pause switch left in, because an export is a copy
//! of what the daemon holds and a reader should see all of it. Written where
//! you ask, or beside the markdown mirror under `~/.mogeung/teams/` as
//! `<id>-<slug>.json` ([ADR-0043](../../../docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)
//! clause 8's one-way mirror, in a second shape). Never read back on its own:
//! importing it is the explicit act below.
//!
//! # Import is a draft, never a save
//!
//! A file in becomes a **draft** the window shows for review, with notes on
//! what was and was not carried; nothing is stored until you press Save,
//! and the daemon validates then as for any save. Two formats:
//!
//! - **mogeung's own JSON**: the id is dropped so the import is a *new* team
//!   rather than an overwrite of one that happens to share an id, the stamps
//!   are zeroed for the daemon to set, and the pause switch is cleared — a
//!   paused team on another machine is not a paused team here.
//! - **OpenRig `rig.yaml`** (version `0.2`, pod-aware), read the way
//!   [ADR-0026](../../../docs/decisions/0026-other-peoples-run-configurations.md)
//!   reads an IDE's launch file: as *their* configuration, mapped to ours
//!   where a mapping is honest and noted where it is not. A member becomes a
//!   seat — its id the seat's name where unique, else `<pod>-<member>`; its
//!   `runtime` the seat's CLI through one match, and a runtime that is not a
//!   CLI (`terminal`) is skipped and said so; its `agent_ref` followed
//!   relative to the rig file to the agent's `guidance/*.md`, which become
//!   the seat's guidance when they are there and a note when they are not.
//!   The rig's `summary` and its culture file become the team's guidance;
//!   its edges are appended as declared text — *who delegates to whom* —
//!   because turning them into wakes would be inventing policy the file
//!   does not state. OpenRig's profiles, skills, plugins, startup actions and
//!   bundles are out of scope (spec 0051) and listed as not carried.
//!
//! The repository is never in either file in a form this machine can trust —
//! a rig's `cwd: .` is relative to a project it was installed into — so the
//! draft arrives with `repo` empty and the window asks for it.

use anyhow::{anyhow, bail, Context, Result};
use mogeung_core::session::SessionSource;
use mogeung_core::team::{Seat, Team, Workplace};
use std::path::Path;

/// The most a file in may be. A team definition is kilobytes.
pub const MAX_FILE: u64 = 1024 * 1024;

/// What an import answers: the draft and what it had to say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub team: Team,
    pub notes: Vec<String>,
}

/// The export: the definition whole, pretty, as JSON text.
pub fn export_json(team: &Team) -> Result<String> {
    Ok(serde_json::to_string_pretty(team)?)
}

/// The export file's name beside the mirror.
pub fn export_name(team: &Team) -> String {
    format!("{}-{}.json", team.id, crate::team::slug(&team.name))
}

/// Which door a file comes through, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mogeung,
    Rig,
}

pub fn format_of(path: &Path) -> Result<Format> {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("json") => Ok(Format::Mogeung),
        Some("yaml") | Some("yml") => Ok(Format::Rig),
        _ => bail!(
            "{} is neither a mogeung team (.json) nor an OpenRig rig (rig.yaml)",
            path.display()
        ),
    }
}

/// mogeung's own JSON, as a draft: a new team, unstamped, not paused.
pub fn import_json(text: &str) -> Result<Imported> {
    let mut team: Team = serde_json::from_str(text).context("not a mogeung team definition")?;
    let mut notes = Vec::new();
    if !team.id.is_empty() {
        notes.push(format!("imported as a new team — the file's id `{}` is not carried", team.id));
    }
    team.id.clear();
    team.created = 0;
    team.updated = 0;
    if team.paused {
        notes.push("the file said paused; the draft is not — pause here if you mean it".into());
        team.paused = false;
    }
    if team.repo.trim().is_empty() {
        notes.push("set the repository: the file names none".into());
    } else {
        notes.push(format!("the repository `{}` is the file's — check it exists here", team.repo));
    }
    Ok(Imported { team, notes })
}

/// OpenRig's `runtime` word to our variant. One match, and a runtime that is
/// not an agent CLI is `None`.
pub fn runtime_cli(runtime: &str) -> Option<SessionSource> {
    match runtime.trim().to_ascii_lowercase().as_str() {
        "claude-code" | "claude" => Some(SessionSource::ClaudeCode),
        "codex" => Some(SessionSource::Codex),
        "qwen-code" | "qwen" => Some(SessionSource::QwenCode),
        "opencode" => Some(SessionSource::OpenCode),
        _ => None,
    }
}

/// A seat name from a member id: lower-case, dashes for anything else.
pub fn seat_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    let out = if out.chars().next().is_some_and(|c| c.is_ascii_digit()) { format!("s-{out}") } else { out };
    if out.is_empty() { "seat".to_string() } else { out.chars().take(32).collect() }
}

/// Follow a `local:<path>` agent ref from the rig's directory to the agent's
/// guidance markdown, in the order the spec lists it. `None` when the ref is
/// not local or the folder is not there.
fn agent_guidance(base: &Path, agent_ref: &str) -> Option<(Vec<String>, Vec<String>)> {
    let rel = agent_ref.strip_prefix("local:")?;
    let dir = base.join(rel);
    let spec = std::fs::read_to_string(dir.join("agent.yaml")).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&spec).ok()?;
    let mut paths: Vec<String> = Vec::new();
    for g in doc["resources"]["guidance"].as_sequence().into_iter().flatten() {
        if let Some(p) = g["path"].as_str() {
            paths.push(p.to_string());
        }
    }
    for f in doc["startup"]["files"].as_sequence().into_iter().flatten() {
        if let Some(p) = f["path"].as_str() {
            if p.ends_with(".md") && !paths.iter().any(|x| x == p) {
                paths.push(p.to_string());
            }
        }
    }
    let mut texts = Vec::new();
    let mut missing = Vec::new();
    for p in &paths {
        match std::fs::read_to_string(dir.join(p)) {
            Ok(t) if !t.trim().is_empty() => texts.push(t.trim_end().to_string()),
            _ => missing.push(p.clone()),
        }
    }
    Some((texts, missing))
}

/// An OpenRig `rig.yaml`, as a draft. `base` is the rig file's directory,
/// which its `local:` refs and `culture_file` are relative to.
pub fn import_rig(text: &str, base: &Path) -> Result<Imported> {
    let doc: serde_yaml::Value = serde_yaml::from_str(text).context("not YAML")?;
    let map = doc.as_mapping().ok_or_else(|| anyhow!("a rig.yaml is a mapping at the top"))?;
    let _ = map;
    let name = doc["name"].as_str().unwrap_or("imported-rig").trim().to_string();
    let mut notes: Vec<String> = Vec::new();
    let mut team = Team {
        id: String::new(),
        name: if name.is_empty() { "imported-rig".into() } else { name },
        repo: String::new(),
        guidance: String::new(),
        permission: Default::default(),
        policy: Default::default(),
        seats: Vec::new(),
        paused: false,
        created: 0,
        updated: 0,
    };
    if let Some(v) = doc["version"].as_str() {
        if v != "0.2" {
            notes.push(format!("rig version {v}; read as 0.2 (pod-aware), the shape this import knows"));
        }
    }
    let mut guidance = String::new();
    if let Some(s) = doc["summary"].as_str() {
        guidance.push_str(s.trim());
        guidance.push('\n');
    }
    if let Some(c) = doc["culture_file"].as_str() {
        match std::fs::read_to_string(base.join(c)) {
            Ok(t) if !t.trim().is_empty() => {
                if !guidance.is_empty() {
                    guidance.push('\n');
                }
                guidance.push_str(t.trim_end());
                guidance.push('\n');
            }
            _ => notes.push(format!("culture file `{c}` was not found beside the rig — not carried")),
        }
    }

    // Members, pod by pod. Names are member ids where unique in the rig.
    struct Member {
        pod: String,
        id: String,
        runtime: String,
        agent_ref: Option<String>,
        cwd: Option<String>,
    }
    let mut members: Vec<Member> = Vec::new();
    for pod in doc["pods"].as_sequence().into_iter().flatten() {
        let pod_id = pod["id"].as_str().unwrap_or("pod").to_string();
        for m in pod["members"].as_sequence().into_iter().flatten() {
            members.push(Member {
                pod: pod_id.clone(),
                id: m["id"].as_str().unwrap_or("member").to_string(),
                runtime: m["runtime"].as_str().unwrap_or("").to_string(),
                agent_ref: m["agent_ref"].as_str().map(str::to_string),
                cwd: m["cwd"].as_str().map(str::to_string),
            });
        }
    }
    if members.is_empty() {
        bail!("the rig names no members under `pods`");
    }
    let unique = |id: &str| members.iter().filter(|m| m.id == id).count() == 1;
    let mut skipped: Vec<String> = Vec::new();
    for m in &members {
        let Some(cli) = runtime_cli(&m.runtime) else {
            skipped.push(format!("`{}.{}` (runtime `{}`)", m.pod, m.id, if m.runtime.is_empty() { "none" } else { &m.runtime }));
            continue;
        };
        let name = if unique(&m.id) { seat_name(&m.id) } else { seat_name(&format!("{}-{}", m.pod, m.id)) };
        let mut seat = Seat::new(name, cli);
        seat.label = format!("{}.{}", m.pod, m.id);
        seat.works_in = Workplace::Worktree;
        if let Some(cwd) = &m.cwd {
            if cwd != "." && !cwd.is_empty() {
                notes.push(format!(
                    "`{}` worked in `{cwd}`; a seat works in a worktree of the team's repository or its checkout, so that is not carried",
                    seat.name
                ));
            }
        }
        if let Some(r) = &m.agent_ref {
            match agent_guidance(base, r) {
                Some((texts, missing)) => {
                    seat.guidance = texts.join("\n\n");
                    if !missing.is_empty() {
                        notes.push(format!("`{}`: guidance file(s) {} named by `{r}` not found", seat.name, missing.join(", ")));
                    }
                    if texts.is_empty() && missing.is_empty() {
                        notes.push(format!("`{}`: `{r}` names no guidance files — the seat starts with the team's only", seat.name));
                    }
                }
                None if r.starts_with("local:") => notes.push(format!("`{}`: agent `{r}` not found beside the rig — no guidance carried", seat.name)),
                None => notes.push(format!("`{}`: agent `{r}` is not a local ref — no guidance carried", seat.name)),
            }
        }
        team.seats.push(seat);
    }
    if !skipped.is_empty() {
        notes.push(format!("skipped, not an agent CLI: {}", skipped.join(", ")));
    }
    if team.seats.is_empty() {
        bail!("none of the rig's members runs an agent CLI this daemon can start");
    }

    // Edges, as declared text: who delegates to or observes whom.
    let mut edges: Vec<String> = Vec::new();
    let describe = |e: &serde_yaml::Value, pod: Option<&str>| -> Option<String> {
        let kind = e["kind"].as_str()?.replace('_', " ");
        let from = e["from"].as_str()?;
        let to = e["to"].as_str()?;
        Some(match pod {
            Some(p) => format!("{p}.{from} {kind} {p}.{to}"),
            None => format!("{from} {kind} {to}"),
        })
    };
    for pod in doc["pods"].as_sequence().into_iter().flatten() {
        let pod_id = pod["id"].as_str();
        for e in pod["edges"].as_sequence().into_iter().flatten() {
            if let Some(line) = describe(e, pod_id) {
                edges.push(line);
            }
        }
    }
    for e in doc["edges"].as_sequence().into_iter().flatten() {
        if let Some(line) = describe(e, None) {
            edges.push(line);
        }
    }
    if !edges.is_empty() {
        if !guidance.is_empty() {
            guidance.push('\n');
        }
        guidance.push_str("## From the rig: who works with whom\n\n");
        for e in &edges {
            guidance.push_str(&format!("- {e}\n"));
        }
        notes.push("the rig's edges are carried as text in the team's guidance, not as wakes — declare any send you want in the policy".into());
    }
    team.guidance = guidance.trim_end().to_string();
    notes.push("set the repository: a rig's `cwd` is relative to a project it was installed into".into());
    notes.push("not carried: profiles, skills, plugins, runtime resources, startup actions, bundles (spec 0051, out of scope)".into());
    Ok(Imported { team, notes })
}

/// Read a file through either door. The path must be absolute and the file
/// small; the daemon canonicalises nothing here because the user named the
/// file, as they name a workspace folder.
pub fn import_file(path: &Path) -> Result<Imported> {
    if !path.is_absolute() {
        bail!("an import takes an absolute path, not {}", path.display());
    }
    let meta = std::fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    if meta.len() > MAX_FILE {
        bail!("{} is {} bytes — a team definition is kilobytes", path.display(), meta.len());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    match format_of(path)? {
        Format::Mogeung => import_json(&text),
        Format::Rig => import_rig(&text, path.parent().unwrap_or(Path::new("."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::team::{Policy, Wake, SeatEvent};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-rig-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The export is the definition whole, and the import of it is a new
    /// team — id dropped, stamps zeroed, the pause cleared, the rest intact.
    #[test]
    fn mogeung_json_round_trips_as_a_new_team() {
        let mut team = Team {
            id: "abc".into(),
            name: "Port OpenRig".into(),
            repo: "/home/you/mogeung".into(),
            guidance: "Read the spec.".into(),
            permission: Default::default(),
            policy: Policy {
                wakes: vec![Wake { on_seat: "planner".into(), on: SeatEvent::Stop, to: "builder".into(), text: "go".into() }],
                ..Policy::default()
            },
            seats: vec![Seat::new("planner", SessionSource::ClaudeCode), Seat::new("builder", SessionSource::Codex)],
            paused: true,
            created: 10,
            updated: 20,
        };
        team.seats[1].guidance = "Build it.".into();
        let json = export_json(&team).unwrap();
        assert!(json.contains("\"id\": \"abc\""), "whole: the id is in the file");
        assert_eq!(export_name(&team), "abc-port-openrig.json");
        let back = import_json(&json).unwrap();
        assert_eq!(back.team.id, "");
        assert_eq!((back.team.created, back.team.updated), (0, 0));
        assert!(!back.team.paused);
        assert_eq!(back.team.name, "Port OpenRig");
        assert_eq!(back.team.seats, team.seats);
        assert_eq!(back.team.policy, team.policy);
        assert!(back.notes.iter().any(|n| n.contains("new team")));
        assert!(back.notes.iter().any(|n| n.contains("paused")));
        assert!(back.notes.iter().any(|n| n.contains("/home/you/mogeung")));
        assert!(import_json("{ not json").is_err());
    }

    /// OpenRig's demo rig, read as other people's configuration: members
    /// become seats by runtime, terminals are skipped and said so, edges are
    /// text, the repository is asked for.
    #[test]
    fn an_openrig_rig_becomes_a_draft_with_notes() {
        let base = scratch("rig");
        std::fs::write(base.join("culture.md"), "# Culture\n\nQuality over speed.\n").unwrap();
        std::fs::create_dir_all(base.join("agents/lead/guidance")).unwrap();
        std::fs::write(
            base.join("agents/lead/agent.yaml"),
            "name: lead\nresources:\n  guidance:\n    - id: role\n      path: guidance/role.md\nstartup:\n  files:\n    - path: guidance/role.md\n",
        )
        .unwrap();
        std::fs::write(base.join("agents/lead/guidance/role.md"), "# Role: Lead\n\nPlan the work.\n").unwrap();
        std::fs::create_dir_all(base.join("agents/qa")).unwrap();
        std::fs::write(base.join("agents/qa/agent.yaml"), "name: qa\nresources:\n  guidance:\n    - path: guidance/missing.md\n").unwrap();
        let rig = r#"
version: "0.2"
name: demo-rig
summary: >
  Two pods and a terminal.
culture_file: culture.md
pods:
  - id: orch
    label: Orchestration
    members:
      - id: lead
        agent_ref: "local:agents/lead"
        runtime: claude-code
        cwd: .
    edges: []
  - id: dev
    members:
      - id: impl
        agent_ref: "local:agents/impl"
        runtime: claude-code
        cwd: packages/ui
      - id: qa
        agent_ref: "local:agents/qa"
        runtime: codex
        cwd: .
      - id: ui
        agent_ref: "builtin:terminal"
        runtime: terminal
        cwd: .
    edges:
      - kind: can_observe
        from: qa
        to: impl
edges:
  - kind: delegates_to
    from: orch.lead
    to: dev.impl
"#;
        let out = import_rig(rig, &base).unwrap();
        let t = &out.team;
        assert_eq!(t.name, "demo-rig");
        assert_eq!(t.id, "");
        assert_eq!(t.repo, "");
        assert_eq!(t.seats.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["lead", "impl", "qa"]);
        assert_eq!(t.seats[0].cli, SessionSource::ClaudeCode);
        assert_eq!(t.seats[2].cli, SessionSource::Codex);
        assert_eq!(t.seats[0].label, "orch.lead");
        assert_eq!(t.seats[0].guidance, "# Role: Lead\n\nPlan the work.");
        assert_eq!(t.seats[1].guidance, "", "an agent not beside the rig carries nothing");
        assert!(t.guidance.starts_with("Two pods and a terminal."), "{}", t.guidance);
        assert!(t.guidance.contains("Quality over speed."));
        assert!(t.guidance.contains("- dev.qa can observe dev.impl"), "{}", t.guidance);
        assert!(t.guidance.contains("- orch.lead delegates to dev.impl"));
        assert!(t.policy.wakes.is_empty(), "edges are text, never invented sends");
        let notes = out.notes.join("\n");
        assert!(notes.contains("skipped, not an agent CLI: `dev.ui` (runtime `terminal`)"), "{notes}");
        assert!(notes.contains("`impl` worked in `packages/ui`"), "{notes}");
        assert!(notes.contains("`impl`: agent `local:agents/impl` not found"), "{notes}");
        assert!(notes.contains("`qa`: guidance file(s) guidance/missing.md"), "{notes}");
        assert!(notes.contains("set the repository"), "{notes}");
        assert!(notes.contains("not carried: profiles"), "{notes}");
        crate::team::validate(t).expect("the draft is addressable as it stands");
        std::fs::remove_dir_all(&base).ok();
    }

    /// Names: member ids where unique, `<pod>-<member>` where two pods share
    /// one, always a valid seat name; and a rig with nothing to start refuses.
    #[test]
    fn member_ids_become_seat_names_and_an_empty_rig_refuses() {
        let base = scratch("names");
        let rig = r#"
version: "0.2"
name: twins
pods:
  - id: a
    members:
      - { id: Worker 1, runtime: codex }
      - { id: lead, runtime: claude-code }
  - id: b
    members:
      - { id: lead, runtime: opencode }
      - { id: 7x, runtime: qwen-code }
"#;
        let out = import_rig(rig, &base).unwrap();
        assert_eq!(
            out.team.seats.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["worker-1", "a-lead", "b-lead", "s-7x"]
        );
        assert_eq!(out.team.seats[2].cli, SessionSource::OpenCode);
        assert_eq!(out.team.seats[3].cli, SessionSource::QwenCode);
        crate::team::validate(&out.team).unwrap();
        assert_eq!(seat_name("Hello, World!"), "hello-world");
        assert_eq!(seat_name(""), "seat");
        assert!(import_rig("version: \"0.2\"\nname: x\npods: []\n", &base).is_err());
        assert!(import_rig("version: \"0.2\"\nname: x\npods:\n  - id: p\n    members:\n      - { id: t, runtime: terminal }\n", &base).is_err());
        assert!(import_rig("- a list\n", &base).is_err());
        assert_eq!(runtime_cli("terminal"), None);
        assert_eq!(runtime_cli("Claude-Code"), Some(SessionSource::ClaudeCode));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn the_door_is_chosen_by_the_name_and_refuses_the_rest() {
        assert_eq!(format_of(Path::new("/x/team.json")).unwrap(), Format::Mogeung);
        assert_eq!(format_of(Path::new("/x/rig.yaml")).unwrap(), Format::Rig);
        assert_eq!(format_of(Path::new("/x/rig.YML")).unwrap(), Format::Rig);
        assert!(format_of(Path::new("/x/team.toml")).is_err());
        assert!(import_file(Path::new("relative.json")).is_err(), "absolute only");
        assert!(import_file(Path::new("/nowhere/at/all.json")).is_err());
    }
}
