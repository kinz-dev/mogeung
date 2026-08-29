//! Embeddings, and the recall question they exist to answer. `R-O6`, and
//! [A38](../../../docs/product/assumptions.md)'s test before it.
//!
//! [Feature 0017](../../../docs/features/0017-cross-session.md) put semantic
//! search out of scope with a reason and a sequence — *"Semantic/embedding
//! search — honest substring/token search first"* — and that condition is met:
//! substring search shipped in 2026-07 and has been in daily use since. So the
//! question is no longer *should there be a second list* but **does a second
//! list earn its place**, which is `A38` and which `--bin judge --recall`
//! measures before anything is drawn.
//!
//! ## Same endpoint, deliberately
//!
//! Embeddings go to the host `model_url` already names, with `embed_model`
//! choosing the model on it. A separate `embed_url` would be a second host to
//! consent to, and [ADR-0031](../../../docs/decisions/0031-consent-to-a-named-host.md)
//! names **one** — a key that could point 67 MB of transcripts at a different
//! machine without asking again would be that gate bypassed by a spelling. The
//! same [`mogeung_core::model::admit`] runs first, so an unconsented host is
//! refused here exactly as it is for a question.
//!
//! ## What is not here
//!
//! No index, no store, no background pass. This module embeds what it is given
//! and compares vectors; where an index would live, and whether it is worth
//! building, is what the harness is for.

use mogeung_core::model::{ModelSettings, admit, normalise_base};
use serde_json::Value;

/// How many texts go in one request.
///
/// Batched because one HTTP round trip per line turns a 500-line corpus into
/// 500 of them; bounded because a batch large enough to exceed the server's
/// own limit fails as one opaque error rather than as a slow answer.
pub const BATCH: usize = 32;

/// The most of one line that is embedded.
///
/// A model has a token limit and a transcript line can be a whole file. The cut
/// is stated wherever a result is shown, because a line that was embedded in
/// part is a line that can be found by its first half only.
pub const MAX_CHARS: usize = 2_000;

/// Embed each text, in order. One vector per input, always.
pub async fn embed(settings: &ModelSettings, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    admit(settings).map_err(|r| r.message())?;
    let base = settings.url.clone().ok_or("no model_url is configured")?;
    let model = settings
        .embed_model
        .clone()
        .ok_or("no embed_model is configured, so there is nothing to embed with")?;
    // The same trimming the chat path does, and for the same reason it exists:
    // `…/v1/models` is the URL a human can `curl`, so it is the one in config
    // files — and `…/v1/models/embeddings` is a 405 that reads like a broken
    // endpoint rather than a mistyped one. `R-O1` bought this lesson once.
    let url = format!("{}/embeddings", normalise_base(&base));

    let mut out = Vec::with_capacity(texts.len());
    for chunk in texts.chunks(BATCH) {
        let input: Vec<String> = chunk.iter().map(|t| clip(t)).collect();
        let body = serde_json::json!({ "model": model, "input": input }).to_string();
        let reply = crate::model::post_json(&url, &body).await?;
        let v: Value = serde_json::from_str(&reply)
            .map_err(|e| format!("the endpoint did not answer with JSON: {e}"))?;
        if let Some(msg) = v.get("error").and_then(|e| e.get("message")).and_then(Value::as_str) {
            return Err(format!("the endpoint refused: {msg}"));
        }
        // The body, when there is no `data`: an endpoint that refuses a batch
        // says why in a shape nobody agrees on, and *answered without data* is
        // the least useful sentence a harness can print.
        let data = v.get("data").and_then(Value::as_array).ok_or_else(|| {
            format!(
                "the endpoint answered without `data`: {}",
                reply.chars().take(300).collect::<String>()
            )
        })?;
        if data.len() != chunk.len() {
            // Refused rather than zipped up hopefully: a short batch silently
            // shifts every vector after it onto the wrong line, and a recall
            // number computed from that would look like a finding.
            return Err(format!(
                "asked for {} embeddings and got {}",
                chunk.len(),
                data.len()
            ));
        }
        for d in data {
            let e = d
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or("an entry carried no `embedding`")?;
            out.push(e.iter().filter_map(Value::as_f64).map(|f| f as f32).collect());
        }
    }
    Ok(out)
}

/// The part of a line that gets embedded.
fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_CHARS {
        return s.to_string();
    }
    s.chars().take(MAX_CHARS).collect()
}

/// Cosine similarity. `0.0` when either side is a zero vector, which is what an
/// endpoint returns for an empty string.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// The `k` nearest of `corpus` to `query`, nearest first, as `(index, score)`.
pub fn nearest(query: &[f32], corpus: &[Vec<f32>], k: usize) -> Vec<(usize, f32)> {
    let mut scored: Vec<(usize, f32)> = corpus
        .iter()
        .enumerate()
        .map(|(i, v)| (i, cosine(query, v)))
        .collect();
    // By score descending, then by index, so a tie is stable rather than
    // whatever the sort felt like — a harness that reorders ties between runs
    // reports differences that are not there.
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_is_nearest_to_itself_and_ties_are_stable() {
        let corpus = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 0.0]];
        let hits = nearest(&[1.0, 0.0], &corpus, 3);
        assert_eq!(hits[0].0, 0);
        assert!((hits[0].1 - 1.0).abs() < 1e-6);
        // 0 and 2 are identical; the lower index wins, every time.
        assert_eq!(hits[1].0, 2);
        assert_eq!(hits[2].0, 1);
    }

    /// An endpoint answers an empty string with zeros, and a NaN in a recall
    /// table is indistinguishable from a bug in the recall table.
    #[test]
    fn a_zero_vector_scores_zero_rather_than_nan() {
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert!(!cosine(&[0.0, 0.0], &[0.0, 0.0]).is_nan());
    }

    #[test]
    fn a_long_line_is_cut_rather_than_refused() {
        let long = "x".repeat(MAX_CHARS * 2);
        assert_eq!(clip(&long).chars().count(), MAX_CHARS);
    }
}
