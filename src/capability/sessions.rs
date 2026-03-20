//! 会话能力模块，负责暴露会话列表与历史消息查询能力。

use std::sync::Arc;

use crate::{
    engine::ToolCallEngine,
    session_store::{MessageView, SessionSummary},
};

/// 会话查询能力。
#[derive(Clone)]
pub struct SessionCapability {
    engine: Arc<ToolCallEngine>,
}

impl SessionCapability {
    /// 基于底层引擎创建会话查询能力。
    pub fn new(engine: Arc<ToolCallEngine>) -> Self {
        Self { engine }
    }

    /// 列出当前全部会话摘要。
    pub async fn list(&self) -> Vec<SessionSummary> {
        self.engine.list_sessions().await
    }

    /// 查询指定会话的历史消息。
    pub async fn history(&self, session_id: &str, limit: Option<usize>) -> Vec<MessageView> {
        self.engine.session_history(session_id, limit).await
    }
}
