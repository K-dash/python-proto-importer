use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod config;
mod generator {
    pub mod protoc;
}
mod postprocess;

#[derive(Parser, Debug)]
#[command(
    name = "proto-importer",
    version,
    about = "Python proto importer toolkit"
)]
struct Cli {
    /// Increase verbosity (-v, -vv). Uses RUST_LOG under the hood
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Environment diagnostics (protoc / buf / grpc_tools / mypy / pyright etc.)
    Doctor,
    /// Generate + postprocess + verify
    Build {
        /// Path to pyproject.toml (defaults to current directory)
        #[arg(long)]
        pyproject: Option<String>,
        /// Skip verification after generation
        #[arg(long)]
        no_verify: bool,
        /// Skip generation, only postprocess and verify (future extension)
        #[arg(long)]
        postprocess_only: bool,
    },
    /// Only verify imports/mypy/pyright without generation
    Check {
        #[arg(long)]
        pyproject: Option<String>,
    },
    /// Clean output and cache directories
    Clean {
        #[arg(long)]
        pyproject: Option<String>,
        /// Delete without confirmation
        #[arg(long)]
        yes: bool,
    },
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| level.to_string());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(env_filter))
        .with_target(false)
        .without_time()
        .init();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Commands::Doctor => doctor::run()?,
        Commands::Build {
            pyproject,
            no_verify,
            postprocess_only,
        } => commands::build(pyproject.as_deref(), no_verify, postprocess_only)?,
        Commands::Check { pyproject } => commands::check(pyproject.as_deref())?,
        Commands::Clean { pyproject, yes } => commands::clean(pyproject.as_deref(), yes)?,
    }

    Ok(())
}

mod commands {
    use super::config::{AppConfig, Backend};
    use super::generator::protoc::ProtocRunner;
    use super::postprocess::add_pyright_header;
    use super::postprocess::apply::apply_rewrites_in_tree;
    use super::postprocess::create_packages;
    use super::postprocess::fds::{collect_generated_basenames_from_bytes, load_fds_from_bytes};
    use super::postprocess::rel_imports::scan_and_report;
    use anyhow::{Context, Result, bail};
    use std::fs;
    use std::path::{Path, PathBuf};

    pub fn build(pyproject: Option<&str>, no_verify: bool, _postprocess_only: bool) -> Result<()> {
        let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
        tracing::info!(?cfg.backend, out=%cfg.out.display(), "build start");

        let allowed_basenames = match cfg.backend {
            Backend::Protoc => {
                let runner = ProtocRunner::new(&cfg);
                let fds_bytes = runner.generate()?;
                let _pool = load_fds_from_bytes(&fds_bytes).context("decode FDS failed")?;
                Some(
                    collect_generated_basenames_from_bytes(&fds_bytes)
                        .context("collect basenames from FDS failed")?,
                )
            }
            Backend::Buf => {
                // To be implemented in v0.2 (FDS collection not supported)
                tracing::warn!("buf backend is not implemented yet");
                None
            }
        };

        // Generate __init__.py files (opt-in/default ON)
        if cfg.postprocess.create_package {
            let created = create_packages(&cfg.out)?;
            tracing::info!("created __init__.py: {}", created);
        }

        // Dry-run for relative import candidates (count only)
        let (files, hits) =
            scan_and_report(&cfg.out).context("scan relative-import candidates failed")?;
        tracing::info!(
            "relative-import candidates: files={}, lines={}",
            files,
            hits
        );

        // Apply minimal relativization if enabled (.py/.pyi)
        if cfg.postprocess.relative_imports {
            let modified = apply_rewrites_in_tree(
                &cfg.out,
                cfg.postprocess.exclude_google,
                &cfg.postprocess.module_suffixes,
                allowed_basenames.as_ref(),
            )
            .context("apply relative-import rewrites failed")?;
            tracing::info!(
                "relative-import rewrites applied: {} files modified",
                modified
            );
        }

        // Optional: add pyright header to generated .py files
        if cfg.postprocess.pyright_header {
            let added = add_pyright_header(&cfg.out)?;
            if added > 0 {
                tracing::info!("pyright header added: {} files", added);
            }
        }

        if !no_verify {
            verify(&cfg)?;
        }
        Ok(())
    }

    pub fn check(pyproject: Option<&str>) -> Result<()> {
        let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
        verify(&cfg)
    }

    pub fn clean(pyproject: Option<&str>, yes: bool) -> Result<()> {
        let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
        let out = &cfg.out;
        if out.exists() {
            if !yes {
                bail!("refusing to remove {} without --yes", out.display());
            }
            tracing::info!("removing {}", out.display());
            fs::remove_dir_all(out)
                .with_context(|| format!("failed to remove {}", out.display()))?;
        }
        Ok(())
    }

    fn verify(cfg: &AppConfig) -> Result<()> {
        use std::ffi::OsStr;
        use walkdir::WalkDir;

        // 1) import dry-run for generated python modules using package-aware testing
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

        // Collect modules as before
        for entry in WalkDir::new(&out_abs).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file()
                && path.extension() == Some(OsStr::new("py"))
                && path.file_name() != Some(OsStr::new("__init__.py"))
            {
                let rel = path.strip_prefix(&out_abs).unwrap_or(path);
                let rel_str = rel.to_string_lossy();
                // must match known module suffixes (e.g. _pb2.py/_pb2_grpc.py)
                if !py_suffixes.is_empty() && !py_suffixes.iter().any(|s| rel_str.ends_with(s)) {
                    continue;
                }
                // Build module name from path components to be OS-agnostic
                // 1) remove extension
                let rel_no_ext = rel.with_extension("");
                // 2) join normal components with '.'
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

        // Keep it deterministic
        modules.sort();

        if modules.is_empty() {
            tracing::info!("no python modules found for verification");
        } else {
            // Package-aware import testing: determine parent path and package name
            let (parent_path, package_name) = determine_package_structure(&out_abs)?;

            tracing::debug!(
                "using parent_path={}, package_name={}",
                parent_path.display(),
                package_name
            );

            // Create comprehensive test script for all modules
            let test_script = create_import_test_script(&package_name, &modules);

            // Run single comprehensive test with detailed output capture
            let mut cmd = std::process::Command::new(&cfg.python_exe);

            // Handle uv-specific command structure
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

            // Parse output for detailed logging
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
                // If comprehensive test fails, run individual tests for detailed reporting
                let failed_modules =
                    run_individual_fallback_tests(cfg, &parent_path, &package_name, &modules)?;
                if !failed_modules.is_empty() {
                    for (m, error) in &failed_modules {
                        tracing::error!(module=%m, "import failed: {}", error);
                    }
                    anyhow::bail!(
                        "import dry-run failed for {} modules (out of {}). Use -v for more details.",
                        failed_modules.len(),
                        modules.len()
                    );
                }
                // If fallback tests passed but comprehensive test failed, it might be a different issue
                tracing::warn!(
                    "comprehensive test failed but individual tests passed - this may indicate a package structure issue"
                );
            }

            tracing::info!("import dry-run passed ({} modules)", modules.len());
        }

        // 2) optional type check commands
        if let Some(v) = &cfg.verify {
            if let Some(cmd) = &v.mypy_cmd {
                if !cmd.is_empty() {
                    run_cmd(cmd).context("mypy_cmd failed")?;
                }
            }
            if let Some(cmd) = &v.pyright_cmd {
                if !cmd.is_empty() {
                    run_cmd(cmd).context("pyright_cmd failed")?;
                }
            }
        }
        Ok(())
    }

    fn determine_package_structure(out_abs: &Path) -> Result<(PathBuf, String)> {
        // Try to find a reasonable parent directory that contains the output as a package
        let out_name = out_abs
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("generated");

        // Check if parent directory exists and use it as PYTHONPATH
        let parent = out_abs.parent();
        if let Some(parent_dir) = parent {
            if parent_dir.exists() {
                return Ok((parent_dir.to_path_buf(), out_name.to_string()));
            }
        }

        // Fallback: use output directory directly (for backward compatibility)
        Ok((out_abs.to_path_buf(), String::new()))
    }

    fn create_import_test_script(package_name: &str, modules: &[String]) -> String {
        let mut script = String::new();
        script.push_str("import sys\n");
        script.push_str("import importlib\n");
        script.push_str("import traceback\n");
        script.push('\n');
        script.push_str("failed = []\n");
        script.push_str("succeeded = []\n");
        script.push('\n');

        for module in modules {
            let full_module = if package_name.is_empty() {
                module.clone()
            } else {
                format!("{}.{}", package_name, module)
            };

            script.push_str(&format!(
                r#"
# Test module: {} -> {}
try:
    mod = importlib.import_module('{}')
    succeeded.append('{}')
except ImportError as e:
    # More specific error handling for ImportError
    import_error = str(e)
    if "relative import" in import_error.lower():
        import_error += " (relative import context issue)"
    failed.append(('{}', 'ImportError: ' + import_error))
except ModuleNotFoundError as e:
    failed.append(('{}', 'ModuleNotFoundError: ' + str(e)))
except SyntaxError as e:
    failed.append(('{}', 'SyntaxError: ' + str(e) + ' at line ' + str(e.lineno or 'unknown')))
except Exception as e:
    tb = traceback.format_exc()
    failed.append(('{}', 'Exception: ' + type(e).__name__ + ': ' + str(e)))
"#,
                module, full_module, full_module, module, module, module, module, module
            ));
        }

        script.push('\n');
        script.push_str("print(f'IMPORT_TEST_SUMMARY:succeeded={len(succeeded)},failed={len(failed)},total={len(succeeded)+len(failed)}', file=sys.stderr)\n");
        script.push('\n');
        script.push_str("if failed:\n");
        script.push_str("    for module, error in failed:\n");
        script.push_str("        print(f'IMPORT_ERROR:{module}:{error}', file=sys.stderr)\n");
        script.push_str("    sys.exit(1)\n");
        script.push_str("else:\n");
        script.push_str(
            "    print('IMPORT_TEST_SUCCESS:all_modules_imported_successfully', file=sys.stderr)\n",
        );

        script
    }

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

            let mut cmd = std::process::Command::new(&cfg.python_exe);

            // Handle uv-specific command structure
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
                let mut error_msg = String::new();

                // Parse structured error output
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
                }

                if error_msg.is_empty() {
                    error_msg = format!(
                        "Unknown error (exit code: {})",
                        output.status.code().unwrap_or(-1)
                    );
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

    fn run_cmd(cmd: &[String]) -> Result<()> {
        let mut it = cmd.iter();
        let prog = it.next().ok_or_else(|| anyhow::anyhow!("empty command"))?;
        let status = std::process::Command::new(prog)
            .args(it)
            .status()
            .with_context(|| format!("failed to run {}", prog))?;
        if !status.success() {
            anyhow::bail!("command failed: {} (status {:?})", prog, status.code());
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::tempdir;

        #[test]
        fn determine_package_structure_with_parent() {
            let dir = tempdir().unwrap();
            let nested_dir = dir.path().join("generated");
            std::fs::create_dir_all(&nested_dir).unwrap();

            let (parent_path, package_name) = determine_package_structure(&nested_dir).unwrap();

            assert_eq!(parent_path, dir.path());
            assert_eq!(package_name, "generated");
        }

        #[test]
        fn determine_package_structure_no_parent() {
            let dir = tempdir().unwrap();

            let (parent_path, package_name) = determine_package_structure(dir.path()).unwrap();

            // Should use parent directory if it exists, or fall back to the directory itself
            if let Some(expected_parent) = dir.path().parent() {
                assert_eq!(parent_path, expected_parent);
                assert_eq!(
                    package_name,
                    dir.path().file_name().unwrap().to_str().unwrap()
                );
            } else {
                // Fallback case: use directory itself
                assert_eq!(parent_path, dir.path());
                assert_eq!(package_name, "");
            }
        }

        #[test]
        fn determine_package_structure_root_directory() {
            use std::path::Path;
            let root = Path::new("/");

            let (parent_path, package_name) = determine_package_structure(root).unwrap();

            // Should handle root directory gracefully
            assert_eq!(parent_path, root);
            assert_eq!(package_name, "");
        }

        #[test]
        fn create_import_test_script_empty_package() {
            let modules = vec!["service_pb2".to_string(), "api_pb2_grpc".to_string()];
            let script = create_import_test_script("", &modules);

            assert!(script.contains("import sys"));
            assert!(script.contains("import importlib"));
            assert!(script.contains("importlib.import_module('service_pb2')"));
            assert!(script.contains("importlib.import_module('api_pb2_grpc')"));
            assert!(script.contains("IMPORT_TEST_SUMMARY"));
        }

        #[test]
        fn create_import_test_script_with_package() {
            let modules = vec!["service_pb2".to_string()];
            let script = create_import_test_script("generated", &modules);

            assert!(script.contains("importlib.import_module('generated.service_pb2')"));
            assert!(script.contains("succeeded.append('service_pb2')"));
            assert!(script.contains("# Test module: service_pb2 -> generated.service_pb2"));
        }

        #[test]
        fn create_import_test_script_empty_modules() {
            let modules: Vec<String> = vec![];
            let script = create_import_test_script("generated", &modules);

            assert!(script.contains("import sys"));
            assert!(script.contains("failed = []"));
            assert!(script.contains("succeeded = []"));
            // Should not contain any import attempts
            assert!(!script.contains("importlib.import_module"));
        }

        #[test]
        fn create_import_test_script_error_handling() {
            let modules = vec!["test_pb2".to_string()];
            let script = create_import_test_script("pkg", &modules);

            // Should include comprehensive error handling
            assert!(script.contains("except ImportError as e:"));
            assert!(script.contains("except ModuleNotFoundError as e:"));
            assert!(script.contains("except SyntaxError as e:"));
            assert!(script.contains("except Exception as e:"));
            assert!(script.contains("relative import"));
        }

        #[test]
        fn create_import_test_script_output_format() {
            let modules = vec!["service_pb2".to_string()];
            let script = create_import_test_script("generated", &modules);

            // Check for expected output format
            assert!(
                script.contains(
                    "IMPORT_TEST_SUMMARY:succeeded={len(succeeded)},failed={len(failed)}"
                )
            );
            assert!(script.contains("IMPORT_ERROR:{module}:{error}"));
            assert!(script.contains("IMPORT_TEST_SUCCESS:all_modules_imported_successfully"));
            assert!(script.contains("sys.exit(1)"));
        }
    }
}

mod doctor {
    use anyhow::{Context, Result, bail};
    use which::which;

    fn check(cmd: &str) -> Option<String> {
        which(cmd)
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
    }

    pub fn run() -> Result<()> {
        let tools = [
            "python3", "uv", "protoc", "buf",
            // grpc_tools.protoc is detected as Python module
            "mypy", "pyright",
        ];

        println!("== Tool presence ==");
        for t in tools {
            println!(
                "{:<10} : {}",
                t,
                check(t).unwrap_or_else(|| "not found".into())
            );
        }

        // Check grpc_tools.protoc availability
        let py = check("uv").unwrap_or_else(|| check("python3").unwrap_or_default());
        if py.is_empty() {
            println!("grpc_tools.protoc : skip (python not found)");
        } else {
            let out = std::process::Command::new(&py)
                .args([
                    "-c",
                    "import pkgutil;print(1 if pkgutil.find_loader('grpc_tools') else 0)",
                ])
                .output()
                .context("failed to run python")?;
            let ok = String::from_utf8_lossy(&out.stdout).trim() == "1";
            println!(
                "grpc_tools      : {}",
                if ok { "found" } else { "not found" }
            );
        }

        // Return non-zero if none exist
        let any_found = [check("protoc"), check("buf")]
            .into_iter()
            .flatten()
            .next()
            .is_some();
        if !any_found {
            bail!("neither protoc nor buf found in PATH");
        }

        Ok(())
    }
}
