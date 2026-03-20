//! 状态子模块负责维护单个回合内的临时执行状态，例如工具调用次数和重复调用检测窗口。

use std::collections::VecDeque;

use super::RECENT_TOOL_SIGNATURE_WINDOW;

#[derive(Debug, Default)]
pub(crate) struct TurnState {
    pub(crate) tool_calls_executed: usize,
    pub(crate) tool_errors: usize,
    recent_tool_signatures: VecDeque<String>,
}

impl TurnState {
    /// 将本轮刚执行过的工具签名压入最近窗口，用于后续重复检测。
    pub(crate) fn push_tool_signature(&mut self, signature: String) {
        if self.recent_tool_signatures.len() == RECENT_TOOL_SIGNATURE_WINDOW {
            self.recent_tool_signatures.pop_front();
        }
        self.recent_tool_signatures.push_back(signature);
    }

    /// 判断下一次调用是否与最近一次调用完全相同。
    pub(crate) fn would_repeat_exact(&self, signature: &str) -> bool {
        self.recent_tool_signatures
            .back()
            .map(|recent| recent == signature)
            .unwrap_or(false)
    }

    /// 判断是否出现 A/B/A 形式的来回抖动调用。
    pub(crate) fn would_ping_pong(&self, signature: &str) -> bool {
        if self.recent_tool_signatures.len() < 3 {
            return false;
        }

        let len = self.recent_tool_signatures.len();
        let a = &self.recent_tool_signatures[len - 3];
        let b = &self.recent_tool_signatures[len - 2];
        let c = &self.recent_tool_signatures[len - 1];
        a == c && b == signature && a != b
    }
}
