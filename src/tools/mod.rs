//! `tools` 模块负责管理工具注册、内置工具构建以及工具执行上下文。
//! 这里将注册表、上下文、内置工具和命令执行辅助逻辑拆成独立子模块。

mod builtin;
mod context;
mod exec;
mod registry;
mod types;

pub use builtin::build_builtin_registry;
pub use registry::ToolRegistry;
pub use types::{ToolDescriptor, ToolExecutionFailure, ToolExecutionRequest, ToolExecutionResult};
