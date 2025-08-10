#![cfg_attr(feature = "python", allow(clippy::useless_conversion))]
use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

pub(crate) mod config;
pub(crate) mod generator {
    pub mod protoc;
}
pub(crate) mod postprocess;

#[derive(Parser, Debug)]
#[command(
    name = "proto-importer",
    version,
    about = "Python proto importer toolkit"
)]
struct Cli {
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Doctor,
    Build {
        #[arg(long)]
        pyproject: Option<String>,
        #[arg(long)]
        no_verify: bool,
        #[arg(long)]
        postprocess_only: bool,
    },
    Check {
        #[arg(long)]
        pyproject: Option<String>,
    },
    Clean {
        #[arg(long)]
        pyproject: Option<String>,
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

pub fn run_cli() -> Result<()> {
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

pub fn run_cli_with<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut v: Vec<String> = args.into_iter().map(Into::into).collect();
    if v.is_empty() || v.first().map(|s| s.is_empty()).unwrap_or(true) {
        v.insert(0, "proto-importer".to_string());
    }

    let cli = Cli::parse_from(v);
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

        let allowed_basenames = if _postprocess_only {
            if !cfg.out.exists() {
                anyhow::bail!(
                    "--postprocess-only: output directory does not exist: {}",
                    cfg.out.display()
                );
            }
            tracing::info!("postprocess-only mode: skip generation");
            None
        } else {
            match cfg.backend {
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
                    tracing::warn!("buf backend is not implemented yet");
                    None
                }
            }
        };

        if cfg.postprocess.create_package {
            let created = create_packages(&cfg.out)?;
            tracing::info!("created __init__.py: {}", created);
        }

        let (files, hits) =
            scan_and_report(&cfg.out).context("scan relative-import candidates failed")?;
        tracing::info!(
            "relative-import candidates: files={}, lines={}",
            files,
            hits
        );

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
                    for (m, error) in &failed_modules {
                        tracing::error!(module=%m, "import failed: {}", error);
                    }
                    anyhow::bail!(
                        "import dry-run failed for {} modules (out of {}). Use -v for more details.",
                        failed_modules.len(),
                        modules.len()
                    );
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

    fn determine_package_structure(out_abs: &Path) -> Result<(PathBuf, String)> {
        let out_name = out_abs
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("generated");

        let parent = out_abs.parent();
        if let Some(parent_dir) = parent.filter(|p| p.exists()) {
            return Ok((parent_dir.to_path_buf(), out_name.to_string()));
        }

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

            if let Some(expected_parent) = dir.path().parent() {
                assert_eq!(parent_path, expected_parent);
                assert_eq!(
                    package_name,
                    dir.path().file_name().unwrap().to_str().unwrap()
                );
            } else {
                assert_eq!(parent_path, dir.path());
                assert_eq!(package_name, "");
            }
        }

        #[test]
        fn determine_package_structure_root_directory() {
            use std::path::Path;
            let root = Path::new("/");

            let (parent_path, package_name) = determine_package_structure(root).unwrap();

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
            assert!(!script.contains("importlib.import_module"));
        }

        #[test]
        fn create_import_test_script_error_handling() {
            let modules = vec!["test_pb2".to_string()];
            let script = create_import_test_script("pkg", &modules);

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
    use crate::config::AppConfig;
    use anyhow::{Result, bail};
    use std::path::Path;
    use std::process::Command;
    use which::which;

    fn check(cmd: &str) -> Option<String> {
        which(cmd)
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
    }

    pub fn run() -> Result<()> {
        println!("== Tool presence ==");

        let py_runner = check("uv")
            .or_else(|| check("python3"))
            .or_else(|| check("python"))
            .unwrap_or_default();

        if let Some(uv_path) = check("uv") {
            let uv_ver = cmd_version(&uv_path, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "uv", uv_path, uv_ver.trim());
        } else {
            println!("{:<14}: not found", "uv");
        }
        if let Some(py3) = check("python3") {
            let py_ver = cmd_version(&py3, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "python3", py3, py_ver.trim());
        } else if let Some(py) = check("python") {
            let py_ver = cmd_version(&py, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "python", py, py_ver.trim());
        } else {
            println!("{:<14}: not found", "python");
        }

        let (grpc_tools_found, grpc_tools_ver) = probe_python_pkg(&py_runner, "grpcio-tools");
        println!(
            "{:<14}: {}{}",
            "grpc_tools",
            if grpc_tools_found {
                "found"
            } else {
                "not found"
            },
            grpc_tools_ver
                .as_deref()
                .map(|v| format!(" ({})", v))
                .unwrap_or_default()
        );

        let (mypy_protobuf_found, mypy_protobuf_ver) =
            probe_python_pkg(&py_runner, "mypy-protobuf");
        println!(
            "{:<14}: {}{}",
            "mypy-protobuf",
            if mypy_protobuf_found {
                "found"
            } else {
                "not found"
            },
            mypy_protobuf_ver
                .as_deref()
                .map(|v| format!(" ({})", v))
                .unwrap_or_default()
        );
        let (mypy_grpc_found, mypy_grpc_ver) = probe_python_pkg(&py_runner, "mypy-grpc");
        println!(
            "{:<14}: {}{}",
            "mypy-grpc",
            if mypy_grpc_found {
                "found"
            } else {
                "not found"
            },
            mypy_grpc_ver
                .as_deref()
                .map(|v| format!(" ({})", v))
                .unwrap_or_default()
        );

        if let Some(p) = check("protoc") {
            let v = cmd_version(&p, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "protoc", p, v.trim());
        } else {
            println!("{:<14}: not found", "protoc");
        }
        if let Some(p) = check("buf") {
            let v = cmd_version(&p, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "buf", p, v.trim());
        } else {
            println!("{:<14}: not found", "buf");
        }

        if let Some(p) = check("mypy") {
            let v = cmd_version(&p, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "mypy", p, v.trim());
        } else {
            println!("{:<14}: not found", "mypy");
        }
        if let Some(p) = check("pyright") {
            let v = cmd_version(&p, &["--version"]).unwrap_or_else(|| "unknown".into());
            println!("{:<14}: {} ({})", "pyright", p, v.trim());
        } else {
            println!("{:<14}: not found", "pyright");
        }

        if let Ok(cfg) = AppConfig::load(Some(Path::new("pyproject.toml"))) {
            println!("\n== Based on pyproject.toml ==");
            if cfg.generate_mypy && !mypy_protobuf_found {
                println!(
                    "hint: mypy-protobuf is required (install via 'uv add mypy-protobuf' or 'pip install mypy-protobuf')"
                );
            }
            if cfg.generate_mypy_grpc && !mypy_grpc_found {
                println!(
                    "hint: mypy-grpc is required (install via 'uv add mypy-grpc' or 'pip install mypy-grpc')"
                );
            }
            if let Some(v) = &cfg.verify {
                if v.mypy_cmd.is_some() && check("mypy").is_none() {
                    println!(
                        "hint: mypy CLI not found (install via 'uv add mypy' or 'pip install mypy')"
                    );
                }
                if v.pyright_cmd.is_some() && check("pyright").is_none() {
                    println!(
                        "hint: pyright CLI not found (install via 'uv add pyright' or 'npm i -g pyright')"
                    );
                }
            }
        }

        if !grpc_tools_found {
            bail!(
                "grpc_tools.protoc not found. Install with 'uv add grpcio-tools' or 'pip install grpcio-tools'"
            );
        }

        Ok(())
    }

    fn cmd_version(bin: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(bin).args(args).output().ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let txt = if s.trim().is_empty() {
                String::from_utf8_lossy(&out.stderr).into_owned()
            } else {
                s.into_owned()
            };
            Some(txt)
        } else {
            None
        }
    }

    fn probe_python_pkg(py_runner: &str, dist_name: &str) -> (bool, Option<String>) {
        if py_runner.is_empty() {
            return (false, None);
        }
        let code = format!(
            "import sys\ntry:\n import importlib.metadata as m\n print('1 ' + m.version('{0}'))\nexcept Exception:\n try:\n  import pkg_resources as pr\n  print('1 ' + pr.get_distribution('{0}').version)\n except Exception:\n  print('0')\n",
            dist_name
        );
        let mut cmd = Command::new(py_runner);
        if py_runner.ends_with("uv") || py_runner == "uv" {
            cmd.arg("run").arg("python").arg("-c").arg(&code);
        } else {
            cmd.arg("-c").arg(&code);
        }
        let out_opt = match cmd.output() {
            Ok(o) if o.status.success() => Some(o),
            _ => None,
        };
        if let Some(out) = out_opt {
            let s = String::from_utf8_lossy(&out.stdout);
            let t = s.trim();
            if let Some(rest) = t.strip_prefix('1') {
                let ver = rest.trim();
                let ver = ver.strip_prefix(' ').unwrap_or(ver);
                return (
                    true,
                    if ver.is_empty() {
                        None
                    } else {
                        Some(ver.to_string())
                    },
                );
            }
        }
        (false, None)
    }
}

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[allow(clippy::useless_conversion)]
#[pyfunction]
fn main(py: Python<'_>) -> PyResult<i32> {
    // Build argv with a stable program name followed by Python's argv[1:]
    let sys = py.import("sys")?;
    let argv: Vec<String> = sys.getattr("argv")?.extract()?;
    let mut args: Vec<String> = Vec::new();
    if argv.len() > 1 {
        args.extend(argv.iter().skip(1).cloned());
    }
    let iter = std::iter::once("proto-importer".to_string()).chain(args.into_iter());
    match crate::run_cli_with(iter) {
        Ok(()) => Ok(0),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
    }
}

#[cfg(feature = "python")]
#[pymodule]
fn python_proto_importer(_py: Python<'_>, m: &pyo3::prelude::Bound<PyModule>) -> PyResult<()> {
    m.add_function(pyo3::wrap_pyfunction!(main, m)?)?;
    Ok(())
}
