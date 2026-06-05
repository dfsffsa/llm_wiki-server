use std::path::{Path, PathBuf};

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

pub fn resolve_project_dir(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project path: {e}"))?;
    if !canonical.is_dir() {
        return Err(format!("Project path is not a directory: {}", canonical.display()));
    }
    Ok(canonical)
}

pub fn read_project_id(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path.join(".llm-wiki/project.json")).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn project_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Project")
        .to_string()
}

pub fn sources_dir(project: &Path) -> PathBuf {
    project.join("raw/sources")
}

pub fn wiki_dir(project: &Path) -> PathBuf {
    project.join("wiki")
}
