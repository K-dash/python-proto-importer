use crate::config::AppConfig;
use anyhow::{Context, Result};
use glob::glob;
use std::fs;
use std::process::Command;
use tempfile::NamedTempFile;

pub struct ProtocRunner<'a> {
    cfg: &'a AppConfig,
}

impl<'a> ProtocRunner<'a> {
    pub fn new(cfg: &'a AppConfig) -> Self {
        Self { cfg }
    }

    pub fn generate(&self) -> Result<Vec<u8>> {
        // 1) descriptor set を作成
        let fds = NamedTempFile::new().context("create temp file for descriptor set")?;
        let fds_path = fds.path().to_path_buf();

        // ensure output directory exists
        if let Err(e) = std::fs::create_dir_all(&self.cfg.out) {
            return Err(e).context(format!(
                "failed to create output directory: {}",
                self.cfg.out.display()
            ));
        }

        // include パス
        let mut args: Vec<String> = Vec::new();
        for inc in &self.cfg.include {
            args.push(format!("--proto_path={}", inc.display()));
        }
        // inputs（globは Python 側に任せる設計だが、v0.1では文字列をそのまま渡す）
        let inputs = &self.cfg.inputs;

        // python -m grpc_tools.protoc ...
        // 既定python_exe（uv/python3）を使う
        let py = &self.cfg.python_exe;
        let mut cmd = Command::new(py);
        cmd.arg("-m").arg("grpc_tools.protoc");
        // Ensure protoc plugins installed in the same env are discoverable
        if let Some(parent) = std::path::Path::new(py).parent() {
            if let Some(parent_str) = parent.to_str() {
                // Only modify PATH if parent directory actually exists and is not empty
                if !parent_str.is_empty() && std::path::Path::new(parent_str).exists() {
                    use std::env;
                    let mut buf = std::ffi::OsString::new();
                    buf.push(parent_str);
                    buf.push(if cfg!(windows) { ";" } else { ":" });
                    if let Some(existing) = env::var_os("PATH") {
                        buf.push(existing);
                    }
                    cmd.env("PATH", buf);
                }
            }
        }

        // 出力先
        cmd.arg(format!("--python_out={}", self.cfg.out.display()));
        cmd.arg(format!("--grpc_python_out={}", self.cfg.out.display()));

        // mypy/mypy_grpc 出力（オプション）
        if self.cfg.generate_mypy {
            cmd.arg(format!("--mypy_out={}", self.cfg.out.display()));
        }
        if self.cfg.generate_mypy_grpc {
            cmd.arg(format!("--mypy_grpc_out={}", self.cfg.out.display()));
        }

        // descriptor set 出力
        cmd.arg("--include_imports");
        cmd.arg(format!("--descriptor_set_out={}", fds_path.display()));

        // include と inputs
        for a in &args {
            cmd.arg(a);
        }
        // Expand globs in inputs (v0.1: perform expansion here)
        // Filter files to only include those under specified include paths
        for pattern in inputs {
            let mut matched_any = false;
            if let Ok(paths) = glob(pattern) {
                for entry in paths.flatten() {
                    // Check if the file is under any of the include paths
                    let should_include = self.cfg.include.iter().any(|inc_path| {
                        // Try canonical path comparison first (most accurate)
                        match (entry.canonicalize(), inc_path.canonicalize()) {
                            (Ok(entry_canonical), Ok(inc_canonical)) => {
                                entry_canonical.starts_with(&inc_canonical)
                            },
                            _ => {
                                // Fallback to string-based comparison if canonicalization fails
                                // This handles cases where files/directories don't exist yet
                                entry.starts_with(inc_path) ||
                                // Also try relative path normalization
                                entry.strip_prefix("./").unwrap_or(&entry).starts_with(inc_path.strip_prefix("./").unwrap_or(inc_path))
                            }
                        }
                    });

                    if should_include {
                        cmd.arg(entry);
                        matched_any = true;
                    }
                }
            }
            if !matched_any {
                // If no files matched after filtering, don't pass anything
                // This prevents protoc errors for files outside include paths
                tracing::debug!("Pattern {} matched no files within include paths", pattern);
            }
        }

        tracing::info!("running grpc_tools.protoc");
        let status = cmd.status().context("failed to run grpc_tools.protoc")?;
        if !status.success() {
            anyhow::bail!("grpc_tools.protoc failed: status {}", status);
        }

        // FDS を読み込んで返却
        let bytes = fs::read(&fds_path).context("failed to read descriptor_set_out")?;
        Ok(bytes)
    }

    // Helper method for testing - allows inspection of command without execution
    #[cfg(test)]
    pub fn build_command(&self) -> Result<(Command, tempfile::NamedTempFile)> {
        let fds = NamedTempFile::new().context("create temp file for descriptor set")?;
        let fds_path = fds.path().to_path_buf();

        let mut args: Vec<String> = Vec::new();
        for inc in &self.cfg.include {
            args.push(format!("--proto_path={}", inc.display()));
        }

        let py = &self.cfg.python_exe;
        let mut cmd = Command::new(py);
        cmd.arg("-m").arg("grpc_tools.protoc");

        // PATH handling
        if let Some(parent) = std::path::Path::new(py).parent() {
            if let Some(parent_str) = parent.to_str() {
                if !parent_str.is_empty() && std::path::Path::new(parent_str).exists() {
                    use std::env;
                    let mut buf = std::ffi::OsString::new();
                    buf.push(parent_str);
                    buf.push(if cfg!(windows) { ";" } else { ":" });
                    if let Some(existing) = env::var_os("PATH") {
                        buf.push(existing);
                    }
                    cmd.env("PATH", buf);
                }
            }
        }

        // Output arguments
        cmd.arg(format!("--python_out={}", self.cfg.out.display()));
        cmd.arg(format!("--grpc_python_out={}", self.cfg.out.display()));

        if self.cfg.generate_mypy {
            cmd.arg(format!("--mypy_out={}", self.cfg.out.display()));
        }
        if self.cfg.generate_mypy_grpc {
            cmd.arg(format!("--mypy_grpc_out={}", self.cfg.out.display()));
        }

        cmd.arg("--include_imports");
        cmd.arg(format!("--descriptor_set_out={}", fds_path.display()));

        for a in &args {
            cmd.arg(a);
        }

        // Process inputs with glob expansion and filtering
        for pattern in &self.cfg.inputs {
            if let Ok(paths) = glob(pattern) {
                for entry in paths.flatten() {
                    let should_include = self.cfg.include.iter().any(|inc_path| {
                        match (entry.canonicalize(), inc_path.canonicalize()) {
                            (Ok(entry_canonical), Ok(inc_canonical)) => {
                                entry_canonical.starts_with(&inc_canonical)
                            }
                            _ => {
                                entry.starts_with(inc_path)
                                    || entry.strip_prefix("./").unwrap_or(&entry).starts_with(
                                        inc_path.strip_prefix("./").unwrap_or(inc_path),
                                    )
                            }
                        }
                    });

                    if should_include {
                        cmd.arg(&entry);
                    }
                }
            }
        }

        Ok((cmd, fds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, Backend, PostProcess};
    use std::fs;
    use tempfile::tempdir;

    fn create_test_config() -> AppConfig {
        AppConfig {
            backend: Backend::Protoc,
            python_exe: "python3".to_string(),
            include: vec![std::path::PathBuf::from(".")],
            inputs: vec!["**/*.proto".to_string()],
            out: std::path::PathBuf::from("generated"),
            generate_mypy: false,
            generate_mypy_grpc: false,
            postprocess: PostProcess {
                protoletariat: true,
                fix_pyi: true,
                create_package: true,
                exclude_google: true,
                pyright_header: false,
                module_suffixes: vec!["_pb2.py".into()],
            },
            verify: None,
        }
    }

    #[test]
    fn new_runner() {
        let config = create_test_config();
        let runner = ProtocRunner::new(&config);
        assert_eq!(runner.cfg.backend as u8, Backend::Protoc as u8);
    }

    #[test]
    fn build_command_basic_args() {
        let config = create_test_config();
        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        let cmd_str = format!("{:?}", cmd);
        assert!(cmd_str.contains("python3"));
        assert!(cmd_str.contains("-m"));
        assert!(cmd_str.contains("grpc_tools.protoc"));
        assert!(cmd_str.contains("--python_out=generated"));
        assert!(cmd_str.contains("--grpc_python_out=generated"));
        assert!(cmd_str.contains("--proto_path=."));
        assert!(cmd_str.contains("--include_imports"));
        assert!(cmd_str.contains("--descriptor_set_out="));
    }

    #[test]
    fn build_command_with_mypy() {
        let mut config = create_test_config();
        config.generate_mypy = true;
        config.generate_mypy_grpc = true;

        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        let cmd_str = format!("{:?}", cmd);
        assert!(cmd_str.contains("--mypy_out=generated"));
        assert!(cmd_str.contains("--mypy_grpc_out=generated"));
    }

    #[test]
    fn build_command_multiple_include_paths() {
        let mut config = create_test_config();
        config.include = vec![
            std::path::PathBuf::from("proto"),
            std::path::PathBuf::from("common"),
        ];

        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        let cmd_str = format!("{:?}", cmd);
        assert!(cmd_str.contains("--proto_path=proto"));
        assert!(cmd_str.contains("--proto_path=common"));
    }

    #[test]
    fn build_command_custom_python_exe() {
        let mut config = create_test_config();
        config.python_exe = "uv".to_string();

        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        let cmd_str = format!("{:?}", cmd);
        assert!(cmd_str.contains("uv"));
    }

    #[test]
    fn include_path_filtering() {
        let dir = tempdir().unwrap();
        let proto_dir = dir.path().join("proto");
        let other_dir = dir.path().join("other");
        fs::create_dir_all(&proto_dir).unwrap();
        fs::create_dir_all(&other_dir).unwrap();

        // Create test files
        fs::write(proto_dir.join("service.proto"), "syntax = \"proto3\";").unwrap();
        fs::write(other_dir.join("external.proto"), "syntax = \"proto3\";").unwrap();

        let mut config = create_test_config();
        config.include = vec![proto_dir.clone()];
        config.inputs = vec![format!("{}/**/*.proto", dir.path().display())];

        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        let cmd_str = format!("{:?}", cmd);
        // Should include service.proto but not external.proto
        assert!(cmd_str.contains("service.proto"));
        assert!(!cmd_str.contains("external.proto"));
    }

    #[test]
    fn relative_path_normalization() {
        let dir = tempdir().unwrap();
        let proto_dir = dir.path().join("proto");
        fs::create_dir_all(&proto_dir).unwrap();
        fs::write(proto_dir.join("service.proto"), "syntax = \"proto3\";").unwrap();

        let mut config = create_test_config();
        config.include = vec![std::path::PathBuf::from("./proto")];
        config.inputs = vec!["proto/**/*.proto".to_string()];

        // Change to test directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        let cmd_str = format!("{:?}", cmd);
        assert!(cmd_str.contains("--proto_path=./proto"));

        std::env::set_current_dir(&original_dir).unwrap();
    }

    #[test]
    fn empty_glob_pattern() {
        let mut config = create_test_config();
        config.inputs = vec!["nonexistent/**/*.proto".to_string()];

        let runner = ProtocRunner::new(&config);
        let (cmd, _temp) = runner.build_command().unwrap();

        // Check command arguments - should not contain paths ending with .proto
        // except for the descriptor_set_out parameter
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        let proto_file_args = args.iter().filter(|arg| {
            if let Some(s) = arg.to_str() {
                s.ends_with(".proto") && !s.contains("descriptor_set_out")
            } else {
                false
            }
        });
        assert_eq!(proto_file_args.count(), 0);
    }

    #[test]
    fn invalid_python_path_handling() {
        let mut config = create_test_config();
        config.python_exe = "/nonexistent/python".to_string();

        let runner = ProtocRunner::new(&config);
        // Should not panic, should handle gracefully
        let result = runner.build_command();
        assert!(result.is_ok());

        let (cmd, _temp) = result.unwrap();
        let cmd_str = format!("{:?}", cmd);
        assert!(cmd_str.contains("/nonexistent/python"));
    }
}
