---
title: The model and the proxy
status: active
updated: 2026-10-08
covers:
  - crates/mogeung-core/src/model.rs
  - crates/mogeung-core/src/llmproxy.rs
  - crates/mogeungd/src/model.rs
  - crates/mogeungd/src/llmproxy.rs
  - crates/mogeungd/src/guide.rs
  - crates/mogeungd/src/why.rs
  - crates/mogeungd/src/embed.rs
  - crates/mogeungd/src/bin/judge.rs
  - crates/mogeungd/src/bin/why.rs
---

# The model and the proxy

Pillar O: a local model may read mogeung's own evidence and answer you, in a
column of its own. The rules are
[ADR-0031](../decisions/0031-consent-to-a-named-host.md) (carried forward from
ADR-0030), [ADR-0032](../decisions/0032-the-chat-panel-remembers.md),
[ADR-0033](../decisions/0033-a-proxy-of-our-own.md) and
[ADR-0034](../decisions/0034-the-draft-is-a-chat-ask.md); the messages are in
[wire-protocol.md](wire-protocol.md#the-model). This document is how the
pieces fit.

## The seam

`mogeung_core::model` holds the policy — what is configured, whether it is
allowed, and the sentence saying why not — and `mogeungd::model` holds the
part that leaves the machine. The split is the one `run.rs` uses: pure
decisions where they can be tested with nothing running, effects where the
processes are.

**The daemon owns the endpoint, not the client** (clause 2). The corpus is on
the daemon's machine, so a window watching a Mac has to read *that* machine's
transcripts with whatever endpoint *that* daemon was given; a client-side call
would be the first piece of local authority in a client. The endpoint is a
URL in config (`model_url`, `model_name`, `embed_model`), not an in-process
model, because the box with the GPU and the box with the sessions may differ.
`curl` makes the call with the body on stdin, so neither argv limits nor
quoting can ever be part of a chat message, and shelling out cannot poison
the runtime.

**Consent names a host** (clause 3). A non-loopback endpoint is refused
unless `allow_remote_model` names it (`"<host>"`, recommended), or is `true`
or `--allow-remote-model` (blanket). No suffix matching, no DNS. The key is
read from `config.toml` rather than only from a flag because the window's
hosted daemon has no argv — flag-only meant *unreachable* on the shape
mogeung is normally run in.

**The chat is loopback's** (clause 4). `model_chat` is the one command that
carries free-form text, and it is refused to any client not on this machine,
with no flag that opens it, because a daemon anyone can reach must not become
a general-purpose LLM proxy. Decided per connection since 2026-09-23
(`R-J103`): a loopback peer is admitted whatever the bind.

**Output is a column, never evidence** (clause 5). Nothing a model says sets
an attention tier, joins a signal, or sits beside a claim as proof. Every
answer carries its provenance and the daemon decides it.

**Model work never runs on its own** (clause 6). Nothing here is on the scan
tick or on selection; every call is a button.

## The four features through one door

**Ask Mogeung** (`R-O5`) is the chat panel. The whole conversation travels on
every ask and the daemon keeps nothing of an ask in flight; since `R-O9` an
answered exchange may be kept in the `chats` table — capped at 200, oldest
touched first out, never mirrored, `chat_history = false` to stop keeping —
which is a convenience for finding a question again rather than a document
([ADR-0032](../decisions/0032-the-chat-panel-remembers.md)). Answers stream
(`R-O11`): chunks coalesced at 60 ms or 400 characters, always followed by one
reply carrying the whole text, so a dropped chunk costs jitter and not
correctness. `model_stop` kills the `curl` carrying the request.

**The draft** (`R-O7`) and **ask the diff** (`R-O4`) ride the same message
rather than families of their own — a purpose on the one free-form message,
not a second one ([ADR-0034](../decisions/0034-the-draft-is-a-chat-ask.md)).
The draft turns flagged hunks and notes into a follow-up instruction, with the
raw concatenation one click away; drafting is asked for, never automatic, and
drafting never sends — copy and the fenced one-shot send are two further
buttons. Ask-the-diff sends ids and a question, never evidence: the daemon
holds the transcripts, so it does the retrieving — the turns leading up to the
last edit of that file, `--bin why`'s own shape and count of six. The reply
carries `cites`, `basis` (`turns`, `code` or `unanswered`) and `narration`;
*unanswered* is the **majority** answer over this corpus (a reason in 5 of 14
edit moments) and is not a failure, and *narration* — every citation is the
assistant's own — is computed in the daemon so no client can render *the
agent said it did this* as *this is why it was done*. An older daemon ignores
`about` and answers as chat; the window detects the missing `basis` and
withholds the text rather than drawing a general answer where provenance was
promised.

**The reading guide** (`R-O3`) asks the model which changed file to read
first. The ordering lives in the store keyed by session, beside the keyword
order, and the daemon decides what the guide contains: it appends every file
the model did not name, marked `ranked: false`, because `--bin judge` found
`claude-opus-5` naming about sixteen files of sixty where a local model named
59. A shortlist drawn as the whole diff hides two thirds of a change, and
that rule belongs in the daemon rather than in each client. The keyword order
is untouched and is what shows without a model; pillar K refuses a blend.

**The similar list** (`R-O6`) is a second list beside grep's and never
instead of it. Two commands because building an index and asking it are
different acts with different costs: a build embeds thousands of lines over
minutes, a query embeds one; neither is on the scan tick. The index is a
cache in SQLite — little-endian `f32`s in a BLOB, with the model and the build
time beside it — dropped and rewritten wholesale on a rebuild, because a
half-rebuilt index mixes two models in a space the cosine assumes is one. The
reply says what built it, when, and whether the corpus has moved since
(`stale`); `refusal` says why there is nothing, because an empty list with no
reason reads as *there is nothing similar*, which is a claim. The same
embeddings group recurring failures by meaning (`R-F4`, `cluster_failures`),
a second grouping the Failures panel switches to, at a threshold of 0.85 that
the harness found rather than a setting anyone chose.

## The two harnesses, and why they are binaries

`--bin judge` and `--bin why` are the pillar's measurements, and they are
programs rather than tests because their corpus is **this machine's**, which
no fixture can stand in for and no CI can carry. Each reads the same `Config`
the daemon reads, resolves the same endpoint including mogeung's own llmproxy,
and exits non-zero when nothing answered — a broken setup that reads as *no
findings* is the failure that costs a year. Neither owns a prompt of its own:
`judge` calls `mogeungd::guide` and `why` calls `mogeungd::why`, which is what
the shipping panels call, because a harness grading a different prompt than
the one that ships measures a feature that does not exist. `why` also holds
`R-O4`'s two candidate retrievals, nearest-in-time and leading-up, because
which one the panel uses was a finding and the corpus answered it before the
panel existed. `judge --recall` and `--clusters` are the similar list's
measurements; `judge --complete` is the one that refuted a feature before it
was drawn (A40).

## The proxy the daemon owns (`R-O10`)

`mogeung_core::llmproxy` holds the policy — which port, what the starter
config says, which hosts it forwards to — and `mogeungd::llmproxy` owns the
child. **The first long-lived child the daemon has ever owned**, and
[ADR-0033](../decisions/0033-a-proxy-of-our-own.md) argues the two things it
crosses. Off unless `llmproxy = true` in the config file, and file-only: a
flag would start a proxy for one invocation and leave it behind.
`llmproxy_bin`, `llmproxy_config` and `llmproxy_port` are the other keys; the
starter config is written once to `~/.mogeung/llmproxy.toml`.

The port is **derived** from the daemon's own, so start-up recomputes where it
left the last one instead of reading a file that may describe a process that
died last week. Start-up **adopts** a proxy already answering there, and
shutdown stops it **by address** — `--listen <addr> --shutdown`, llmproxy's
own mechanism — rather than by signal, because llmproxy re-execs itself as
`--foreground` and detaches, so a recorded pid names something already exited
and a process-group kill would reach nothing. The window stops the proxy it
started, because the hosted daemon's shutdown future never resolves and
`server::run`'s cleanup is unreachable on that path; the shell's exit calls
the same `llmproxy::stop(bin, port)`, against a target recorded at host time
and only when the window actually hosted.

**The admin interface is reachable because the daemon reads its port.**
llmproxy binds admin on a random loopback port and writes the URL into its
own runtime metadata, which the daemon reads and puts a button on; the shell
opens it through `open_local_url`, which accepts `http://` on a parsed
loopback host and nothing else.

**Where it forwards is reported, never gated.** A proxy on `127.0.0.1` passes
clause 3 without asking; routing is per request, so a gate could only be
sometimes-right. The Health window's proxy row names the upstream hosts, read
from the config file rather than from the running process so the answer
survives the proxy being down. `yolomop` is the launcher that starts `claude`
through it.

## What this pillar refuses

Any write to a worktree file, any path into a session's input, a model that
sets an attention tier, model output presented as evidence beside a claim, a
cloud endpoint by default, and an agentic loop. Every feature above was
measured before it was drawn; the measurements are in the roadmap's pillar O
rows and the assumptions A35 to A41.
