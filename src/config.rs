use anyhow::{Context, Result, anyhow, bail};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_name: String,
    pub server_addr: String,
    pub default_system_prompt: String,
    pub max_iterations: usize,
    pub llm: LlmConfig,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    OpenAi,
    Gemini,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let provider = LlmProvider::from_env()?;
        let model =
            std::env::var("LLM_MODEL").unwrap_or_else(|_| provider.default_model().to_string());
        let base_url = std::env::var("OPENAI_BASE_URL")
            .ok()
            .or_else(|| std::env::var("LLM_BASE_URL").ok());

        let api_key = match provider {
            LlmProvider::OpenAi => std::env::var("OPENAI_API_KEY")
                .or_else(|_| std::env::var("LLM_API_KEY"))
                .context("missing OPENAI_API_KEY or LLM_API_KEY")?,
            LlmProvider::Gemini => std::env::var("GOOGLE_API_KEY")
                .or_else(|_| std::env::var("LLM_API_KEY"))
                .context("missing GOOGLE_API_KEY or LLM_API_KEY")?,
        };

        let max_iterations = match std::env::var("MAX_TOOL_ITERATIONS") {
            Ok(raw) => raw
                .parse::<usize>()
                .with_context(|| format!("invalid MAX_TOOL_ITERATIONS: {raw}"))?,
            Err(_) => 8,
        };
        if max_iterations == 0 {
            bail!("MAX_TOOL_ITERATIONS must be greater than 0");
        }

        Ok(Self {
            app_name: std::env::var("APP_NAME").unwrap_or_else(|_| "rs-tool-call".to_string()),
            server_addr: std::env::var("SERVER_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:7878".to_string()),
            default_system_prompt: std::env::var("SYSTEM_PROMPT").unwrap_or_else(|_| {
                "You are a tool-calling assistant inspired by OpenClaw. When deterministic work or external state is needed, call the available tools first, then synthesize a concise final answer.".to_string()
            }),
            max_iterations,
            llm: LlmConfig {
                provider,
                model,
                api_key,
                base_url,
            },
        })
    }
}

impl LlmProvider {
    fn from_env() -> Result<Self> {
        let raw = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string());
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::OpenAi),
            "gemini" => Ok(Self::Gemini),
            other => Err(anyhow!("unsupported LLM_PROVIDER: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-4o-mini",
            Self::Gemini => "gemini-2.5-flash",
        }
    }
}
