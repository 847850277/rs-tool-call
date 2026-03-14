use std::sync::Arc;

use adk_model::openai::{OpenAIClient, OpenAIConfig};
use adk_rust::Llm;
use anyhow::{Result, bail};

#[cfg(feature = "gemini-provider")]
use adk_model::GeminiModel;

use crate::config::{LlmConfig, LlmProvider};

pub fn build_llm(config: &LlmConfig) -> Result<Arc<dyn Llm>> {
    let model: Arc<dyn Llm> = match config.provider {
        LlmProvider::OpenAi | LlmProvider::Glm | LlmProvider::SiliconFlow => {
            Arc::new(OpenAIClient::new(OpenAIConfig {
                api_key: config.api_key.clone(),
                model: config.model.clone(),
                organization_id: None,
                project_id: None,
                base_url: config.base_url.clone(),
            })?)
        }
        LlmProvider::Gemini => {
            #[cfg(feature = "gemini-provider")]
            {
                Arc::new(GeminiModel::new(
                    config.api_key.clone(),
                    config.model.clone(),
                )?)
            }
            #[cfg(not(feature = "gemini-provider"))]
            {
                bail!("this binary was built without the `gemini-provider` feature");
            }
        }
    };

    Ok(model)
}
