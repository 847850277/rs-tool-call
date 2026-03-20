//! `models` 模块负责根据配置构建底层 LLM 客户端。
//! 这里按 provider 类型把 OpenAI 兼容客户端和 Gemini 客户端拆成独立子模块。

mod gemini;
mod openai_compatible;

use std::sync::Arc;

use adk_rust::Llm;
use anyhow::Result;

use crate::config::{LlmConfig, LlmProvider};

/// 根据配置构建可供引擎使用的统一 LLM 实例。
pub fn build_llm(config: &LlmConfig) -> Result<Arc<dyn Llm>> {
    match config.provider {
        LlmProvider::OpenAi | LlmProvider::Glm | LlmProvider::SiliconFlow => {
            openai_compatible::build_openai_compatible_llm(config)
        }
        LlmProvider::Gemini => gemini::build_gemini_llm(config),
    }
}
