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
        for pattern in inputs {
            let mut matched_any = false;
            if let Ok(paths) = glob(pattern) {
                for entry in paths.flatten() {
                    cmd.arg(entry);
                    matched_any = true;
                }
            }
            if !matched_any {
                // Fallback: pass through as-is
                cmd.arg(pattern);
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
}
