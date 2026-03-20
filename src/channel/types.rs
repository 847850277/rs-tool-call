//! 通道公共类型模块，负责定义多通道共享的统一入站消息模型。

/// 当前系统支持或计划支持的通道类型。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Feishu,
    Dingtalk,
    Wecom,
}

impl ChannelKind {
    /// 返回通道类型对应的稳定字符串标识。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Feishu => "feishu",
            Self::Dingtalk => "dingtalk",
            Self::Wecom => "wecom",
        }
    }
}

/// 统一的入站文本消息模型。
/// 不同通道在解析后都应尽量收敛到这套字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundTextMessage {
    pub channel: ChannelKind,
    pub event_id: Option<String>,
    pub message_id: String,
    pub chat_id: Option<String>,
    pub chat_type: Option<String>,
    pub user_id: String,
    pub session_id: String,
    pub text: String,
}

/// 通用消息解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundMessageParseOutcome {
    NotMessageEvent,
    Ignored { reason: &'static str },
    Text(InboundTextMessage),
}

/// 统一的出站文本回复模型。
/// 不同通道在发送回复前都应尽量收敛到这套字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundTextReply {
    pub channel: ChannelKind,
    pub reply_to_message_id: String,
    pub session_id: String,
    pub text: String,
}
