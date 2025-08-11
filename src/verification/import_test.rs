use crate::config::AppConfig;
use crate::utils::run_cmd;
use crate::verification::{
    create_import_test_script, determine_package_structure, determine_package_structure_legacy,
};
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::Path;
use walkdir::WalkDir;

/// Run comprehensive import verification for generated Python modules
pub fn verify(cfg: &AppConfig) -> Result<()> {
    let out_abs = cfg.out.canonicalize().unwrap_or_else(|_| cfg.out.clone());
    let mut modules: Vec<String> = Vec::new();
    let py_suffixes: Vec<&str> = cfg
        .postprocess
        .module_suffixes
        .iter()
        .filter_map(|s| {
            if s.ends_with(".py") {
                Some(s.as_str())
            } else {
                None
            }
        })
        .collect();

    for entry in WalkDir::new(&out_abs).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file()
            && path.extension() == Some(OsStr::new("py"))
            && path.file_name() != Some(OsStr::new("__init__.py"))
        {
            let rel = path.strip_prefix(&out_abs).unwrap_or(path);
            let rel_str = rel.to_string_lossy();
            if !py_suffixes.is_empty() && !py_suffixes.iter().any(|s| rel_str.ends_with(s)) {
                continue;
            }
            let rel_no_ext = rel.with_extension("");
            let mut parts: Vec<String> = Vec::new();
            for comp in rel_no_ext.components() {
                if let std::path::Component::Normal(os) = comp {
                    parts.push(os.to_string_lossy().to_string());
                }
            }
            if !parts.is_empty() {
                modules.push(parts.join("."));
            }
        }
    }

    modules.sort();

    if modules.is_empty() {
        tracing::info!("no python modules found for verification");
    } else {
        let (parent_path, package_name) = determine_package_structure(&out_abs)?;

        tracing::debug!(
            "using parent_path={}, package_name={}",
            parent_path.display(),
            package_name
        );

        let test_script = create_import_test_script(&package_name, &modules);

        // In debug mode, save the test script to a temporary file for inspection
        if tracing::enabled!(tracing::Level::DEBUG)
            && let Ok(temp_dir) = std::env::temp_dir().canonicalize()
        {
            let script_path = temp_dir.join(format!(
                "python_proto_importer_test_{}.py",
                std::process::id()
            ));
            if let Err(e) = std::fs::write(&script_path, &test_script) {
                tracing::debug!(
                    "failed to write debug script to {}: {}",
                    script_path.display(),
                    e
                );
            } else {
                tracing::debug!(
                    "comprehensive test script saved to: {}",
                    script_path.display()
                );
            }
        }

        let mut cmd = std::process::Command::new(&cfg.python_exe);
        if cfg.python_exe == "uv" {
            cmd.arg("run").arg("python").arg("-c").arg(&test_script);
        } else {
            cmd.arg("-c").arg(&test_script);
        }

        let output = cmd
            .env("PYTHONPATH", &parent_path)
            .output()
            .with_context(|| {
                format!(
                    "failed running {} for package-aware import dry-run",
                    cfg.python_exe
                )
            })?;

        let stderr_output = String::from_utf8_lossy(&output.stderr);
        for line in stderr_output.lines() {
            if line.starts_with("IMPORT_TEST_SUMMARY:") {
                tracing::debug!(
                    "{}",
                    line.strip_prefix("IMPORT_TEST_SUMMARY:").unwrap_or(line)
                );
            } else if line.starts_with("IMPORT_TEST_SUCCESS:") {
                tracing::debug!(
                    "comprehensive import test: {}",
                    line.strip_prefix("IMPORT_TEST_SUCCESS:")
                        .unwrap_or("success")
                );
            } else if line.starts_with("IMPORT_ERROR:") {
                tracing::warn!(
                    "import issue detected: {}",
                    line.strip_prefix("IMPORT_ERROR:").unwrap_or(line)
                );
            }
        }

        if !output.status.success() {
            tracing::warn!(
                "comprehensive import test failed, running individual fallback tests for detailed diagnosis"
            );
            let failed_modules =
                run_individual_fallback_tests(cfg, &parent_path, &package_name, &modules)?;
            if !failed_modules.is_empty() {
                // Try legacy package structure determination as a fallback
                tracing::warn!("retrying with legacy package structure determination...");
                let (legacy_parent_path, legacy_package_name) =
                    determine_package_structure_legacy(&out_abs)?;

                if legacy_parent_path != parent_path || legacy_package_name != package_name {
                    tracing::debug!(
                        "legacy fallback: parent_path={}, package_name={}",
                        legacy_parent_path.display(),
                        legacy_package_name
                    );
                    let legacy_failed_modules = run_individual_fallback_tests(
                        cfg,
                        &legacy_parent_path,
                        &legacy_package_name,
                        &modules,
                    )?;

                    if legacy_failed_modules.is_empty() {
                        tracing::info!(
                            "import dry-run passed with legacy package structure ({} modules)",
                            modules.len()
                        );
                    } else if legacy_failed_modules.len() < failed_modules.len() {
                        tracing::warn!(
                            "legacy fallback reduced failures from {} to {} modules",
                            failed_modules.len(),
                            legacy_failed_modules.len()
                        );
                        for (m, error) in &legacy_failed_modules {
                            tracing::error!(module=%m, "import failed (legacy fallback): {}", error);
                        }
                        anyhow::bail!(
                            "import dry-run failed for {} modules (out of {}) even with legacy fallback. Use -v for more details.",
                            legacy_failed_modules.len(),
                            modules.len()
                        );
                    } else {
                        tracing::warn!(
                            "legacy fallback did not improve results, showing original errors"
                        );
                        for (m, error) in &failed_modules {
                            tracing::error!(module=%m, "import failed: {}", error);
                        }
                        anyhow::bail!(
                            "import dry-run failed for {} modules (out of {}). Use -v for more details.",
                            failed_modules.len(),
                            modules.len()
                        );
                    }
                } else {
                    tracing::debug!("legacy fallback would use same configuration, skipping");
                    for (m, error) in &failed_modules {
                        tracing::error!(module=%m, "import failed: {}", error);
                    }
                    anyhow::bail!(
                        "import dry-run failed for {} modules (out of {}). Use -v for more details.",
                        failed_modules.len(),
                        modules.len()
                    );
                }
            }
            tracing::warn!(
                "comprehensive test failed but individual tests passed - this may indicate a package structure issue"
            );
        }

        tracing::info!("import dry-run passed ({} modules)", modules.len());
    }

    if let Some(v) = &cfg.verify {
        if let Some(cmd) = v.mypy_cmd.as_deref().filter(|cmd| !cmd.is_empty()) {
            run_cmd(cmd).context("mypy_cmd failed")?;
        }
        if let Some(cmd) = v.pyright_cmd.as_deref().filter(|cmd| !cmd.is_empty()) {
            run_cmd(cmd).context("pyright_cmd failed")?;
        }
    }
    Ok(())
}

/// Run individual fallback tests for each module to provide detailed diagnosis
fn run_individual_fallback_tests(
    cfg: &AppConfig,
    parent_path: &Path,
    package_name: &str,
    modules: &[String],
) -> Result<Vec<(String, String)>> {
    let mut failed = Vec::new();

    tracing::debug!(
        "running individual fallback tests for {} modules",
        modules.len()
    );
    tracing::debug!(
        "environment: PYTHONPATH={}, package_name={}, python_exe={}",
        parent_path.display(),
        package_name,
        cfg.python_exe
    );

    for (idx, module) in modules.iter().enumerate() {
        let full_module = if package_name.is_empty() {
            module.clone()
        } else {
            format!("{}.{}", package_name, module)
        };

        tracing::trace!(
            "testing individual module ({}/{}): {}",
            idx + 1,
            modules.len(),
            full_module
        );

        let test_script = format!(
            r#"
import sys
import importlib
import traceback

module_name = '{}'
full_module_name = '{}'

try:
    mod = importlib.import_module(full_module_name)
    print('SUCCESS:' + module_name, file=sys.stderr)
except ImportError as e:
    error_msg = str(e)
    if "relative import" in error_msg.lower():
        print('RELATIVE_IMPORT_ERROR:' + module_name + ':' + error_msg, file=sys.stderr)
    else:
        print('IMPORT_ERROR:' + module_name + ':' + error_msg, file=sys.stderr)
except ModuleNotFoundError as e:
    print('MODULE_NOT_FOUND_ERROR:' + module_name + ':' + str(e), file=sys.stderr)
except SyntaxError as e:
    print('SYNTAX_ERROR:' + module_name + ':line ' + str(e.lineno or '?') + ': ' + str(e), file=sys.stderr)
except Exception as e:
    print('GENERAL_ERROR:' + module_name + ':' + type(e).__name__ + ': ' + str(e), file=sys.stderr)
    traceback.print_exc(file=sys.stderr)
"#,
            module, full_module
        );

        // In debug mode, save individual test scripts to temporary files for inspection
        if tracing::enabled!(tracing::Level::TRACE)
            && let Ok(temp_dir) = std::env::temp_dir().canonicalize()
        {
            let script_path = temp_dir.join(format!(
                "python_proto_importer_individual_{}_{}.py",
                std::process::id(),
                idx
            ));
            if let Err(e) = std::fs::write(&script_path, &test_script) {
                tracing::trace!(
                    "failed to write debug script to {}: {}",
                    script_path.display(),
                    e
                );
            } else {
                tracing::trace!("individual test script saved to: {}", script_path.display());
            }
        }

        let mut cmd = std::process::Command::new(&cfg.python_exe);
        if cfg.python_exe == "uv" {
            cmd.arg("run").arg("python").arg("-c").arg(&test_script);
        } else {
            cmd.arg("-c").arg(&test_script);
        }

        let output = cmd
            .env("PYTHONPATH", parent_path)
            .output()
            .with_context(|| {
                format!(
                    "failed running {} for individual fallback test",
                    cfg.python_exe
                )
            })?;

        if !output.status.success() {
            let stderr_output = String::from_utf8_lossy(&output.stderr);
            let stdout_output = String::from_utf8_lossy(&output.stdout);
            let mut error_msg = String::new();

            // Debug output of full stderr and stdout in verbose mode
            if tracing::enabled!(tracing::Level::DEBUG) {
                tracing::debug!("individual test failed for module {}", module);
                tracing::debug!("exit code: {:?}", output.status.code());
                if !stderr_output.trim().is_empty() {
                    tracing::debug!("stderr:\n{}", stderr_output);
                }
                if !stdout_output.trim().is_empty() {
                    tracing::debug!("stdout:\n{}", stdout_output);
                }
            }

            // Parse stderr for known error patterns
            for line in stderr_output.lines() {
                if line.starts_with("RELATIVE_IMPORT_ERROR:") {
                    error_msg = format!(
                        "Relative import issue: {}",
                        line.strip_prefix("RELATIVE_IMPORT_ERROR:").unwrap_or(line)
                    );
                    break;
                } else if line.starts_with("IMPORT_ERROR:") {
                    error_msg = format!(
                        "Import error: {}",
                        line.strip_prefix("IMPORT_ERROR:").unwrap_or(line)
                    );
                    break;
                } else if line.starts_with("MODULE_NOT_FOUND_ERROR:") {
                    error_msg = format!(
                        "Module not found: {}",
                        line.strip_prefix("MODULE_NOT_FOUND_ERROR:").unwrap_or(line)
                    );
                    break;
                } else if line.starts_with("SYNTAX_ERROR:") {
                    error_msg = format!(
                        "Syntax error: {}",
                        line.strip_prefix("SYNTAX_ERROR:").unwrap_or(line)
                    );
                    break;
                } else if line.starts_with("GENERAL_ERROR:") {
                    error_msg = format!(
                        "General error: {}",
                        line.strip_prefix("GENERAL_ERROR:").unwrap_or(line)
                    );
                    break;
                }
                // Also check for common Python error patterns in stderr
                else if line.contains("ImportError:") {
                    error_msg = format!("ImportError found in stderr: {}", line.trim());
                    break;
                } else if line.contains("ModuleNotFoundError:") {
                    error_msg = format!("ModuleNotFoundError found in stderr: {}", line.trim());
                    break;
                } else if line.contains("SyntaxError:") {
                    error_msg = format!("SyntaxError found in stderr: {}", line.trim());
                    break;
                } else if line.contains("NameError:") {
                    error_msg = format!("NameError found in stderr: {}", line.trim());
                    break;
                }
            }

            // If no error pattern found in stderr, check stdout
            if error_msg.is_empty() {
                for line in stdout_output.lines() {
                    if line.contains("ImportError:") {
                        error_msg = format!("ImportError found in stdout: {}", line.trim());
                        break;
                    } else if line.contains("ModuleNotFoundError:") {
                        error_msg = format!("ModuleNotFoundError found in stdout: {}", line.trim());
                        break;
                    } else if line.contains("SyntaxError:") {
                        error_msg = format!("SyntaxError found in stdout: {}", line.trim());
                        break;
                    } else if line.contains("Traceback (most recent call last):") {
                        error_msg = format!("Python traceback found in stdout: {}", line.trim());
                        break;
                    }
                }
            }

            // If still no specific error found, provide more detailed information
            if error_msg.is_empty() {
                let detailed_info =
                    if !stderr_output.trim().is_empty() || !stdout_output.trim().is_empty() {
                        let stderr_preview =
                            stderr_output.lines().take(2).collect::<Vec<_>>().join("; ");
                        let stdout_preview =
                            stdout_output.lines().take(2).collect::<Vec<_>>().join("; ");
                        format!(
                            "Unknown error (exit code: {}) - stderr: '{}' - stdout: '{}'",
                            output.status.code().unwrap_or(-1),
                            stderr_preview.trim(),
                            stdout_preview.trim()
                        )
                    } else {
                        format!(
                            "Unknown error (exit code: {}) - no output",
                            output.status.code().unwrap_or(-1)
                        )
                    };
                error_msg = detailed_info;
            }

            failed.push((module.clone(), error_msg));
        } else {
            tracing::trace!("individual test passed: {}", module);
        }
    }

    tracing::debug!(
        "individual fallback tests completed: {}/{} failed",
        failed.len(),
        modules.len()
    );
    Ok(failed)
}
