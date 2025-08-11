//! Verification modules for generated Python protobuf code.
//!
//! This module provides comprehensive verification capabilities to ensure that
//! generated Python code is correct, importable, and ready for production use.
//! The verification phase is the final quality gate before generated code is
//! considered complete.
//!
//! # Verification Components
//!
//! - **Import Testing** ([`import_test`]): Validates that all generated modules can be imported
//! - **Package Structure Analysis** ([`package_structure`]): Determines optimal Python package layout
//! - **Test Script Generation** ([`script_generator`]): Creates dynamic test scripts for validation
//!
//! # Verification Pipeline
//!
//! The verification process follows these steps:
//!
//! 1. **Package Structure Detection**: Analyzes the output directory structure
//! 2. **Import Script Generation**: Creates a Python test script to import all modules  
//! 3. **Import Execution**: Runs the test script in the appropriate Python environment
//! 4. **Type Checking** (optional): Executes configured mypy/pyright commands
//! 5. **Result Analysis**: Reports any failures with actionable error messages
//!
//! # Usage Example
//!
//! ```no_run
//! use python_proto_importer::verification::import_test::verify;
//! use python_proto_importer::config::AppConfig;
//! use std::path::Path;
//!
//! // Load configuration and verify generated code
//! let config = AppConfig::load(Some(Path::new("pyproject.toml")))?;
//! verify(&config)?;
//!
//! println!("All generated code verified successfully!");
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod import_test;
pub mod package_structure;
pub mod script_generator;

pub use package_structure::{determine_package_structure, determine_package_structure_legacy};
pub use script_generator::create_import_test_script;
