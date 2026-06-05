use serde::Deserialize;
use serde_json::json;

use crate::api::{self, err, ok, resolve_project};
use llm_wiki_common::search_keyword;
use crate::state::ServerState;

const MAX_SEARCH_RESULTS: usize = 50;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest {
    query: String,
    top_k: Option<usize>,
    include_content: Option<bool>,
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
    if req.query_embedding.is_some() {
        return err(
            501,
            "Explicit queryEmbedding is not supported in headless keyword search yet (Phase 1). Hybrid vector search will be added when the core crate is extracted from upstream.",
        );
    }
    let top_k = req.top_k.unwrap_or(10).clamp(1, MAX_SEARCH_RESULTS);
    match search_keyword(
        project.path.clone(),
        req.query,
        top_k,
        req.include_content.unwrap_or(false),
    ) {
        Ok(search) => ok(json!({
            "ok": true,
            "projectId": project.id,
            "mode": search.mode,
            "note": "Headless Phase 1: keyword search only. LanceDB hybrid search pending core crate extraction.",
            "tokenHits": search.token_hits,
            "vectorHits": search.vector_hits,
            "results": search.results,
        })),
        Err(e) => err(500, e),
    }
}
