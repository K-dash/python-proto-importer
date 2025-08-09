use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// Very small scaffold for future import rewriting.
/// For now, it only identifies candidate lines and returns count.
pub fn rewrite_file_for_relative_imports(path: &Path) -> Result<usize> {
    let content = fs::read_to_string(path)?;
    let import_re = Regex::new(r"(?m)^import\s+([A-Za-z0-9_\.]+_pb2(?:_grpc)?)\b").unwrap();
    let from_re = Regex::new(r"(?m)^from\s+([A-Za-z0-9_\.]+)\s+import\s+([A-Za-z0-9_]+_pb2(?:_grpc)?)\b").unwrap();

    let mut hits = 0usize;
    hits += import_re.find_iter(&content).count();
    hits += from_re.find_iter(&content).count();

    // No modifications yet; further phases will compute and apply rewrites.
    Ok(hits)
}

/// Walk output tree and report count of candidate files/lines (dry-run).
pub fn scan_and_report(root: &Path) -> Result<(usize, usize)> {
    let mut files = 0usize;
    let mut lines = 0usize;
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("py") {
            files += 1;
            lines += rewrite_file_for_relative_imports(p)?;
        }
    }
    Ok((files, lines))
}
