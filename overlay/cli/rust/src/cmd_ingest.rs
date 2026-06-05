use std::path::PathBuf;
use std::process::Command;

use llm_wiki_common::project::resolve_project_dir;

use crate::config::{default_config_path, load_config};

pub fn run(file: PathBuf, project: PathBuf, config: Option<PathBuf>) -> Result<(), String> {
    let project = resolve_project_dir(project.to_string_lossy().as_ref())?;
    let file = file
        .canonicalize()
        .map_err(|e| format!("Source file not found: {e}"))?;

    let config_path = config
        .or_else(default_config_path)
        .ok_or_else(|| "Config required for ingest (--config or LLM_WIKI_CONFIG)".to_string())?;
    load_config(&config_path)?;

    let repo_root = repo_root();
    let node_dir = repo_root.join("overlay/cli/node");
    let script = node_dir.join("src/cmd-ingest.ts");
    if !script.is_file() {
        return Err(format!("Node ingest script not found: {}", script.display()));
    }

    let status = Command::new("npx")
        .arg("tsx")
        .arg(&script)
        .arg("--project")
        .arg(&project)
        .arg("--source")
        .arg(&file)
        .arg("--config")
        .arg(&config_path)
        .env("LLM_WIKI_BIN", std::env::current_exe().unwrap_or_default())
        .env("LLM_WIKI_REPO", &repo_root)
        .current_dir(&node_dir)
        .status()
        .map_err(|e| format!("Failed to run Node ingest (is Node/npx installed?): {e}"))?;

    if !status.success() {
        return Err(format!("Ingest exited with status {status}"));
    }
    Ok(())
}

fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("LLM_WIKI_REPO") {
        if !root.is_empty() {
            return PathBuf::from(root);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}
