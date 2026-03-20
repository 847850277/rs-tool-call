//! 结构化抽取能力模块，负责在不进入 tool loop 的前提下执行单轮结构化信息抽取。

use std::{collections::HashMap, sync::Arc};

use adk_rust::{Content, Llm, LlmRequest, Part, futures::StreamExt};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::logging;

const EXTRACTION_PHASE: &str = "structured_extraction";

/// 结构化抽取请求。
#[derive(Debug, Clone)]
pub struct StructuredExtractionRequest {
    pub schema: Value,
    pub input_text: String,
    pub schema_name: Option<String>,
    pub instructions: Option<String>,
}

/// 结构化抽取响应。
#[derive(Debug, Clone)]
pub struct StructuredExtractionResponse {
    pub data: Value,
    pub raw_model_text: String,
}

/// 结构化抽取能力。
/// 该能力直接调用底层 LLM，不会进入现有的工具调用主循环。
#[derive(Clone)]
pub struct StructuredExtractionCapability {
    llm: Arc<dyn Llm>,
}

impl StructuredExtractionCapability {
    /// 基于底层 LLM 创建结构化抽取能力。
    pub fn new(llm: Arc<dyn Llm>) -> Self {
        Self { llm }
    }

    /// 执行单轮结构化抽取，并返回解析后的 JSON 数据。
    pub async fn execute(
        &self,
        request: StructuredExtractionRequest,
    ) -> Result<StructuredExtractionResponse> {
        let contents = vec![
            Content::new("system").with_text(build_extraction_system_prompt()),
            Content::new("user").with_text(build_extraction_user_prompt(&request)),
        ];

        let mut llm_request = LlmRequest::new(self.llm.name().to_string(), contents);
        llm_request.tools = HashMap::new();
        logging::log_llm_request(
            &llm_request.model,
            &llm_request.contents,
            &llm_request.tools,
            Some(EXTRACTION_PHASE),
        );

        let mut stream = self.llm.generate_content(llm_request, true).await?;
        let mut parts = Vec::new();
        let mut saw_chunk = false;

        while let Some(item) = stream.next().await {
            let response = item?;
            saw_chunk = true;
            if let Some(content) = response.content {
                append_text_parts(&mut parts, content.parts);
            }
        }

        if !saw_chunk {
            bail!("llm returned an empty response stream");
        }

        let raw_model_text = parts
            .iter()
            .filter_map(|part| match part {
                Part::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();

        let response_content = if raw_model_text.is_empty() {
            None
        } else {
            Some(Content::new("model").with_text(raw_model_text.clone()))
        };
        logging::log_llm_response(
            self.llm.name(),
            None,
            response_content.as_ref(),
            Some(EXTRACTION_PHASE),
        );

        let data = parse_json_response(&raw_model_text).with_context(|| {
            format!("failed to parse structured extraction response as JSON; raw={raw_model_text}")
        })?;

        Ok(StructuredExtractionResponse {
            data,
            raw_model_text,
        })
    }
}

/// 构造结构化抽取专用系统提示词。
fn build_extraction_system_prompt() -> &'static str {
    "You are a structured information extraction assistant. Read the provided source text and return exactly one valid JSON value that matches the provided JSON schema. Do not call tools. Do not add markdown fences, explanations, or extra prose. Extract only information supported by the source text. If a value is missing or uncertain, prefer null when that keeps the output valid. Keep keys exactly as requested."
}

/// 构造结构化抽取的用户提示词。
fn build_extraction_user_prompt(request: &StructuredExtractionRequest) -> String {
    let schema_name = request
        .schema_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("structured_extraction");
    let instructions = request
        .instructions
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("No additional instructions.");

    format!(
        "Task: {schema_name}\nAdditional instructions:\n{instructions}\n\nJSON schema:\n{}\n\nSource text:\n{}\n\nReturn only valid JSON.",
        serde_json::to_string_pretty(&request.schema)
            .unwrap_or_else(|_| request.schema.to_string()),
        request.input_text.trim()
    )
}

/// 合并流式文本片段，避免相邻文本块之间被错误分隔。
fn append_text_parts(target: &mut Vec<Part>, incoming: Vec<Part>) {
    for part in incoming {
        match part {
            Part::Text { text } => {
                if let Some(Part::Text { text: current }) = target.last_mut() {
                    current.push_str(&text);
                } else {
                    target.push(Part::Text { text });
                }
            }
            other => target.push(other),
        }
    }
}

/// 解析模型返回的文本，尽量从中提取出一个合法的 JSON 值。
fn parse_json_response(raw: &str) -> Result<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("structured extraction response is empty");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    if let Some(candidate) = extract_json_code_fence(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
            return Ok(value);
        }
    }

    if let Some(candidate) = extract_first_balanced_json(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
            return Ok(value);
        }
    }

    Err(anyhow!("no valid JSON payload found in model response"))
}

/// 从 Markdown 代码块中提取 JSON 内容。
fn extract_json_code_fence(text: &str) -> Option<String> {
    let fence_start = text.find("```")?;
    let after_start = &text[fence_start + 3..];
    let fence_end = after_start.find("```")?;
    let fenced = after_start[..fence_end].trim();
    let fenced = fenced
        .strip_prefix("json")
        .map(str::trim_start)
        .unwrap_or(fenced);
    if fenced.is_empty() {
        None
    } else {
        Some(fenced.to_string())
    }
}

/// 从文本中提取第一个括号平衡的 JSON 对象或数组。
fn extract_first_balanced_json(text: &str) -> Option<String> {
    let start = text.find(['{', '['])?;
    let bytes = text.as_bytes();
    let mut in_string = false;
    let mut escaped = false;
    let mut stack: Vec<u8> = Vec::new();

    for (offset, byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match *byte {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match *byte {
            b'"' => in_string = true,
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'}' | b']' => {
                let expected = stack.pop()?;
                if *byte != expected {
                    return None;
                }
                if stack.is_empty() {
                    let end = start + offset + 1;
                    return Some(text[start..end].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use adk_rust::{Content, Llm, LlmRequest, LlmResponse, LlmResponseStream, async_trait};

    use super::*;

    struct ScriptedLlm {
        responses: Mutex<VecDeque<LlmResponse>>,
    }

    impl ScriptedLlm {
        fn new(responses: Vec<LlmResponse>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl Llm for ScriptedLlm {
        fn name(&self) -> &str {
            "scripted-llm"
        }

        async fn generate_content(
            &self,
            _req: LlmRequest,
            _stream: bool,
        ) -> adk_rust::Result<LlmResponseStream> {
            let next = self
                .responses
                .lock()
                .expect("scripted llm poisoned")
                .pop_front()
                .expect("missing scripted response");
            let stream = adk_rust::futures::stream::once(async move { Ok(next) });
            Ok(Box::pin(stream))
        }
    }

    #[tokio::test]
    async fn extraction_capability_parses_direct_json() {
        let capability = StructuredExtractionCapability::new(Arc::new(ScriptedLlm::new(vec![
            LlmResponse::new(
                Content::new("model").with_text("{\"gender\":\"男\",\"age\":32,\"name\":\"张三\"}"),
            ),
        ])));

        let response = capability
            .execute(StructuredExtractionRequest {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "gender": {"type": "string"},
                        "age": {"type": "integer"}
                    }
                }),
                input_text: "我叫张三，男，32岁".to_string(),
                schema_name: Some("basic_profile".to_string()),
                instructions: None,
            })
            .await
            .expect("structured extraction should succeed");

        assert_eq!(response.data["name"], "张三");
        assert_eq!(response.data["gender"], "男");
        assert_eq!(response.data["age"], 32);
        assert_eq!(
            response.raw_model_text,
            "{\"gender\":\"男\",\"age\":32,\"name\":\"张三\"}"
        );
    }

    #[test]
    fn parse_json_response_extracts_json_from_fenced_block() {
        let value = parse_json_response("```json\n{\"name\":\"张三\",\"age\":32}\n```")
            .expect("json fenced block should parse");

        assert_eq!(value["name"], "张三");
        assert_eq!(value["age"], 32);
    }
}
