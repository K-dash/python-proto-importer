use crate::config::AppConfig;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

/// Execute the clean command
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
