use crate::config::{AppConfig, Backend};
use crate::generator::protoc::ProtocRunner;
use crate::postprocess::add_pyright_header;
use crate::postprocess::apply::apply_rewrites_in_tree;
use crate::postprocess::create_packages;
use crate::postprocess::fds::{collect_generated_basenames_from_bytes, load_fds_from_bytes};
use crate::postprocess::rel_imports::scan_and_report;
use crate::verification::import_test::verify;
use anyhow::{Context, Result};
use std::path::Path;

/// Execute the build command
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
