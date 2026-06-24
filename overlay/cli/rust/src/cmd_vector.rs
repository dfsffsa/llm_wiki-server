use std::path::PathBuf;

use llm_wiki_common::vector;

pub async fn upsert_chunks_from_stdin(project: PathBuf, page_id: String) -> Result<(), String> {
    vector::upsert_chunks_from_stdin(&project, &page_id).await
}

pub async fn delete_page(project: PathBuf, page_id: String) -> Result<(), String> {
    vector::delete_page(&project.to_string_lossy(), &page_id).await
}

pub async fn count_chunks(project: PathBuf) -> Result<(), String> {
    let count = vector::count_chunks(&project.to_string_lossy()).await?;
    println!("{count}");
    Ok(())
}
