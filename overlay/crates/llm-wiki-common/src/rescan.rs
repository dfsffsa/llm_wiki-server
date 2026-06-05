use std::fs;
use std::path::Path;

use md5::{Digest, Md5};
use serde::Serialize;
use walkdir::WalkDir;

use crate::project::sources_dir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFileEntry {
    pub path: String,
    pub size: u64,
    pub md5: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RescanReport {
    pub project: String,
    pub sources_root: String,
    pub files: Vec<SourceFileEntry>,
    pub total_files: usize,
    pub total_bytes: u64,
}

pub fn rescan_project(project: &Path) -> Result<RescanReport, String> {
    let root = sources_dir(project);
    if !root.is_dir() {
        return Ok(RescanReport {
            project: project.to_string_lossy().replace('\\', "/"),
            sources_root: root.to_string_lossy().replace('\\', "/"),
            files: Vec::new(),
            total_files: 0,
            total_bytes: 0,
        });
    }

    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let meta = entry
            .metadata()
            .map_err(|e| format!("Failed to read metadata: {e}"))?;
        let size = meta.len();
        total_bytes += size;
        let rel = entry
            .path()
            .strip_prefix(project)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| entry.path().to_string_lossy().replace('\\', "/"));
        files.push(SourceFileEntry {
            path: rel,
            size,
            md5: file_md5(entry.path())?,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let total_files = files.len();
    Ok(RescanReport {
        project: project.to_string_lossy().replace('\\', "/"),
        sources_root: root.to_string_lossy().replace('\\', "/"),
        files,
        total_files,
        total_bytes,
    })
}

fn file_md5(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let digest = Md5::digest(bytes);
    Ok(format!("{:x}", digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rescan_finds_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let sources = dir.path().join("raw/sources");
        fs::create_dir_all(&sources).unwrap();
        let file = sources.join("note.md");
        let mut f = fs::File::create(&file).unwrap();
        writeln!(f, "hello").unwrap();
        let report = rescan_project(dir.path()).unwrap();
        assert_eq!(report.total_files, 1);
        assert!(report.files[0].path.ends_with("raw/sources/note.md"));
    }
}
