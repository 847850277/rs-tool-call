//! LLM 日志模块，负责记录模型请求和响应的结构化内容。

use std::collections::HashMap;

use adk_rust::Content;
use serde_json::{Value, json};
use tracing::info;

/// 记录一次完整的 LLM 请求，包括消息数量、工具数量和原始内容。
pub fn log_llm_request(
    model: &str,
    contents: &[Content],
    tools: &HashMap<String, Value>,
    phase: Option<&str>,
) {
    let mut entry = json!({
        "type": "llm_request",
        "model": model,
        "message_count": contents.len(),
        "tool_count": tools.len(),
        "tools": tools,
        "contents": contents,
    });
    if let Some(p) = phase {
        entry["phase"] = json!(p);
    }
    info!(
        "{}",
        serde_json::to_string_pretty(&entry).unwrap_or_default()
    );
}

/// 记录一次完整的 LLM 响应。
pub fn log_llm_response(
    model: &str,
    finish_reason: Option<String>,
    content: Option<&Content>,
    phase: Option<&str>,
) {
    let mut entry = json!({
        "type": "llm_response",
        "model": model,
        "finish_reason": finish_reason,
        "content": content,
    });
    if let Some(p) = phase {
        entry["phase"] = json!(p);
    }
    info!(
        "{}",
        serde_json::to_string_pretty(&entry).unwrap_or_default()
    );
}
