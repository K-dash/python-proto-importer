//! # python-proto-importer
//!
//! A Rust-powered CLI that brings production-grade reliability to Python gRPC/Protobuf code generation.
//! Generate, validate, and maintain protobuf-based Python code with confidence.
//!
//! ## Core Features
//!
//! - **Automatic Import Rewriting**: Converts absolute imports to relative imports for better portability
//! - **Built-in Quality Assurance**: Validates generated code before it reaches your project
//! - **Comprehensive Verification**: Import testing and optional type checking with optimal settings
//! - **Production-Ready Workflow**: Single command for generation, postprocessing, and validation
//!
//! ## Quick Start
//!
//! ```toml
//! # pyproject.toml
//! [tool.python_proto_importer]
//! inputs = ["proto/**/*.proto"]
//! out = "generated"
//! ```
//!
//! Then run: `proto-importer build`

#![cfg_attr(feature = "python", allow(clippy::useless_conversion))]

// Module declarations
pub(crate) mod cli;
pub mod commands;
pub mod config;
pub mod doctor;
pub(crate) mod generator {
    pub mod protoc;
}
pub mod postprocess;
pub(crate) mod python;
pub(crate) mod utils;
pub mod verification;

// Re-export main CLI functions
use anyhow::Result;

/// Main entry point for CLI usage.
///
/// This function initializes the CLI application and processes command-line arguments
/// using the standard argument parsing. It's the primary entry point when using
/// this library as a CLI tool.
///
/// # Returns
///
/// Returns `Ok(())` on successful execution, or an `anyhow::Error` if any step
/// in the process fails.
///
/// # Example
///
/// ```no_run
/// use python_proto_importer::run_cli;
///
/// fn main() -> anyhow::Result<()> {
///     run_cli()
/// }
/// ```
pub fn run_cli() -> Result<()> {
    cli::run_cli()
}

/// Entry point for CLI usage with custom arguments.
///
/// This function allows programmatic invocation of the CLI with custom arguments,
/// useful for testing or when integrating the tool into other applications.
///
/// # Arguments
///
/// * `args` - An iterator of arguments where each item can be converted to `String`.
///   The first argument should typically be the program name.
///
/// # Returns
///
/// Returns `Ok(())` on successful execution, or an `anyhow::Error` if any step
/// in the process fails.
///
/// # Example
///
/// ```no_run
/// use python_proto_importer::run_cli_with;
///
/// fn main() -> anyhow::Result<()> {
///     let args = vec!["proto-importer", "build", "--no-verify"];
///     run_cli_with(args)
/// }
/// ```
pub fn run_cli_with<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::run_cli_with(args)
}
