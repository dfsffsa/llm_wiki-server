use std::fs;
use std::path::{Path, PathBuf};

const TEXT_EXTENSIONS: &[&str] = &[
    "md", "mdx", "txt", "csv", "json", "yaml", "yml", "xml", "html", "htm", "rtf", "log",
];

const OFFICE_EXTENSIONS: &[&str] = &["pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls", "odt", "odp", "ods"];

pub fn run(file: PathBuf, out: Option<PathBuf>, copy_fallback: bool) -> Result<(), String> {
    let file = file
        .canonicalize()
        .map_err(|e| format!("Input file not found: {e}"))?;
    if !file.is_file() {
        return Err(format!("Not a file: {}", file.display()));
    }

    let ext = file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let out = out.unwrap_or_else(|| default_out_path(&file));
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create output dir: {e}"))?;
    }

    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        let text = fs::read_to_string(&file)
            .map_err(|e| format!("Failed to read text file: {e}"))?;
        fs::write(&out, text).map_err(|e| format!("Failed to write output: {e}"))?;
        println!("wrote text to {}", out.display());
        return Ok(());
    }

    if OFFICE_EXTENSIONS.contains(&ext.as_str()) {
        if copy_fallback {
            fs::copy(&file, &out).map_err(|e| format!("Failed to copy file: {e}"))?;
            eprintln!(
                "warning: copied binary as-is; PDF/Office text extraction requires the desktop app or PDFium."
            );
            println!("copied to {}", out.display());
            return Ok(());
        }
        return Err(format!(
            "Cannot extract text from .{ext} in CLI yet. Use the desktop app, convert to .md/.txt, or rerun with --copy-fallback."
        ));
    }

    if copy_fallback {
        fs::copy(&file, &out).map_err(|e| format!("Failed to copy file: {e}"))?;
        println!("copied to {}", out.display());
        return Ok(());
    }

    Err(format!(
        "Unsupported file type '.{ext}'. Use --copy-fallback to copy the file unchanged."
    ))
}

fn default_out_path(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let cache = parent.join(".cache");
    let name = input.file_name().unwrap_or_default().to_string_lossy();
    cache.join(format!("{name}.txt"))
}
