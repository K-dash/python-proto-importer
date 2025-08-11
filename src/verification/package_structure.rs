use anyhow::Result;
use std::path::{Path, PathBuf};

/// Legacy package structure determination
/// Simply uses parent as PYTHONPATH and out_name as package_name
pub fn determine_package_structure_legacy(out_abs: &Path) -> Result<(PathBuf, String)> {
    let out_name = out_abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("generated");

    tracing::debug!(
        "determine_package_structure_legacy: using simple structure: PYTHONPATH={}, package_name={}",
        out_abs
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        out_name
    );

    let parent = out_abs.parent();
    if let Some(parent_dir) = parent.filter(|p| p.exists()) {
        return Ok((parent_dir.to_path_buf(), out_name.to_string()));
    }

    Ok((out_abs.to_path_buf(), String::new()))
}

/// Intelligent package structure determination
/// Prefers PYTHONPATH to point at the directory which contains the "package root".
/// If the parent of `out_abs` is a package (has __init__.py), use its parent as
/// PYTHONPATH and set package_name to "{parent}.{out}". Otherwise use the parent
/// as PYTHONPATH and package_name to `out`.
pub fn determine_package_structure(out_abs: &Path) -> Result<(PathBuf, String)> {
    let out_name = out_abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("generated");

    tracing::debug!(
        "determine_package_structure: analyzing out_abs={}",
        out_abs.display()
    );
    tracing::debug!("determine_package_structure: out_name={}", out_name);

    if let Some(parent_dir) = out_abs.parent() {
        tracing::debug!(
            "determine_package_structure: parent_dir={}",
            parent_dir.display()
        );
        if parent_dir.exists() {
            let parent_init = parent_dir.join("__init__.py");
            tracing::debug!(
                "determine_package_structure: checking for parent_init={}",
                parent_init.display()
            );
            if parent_init.exists() {
                tracing::debug!(
                    "determine_package_structure: parent is a package (has __init__.py)"
                );
                if let Some(grand) = parent_dir.parent() {
                    tracing::debug!(
                        "determine_package_structure: grandparent_dir={}",
                        grand.display()
                    );
                    if grand.exists() {
                        let parent_name = parent_dir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("");
                        let pkg = if parent_name.is_empty() {
                            out_name.to_string()
                        } else {
                            format!("{}.{}", parent_name, out_name)
                        };
                        tracing::debug!(
                            "determine_package_structure: using nested package structure: PYTHONPATH={}, package_name={}",
                            grand.display(),
                            pkg
                        );
                        return Ok((grand.to_path_buf(), pkg));
                    } else {
                        tracing::debug!(
                            "determine_package_structure: grandparent does not exist, falling back to standard structure"
                        );
                    }
                } else {
                    tracing::debug!(
                        "determine_package_structure: no grandparent, falling back to standard structure"
                    );
                }
            } else {
                tracing::debug!(
                    "determine_package_structure: parent is not a package (no __init__.py)"
                );
            }
            tracing::debug!(
                "determine_package_structure: using standard structure: PYTHONPATH={}, package_name={}",
                parent_dir.display(),
                out_name
            );
            return Ok((parent_dir.to_path_buf(), out_name.to_string()));
        } else {
            tracing::debug!("determine_package_structure: parent directory does not exist");
        }
    } else {
        tracing::debug!("determine_package_structure: no parent directory");
    }

    tracing::debug!(
        "determine_package_structure: fallback to out_abs as PYTHONPATH: PYTHONPATH={}, package_name=empty",
        out_abs.display()
    );
    Ok((out_abs.to_path_buf(), String::new()))
}
