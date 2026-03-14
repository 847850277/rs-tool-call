use std::{collections::VecDeque, sync::Arc};

use adk_rust::{
    Content, FinishReason, FunctionResponseData, Llm, LlmRequest, LlmResponse, Part,
    futures::StreamExt,
};
use anyhow::{Result, anyhow};
use serde::Serialize;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    session_store::{MessageView, SessionStore},
    tools::{ToolExecutionFailure, ToolExecutionRequest, ToolExecutionResult, ToolRegistry},
};

pub struct ToolCallEngine {
    app_name: String,
    llm: Arc<dyn Llm>,
    registry: ToolRegistry,
    session_store: SessionStore,
    default_system_prompt: String,
    max_iterations: usize,
}

#[derive(Debug, Clone)]
pub struct ChatTurnRequest {
    pub session_id: String,
    pub user_id: String,
    pub message: String,
    pub system_prompt: Option<String>,
    pub max_iterations: Option<usize>,
    pub persist: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallTrace {
    pub function_call_id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub status: String,
    pub output: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTurnResponse {
    pub session_id: String,
    pub user_id: String,
    pub answer: String,
    pub finish_reason: Option<String>,
    pub iterations: usize,
    pub tool_calls: Vec<ToolCallTrace>,
    pub turn_messages: Vec<MessageView>,
    pub session_message_count: usize,
}

impl ToolCallEngine {
    pub fn new(
        app_name: String,
        llm: Arc<dyn Llm>,
        registry: ToolRegistry,
        session_store: SessionStore,
        default_system_prompt: String,
        max_iterations: usize,
    ) -> Self {
        Self {
            app_name,
            llm,
            registry,
            session_store,
            default_system_prompt,
            max_iterations,
        }
    }

    pub fn tools(&self) -> Vec<crate::tools::ToolDescriptor> {
        self.registry.descriptors()
    }

    pub async fn list_sessions(&self) -> Vec<crate::session_store::SessionSummary> {
        self.session_store.list().await
    }

    pub async fn session_history(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Vec<MessageView> {
        self.session_store.history(session_id, limit).await
    }

    pub async fn invoke_tool(
        &self,
        user_id: String,
        session_id: String,
        tool_name: String,
        args: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
        info!(
            session_id = %session_id,
            user_id = %user_id,
            tool = %tool_name,
            args_preview = %preview_json(&args, 200),
            "dispatching direct tool invocation"
        );
        self.registry
            .execute(ToolExecutionRequest {
                app_name: self.app_name.clone(),
                user_id,
                session_id,
                invocation_id: Uuid::new_v4().to_string(),
                function_call_id: Uuid::new_v4().to_string(),
                tool_name,
                args,
                user_content: Content::new("user").with_text("direct tool invocation"),
            })
            .await
    }

    pub async fn run_turn(&self, request: ChatTurnRequest) -> Result<ChatTurnResponse> {
        let system_prompt = request
            .system_prompt
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.default_system_prompt.clone());
        let max_iterations = request.max_iterations.unwrap_or(self.max_iterations);
        let user_content = Content::new("user").with_text(request.message.clone());

        let mut transcript = VecDeque::new();
        transcript.push_back(Content::new("system").with_text(system_prompt.clone()));
        for item in self.session_store.snapshot(&request.session_id).await {
            transcript.push_back(item);
        }
        transcript.push_back(user_content.clone());

        let invocation_id = Uuid::new_v4().to_string();
        let mut turn_messages = vec![user_content];
        let mut traces = Vec::new();
        let mut iterations = 0usize;
        let mut final_content = None;
        let mut finish_reason = None;

        info!(
            session_id = %request.session_id,
            user_id = %request.user_id,
            max_iterations,
            persisted_history = request.persist,
            prior_message_count = transcript.len().saturating_sub(2),
            "starting tool-call turn"
        );
        debug!(
            session_id = %request.session_id,
            system_prompt_preview = %preview_text(&system_prompt, 200),
            user_message_preview = %preview_text(&request.message, 200),
            "prepared turn transcript"
        );

        for index in 0..max_iterations {
            iterations = index + 1;
            info!(
                session_id = %request.session_id,
                user_id = %request.user_id,
                iteration = iterations,
                transcript_messages = transcript.len(),
                "requesting llm response"
            );
            let response = self
                .collect_llm_response(transcript.make_contiguous().to_vec())
                .await?;
            finish_reason = response.finish_reason;

            let model_content = response
                .content
                .unwrap_or_else(|| Content::new("model").with_text(""));
            let function_calls = extract_function_calls(&model_content);
            info!(
                session_id = %request.session_id,
                iteration = iterations,
                finish_reason = ?finish_reason.as_ref().map(finish_reason_to_string),
                function_call_count = function_calls.len(),
                model_text_preview = %preview_text(&extract_text(&model_content), 160),
                "received llm response"
            );
            transcript.push_back(model_content.clone());
            turn_messages.push(model_content.clone());

            if function_calls.is_empty() {
                final_content = Some(model_content);
                info!(
                    session_id = %request.session_id,
                    iteration = iterations,
                    "llm returned final answer without further tool calls"
                );
                break;
            }

            for function_call in function_calls {
                info!(
                    session_id = %request.session_id,
                    iteration = iterations,
                    function_call_id = %function_call.function_call_id,
                    tool = %function_call.name,
                    args_preview = %preview_json(&function_call.args, 200),
                    "dispatching tool call"
                );
                let tool_result = self
                    .dispatch_tool_call(
                        &request,
                        &invocation_id,
                        &function_call.function_call_id,
                        &function_call.name,
                        function_call.args.clone(),
                    )
                    .await;

                let (tool_trace, tool_content) = match tool_result {
                    Ok(result) => (
                        {
                            info!(
                                session_id = %request.session_id,
                                iteration = iterations,
                                function_call_id = %result.function_call_id,
                                tool = %result.tool_name,
                                output_preview = %preview_json(&result.output, 200),
                                "tool call completed"
                            );
                            ToolCallTrace {
                                function_call_id: result.function_call_id.clone(),
                                name: result.tool_name.clone(),
                                args: result.args.clone(),
                                status: "ok".to_string(),
                                output: result.output.clone(),
                            }
                        },
                        Content {
                            role: "tool".to_string(),
                            parts: vec![Part::FunctionResponse {
                                function_response: FunctionResponseData {
                                    name: result.tool_name,
                                    response: result.output,
                                },
                                id: Some(result.function_call_id),
                            }],
                        },
                    ),
                    Err(error) => {
                        warn!(
                            session_id = %request.session_id,
                            iteration = iterations,
                            function_call_id = %function_call.function_call_id,
                            tool = %function_call.name,
                            error = %error,
                            "tool call failed"
                        );
                        let failure = ToolExecutionFailure {
                            function_call_id: function_call.function_call_id.clone(),
                            tool_name: function_call.name.clone(),
                            args: function_call.args.clone(),
                            message: error.to_string(),
                        };
                        let payload = serde_json::json!({
                            "status": "error",
                            "message": failure.message,
                        });
                        (
                            ToolCallTrace {
                                function_call_id: failure.function_call_id.clone(),
                                name: failure.tool_name.clone(),
                                args: failure.args.clone(),
                                status: "error".to_string(),
                                output: payload.clone(),
                            },
                            Content {
                                role: "tool".to_string(),
                                parts: vec![Part::FunctionResponse {
                                    function_response: FunctionResponseData {
                                        name: failure.tool_name,
                                        response: payload,
                                    },
                                    id: Some(failure.function_call_id),
                                }],
                            },
                        )
                    }
                };

                traces.push(tool_trace);
                transcript.push_back(tool_content.clone());
                turn_messages.push(tool_content);
            }
        }

        let final_content = final_content.ok_or_else(|| {
            warn!(
                session_id = %request.session_id,
                user_id = %request.user_id,
                max_iterations,
                "tool-call loop reached max iterations without final answer"
            );
            anyhow!("tool-call loop reached max iterations without a final assistant answer")
        })?;

        if request.persist {
            self.session_store
                .append_many(&request.session_id, turn_messages.iter().cloned())
                .await;
            debug!(
                session_id = %request.session_id,
                appended_messages = turn_messages.len(),
                "persisted turn messages to session store"
            );
        }

        let answer = extract_text(&final_content);
        let session_message_count = if request.persist {
            self.session_store
                .session_message_count(&request.session_id)
                .await
        } else {
            turn_messages.len()
        };

        info!(
            session_id = %request.session_id,
            user_id = %request.user_id,
            iterations,
            tool_call_count = traces.len(),
            answer_preview = %preview_text(&answer, 200),
            session_message_count,
            "finished tool-call turn"
        );

        Ok(ChatTurnResponse {
            session_id: request.session_id,
            user_id: request.user_id,
            answer,
            finish_reason: finish_reason.as_ref().map(finish_reason_to_string),
            iterations,
            tool_calls: traces,
            turn_messages: turn_messages.iter().map(MessageView::from).collect(),
            session_message_count,
        })
    }

    async fn collect_llm_response(&self, contents: Vec<Content>) -> Result<LlmResponse> {
        debug!(
            llm = self.llm.name(),
            input_message_count = contents.len(),
            tool_schema_count = self.registry.schemas().len(),
            "collecting llm response stream"
        );
        let mut request = LlmRequest::new(self.llm.name().to_string(), contents);
        request.tools = self.registry.schemas();

        let mut stream = self.llm.generate_content(request, true).await?;
        let mut all_parts = Vec::new();
        let mut usage_metadata = None;
        let mut finish_reason = None;
        let mut saw_chunk = false;

        while let Some(item) = stream.next().await {
            let response = item?;
            saw_chunk = true;
            if let Some(content) = response.content {
                all_parts.extend(content.parts);
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
            "assembled llm response stream"
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

    async fn dispatch_tool_call(
        &self,
        request: &ChatTurnRequest,
        invocation_id: &str,
        function_call_id: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
        self.registry
            .execute(ToolExecutionRequest {
                app_name: self.app_name.clone(),
                user_id: request.user_id.clone(),
                session_id: request.session_id.clone(),
                invocation_id: invocation_id.to_string(),
                function_call_id: function_call_id.to_string(),
                tool_name: tool_name.to_string(),
                args,
                user_content: Content::new("user").with_text(request.message.clone()),
            })
            .await
    }

}

#[derive(Debug, Clone)]
struct FunctionCallEnvelope {
    function_call_id: String,
    name: String,
    args: serde_json::Value,
}

fn extract_function_calls(content: &Content) -> Vec<FunctionCallEnvelope> {
    content
        .parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| match part {
            Part::FunctionCall { name, args, id } => Some(FunctionCallEnvelope {
                function_call_id: id
                    .clone()
                    .unwrap_or_else(|| format!("call-{}-{index}", Uuid::new_v4())),
                name: name.clone(),
                args: args.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn extract_text(content: &Content) -> String {
    let text = content
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    text.trim().to_string()
}

fn finish_reason_to_string(reason: &FinishReason) -> String {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::MaxTokens => "max_tokens",
        FinishReason::Safety => "safety",
        FinishReason::Recitation => "recitation",
        FinishReason::Other => "other",
    }
    .to_string()
}

fn preview_text(input: &str, limit: usize) -> String {
    let mut preview = input.trim().replace('\n', "\\n");
    if preview.chars().count() > limit {
        preview = preview.chars().take(limit).collect::<String>();
        preview.push_str("...");
    }
    preview
}

fn preview_json(value: &serde_json::Value, limit: usize) -> String {
    preview_text(
        &serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
        limit,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use adk_rust::{Content, Llm, LlmRequest, LlmResponse, LlmResponseStream, Part, async_trait};

    use super::*;
    use crate::{session_store::SessionStore, tools::build_builtin_registry};

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
    async fn executes_tool_loop_and_persists_turn() {
        let llm = ScriptedLlm::new(vec![
            LlmResponse {
                content: Some(Content {
                    role: "model".to_string(),
                    parts: vec![Part::FunctionCall {
                        name: "math_add".to_string(),
                        args: serde_json::json!({"a": 2.0, "b": 3.0}),
                        id: Some("call_math".to_string()),
                    }],
                }),
                usage_metadata: None,
                finish_reason: Some(FinishReason::Stop),
                citation_metadata: None,
                partial: false,
                turn_complete: true,
                interrupted: false,
                error_code: None,
                error_message: None,
            },
            LlmResponse::new(Content::new("model").with_text("2 + 3 = 5")),
        ]);
        let store = SessionStore::default();
        let registry = build_builtin_registry(store.clone()).expect("registry");
        let engine = ToolCallEngine::new(
            "test-app".to_string(),
            Arc::new(llm),
            registry,
            store.clone(),
            "use tools".to_string(),
            4,
        );

        let response = engine
            .run_turn(ChatTurnRequest {
                session_id: "main".to_string(),
                user_id: "tester".to_string(),
                message: "what is 2 + 3?".to_string(),
                system_prompt: None,
                max_iterations: None,
                persist: true,
            })
            .await
            .expect("chat response");

        assert_eq!(response.answer, "2 + 3 = 5");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].status, "ok");

        let history = store.history("main", None).await;
        assert_eq!(history.len(), 4);
        assert_eq!(history[1].kind, "tool_call");
        assert_eq!(history[2].kind, "tool_result");
    }
}
