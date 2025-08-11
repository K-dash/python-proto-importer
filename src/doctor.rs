use crate::config::AppConfig;
use crate::verification::{determine_package_structure, determine_package_structure_legacy};
use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;
use which::which;

/// Check if a command is available in PATH and return its path.
///
/// # Arguments
///
/// * `cmd` - Command name to check for
///
/// # Returns
///
/// Returns `Some(String)` with the full path if the command is found,
/// or `None` if it's not available.
fn check(cmd: &str) -> Option<String> {
    which(cmd)
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
}

/// Run environment diagnostics and display system information.
///
/// This function performs a comprehensive check of the development environment,
/// reporting on the availability and versions of tools needed for proto-to-Python
/// code generation. It checks for:
///
/// - Python interpreters (uv, python3, python)
/// - Required Python packages (grpcio-tools)
/// - Optional Python packages (mypy-protobuf, mypy-grpc)
/// - Type checkers (mypy, pyright)
/// - System tools (protoc, buf)
///
/// The function also attempts to load and validate a pyproject.toml configuration
/// to provide targeted recommendations based on the current project setup.
///
/// # Returns
///
/// Returns `Ok(())` on successful completion of all checks, or an error
/// if critical issues prevent the diagnostic from running.
///
/// # Example
///
/// ```no_run
/// use python_proto_importer::doctor;
///
/// fn main() -> anyhow::Result<()> {
///     doctor::run()
/// }
/// ```
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

    let (mypy_protobuf_found, mypy_protobuf_ver) = probe_python_pkg(&py_runner, "mypy-protobuf");
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

    // Check package structure if pyproject.toml is found
    if let Ok(cfg) = AppConfig::load(Some(Path::new("pyproject.toml"))) {
        println!("\n== Package structure analysis ==");

        let out_abs = cfg.out.canonicalize().unwrap_or_else(|_| cfg.out.clone());
        println!("Output directory: {}", out_abs.display());

        if !out_abs.exists() {
            println!("  ❌ Output directory does not exist. Run 'build' first to generate files.");
        } else {
            println!("  ✅ Output directory exists");

            // Analyze current package structure determination
            let (parent_path, package_name) =
                determine_package_structure(&out_abs).unwrap_or_else(|e| {
                    println!("  ❌ Failed to determine package structure: {}", e);
                    (std::path::PathBuf::new(), String::new())
                });

            let (legacy_parent_path, legacy_package_name) =
                determine_package_structure_legacy(&out_abs).unwrap_or_else(|e| {
                    println!("  ❌ Failed to determine legacy package structure: {}", e);
                    (std::path::PathBuf::new(), String::new())
                });

            println!("  Current implementation:");
            println!("    PYTHONPATH: {}", parent_path.display());
            println!(
                "    Package name: {}",
                if package_name.is_empty() {
                    "<empty>"
                } else {
                    &package_name
                }
            );

            if parent_path != legacy_parent_path || package_name != legacy_package_name {
                println!("  Legacy implementation (fallback):");
                println!("    PYTHONPATH: {}", legacy_parent_path.display());
                println!(
                    "    Package name: {}",
                    if legacy_package_name.is_empty() {
                        "<empty>"
                    } else {
                        &legacy_package_name
                    }
                );
            }

            // Check parent directory package status
            if let Some(parent_dir) = out_abs.parent() {
                let parent_init = parent_dir.join("__init__.py");
                if parent_init.exists() {
                    println!("  ✅ Parent directory is a Python package (has __init__.py)");
                    if let Some(parent_name) = parent_dir.file_name().and_then(|n| n.to_str()) {
                        println!("    Package name: {}", parent_name);
                    }
                    if let Some(grandparent) = parent_dir.parent() {
                        println!("    Recommended PYTHONPATH: {}", grandparent.display());
                    }
                } else {
                    println!("  ℹ️  Parent directory is not a Python package (no __init__.py)");
                    println!("    This is fine for simple structures");
                }
            }

            // Count generated files
            if let Ok(entries) = std::fs::read_dir(&out_abs) {
                let py_files: Vec<_> = entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.path().extension().and_then(|ext| ext.to_str()) == Some("py")
                    })
                    .collect();
                println!("  Generated files: {} Python files found", py_files.len());
            }
        }
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
