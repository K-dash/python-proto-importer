use crate::config::AppConfig;
use crate::verification::import_test::verify;
use anyhow::{Context, Result};
use std::path::Path;

/// Execute the check command
pub fn check(pyproject: Option<&str>) -> Result<()> {
    let cfg = AppConfig::load(pyproject.map(Path::new)).context("failed to load config")?;
    verify(&cfg)
}
