//! OpenAI 兼容模型构建模块，负责创建使用 OpenAI 协议的模型客户端。

use std::sync::Arc;

use adk_model::openai::{OpenAIClient, OpenAIConfig};
use adk_rust::Llm;
use anyhow::Result;

use crate::config::LlmConfig;

/// 构建一个 OpenAI 兼容的 LLM 客户端。
pub(crate) fn build_openai_compatible_llm(config: &LlmConfig) -> Result<Arc<dyn Llm>> {
    Ok(Arc::new(OpenAIClient::new(OpenAIConfig {
        api_key: config.api_key.clone(),
        model: config.model.clone(),
        organization_id: None,
        project_id: None,
        base_url: config.base_url.clone(),
    })?))
}
