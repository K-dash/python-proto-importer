#![cfg_attr(feature = "python", allow(clippy::useless_conversion))]

// Module declarations
pub(crate) mod cli;
pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod doctor;
pub(crate) mod generator {
    pub mod protoc;
}
pub(crate) mod postprocess;
pub(crate) mod python;
pub(crate) mod utils;
pub(crate) mod verification;

// Re-export main CLI functions
use anyhow::Result;

/// Main entry point for CLI usage
pub fn run_cli() -> Result<()> {
    cli::run_cli()
}

/// Entry point for CLI usage with custom arguments
pub fn run_cli_with<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::run_cli_with(args)
}
