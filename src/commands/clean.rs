use crate::config::AppConfig;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

/// Execute the clean command to remove the generated output directory.
///
/// This command removes the entire output directory and all its contents, as
/// specified in the pyproject.toml configuration. It provides a safety mechanism
/// by requiring explicit confirmation to prevent accidental deletion.
///
/// # Arguments
///
/// * `pyproject` - Optional path to the pyproject.toml file. If None, uses "pyproject.toml"
/// * `yes` - Safety flag that must be true to actually perform the deletion
///
/// # Returns
///
/// Returns `Ok(())` if the operation completes successfully, or an error if:
/// - Configuration cannot be loaded
/// - The safety flag (`yes`) is false when the directory exists
/// - Directory removal fails due to permissions or other filesystem issues
///
/// # Safety Features
///
/// - **Confirmation Required**: Refuses to delete without explicit `yes` flag
/// - **No-op for Missing**: Succeeds silently if the output directory doesn't exist
/// - **Complete Removal**: Recursively removes all files and subdirectories
///
/// # Use Cases
///
/// - **Fresh Start**: Clear all generated code before regeneration
/// - **CI Cleanup**: Ensure clean environment between builds
/// - **Development**: Reset state during iteration
///
/// # Example
///
/// ```no_run
/// use python_proto_importer::commands::clean;
///
/// // Safe call - will refuse to delete without confirmation
/// let result = clean(None, false);
/// assert!(result.is_err()); // Expects error without --yes
///
/// // Actual deletion with confirmation
/// clean(None, true)?; // Removes the configured output directory
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn clean(pyproject: Option<&str>, yes: bool) -> Result<()> {
    let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
    let out = &cfg.out;
    if out.exists() {
        if !yes {
            bail!("refusing to remove {} without --yes", out.display());
        }
        tracing::info!("removing {}", out.display());
        fs::remove_dir_all(out).with_context(|| format!("failed to remove {}", out.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_config_file(dir: &Path, out_dir: &str) -> Result<String> {
        let config_file = dir.join("pyproject.toml");
        let mut file = fs::File::create(&config_file)?;
        writeln!(file, "[tool.python_proto_importer]")?;
        writeln!(file, "out = \"{}\"", out_dir)?;
        writeln!(file, "proto_path = [\"proto\"]")?;
        writeln!(file, "python_exe = \"python3\"")?;
        Ok(config_file.to_string_lossy().to_string())
    }

    #[test]
    fn test_clean_without_yes_flag() {
        let temp_dir = TempDir::new().unwrap();
        let out_dir = temp_dir.path().join("output");
        fs::create_dir(&out_dir).unwrap();

        let config_file =
            create_test_config_file(temp_dir.path(), &out_dir.to_string_lossy()).unwrap();

        let result = clean(Some(&config_file), false);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("refusing to remove")
        );
        assert!(out_dir.exists()); // Directory should still exist
    }

    #[test]
    fn test_clean_with_yes_flag() {
        let temp_dir = TempDir::new().unwrap();
        let out_dir = temp_dir.path().join("output");
        fs::create_dir(&out_dir).unwrap();

        let config_file =
            create_test_config_file(temp_dir.path(), &out_dir.to_string_lossy()).unwrap();

        let result = clean(Some(&config_file), true);

        assert!(result.is_ok());
        assert!(!out_dir.exists()); // Directory should be removed
    }

    #[test]
    fn test_clean_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let out_dir = temp_dir.path().join("nonexistent");

        let config_file =
            create_test_config_file(temp_dir.path(), &out_dir.to_string_lossy()).unwrap();

        let result = clean(Some(&config_file), true);

        // Should succeed even if directory doesn't exist
        assert!(result.is_ok());
    }

    #[test]
    fn test_clean_with_files_in_directory() {
        let temp_dir = TempDir::new().unwrap();
        let out_dir = temp_dir.path().join("output");
        fs::create_dir(&out_dir).unwrap();

        // Create some files in the output directory
        fs::write(out_dir.join("test_file.py"), "# test content").unwrap();
        let subdir = out_dir.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("another_file.py"), "# more content").unwrap();

        let config_file =
            create_test_config_file(temp_dir.path(), &out_dir.to_string_lossy()).unwrap();

        let result = clean(Some(&config_file), true);

        assert!(result.is_ok());
        assert!(!out_dir.exists()); // Directory and all contents should be removed
    }
}
