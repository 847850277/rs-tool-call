//! 工具注册表模块，负责统一管理工具集合、描述导出和执行分发。

use std::{collections::HashMap, sync::Arc};

use adk_rust::{
    Tool, ToolContext,
    serde_json::{Value, json},
};
use anyhow::{Result, anyhow, bail};

use super::{
    ToolDescriptor, ToolExecutionRequest, ToolExecutionResult, context::RequestToolContext,
};

/// 工具注册表，按名称索引当前可用的全部工具。
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    /// 使用给定工具列表创建注册表，并拒绝重复名称。
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Result<Self> {
        let mut map = HashMap::new();
        for tool in tools {
            let name = tool.name().to_string();
            if map.insert(name.clone(), tool).is_some() {
                bail!("duplicate tool registration: {name}");
            }
        }

        Ok(Self {
            tools: Arc::new(map),
        })
    }

    /// 返回当前注册表中全部工具的描述信息。
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        let mut items = self
            .tools
            .values()
            .map(|tool| ToolDescriptor {
                name: tool.name().to_string(),
                description: tool.enhanced_description(),
                parameters_schema: tool
                    .parameters_schema()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                response_schema: tool.response_schema(),
                long_running: tool.is_long_running(),
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name));
        items
    }

    /// 返回适合发送给 LLM 的工具 schema 映射。
    pub fn schemas(&self) -> HashMap<String, Value> {
        self.tools
            .values()
            .map(|tool| {
                (
                    tool.name().to_string(),
                    json!({
                        "description": tool.enhanced_description(),
                        "parameters": tool.parameters_schema().unwrap_or_else(|| json!({
                            "type": "object",
                            "properties": {}
                        })),
                    }),
                )
            })
            .collect()
    }

    /// 判断某个工具是否已注册。
    pub fn has(&self, tool_name: &str) -> bool {
        self.tools.contains_key(tool_name)
    }

    /// 按统一请求结构执行指定工具。
    pub async fn execute(&self, request: ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let tool = self
            .tools
            .get(&request.tool_name)
            .cloned()
            .ok_or_else(|| anyhow!("tool not found: {}", request.tool_name))?;
        let context = Arc::new(RequestToolContext::new(&request));
        let output = tool.execute(context.clone(), request.args.clone()).await?;

        Ok(ToolExecutionResult {
            function_call_id: request.function_call_id,
            tool_name: request.tool_name,
            args: request.args,
            output,
            actions: context.actions(),
        })
    }
}
