//! Reading Anthropic's pricing page once a week, on consent. `R-J124`,
//! feature 0050, [ADR-0042](../../../docs/decisions/0042-a-timer-may-read-a-public-page.md).
//!
//! **Compare-only.** A reading is parsed and checked (`pricing::read_pages`),
//! its disagreements with the built-in table are reported in Health, and the
//! reading itself is kept on disk as evidence. Nothing here prices anything:
//! that waits on A49 and A50, which four weekly readings are meant to settle.
//!
//! Three things keep a timer that reaches the internet honest:
//!
//! - **Consent names the host.** `refresh_pricing = "platform.claude.com"` in
//!   the config file, read at every check so a save takes effect without a
//!   restart. Absent is off; any other host is refused, not reached.
//! - **Once a week, and a failure is not a loop.** A check runs every six
//!   hours; it reads only when the last *successful* reading is a week old,
//!   and it waits six hours after any attempt, so a page that is down is
//!   asked four times a day rather than on every tick.
//! - **Nothing about this machine goes out.** Two GETs of public pages, no
//!   cookies, no headers but curl's own.
//!
//! The record of attempts lives in `<data>/pricing/state.json`, beside the
//! database, so a restart neither forgets a failure nor reads again early.

use chrono::{DateTime, Duration, Utc};
use mogeung_core::health::PricingHealth;
use mogeung_core::pricing::{self, Reading};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// How often the task wakes to ask whether a reading is due.
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 3600);
/// How long after start-up the first check waits, so it never competes with
/// the first scan — which is the one the window is waiting on.
pub const FIRST_CHECK_AFTER: std::time::Duration = std::time::Duration::from_secs(60);
/// A successful reading is good for this long.
const READ_EVERY: Duration = Duration::days(7);
/// After any attempt, wait at least this long before the next. Five minutes
/// under `CHECK_EVERY`, so a tick that lands a little early still counts.
const RETRY_AFTER: Duration = Duration::minutes(6 * 60 - 5);
/// Per request. The page is 49 KB.
const TIMEOUT_SECS: u32 = 15;
/// Anything larger is not the page.
const MAX_BYTES: u32 = 2_000_000;

/// What the config file says about readings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consent {
    Off,
    On,
    /// A host was named, and it is not the one the page is on.
    Refused(String),
}

/// Resolve the `refresh_pricing` value.
///
/// Exactly the host, the way ADR-0031's named consent works: a value that
/// says somewhere else is not consent to here, whatever it was meant to say.
pub fn consent(value: Option<&str>) -> Consent {
    match value.map(str::trim) {
        None | Some("") => Consent::Off,
        Some(h) if h.eq_ignore_ascii_case(pricing::PRICING_HOST) => Consent::On,
        Some(h) => Consent::Refused(format!(
            "refresh_pricing names {h:?}, but the pricing page is on {:?} — nothing is read until it names that host",
            pricing::PRICING_HOST
        )),
    }
}

/// Is a reading due? Off is never due.
pub fn due(h: &PricingHealth, consent: &Consent, now: DateTime<Utc>) -> bool {
    if *consent != Consent::On {
        return false;
    }
    let fresh = h.last_ok.is_some_and(|t| now - t < READ_EVERY);
    let too_soon = h.last_attempt.is_some_and(|t| now - t < RETRY_AFTER);
    !fresh && !too_soon
}

/// Put the consent into the record, so Health says what the file says.
pub fn note_consent(h: &mut PricingHealth, consent: &Consent) {
    h.host = (*consent == Consent::On).then(|| pricing::PRICING_HOST.to_string());
    h.refusal = match consent {
        Consent::Refused(why) => Some(why.clone()),
        _ => None,
    };
}

/// Fold one attempt into the record. Returns the reading when it was
/// accepted, so the caller can keep it.
///
/// A failure leaves the last accepted reading's findings in place: what the
/// page said a week ago is still the best statement of it, and the error
/// beside it says it could not be read since.
pub fn record(
    h: &mut PricingHealth,
    pages: Result<(String, String), String>,
    now: DateTime<Utc>,
) -> Option<Reading> {
    h.last_attempt = Some(now);
    match pages.and_then(|(p, d)| pricing::read_pages(&p, &d)) {
        Ok(reading) => {
            h.last_ok = Some(now);
            h.last_error = None;
            h.models_read = reading.standard.len() as u32;
            h.differences = pricing::differences(&reading);
            h.unmapped = reading.unmapped.clone();
            Some(reading)
        }
        Err(e) => {
            h.last_error = Some(e);
            None
        }
    }
}

/// Where readings and their record are kept: beside the database.
pub fn dir_for(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("pricing")
}

/// The record as last saved, or an empty one. A file that does not parse is
/// an empty record, not a failure to start: the worst it costs is one early
/// reading.
pub fn load(dir: &Path) -> PricingHealth {
    std::fs::read_to_string(dir.join("state.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, h: &PricingHealth, reading: Option<(&Reading, DateTime<Utc>)>) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!("pricing: {} could not be created: {e}", dir.display());
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(h) {
        let _ = std::fs::write(dir.join("state.json"), text);
    }
    // One file per day read, so the readings are a series a later phase can
    // price from — and so A49 has something to be judged on.
    if let Some((r, at)) = reading {
        if let Ok(text) = serde_json::to_string_pretty(r) {
            let _ = std::fs::write(dir.join(format!("{}.json", at.format("%Y-%m-%d"))), text);
        }
    }
}

/// One GET, through curl as the model seam already does — no HTTP client
/// added for two requests a week.
async fn fetch(url: &str) -> Result<String, String> {
    let out = tokio::process::Command::new("curl")
        .args([
            "-sS",
            "-f",
            "-L",
            // Never downgraded, whatever a redirect says.
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            &TIMEOUT_SECS.to_string(),
            "--max-filesize",
            &MAX_BYTES.to_string(),
            url,
        ])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("{url}: {}", if why.is_empty() { out.status.to_string() } else { why }));
    }
    String::from_utf8(out.stdout).map_err(|_| format!("{url}: not text"))
}

/// One check: read the consent, and read the pages if a reading is due.
pub async fn check(state: &crate::state::AppState, dir: &Path) {
    let (config, _) = mogeung_core::config::Config::load();
    let consent = consent(config.refresh_pricing.as_deref());
    let now = Utc::now();
    let wanted = {
        let mut h = state.pricing.lock().await;
        note_consent(&mut h, &consent);
        due(&h, &consent, now)
    };
    if wanted {
        let pages = match fetch(pricing::PRICING_URL).await {
            Ok(p) => fetch(pricing::DEPRECATIONS_URL).await.map(|d| (p, d)),
            Err(e) => Err(e),
        };
        let mut h = state.pricing.lock().await;
        let reading = record(&mut h, pages, now);
        match (&reading, &h.last_error) {
            (Some(_), _) => tracing::info!(
                "pricing: read {} model(s) from {}, {} difference(s) from the built-in table",
                h.models_read,
                pricing::PRICING_HOST,
                h.differences.len()
            ),
            (None, Some(e)) => tracing::warn!("pricing: the page could not be read: {e}"),
            (None, None) => {}
        }
        save(dir, &h, reading.as_ref().map(|r| (r, now)));
    } else {
        save(dir, &*state.pricing.lock().await, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    /// **Off reads nothing**, however stale the record — the default, and the
    /// state of every install that has not said otherwise.
    #[test]
    fn nothing_is_due_without_consent() {
        let h = PricingHealth::default();
        assert!(!due(&h, &Consent::Off, at("2026-10-08T12:00:00Z")));
        assert!(!due(&h, &consent(Some("example.com")), at("2026-10-08T12:00:00Z")));
        assert!(due(&h, &Consent::On, at("2026-10-08T12:00:00Z")));
    }

    #[test]
    fn consent_names_the_host() {
        assert_eq!(consent(None), Consent::Off);
        assert_eq!(consent(Some("  ")), Consent::Off);
        assert_eq!(consent(Some("platform.claude.com")), Consent::On);
        assert_eq!(consent(Some("Platform.Claude.com")), Consent::On);
        let Consent::Refused(why) = consent(Some("evil.example")) else { panic!() };
        assert!(why.contains("evil.example") && why.contains("platform.claude.com"), "{why}");
    }

    /// Weekly on success; a failure waits six hours, not a tick.
    #[test]
    fn reads_weekly_and_backs_off_after_a_failure() {
        let mut h = PricingHealth {
            last_ok: Some(at("2026-10-01T12:00:00Z")),
            last_attempt: Some(at("2026-10-01T12:00:00Z")),
            ..Default::default()
        };
        assert!(!due(&h, &Consent::On, at("2026-10-08T11:59:00Z")), "six days and a bit is fresh");
        assert!(due(&h, &Consent::On, at("2026-10-08T12:00:00Z")));

        h.last_attempt = Some(at("2026-10-08T12:00:00Z"));
        assert!(!due(&h, &Consent::On, at("2026-10-08T15:00:00Z")), "three hours after a failure");
        assert!(due(&h, &Consent::On, at("2026-10-08T18:00:00Z")), "the next six-hour tick");
    }

    /// A failure keeps what the last good reading found, and says why it
    /// could not read since.
    #[test]
    fn a_failure_keeps_the_last_findings() {
        let mut h = PricingHealth::default();
        let page = include_str!("../../mogeung-core/tests/fixtures/pricing-2026-10-08.md");
        let ids = include_str!("../../mogeung-core/tests/fixtures/model-deprecations-2026-10-08.md");
        let ok = record(&mut h, Ok((page.into(), ids.into())), at("2026-10-08T12:00:00Z"));
        assert!(ok.is_some());
        assert_eq!((h.models_read, h.differences.len()), (20, 0));
        assert_eq!(h.unmapped, vec!["Claude Haiku 3.5".to_string()]);

        let failed = record(&mut h, Err("curl: (6) Could not resolve host".into()), at("2026-10-15T12:00:00Z"));
        assert!(failed.is_none());
        assert_eq!(h.last_ok, Some(at("2026-10-08T12:00:00Z")));
        assert_eq!(h.models_read, 20);
        assert!(h.last_error.as_deref().unwrap().contains("resolve"));

        // A page that changed shape is an error like any other.
        record(&mut h, Ok(("<html>".into(), ids.into())), at("2026-10-15T18:00:00Z"));
        assert!(h.last_error.as_deref().unwrap().contains("Model pricing"));
    }

    #[test]
    fn the_record_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("mogeung-pricing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = PricingHealth {
            last_attempt: Some(at("2026-10-08T12:00:00Z")),
            last_error: Some("down".into()),
            ..Default::default()
        };
        save(&dir, &h, Some((&Reading::default(), at("2026-10-08T12:00:00Z"))));
        assert_eq!(load(&dir), h);
        assert!(dir.join("2026-10-08.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
        // Nothing there is an empty record, not an error.
        assert_eq!(load(&dir), PricingHealth::default());
    }

    #[test]
    fn readings_live_beside_the_database() {
        assert_eq!(dir_for(Path::new("/home/u/.mogeung/mogeung.db")), PathBuf::from("/home/u/.mogeung/pricing"));
    }
}
