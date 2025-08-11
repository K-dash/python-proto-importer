use anyhow::{Context, Result};

/// Run a command with the given arguments
pub fn run_cmd(cmd: &[String]) -> Result<()> {
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
