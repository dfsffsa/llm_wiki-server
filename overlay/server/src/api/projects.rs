use std::borrow::ToOwned;
use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{self, ok, normalize_path, project_name_from_path, read_project_id};
use crate::state::ServerState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub current: bool,
}

pub fn handle_projects(state: &ServerState) -> api::ApiResponse {
    let projects = load_projects(state);
    let current_project = projects.iter().find(|project| project.current).cloned();
    ok(json!({
        "ok": true,
        "projects": projects,
        "currentProject": current_project,
    }))
}

pub fn load_projects(state: &ServerState) -> Vec<ProjectEntry> {
    let current = normalize_path(&state.project_path().to_string_lossy());
    let mut by_path: BTreeMap<String, ProjectEntry> = BTreeMap::new();

    if let Some(parsed) = state.load_app_state() {
        if let Some(registry) = parsed.get("projectRegistry").and_then(Value::as_object) {
            for (id, value) in registry {
                let path = value.get("path").and_then(Value::as_str).unwrap_or("");
                if path.is_empty() {
                    continue;
                }
                let path = normalize_path(path);
                let name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| project_name_from_path(&path));
                by_path.insert(
                    path.clone(),
                    ProjectEntry {
                        id: id.clone(),
                        name,
                        current: path == current,
                        path,
                    },
                );
            }
        }
        if let Some(recents) = parsed.get("recentProjects").and_then(Value::as_array) {
            for value in recents {
                let path = value.get("path").and_then(Value::as_str).unwrap_or("");
                if path.is_empty() {
                    continue;
                }
                let path = normalize_path(path);
                by_path.entry(path.clone()).or_insert_with(|| {
                    let id = read_project_id(&path).unwrap_or_else(|| path.clone());
                    let name = value
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| project_name_from_path(&path));
                    ProjectEntry {
                        id,
                        name,
                        current: path == current,
                        path,
                    }
                });
            }
        }
        for path in configured_project_paths(&parsed) {
            insert_project_path(&mut by_path, &path, &current);
        }
    }

    for path in configured_project_paths_from_env() {
        insert_project_path(&mut by_path, &path, &current);
    }

    if !current.is_empty() {
        by_path
            .entry(current.clone())
            .and_modify(|entry| entry.current = true)
            .or_insert_with(|| ProjectEntry {
                id: read_project_id(&current).unwrap_or_else(|| current.clone()),
                name: project_name_from_path(&current),
                current: true,
                path: current.clone(),
            });
    }

    by_path.into_values().collect()
}

fn insert_project_path(
    by_path: &mut BTreeMap<String, ProjectEntry>,
    raw_path: &str,
    current: &str,
) {
    let path = match canonicalize_project_path(raw_path) {
        Some(p) => p,
        None => return,
    };
    by_path.entry(path.clone()).or_insert_with(|| ProjectEntry {
        id: read_project_id(&path).unwrap_or_else(|| path.clone()),
        name: project_name_from_path(&path),
        current: path == current,
        path,
    });
}

fn canonicalize_project_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = Path::new(trimmed);
    if !path.is_dir() {
        eprintln!("[projects] skipping missing path: {trimmed}");
        return None;
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !canonical.join("wiki").is_dir() {
        eprintln!(
            "[projects] skipping path without wiki/: {}",
            canonical.display()
        );
        return None;
    }
    Some(normalize_path(&canonical.to_string_lossy()))
}

fn configured_project_paths_from_env() -> Vec<String> {
    std::env::var("LLM_WIKI_PROJECTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn configured_project_paths(config: &Value) -> Vec<String> {
    let Some(items) = config.get("projects").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for item in items {
        let path = if let Some(s) = item.as_str() {
            s.to_string()
        } else if let Some(obj) = item.as_object() {
            obj.get("path")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_default()
        } else {
            String::new()
        };
        if !path.is_empty() {
            paths.push(path);
        }
    }
    paths
}
