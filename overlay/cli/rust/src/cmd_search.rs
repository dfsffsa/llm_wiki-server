use std::path::PathBuf;

use llm_wiki_common::project::resolve_project_dir;
use llm_wiki_common::search_keyword;

pub fn run(
    query: String,
    project: PathBuf,
    top_k: usize,
    json: bool,
    include_content: bool,
) -> Result<(), String> {
    let project = resolve_project_dir(project.to_string_lossy().as_ref())?;
    let response = search_keyword(
        project.to_string_lossy().into_owned(),
        query,
        top_k,
        include_content,
    )?;

    if json {
        println!("{}", serde_json::to_string_pretty(&response).map_err(|e| e.to_string())?);
        return Ok(());
    }

    println!("mode: {} (token hits: {})", response.mode, response.token_hits);
    for (idx, hit) in response.results.iter().enumerate() {
        println!(
            "{:2}. [{:.1}] {} — {}",
            idx + 1,
            hit.score,
            hit.path,
            hit.title
        );
        if !hit.snippet.is_empty() {
            println!("    {}", hit.snippet);
        }
    }
    Ok(())
}
