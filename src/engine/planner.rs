//! 规划子模块负责把模型输出整理成候选动作，并在 guard 约束下选出本轮唯一提交动作。

use std::collections::HashSet;

use adk_rust::Content;
use serde_json::json;
use uuid::Uuid;

use super::{
    ActionCandidate, ChatTurnRequest, DEFAULT_HISTORY_PROBE_LIMIT, FunctionCallEnvelope,
    PlannedAction, PlanningCandidateTrace, SelectedAction, ToolCallEngine, ToolCallTrace,
    TurnState, candidate_action_type, candidate_preview, extract_function_calls, extract_text,
    preview_json, tool_call_signature,
};

impl ToolCallEngine {
    /// 生成规划器使用的系统提示词，在基础 prompt 上追加当前 loop 的执行规则。
    pub(crate) fn build_planner_system_prompt(&self, base: &str) -> String {
        format!(
            "{base}\n\nLoop policy:\n- Work as an iterative plan-execute-observe loop.\n- Plan only the next action, never a long chain.\n- Consider up to {} immediate next-step directions before committing one.\n- Commit at most one tool call per iteration.\n- After each tool result, re-plan from the updated transcript.\n- Avoid repeating the same tool with the same arguments.\n- Only return a final answer when you have verified the result is correct and meaningful, not just that a command ran successfully.\n- If a tool result is an error or indicates service unavailability, try a different approach rather than returning that as the answer.\n- Do not spend turns probing whether curl, wget, nc, python, or similar binaries exist unless the user explicitly asked to debug the server environment.\n- If more user input is required, ask one concise clarification question.\n- If the answer is ready, return the final answer directly.",
            self.planner_candidate_limit
        )
    }

    /// 根据模型当前输出构造“下一步动作候选集”。
    /// 候选集只关注下一步，而不是整条长链。
    pub(crate) fn plan_candidates(
        &self,
        request: &ChatTurnRequest,
        model_content: &Content,
        has_prior_history: bool,
    ) -> Vec<ActionCandidate> {
        let mut candidates = Vec::new();
        let mut seen_signatures = HashSet::new();

        for function_call in extract_function_calls(model_content) {
            let signature = tool_call_signature(&function_call.name, &function_call.args);
            if !seen_signatures.insert(signature) {
                continue;
            }

            candidates.push(ActionCandidate {
                label: format!("tool:{}", function_call.name),
                reason: "model proposed an immediate tool step".to_string(),
                action: PlannedAction::CallTool(function_call),
            });
            if candidates.len() >= self.planner_candidate_limit {
                return candidates;
            }
        }

        let text = extract_text(model_content);
        if !text.is_empty() && candidates.len() < self.planner_candidate_limit {
            candidates.push(ActionCandidate {
                label: "answer:direct".to_string(),
                reason: "model produced a direct answer candidate".to_string(),
                action: PlannedAction::Answer { text },
            });
        }

        if has_prior_history
            && candidates.len() < self.planner_candidate_limit
            && self.registry.has("sessions_history")
        {
            let args = json!({
                "session_id": request.session_id,
                "limit": DEFAULT_HISTORY_PROBE_LIMIT,
            });
            let signature = tool_call_signature("sessions_history", &args);
            if seen_signatures.insert(signature) {
                candidates.push(ActionCandidate {
                    label: "context:history".to_string(),
                    reason: "default context branch that inspects committed session history before another action"
                        .to_string(),
                    action: PlannedAction::CallTool(FunctionCallEnvelope {
                        function_call_id: format!("call-{}", Uuid::new_v4()),
                        name: "sessions_history".to_string(),
                        args,
                    }),
                });
            }
        }

        if candidates.len() < self.planner_candidate_limit {
            candidates.push(ActionCandidate {
                label: "clarify:user".to_string(),
                reason: "default clarification branch when no safer committed step remains"
                    .to_string(),
                action: PlannedAction::AskUser {
                    question:
                        "I need a bit more context to continue safely. What exact result should the next step produce?"
                            .to_string(),
                },
            });
        }

        candidates.truncate(self.planner_candidate_limit);
        candidates
    }

    /// 在候选动作中选择本轮真正提交的动作，并记录未选中原因用于追踪。
    pub(crate) fn select_action(
        &self,
        candidates: Vec<ActionCandidate>,
        state: &TurnState,
    ) -> SelectedAction {
        let mut rejections = vec![String::new(); candidates.len()];
        let mut fallback_ask_user_idx = None;
        let mut selected_idx = None;
        let mut selection_reason = String::new();

        for (index, candidate) in candidates.iter().enumerate() {
            if selected_idx.is_some() {
                break;
            }
            match &candidate.action {
                PlannedAction::CallTool(function_call) => {
                    if state.tool_calls_executed >= self.max_tool_calls_per_turn {
                        rejections[index] = "rejected by max_tool_calls_per_turn guard".to_string();
                        continue;
                    }

                    let signature = tool_call_signature(&function_call.name, &function_call.args);
                    if state.would_repeat_exact(&signature) {
                        rejections[index] =
                            "rejected by repeated-call detection (same tool + same args)"
                                .to_string();
                        continue;
                    }

                    if state.would_ping_pong(&signature) {
                        rejections[index] =
                            "rejected by repeated-call detection (A/B ping-pong)".to_string();
                        continue;
                    }

                    selected_idx = Some(index);
                    selection_reason = format!(
                        "{}; selected the first viable committed tool step",
                        candidate.reason
                    );
                    break;
                }
                PlannedAction::Answer { text } => {
                    if text.trim().is_empty() {
                        rejections[index] = "rejected because the answer text is empty".to_string();
                        continue;
                    }

                    selected_idx = Some(index);
                    selection_reason = format!(
                        "{}; selected the direct answer because no earlier viable tool candidate won",
                        candidate.reason
                    );
                    break;
                }
                PlannedAction::AskUser { question } => {
                    if question.trim().is_empty() {
                        rejections[index] =
                            "rejected because the clarification question is empty".to_string();
                        continue;
                    }
                    fallback_ask_user_idx = Some(index);
                    rejections[index] = "kept as a fallback clarification branch".to_string();
                }
            }
        }

        if selected_idx.is_none() {
            selected_idx = fallback_ask_user_idx;
            if selected_idx.is_some() {
                selection_reason =
                    "selected the clarification branch because no safe tool or final answer candidate remained"
                        .to_string();
            }
        }

        let selected_idx = selected_idx.unwrap_or(0);
        let selected_preview = candidate_preview(&candidates[selected_idx].action);
        let selected_action = candidates[selected_idx].action.clone();
        let candidate_traces = candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| PlanningCandidateTrace {
                label: candidate.label.clone(),
                action_type: candidate_action_type(&candidate.action).to_string(),
                preview: candidate_preview(&candidate.action),
                selected: index == selected_idx,
                reason: if index == selected_idx {
                    selection_reason.clone()
                } else if rejections[index].is_empty() {
                    "not selected because an earlier candidate won".to_string()
                } else {
                    rejections[index].clone()
                },
            })
            .collect();

        SelectedAction {
            action: selected_action,
            selected_preview,
            selection_reason,
            candidate_traces,
        }
    }

    /// 在回合无法正常收敛时，构造一个可直接返回给用户的兜底回答。
    pub(crate) fn build_fallback_content(
        &self,
        reason: &str,
        tool_traces: &[ToolCallTrace],
    ) -> Content {
        let summary = if tool_traces.is_empty() {
            format!("I stopped because {reason}. Please clarify the next objective.")
        } else {
            let recent = tool_traces
                .iter()
                .rev()
                .take(2)
                .map(|trace| format!("{} => {}", trace.name, preview_json(&trace.output, 120)))
                .collect::<Vec<_>>()
                .join("; ");
            format!("I stopped because {reason}. Recent tool observations: {recent}")
        };
        Content::new("model").with_text(summary)
    }
}
