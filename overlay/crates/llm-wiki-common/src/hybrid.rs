//! Hybrid (keyword + vector) search merging.
//!
//! The pure logic here is the Reciprocal Rank Fusion (RRF) that merges two
//! ranked lists — keyword hits (keyed by wiki relpath) and vector hits
//! (keyed by `page_id`) — into one ranked list. LanceDB access lives in
//! [`crate::vector`]; file enrichment (title/snippet) lives in
//! [`crate::search`]. This module only owns the rank math, so it is fully
//! unit-testable with no IO.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// k constant for RRF. 60 is the standard value from the original paper;
/// it dampens the influence of rank position so a #1 hit isn't infinitely
/// better than a #2 hit.
const RRF_K: f64 = 60.0;

/// A normalized item appearing in one or both ranked lists, identified by a
/// shared key. For keyword results the key is the wiki relpath; for vector
/// results the key is also resolved to a relpath by the caller (via the
/// `page_id` → relpath map), so fusion dedupes across both channels on path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedItem {
    /// Wiki relpath, e.g. `wiki/concepts/foo.md`. Same shape from both sides.
    pub key: String,
    /// Rank (0-based) in the keyword list, if present.
    pub keyword_rank: Option<usize>,
    /// Rank (0-based) in the vector list, if present. Carries the raw
    /// LanceDB distance for display as `vector_score`.
    pub vector_rank: Option<(usize, f32)>,
}

impl RankedItem {
    /// RRF score: sum of `1/(k + rank)` over the lists the item appears in.
    /// Items in both lists get two contributions; items in one get one.
    pub fn rrf_score(&self) -> f64 {
        let mut score = 0.0;
        if let Some(r) = self.keyword_rank {
            score += 1.0 / (RRF_K + r as f64);
        }
        if let Some((r, _dist)) = self.vector_rank {
            score += 1.0 / (RRF_K + r as f64);
        }
        score
    }
}

/// Fuse a keyword ranked list and a vector ranked list into one, ordered by
/// descending RRF score (ties broken by key for determinism). `limit` caps
/// the output length.
///
/// Both inputs must already be ordered best-first (rank 0 = best). Keys are
/// matched exactly; the caller is responsible for normalizing vector
/// `page_id` to the same relpath form used by keyword results.
pub fn merge_rrf(keyword: Vec<String>, vector: Vec<(String, f32)>, limit: usize) -> Vec<RankedItem> {
    let mut by_key: HashMap<String, RankedItem> = HashMap::new();
    for (rank, key) in keyword.into_iter().enumerate() {
        by_key.entry(key.clone()).or_insert(RankedItem {
            key,
            keyword_rank: None,
            vector_rank: None,
        }).keyword_rank = Some(rank);
    }
    for (rank, (key, dist)) in vector.into_iter().enumerate() {
        by_key.entry(key.clone()).or_insert(RankedItem {
            key,
            keyword_rank: None,
            vector_rank: None,
        }).vector_rank = Some((rank, dist));
    }

    let mut items: Vec<RankedItem> = by_key.into_values().collect();
    items.sort_by(|a, b| {
        b.rrf_score()
            .partial_cmp(&a.rrf_score())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    items.truncate(limit);
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_score_item_in_both_lists_is_higher_than_one_list() {
        let both = RankedItem {
            key: "a".into(),
            keyword_rank: Some(0),
            vector_rank: Some((0, 0.1)),
        };
        let keyword_only = RankedItem {
            key: "b".into(),
            keyword_rank: Some(0),
            vector_rank: None,
        };
        assert!(both.rrf_score() > keyword_only.rrf_score());
    }

    #[test]
    fn merge_returns_keyword_only_when_no_vector() {
        let out = merge_rrf(vec!["wiki/a.md".into()], vec![], 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "wiki/a.md");
        assert_eq!(out[0].keyword_rank, Some(0));
        assert!(out[0].vector_rank.is_none());
    }

    #[test]
    fn merge_returns_vector_only_when_no_keyword() {
        let out = merge_rrf(vec![], vec![("wiki/a.md".into(), 0.2)], 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "wiki/a.md");
        assert!(out[0].keyword_rank.is_none());
        assert_eq!(out[0].vector_rank, Some((0, 0.2)));
    }

    #[test]
    fn merge_dedupes_same_key_across_lists() {
        let out = merge_rrf(
            vec!["wiki/a.md".into()],
            vec![("wiki/a.md".into(), 0.3)],
            10,
        );
        assert_eq!(out.len(), 1);
        let item = &out[0];
        assert_eq!(item.keyword_rank, Some(0));
        assert_eq!(item.vector_rank, Some((0, 0.3)));
    }

    #[test]
    fn merge_orders_by_rrf_score_descending() {
        // a: keyword#0 + vector#0  -> highest
        // b: keyword#1 only
        // c: vector#1 only
        let out = merge_rrf(
            vec!["wiki/a.md".into(), "wiki/b.md".into()],
            vec![("wiki/a.md".into(), 0.1), ("wiki/c.md".into(), 0.4)],
            10,
        );
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].key, "wiki/a.md"); // in both, rank 0 each
        // b (keyword#1) vs c (vector#1): same rank contribution -> tie -> key order
        // wiki/b.md < wiki/c.md, so b before c.
        assert_eq!(out[1].key, "wiki/b.md");
        assert_eq!(out[2].key, "wiki/c.md");
    }

    #[test]
    fn merge_respects_limit() {
        let out = merge_rrf(
            (0..5).map(|i| format!("wiki/{i}.md")).collect(),
            vec![],
            2,
        );
        assert_eq!(out.len(), 2);
    }
}
