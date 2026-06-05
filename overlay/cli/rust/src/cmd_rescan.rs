use std::path::PathBuf;

use llm_wiki_common::project::resolve_project_dir;
use llm_wiki_common::rescan::{rescan_project, RescanReport};

pub fn run(project: PathBuf, json: bool) -> Result<(), String> {
    let project = resolve_project_dir(project.to_string_lossy().as_ref())?;
    let report = rescan_project(&project)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    print_report(&report);
    Ok(())
}

fn print_report(report: &RescanReport) {
    println!("project: {}", report.project);
    println!("sources: {} ({} files, {} bytes)", report.sources_root, report.total_files, report.total_bytes);
    for file in &report.files {
        println!("  {}  {} bytes  md5={}", file.path, file.size, file.md5);
    }
}
