//! LLM 配置模块，负责描述模型提供方、默认模型和 OpenAI 兼容端点配置。

use anyhow::{Result, anyhow};

/// 大模型配置，描述当前 provider、模型名称、鉴权信息和可选 base URL。
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

/// 当前支持的 LLM 提供方枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    OpenAi,
    Glm,
    SiliconFlow,
    Gemini,
}

impl LlmProvider {
    /// 从环境变量中解析 provider 类型。
    pub(crate) fn from_env() -> Result<Self> {
        let raw = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string());
        Self::parse(&raw)
    }

    /// 将字符串 provider 名称解析成内部枚举。
    pub(crate) fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::OpenAi),
            "glm" | "bailian" | "dashscope" => Ok(Self::Glm),
            "siliconflow" | "silicon-flow" | "silicon" => Ok(Self::SiliconFlow),
            "gemini" => Ok(Self::Gemini),
            other => Err(anyhow!("unsupported LLM_PROVIDER: {other}")),
        }
    }

    /// 返回 provider 对外展示时使用的稳定字符串名称。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Glm => "glm",
            Self::SiliconFlow => "siliconflow",
            Self::Gemini => "gemini",
        }
    }

    /// 返回 provider 的默认模型名称。
    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-4o-mini",
            Self::Glm => "glm-5",
            Self::SiliconFlow => "zai-org/GLM-4.6",
            Self::Gemini => "gemini-2.5-flash",
        }
    }

    /// 返回 provider 的默认 OpenAI 兼容 base URL。
    pub(crate) fn default_base_url(self) -> Option<&'static str> {
        match self {
            Self::OpenAi => None,
            Self::Glm => Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
            Self::SiliconFlow => Some("https://api.siliconflow.cn/v1"),
            Self::Gemini => None,
        }
    }

    /// 返回 provider 支持的 API Key 环境变量名列表。
    pub(crate) fn api_key_envs(self) -> &'static [&'static str] {
        match self {
            Self::OpenAi => &["OPENAI_API_KEY", "LLM_API_KEY"],
            Self::Glm => &[
                "DASHSCOPE_API_KEY",
                "BAILIAN_API_KEY",
                "GLM_API_KEY",
                "LLM_API_KEY",
            ],
            Self::SiliconFlow => &["SILICONFLOW_API_KEY", "OPENAI_API_KEY", "LLM_API_KEY"],
            Self::Gemini => &["GOOGLE_API_KEY", "LLM_API_KEY"],
        }
    }

    /// 返回 provider 支持的 Base URL 环境变量名列表。
    pub(crate) fn base_url_envs(self) -> &'static [&'static str] {
        match self {
            Self::OpenAi => &["OPENAI_BASE_URL", "LLM_BASE_URL"],
            Self::Glm => &[
                "DASHSCOPE_BASE_URL",
                "BAILIAN_BASE_URL",
                "GLM_BASE_URL",
                "OPENAI_BASE_URL",
                "LLM_BASE_URL",
            ],
            Self::SiliconFlow => &["SILICONFLOW_BASE_URL", "OPENAI_BASE_URL", "LLM_BASE_URL"],
            Self::Gemini => &["LLM_BASE_URL"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LlmProvider;

    #[test]
    fn glm_aliases_are_supported() {
        for raw in ["glm", "bailian", "dashscope"] {
            let provider = LlmProvider::parse(raw).expect("glm alias");
            assert_eq!(provider.as_str(), "glm");
        }
    }

    #[test]
    fn glm_defaults_match_bailian_openai_compatible_endpoint() {
        assert_eq!(LlmProvider::Glm.default_model(), "glm-5");
        assert_eq!(
            LlmProvider::Glm.default_base_url(),
            Some("https://dashscope.aliyuncs.com/compatible-mode/v1")
        );
    }

    #[test]
    fn siliconflow_aliases_are_supported() {
        for raw in ["siliconflow", "silicon-flow", "silicon"] {
            let provider = LlmProvider::parse(raw).expect("siliconflow alias");
            assert_eq!(provider.as_str(), "siliconflow");
        }
    }

    #[test]
    fn siliconflow_defaults_match_openai_compatible_endpoint() {
        assert_eq!(LlmProvider::SiliconFlow.default_model(), "zai-org/GLM-4.6");
        assert_eq!(
            LlmProvider::SiliconFlow.default_base_url(),
            Some("https://api.siliconflow.cn/v1")
        );
    }
}
