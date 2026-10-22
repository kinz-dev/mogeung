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
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// How long a single ask may take before curl gives up. Generous, because a
/// local model on a cold cache is slow in a way a web request is not — and the
/// panel shows the wait rather than hiding it.
const TIMEOUT_SECS: u64 = 120;
const CONNECT_TIMEOUT_SECS: u64 = 5;

/// What the daemon knows about its model, and the residue of the last ask.
#[derive(Debug, Default)]
pub struct Model {
    inner: Mutex<Inner>,
    /// The chat answers in flight, by the client's id, each with the bell that
    /// stops it. `R-J118`. Registered for the life of one answer and removed
    /// when it ends, however it ends.
    stops: Mutex<HashMap<String, Arc<Notify>>>,
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
    /// `ask_agent` from the config file. `R-O14`. **Defaults to true** in
    /// [`Model::new`], matching the file's absent-means-yes.
    agent_enabled: bool,
    /// What the last probe found: what will answer, or why plain chat will.
    /// For the Health row only; every ask probes again.
    agent: Option<String>,
    agent_note: Option<String>,
}

impl Model {
    pub fn new() -> Self {
        Model {
            inner: Mutex::new(Inner {
                chat_allowed: true,
                agent_enabled: true,
                ..Inner::default()
            }),
            stops: Mutex::new(HashMap::new()),
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
    /// `ask_agent` from the config file, at start-up and on a save. `R-O14`.
    pub fn set_agent_enabled(&self, enabled: bool) {
        let mut g = self.inner.lock().expect("model lock");
        g.agent_enabled = enabled;
        if !enabled {
            g.agent = None;
            g.agent_note = None;
        }
    }

    pub fn agent_enabled(&self) -> bool {
        self.inner.lock().expect("model lock").agent_enabled
    }

    /// Remember what a probe found, for the Health row.
    pub fn note_agent(&self, probe: &Result<crate::askagent::Ready, mogeung_core::askagent::Unavailable>) {
        let mut g = self.inner.lock().expect("model lock");
        match probe {
            Ok(r) => {
                g.agent = Some(r.provenance());
                g.agent_note = None;
            }
            Err(u) => {
                g.agent = None;
                g.agent_note = u.note();
            }
        }
    }

    /// Probe on a blocking thread, and remember what it found.
    pub async fn probe_agent(&self) -> Result<crate::askagent::Ready, mogeung_core::askagent::Unavailable> {
        let enabled = self.agent_enabled();
        let probe = tokio::task::spawn_blocking(move || crate::askagent::probe(enabled))
            .await
            .unwrap_or_else(|e| Err(mogeung_core::askagent::Unavailable::BwrapFails(format!("the probe failed: {e}"))));
        self.note_agent(&probe);
        probe
    }

    pub fn health(&self) -> Option<ModelHealth> {
        self.health_for(false)
    }

    /// The Health row as a client on **this machine** should read it.
    /// `R-J103`, ADR-0030's 2026-09-23 amendment: clause 4 refuses the chat to
    /// a client that is not on this machine, so the row a loopback client is
    /// shown must not carry the refusal meant for everyone else.
    pub fn health_for(&self, from_this_machine: bool) -> Option<ModelHealth> {
        let g = self.inner.lock().expect("model lock");
        // Nothing configured is still worth a row: the window needs to say
        // *no model configured* rather than render an empty panel, which reads
        // as broken. `None` is reserved for a daemon that predates all this.
        let mut h = policy::health(&g.settings, g.chat_allowed || from_this_machine);
        h.last_error = g.last_error.clone();
        h.last_ok_ms = g.last_ok_ms;
        h.ask_agent = g.agent.clone();
        h.ask_agent_note = g.agent_note.clone();
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
    ///
    /// `from_this_machine` is whether the client that asked is on this
    /// machine — ADR-0030 clause 4 as amended 2026-09-23 refuses the chat to
    /// any other client, whatever the daemon is bound to (`R-J103`).
    ///
    /// `id` is the client's, and [`Model::stop`] with it ends the answer early.
    pub async fn chat_streaming<F>(
        &self,
        id: &str,
        messages: &[ChatTurn],
        from_this_machine: bool,
        mut on_delta: F,
    ) -> Result<Answer, String>
    where
        F: FnMut(String) + Send,
    {
        let bell = Arc::new(Notify::new());
        self.stops.lock().expect("stops lock").insert(id.to_string(), bell.clone());
        let answer = self.ask(messages, from_this_machine, &mut on_delta, Some(&bell)).await;
        self.stops.lock().expect("stops lock").remove(id);
        answer
    }

    /// Stop the chat answer `id`, keeping what has arrived. `R-J118`.
    ///
    /// `false` when no such answer is in flight — it finished, or never began.
    /// A stop that lands between two reads is not lost: `notify_one` leaves a
    /// permit for the next wait.
    pub fn stop(&self, id: &str) -> bool {
        match self.stops.lock().expect("stops lock").get(id) {
            Some(bell) => {
                bell.notify_one();
                true
            }
            None => false,
        }
    }

    /// Ask, and answer. `Err` is a sentence for a human — every failure here is
    /// something the panel shows rather than something it hides.
    ///
    /// Judged by the bind alone: for the daemon's own asks and the binaries.
    /// A client's request goes through [`Model::chat_as`].
    pub async fn chat(&self, messages: &[ChatTurn]) -> Result<Answer, String> {
        self.ask(messages, false, &mut |_| {}, None).await
    }

    /// Ask on behalf of a client, which may be on this machine. `R-J103`.
    pub async fn chat_as(&self, messages: &[ChatTurn], from_this_machine: bool) -> Result<Answer, String> {
        self.ask(messages, from_this_machine, &mut |_| {}, None).await
    }

    async fn ask(
        &self,
        messages: &[ChatTurn],
        from_this_machine: bool,
        on_delta: &mut (dyn FnMut(String) + Send),
        stop: Option<&Notify>,
    ) -> Result<Answer, String> {
        // Clause 4 keeps a daemon reachable from the LAN from becoming a
        // general-purpose LLM proxy *for the LAN*. The window on this machine
        // is not the LAN, whatever the bind — it could have asked the model
        // directly — so it passes; any other client is judged by the bind.
        let settings = self.admitted(messages, from_this_machine)?;
        let base = policy::normalise_base(settings.url.as_deref().unwrap_or_default());
        let url = policy::chat_url(&base);
        let body = chat_body(settings.model.as_deref(), messages);
        let started = std::time::Instant::now();
        let outcome = post_streaming(&url, &body, on_delta, stop).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let mut g = self.inner.lock().expect("model lock");
        match outcome {
            Ok(Streamed { text, model, stopped }) => {
                g.last_error = None;
                g.last_ok_ms = Some(elapsed_ms);
                Ok(Answer { text, model, elapsed_ms, stopped })
            }
            Err(e) => {
                g.last_error = Some(e.clone());
                Err(e)
            }
        }
    }
}

impl Model {
    /// The settings an ask may use, or the sentence saying why not. One gate
    /// for the chat and the agent alike, so they cannot come to disagree.
    fn admitted(&self, messages: &[ChatTurn], from_this_machine: bool) -> Result<ModelSettings, String> {
        // Clause 4 keeps a daemon reachable from the LAN from becoming a
        // general-purpose LLM proxy *for the LAN*. The window on this machine
        // is not the LAN, whatever the bind — it could have asked the model
        // directly — so it passes; any other client is judged by the bind.
        let (settings, chat_allowed) = {
            let g = self.inner.lock().expect("model lock");
            (g.settings.clone(), g.chat_allowed || from_this_machine)
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
        Ok(settings)
    }

    /// Ask the sandboxed agent, forwarding text and steps as they land.
    /// `R-O14`, ADR-0046.
    ///
    /// The same gate as the chat, the same endpoint (the agent's opencode
    /// config is built from these settings, llmproxy included), the same stop
    /// bell under the client's id, and the same residue in the Health row.
    pub async fn agent_streaming<T, S>(
        &self,
        id: &str,
        messages: &[ChatTurn],
        from_this_machine: bool,
        ready: &crate::askagent::Ready,
        mut on_text: T,
        mut on_step: S,
    ) -> Result<AgentAnswer, String>
    where
        T: FnMut(String) + Send,
        S: FnMut(mogeung_core::askagent::AgentStep) + Send,
    {
        let settings = self.admitted(messages, from_this_machine)?;
        let base = policy::normalise_base(settings.url.as_deref().unwrap_or_default());
        let bell = Arc::new(Notify::new());
        self.stops.lock().expect("stops lock").insert(id.to_string(), bell.clone());
        let started = std::time::Instant::now();
        let outcome = crate::askagent::run(
            ready,
            &base,
            settings.model.as_deref(),
            messages,
            &mut on_text,
            &mut on_step,
            Some(&bell),
        )
        .await;
        self.stops.lock().expect("stops lock").remove(id);
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let mut g = self.inner.lock().expect("model lock");
        match outcome {
            Ok(o) => {
                g.last_error = None;
                g.last_ok_ms = Some(elapsed_ms);
                Ok(AgentAnswer {
                    text: o.text,
                    steps: o.steps,
                    stopped: o.stopped,
                    model: settings.model.unwrap_or_else(|| "default".into()),
                    elapsed_ms,
                    agent: ready.provenance(),
                })
            }
            Err(e) => {
                g.last_error = Some(e.clone());
                Err(e)
            }
        }
    }
}

/// One answer from the sandboxed agent. `R-O14`.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentAnswer {
    pub text: String,
    pub steps: Vec<mogeung_core::askagent::AgentStep>,
    pub stopped: bool,
    /// The model the agent was configured with. opencode does not say which
    /// one answered, so this is what was asked for, not a report.
    pub model: String,
    pub elapsed_ms: u64,
    /// *opencode 1.19.2 · sandboxed*.
    pub agent: String,
}

/// One answer, and what actually produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub text: String,
    /// What answered, which may not be what was asked for — an endpoint is
    /// free to route `default` wherever it likes, and the panel says so.
    pub model: String,
    pub elapsed_ms: u64,
    /// Ended by [`Model::stop`] rather than by the model: `text` is what had
    /// arrived, and may be empty. `R-J118`.
    pub stopped: bool,
}

/// What a streamed POST came to.
#[derive(Debug, PartialEq)]
struct Streamed {
    text: String,
    model: String,
    stopped: bool,
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
    stop: Option<&Notify>,
) -> Result<Streamed, String> {
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

    let mut stopped = false;
    loop {
        let next = match stop {
            // Raced against the stop, so one that rings while the model is
            // thinking — nothing arriving for half a minute — still lands now.
            Some(bell) => tokio::select! {
                line = lines.next_line() => line,
                _ = bell.notified() => {
                    stopped = true;
                    break;
                }
            },
            None => lines.next_line().await,
        };
        let Ok(Some(line)) = next else { break };
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

    // Killing curl closes the connection, which is what tells the server to
    // stop generating: an answer abandoned here and left running would go on
    // spending the GPU on words nobody will read.
    if stopped {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Ok(Streamed {
            text: state.text.trim().to_string(),
            model: if state.model.is_empty() { "unknown model".into() } else { state.model },
            stopped: true,
        });
    }

    let status = child.wait().await;

    // **Any frame at all** means this was a stream, so `finish` decides what it
    // adds up to — including "nothing", which it reports as a model that
    // answered with nothing rather than as unparseable JSON. Judging by
    // accumulated text instead sent a truncated stream to the whole-body
    // parser below, which quoted raw SSE back at the user.
    //
    // A partial answer is kept rather than discarded: a timeout half way
    // through a long reply leaves something worth reading, and throwing it
    // away to report a failure the user can already see is the wrong trade.
    if state.frames > 0 {
        return state.finish().map(|(text, model)| Streamed { text, model, stopped: false });
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
        Ok(_) => parse_reply(&whole).map(|(text, model)| Streamed { text, model, stopped: false }),
    }
}

/// POST and return the body, or a sentence saying why not.
pub(crate) async fn post_json(url: &str, body: &str) -> Result<String, String> {
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
    /// How many `data:` frames arrived, of any kind.
    ///
    /// This and **not** the accumulated text is what says whether the response
    /// was a stream. A reply cut off after its opening `{"role":"assistant"}`
    /// frame — a timeout on a long prompt, which is exactly what a big diff
    /// produces — has no text and is still unmistakably a stream. Judging by
    /// the text sent that case to the whole-body parser, which reported *the
    /// endpoint did not answer with JSON* and quoted the SSE at the user.
    /// Found by `--bin judge` on a 60-file diff.
    pub frames: usize,
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
        self.frames += 1;
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
            embed_model: None,
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
            embed_model: None,
        });
        m.set_chat_allowed(false);
        let err = m.chat(&turns()).await.unwrap_err();
        assert!(err.contains("loopback"), "{err}");
        assert!(!err.contains("--allow"), "must not imply a flag exists: {err}");
        // Another machine's client is refused the same way.
        let err = m.chat_as(&turns(), false).await.unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    /// `R-J103`: a daemon bound to the LAN still answers the window on its
    /// own machine. Admitted past clause 4 means the call goes to the
    /// endpoint — which, on a port nobody holds, fails as a connection
    /// rather than as the refusal.
    #[tokio::test]
    async fn a_client_on_this_machine_is_admitted_on_a_lan_bind() {
        let m = Model::new();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        m.configure(ModelSettings {
            url: Some(format!("http://127.0.0.1:{port}/v1")),
            model: Some("m".into()),
            consent: mogeung_core::model::RemoteConsent::None,
            embed_model: None,
        });
        m.set_chat_allowed(false);
        let err = m.chat_as(&turns(), true).await.unwrap_err();
        assert!(!err.contains("beyond loopback"), "refused by clause 4 when it should have been admitted: {err}");
        // And the health row a client on this machine is shown says so.
        let h = m.health_for(true).unwrap();
        assert!(h.chat_allowed);
        assert!(!m.health().unwrap().chat_allowed, "everyone else still reads the refusal");
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
            embed_model: None,
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

    /// A stream cut off after its opening frame is still a **stream**, and must
    /// not be handed to the whole-body parser. `R-O11`, found by `--bin judge`
    /// on a 60-file diff that timed out: the old check asked whether any text
    /// had accumulated, so this case reported *the endpoint did not answer with
    /// JSON* and quoted raw SSE at the user.
    #[test]
    fn a_truncated_stream_is_still_a_stream() {
        let mut st = StreamState::default();
        st.push_line(r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#);
        assert_eq!(st.frames, 1, "a frame arrived, even though no text did");
        assert!(st.text.is_empty() && st.reasoning.is_empty() && st.error.is_none());
        // And it reports the honest thing rather than a parser complaint.
        assert_eq!(st.finish().unwrap_err(), "the model answered with nothing");

        // Nothing at all is the only case that may fall through to the
        // whole-body parser.
        assert_eq!(StreamState::default().frames, 0);
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


    /// `R-J118`. A model that is still writing is stopped where it is: the
    /// words so far come back marked `stopped`, at once rather than when the
    /// model would have finished, and the connection to the model is closed —
    /// which is what makes the server stop generating, and the half of this a
    /// panel could not show.
    #[tokio::test]
    async fn a_stop_keeps_what_arrived_and_hangs_up_on_the_model() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (hung_up_tx, hung_up) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = sock.read(&mut buf).await;
            let frame = |t: &str| {
                format!("data: {{\"model\":\"m\",\"choices\":[{{\"delta\":{{\"content\":\"{t}\"}}}}]}}\n\n")
            };
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
            sock.write_all(format!("{head}{}", frame("Hello")).as_bytes()).await.unwrap();
            // Then a word every 50 ms for a minute — a model that is nowhere
            // near done — until the write fails because nobody is listening.
            for _ in 0..1200 {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if sock.write_all(frame(" more").as_bytes()).await.is_err() {
                    let _ = hung_up_tx.send(());
                    return;
                }
            }
        });

        let m = std::sync::Arc::new(Model::new());
        m.configure(ModelSettings {
            url: Some(format!("http://127.0.0.1:{port}/v1")),
            model: Some("m".into()),
            consent: mogeung_core::model::RemoteConsent::None,
            embed_model: None,
        });
        let (first_tx, first) = tokio::sync::oneshot::channel::<()>();
        let mut first_tx = Some(first_tx);
        let asking = {
            let m = m.clone();
            tokio::spawn(async move {
                m.chat_streaming("ask-1", &turns(), true, move |_| {
                    if let Some(tx) = first_tx.take() {
                        let _ = tx.send(());
                    }
                })
                .await
            })
        };
        first.await.expect("the model's first words");
        assert!(m.stop("ask-1"), "the answer is in flight, so there is something to stop");

        let started = std::time::Instant::now();
        let a = tokio::time::timeout(std::time::Duration::from_secs(5), asking)
            .await
            .expect("a stopped answer returns now, not when the model is done")
            .unwrap()
            .expect("a stop is an answer, not a failure");
        assert!(a.stopped);
        assert!(a.text.starts_with("Hello"), "{:?}", a.text);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        tokio::time::timeout(std::time::Duration::from_secs(5), hung_up)
            .await
            .expect("the model was left talking to nobody")
            .unwrap();

        // And it is gone: a second stop finds nothing in flight.
        assert!(!m.stop("ask-1"));
        assert!(m.health().unwrap().last_error.is_none(), "a stop is not the endpoint failing");
    }
}
