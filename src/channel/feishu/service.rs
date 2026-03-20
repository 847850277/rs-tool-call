//! 飞书服务编排模块，负责把飞书消息事件接到引擎回合并将结果回复给飞书。

use anyhow::Result;
use serde_json::{Value, json};

use crate::{
    capability::{ConversationCapability, ConversationRequest},
    channel::{InboundTextMessage, OutboundTextReply},
    config::FeishuCallbackConfig,
    logging,
};

use super::FeishuBotClient;

/// 处理一条统一入站文本消息，并通过飞书回复链路将结果回复回原消息。
pub async fn handle_text_message_event(
    conversation: ConversationCapability,
    config: FeishuCallbackConfig,
    event: InboundTextMessage,
) -> Result<()> {
    let response = conversation
        .execute(ConversationRequest {
            session_id: event.session_id.clone(),
            user_id: event.user_id.clone(),
            message: event.text.clone(),
            system_prompt: None,
            max_iterations: None,
            persist: true,
        })
        .await?;

    let answer = if response.answer.trim().is_empty() {
        "我暂时还没有合适的回复，请稍后再试。".to_string()
    } else {
        response.answer
    };
    let reply = OutboundTextReply {
        channel: event.channel,
        reply_to_message_id: event.message_id.clone(),
        session_id: event.session_id.clone(),
        text: answer,
    };

    FeishuBotClient::new(config).send_text_reply(&reply).await?;

    logging::log_channel_text_replied(
        reply.channel.as_str(),
        &reply.reply_to_message_id,
        &reply.session_id,
        &reply.text,
    );
    Ok(())
}

/// 返回飞书事件接收成功时的标准 ACK 响应。
pub fn callback_ack() -> Value {
    json!({
        "code": 0,
        "msg": "ok"
    })
}
