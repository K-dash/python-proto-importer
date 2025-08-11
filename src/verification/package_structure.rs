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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io;
    use tempfile::TempDir;

    #[test]
    fn test_determine_package_structure_legacy_with_parent() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let parent_path = temp_dir.path();
        let out_path = parent_path.join("my_package");
        fs::create_dir(&out_path)?;

        let result = determine_package_structure_legacy(&out_path).unwrap();

        assert_eq!(result.0, parent_path);
        assert_eq!(result.1, "my_package");

        Ok(())
    }

    #[test]
    fn test_determine_package_structure_legacy_no_parent() {
        let root_path = PathBuf::from("/");
        let result = determine_package_structure_legacy(&root_path).unwrap();

        // For root path, there's no parent, so it falls back to using the path itself
        assert_eq!(result.0, root_path);
        assert_eq!(result.1, String::new());
    }

    #[test]
    fn test_determine_package_structure_simple_case() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let parent_path = temp_dir.path();
        let out_path = parent_path.join("simple_package");
        fs::create_dir(&out_path)?;

        let result = determine_package_structure(&out_path).unwrap();

        // Parent has no __init__.py, so should use standard structure
        assert_eq!(result.0, parent_path);
        assert_eq!(result.1, "simple_package");

        Ok(())
    }

    #[test]
    fn test_determine_package_structure_nested_package() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let grandparent_path = temp_dir.path();
        let parent_path = grandparent_path.join("parent_pkg");
        let out_path = parent_path.join("child_pkg");

        fs::create_dir_all(&out_path)?;
        fs::write(parent_path.join("__init__.py"), "")?; // Make parent a package

        let result = determine_package_structure(&out_path).unwrap();

        // Parent has __init__.py, so should use nested structure
        assert_eq!(result.0, grandparent_path);
        assert_eq!(result.1, "parent_pkg.child_pkg");

        Ok(())
    }

    #[test]
    fn test_determine_package_structure_nested_package_no_grandparent() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let parent_path = temp_dir.path().join("parent_pkg");
        let out_path = parent_path.join("child_pkg");

        fs::create_dir_all(&out_path)?;
        fs::write(parent_path.join("__init__.py"), "")?;

        let result = determine_package_structure(&out_path).unwrap();

        // Grandparent exists (temp_dir), so should use nested structure
        assert_eq!(result.0, temp_dir.path());
        assert_eq!(result.1, "parent_pkg.child_pkg");

        Ok(())
    }

    #[test]
    fn test_determine_package_structure_fallback_to_self() {
        let nonexistent_path = PathBuf::from("/nonexistent/path/package");

        let result = determine_package_structure(&nonexistent_path).unwrap();

        // No parent exists, should fallback to using path itself with empty package name
        assert_eq!(result.0, nonexistent_path);
        assert_eq!(result.1, String::new());
    }

    #[test]
    fn test_determine_package_structure_empty_parent_name() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let parent_path = temp_dir.path().join("");
        let out_path = parent_path.join("package");

        // This creates an unusual situation where parent name might be empty
        let result = determine_package_structure(&out_path);

        // Should handle gracefully
        assert!(result.is_ok());

        Ok(())
    }
}
