use std::path::Path;

use arrow_array::{
    ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt32Array,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;
use serde::Deserialize;
use std::sync::Arc;

const TABLE_V2: &str = "wiki_chunks_v2";

#[derive(Debug, Deserialize)]
pub struct ChunkUpsertInput {
    pub chunk_index: u32,
    pub chunk_text: String,
    pub heading_path: String,
    pub embedding: Vec<f32>,
}

fn db_path(project_path: &str) -> String {
    format!("{}/.llm-wiki/lancedb", project_path.replace('\\', "/"))
}

fn validate_page_id(page_id: &str) -> Result<(), String> {
    if page_id.is_empty() || page_id.len() > 256 {
        return Err("Invalid page_id: empty or too long".to_string());
    }
    if !page_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "Invalid page_id: contains disallowed characters: {page_id}"
        ));
    }
    Ok(())
}

fn make_schema_v2(dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("page_id", DataType::Utf8, false),
        Field::new("chunk_index", DataType::UInt32, false),
        Field::new("chunk_text", DataType::Utf8, false),
        Field::new("heading_path", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim),
            false,
        ),
    ]))
}

fn make_batch_v2(
    schema: Arc<Schema>,
    page_id: &str,
    chunks: &[ChunkUpsertInput],
    dim: i32,
) -> Result<RecordBatch, String> {
    let chunk_ids: Vec<String> = chunks
        .iter()
        .map(|c| format!("{page_id}#{}", c.chunk_index))
        .collect();
    let page_ids = vec![page_id.to_string(); chunks.len()];
    let indexes: Vec<u32> = chunks.iter().map(|c| c.chunk_index).collect();
    let texts: Vec<String> = chunks.iter().map(|c| c.chunk_text.clone()).collect();
    let headings: Vec<String> = chunks.iter().map(|c| c.heading_path.clone()).collect();

    let mut flat: Vec<f32> = Vec::with_capacity(chunks.len() * dim as usize);
    for chunk in chunks {
        if chunk.embedding.len() != dim as usize {
            return Err(format!(
                "Chunk #{} embedding dim {} != expected {dim}",
                chunk.chunk_index,
                chunk.embedding.len()
            ));
        }
        flat.extend(&chunk.embedding);
    }

    let chunk_ids_arr: ArrayRef = Arc::new(StringArray::from(chunk_ids));
    let page_ids_arr: ArrayRef = Arc::new(StringArray::from(page_ids));
    let indexes_arr: ArrayRef = Arc::new(UInt32Array::from(indexes));
    let texts_arr: ArrayRef = Arc::new(StringArray::from(texts));
    let heading_paths_arr: ArrayRef = Arc::new(StringArray::from(headings));
    let values = Float32Array::from(flat);
    let vector_arr: ArrayRef = Arc::new(FixedSizeListArray::new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim,
        Arc::new(values),
        None,
    ));

    RecordBatch::try_new(
        schema,
        vec![
            chunk_ids_arr,
            page_ids_arr,
            indexes_arr,
            texts_arr,
            heading_paths_arr,
            vector_arr,
        ],
    )
    .map_err(|e| format!("Batch error: {e}"))
}

pub async fn upsert_chunks(
    project_path: &str,
    page_id: &str,
    chunks: Vec<ChunkUpsertInput>,
) -> Result<(), String> {
    validate_page_id(page_id)?;
    if chunks.is_empty() {
        return Ok(());
    }

    let dim = chunks[0].embedding.len() as i32;
    if dim == 0 {
        return Err("Chunk #0 has empty embedding".to_string());
    }

    let db = connect(&db_path(project_path))
        .execute()
        .await
        .map_err(|e| format!("DB connect error: {e}"))?;

    let schema = make_schema_v2(dim);
    let batch = make_batch_v2(schema.clone(), page_id, &chunks, dim)?;
    let data = vec![batch];

    let tables = db
        .table_names()
        .execute()
        .await
        .map_err(|e| format!("List tables error: {e}"))?;

    if tables.contains(&TABLE_V2.to_string()) {
        let table = db
            .open_table(TABLE_V2)
            .execute()
            .await
            .map_err(|e| format!("Open table error: {e}"))?;
        let _ = table.delete(&format!("page_id = '{page_id}'")).await;
        table
            .add(data)
            .execute()
            .await
            .map_err(|e| format!("Add error: {e}"))?;
    } else {
        db.create_table(TABLE_V2, data)
            .execute()
            .await
            .map_err(|e| format!("Create table error: {e}"))?;
    }
    Ok(())
}

pub async fn delete_page(project_path: &str, page_id: &str) -> Result<(), String> {
    validate_page_id(page_id)?;
    let db = connect(&db_path(project_path))
        .execute()
        .await
        .map_err(|e| format!("DB connect error: {e}"))?;
    let tables = db
        .table_names()
        .execute()
        .await
        .map_err(|e| format!("List tables error: {e}"))?;
    if !tables.contains(&TABLE_V2.to_string()) {
        return Ok(());
    }
    let table = db
        .open_table(TABLE_V2)
        .execute()
        .await
        .map_err(|e| format!("Open table error: {e}"))?;
    table
        .delete(&format!("page_id = '{page_id}'"))
        .await
        .map_err(|e| format!("Delete error: {e}"))?;
    Ok(())
}

pub async fn count_chunks(project_path: &str) -> Result<usize, String> {
    let db = connect(&db_path(project_path))
        .execute()
        .await
        .map_err(|e| format!("DB connect error: {e}"))?;
    let tables = db
        .table_names()
        .execute()
        .await
        .map_err(|e| format!("List tables error: {e}"))?;
    if !tables.contains(&TABLE_V2.to_string()) {
        return Ok(0);
    }
    let table = db
        .open_table(TABLE_V2)
        .execute()
        .await
        .map_err(|e| format!("Open table error: {e}"))?;
    Ok(table.count_rows(None).await.map_err(|e| e.to_string())?)
}

#[derive(Debug, Deserialize)]
struct UpsertPayload {
    chunks: Vec<ChunkUpsertInput>,
}

pub async fn upsert_chunks_from_stdin(project_path: &Path, page_id: &str) -> Result<(), String> {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
        .map_err(|e| format!("Failed to read stdin: {e}"))?;
    let payload: UpsertPayload =
        serde_json::from_str(&input).map_err(|e| format!("Invalid JSON on stdin: {e}"))?;
    upsert_chunks(
        &project_path.to_string_lossy(),
        page_id,
        payload.chunks,
    )
    .await
}
