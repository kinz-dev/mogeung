//! The models and thinking levels a seat can be started with, for the form.
//! `R-P16`.
//!
//! Asked 2026-10-13: *"start an agent with different model and thinking
//! level"*, with a Claude seat on Opus at high thinking, another on Sonnet at
//! medium, and an opencode seat on its defaults. The model box was already
//! there and took any text; what it lacked was names to choose from, and a
//! level beside it.
//!
//! **Read from what is on this machine, never by starting a CLI.** Codex
//! keeps the list it shows in `/model` in `models_cache.json` under its home,
//! with the levels each model takes, and that file is read as it is — read
//! only, like every other file under an agent home. opencode can print its
//! list, but only by running `opencode models`, which may refresh opencode's
//! own cache; mogeung does not start a CLI to fill a picker, so an opencode
//! seat, like a Qwen one, gets the free-text box and a hint at the spelling.
//! Claude Code keeps no list; its `--help` names its aliases, and the
//! current model ids are the ones the price table already carries.
//!
//! The box stays free text whatever is listed: a model this file does not
//! know is still the CLI's to accept.

use mogeung_core::session::SessionSource;
use mogeung_core::team::{CliModels, Effort, ModelChoice};
use serde_json::Value;
use std::path::Path;

/// Claude Code's aliases, from its `--help` (2.1.296): each resolves to the
/// newest model of its family when the seat starts. Then the current ids,
/// for a seat that should stay on one model when a newer one ships.
const CLAUDE: &[(&str, &str)] = &[
    ("opus", "Opus — the latest"),
    ("sonnet", "Sonnet — the latest"),
    ("haiku", "Haiku — the latest"),
    ("fable", "Fable — the latest"),
    ("claude-opus-5-5", "Opus 5.5"),
    ("claude-sonnet-5-5", "Sonnet 5.5"),
    ("claude-sonnet-5", "Sonnet 5"),
    ("claude-haiku-5-5", "Haiku 5.5"),
    ("claude-fable-5-1", "Fable 5.1"),
];

/// Every CLI's entry, in the order the form lists CLIs.
pub fn seat_models(codex_home: &Path) -> Vec<CliModels> {
    let cache = codex_home.join("models_cache.json");
    let codex = std::fs::read_to_string(&cache).ok().map(|t| codex_models(&t)).unwrap_or_default();
    let codex_source = if codex.is_empty() {
        format!("no model list at {} — type the model as Codex spells it", cache.display())
    } else {
        "Codex's own list, as its /model shows it".to_string()
    };
    vec![
        CliModels {
            cli: SessionSource::ClaudeCode,
            models: claude_models(),
            source: "Claude Code's aliases and the current model ids".into(),
            efforts: Effort::taken_by(SessionSource::ClaudeCode).to_vec(),
        },
        CliModels {
            cli: SessionSource::Codex,
            models: codex,
            source: codex_source,
            efforts: Effort::taken_by(SessionSource::Codex).to_vec(),
        },
        CliModels {
            cli: SessionSource::QwenCode,
            models: Vec::new(),
            source: "type the model as Qwen Code spells it".into(),
            efforts: Effort::taken_by(SessionSource::QwenCode).to_vec(),
        },
        CliModels {
            cli: SessionSource::OpenCode,
            models: Vec::new(),
            source: "type it as provider/model, the way `opencode models` lists it".into(),
            efforts: Effort::taken_by(SessionSource::OpenCode).to_vec(),
        },
    ]
}

pub fn claude_models() -> Vec<ModelChoice> {
    CLAUDE
        .iter()
        .map(|(id, label)| ModelChoice {
            id: (*id).into(),
            label: (*label).into(),
            efforts: Vec::new(),
            default_effort: None,
        })
        .collect()
}

/// The models Codex lists, from its `models_cache.json`. A model it hides
/// (`visibility` other than `list`) is left out, as its own picker leaves it
/// out. Degrades to empty on any shape it does not expect — the file is
/// Codex's, undocumented, and moves when Codex does.
pub fn codex_models(text: &str) -> Vec<ModelChoice> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(models) = v.get("models").and_then(Value::as_array) else {
        return Vec::new();
    };
    models
        .iter()
        .filter_map(|m| {
            let id = m.get("slug").and_then(Value::as_str)?.trim();
            if id.is_empty() {
                return None;
            }
            if let Some(vis) = m.get("visibility").and_then(Value::as_str) {
                if vis != "list" {
                    return None;
                }
            }
            let efforts = m
                .get("supported_reasoning_levels")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|l| l.get("effort").and_then(Value::as_str).and_then(Effort::parse))
                        .collect()
                })
                .unwrap_or_default();
            Some(ModelChoice {
                id: id.to_string(),
                label: m.get("display_name").and_then(Value::as_str).unwrap_or_default().to_string(),
                efforts,
                default_effort: m.get("default_reasoning_level").and_then(Value::as_str).and_then(Effort::parse),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_lists_what_its_own_picker_lists_with_the_levels_each_takes() {
        let text = r#"{"fetched_at":"x","models":[
          {"slug":"gpt-hidden","visibility":"hide","supported_reasoning_levels":[{"effort":"low"}]},
          {"slug":"gpt-5.6-terra","display_name":"GPT-5.6-Terra","visibility":"list","default_reasoning_level":"medium",
           "supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"},{"effort":"high"},{"effort":"xhigh"},{"effort":"max"},{"effort":"ultra"},{"effort":"telepathic"}]},
          {"slug":"gpt-plain"},
          {"display_name":"no slug"},
          "not an object"
        ]}"#;
        let m = codex_models(text);
        assert_eq!(m.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["gpt-5.6-terra", "gpt-plain"]);
        assert_eq!(m[0].label, "GPT-5.6-Terra");
        assert_eq!(m[0].default_effort, Some(Effort::Medium));
        // A level this build has no word for is dropped, not guessed at.
        assert_eq!(
            m[0].efforts,
            [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max, Effort::Ultra]
        );
        assert!(m[1].efforts.is_empty());
    }

    #[test]
    fn a_codex_cache_in_a_shape_it_does_not_know_is_no_models_not_a_panic() {
        assert!(codex_models("").is_empty());
        assert!(codex_models("[1,2]").is_empty());
        assert!(codex_models(r#"{"models":{"a":1}}"#).is_empty());
    }

    #[test]
    fn every_cli_has_an_entry_and_a_missing_codex_cache_says_where_it_looked() {
        let dir = std::env::temp_dir().join(format!("mogeung-catalogue-{}", std::process::id()));
        let all = seat_models(&dir);
        assert_eq!(all.len(), 4);
        let codex = all.iter().find(|c| c.cli == SessionSource::Codex).unwrap();
        assert!(codex.models.is_empty());
        assert!(codex.source.contains("models_cache.json"), "{}", codex.source);
        let qwen = all.iter().find(|c| c.cli == SessionSource::QwenCode).unwrap();
        assert!(qwen.efforts.is_empty(), "Qwen takes no level at launch");
        let claude = all.iter().find(|c| c.cli == SessionSource::ClaudeCode).unwrap();
        assert!(claude.models.iter().any(|m| m.id == "opus"));
        assert!(!claude.efforts.contains(&Effort::Ultra));
    }
}
