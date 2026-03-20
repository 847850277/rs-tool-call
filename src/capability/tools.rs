//! 工具能力模块，负责暴露工具目录查询和直接工具调用能力。

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use crate::{
    engine::ToolCallEngine,
    tools::{ToolDescriptor, ToolExecutionResult},
};

/// 直接工具调用能力的输入参数。
#[derive(Debug, Clone)]
pub struct DirectToolInvocationRequest {
    pub tool: String,
    pub session_id: String,
    pub user_id: String,
    pub args: Value,
}

/// 工具能力。
#[derive(Clone)]
pub struct ToolCapability {
    engine: Arc<ToolCallEngine>,
}

impl ToolCapability {
    /// 基于底层引擎创建工具能力。
    pub fn new(engine: Arc<ToolCallEngine>) -> Self {
        Self { engine }
    }

    /// 返回当前系统已注册的工具描述列表。
    pub fn list_descriptors(&self) -> Vec<ToolDescriptor> {
        self.engine.tools()
    }

    /// 直接调用某个工具，并返回执行结果。
    pub async fn invoke(
        &self,
        request: DirectToolInvocationRequest,
    ) -> Result<ToolExecutionResult> {
        self.engine
            .invoke_tool(
                request.user_id,
                request.session_id,
                request.tool,
                request.args,
            )
            .await
    }
}
