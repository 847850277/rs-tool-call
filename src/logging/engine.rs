//! 引擎日志模块，负责记录 plan-execute loop 每一步的提交动作。

use serde_json::{Value, json};
use tracing::info;

/// 记录引擎在某轮中直接返回最终答案。
pub fn log_chain_step_answer(
    session_id: &str,
    iteration: usize,
    text: &str,
    selection_reason: &str,
) {
    let entry = json!({
        "type": "chain_step",
        "session_id": session_id,
        "iteration": iteration,
        "action": "answer",
        "text": text,
        "selection_reason": selection_reason,
    });
    info!(
        "{}",
        serde_json::to_string_pretty(&entry).unwrap_or_default()
    );
}

/// 记录引擎在某轮中选择向用户追问。
pub fn log_chain_step_ask_user(
    session_id: &str,
    iteration: usize,
    question: &str,
    selection_reason: &str,
) {
    let entry = json!({
        "type": "chain_step",
        "session_id": session_id,
        "iteration": iteration,
        "action": "ask_user",
        "question": question,
        "selection_reason": selection_reason,
    });
    info!(
        "{}",
        serde_json::to_string_pretty(&entry).unwrap_or_default()
    );
}

/// 记录引擎在某轮中执行工具调用。
pub fn log_chain_step_tool(
    session_id: &str,
    iteration: usize,
    name: &str,
    args: &Value,
    status: &str,
    output: &Value,
    selection_reason: &str,
) {
    let entry = json!({
        "type": "chain_step",
        "session_id": session_id,
        "iteration": iteration,
        "action": "call_tool",
        "tool": name,
        "args": args,
        "status": status,
        "output": output,
        "selection_reason": selection_reason,
    });
    info!(
        "{}",
        serde_json::to_string_pretty(&entry).unwrap_or_default()
    );
}
