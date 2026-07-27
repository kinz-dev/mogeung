//! Finding one session among many.
//!
//! Free text alone stops working at about a dozen sessions: three of them are
//! in the same repo, two are on the same branch, and "the one that touched
//! `state.rs`" is not in any of their titles. So the filter understands a few
//! fields, and everything else is still plain substring matching.
//!
//! `repo:mogeung branch:main retry` means all three must hold. Deliberately
//! not a query language — no OR, no negation, no quoting. Those need a parser,
//! a syntax to learn, and an error surface, in exchange for a case that has not
//! come up.

use mogeung_core::Session;

/// A parsed filter box.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub repo: Option<String>,
    pub branch: Option<String>,
    /// Matches against files the session has touched.
    pub file: Option<String>,
    /// Matches against the label the user gave the session (`R-B26`).
    pub label: Option<String>,
    /// Everything without a field prefix, joined.
    pub text: String,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.repo.is_none()
            && self.branch.is_none()
            && self.file.is_none()
            && self.label.is_none()
            && self.text.is_empty()
    }
}

/// Split a filter string into fields and free text.
///
/// Unknown prefixes (`foo:bar`) stay free text rather than becoming an error:
/// a colon in a prompt is far more likely than someone inventing a field, and
/// silently matching nothing would look broken.
pub fn parse(input: &str) -> Query {
    let mut q = Query::default();
    let mut text: Vec<String> = Vec::new();

    for token in input.split_whitespace() {
        let lower = token.to_lowercase();
        match lower.split_once(':') {
            Some((field, value)) if !value.is_empty() => match field {
                "repo" | "r" => q.repo = Some(value.to_string()),
                "branch" | "b" => q.branch = Some(value.to_string()),
                "file" | "path" | "f" => q.file = Some(value.to_string()),
                "label" | "l" => q.label = Some(value.to_string()),
                _ => text.push(lower),
            },
            _ => text.push(lower),
        }
    }
    q.text = text.join(" ");
    q
}

/// Does this session satisfy every part of the query?
///
/// `label` comes in from outside because a `Session` does not carry one — the
/// user's label is client state (`prefs.rs`), and this module stays pure.
pub fn matches(q: &Query, s: &Session, label: Option<&str>) -> bool {
    if let Some(repo) = &q.repo {
        if !s.repo_name().to_lowercase().contains(repo)
            && !s.cwd.to_lowercase().contains(repo)
        {
            return false;
        }
    }
    if let Some(branch) = &q.branch {
        match &s.git_branch {
            Some(b) if b.to_lowercase().contains(branch) => {}
            _ => return false,
        }
    }
    if let Some(file) = &q.file {
        if !s
            .touched_files
            .iter()
            .any(|f| f.to_lowercase().contains(file))
        {
            return false;
        }
    }
    if let Some(wanted) = &q.label {
        match label {
            Some(l) if l.to_lowercase().contains(wanted) => {}
            // A label query excludes the unlabelled: `label:` means "among
            // the ones I named", not "maybe this one too".
            _ => return false,
        }
    }
    if q.text.is_empty() {
        return true;
    }
    // Everything you might plausibly remember a session by — the name the
    // user gave it most of all.
    let hay = format!(
        "{} {} {} {} {} {}",
        s.label(),
        label.unwrap_or_default(),
        s.repo_name(),
        s.cwd,
        s.git_branch.clone().unwrap_or_default(),
        s.last_activity.clone().unwrap_or_default()
    )
    .to_lowercase();
    // Every word must appear, so adding a word always narrows.
    q.text.split_whitespace().all(|w| hay.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn session(label: &str, repo: &str, branch: &str, files: &[&str]) -> Session {
        let now = Utc::now();
        Session {
            id: format!("id-{label}"),
            title: Some(label.to_string()),
            name: None,
            last_prompt: None,
            cwd: format!("/work/{repo}"),
            repo_root: Some(format!("/work/{repo}")),
            git_branch: Some(branch.to_string()),
            pid: None,
            alive: false,
            live_status: None,
            version: None,
            started_at: now,
            last_event_at: now,
            status_since: None,
            turns: 0,
            tool_calls: 0,
            tokens_in: 0,
            tokens_out: 0,
            last_activity: None,
            touched_files: files.iter().map(|f| f.to_string()).collect(),
            base_sha: None,
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            error: None,
            transcript_path: String::new(),
            reviewed: false,
            open_tools: vec![],
            snoozed_until: None,
            collisions: vec![],
            loop_signal: None,
            recent_touches: vec![],
            recent_tools: vec![],
            tmux_target: None,
        }
    }

    #[test]
    fn plain_text_is_still_plain_text() {
        let q = parse("retry loop");
        assert_eq!(q.text, "retry loop");
        assert!(q.repo.is_none());
    }

    #[test]
    fn fields_are_extracted_and_lowercased() {
        let q = parse("repo:Mogeung branch:MAIN retry");
        assert_eq!(q.repo.as_deref(), Some("mogeung"));
        assert_eq!(q.branch.as_deref(), Some("main"));
        assert_eq!(q.text, "retry");
    }

    #[test]
    fn short_aliases_work() {
        assert_eq!(parse("r:foo").repo.as_deref(), Some("foo"));
        assert_eq!(parse("b:foo").branch.as_deref(), Some("foo"));
        assert_eq!(parse("f:foo").file.as_deref(), Some("foo"));
        assert_eq!(parse("path:foo").file.as_deref(), Some("foo"));
    }

    /// A colon in a prompt is much more likely than an invented field name, so
    /// an unknown prefix must not silently match nothing.
    #[test]
    fn an_unknown_prefix_stays_free_text() {
        let q = parse("todo: fix the thing");
        assert!(q.repo.is_none() && q.branch.is_none() && q.file.is_none());
        assert!(q.text.contains("todo:"));
    }

    #[test]
    fn a_bare_colon_is_not_a_field() {
        let q = parse("repo:");
        assert!(q.repo.is_none(), "an empty value should not filter anything out");
        assert_eq!(q.text, "repo:");
    }

    #[test]
    fn fields_combine_as_and() {
        let a = session("fix retry", "mogeung", "main", &[]);
        let b = session("fix retry", "other", "main", &[]);
        let q = parse("repo:mogeung retry");
        assert!(matches(&q, &a, None));
        assert!(!matches(&q, &b, None), "repo must actually narrow");
    }

    /// The case free text cannot answer: which session touched this file?
    #[test]
    fn file_matches_against_touched_files() {
        let a = session("something", "mogeung", "main", &["/work/mogeung/src/state.rs"]);
        let b = session("something", "mogeung", "main", &["/work/mogeung/src/git.rs"]);
        let q = parse("file:state.rs");
        assert!(matches(&q, &a, None));
        assert!(!matches(&q, &b, None));
    }

    #[test]
    fn every_word_must_match_so_typing_more_narrows() {
        let s = session("fix the retry loop", "mogeung", "main", &[]);
        assert!(matches(&parse("retry"), &s, None));
        assert!(matches(&parse("retry loop"), &s, None));
        assert!(!matches(&parse("retry banana"), &s, None));
    }

    #[test]
    fn an_empty_query_matches_everything() {
        let s = session("anything", "repo", "main", &[]);
        assert!(parse("").is_empty());
        assert!(matches(&parse(""), &s, None));
        assert!(matches(&parse("   "), &s, None));
    }

    #[test]
    fn branch_filter_skips_sessions_with_no_branch() {
        let mut s = session("x", "repo", "main", &[]);
        s.git_branch = None;
        assert!(!matches(&parse("branch:main"), &s, None));
    }

    /// The question labels exist to answer: "the risky one", by name. `R-B26`.
    #[test]
    fn label_filter_narrows_and_excludes_the_unlabelled() {
        let s = session("something", "mogeung", "main", &[]);
        assert!(matches(&parse("label:risky"), &s, Some("the risky one")));
        assert!(matches(&parse("l:risky"), &s, Some("the risky one")), "l: is the alias");
        assert!(matches(&parse("label:RISKY"), &s, Some("The Risky One")), "case-blind");
        assert!(!matches(&parse("label:safe"), &s, Some("the risky one")));
        assert!(
            !matches(&parse("label:risky"), &s, None),
            "label: must exclude sessions never labelled"
        );
    }

    /// Plain typing must find a named session too — the label is the word the
    /// user is most likely to remember, so it cannot need its own prefix.
    #[test]
    fn free_text_matches_the_label() {
        let s = session("something", "mogeung", "main", &[]);
        assert!(matches(&parse("risky"), &s, Some("the risky one")));
        assert!(!matches(&parse("risky"), &s, None));
    }
}
