use crate::config::AppConfig;
use crate::verification::import_test::verify;
use anyhow::{Context, Result};
use std::path::Path;

/// Execute the check command to verify existing generated Python code.
///
/// This command runs only the verification phase without regenerating any code.
/// It loads the configuration from pyproject.toml and performs the same validation
/// checks that are run at the end of the build process.
///
/// # Arguments
///
/// * `pyproject` - Optional path to the pyproject.toml file. If None, uses "pyproject.toml"
///
/// # Returns
///
/// Returns `Ok(())` if all verification checks pass, or an error if:
/// - Configuration cannot be loaded
/// - Import tests fail
/// - Type checking fails (if configured)
///
/// # Verification Steps
///
/// 1. **Configuration Loading**: Reads and validates pyproject.toml settings
/// 2. **Import Validation**: Attempts to import every generated Python module
/// 3. **Type Checking**: Runs configured type checkers (mypy/pyright) if specified
///
/// # Use Cases
///
/// - **CI Integration**: Verify generated code without regenerating
/// - **Development**: Quick validation after manual changes
/// - **Debugging**: Isolate verification issues from generation issues
///
/// # Example
///
/// ```no_run
/// use python_proto_importer::commands::check;
///
/// // Check with default pyproject.toml
/// check(None)?;
///
/// // Check with custom config file
/// check(Some("custom.toml"))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn check(pyproject: Option<&str>) -> Result<()> {
    let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
    verify(&cfg)
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
    fn test_check_invalid_config() {
        let result = check(Some("nonexistent_config.toml"));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to load config")
        );
    }

    #[test]
    fn test_check_with_empty_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let out_dir = temp_dir.path().join("empty_output");
        fs::create_dir(&out_dir).unwrap();

        let config_file =
            create_test_config_file(temp_dir.path(), &out_dir.to_string_lossy()).unwrap();

        // This should succeed because verify() handles empty directories gracefully
        let result = check(Some(&config_file));
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_nonexistent_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let out_dir = temp_dir.path().join("nonexistent_output");

        let config_file =
            create_test_config_file(temp_dir.path(), &out_dir.to_string_lossy()).unwrap();

        // verify() should handle nonexistent output directory gracefully
        let result = check(Some(&config_file));
        assert!(result.is_ok());
    }
}
