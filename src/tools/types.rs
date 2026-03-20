//! 工具类型模块，集中定义工具注册表对外暴露的数据结构。

use adk_rust::{Content, EventActions, serde_json::Value};
use serde::Serialize;

/// 工具描述信息，供 API 列表或调试页面展示。
#[derive(Debug, Clone, Serialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub response_schema: Option<Value>,
    pub long_running: bool,
}

/// 一次工具执行请求的统一输入结构。
#[derive(Debug, Clone)]
pub struct ToolExecutionRequest {
    pub app_name: String,
    pub user_id: String,
    pub session_id: String,
    pub invocation_id: String,
    pub function_call_id: String,
    pub tool_name: String,
    pub args: Value,
    pub user_content: Content,
}

/// 一次工具执行成功后的返回结果。
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub function_call_id: String,
    pub tool_name: String,
    pub args: Value,
    pub output: Value,
    #[allow(dead_code)]
    pub actions: EventActions,
}

/// 一次工具执行失败后的标准错误结构。
#[derive(Debug, Clone, Serialize)]
pub struct ToolExecutionFailure {
    pub function_call_id: String,
    pub tool_name: String,
    pub args: Value,
    pub message: String,
}
