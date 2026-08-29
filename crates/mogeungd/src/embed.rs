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

/// Group vectors that are within `threshold` of each other. `R-F4` by meaning.
///
/// **Greedy single-link, and the shape is the honesty.** Each item joins the
/// first cluster whose *seed* it is close enough to, and seeds are taken in
/// input order — so the caller decides what leads a cluster by deciding the
/// order, and the result is reproducible rather than dependent on a random
/// restart. k-means would need a `k` nobody knows and would move rows between
/// runs; agglomerative linkage would join two clusters through a chain of
/// near-misses, which is how *timeout* and *permission denied* end up in one
/// row and the panel starts lying.
///
/// Returns index groups, largest first, each preserving input order.
pub fn cluster(vectors: &[Vec<f32>], threshold: f32) -> Vec<Vec<usize>> {
    let mut seeds: Vec<usize> = Vec::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, v) in vectors.iter().enumerate() {
        let mut joined = false;
        for (g, &seed) in seeds.iter().enumerate() {
            if cosine(v, &vectors[seed]) >= threshold {
                groups[g].push(i);
                joined = true;
                break;
            }
        }
        if !joined {
            seeds.push(i);
            groups.push(vec![i]);
        }
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    groups
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
    fn clustering_joins_the_close_and_leaves_the_far_alone() {
        let a = vec![1.0, 0.0];
        let a2 = vec![0.99, 0.14];
        let b = vec![0.0, 1.0];
        let groups = cluster(&[a, a2, b], 0.9);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0, 1], "the two near vectors join, in input order");
        assert_eq!(groups[1], vec![2]);
    }

    /// Single-link through a chain is how *timeout* and *permission denied*
    /// end up in one row. Each item is compared to the **seed**, so a chain of
    /// near-misses cannot walk a cluster across the space.
    #[test]
    fn a_chain_of_near_misses_does_not_walk_a_cluster() {
        // Each adjacent pair is close; the ends are not.
        let v = |x: f32, y: f32| vec![x, y];
        let groups = cluster(&[v(1.0, 0.0), v(0.8, 0.6), v(0.0, 1.0)], 0.85);
        assert_eq!(groups.len(), 3, "no cluster spans the ends through the middle");
    }

    #[test]
    fn a_long_line_is_cut_rather_than_refused() {
        let long = "x".repeat(MAX_CHARS * 2);
        assert_eq!(clip(&long).chars().count(), MAX_CHARS);
    }
}

// ---------------------------------------------------------------------------
// The index (R-O6)
// ---------------------------------------------------------------------------

/// How much of the corpus is indexed.
///
/// A bound rather than everything, and stated where a reader can see it: a
/// vector is ~4 KB, so this is tens of megabytes of database for a corpus of
/// hundreds of sessions. The list says how many lines it searched, because a
/// *similar* list that quietly covers a tenth of the corpus is worse than no
/// list — it looks like an answer.
pub const INDEX_LINES: usize = 4_000;

/// The most rows a query answers with.
pub const TOP_K: usize = 8;

/// Below this a "similar" hit is noise wearing a score.
///
/// From the recall run rather than from taste: the answers it found ranked at
/// 0.6 and above, and everything under that was a different subject with a
/// shared word.
pub const FLOOR: f32 = 0.45;

/// Build the index from the corpus, and replace whatever was there. `R-O6`.
///
/// Asked for, never automatic — ADR-0031 clause 6, and `R-J8`'s rule that the
/// scan tick does no work. Returns how many lines were indexed.
pub async fn build_index(
    settings: &ModelSettings,
    store: &crate::store::Store,
    projects_root: &std::path::Path,
    history_path: &std::path::Path,
) -> Result<usize, String> {
    let model = settings
        .embed_model
        .clone()
        .ok_or("no embed_model is configured, so there is nothing to build with")?;
    let corpus = crate::insight::corpus_lines(projects_root, history_path, INDEX_LINES);
    if corpus.is_empty() {
        return Err("there is nothing in the corpus to index yet".into());
    }
    let texts: Vec<String> = corpus.iter().map(|c| c.text.clone()).collect();
    let vectors = embed(settings, &texts).await?;
    if vectors.len() != corpus.len() {
        return Err(format!(
            "the endpoint returned {} vectors for {} lines",
            vectors.len(),
            corpus.len()
        ));
    }
    let rows: Vec<(String, u64, String, Option<String>, String, Vec<f32>)> = corpus
        .iter()
        .zip(vectors)
        .map(|(c, v)| {
            (
                crate::insight::session_id_of(&c.path),
                c.line,
                c.role.clone(),
                c.timestamp.map(|t| t.to_rfc3339()),
                c.text.chars().take(300).collect::<String>(),
                v,
            )
        })
        .collect();
    let n = rows.len();
    let built = chrono::Utc::now().timestamp_millis();
    store
        .replace_semantic_index(&rows, &model, built)
        .map_err(|e| format!("the index could not be written: {e}"))?;
    Ok(n)
}

/// Ask the index. `R-O6`.
pub async fn search_index(
    settings: &ModelSettings,
    store: &crate::store::Store,
    query: &str,
) -> Result<Vec<mogeung_core::wire::SemanticHit>, String> {
    let rows = store
        .semantic_rows()
        .map_err(|e| format!("the index could not be read: {e}"))?;
    if rows.is_empty() {
        return Err("there is no index yet — build one to get a second list".into());
    }
    let qv = embed(settings, &[query.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or("the endpoint returned no vector for that query")?;
    let vectors: Vec<Vec<f32>> = rows.iter().map(|r| r.5.clone()).collect();
    Ok(nearest(&qv, &vectors, TOP_K)
        .into_iter()
        .filter(|(_, score)| *score >= FLOOR)
        .map(|(i, score)| {
            let (session_id, line, role, ts, preview, _) = &rows[i];
            mogeung_core::wire::SemanticHit {
                session_id: session_id.clone(),
                line: *line,
                role: role.clone(),
                timestamp: ts
                    .as_deref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|t| t.with_timezone(&chrono::Utc)),
                preview: preview.clone(),
                score,
            }
        })
        .collect())
}

/// How close two failures must be to be called the same failure.
///
/// **From the corpus, not from taste.** `--bin judge --clusters` over 232
/// literal groups on this machine: at `0.75` the *zsh rejected my command*
/// family merges two different mistakes — a bad glob and an unquoted `==` —
/// which is a coarser statement than a panel should make on its own. At `0.85`
/// those separate and the joins that remain are the same failure worded
/// differently: nine spellings of one shell error, five of a browser timeout,
/// four of a two-minute command timeout. At `0.92` almost nothing joins and the
/// feature is a rename of the list that already exists.
pub const CLUSTER_THRESHOLD: f32 = 0.85;

/// Group recurring failures by meaning. `R-F4` by meaning, `R-O6`.
///
/// The literal groups are the input, not the transcripts: `insight`'s
/// normalisation has already done the cheap, checkable half of the job, and
/// re-doing it with a model would be spending a model call to reach the same
/// place. What this adds is the join **between** groups that no normalisation
/// can make — *(eval):1: unmatched '* and *(eval):1: == not found* are one
/// mistake and share not one distinctive word.
pub async fn cluster_failures(
    settings: &ModelSettings,
    failures: Vec<mogeung_core::insight::RecurringFailure>,
) -> Result<Vec<mogeung_core::insight::FailureCluster>, String> {
    use mogeung_core::insight::FailureCluster;
    if failures.is_empty() {
        return Ok(Vec::new());
    }
    // The **example** rather than the normalised key: normalisation replaces
    // the digits and paths, so embedding the key would embed the normaliser.
    let texts: Vec<String> = failures.iter().map(|f| f.example.clone()).collect();
    let vectors = embed(settings, &texts).await?;
    if vectors.len() != failures.len() {
        return Err(format!(
            "the endpoint returned {} vectors for {} groups",
            vectors.len(),
            failures.len()
        ));
    }
    let mut out: Vec<FailureCluster> = cluster(&vectors, CLUSTER_THRESHOLD)
        .into_iter()
        .map(|group| {
            let mut members: Vec<_> = group.iter().map(|&i| failures[i].clone()).collect();
            // Largest first, so the face of the cluster is the failure you have
            // actually been hitting rather than whichever one was seen first.
            members.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.example.cmp(&b.example)));
            let mut sessions: Vec<String> =
                members.iter().flat_map(|m| m.sessions.clone()).collect();
            sessions.sort();
            sessions.dedup();
            FailureCluster {
                label: members[0].example.clone(),
                count: members.iter().map(|m| m.count).sum(),
                sessions,
                members,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.sessions
            .len()
            .cmp(&a.sessions.len())
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.label.cmp(&b.label))
    });
    Ok(out)
}
