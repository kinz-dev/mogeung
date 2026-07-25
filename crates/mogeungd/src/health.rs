//! Accumulates what the scan loop saw, and derives alerts from it.
//!
//! Roadmap `R-A1` (format canary), `R-A2` (version watch) and `R-A5` (skipped
//! history) all feed one structure, because from the user's side they are one
//! question: *is mogeung still seeing everything?*
//!
//! Counting lives here rather than in the parser so that `adapter::parse_line`
//! stays a pure function of one line — easy to test, and impossible to make
//! stateful by accident.

use chrono::{DateTime, Utc};
use mogeung_core::health::{Alert, Health, LineClass};
use std::collections::{BTreeMap, BTreeSet};

/// Running totals across every scan since the daemon started.
#[derive(Debug, Default)]
pub struct HealthTracker {
    last_scan: Option<chrono::DateTime<chrono::Utc>>,
    scans: u64,

    sessions_known: u64,
    sessions_live: u64,
    transcripts_found: u64,

    lines_parsed: u64,
    lines_ignored: u64,
    lines_barren: u64,
    lines_malformed: u64,

    /// Unrecognised types and how often each appeared. Ordered so the health
    /// view is stable between frames rather than shuffling on every repaint.
    unknown_types: BTreeMap<String, u64>,

    /// Every version seen anywhere in the watched history.
    versions: BTreeSet<String>,

    /// The version behind the newest transcript line seen so far, and when.
    ///
    /// Ordering by *encounter* was the obvious implementation and it was wrong:
    /// the scan walks 30 unrelated sessions spanning a fortnight, so "the last
    /// two versions I encountered" reported a change between two historical
    /// releases while the user was running something newer than both. Ordering
    /// by the transcript's own timestamp is the only thing that means anything.
    current: Option<(String, DateTime<Utc>)>,

    /// What `current` was before it last changed.
    previous: Option<String>,

    /// Transcripts whose early history we skipped, and by how much.
    skipped: BTreeMap<String, u64>,

    max_transcript_bytes: u64,
}

impl HealthTracker {
    pub fn new(max_transcript_bytes: u64) -> Self {
        HealthTracker {
            max_transcript_bytes,
            ..Default::default()
        }
    }

    pub fn record_line(&mut self, class: LineClass) {
        match class {
            LineClass::Parsed => self.lines_parsed += 1,
            LineClass::Ignored => self.lines_ignored += 1,
            LineClass::Barren => self.lines_barren += 1,
            LineClass::Malformed => self.lines_malformed += 1,
            // Counted by name in `record_unknown` so the alert can say which.
            LineClass::Unknown => {}
        }
    }

    pub fn record_unknown(&mut self, event_type: &str) {
        *self.unknown_types.entry(event_type.to_string()).or_insert(0) += 1;
    }

    /// Note the Claude Code version a transcript line reported, and when that
    /// line was written.
    ///
    /// `at` is the line's own timestamp, not the time we read it. Sessions are
    /// scanned newest-file-first and each carries whatever release it ran under,
    /// so read order says nothing about which version is current.
    pub fn record_version(&mut self, version: &str, at: Option<DateTime<Utc>>) {
        if version.is_empty() {
            return;
        }
        self.versions.insert(version.to_string());

        // A line with no timestamp cannot move the clock forward, but it should
        // still establish a baseline if we have nothing at all.
        let Some(at) = at else {
            if self.current.is_none() {
                self.current = Some((version.to_string(), DateTime::<Utc>::MIN_UTC));
            }
            return;
        };

        match &self.current {
            Some((cur, cur_at)) if at <= *cur_at => {}
            Some((cur, _)) if cur == version => {
                self.current = Some((version.to_string(), at));
            }
            Some((cur, _)) => {
                self.previous = Some(cur.clone());
                self.current = Some((version.to_string(), at));
            }
            None => self.current = Some((version.to_string(), at)),
        }
    }

    /// Record that a transcript was too large to read whole.
    ///
    /// Keyed by session so re-scanning the same oversized file does not inflate
    /// the total every tick.
    pub fn record_skipped(&mut self, session_id: &str, bytes: u64) {
        self.skipped.insert(session_id.to_string(), bytes);
    }

    pub fn finish_scan(&mut self, sessions_known: u64, sessions_live: u64, transcripts: u64) {
        self.scans += 1;
        self.last_scan = Some(chrono::Utc::now());
        self.sessions_known = sessions_known;
        self.sessions_live = sessions_live;
        self.transcripts_found = transcripts;
    }

    fn lines_unknown(&self) -> u64 {
        self.unknown_types.values().sum()
    }

    /// Build the client-facing snapshot, alerts included.
    pub fn snapshot(&self) -> Health {
        let lines_unknown = self.lines_unknown();
        let lines_seen = self.lines_parsed
            + self.lines_ignored
            + self.lines_barren
            + lines_unknown
            + self.lines_malformed;

        let mut alerts = Vec::new();

        // Most-frequent unknown type first: if a format change introduced
        // several, the common one is the one costing us the most data.
        let mut unknown: Vec<(String, u64)> =
            self.unknown_types.iter().map(|(k, v)| (k.clone(), *v)).collect();
        unknown.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (event_type, count) in &unknown {
            alerts.push(Alert::UnknownEventType {
                event_type: event_type.clone(),
                count: *count,
            });
        }

        if self.lines_malformed > 0 {
            alerts.push(Alert::MalformedLines {
                count: self.lines_malformed,
            });
        }

        // Only a genuine move of the *current* version. Merely having seen many
        // releases across a fortnight of history is not a change — it is what
        // any long-lived transcript directory looks like.
        if let (Some(from), Some((to, _))) = (&self.previous, &self.current) {
            alerts.push(Alert::VersionChanged {
                from: from.clone(),
                to: to.clone(),
            });
        }

        for (session_id, bytes) in &self.skipped {
            alerts.push(Alert::HistorySkipped {
                session_id: session_id.clone(),
                skipped_bytes: *bytes,
            });
        }

        Health {
            last_scan: self.last_scan,
            scans: self.scans,
            sessions_known: self.sessions_known,
            sessions_live: self.sessions_live,
            transcripts_found: self.transcripts_found,
            lines_seen,
            lines_parsed: self.lines_parsed,
            lines_ignored: self.lines_ignored,
            lines_barren: self.lines_barren,
            lines_unknown,
            lines_malformed: self.lines_malformed,
            unknown_types: unknown,
            versions_seen: self.versions.iter().cloned().collect(),
            current_version: self.current.as_ref().map(|(v, _)| v.clone()),
            history_skipped_bytes: self.skipped.values().sum(),
            max_transcript_bytes: self.max_transcript_bytes,
            alerts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_scan_raises_nothing() {
        let mut t = HealthTracker::new(8 << 20);
        for _ in 0..50 {
            t.record_line(LineClass::Parsed);
        }
        for _ in 0..200 {
            t.record_line(LineClass::Ignored);
        }
        t.record_version("2.1.219", at("2026-07-25T10:00:00Z"));
        t.finish_scan(3, 1, 3);

        let h = t.snapshot();
        assert!(h.alerts.is_empty(), "unexpected alerts: {:?}", h.alerts);
        assert_eq!(h.lines_seen, 250);
        assert_eq!(h.blind_ratio(), 0.0);
    }

    #[test]
    fn one_unknown_type_is_enough_to_raise_it() {
        let mut t = HealthTracker::new(8 << 20);
        t.record_line(LineClass::Unknown);
        t.record_unknown("warp-drive");
        let h = t.snapshot();

        assert_eq!(h.lines_unknown, 1);
        assert_eq!(h.lines_seen, 1, "an unknown line must still count as seen");
        match &h.alerts[..] {
            [Alert::UnknownEventType { event_type, count }] => {
                assert_eq!(event_type, "warp-drive");
                assert_eq!(*count, 1);
            }
            other => panic!("expected exactly one unknown-type alert, got {other:?}"),
        }
    }

    #[test]
    fn unknown_types_are_ranked_by_how_much_they_cost_us() {
        let mut t = HealthTracker::new(8 << 20);
        for _ in 0..3 {
            t.record_unknown("rare");
        }
        for _ in 0..40 {
            t.record_unknown("common");
        }
        let h = t.snapshot();
        assert_eq!(h.unknown_types[0].0, "common");
        assert_eq!(h.unknown_types[0].1, 40);
    }

    fn at(s: &str) -> Option<chrono::DateTime<Utc>> {
        Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn a_real_upgrade_is_reported_once_and_the_right_way_round() {
        let mut t = HealthTracker::new(8 << 20);
        t.record_version("2.1.219", at("2026-07-01T10:00:00Z"));
        t.record_version("2.1.220", at("2026-07-10T10:00:00Z"));
        // Re-reporting the same version is not a change.
        t.record_version("2.1.220", at("2026-07-11T10:00:00Z"));

        let h = t.snapshot();
        assert_eq!(h.current_version.as_deref(), Some("2.1.220"));
        let changes: Vec<_> = h
            .alerts
            .iter()
            .filter(|a| matches!(a, Alert::VersionChanged { .. }))
            .collect();
        assert_eq!(changes.len(), 1, "one alert per upgrade, not per release");
        assert_eq!(
            changes[0],
            &Alert::VersionChanged {
                from: "2.1.219".into(),
                to: "2.1.220".into()
            }
        );
    }

    /// Regression, found by running against a real 30-session corpus.
    ///
    /// Transcripts are scanned newest-file-first and each reports whatever
    /// release it ran under, so *encounter* order is unrelated to time. The
    /// first implementation reported "changed from 2.1.209 to 2.1.210" — two
    /// historical releases, in the wrong order — while the user was actually
    /// running 2.1.220. A confidently wrong alert is worse than no alert.
    #[test]
    fn old_sessions_scanned_after_new_ones_do_not_fake_a_downgrade() {
        let mut t = HealthTracker::new(8 << 20);
        // Newest transcript first, as scan_transcripts orders them.
        t.record_version("2.1.220", at("2026-07-25T10:00:00Z"));
        // Then a fortnight of older sessions, in no particular order.
        for (v, ts) in [
            ("2.1.209", "2026-07-14T10:00:00Z"),
            ("2.1.214", "2026-07-18T10:00:00Z"),
            ("2.1.210", "2026-07-15T10:00:00Z"),
            ("2.1.142", "2026-07-12T10:00:00Z"),
        ] {
            t.record_version(v, at(ts));
        }

        let h = t.snapshot();
        assert_eq!(
            h.current_version.as_deref(),
            Some("2.1.220"),
            "the newest transcript decides what you are running"
        );
        assert!(
            h.alerts.is_empty(),
            "reading old history is not an upgrade: {:?}",
            h.alerts
        );
        assert_eq!(h.versions_seen.len(), 5, "all versions are still listed");
    }

    /// A session that spanned an upgrade is a true positive: its own lines move
    /// forward in time across the boundary.
    #[test]
    fn a_session_spanning_an_upgrade_still_reports_it() {
        let mut t = HealthTracker::new(8 << 20);
        t.record_version("2.1.219", at("2026-07-25T09:00:00Z"));
        t.record_version("2.1.220", at("2026-07-25T11:00:00Z"));
        let h = t.snapshot();
        assert_eq!(h.current_version.as_deref(), Some("2.1.220"));
        assert_eq!(h.alerts.len(), 1);
    }

    #[test]
    fn a_single_version_is_not_a_change() {
        let mut t = HealthTracker::new(8 << 20);
        t.record_version("2.1.219", at("2026-07-25T10:00:00Z"));
        let h = t.snapshot();
        assert!(h.alerts.is_empty());
        assert_eq!(h.current_version.as_deref(), Some("2.1.219"));
    }

    #[test]
    fn rescanning_an_oversized_file_does_not_inflate_the_total() {
        let mut t = HealthTracker::new(8 << 20);
        for _ in 0..10 {
            t.record_skipped("sess-1", 4_000_000);
        }
        t.record_skipped("sess-2", 1_000_000);

        let h = t.snapshot();
        assert_eq!(h.history_skipped_bytes, 5_000_000);
        assert_eq!(
            h.alerts.len(),
            2,
            "one alert per oversized session, not per scan"
        );
        assert_eq!(h.urgent_alerts(), 0, "a stated cap is not an emergency");
    }
}
