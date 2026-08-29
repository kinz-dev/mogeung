//! The local model seam. `R-O1`.
//!
//! mogeung may employ a model to read the evidence it already holds — see
//! [ADR-0030](../../../docs/decisions/0030-a-model-reads-the-evidence.md). This
//! module holds the parts both binaries need: what is configured, whether it is
//! allowed, and the one-line reason when it is not.
//!
//! **Everything here is pure.** The call itself lives in `mogeungd::model`,
//! because a request that leaves the machine belongs on the side that already
//! owns processes and sockets. What is here is the policy, so the policy can be
//! tested without an endpoint and so the window and the daemon cannot come to
//! disagree about it.

use serde::{Deserialize, Serialize};

/// One turn of a chat. The daemon stores none of these: the conversation lives
/// in the window and is sent whole on every ask, which is what makes `R-O5`
/// ephemeral by construction rather than by a promise to delete something.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTurn {
    /// `user` or `assistant`. Not an enum: this is forwarded verbatim to an
    /// OpenAI-compatible endpoint, and a server that grows a third role should
    /// not need a mogeung release.
    pub role: String,
    pub content: String,
}

impl ChatTurn {
    pub fn user(content: impl Into<String>) -> Self {
        ChatTurn { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        ChatTurn { role: "assistant".into(), content: content.into() }
    }
}

/// Consent to reach a model endpoint that is not this machine.
/// [ADR-0031](../../../docs/decisions/0031-consent-to-a-named-host.md) clause 3.
///
/// Three states, and the middle one is the point: consent normally **names the
/// host it is consent for**, so pointing `model_url` somewhere else asks again
/// rather than inheriting a yes given for a different machine.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum RemoteConsent {
    /// Nothing granted. Loopback endpoints still work; anything else is
    /// refused. The ordinary state, and the default.
    #[default]
    None,
    /// Any host. What `--allow-remote-model` grants, and what
    /// `allow_remote_model = true` grants — honest, blanket, and no weaker
    /// than the flag has always been.
    Any,
    /// This host and no other. `allow_remote_model = "spark-7ecc"`.
    Host(String),
}

impl RemoteConsent {
    /// Does this consent cover `host`? Compared through [`host_of`], so a
    /// bare name, a `name:port` and a whole URL all work — the config file is
    /// hand-written and `http://spark-7ecc:8000/v1` is what is on the
    /// clipboard when someone reaches for this key.
    pub fn covers(&self, host: &str) -> bool {
        match self {
            RemoteConsent::None => false,
            RemoteConsent::Any => true,
            RemoteConsent::Host(h) => match (host_of(h), host_of(host)) {
                (Some(a), Some(b)) => same_host(&a, &b),
                _ => false,
            },
        }
    }

    /// The host this consent names, for the refusal that has to explain a
    /// mismatch. `None` for both the absent and the blanket case, neither of
    /// which can mismatch.
    pub fn named(&self) -> Option<&str> {
        match self {
            RemoteConsent::Host(h) => Some(h),
            _ => None,
        }
    }

    pub fn granted(&self) -> bool {
        !matches!(self, RemoteConsent::None)
    }
}

/// `false` / absent, `true`, or a host name — the three things someone
/// actually writes in a TOML file, read as the three states.
///
/// Serialised back as the same shapes, so a round trip through the file does
/// not turn a named host into something the next version has to guess at.
impl Serialize for RemoteConsent {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            RemoteConsent::None => s.serialize_bool(false),
            RemoteConsent::Any => s.serialize_bool(true),
            RemoteConsent::Host(h) => s.serialize_str(h),
        }
    }
}

impl<'de> Deserialize<'de> for RemoteConsent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Flag(bool),
            Host(String),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Flag(true) => RemoteConsent::Any,
            Raw::Flag(false) => RemoteConsent::None,
            // An empty string is someone clearing the key rather than
            // consenting to a host called "". Read it as they meant it.
            Raw::Host(h) if h.trim().is_empty() => RemoteConsent::None,
            Raw::Host(h) => RemoteConsent::Host(h.trim().to_string()),
        })
    }
}

/// What the daemon was configured with, and what that adds up to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelSettings {
    /// Base URL of an OpenAI-compatible API — the part before `/chat/completions`,
    /// e.g. `http://spark-7ecc:8000/v1`.
    pub url: Option<String>,
    /// Model id to ask for, as the endpoint's own `/models` lists it.
    pub model: Option<String>,
    /// ADR-0031 clause 3: a non-loopback endpoint is publishing, and needs
    /// consent said out loud — the flag, or the config key that names the host.
    pub consent: RemoteConsent,
    /// Which embedding model to ask this **same** endpoint for. `R-O6`.
    ///
    /// On the settings rather than beside them because it shares the URL and
    /// therefore the consent: one host is named, and embeddings cannot quietly
    /// go somewhere else.
    #[serde(default)]
    pub embed_model: Option<String>,
}

impl ModelSettings {
    pub fn configured(&self) -> bool {
        self.url.as_ref().is_some_and(|u| !u.trim().is_empty())
    }

    /// The host of the configured endpoint, if there is one.
    pub fn host(&self) -> Option<String> {
        self.url.as_deref().and_then(host_of)
    }

    /// Is the endpoint somewhere other than this machine?
    ///
    /// **No DNS is done, deliberately.** A hostname that happens to resolve to
    /// `127.0.0.1` is still treated as remote, so this fails closed: the worst
    /// case is being asked for a flag you did not strictly need, rather than
    /// posting transcripts to a host nobody consented to. Resolving would also
    /// make the answer depend on the network at the moment it was asked, which
    /// is not a property a safety check should have.
    pub fn remote(&self) -> bool {
        match self.host() {
            Some(h) => !is_local_host(&h),
            // Unconfigured is not remote; it is nothing. The refusal for that
            // case is `NotConfigured`, which says something more useful.
            None => false,
        }
    }
}

/// Why a model request was refused, before it was made.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// Nothing is configured. Not an error — the ordinary state of a fresh
    /// install, and every surface must render as it did before models existed.
    NotConfigured,
    /// The endpoint is not on this machine and nobody said that was intended.
    RemoteEndpoint { host: String },
    /// Consent was given, but for a different machine. ADR-0031's whole point:
    /// a yes said for one host is not a yes for the next one.
    ConsentNamesAnotherHost { host: String, consented: String },
    /// ADR-0030 clause 4: the free-form ask, on a daemon reachable from the
    /// network. There is no flag for this one.
    PublicBind,
}

impl Refusal {
    /// One line, written for someone who has not read the source — and, where
    /// there is a way out, naming it. A refusal that does not say what to do
    /// instead gets worked around by whatever the internet suggests first,
    /// which is `server::admit`'s lesson.
    pub fn message(&self) -> String {
        match self {
            Refusal::NotConfigured => "no model configured — set `model_url` in \
                 ~/.mogeung/config.toml, or pass --model-url"
                .to_string(),
            Refusal::RemoteEndpoint { host } => format!(
                "refusing to send anything to {host}: it is not this machine, and \
                 a model endpoint elsewhere is publishing (ADR-0031 clause 3, \
                 ADR-0014). Name it in ~/.mogeung/config.toml — \
                 allow_remote_model = \"{host}\" — or start the daemon with \
                 --allow-remote-model."
            ),
            Refusal::ConsentNamesAnotherHost { host, consented } => format!(
                "refusing to send anything to {host}: allow_remote_model consents \
                 to {consented}, which is a different machine. Consent names a \
                 host on purpose (ADR-0031 clause 3) — set model_url back to \
                 {consented}, or change allow_remote_model to \"{host}\"."
            ),
            Refusal::PublicBind => "chat is refused on a daemon bound beyond \
                 loopback (ADR-0030 clause 4): it carries free-form text, and a \
                 daemon anyone can reach must not become a general-purpose LLM \
                 proxy. There is no flag for this — bind 127.0.0.1 and reach it \
                 over ssh."
                .to_string(),
        }
    }
}

/// The settings check every model request passes, chat or not.
pub fn admit(settings: &ModelSettings) -> Result<(), Refusal> {
    if !settings.configured() {
        return Err(Refusal::NotConfigured);
    }
    if settings.remote() {
        let host = settings.host().unwrap_or_else(|| "that host".into());
        if !settings.consent.covers(&host) {
            // Two different sentences, because they are two different
            // mistakes. "You never said" and "you said, about somewhere else"
            // send you to different lines of the same file.
            return Err(match settings.consent.named() {
                Some(consented) => Refusal::ConsentNamesAnotherHost {
                    host,
                    consented: consented.to_string(),
                },
                None => Refusal::RemoteEndpoint { host },
            });
        }
    }
    Ok(())
}

/// The extra check the free-form ask passes. `chat_allowed` comes from the bind
/// address, decided once at start-up the way `runs_allowed` is — two places
/// that both compute "is this safe" are two places that can come to disagree.
pub fn admit_chat(settings: &ModelSettings, chat_allowed: bool) -> Result<(), Refusal> {
    admit(settings)?;
    if !chat_allowed {
        return Err(Refusal::PublicBind);
    }
    Ok(())
}

/// ADR-0030 clause 4, decided from the bind address.
///
/// Loopback is the same trust boundary as the terminal panel, which can already
/// run anything. Unlike [`crate::model::admit`]'s remote-endpoint gate there is
/// **no flag** here: an escape hatch that exists is one that becomes the
/// documented workaround.
pub fn chat_allowed(bind: std::net::SocketAddr) -> bool {
    bind.ip().is_loopback()
}

/// What the Health view says about the model, so "is it on?" is answerable
/// without making a call. Never probed on the scan tick — `last_error` and
/// `last_ok` are the residue of asks that actually happened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelHealth {
    pub configured: bool,
    /// Host only, never the full URL: a URL can carry a key in a query string
    /// and this is rendered in a window and pasted into bug reports.
    pub host: Option<String>,
    pub model: Option<String>,
    pub remote: bool,
    /// The settings gate passes — the endpoint may be asked.
    pub allowed: bool,
    /// The bind gate passes — the chat panel may ask.
    pub chat_allowed: bool,
    /// Why not, when one of the two is false.
    pub refusal: Option<String>,
    /// The last failure, and the last success, in wall-clock milliseconds.
    pub last_error: Option<String>,
    pub last_ok_ms: Option<u64>,
}

/// Build the health row from settings and the bind decision.
pub fn health(settings: &ModelSettings, chat_allowed: bool) -> ModelHealth {
    let refusal = admit_chat(settings, chat_allowed).err();
    ModelHealth {
        configured: settings.configured(),
        host: settings.host(),
        model: settings.model.clone(),
        remote: settings.remote(),
        allowed: admit(settings).is_ok(),
        chat_allowed,
        refusal: refusal.map(|r| r.message()),
        last_error: None,
        last_ok_ms: None,
    }
}

/// The first thing you asked, on one line. `R-O9`.
///
/// Derived rather than entered — a conversation you have to name before you
/// can have one is a conversation you do not start. Whitespace is collapsed
/// because a pasted stack trace's first line is mostly indentation, and the
/// cut counts **characters, not bytes**, so a question in any script survives
/// it intact.
pub fn chat_title(turns: &[ChatTurn]) -> String {
    let first = turns
        .iter()
        .find(|t| t.role == "user")
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let flat = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        // A title has to be *something*: a blank row in the history is a door
        // you cannot tell from a rendering bug.
        return "(empty)".to_string();
    }
    if flat.chars().count() <= 72 {
        return flat;
    }
    let cut: String = flat.chars().take(71).collect();
    format!("{}…", cut.trim_end())
}

/// The host out of a URL, without a URL crate.
///
/// Handles the three shapes that actually turn up in a config file: a scheme or
/// none, a port or none, and a bracketed IPv6 literal. Anything it cannot read
/// is `None`, which the callers treat as *not local* rather than as *fine*.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next()?;
    // `user:pass@host` — the last `@` wins, since a password may contain one.
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if let Some(after) = authority.strip_prefix('[') {
        let (h, _) = after.split_once(']')?;
        return (!h.is_empty()).then(|| h.to_string());
    }
    let h = authority.split(':').next()?;
    (!h.is_empty()).then(|| h.to_string())
}

/// Are these two host names the same machine, as far as we are willing to say
/// without asking the network?
///
/// Case-folded and bracket-stripped, and **nothing more** — no DNS, no suffix
/// matching. `spark-7ecc` and `spark-7ecc.local` are two names here, which is
/// the direction this has to fail in: the cost of being strict is re-reading
/// one line of a config file, and the cost of being clever is consent that
/// silently covers a host nobody named.
pub fn same_host(a: &str, b: &str) -> bool {
    let norm = |h: &str| h.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    norm(a) == norm(b)
}

/// Is this host name or address this machine?
pub fn is_local_host(host: &str) -> bool {
    let h = host.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    if h == "localhost" {
        return true;
    }
    h.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// The `/chat/completions` URL for a base like `http://host:8000/v1`.
///
/// A trailing slash in the config file is the likeliest typo and must not
/// produce a `//` that some servers 404 on.
pub fn chat_url(base: &str) -> String {
    format!("{}/chat/completions", base.trim_end_matches('/'))
}

/// The `/models` URL, which is what a human pastes when asked for the endpoint
/// — so it is also what the config file is likely to be given by mistake.
pub fn models_url(base: &str) -> String {
    format!("{}/models", base.trim_end_matches('/'))
}

/// Read a base URL as forgivingly as is safe.
///
/// The endpoint people have in their shell history is the one they can `curl`,
/// which is `…/v1/models` — pasting that into `model_url` would otherwise ask
/// for `…/v1/models/chat/completions` and fail with a 404 nobody can read. The
/// suffix is stripped rather than refused, because refusing a URL that works in
/// curl is a worse first five minutes.
pub fn normalise_base(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    for tail in ["/models", "/chat/completions"] {
        if let Some(base) = u.strip_suffix(tail) {
            return base.to_string();
        }
    }
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(url: &str, consent: RemoteConsent) -> ModelSettings {
        ModelSettings {
            url: Some(url.into()),
            model: Some("qwen3.8-sglang".into()),
            consent,
            embed_model: None,
        }
    }

    /// Shorthand for the two states that were the whole world before ADR-0031.
    fn no(url: &str) -> ModelSettings {
        settings(url, RemoteConsent::None)
    }
    fn any(url: &str) -> ModelSettings {
        settings(url, RemoteConsent::Any)
    }

    #[test]
    fn a_host_comes_out_of_every_url_shape_we_might_be_given() {
        assert_eq!(host_of("http://spark-7ecc:8000/v1").as_deref(), Some("spark-7ecc"));
        assert_eq!(host_of("http://127.0.0.1:8000/v1").as_deref(), Some("127.0.0.1"));
        assert_eq!(host_of("https://user:pa@ss@example.com/v1").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://[::1]:8000/v1").as_deref(), Some("::1"));
        assert_eq!(host_of("localhost:8000").as_deref(), Some("localhost"));
        assert_eq!(host_of(""), None);
    }

    /// The direction this must fail in. A name we cannot read, or one that only
    /// DNS could resolve, counts as **remote** — so the worst case is a flag
    /// you did not need, never a transcript posted to a host nobody named.
    #[test]
    fn only_literal_loopback_counts_as_local() {
        for local in ["localhost", "127.0.0.1", "127.1.2.3", "::1", "[::1]"] {
            assert!(is_local_host(local), "{local} is this machine");
        }
        for remote in ["spark-7ecc", "example.com", "192.168.1.5", "0.0.0.0"] {
            assert!(!is_local_host(remote), "{remote} must not read as local");
        }
    }

    #[test]
    fn nothing_configured_is_not_an_error_it_is_a_state() {
        let none = ModelSettings::default();
        assert!(!none.configured());
        assert_eq!(admit(&none), Err(Refusal::NotConfigured));
        assert!(!none.remote(), "unconfigured is not remote, it is nothing");
    }

    /// ADR-0031 clause 3. This is the case the user's own desk is in.
    #[test]
    fn a_remote_endpoint_needs_consent_and_the_refusal_names_both_ways_to_give_it() {
        let s = no("http://spark-7ecc:8000/v1");
        assert!(s.remote());
        let err = admit(&s).unwrap_err();
        assert_eq!(err, Refusal::RemoteEndpoint { host: "spark-7ecc".into() });
        let msg = err.message();
        assert!(msg.contains("--allow-remote-model"), "{msg}");
        assert!(msg.contains("allow_remote_model"), "the file is reachable too: {msg}");
        assert!(msg.contains("spark-7ecc"), "the refusal must name the host: {msg}");

        assert!(admit(&any("http://spark-7ecc:8000/v1")).is_ok());
    }

    /// The property that made a config key admissible where `--allow-run` has
    /// no twin: consent is **to a host**, so it does not travel with the URL.
    #[test]
    fn consent_to_one_host_is_not_consent_to_the_next() {
        let named = RemoteConsent::Host("spark-7ecc".into());
        assert!(admit(&settings("http://spark-7ecc:8000/v1", named.clone())).is_ok());

        let elsewhere = settings("http://api.example.com/v1", named.clone());
        let err = admit(&elsewhere).unwrap_err();
        assert_eq!(
            err,
            Refusal::ConsentNamesAnotherHost {
                host: "api.example.com".into(),
                consented: "spark-7ecc".into(),
            }
        );
        let msg = err.message();
        assert!(msg.contains("api.example.com") && msg.contains("spark-7ecc"), "{msg}");

        // Case and the shapes a hand-written file arrives in. The URL form is
        // what is on the clipboard when someone reaches for this key.
        for written in ["SPARK-7ecc", " spark-7ecc ", "spark-7ecc:8000", "http://spark-7ecc:8000/v1"] {
            let c = RemoteConsent::Host(written.into());
            assert!(c.covers("spark-7ecc"), "{written} should cover the host it names");
        }
        // And nothing clever: a different name for possibly the same machine
        // is still a different name.
        assert!(!named.covers("spark-7ecc.local"));
        assert!(!RemoteConsent::None.covers("spark-7ecc"));
        assert!(RemoteConsent::Any.covers("anything-at-all"));
    }

    /// The three things someone writes in a TOML file, read as the three
    /// states — and an empty string read as clearing the key rather than as
    /// consenting to a host called "".
    #[test]
    fn the_config_key_reads_the_shapes_people_write() {
        let parse = |v: &str| {
            #[derive(Deserialize)]
            struct T {
                k: RemoteConsent,
            }
            toml::from_str::<T>(&format!("k = {v}")).unwrap().k
        };
        assert_eq!(parse("true"), RemoteConsent::Any);
        assert_eq!(parse("false"), RemoteConsent::None);
        assert_eq!(parse("\"spark-7ecc\""), RemoteConsent::Host("spark-7ecc".into()));
        assert_eq!(parse("\"  \""), RemoteConsent::None);
        assert!(!RemoteConsent::default().granted());
    }

    #[test]
    fn a_loopback_endpoint_needs_no_consent() {
        assert!(admit(&no("http://127.0.0.1:8000/v1")).is_ok());
        assert!(admit(&no("http://localhost:8000/v1")).is_ok());
    }

    /// ADR-0030 clause 4, and the property that distinguishes it from clause 3:
    /// there is no flag, so no combination of settings opens it.
    #[test]
    fn chat_is_refused_off_loopback_with_no_way_round_it() {
        let s = any("http://127.0.0.1:8000/v1");
        assert!(admit_chat(&s, true).is_ok());
        assert_eq!(admit_chat(&s, false), Err(Refusal::PublicBind));
        let msg = Refusal::PublicBind.message();
        assert!(msg.contains("ssh"), "must offer the way out: {msg}");
        assert!(!msg.contains("--allow"), "must not imply a flag exists: {msg}");
    }

    #[test]
    fn the_bind_decides_whether_chat_is_allowed() {
        let at = |s: &str| s.parse::<std::net::SocketAddr>().unwrap();
        assert!(chat_allowed(at("127.0.0.1:7717")));
        assert!(chat_allowed(at("[::1]:7717")));
        assert!(!chat_allowed(at("0.0.0.0:7717")));
        assert!(!chat_allowed(at("192.168.1.5:7717")));
    }

    /// The URL a human can `curl` is `…/v1/models`, and that is what gets
    /// pasted into the config file. Asking for `…/v1/models/chat/completions`
    /// afterwards fails with a 404 nobody can read.
    #[test]
    fn the_models_url_is_accepted_as_a_base() {
        assert_eq!(normalise_base("http://spark-7ecc:8000/v1/models"), "http://spark-7ecc:8000/v1");
        assert_eq!(normalise_base("http://spark-7ecc:8000/v1/"), "http://spark-7ecc:8000/v1");
        assert_eq!(normalise_base("http://spark-7ecc:8000/v1"), "http://spark-7ecc:8000/v1");
        assert_eq!(
            chat_url(&normalise_base("http://spark-7ecc:8000/v1/models")),
            "http://spark-7ecc:8000/v1/chat/completions"
        );
        assert_eq!(models_url("http://h:1/v1/"), "http://h:1/v1/models");
    }

    /// The health row is what the window renders when there is nothing to
    /// render, so it has to carry the reason rather than just the absence.
    #[test]
    fn health_carries_the_refusal_rather_than_only_the_absence() {
        let h = health(&no("http://spark-7ecc:8000/v1"), true);
        assert!(h.configured);
        assert!(h.remote);
        assert!(!h.allowed);
        assert_eq!(h.host.as_deref(), Some("spark-7ecc"));
        assert!(h.refusal.unwrap().contains("--allow-remote-model"));

        let off = health(&ModelSettings::default(), true);
        assert!(!off.configured);
        assert!(off.refusal.unwrap().contains("no model configured"));

        let ok = health(&no("http://127.0.0.1:8000/v1"), true);
        assert!(ok.allowed && ok.chat_allowed && ok.refusal.is_none());
    }

    /// A history is scanned, not read, so the title has to survive the two
    /// things people actually paste into a chat box: something indented, and
    /// something very long. `R-O9`.
    #[test]
    fn a_title_is_the_first_ask_flattened_onto_one_line() {
        let ask = |c: &str| chat_title(&[ChatTurn::user(c), ChatTurn::assistant("...")]);

        assert_eq!(ask("why is the queue empty"), "why is the queue empty");
        // A pasted stack trace's first line is mostly indentation.
        assert_eq!(ask("  thread 'main'\n    at foo\n"), "thread 'main' at foo");

        let long = ask(&"x".repeat(200));
        assert_eq!(long.chars().count(), 72, "71 and the ellipsis");
        assert!(long.ends_with('…'));

        // Counted in characters, not bytes — otherwise this panics on a
        // boundary or truncates into nonsense.
        let korean = ask(&"모괴".repeat(100));
        assert_eq!(korean.chars().count(), 72);

        // The assistant never names the conversation, and a title is always
        // something rather than a blank row you cannot tell from a bug.
        assert_eq!(chat_title(&[ChatTurn::assistant("hello")]), "(empty)");
        assert_eq!(chat_title(&[]), "(empty)");
        assert_eq!(ask("   "), "(empty)");
    }

    /// The host is rendered in a window and pasted into bug reports; a URL can
    /// carry a key in its query string and must not travel with it.
    #[test]
    fn health_publishes_the_host_and_never_the_url() {
        let h = health(&any("http://spark-7ecc:8000/v1?api-key=hunter2"), true);
        assert_eq!(h.host.as_deref(), Some("spark-7ecc"));
        let rendered = format!("{h:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
    }
}
