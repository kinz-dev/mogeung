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

    /// Ask, and answer, forwarding the reply as it arrives. `R-O11`.
    ///
    /// `on_delta` is called with **coalesced** batches of new answer text, in
    /// order. It is never called with reasoning, and never after the returned
    /// future resolves — so a caller may forward each batch straight to a
    /// socket and then send the finished answer without racing itself.
    ///
    /// The returned `Answer` still carries the whole text. That redundancy is
    /// the design: the deltas are an early view, the answer is the truth, and
    /// a caller that ignores `on_delta` entirely behaves exactly as before.
    pub async fn chat_streaming<F>(
        &self,
        messages: &[ChatTurn],
        mut on_delta: F,
    ) -> Result<Answer, String>
    where
        F: FnMut(String) + Send,
    {
        self.ask(messages, &mut on_delta).await
    }

    /// Ask, and answer. `Err` is a sentence for a human — every failure here is
    /// something the panel shows rather than something it hides.
    pub async fn chat(&self, messages: &[ChatTurn]) -> Result<Answer, String> {
        self.ask(messages, &mut |_| {}).await
    }

    async fn ask(
        &self,
        messages: &[ChatTurn],
        on_delta: &mut (dyn FnMut(String) + Send),
    ) -> Result<Answer, String> {
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
        let outcome = post_streaming(&url, &body, on_delta).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
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
        // Streaming since `R-O11`. The panel shows the answer as it arrives,
        // which on a route to a large model is the difference between one
        // second and forty before anything appears at all.
        //
        // The reader degrades: a server that answers with one JSON object
        // despite this is parsed as one, so an endpoint that does not stream
        // still works rather than showing nothing.
        "stream": true,
    });
    // Omitted rather than sent empty when unset, so the endpoint's own default
    // applies. `"model": ""` is a 400 on most servers.
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        body["model"] = serde_json::Value::String(m.to_string());
    }
    body.to_string()
}

/// POST, read the stream as it arrives, and answer with the finished reply.
///
/// `-N` on curl is load-bearing: without it curl buffers its own stdout when
/// the destination is a pipe, and the whole point of this — text appearing
/// early — is lost while every test still passes, because the *content* is
/// identical and only the timing differs. That is the failure mode this
/// function exists to avoid, so the flag has a comment rather than being one
/// of nine in a list.
///
/// **It degrades to a single response.** A server that ignores `stream: true`
/// answers with one JSON object and no `data:` prefix; nothing is forwarded
/// live, the whole body is parsed by [`parse_reply`], and the panel simply
/// behaves as it did before streaming existed.
async fn post_streaming(
    url: &str,
    body: &str,
    on_delta: &mut (dyn FnMut(String) + Send),
) -> Result<(String, String), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = tokio::process::Command::new("curl")
        .args([
            "-sS",
            // No output buffering — see above.
            "-N",
            "--connect-timeout",
            &CONNECT_TIMEOUT_SECS.to_string(),
            "-m",
            &TIMEOUT_SECS.to_string(),
            "-H",
            "Content-Type: application/json",
            "-H",
            "Accept: text/event-stream",
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
        let _ = stdin.write_all(body.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    // Taken before the read loop, because after `wait` the handle is gone and
    // curl's own message is the only thing that explains a connection failure.
    let mut stderr = child.stderr.take();
    let stdout = child.stdout.take().ok_or("curl produced no output")?;
    let mut lines = BufReader::new(stdout).lines();
    let mut state = StreamState::default();
    let mut coalescer = Coalescer::default();
    // Kept whole as well, so a server that did not stream can still be read by
    // the single-response parser below.
    let mut whole = String::new();

    while let Ok(Some(line)) = lines.next_line().await {
        whole.push_str(&line);
        whole.push('\n');
        if let Some(delta) = state.push_line(&line) {
            if let Some(batch) = coalescer.push(&delta, std::time::Instant::now()) {
                on_delta(batch);
            }
        }
        if state.done || state.error.is_some() {
            break;
        }
    }
    // The tail, always — otherwise the last words of every answer would wait
    // for a flush that never comes.
    if let Some(batch) = coalescer.take() {
        on_delta(batch);
    }

    let status = child.wait().await;

    // Anything at all arrived as a stream — including a mid-stream error
    // frame. `finish` decides what it adds up to. A partial answer is kept
    // rather than discarded: a timeout half way through a long reply leaves
    // something worth reading, and throwing it away to report a failure the
    // user can already see is the wrong trade.
    if !(state.text.is_empty() && state.reasoning.is_empty() && state.error.is_none()) {
        return state.finish();
    }

    // Nothing streamed. Either the server answered whole despite the request,
    // or it failed — and curl's stderr is what tells those apart.
    let complaint = match stderr.take() {
        Some(mut e) => {
            use tokio::io::AsyncReadExt;
            let mut buf = String::new();
            let _ = e.read_to_string(&mut buf).await;
            buf.trim().to_string()
        }
        None => String::new(),
    };
    match status {
        Err(e) => Err(format!("curl did not finish: {e}")),
        Ok(s) if !s.success() => Err(if complaint.is_empty() {
            format!("curl failed ({s})")
        } else {
            format!("could not reach the model: {complaint}")
        }),
        // The degrade path: one whole JSON object, read the old way.
        Ok(_) => parse_reply(&whole),
    }
}

/// POST and return the body, or a sentence saying why not.
#[allow(dead_code)]
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

/// What an OpenAI-compatible stream has said so far. `R-O11`.
///
/// A struct rather than a parser function because a stream is a fold: each
/// line changes what is known, and the interesting failures — a frame that is
/// not JSON, an `error` object arriving mid-stream, a thinking model that
/// sends only `reasoning_content` — are all about what the *accumulated* state
/// then means. Pure, so every one of those is a test rather than something you
/// find out from a panel that showed nothing.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StreamState {
    /// The answer so far.
    pub text: String,
    /// Reasoning so far, kept apart. Never forwarded as it arrives: it is only
    /// ever shown **labelled**, and only when there is no answer at all.
    pub reasoning: String,
    /// What is actually answering, from whichever frame carried it.
    pub model: String,
    /// `[DONE]` seen.
    pub done: bool,
    /// An error frame. Stops the read and becomes the panel's message.
    pub error: Option<String>,
}

impl StreamState {
    /// Feed one line of the response, and answer with the text to forward now.
    ///
    /// `None` for everything that is not new visible answer text: comments,
    /// blank lines, `[DONE]`, role-only first frames, and reasoning. The
    /// caller forwards exactly what comes back and nothing else, so a change
    /// in what is *shown live* is a change here rather than in three places.
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim_end_matches(['\r', '\n']);
        // Comments (`: keep-alive`), `event:` lines and blank separators.
        let Some(payload) = line.strip_prefix("data:") else {
            return None;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            return None;
        }
        if payload == "[DONE]" {
            self.done = true;
            return None;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
            // A frame that is not JSON is skipped rather than fatal: one bad
            // frame must not lose an answer that is otherwise arriving fine.
            return None;
        };

        if let Some(e) = v.get("error") {
            let msg = e
                .get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| e.to_string());
            self.error = Some(format!("the model refused: {}", excerpt(&msg)));
            return None;
        }
        if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
            if !m.is_empty() {
                self.model = m.to_string();
            }
        }

        let choice = v.get("choices").and_then(|c| c.get(0));
        // `delta` while streaming; `message` if a server answers a streaming
        // request with a whole completion in one frame, which some do.
        let part = choice
            .and_then(|c| c.get("delta").or_else(|| c.get("message")))?;

        if let Some(r) = part.get("reasoning_content").and_then(|c| c.as_str()) {
            self.reasoning.push_str(r);
        }
        let content = part.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if content.is_empty() {
            return None;
        }
        self.text.push_str(content);
        Some(content.to_string())
    }

    /// What the whole stream added up to, under the same rules a single
    /// response follows — so the two paths cannot come to disagree about what
    /// an empty answer means.
    pub fn finish(self) -> Result<(String, String), String> {
        if let Some(e) = self.error {
            return Err(e);
        }
        let model = if self.model.is_empty() { "unknown model".to_string() } else { self.model };
        let text = self.text.trim();
        if !text.is_empty() {
            return Ok((text.to_string(), model));
        }
        // A thinking model can spend its whole budget reasoning and stream no
        // content at all. Offered and **labelled**, never passed off as the
        // answer — the same words the non-streaming path uses.
        let reasoning = self.reasoning.trim();
        if !reasoning.is_empty() {
            return Ok((
                format!("_(the model returned only its reasoning)_\n\n{reasoning}"),
                model,
            ));
        }
        Err("the model answered with nothing".into())
    }
}

/// How long deltas are held before being forwarded, and how much.
///
/// Coalescing is not an optimisation, it is what keeps the reply lane from
/// dropping a slow client mid-answer (`R-J59` bounds it at 256). A fast
/// endpoint emits hundreds of tokens a second; at one event each, a client
/// that stalls for a moment is disconnected for lag — a worse outcome than
/// the wait streaming exists to remove.
///
/// 60 ms is under the threshold where text stops looking live and well above
/// per-token, and the size cap means a burst is forwarded promptly rather than
/// waiting out the clock.
const COALESCE_MS: u64 = 60;
const COALESCE_CHARS: usize = 400;

/// Holds deltas briefly so a fast stream becomes a readable trickle.
///
/// Split out and given a clock parameter because the alternative — timing
/// logic inline in an async read loop — is the kind of thing that silently
/// stops flushing and shows nothing until the end, which looks exactly like
/// streaming not working at all.
#[derive(Debug, Default)]
pub struct Coalescer {
    buf: String,
    since: Option<std::time::Instant>,
}

impl Coalescer {
    /// Add a delta; answer with a batch when one is due.
    pub fn push(&mut self, delta: &str, now: std::time::Instant) -> Option<String> {
        if self.buf.is_empty() {
            self.since = Some(now);
        }
        self.buf.push_str(delta);
        let waited = self
            .since
            .map(|t| now.duration_since(t).as_millis() as u64)
            .unwrap_or(0);
        if waited >= COALESCE_MS || self.buf.chars().count() >= COALESCE_CHARS {
            return self.take();
        }
        None
    }

    /// Everything still held. Called at the end of a stream, so the tail is
    /// never left in the buffer.
    pub fn take(&mut self) -> Option<String> {
        self.since = None;
        (!self.buf.is_empty()).then(|| std::mem::take(&mut self.buf))
    }
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
        assert_eq!(v["stream"], true, "streaming since `R-O11`");
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
    /// A stream is a fold, so these walk one. `R-O11`.
    #[test]
    fn a_stream_becomes_an_answer_a_frame_at_a_time() {
        let mut st = StreamState::default();
        // Comments, blanks and the role-only opening frame carry no answer.
        assert_eq!(st.push_line(": keep-alive"), None);
        assert_eq!(st.push_line(""), None);
        assert_eq!(st.push_line("event: message"), None);
        assert_eq!(
            st.push_line(r#"data: {"model":"qwen","choices":[{"delta":{"role":"assistant"}}]}"#),
            None
        );
        assert_eq!(st.model, "qwen", "the model is learnt from whichever frame has it");

        assert_eq!(
            st.push_line(r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#).as_deref(),
            Some("Hello")
        );
        assert_eq!(
            st.push_line(r#"data: {"choices":[{"delta":{"content":", world"}}]}"#).as_deref(),
            Some(", world")
        );
        // Not JSON: skipped, never fatal — one bad frame must not lose an
        // answer that is otherwise arriving fine.
        assert_eq!(st.push_line("data: {oops"), None);
        assert_eq!(st.push_line("data: [DONE]"), None);
        assert!(st.done);

        assert_eq!(st.finish(), Ok(("Hello, world".to_string(), "qwen".to_string())));
    }

    /// The reasoning rule, and it must match the non-streaming path exactly —
    /// two answers to "what does an empty content mean" is how a panel comes
    /// to show different things for the same reply.
    #[test]
    fn reasoning_is_never_streamed_and_only_shown_when_it_is_all_there_is() {
        let mut st = StreamState::default();
        assert_eq!(
            st.push_line(r#"data: {"choices":[{"delta":{"reasoning_content":"thinking…"}}]}"#),
            None,
            "reasoning is accumulated, never forwarded as it arrives"
        );
        assert_eq!(st.push_line("data: [DONE]"), None);
        let (text, _) = st.finish().unwrap();
        assert!(text.starts_with("_(the model returned only its reasoning)_"), "{text}");
        assert!(text.contains("thinking…"));

        // With an answer as well, the reasoning is not shown at all.
        let mut st = StreamState::default();
        st.push_line(r#"data: {"choices":[{"delta":{"reasoning_content":"thinking…"}}]}"#);
        st.push_line(r#"data: {"choices":[{"delta":{"content":"42"}}]}"#);
        assert_eq!(st.finish().unwrap().0, "42");

        // Nothing at all is an error, not an empty bubble.
        assert!(StreamState::default().finish().is_err());
    }

    /// An error can arrive mid-stream, after text has already been shown. It
    /// wins: a refusal presented as a truncated answer is the worst outcome.
    #[test]
    fn an_error_frame_ends_the_stream_and_says_so() {
        let mut st = StreamState::default();
        st.push_line(r#"data: {"choices":[{"delta":{"content":"partial"}}]}"#);
        assert_eq!(
            st.push_line(r#"data: {"error":{"message":"context length exceeded"}}"#),
            None
        );
        assert!(st.error.is_some());
        let e = st.finish().unwrap_err();
        assert!(e.contains("context length exceeded"), "{e}");
    }

    /// Some servers answer a streaming request with one whole completion in a
    /// single frame, using `message` rather than `delta`.
    #[test]
    fn a_whole_message_in_one_frame_is_still_read() {
        let mut st = StreamState::default();
        assert_eq!(
            st.push_line(r#"data: {"model":"m","choices":[{"message":{"content":"whole"}}]}"#)
                .as_deref(),
            Some("whole")
        );
        assert_eq!(st.finish().unwrap(), ("whole".to_string(), "m".to_string()));
    }

    /// Coalescing is what keeps a fast endpoint from having a slow client
    /// dropped for lag (`R-J59` bounds the lane at 256). These pin both
    /// triggers and, most importantly, that the tail is never stranded.
    #[test]
    fn deltas_are_held_briefly_and_the_tail_is_always_flushed() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut c = Coalescer::default();

        // Under both thresholds: held.
        assert_eq!(c.push("a", t0), None);
        assert_eq!(c.push("b", t0 + Duration::from_millis(10)), None);
        // Past the interval: the batch comes out whole and in order.
        assert_eq!(
            c.push("c", t0 + Duration::from_millis(COALESCE_MS + 1)).as_deref(),
            Some("abc")
        );
        // And the clock restarts, so the next batch is not flushed instantly.
        assert_eq!(c.push("d", t0 + Duration::from_millis(COALESCE_MS + 2)), None);

        // A burst goes out on size rather than waiting out the clock.
        let big = "x".repeat(COALESCE_CHARS);
        assert!(c.push(&big, t0 + Duration::from_millis(COALESCE_MS + 3)).is_some());

        // The tail. Without this the last words of every answer would wait for
        // a flush that never comes.
        let mut c = Coalescer::default();
        assert_eq!(c.push("tail", t0), None);
        assert_eq!(c.take().as_deref(), Some("tail"));
        assert_eq!(c.take(), None, "and nothing twice");
    }

    /// The request says so, since `R-O11`. A server whose reply is one whole
    /// object is still read — that is `post_streaming`'s degrade path — but
    /// the ask is for a stream.
    #[test]
    fn the_body_asks_for_a_stream() {
        let body: serde_json::Value =
            serde_json::from_str(&chat_body(Some("m"), &turns())).unwrap();
        assert_eq!(body["stream"], serde_json::json!(true));
    }

}
