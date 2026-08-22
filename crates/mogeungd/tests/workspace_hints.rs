//! Folders a session was noticed working in, end to end. `R-J39`.
//!
//! `R-J40` gave a workspace the folders you add by hand. This is the half that
//! shortens the click: mogeung already knows where a session has been writing,
//! and the two channels that say so — the CLI's own `/add-dir` confirmation,
//! and files edited outside the session's root — are both already folded into
//! the session by the time anyone asks.
//!
//! **Offered, never added.** Nothing here widens what the daemon will read; the
//! suggestions are a list, and the `+` is the user's.
//!
//! The measurement this is built on, over the 160 transcripts on the author's
//! machine (2026-08-22): 40 sessions wrote outside their own root, and ranking
//! the folders they wrote to sorted itself — every one that was a **git
//! repository** was a real sibling project, and every one that was not was the
//! harness talking to itself (the agent's memory directory, a per-session
//! scratchpad, one dotfile that rolled up to `$HOME`). That is where the
//! inference rule comes from, and this test is the corpus's shape in miniature.

use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// A repository on disk, so the roll-up has a `.git` to find.
fn repo(dir: &Path) {
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
}

struct Fixture {
    state: std::sync::Arc<AppState>,
    dir: PathBuf,
}

/// A session rooted in `own`, which wrote two files in the sibling repository,
/// one in the agent's own memory folder, one in a scratchpad, and ran
/// `/add-dir` for a third folder.
async fn boot(tag: &str) -> Fixture {
    // The temp directory, deliberately: `target/` is inside this repository, so
    // a fixture there would take *mogeung's own* checkout as the session root
    // and every folder in the test would already be inside it.
    let dir = std::env::temp_dir().join(format!("mogeung-hints-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let home = dir.join("claude");
    let own = dir.join("own");
    let sibling = dir.join("sibling");
    let announced = dir.join("announced");
    repo(&own);
    repo(&sibling);
    std::fs::create_dir_all(&announced).unwrap();
    // The two shapes that dominated the measurement and must never be offered:
    // the agent's own memory directory, and a per-session scratchpad. Neither
    // is a repository, and the memory one is under the claude home as well.
    std::fs::create_dir_all(home.join("projects/-tmp-own/memory")).unwrap();
    let pad = std::env::temp_dir().join(format!("mogeung-hints-pad-{}", std::process::id()));
    std::fs::create_dir_all(&pad).unwrap();

    let edit = |id: &str, path: PathBuf| {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-08-22T10:00:00Z","cwd":"{own}","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"Edit","input":{{"file_path":"{path}"}}}}]}}}}"#,
            own = own.display(),
            path = path.display(),
        )
    };
    // The CLI agreeing, verbatim down to the bold escapes it writes.
    let confirm = format!(
        r#"{{"type":"user","timestamp":"2026-08-22T10:00:01Z","cwd":"{own}","message":{{"role":"user","content":"<local-command-stdout>Added \u001b[1m{announced}\u001b[22m as a working directory for this session \u001b[2m· /permissions to manage \u001b[22m</local-command-stdout>"}}}}"#,
        own = own.display(),
        announced = announced.display(),
    );
    let lines = [
        edit("t1", own.join("src/lib.rs")),
        edit("t2", sibling.join("src/a.rs")),
        edit("t3", sibling.join("src/b.rs")),
        edit("t4", home.join("projects/-tmp-own/memory/note.md")),
        edit("t5", pad.join("probe.py")),
        confirm,
    ]
    .join("\n");
    write(&home.join("projects/-tmp-own/s1.jsonl"), &lines);

    let store = Store::open(&dir.join("test.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    state.scan().await;
    Fixture { state, dir }
}

async fn session_id(state: &AppState) -> String {
    let sessions = state.sessions.read().await;
    sessions.values().next().expect("a session was scanned").id.clone()
}

/// Both channels reach the client, ranked, with the harness's own folders and
/// the session's own root left out of it.
#[tokio::test]
async fn a_session_is_offered_the_folders_it_has_been_working_in() {
    let f = boot("offer").await;
    let id = session_id(&f.state).await;
    let view = f.state.workspace(&id).await.unwrap();

    assert!(view.dirs.is_empty(), "nothing has been added: {:?}", view.dirs);
    let paths: Vec<&str> = view.hints.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            f.dir.join("announced").to_string_lossy().to_string(),
            f.dir.join("sibling").to_string_lossy().to_string(),
        ],
        "the CLI's own confirmation leads; the harness's folders are absent"
    );
    // Two files rolled up to one repository, and the `/add-dir` folder offered
    // without a single file having been written in it.
    assert_eq!(view.hints[0].source, "add-dir");
    assert_eq!(view.hints[0].files, 0);
    assert_eq!(view.hints[1].source, "edits");
    assert_eq!(view.hints[1].files, 2);
}

/// The line that carries a suggestion is CLI output, and it used to be counted
/// as the human's turn — a session that had just compacted reported the
/// compaction banner as the last thing you asked for.
#[tokio::test]
async fn the_clis_own_output_is_not_the_last_thing_you_said() {
    let f = boot("prompt").await;
    let id = session_id(&f.state).await;
    let sessions = f.state.sessions.read().await;
    let s = sessions.get(&id).unwrap();

    assert_eq!(s.last_prompt, None);
    assert_eq!(s.turns, 0);
}
