//! 工具配置模块，负责描述内置工具的开关和执行限制。

/// `exec_command` 工具的运行时配置。
#[derive(Debug, Clone)]
pub struct ExecCommandToolConfig {
    pub enabled: bool,
    pub shell: String,
    pub timeout_secs: u64,
    pub max_output_chars: usize,
}
