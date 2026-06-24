use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{self, err, ok, resolve_project};
use crate::llm::{embed_query, parse_embedding_config};
use crate::state::ServerState;
use llm_wiki_common::hybrid_search;

const MAX_SEARCH_RESULTS: usize = 50;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest {
    query: String,
    top_k: Option<usize>,
    include_content: Option<bool>,
    /// Optional pre-computed query embedding (e.g. from a client that
    /// already embedded the query). When present, skips server-side
    /// embedding. When absent, the server embeds via embeddingConfig if
    /// configured; otherwise search is keyword-only.
    query_embedding: Option<Vec<f32>>,
}

pub fn handle_search(state: &ServerState, project_id: &str, body: &str) -> api::ApiResponse {
    let project = match resolve_project(state, project_id) {
        Ok(project) => project,
        Err(e) => return err(404, e),
    };
    let req: SearchRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return err(400, format!("Invalid JSON: {e}")),
    };
    if req.query.trim().is_empty() {
        return err(400, "query is required");
    }
    let top_k = req.top_k.unwrap_or(10).clamp(1, MAX_SEARCH_RESULTS);
    let include_content = req.include_content.unwrap_or(false);

    // Resolve the query embedding, if possible. Any failure here degrades to
    // keyword-only — the endpoint never 500s just because vectors are
    // unavailable. Priority: client-supplied > server-side embed.
    let query_embedding =
        resolve_query_embedding(state, &req.query, req.query_embedding.as_deref());

    // hybrid_search runs keyword always and fuses vectors when an embedding
    // is supplied; it swallows vector-side failures internally (returns
    // keyword-only). It is async (LanceDB access), so block_on the shared
    // runtime.
    let response = match state.runtime() {
        Some(runtime) => match runtime.block_on(hybrid_search(
            project.path.clone(),
            req.query,
            top_k,
            include_content,
            query_embedding,
        )) {
            Ok(r) => r,
            Err(e) => return err(500, e),
        },
        None => {
            // No runtime → keyword-only fallback (no vector access).
            match llm_wiki_common::search_keyword(
                project.path.clone(),
                req.query,
                top_k,
                include_content,
            ) {
                Ok(r) => r,
                Err(e) => return err(500, e),
            }
        }
    };

    ok(json!({
        "ok": true,
        "projectId": project.id,
        "mode": response.mode,
        "tokenHits": response.token_hits,
        "vectorHits": response.vector_hits,
        "results": response.results,
    }))
}

/// Decide which query embedding (if any) to pass into hybrid search.
///
/// - If the client supplied one, trust it.
/// - Else if embeddingConfig is enabled, embed server-side. On any failure
///   (no runtime, missing key, endpoint down) log and return None — the
///   caller degrades to keyword-only.
/// - Else None (keyword-only).
fn resolve_query_embedding(
    state: &ServerState,
    query: &str,
    client_embedding: Option<&[f32]>,
) -> Option<Vec<f32>> {
    if let Some(emb) = client_embedding {
        if !emb.is_empty() {
            return Some(emb.to_vec());
        }
    }
    let app_state = state.load_app_state().unwrap_or(Value::Null);
    let emb_cfg = parse_embedding_config(&app_state)?;
    let runtime = state.runtime()?;
    // Embedding the query is best-effort; failures degrade to keyword.
    match runtime.block_on(embed_query(&emb_cfg, query)) {
        Ok(vec) => Some(vec),
        Err(e) => {
            eprintln!("[search] query embedding failed, keyword-only: {e}");
            None
        }
    }
}
