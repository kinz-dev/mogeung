//! A model in use with no price is raised in Health. `R-J124`.
//!
//! The price table's canary. On 2026-10-08 five models this machine used had
//! no rate, 46% of the history's equivalent cost was missing, and the only
//! sign was a line under the Analytics summary — so nobody saw it for seven
//! weeks. This pins that a usage report puts the gap where the format canary
//! puts an unknown event type, and that only a gap that *can* be answered is
//! raised.

use mogeung_core::health::Alert;
use mogeungd::{state::AppState, store::Store};
use std::path::Path;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn assistant(model: &str) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"2026-10-08T10:00:00.000Z","cwd":"/tmp","message":{{"model":"{model}","content":[],"usage":{{"input_tokens":100,"output_tokens":10}}}}}}"#
    )
}

#[tokio::test]
async fn an_unpriced_anthropic_model_is_an_alert() {
    let home = std::env::temp_dir().join(format!("mogeung-unpriced-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let lines = [
        assistant("claude-opus-5-5"),
        // Released after this build: the gap the alert is for.
        assistant("claude-opus-9"),
        assistant("claude-opus-9"),
        // Not Anthropic's: no published price to go and find, so no alert
        // that could never be answered.
        assistant("qwen3.8-sglang"),
    ];
    // The fold reads complete lines only; a last line without its newline is
    // one still being written.
    let body = lines.join("\n") + "\n";
    write(&home.join("projects").join("-tmp").join("44444444-4444-4444-4444-444444444444.jsonl"), &body);
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home.clone()).unwrap();

    let before = state.health().await;
    assert!(!before.alerts.iter().any(|a| matches!(a, Alert::UnpricedModel { .. })));

    let report = state.usage_report().await;
    assert!(report.unpriced_models.contains(&"qwen3.8-sglang".to_string()), "still unpriced in Analytics");

    let h = state.health().await;
    let unpriced: Vec<_> = h
        .alerts
        .iter()
        .filter_map(|a| match a {
            Alert::UnpricedModel { model, messages } => Some((model.as_str(), *messages)),
            _ => None,
        })
        .collect();
    assert_eq!(unpriced, vec![("claude-opus-9", 2)]);
    assert!(h.urgent_alerts() >= 1, "it is a hole in a total, so it is urgent");
    // The price table is described even with readings off.
    let pricing = h.pricing.expect("pricing health");
    assert_eq!(pricing.built_in_as_of, mogeung_core::pricing::RATES_AS_OF);
    assert_eq!(pricing.host, None);

    let _ = std::fs::remove_dir_all(&home);
}
