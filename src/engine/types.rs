//! 类型子模块集中定义引擎对外响应结构和内部规划动作结构。

use serde::Serialize;
use serde_json::Value;

use crate::session_store::MessageView;

/// 一次聊天回合的输入参数。
#[derive(Debug, Clone)]
pub struct ChatTurnRequest {
    pub session_id: String,
    pub user_id: String,
    pub message: String,
    pub system_prompt: Option<String>,
    pub max_iterations: Option<usize>,
    pub persist: bool,
}

/// 单次工具调用的追踪结果。
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallTrace {
    pub function_call_id: String,
    pub name: String,
    pub args: Value,
    pub status: String,
    pub output: Value,
}

/// 单次规划过程中某个候选动作的可观测信息。
#[derive(Debug, Clone, Serialize)]
pub struct PlanningCandidateTrace {
    pub label: String,
    pub action_type: String,
    pub preview: String,
    pub selected: bool,
    pub reason: String,
}

/// 单轮规划与执行的追踪信息。
#[derive(Debug, Clone, Serialize)]
pub struct PlanningStepTrace {
    pub iteration: usize,
    pub selected_action: String,
    pub selection_reason: String,
    pub observation: Option<String>,
    pub candidates: Vec<PlanningCandidateTrace>,
}

/// 一次聊天回合的完整输出。
#[derive(Debug, Clone, Serialize)]
pub struct ChatTurnResponse {
    pub session_id: String,
    pub user_id: String,
    pub answer: String,
    pub finish_reason: Option<String>,
    pub iterations: usize,
    pub tool_calls: Vec<ToolCallTrace>,
    pub planning_steps: Vec<PlanningStepTrace>,
    pub turn_messages: Vec<MessageView>,
    pub session_message_count: usize,
}

/// 对模型函数调用片段的内部包装，统一保留调用 ID、名称和参数。
#[derive(Debug, Clone)]
pub(crate) struct FunctionCallEnvelope {
    pub(crate) function_call_id: String,
    pub(crate) name: String,
    pub(crate) args: Value,
}

/// 规划器可提交的动作枚举。
#[derive(Debug, Clone)]
pub(crate) enum PlannedAction {
    CallTool(FunctionCallEnvelope),
    Answer { text: String },
    AskUser { question: String },
}

/// 单个候选动作及其来源说明。
#[derive(Debug, Clone)]
pub(crate) struct ActionCandidate {
    pub(crate) label: String,
    pub(crate) reason: String,
    pub(crate) action: PlannedAction,
}

/// 规划器最终选中的动作及对应的候选追踪信息。
#[derive(Debug, Clone)]
pub(crate) struct SelectedAction {
    pub(crate) action: PlannedAction,
    pub(crate) selected_preview: String,
    pub(crate) selection_reason: String,
    pub(crate) candidate_traces: Vec<PlanningCandidateTrace>,
}
