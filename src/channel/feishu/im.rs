//! 飞书 IM 模块，负责文本消息事件解析、消息清洗以及回复 API 调用。

use anyhow::{Result, anyhow, bail};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    channel::{ChannelKind, InboundMessageParseOutcome, InboundTextMessage, OutboundTextReply},
    config::FeishuCallbackConfig,
    logging,
};

use super::callback::extract_event_type;

/// 飞书机器人客户端，负责获取 tenant token 并回复消息。
#[derive(Clone)]
pub struct FeishuBotClient {
    http: Client,
    config: FeishuCallbackConfig,
}

#[derive(Debug, Deserialize)]
struct TenantAccessTokenResponse {
    code: i64,
    msg: Option<String>,
    tenant_access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeishuApiResponse {
    code: i64,
    msg: Option<String>,
}

impl FeishuBotClient {
    /// 创建一个飞书机器人客户端实例。
    pub fn new(config: FeishuCallbackConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    /// 发送统一出站文本回复，内部会自动完成 access token 获取和文本格式整理。
    pub async fn send_text_reply(&self, reply: &OutboundTextReply) -> Result<()> {
        let formatted = format_reply_text_for_feishu(&reply.text);
        logging::log_channel_reply_stage(
            reply.channel.as_str(),
            &reply.reply_to_message_id,
            "tenant_access_token",
            &formatted,
        );
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/messages/{}/reply",
            self.config.open_base_url.trim_end_matches('/'),
            reply.reply_to_message_id
        );
        logging::log_channel_reply_stage(
            reply.channel.as_str(),
            &reply.reply_to_message_id,
            "reply_api",
            &formatted,
        );
        let response = self
            .http
            .post(url)
            .bearer_auth(token)
            .json(&build_reply_request(&formatted))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            bail!(
                "feishu reply api returned HTTP {}: {}",
                status.as_u16(),
                body
            );
        }

        let payload: FeishuApiResponse = serde_json::from_str(&body)
            .map_err(|error| anyhow!("invalid feishu reply api response: {error}; body={body}"))?;
        if payload.code != 0 {
            bail!(
                "feishu reply api returned code {}: {}",
                payload.code,
                payload.msg.unwrap_or_else(|| "unknown error".to_string())
            );
        }
        logging::log_channel_reply_success(reply.channel.as_str(), &reply.reply_to_message_id);
        Ok(())
    }

    /// 获取飞书租户访问令牌。
    async fn tenant_access_token(&self) -> Result<String> {
        let app_id = self
            .config
            .app_id
            .as_deref()
            .ok_or_else(|| anyhow!("FEISHU_APP_ID is not configured"))?;
        let app_secret = self
            .config
            .app_secret
            .as_deref()
            .ok_or_else(|| anyhow!("FEISHU_APP_SECRET is not configured"))?;
        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.config.open_base_url.trim_end_matches('/')
        );

        let response = self
            .http
            .post(url)
            .json(&json!({
                "app_id": app_id,
                "app_secret": app_secret,
            }))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            bail!(
                "feishu tenant_access_token api returned HTTP {}: {}",
                status.as_u16(),
                body
            );
        }

        let payload: TenantAccessTokenResponse = serde_json::from_str(&body).map_err(|error| {
            anyhow!("invalid feishu tenant_access_token response: {error}; body={body}")
        })?;
        if payload.code != 0 {
            bail!(
                "feishu tenant_access_token api returned code {}: {}",
                payload.code,
                payload.msg.unwrap_or_else(|| "unknown error".to_string())
            );
        }
        payload
            .tenant_access_token
            .ok_or_else(|| anyhow!("tenant_access_token missing in feishu response"))
    }
}

/// 将飞书回调负载解析成统一的入站文本消息模型。
pub fn parse_message_event(
    payload: &Value,
    config: &FeishuCallbackConfig,
) -> Result<InboundMessageParseOutcome> {
    if !matches!(extract_event_type(payload), Some("im.message.receive_v1")) {
        return Ok(InboundMessageParseOutcome::NotMessageEvent);
    }

    if payload
        .pointer("/event/sender/sender_type")
        .and_then(Value::as_str)
        == Some("app")
    {
        return Ok(InboundMessageParseOutcome::Ignored {
            reason: "ignore app-originated message event",
        });
    }

    let message_type = payload
        .pointer("/event/message/message_type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing /event/message/message_type"))?;
    if message_type != "text" {
        return Ok(InboundMessageParseOutcome::Ignored {
            reason: "ignore non-text message event",
        });
    }

    let message_id = payload
        .pointer("/event/message/message_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing /event/message/message_id"))?
        .to_string();
    let chat_id = payload
        .pointer("/event/message/chat_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let chat_type = payload
        .pointer("/event/message/chat_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let event_id = payload
        .get("event_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let user_id = payload
        .pointer("/event/sender/sender_id/open_id")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .pointer("/event/sender/sender_id/user_id")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            payload
                .pointer("/event/sender/sender_id/union_id")
                .and_then(Value::as_str)
        })
        .ok_or_else(|| anyhow!("missing sender identifier in event payload"))?
        .to_string();

    if chat_type.as_deref() == Some("group")
        && config.require_mention
        && !payload
            .pointer("/event/message/mentions")
            .and_then(Value::as_array)
            .map(|mentions| !mentions.is_empty())
            .unwrap_or(false)
    {
        return Ok(InboundMessageParseOutcome::Ignored {
            reason: "ignore group message without bot mention",
        });
    }

    let content_raw = payload
        .pointer("/event/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing /event/message/content"))?;
    let text = parse_text_message_content(
        content_raw,
        payload
            .pointer("/event/message/mentions")
            .and_then(Value::as_array),
    )?;
    let session_seed = chat_id.clone().unwrap_or_else(|| message_id.clone());

    Ok(InboundMessageParseOutcome::Text(InboundTextMessage {
        channel: ChannelKind::Feishu,
        event_id,
        message_id,
        chat_id,
        chat_type,
        user_id,
        session_id: format!("feishu:{session_seed}"),
        text,
    }))
}

/// 解析飞书文本消息内容，并移除 mention key 等噪声。
fn parse_text_message_content(content_raw: &str, mentions: Option<&Vec<Value>>) -> Result<String> {
    let content: Value = serde_json::from_str(content_raw)
        .map_err(|error| anyhow!("invalid feishu text message content JSON: {error}"))?;
    let mut text = content
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing text field in feishu message content"))?
        .trim()
        .to_string();
    if let Some(mentions) = mentions {
        for mention in mentions {
            if let Some(key) = mention.get("key").and_then(Value::as_str) {
                text = text.replace(key, " ");
            }
        }
    }
    text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        bail!("feishu text message content is empty");
    }
    Ok(text)
}

/// 构造飞书文本回复请求体。
fn build_reply_request(text: &str) -> Value {
    json!({
        "content": json!({ "text": text }).to_string(),
        "msg_type": "text",
    })
}

/// 把模型回复整理成更适合飞书 IM 展示的纯文本格式。
fn format_reply_text_for_feishu(input: &str) -> String {
    let mut text = input.replace("\r\n", "\n");
    text = text.replace("**", "");
    text = text.replace("__", "");
    text = text
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    let mut normalized = Vec::new();
    let mut previous_blank = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if blank && previous_blank {
            continue;
        }
        normalized.push(line.trim_start().to_string());
        previous_blank = blank;
    }

    let text = normalized.join("\n").trim().to_string();
    if text.is_empty() {
        "我暂时没有可发送的回复。".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_message_event() {
        let payload = json!({
            "schema": "2.0",
            "header": {
                "event_type": "im.message.receive_v1"
            },
            "event_id": "evt-1",
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_xxx"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_123",
                    "chat_id": "oc_456",
                    "chat_type": "group",
                    "message_type": "text",
                    "mentions": [
                        { "key": "@_user_1" }
                    ],
                    "content": "{\"text\":\"@bot 你是谁\"}"
                }
            }
        });

        let event = parse_message_event(
            &payload,
            &FeishuCallbackConfig {
                require_mention: true,
                ..FeishuCallbackConfig::default()
            },
        )
        .expect("parse should succeed");

        match event {
            InboundMessageParseOutcome::Text(event) => {
                assert_eq!(event.channel, ChannelKind::Feishu);
                assert_eq!(event.message_id, "om_123");
                assert_eq!(event.user_id, "ou_xxx");
                assert_eq!(event.session_id, "feishu:oc_456");
                assert_eq!(event.text, "@bot 你是谁");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn strips_feishu_mention_keys_from_text() {
        let payload = json!({
            "header": {
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_xxx"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_123",
                    "chat_id": "oc_456",
                    "chat_type": "group",
                    "message_type": "text",
                    "mentions": [
                        { "key": "@_user_1" }
                    ],
                    "content": "{\"text\":\"@_user_1 我是谁\"}"
                }
            }
        });

        let event = parse_message_event(
            &payload,
            &FeishuCallbackConfig {
                require_mention: true,
                ..FeishuCallbackConfig::default()
            },
        )
        .expect("parse should succeed");

        match event {
            InboundMessageParseOutcome::Text(event) => {
                assert_eq!(event.text, "我是谁");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn ignores_group_message_without_mention_by_default() {
        let payload = json!({
            "header": {
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_xxx"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_123",
                    "chat_type": "group",
                    "message_type": "text",
                    "content": "{\"text\":\"你好\"}"
                }
            }
        });

        let outcome = parse_message_event(
            &payload,
            &FeishuCallbackConfig {
                require_mention: true,
                ..FeishuCallbackConfig::default()
            },
        )
        .expect("parse should succeed");
        assert_eq!(
            outcome,
            InboundMessageParseOutcome::Ignored {
                reason: "ignore group message without bot mention",
            }
        );
    }

    #[test]
    fn builds_reply_request_body() {
        assert_eq!(
            build_reply_request("hello"),
            json!({
                "content": "{\"text\":\"hello\"}",
                "msg_type": "text"
            })
        );
    }

    #[test]
    fn formats_reply_text_for_feishu_as_plain_text() {
        assert_eq!(
            format_reply_text_for_feishu("**会话历史**\n\n\n- 第一条\n- 第二条\n"),
            "会话历史\n\n- 第一条\n- 第二条"
        );
    }
}
