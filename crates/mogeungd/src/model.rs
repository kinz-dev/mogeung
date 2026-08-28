//! Asking the configured model. `R-O1`, `R-O5`.
//!
//! The policy — what is configured, whether it is allowed, and the sentence
//! that says why not — is in `mogeung_core::model`, so it can be tested with no
//! endpoint and so the two binaries cannot come to disagree about it. What is
//! here is the part that leaves the machine.
//!
//! **curl, not an HTTP client dependency.** `notify.rs` already made this trade
//! for the same reason: this is one POST on a human-initiated action, shelling
//! out cannot poison the async runtime, and the alternative drags a TLS stack
//! into a workspace that has managed without one. The body goes down **stdin**
//! rather than argv — a chat message is arbitrary user text of arbitrary
//! length, and neither argv limits nor quoting should ever be part of this.
//!
//! **Never on the scan tick** (ADR-0030 clause 6). Everything here runs on the
//! request path, started by something a person did.

use mogeung_core::model::{self as policy, ChatTurn, ModelHealth, ModelSettings};
use std::process::Stdio;
use std::sync::Mutex;

/// How long a single ask may take before curl gives up. Generous, because a
/// local model on a cold cache is slow in a way a web request is not — and the
/// panel shows the wait rather than hiding it.
const TIMEOUT_SECS: u64 = 120;
const CONNECT_TIMEOUT_SECS: u64 = 5;

/// What the daemon knows about its model, and the residue of the last ask.
#[derive(Debug, Default)]
pub struct Model {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    settings: ModelSettings,
    /// ADR-0030 clause 4, decided from the bind address at start-up — the same
    /// arrangement as `Runs::set_allowed`, so the start-up decision and the
    /// per-request gate cannot come to disagree.
    ///
    /// Defaults to **true** for the same reason `writes_allowed` defaults to
    /// allowed: an unconfigured caller is a test or a window hosting a daemon
    /// on loopback, and the real fence is `server::admit`, which refuses a
    /// non-loopback bind with no token before any of this exists.
    chat_allowed: bool,
    last_error: Option<String>,
    last_ok_ms: Option<u64>,
}

impl Model {
    pub fn new() -> Self {
        Model {
            inner: Mutex::new(Inner {
                chat_allowed: true,
                ..Inner::default()
            }),
        }
    }

    /// Called once by `server::prepare`, from the config and the flags.
    pub fn configure(&self, settings: ModelSettings) {
        let mut g = self.inner.lock().expect("model lock");
        g.settings = settings;
    }

    /// Called once by `server::run`, from the bind address.
    pub fn set_chat_allowed(&self, allowed: bool) {
        let mut g = self.inner.lock().expect("model lock");
        g.chat_allowed = allowed;
    }

    pub fn settings(&self) -> ModelSettings {
        self.inner.lock().expect("model lock").settings.clone()
    }

    /// The bind gate, for the verbs that are not the ask itself.
    ///
    /// The chat **history** is refused wherever chat is: it holds the same
    /// free-form text, and a daemon that will not take a question has no
    /// business handing back the last two hundred. `R-O9`.
    pub fn chat_allowed(&self) -> bool {
        self.inner.lock().expect("model lock").chat_allowed
    }

    /// The Health row, including what the last ask did.
    pub fn health(&self) -> Option<ModelHealth> {
        let g = self.inner.lock().expect("model lock");
        // Nothing configured is still worth a row: the window needs to say
        // *no model configured* rather than render an empty panel, which reads
        // as broken. `None` is reserved for a daemon that predates all this.
        let mut h = policy::health(&g.settings, g.chat_allowed);
        h.last_error = g.last_error.clone();
        h.last_ok_ms = g.last_ok_ms;
        Some(h)
    }

    /// Ask, and answer. `Err` is a sentence for a human — every failure here is
    /// something the panel shows rather than something it hides.
    pub async fn chat(&self, messages: &[ChatTurn]) -> Result<Answer, String> {
        let (settings, chat_allowed) = {
            let g = self.inner.lock().expect("model lock");
            (g.settings.clone(), g.chat_allowed)
        };
        if let Err(r) = policy::admit_chat(&settings, chat_allowed) {
            // A refusal is not recorded as `last_error`: nothing was attempted,
            // and a health row saying "the endpoint failed" when the endpoint
            // was never asked is the kind of wrong that costs an afternoon.
            return Err(r.message());
        }
        if messages.is_empty() {
            return Err("nothing to ask".into());
        }

        let base = policy::normalise_base(settings.url.as_deref().unwrap_or_default());
        let url = policy::chat_url(&base);
        let body = chat_body(settings.model.as_deref(), messages);
        let started = std::time::Instant::now();
        let raw = post_json(&url, &body).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let outcome = raw.and_then(|r| parse_reply(&r));
        let mut g = self.inner.lock().expect("model lock");
        match outcome {
            Ok((text, model)) => {
                g.last_error = None;
                g.last_ok_ms = Some(elapsed_ms);
                Ok(Answer { text, model, elapsed_ms })
            }
            Err(e) => {
                g.last_error = Some(e.clone());
                Err(e)
            }
        }
    }
}

/// One answer, and what actually produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub text: String,
    /// What answered, which may not be what was asked for — an endpoint is
    /// free to route `default` wherever it likes, and the panel says so.
    pub model: String,
    pub elapsed_ms: u64,
}

/// The request body, as JSON text. Pure, so the shape is a test rather than a
/// thing you learn from a 400.
pub fn chat_body(model: Option<&str>, messages: &[ChatTurn]) -> String {
    let mut body = serde_json::json!({
        "messages": messages,
        // Explicit rather than defaulted: this cut reads one whole answer, and
        // a server whose default is streaming would otherwise hand back a
        // `text/event-stream` nothing here can parse.
        "stream": false,
    });
    // Omitted rather than sent empty when unset, so the endpoint's own default
    // applies. `"model": ""` is a 400 on most servers.
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        body["model"] = serde_json::Value::String(m.to_string());
    }
    body.to_string()
}

/// POST and return the body, or a sentence saying why not.
async fn post_json(url: &str, body: &str) -> Result<String, String> {
    let mut child = tokio::process::Command::new("curl")
        .args([
            "-sS",
            "--connect-timeout",
            &CONNECT_TIMEOUT_SECS.to_string(),
            "-m",
            &TIMEOUT_SECS.to_string(),
            "-H",
            "Content-Type: application/json",
            // stdin, so no message can be too long or need quoting.
            "--data-binary",
            "@-",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run curl: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        // A closed pipe here means curl died before reading — its stderr says
        // why, and that is a better message than "broken pipe".
        let _ = stdin.write_all(body.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let out = child
        .wait_with_output()
        .await
        .map_err(|e| format!("curl did not finish: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("curl failed ({})", out.status)
        } else {
            format!("could not reach the model: {err}")
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Read an OpenAI-compatible chat completion.
///
/// Written to degrade rather than panic, for `A4`'s reason one layer out: this
/// is a private-ish shape from somebody else's server, and the failure that
/// matters is a confident wrong answer rather than a crash.
pub fn parse_reply(raw: &str) -> Result<(String, String), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("the endpoint returned nothing".into());
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        // Not JSON at all — a proxy's HTML error page, or the wrong URL. The
        // first line is worth more than "invalid JSON".
        return Err(format!("the endpoint did not answer with JSON: {}", excerpt(trimmed)));
    };

    // The error shape, which is JSON and would otherwise read as an empty answer.
    if let Some(e) = v.get("error") {
        let msg = e
            .get("message")
            .and_then(|m| m.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| e.to_string());
        return Err(format!("the model refused: {}", excerpt(&msg)));
    }

    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown model")
        .to_string();
    let message = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .ok_or_else(|| format!("no answer in the response: {}", excerpt(trimmed)))?;

    let content = message
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !content.is_empty() {
        return Ok((content, model));
    }

    // A thinking model can spend its whole budget reasoning and return an empty
    // `content` beside a full `reasoning_content` — which is what the endpoint
    // on this desk does. Showing nothing there looks like a bug in mogeung, so
    // the reasoning is offered and **labelled**, never passed off as the answer.
    let reasoning = message
        .get("reasoning_content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .trim();
    if !reasoning.is_empty() {
        return Ok((
            format!("_(the model returned only its reasoning)_\n\n{reasoning}"),
            model,
        ));
    }
    Err("the model answered with nothing".into())
}

/// A snippet fit for one line of a panel.
fn excerpt(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= 200 {
        return flat;
    }
    format!("{}…", flat.chars().take(200).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turns() -> Vec<ChatTurn> {
        vec![ChatTurn::user("what does -w do in git diff?")]
    }

    #[test]
    fn the_body_names_the_model_only_when_there_is_one() {
        let with = chat_body(Some("qwen3.8-sglang"), &turns());
        let v: serde_json::Value = serde_json::from_str(&with).unwrap();
        assert_eq!(v["model"], "qwen3.8-sglang");
        assert_eq!(v["stream"], false, "this cut reads one whole answer");
        assert_eq!(v["messages"][0]["role"], "user");

        // An unset model must be absent rather than empty: `"model": ""` is a
        // 400 on most servers, where absent means "yours".
        let without: serde_json::Value = serde_json::from_str(&chat_body(None, &turns())).unwrap();
        assert!(without.get("model").is_none());
        let blank: serde_json::Value =
            serde_json::from_str(&chat_body(Some("  "), &turns())).unwrap();
        assert!(blank.get("model").is_none());
    }

    /// The real shape, taken from the endpoint this was built against.
    #[test]
    fn a_real_answer_parses() {
        let raw = r#"{"id":"08b3","model":"RadixArk/Qwen3.8-27B-NVFP4","object":"chat.completion",
            "choices":[{"finish_reason":"stop","index":0,"message":{
              "content":"\n\nThe `-w` flag ignores whitespace.","role":"assistant",
              "reasoning_content":"User asks about -w."}}]}"#;
        let (text, model) = parse_reply(raw).unwrap();
        assert_eq!(text, "The `-w` flag ignores whitespace.");
        assert_eq!(model, "RadixArk/Qwen3.8-27B-NVFP4");
        assert!(!text.contains("User asks"), "the reasoning is not the answer");
    }

    /// A thinking model that spends its budget reasoning returns an empty
    /// `content`. Rendering that as an empty bubble reads as a mogeung bug, so
    /// the reasoning is shown and said to be reasoning.
    #[test]
    fn reasoning_only_is_offered_and_labelled_rather_than_shown_as_the_answer() {
        let raw = r#"{"model":"m","choices":[{"message":{"content":"",
            "reasoning_content":"I should think about this."}}]}"#;
        let (text, _) = parse_reply(raw).unwrap();
        assert!(text.contains("only its reasoning"), "{text}");
        assert!(text.contains("I should think about this."));
    }

    /// Every one of these used to be the same empty panel.
    #[test]
    fn every_failure_shape_says_something_a_human_can_act_on() {
        let cases = [
            ("", "returned nothing"),
            ("<html>404 Not Found</html>", "did not answer with JSON"),
            (r#"{"error":{"message":"model 'nope' not found"}}"#, "not found"),
            (r#"{"choices":[]}"#, "no answer in the response"),
            (r#"{"model":"m","choices":[{"message":{"content":""}}]}"#, "answered with nothing"),
        ];
        for (raw, want) in cases {
            let err = parse_reply(raw).unwrap_err();
            assert!(err.contains(want), "{raw:?} → {err:?}, wanted {want:?}");
        }
    }

    #[test]
    fn a_long_error_is_cut_to_one_line() {
        let long = "x".repeat(5000);
        let err = parse_reply(&format!("{{\"error\":{{\"message\":\"{long}\"}}}}")).unwrap_err();
        assert!(err.chars().count() < 300, "{}", err.chars().count());
        assert!(err.ends_with('…'));
    }

    /// The gate, from the daemon's side. Nothing is attempted and nothing is
    /// recorded as an endpoint failure — a health row blaming an endpoint that
    /// was never asked is worse than no row.
    #[tokio::test]
    async fn a_refusal_is_returned_without_touching_the_network_or_the_health_row() {
        let m = Model::new();
        let err = m.chat(&turns()).await.unwrap_err();
        assert!(err.contains("no model configured"), "{err}");
        assert!(m.health().unwrap().last_error.is_none(), "nothing was attempted");

        m.configure(ModelSettings {
            url: Some("http://spark-7ecc:8000/v1".into()),
            model: None,
            consent: mogeung_core::model::RemoteConsent::None,
        });
        let err = m.chat(&turns()).await.unwrap_err();
        assert!(err.contains("--allow-remote-model"), "{err}");
        assert!(m.health().unwrap().last_error.is_none());
    }

    /// ADR-0030 clause 4: no settings open this.
    #[tokio::test]
    async fn chat_is_refused_on_a_public_bind_however_it_is_configured() {
        let m = Model::new();
        m.configure(ModelSettings {
            url: Some("http://127.0.0.1:8000/v1".into()),
            model: Some("m".into()),
            consent: mogeung_core::model::RemoteConsent::Any,
        });
        m.set_chat_allowed(false);
        let err = m.chat(&turns()).await.unwrap_err();
        assert!(err.contains("loopback"), "{err}");
        assert!(!err.contains("--allow"), "must not imply a flag exists: {err}");
    }

    #[test]
    fn the_health_row_survives_having_no_configuration() {
        let h = Model::new().health().expect("a row even with nothing set");
        assert!(!h.configured);
        assert!(h.refusal.is_some(), "the window renders the reason, not a blank");
    }

    /// The failure of an ask that really happened *is* recorded, which is what
    /// makes the row worth reading. Port 9 is discard: nothing answers.
    #[tokio::test]
    async fn a_real_failure_lands_in_the_health_row() {
        let m = Model::new();
        m.configure(ModelSettings {
            url: Some("http://127.0.0.1:9/v1".into()),
            model: None,
            consent: mogeung_core::model::RemoteConsent::None,
        });
        let err = m.chat(&turns()).await.unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(m.health().unwrap().last_error.as_deref(), Some(err.as_str()));
    }
}
