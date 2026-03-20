//! Gemini 模型构建模块，负责在启用对应 feature 时构建 Gemini 客户端。

use std::sync::Arc;

use adk_rust::Llm;
use anyhow::{Result, bail};

#[cfg(feature = "gemini-provider")]
use adk_model::GeminiModel;

use crate::config::LlmConfig;

/// 构建 Gemini LLM 客户端。
pub(crate) fn build_gemini_llm(config: &LlmConfig) -> Result<Arc<dyn Llm>> {
    #[cfg(feature = "gemini-provider")]
    {
        Ok(Arc::new(GeminiModel::new(
            config.api_key.clone(),
            config.model.clone(),
        )?))
    }
    #[cfg(not(feature = "gemini-provider"))]
    {
        let _ = config;
        bail!("this binary was built without the `gemini-provider` feature");
    }
}
