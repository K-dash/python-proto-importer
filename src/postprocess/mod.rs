use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub mod rel_imports;

pub fn create_packages(root: &Path) -> Result<usize> {
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            dirs.insert(path.to_path_buf());
        }
    }

    let mut created = 0usize;
    for dir in dirs {
        let init_py = dir.join("__init__.py");
        if !init_py.exists() {
            fs::write(&init_py, b"")
                .with_context(|| format!("failed to write {}", init_py.display()))?;
            created += 1;
        }
    }
    Ok(created)
}
