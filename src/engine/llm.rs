//! LLM 子模块负责与模型通信，包括规划阶段调用和最终答案综合阶段调用。

use std::collections::HashMap;

use adk_rust::{Content, LlmRequest, LlmResponse, Part, futures::StreamExt};
use anyhow::{Result, anyhow};
use tracing::{debug, warn};

use super::{ToolCallEngine, append_stream_parts, finish_reason_to_string};
use crate::logging;

impl ToolCallEngine {
    /// 向模型发起一次规划请求，并把流式响应收敛成单个 `LlmResponse`。
    pub(crate) async fn collect_llm_response(&self, contents: Vec<Content>) -> Result<LlmResponse> {
        debug!(
            llm = self.llm.name(),
            input_message_count = contents.len(),
            tool_schema_count = self.registry.schemas().len(),
            "collecting planner response stream"
        );
        let mut request = LlmRequest::new(self.llm.name().to_string(), contents);
        request.tools = self.registry.schemas();

        logging::log_llm_request(&request.model, &request.contents, &request.tools, None);

        let mut stream = self.llm.generate_content(request, true).await?;
        let mut all_parts = Vec::new();
        let mut usage_metadata = None;
        let mut finish_reason = None;
        let mut saw_chunk = false;

        while let Some(item) = stream.next().await {
            let response = item?;
            saw_chunk = true;
            if let Some(content) = response.content {
                append_stream_parts(&mut all_parts, content.parts);
            }
            if usage_metadata.is_none() {
                usage_metadata = response.usage_metadata;
            }
            if let Some(reason) = response.finish_reason {
                finish_reason = Some(reason);
            }
        }

        if !saw_chunk {
            return Err(anyhow!("llm returned an empty response stream"));
        }

        let content = if all_parts.is_empty() {
            None
        } else {
            Some(Content {
                role: "model".to_string(),
                parts: all_parts,
            })
        };

        debug!(
            llm = self.llm.name(),
            finish_reason = ?finish_reason.as_ref().map(finish_reason_to_string),
            part_count = content.as_ref().map(|item| item.parts.len()).unwrap_or_default(),
            "assembled planner response stream"
        );

        logging::log_llm_response(
            self.llm.name(),
            finish_reason.as_ref().map(finish_reason_to_string),
            content.as_ref(),
            None,
        );

        Ok(LlmResponse {
            content,
            usage_metadata,
            finish_reason,
            citation_metadata: None,
            partial: false,
            turn_complete: true,
            interrupted: false,
            error_code: None,
            error_message: None,
        })
    }

    /// 在主循环未直接收敛时，请模型基于完整执行记录综合出最终答复。
    pub(crate) async fn synthesize_final_answer(&self, transcript: &[Content]) -> Option<Content> {
        let mut synthesis_transcript = transcript.to_vec();
        synthesis_transcript.push(Content::new("user").with_text(
            "Based on all the steps and tool results above, provide a clear and concise final answer to the original user request. Use the same language the user used. If you obtained valid data, present it directly. If all attempts failed, explain what went wrong and suggest alternatives. Do not mention iteration limits, internal engine details, or tool names.",
        ));
        let mut request = LlmRequest::new(self.llm.name().to_string(), synthesis_transcript);
        request.tools = HashMap::new();

        logging::log_llm_request(
            &request.model,
            &request.contents,
            &request.tools,
            Some("synthesis"),
        );

        match self.llm.generate_content(request, true).await {
            Ok(mut stream) => {
                let mut parts = Vec::new();
                while let Some(item) = stream.next().await {
                    if let Ok(response) = item {
                        if let Some(content) = response.content {
                            append_stream_parts(&mut parts, content.parts);
                        }
                    }
                }
                let text = parts
                    .iter()
                    .filter_map(|part| match part {
                        Part::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                let text = text.trim().to_string();
                let synthesis_content = if text.is_empty() {
                    None
                } else {
                    Some(Content::new("model").with_text(text))
                };
                logging::log_llm_response(
                    self.llm.name(),
                    None,
                    synthesis_content.as_ref(),
                    Some("synthesis"),
                );
                synthesis_content
            }
            Err(err) => {
                warn!(error = %err, "synthesis LLM call failed, using built-in fallback");
                None
            }
        }
    }
}
