use std::path::PathBuf;
use std::process::Command;

use llm_wiki_common::project::{resolve_project_dir, wiki_dir};
use walkdir::WalkDir;

use crate::config::{default_config_path, load_config, resolve_config_path};

pub async fn run(project: PathBuf, vectors: bool, config: Option<PathBuf>) -> Result<(), String> {
    let project = resolve_project_dir(project.to_string_lossy().as_ref())?;
    let wiki = wiki_dir(&project);
    let mut md_count = 0usize;
    if wiki.is_dir() {
        for entry in WalkDir::new(&wiki).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("md")
            {
                md_count += 1;
            }
        }
    }

    if !vectors {
        println!("wiki markdown files: {md_count}");
        println!("Run with --vectors to rebuild LanceDB embeddings (requires Node + config).");
        return Ok(());
    }

    let config_path = config
        .or_else(default_config_path)
        .ok_or_else(|| "Config required for vector reindex (--config or LLM_WIKI_CONFIG)".to_string())?;
    let config_path = resolve_config_path(config_path)?;
    let _cfg = load_config(&config_path)?;

    let repo_root = repo_root();
    let node_dir = repo_root.join("overlay/cli/node");
    let script = node_dir.join("src/cmd-reindex.ts");
    if !script.is_file() {
        return Err(format!("Node reindex script not found: {}", script.display()));
    }

    let status = Command::new("npx")
        .arg("tsx")
        .arg(&script)
        .arg("--project")
        .arg(&project)
        .arg("--config")
        .arg(&config_path)
        .env("LLM_WIKI_BIN", std::env::current_exe().unwrap_or_default())
        .current_dir(&node_dir)
        .status()
        .map_err(|e| format!("Failed to run Node reindex: {e}"))?;

    if !status.success() {
        return Err(format!("Node reindex exited with status {status}"));
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
