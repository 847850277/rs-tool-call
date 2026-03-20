//! 聊天能力模块，负责对外暴露一次完整文本回合执行能力。

use std::sync::Arc;

use anyhow::Result;

use crate::engine::{ChatTurnRequest, ChatTurnResponse, ToolCallEngine};

/// 对话能力输入参数。
#[derive(Debug, Clone)]
pub struct ConversationRequest {
    pub session_id: String,
    pub user_id: String,
    pub message: String,
    pub system_prompt: Option<String>,
    pub max_iterations: Option<usize>,
    pub persist: bool,
}

/// 对话能力，负责驱动一次完整的 plan-execute-observe 回合。
#[derive(Clone)]
pub struct ConversationCapability {
    engine: Arc<ToolCallEngine>,
}

impl ConversationCapability {
    /// 基于底层引擎创建对话能力。
    pub fn new(engine: Arc<ToolCallEngine>) -> Self {
        Self { engine }
    }

    /// 执行一次对话请求，并返回完整回合结果。
    pub async fn execute(&self, request: ConversationRequest) -> Result<ChatTurnResponse> {
        self.engine
            .run_turn(ChatTurnRequest {
                session_id: request.session_id,
                user_id: request.user_id,
                message: request.message,
                system_prompt: request.system_prompt,
                max_iterations: request.max_iterations,
                persist: request.persist,
            })
            .await
    }
}
