use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::process::Command;
use tempfile::NamedTempFile;

pub struct ProtocRunner<'a> {
    cfg: &'a AppConfig,
}

impl<'a> ProtocRunner<'a> {
    pub fn new(cfg: &'a AppConfig) -> Self {
        Self { cfg }
    }

    pub fn generate(&self) -> Result<()> {
        // 1) descriptor set を作成
        let fds = NamedTempFile::new().context("create temp file for descriptor set")?;
        let fds_path = fds.path().to_path_buf();

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

        // 出力先
        cmd.arg(format!("--python_out={}", self.cfg.out.display()));
        cmd.arg(format!("--grpc_python_out={}", self.cfg.out.display()));

        // mypy/mypy_grpc は v0.1 では後日拡張（現状は無効）

        // descriptor set 出力
        cmd.arg("--include_imports");
        cmd.arg(format!("--descriptor_set_out={}", fds_path.display()));

        // include と inputs
        for a in &args {
            cmd.arg(a);
        }
        for i in inputs {
            cmd.arg(i);
        }

        tracing::info!("running grpc_tools.protoc");
        let status = cmd.status().context("failed to run grpc_tools.protoc")?;
        if !status.success() {
            anyhow::bail!("grpc_tools.protoc failed: status {}", status);
        }

        // TODO: fds_path を読み込んで後処理に渡す（次フェーズ）
        Ok(())
    }
}
