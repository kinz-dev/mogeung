//! A proxy of mogeung's own, in front of the model. `R-O10`.
//!
//! [ADR-0033](../../../docs/decisions/0033-a-proxy-of-our-own.md) is the
//! decision; this module is the half that can be tested with nothing running.
//! The spawn, the probe and the kill live in `mogeungd::llmproxy`, because a
//! child process belongs on the side that already owns processes.
//!
//! **Why a proxy at all.** `R-O5` asks one endpoint one question. A routing
//! proxy turns that into *the right model for this question* — a local model
//! for "what does this flag do" and a subscription-backed one for "why did this
//! diff regress" — without mogeung growing an opinion about models, which is
//! `R-O2`'s job and not a thing to guess at.
//!
//! **Why our own instance rather than yours.** Asked 2026-08-28: *"I don't want
//! to share a llmproxy server instance, because I want to be able to define its
//! own llmproxy rules."* Ask Mogeung asks different questions than a coding
//! agent does, and rules tuned for one are wrong for the other. llmproxy keys
//! its daemon metadata on the **bound address**, so a second instance on a
//! second port is a first-class arrangement there rather than a fight with the
//! one already running.

use serde::{Deserialize, Serialize};

/// Where mogeung's own proxy listens, derived from the daemon's own port.
///
/// Derived rather than random, and that is the load-bearing choice. A random
/// port has to be *written down somewhere* for the next start-up to find the
/// instance it left behind — and a file recording where a process is, is
/// exactly the stale-pid-file failure
/// [ADR-0009](../../../docs/decisions/0009-the-window-may-host-a-daemon.md)
/// rejected for this daemon's own port. Deriving it means the answer is
/// recomputed rather than remembered, so it cannot go stale.
///
/// `+1000` rather than `+1`: the neighbouring port is the likeliest thing to
/// already be taken by something related, and a thousand is far enough to be
/// nobody's neighbour while staying inside the ephemeral range for every
/// realistic daemon port.
pub fn derive_port(daemon_port: u16) -> u16 {
    daemon_port.saturating_add(1000)
}

/// What the daemon was told about the proxy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxySettings {
    /// Off unless asked for. `R-O5` works without any of this.
    pub enabled: bool,
    /// The binary, looked up on `PATH` unless this is a path.
    pub bin: String,
    /// The rules. mogeung's own file, never `~/.llmproxy/config.toml` — that
    /// one belongs to the agent CLIs and is the thing being kept separate.
    pub config: Option<std::path::PathBuf>,
    /// An explicit port, when the derived one will not do.
    pub port: Option<u16>,
}

impl Default for ProxySettings {
    fn default() -> Self {
        ProxySettings { enabled: false, bin: "llmproxy".into(), config: None, port: None }
    }
}

impl ProxySettings {
    pub fn port_for(&self, daemon_port: u16) -> u16 {
        self.port.unwrap_or_else(|| derive_port(daemon_port))
    }

    /// The base URL the model seam should use — the same `…/v1` shape a human
    /// would put in `model_url`, so nothing downstream has to know this is a
    /// proxy rather than an endpoint.
    pub fn url_for(&self, port: u16) -> String {
        format!("http://127.0.0.1:{port}/v1")
    }
}

/// What happened when the daemon tried. Ordered by how much explaining each
/// one owes the person reading the health row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProxyState {
    /// Not asked for. Every surface renders as it did before this existed.
    Off,
    /// Something was already serving that port and answered as an llmproxy, so
    /// it was left alone and used. Ours from a previous run, almost certainly —
    /// nothing else puts an llmproxy on a port derived from mogeung's.
    Adopted { port: u16 },
    /// This daemon started it, and will stop it.
    ///
    /// No pid: llmproxy re-execs itself as `--foreground` and detaches, so the
    /// process that was spawned is gone within a second and the one holding
    /// the port was never our child. The port is the identity that persists.
    Hosting { port: u16 },
    /// It could not be started, and the panel says so rather than timing out.
    Failed { reason: String },
}

/// The health row for the proxy. `R-O10`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyHealth {
    pub state: ProxyState,
    /// What the model seam was pointed at, when it was pointed anywhere.
    pub url: Option<String>,
    /// **Where this proxy forwards, off this machine.**
    ///
    /// The reason this field exists is the hole a proxy opens in
    /// [ADR-0031](../../../docs/decisions/0031-consent-to-a-named-host.md)
    /// clause 3: consent is decided from the endpoint's host, and a proxy on
    /// `127.0.0.1` is loopback, so the gate passes without asking while the
    /// bytes go to a vendor. mogeung cannot gate what a proxy forwards — but
    /// because it **wrote the config**, it can read it and say. An empty list
    /// means every provider in that file is on this machine.
    pub forwards_to: Vec<String>,
}

impl ProxyHealth {
    pub fn off() -> Self {
        ProxyHealth { state: ProxyState::Off, url: None, forwards_to: Vec::new() }
    }
}

/// The hosts a proxy config would send prompts to, that are not this machine.
///
/// Read out of the config text rather than asked of the running proxy: this
/// has to be answerable when the proxy is down, and a claim about where bytes
/// go must come from the file that decides it rather than from the process
/// that could be anything by the time it is asked.
///
/// Deliberately forgiving — it is a **disclosure, not a gate**. An entry it
/// cannot parse is skipped rather than treated as an error, because the cost
/// of a missed line here is an incomplete sentence in a health row, while the
/// cost of refusing to start over an unreadable `base_url` is a dead panel.
pub fn forwards_to(config_text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_provider = false;
    for line in config_text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            // Only `[providers.*]` names an upstream. `[integrated.targets.*]`
            // names a provider that is already covered by its own section, and
            // counting it would list the same host twice.
            in_provider = line.starts_with("[providers.") && !line.contains(".pricing");
            continue;
        }
        if !in_provider {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.trim() != "base_url" {
            continue;
        }
        let url = value.trim().trim_matches(['"', '\'']);
        let Some(host) = crate::model::host_of(url) else { continue };
        if crate::model::is_local_host(&host) {
            continue;
        }
        if !out.iter().any(|h| crate::model::same_host(h, &host)) {
            out.push(host);
        }
    }
    out
}

/// The config written on first start, when there is none.
///
/// **Written once and never touched again**, which is the whole contract: the
/// file is yours the moment it exists, and a starter that rewrote itself would
/// be a routing table that changes under you on upgrade.
///
/// It is deliberately dull. The only provider is whatever the daemon was
/// already pointed at, so turning the proxy on changes **nothing** about where
/// bytes go — day one answers come from the same place as the day before, and
/// adding a subscription-backed provider is an edit you make on purpose. A
/// starter that helpfully wired up `claude_sub` would be publishing by default
/// dressed as a convenience.
pub fn starter_config(listen: &str, upstream: &str) -> String {
    format!(
        r#"# mogeung's own llmproxy rules — `R-O10`, ADR-0033.
#
# This file was written once, because there was none, and mogeung will not
# touch it again. It is yours.
#
# It is separate from ~/.llmproxy/config.toml on purpose: the rules that suit a
# coding agent are not the rules that suit a chat panel, and that separation is
# the whole reason this instance exists.
#
# `listen_addr` is managed by mogeung — it is passed on the command line and
# derived from the daemon's own port, so editing it here has no effect.
listen_addr = "{listen}"

default_provider = "default"

# The endpoint mogeung was already using. Nothing here leaves this machine
# until you add a provider that does.
[providers.default]
kind = "openai-compatible"
base_url = "{upstream}"

# To route deep questions to a subscription-backed model, add a provider and a
# target. Both borrow the login the official CLI already wrote — no API key:
#
# [providers.claude_sub]
# kind = "anthropic"
# auth = "claude-oauth"
# base_url = "https://api.anthropic.com/v1"
#
# [integrated.targets.DEEP]
# provider = "claude_sub"
# model = "claude-opus-5"
# failover = ["LOCAL"]
# hint = "questions needing investigation or justification from evidence"
#
# [integrated.targets.LOCAL]
# provider = "default"
# model = "..."
# default = true
# hint = "everything else"
#
# Anything you add above is reported in mogeung's Health view as a host this
# proxy forwards to — mogeung cannot gate what a proxy does with a prompt, so
# it says where it goes instead.
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Derived, not remembered. The next start-up recomputes where it left the
    /// proxy rather than reading a file that could be describing a process that
    /// died last week.
    #[test]
    fn the_port_comes_out_of_the_daemons_own() {
        assert_eq!(derive_port(7717), 8717);
        assert_eq!(derive_port(7788), 8788);
        // Saturating rather than wrapping: the top of the range must not fold
        // round to a privileged port.
        assert_eq!(derive_port(u16::MAX), u16::MAX);

        let s = ProxySettings::default();
        assert_eq!(s.port_for(7717), 8717);
        assert_eq!(ProxySettings { port: Some(9000), ..s.clone() }.port_for(7717), 9000);
        assert_eq!(s.url_for(8717), "http://127.0.0.1:8717/v1");
    }

    /// The disclosure that stands in for a gate mogeung cannot have. `R-O10`.
    ///
    /// ADR-0031 clause 3 reads the **endpoint** host, and a proxy on loopback
    /// passes it while forwarding anywhere. This is the sentence that keeps the
    /// health row honest, so what it must never do is under-report.
    #[test]
    fn every_provider_that_leaves_the_machine_is_named() {
        let cfg = r#"
listen_addr = "127.0.0.1:8717"

[providers.default]
base_url = "http://spark-7ecc:8000/v1"

[providers.local]
base_url = "http://127.0.0.1:11434/v1"

[providers.claude_sub]
kind = "anthropic"
auth = "claude-oauth"
base_url = "https://api.anthropic.com/v1"

[providers.claude_sub.pricing]
base_url = "not-a-provider-url"

[integrated.targets.DEEP]
provider = "claude_sub"
"#;
        assert_eq!(
            forwards_to(cfg),
            vec!["spark-7ecc".to_string(), "api.anthropic.com".to_string()],
            "loopback is not forwarding; pricing and targets are not upstreams"
        );
    }

    /// A commented-out provider is not a provider. Getting this wrong would
    /// name a host in the Health view that nothing ever talks to, which teaches
    /// the reader to stop believing the row.
    #[test]
    fn a_commented_provider_is_not_reported() {
        let cfg = r#"
[providers.default]
base_url = "http://127.0.0.1:8000/v1"

# [providers.claude_sub]
# base_url = "https://api.anthropic.com/v1"
"#;
        assert!(forwards_to(cfg).is_empty());
        assert!(forwards_to("").is_empty());
        assert!(forwards_to("nonsense {{{").is_empty(), "unparseable is silent, never a panic");
    }

    /// Turning the proxy on must not move anybody's bytes. The starter names
    /// the endpoint that was already in use and nothing else.
    #[test]
    fn the_starter_config_forwards_exactly_where_we_already_pointed() {
        let cfg = starter_config("127.0.0.1:8717", "http://spark-7ecc:8000/v1");
        assert_eq!(forwards_to(&cfg), vec!["spark-7ecc".to_string()]);
        assert!(cfg.contains("listen_addr = \"127.0.0.1:8717\""));
        // The subscription example is present but commented, so it documents
        // the next step without taking it.
        assert!(cfg.contains("# auth = \"claude-oauth\""));

        let local = starter_config("127.0.0.1:8717", "http://127.0.0.1:8000/v1");
        assert!(forwards_to(&local).is_empty(), "a local endpoint forwards nowhere");
    }

    #[test]
    fn off_is_a_state_and_not_an_error() {
        let h = ProxyHealth::off();
        assert_eq!(h.state, ProxyState::Off);
        assert!(h.url.is_none() && h.forwards_to.is_empty());
        assert!(!ProxySettings::default().enabled);
    }
}
