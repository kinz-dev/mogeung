---
title: Ask answers with opencode, in a sandbox it cannot leave
status: active
updated: 2026-10-22
roadmap: [R-O14]
depends_on: [A37, A55, A56]
---

# 0053 — Ask answers with opencode, in a sandbox it cannot leave

## Spec

*Owned by the human. What and why, never how.*

### Problem

Reported 2026-10-22. Asked to read a blog post from its link and summarise
it, Ask Mogeung answered:

> No, I cannot read or access the content of that URL. … The URL contains a
> future date … I cannot browse the live internet.

The owner's own `opencode`, connected to the same model, read the page and
summarised it. *"I want the same feature to be built in the Ask Mogeung."*

The model is the same; what it is handed is not. opencode gives the model
tools (`webfetch`, bash, files) and runs the loop. Ask sends the conversation
straight to `/chat/completions` with no tools and no system message, so the
model cannot read a page and does not know today's date. That second gap is
where *"a future date"* comes from.

Three shapes were put to the owner: the daemon fetching the links you paste,
a checkbox choosing opencode, or opencode always. The owner leaned to
opencode for *"all the opencode agent capability"* and asked whether the
risk, that it *"can run program and do other stuff"*, could be mitigated.
After seeing the sandbox measured, the owner chose:

- **Full but confined.** Every opencode tool, inside a sandbox that cannot
  touch your files.
- **Agent by default.** No checkbox. Plain chat is the fallback when there is
  no sandbox.
- **Linux first.**
- **No web search.** It sends queries to a third party.

### Assumptions

- **A55** `SUPPORTED`: the sandbox keeps a run from writing outside its
  tmpfs, reading HOME, or reaching the session bus, ssh-agent or docker, and
  still lets it reach the network. Probed 2026-10-22 on this machine, and
  pinned by a test.
- **A56** `SUPPORTED`: opencode can be run from flags and environment alone,
  loading nothing of the user's.
- **A37** `UNTESTED`: the Ask panel is worth having beside an agent already
  open. This feature makes the panel more capable. It does not test the
  habit, and the fortnight under `R-O8` still owns that.

Neither A55 nor A56 says anything about macOS, which is why it is out of
scope.

### Acceptance

- [ ] Pasting a link into Ask with a question about it produces an answer
      drawn from the page, and a step under the answer shows the fetch.
- [ ] The answer says it came from opencode in a sandbox, beside the model
      and the time.
- [ ] Asked *"what is today's date"*, Ask answers with the right one.
- [ ] Asked to write a file in my home directory, it cannot. The step shows
      the refusal, and the file does not exist.
- [ ] With `bwrap` or `opencode` missing, Ask still answers as plain chat and
      says, once per answer, why it did not use the agent.
- [ ] `ask_agent = false` in `~/.mogeung/config.toml` returns Ask to plain
      chat with no note.
- [ ] Stop ends an agent answer, and keeps whatever text had arrived.
- [ ] An ask leaves nothing on disk: no opencode session appears on the board
      and nothing is written under `~/.local/share/opencode` or
      `~/.config/opencode`.
- [ ] The draft, ask-the-diff, the command line and the handover answer
      exactly as before.

### Explicitly out of scope

- **macOS and Windows.** They get plain chat, saying so.
- **Web search.** It would need consent naming the search provider's host.
- **Letting the agent see a repository or any of the user's files.**
- **Network isolation:** the LAN, and abstract unix sockets. ADR-0046 names
  them as what stays exposed.
- **A long-lived `opencode serve`, and real multi-turn opencode sessions.**
- **Keeping tool steps in the chat history.**

## Plan

*Drafted by an agent, approved by the human 2026-10-22 (the four choices
above), before implementation.*

### Approach

**Policy in `mogeung_core::askagent`, effects in `mogeungd::askagent`**, the
seam `model.rs` and `run.rs` already use.

- **Core (pure, tested with nothing running):**
  - the opencode config for one ask, built from `ModelSettings`;
  - the `bwrap` argv for a given binary and a probed `--size` support;
  - the conversation rendered into one message;
  - a parser for `opencode run --format json` lines, which degrades;
  - the sentence for each reason the agent is unavailable.
- **Daemon:**
  - **Availability:** Linux, `ask_agent` not off, `bwrap` found and
    starting, and `opencode` found.
  - **The run:** the binary path is canonicalised, because the npm install
    is a symlink into `node_modules` and `/home` is hidden. The message goes
    in on stdin. Stdout is read a line at a time: text parts are forwarded as
    `model_chunk`, tool steps as a new `model_step`. The stop bell kills
    `bwrap`, which takes the namespace with it.
  - **Limits:** a wall-clock limit of 10 minutes and a step cap of 25.
- **Wire:**
  - `model_chat` gains `agent: bool`. Only the Ask panel sets it.
  - `model_reply` gains `agent` (what answered, e.g. *opencode 1.19.2 ·
    sandboxed*), `agent_note` (why it did not, when it was asked for) and
    `steps`. The reply alone stays enough to render the answer: a dropped
    step costs jitter, never correctness.
  - `ModelHealth` gains the same two strings, so the empty panel can say what
    will answer before you ask.
- **Window:**
  - steps under the answer while it works and after;
  - the provenance line and the note;
  - the empty-state hint.

### Files touched

- `crates/mogeung-core/src/askagent.rs` (new), `model.rs`, `wire.rs`,
  `config.rs`, `lib.rs`
- `crates/mogeungd/src/askagent.rs` (new), `model.rs`, `api.rs`, `server.rs`,
  `main.rs`, `lib.rs`
- `desktop/src-tauri/src/daemon.rs`, which takes the new `Options` field
- `desktop/src/wire/types.ts`, `store/index.ts`, `ui/tools/ChatTool.tsx`,
  and their tests
- `docs/design/model-and-proxy.md`, `docs/design/wire-protocol.md`

### Risks and unknowns

- **opencode's JSON stream is undocumented (`A4`).** The parser ignores
  shapes it does not know. A run that produced no text and no error says
  so, rather than showing an empty answer.
- **`bwrap` older than 0.10 has no `--size`.** The tmpfs is then uncapped,
  which means half of RAM.
- **opencode installed in a way whose binary needs its directory.** The
  binary's directory is bound read-only, not the file alone.
- **A first ask after an opencode upgrade may be slower.**

### Test strategy

- **Core:**
  - the config denies `websearch`, `question` and `doom_loop`, names the
    daemon's endpoint and model, and turns sharing off;
  - the argv hides every path ADR-0046 lists, binds nothing writable but the
    sized tmpfs, and clears the environment;
  - rendering a conversation, single and multi-turn;
  - parsing every event shape seen, plus garbage, an unknown type and an
    error.
- **Daemon, the test that would fail today:**
  - run the daemon's own `bwrap` argv around a shell script, on any Linux
    with `bwrap`;
  - assert that a write outside fails, HOME's real files are invisible,
    `$XDG_RUNTIME_DIR/bus` is gone, and the tmpfs is writable;
  - skip, saying so, where there is no `bwrap`.
- **Daemon, the runner:** a fake `opencode` script inside the real sandbox,
  emitting fixture lines, for chunks, steps, an error event and stop.
- **Window:** `askModel` sends `agent: true` and the draft does not;
  `model_step` lands on the right turn; the turn shows steps, provenance and
  the note.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*

**The spike, 2026-10-22, before a line of product code.** Every number here
is from `opencode 1.19.2` on kinz-xps14 against the Spark model.

- **opencode's default agent is `* → allow`.** `opencode debug agent build`
  says so. Out of the box, `opencode run` can run anything.
- **Moving XDG is not enough.** With only XDG moved, the resolved agent still
  allowed the owner's `~/.claude/skills/*`, because opencode reads Claude
  Code's skills from HOME. HOME has to move too, and then they are gone.
- **A custom agent defined inline works.** `"*": "deny"` plus `webfetch`
  resolved to `webfetch` alone. Asked to run bash, the agent said it had no
  shell, then fetched `example.com` and summarised it. That took 16 s cold.
- **The sandbox holds when opencode's permissions do not.** With bash allowed
  and the sandbox read-only, `touch` outside gave *Read-only file system*.
  With `/home` on a tmpfs, `~/.zshrc` was *No such file or directory*.
- **The first canary proved nothing.** It sat under `/tmp`, which the sandbox
  replaces, so it failed with *No such file or directory* for the wrong
  reason. It was redone under `/var/tmp`.
- **`/run` had to go.** It holds the owner's session bus, ssh-agent, keyring,
  gnupg and `docker.sock`, and the owner is in the `docker` and `lxd` groups.
  A read-only mount does not stop `connect()`. The fix is to hide all of
  `/run` and bind back only `/run/systemd/resolve` for DNS. Hiding `/var`
  takes CUPS's socket, the only other one found.
- **Xwayland's abstract socket is reachable but refuses without the cookie,**
  and the cookie is in `/run/user`.
- **Timing and leftovers.** With the message on stdin, `--clearenv`, a fresh
  data dir and `OPENCODE_DISABLE_MODELS_FETCH`, a trivial ask took 6 s, and
  one with three bash calls took 11 s. Nothing was left behind: no model
  catalog and no session outside the run's own directories.
- **The error shape.** An unreachable endpoint gives one `{"type":"error",
  "error":{"name":"APIError","data":{"message":…}}}` line and exit 1, after
  about 65 s of opencode's own retries.
- **`bwrap --size` needs a tmpfs mount point under an existing tmpfs.**
  `/ask` failed on the read-only root, so the sized tmpfs is `/tmp` itself.

**Building it, 2026-10-22.**

- **A hidden path is still writable.** The empty tmpfs over `/home` is
  writable, unsized and in RAM. The first end-to-end run asked the agent to
  `touch /home/kinz/ASK_AGENT_WAS_HERE`, and it succeeded inside the sandbox
  (nothing reached the disk). Every hidden path is now `--remount-ro`, after
  the mounts made inside it, so the sized `/tmp` is the only writable place.
  The argv test pins the order, and the containment test now writes into
  `/home`, `/run` and `/var` to check.
- **bwrap makes a skeleton for the binary.** Binding opencode's directory
  back makes `bwrap` create `~/.local/lib/node_modules/…` as empty mount
  points, so the first containment test, `ls -A $HOME`, saw `.local` and
  failed. It was right to fail and wrong about why. The test now checks every
  real entry of HOME except that skeleton, plus this repository's
  `Cargo.toml`.
- **The containment test is only evidence if it can fail.** Taking `/run` out
  of `HIDDEN` turned it red on the session bus, `docker.sock` and the system
  bus, and then it was put back.
- **Through the real wire**, with a throwaway daemon on its own config and
  port:
  - an agent ask fetched `example.com` and the IANA page it links to, got the
    date right, and arrived as two `model_step` events and one paragraph;
  - a plain ask on the same socket answered in 1.1 s with no `agent` field;
  - a write to `/home/kinz/WAS_HERE` was reported by the agent and was not on
    disk afterwards.

  The first 100 s of that run were the throwaway database's first scan, not
  this feature: warm, the same plain ask took 1.1 s.
- **A bash step's ✓ means the tool ran, not that the command succeeded.**
  opencode reports `completed` for a bash call whose command failed, so the
  agent's own text is what says *Read-only file system*. That is acceptable
  for a record of actions, and it is why the panel never presents steps as
  evidence.
- **Not done:** the panel was not opened in the installed window, and that is
  the ⏳.
