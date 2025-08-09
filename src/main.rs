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
    /// 環境診断（protoc / buf / grpc_tools / mypy / pyright など）
    Doctor,
    /// 生成＋後処理＋検証
    Build {
        /// pyproject.toml のパス（省略時カレント）
        #[arg(long)]
        pyproject: Option<String>,
        /// 生成後に検証をスキップ
        #[arg(long)]
        no_verify: bool,
        /// 生成せず後処理・検証のみ（将来拡張用）
        #[arg(long)]
        postprocess_only: bool,
    },
    /// 生成せず import/mypy/pyright の検証のみ
    Check {
        #[arg(long)]
        pyproject: Option<String>,
    },
    /// 出力とキャッシュの削除
    Clean {
        #[arg(long)]
        pyproject: Option<String>,
        /// 確認なしで削除
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
    use super::postprocess::create_packages;
    use anyhow::{Context, Result, bail};
    use std::fs;
    use std::path::Path;

    pub fn build(pyproject: Option<&str>, no_verify: bool, _postprocess_only: bool) -> Result<()> {
        let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
        tracing::info!(?cfg.backend, out=%cfg.out.display(), "build start");

        match cfg.backend {
            Backend::Protoc => {
                let runner = ProtocRunner::new(&cfg);
                runner.generate()?;
            }
            Backend::Buf => {
                // v0.2 で実装
                tracing::warn!("buf backend is not implemented yet");
            }
        }

        // __init__.py 生成（オプトイン/デフォルトON）
        if cfg.postprocess.create_package {
            let created = create_packages(&cfg.out)?;
            tracing::info!("created __init__.py: {}", created);
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

        // 1) import dry-run for generated python modules
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
                // must match known module suffixes (e.g. _pb2.py/_pb2_grpc.py)
                if !py_suffixes.is_empty() && !py_suffixes.iter().any(|s| rel_str.ends_with(s)) {
                    continue;
                }
                let mod_name = rel_str.trim_end_matches(".py").replace('/', ".");
                modules.push(mod_name);
            }
        }

        // Keep it deterministic
        modules.sort();

        // Try to import each module with PYTHONPATH=out_abs
        let mut failed: Vec<(String, i32)> = Vec::new();
        for m in modules.iter() {
            let status = std::process::Command::new(&cfg.python_exe)
                .arg("-c")
                .arg(format!(
                    "import sys,importlib; sys.path.insert(0, r'{path}'); importlib.import_module('{m}')",
                    path = out_abs.display(),
                    m = m
                ))
                .status()
                .with_context(|| format!("failed running {} for import dry-run", cfg.python_exe))?;
            if !status.success() {
                failed.push((m.clone(), status.code().unwrap_or(-1)));
            }
        }
        if !failed.is_empty() {
            for (m, code) in &failed {
                tracing::error!(module=%m, exit_code=%code, "import failed");
            }
            anyhow::bail!("import dry-run failed for {} modules", failed.len());
        }
        tracing::info!("import dry-run passed ({} modules)", modules.len());

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
            // grpc_tools.protoc は Python モジュールとして検出
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

        // grpc_tools.protoc の確認
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

        // いずれも存在しない場合は非0を返す
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
