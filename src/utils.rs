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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_cmd_empty_command() {
        let cmd = vec![];
        let result = run_cmd(&cmd);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty command"));
    }

    #[test]
    fn test_run_cmd_success() {
        let cmd = vec!["echo".to_string(), "hello".to_string()];
        let result = run_cmd(&cmd);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_cmd_failure() {
        let cmd = vec!["false".to_string()];
        let result = run_cmd(&cmd);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("command failed"));
    }

    #[test]
    fn test_run_cmd_nonexistent_program() {
        let cmd = vec!["nonexistent_program_xyz123".to_string()];
        let result = run_cmd(&cmd);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to run"));
    }
}
