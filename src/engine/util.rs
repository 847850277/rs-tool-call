//! 工具子模块负责引擎内部的公共辅助函数，例如内容抽取、预览文本和签名生成。

use adk_rust::{Content, FinishReason, Part};
use serde_json::Value;
use uuid::Uuid;

use super::{FunctionCallEnvelope, PlannedAction};

/// 把内部函数调用包装成模型可见的 `FunctionCall` 内容片段。
pub(crate) fn build_model_tool_call_content(function_call: &FunctionCallEnvelope) -> Content {
    Content {
        role: "model".to_string(),
        parts: vec![Part::FunctionCall {
            name: function_call.name.clone(),
            args: function_call.args.clone(),
            id: Some(function_call.function_call_id.clone()),
            thought_signature: None,
        }],
    }
}

/// 返回候选动作的类型标签，便于追踪和日志记录。
pub(crate) fn candidate_action_type(action: &PlannedAction) -> &'static str {
    match action {
        PlannedAction::CallTool(_) => "call_tool",
        PlannedAction::Answer { .. } => "answer",
        PlannedAction::AskUser { .. } => "ask_user",
    }
}

/// 生成候选动作的简短预览文本，便于日志和调试输出。
pub(crate) fn candidate_preview(action: &PlannedAction) -> String {
    match action {
        PlannedAction::CallTool(function_call) => format!(
            "{}({})",
            function_call.name,
            preview_json(&function_call.args, 120)
        ),
        PlannedAction::Answer { text } => preview_text(text, 160),
        PlannedAction::AskUser { question } => preview_text(question, 160),
    }
}

/// 根据工具名和参数生成稳定签名，用于重复调用检测。
pub(crate) fn tool_call_signature(name: &str, args: &Value) -> String {
    format!(
        "{}:{}",
        name,
        serde_json::to_string(args).unwrap_or_else(|_| "<invalid-json>".to_string())
    )
}

/// 从模型返回内容中提取所有函数调用片段。
pub(crate) fn extract_function_calls(content: &Content) -> Vec<FunctionCallEnvelope> {
    content
        .parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| match part {
            Part::FunctionCall { name, args, id, .. } => Some(FunctionCallEnvelope {
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

/// 从内容片段中提取纯文本并做首尾裁剪。
pub(crate) fn extract_text(content: &Content) -> String {
    let text = content
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    text.trim().to_string()
}

/// 合并流式返回的分片文本，避免多段文本之间被错误插入换行。
pub(crate) fn append_stream_parts(target: &mut Vec<Part>, incoming: Vec<Part>) {
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

/// 把 SDK 的结束原因枚举转换成更稳定的字符串表示。
pub(crate) fn finish_reason_to_string(reason: &FinishReason) -> String {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::MaxTokens => "max_tokens",
        FinishReason::Safety => "safety",
        FinishReason::Recitation => "recitation",
        FinishReason::Other => "other",
    }
    .to_string()
}

/// 生成适合日志输出的文本预览。
pub(crate) fn preview_text(input: &str, limit: usize) -> String {
    let mut preview = input.trim().replace('\n', "\\n");
    if preview.chars().count() > limit {
        preview = preview.chars().take(limit).collect::<String>();
        preview.push_str("...");
    }
    preview
}

/// 生成适合日志输出的 JSON 预览。
pub(crate) fn preview_json(value: &Value, limit: usize) -> String {
    preview_text(
        &serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
        limit,
    )
}
