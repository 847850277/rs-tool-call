//! 命令执行工具模块，负责处理 `exec_command` 的参数、结果以及实际命令运行。

use std::process::Stdio;

use anyhow::{Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::timeout};

use crate::config::ExecCommandToolConfig;

/// `exec_command` 工具的输入参数。
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExecCommandArgs {
    pub(crate) cmd: String,
    #[serde(default)]
    pub(crate) workdir: Option<String>,
    #[serde(default)]
    pub(crate) timeout_secs: Option<u64>,
    #[serde(default)]
    pub(crate) max_output_chars: Option<usize>,
    #[serde(default)]
    pub(crate) shell: Option<String>,
}

/// `exec_command` 工具的执行结果。
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExecCommandResult {
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
    pub(crate) command: String,
    pub(crate) workdir: Option<String>,
}

/// 执行一条 shell 命令，并返回截断后的 stdout/stderr。
pub(crate) async fn run_exec_command(
    input: ExecCommandArgs,
    config: &ExecCommandToolConfig,
) -> Result<ExecCommandResult> {
    let cmd = input.cmd.trim();
    if cmd.is_empty() {
        bail!("cmd must not be empty");
    }

    let shell = input
        .shell
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(config.shell.as_str());
    let timeout_secs = input
        .timeout_secs
        .unwrap_or(config.timeout_secs)
        .clamp(1, 120);
    let max_output_chars = input
        .max_output_chars
        .unwrap_or(config.max_output_chars)
        .clamp(128, 20_000);

    let mut command = Command::new(shell);
    command
        .arg("-lc")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(workdir) = input
        .workdir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        command.current_dir(workdir);
    }

    let child = command.spawn()?;
    let output = match timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            return Ok(ExecCommandResult {
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("command timed out after {timeout_secs}s"),
                timed_out: true,
                command: cmd.to_string(),
                workdir: input.workdir,
            });
        }
    };

    Ok(ExecCommandResult {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: truncate_output(&String::from_utf8_lossy(&output.stdout), max_output_chars),
        stderr: truncate_output(&String::from_utf8_lossy(&output.stderr), max_output_chars),
        timed_out: false,
        command: cmd.to_string(),
        workdir: input.workdir,
    })
}

/// 将 Unix 毫秒时间戳转换成简单的 UTC 字符串表示。
pub(crate) fn format_unix_ms(unix_ms: u64) -> String {
    let seconds = unix_ms / 1_000;
    let millis = unix_ms % 1_000;
    format!("{seconds}.{millis:03}Z")
}

/// 按最大字符数截断命令输出，避免把过长内容塞进上下文。
pub(crate) fn truncate_output(value: &str, max_chars: usize) -> String {
    let mut truncated = value.trim().chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        truncated.push_str("...");
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exec_command_runs_and_captures_output() {
        let result = run_exec_command(
            ExecCommandArgs {
                cmd: "printf 'hello'".to_string(),
                workdir: None,
                timeout_secs: Some(5),
                max_output_chars: Some(100),
                shell: Some("/bin/sh".to_string()),
            },
            &ExecCommandToolConfig {
                enabled: true,
                shell: "/bin/sh".to_string(),
                timeout_secs: 20,
                max_output_chars: 4000,
            },
        )
        .await
        .expect("command should succeed");

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "hello");
    }

    #[test]
    fn truncate_output_limits_text() {
        assert_eq!(truncate_output("abcdef", 4), "abcd...");
        assert_eq!(truncate_output("abc", 4), "abc");
    }
}
