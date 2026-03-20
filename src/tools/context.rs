//! 工具上下文模块，负责把引擎层的工具请求转换为 ADK 工具上下文对象。

use std::sync::{Arc, Mutex};

use adk_rust::{
    AdkError, CallbackContext, Content, EventActions, ReadonlyContext, ToolContext, async_trait,
};

use super::ToolExecutionRequest;

/// 请求级工具上下文，用于在工具执行期间暴露会话、用户和动作状态。
pub(crate) struct RequestToolContext {
    invocation_id: String,
    agent_name: String,
    user_id: String,
    app_name: String,
    session_id: String,
    branch: String,
    user_content: Content,
    function_call_id: String,
    actions: Mutex<EventActions>,
}

impl RequestToolContext {
    /// 根据统一的工具执行请求构造上下文对象。
    pub(crate) fn new(request: &ToolExecutionRequest) -> Self {
        Self {
            invocation_id: request.invocation_id.clone(),
            agent_name: "tool-call-engine".to_string(),
            user_id: request.user_id.clone(),
            app_name: request.app_name.clone(),
            session_id: request.session_id.clone(),
            branch: String::new(),
            user_content: request.user_content.clone(),
            function_call_id: request.function_call_id.clone(),
            actions: Mutex::new(EventActions::default()),
        }
    }
}

#[async_trait]
impl ReadonlyContext for RequestToolContext {
    fn invocation_id(&self) -> &str {
        &self.invocation_id
    }

    fn agent_name(&self) -> &str {
        &self.agent_name
    }

    fn user_id(&self) -> &str {
        &self.user_id
    }

    fn app_name(&self) -> &str {
        &self.app_name
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn branch(&self) -> &str {
        &self.branch
    }

    fn user_content(&self) -> &Content {
        &self.user_content
    }
}

#[async_trait]
impl CallbackContext for RequestToolContext {
    fn artifacts(&self) -> Option<Arc<dyn adk_rust::Artifacts>> {
        None
    }
}

#[async_trait]
impl ToolContext for RequestToolContext {
    fn function_call_id(&self) -> &str {
        &self.function_call_id
    }

    fn actions(&self) -> EventActions {
        self.actions.lock().expect("tool actions poisoned").clone()
    }

    fn set_actions(&self, actions: EventActions) {
        *self.actions.lock().expect("tool actions poisoned") = actions;
    }

    async fn search_memory(
        &self,
        _query: &str,
    ) -> std::result::Result<Vec<adk_rust::MemoryEntry>, AdkError> {
        Ok(Vec::new())
    }
}
