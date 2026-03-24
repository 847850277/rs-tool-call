//! 飞书服务编排模块，负责把飞书消息事件接到引擎回合并将结果回复给飞书。

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use crate::{
    capability::{
        ConversationCapability, ConversationRequest, EnglishLearningCapability,
        MediaTranslateCapability, MediaTranslateInput, MediaTranslateRequest,
    },
    channel::{InboundAudioMessage, InboundTextMessage, OutboundTextReply},
    config::FeishuCallbackConfig,
    logging,
};

use super::FeishuBotClient;

/// 处理一条统一入站文本消息，并通过飞书回复链路将结果回复回原消息。
pub async fn handle_text_message_event(
    conversation: ConversationCapability,
    english_learning: EnglishLearningCapability,
    config: FeishuCallbackConfig,
    event: InboundTextMessage,
) -> Result<()> {
    let answer = build_text_reply(
        &conversation,
        &english_learning,
        &event.session_id,
        &event.user_id,
        &event.text,
    )
    .await?;
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

/// 处理一条统一入站语音消息：先下载语音资源，再转写为文本，最后复用现有对话链路生成回复。
pub async fn handle_audio_message_event(
    conversation: ConversationCapability,
    english_learning: EnglishLearningCapability,
    media_translate: MediaTranslateCapability,
    config: FeishuCallbackConfig,
    event: InboundAudioMessage,
) -> Result<()> {
    let client = FeishuBotClient::new(config.clone());
    if matches!(event.duration_ms, Some(0)) {
        let reply = OutboundTextReply {
            channel: event.channel,
            reply_to_message_id: event.message_id.clone(),
            session_id: event.session_id.clone(),
            text: "我收到了这条语音，但飞书回调里显示时长是 0 秒。这种语音通常无法稳定转写，请重新录一条 1 秒以上、内容更完整的语音。".to_string(),
        };
        client.send_text_reply(&reply).await?;
        logging::log_channel_text_replied(
            reply.channel.as_str(),
            &reply.reply_to_message_id,
            &reply.session_id,
            &reply.text,
        );
        return Ok(());
    }

    let audio = client
        .download_audio_resource(
            &event.message_id,
            &event.file_key,
            &event.resource_type,
            event.format_hint.as_deref(),
        )
        .await?;
    let audio_data_url = format!(
        "data:{};base64,{}",
        audio.mime_type,
        STANDARD.encode(&audio.bytes)
    );
    let learning_audio_mode = english_learning
        .has_active_lesson_session(&event.session_id)
        .await;
    let source_lang = if learning_audio_mode {
        Some("en".to_string())
    } else {
        config.audio_source_lang.clone()
    };
    let target_lang = if learning_audio_mode {
        "en".to_string()
    } else {
        config.audio_target_lang.clone()
    };
    let transcript_response = media_translate
        .execute(MediaTranslateRequest {
            source_lang,
            target_lang,
            input: MediaTranslateInput::Audio {
                data: audio_data_url,
                format: audio.format,
            },
            output_audio: None,
            include_usage: true,
        })
        .await;
    let transcript = match transcript_response {
        Ok(value) => value.translated_text.trim().to_string(),
        Err(error) => {
            let reply = OutboundTextReply {
                channel: event.channel,
                reply_to_message_id: event.message_id.clone(),
                session_id: event.session_id.clone(),
                text: if learning_audio_mode {
                    "我收到了这条英语跟读语音，但这次没有成功识别出英文文本。请尽量录制 1 秒以上、语速稍慢一点、环境更安静的语音后再试。".to_string()
                } else {
                    "我收到了这条语音，但这次没有成功识别出可用文本。请重新录一条更清晰、稍长一点的语音后再试。".to_string()
                },
            };
            client.send_text_reply(&reply).await?;
            logging::log_channel_text_replied(
                reply.channel.as_str(),
                &reply.reply_to_message_id,
                &reply.session_id,
                &reply.text,
            );
            return Err(anyhow::anyhow!(
                "failed to transcribe channel audio: {error}"
            ));
        }
    };

    if transcript.is_empty() {
        let reply = OutboundTextReply {
            channel: event.channel,
            reply_to_message_id: event.message_id.clone(),
            session_id: event.session_id.clone(),
            text: "我收到了语音消息，但这次没有成功识别出可用文本。".to_string(),
        };
        client.send_text_reply(&reply).await?;
        logging::log_channel_text_replied(
            reply.channel.as_str(),
            &reply.reply_to_message_id,
            &reply.session_id,
            &reply.text,
        );
        return Ok(());
    }
    logging::log_channel_audio_transcribed(
        event.channel.as_str(),
        &event.message_id,
        &event.session_id,
        &transcript,
    );
    let answer = build_audio_reply(
        &conversation,
        &english_learning,
        &event.session_id,
        &event.user_id,
        &transcript,
    )
    .await?;
    let reply = OutboundTextReply {
        channel: event.channel,
        reply_to_message_id: event.message_id.clone(),
        session_id: event.session_id.clone(),
        text: answer,
    };

    client.send_text_reply(&reply).await?;
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

async fn build_text_reply(
    conversation: &ConversationCapability,
    english_learning: &EnglishLearningCapability,
    session_id: &str,
    user_id: &str,
    message: &str,
) -> Result<String> {
    if let Some(reply) = english_learning
        .maybe_handle_message(session_id, message)
        .await?
    {
        let trimmed = reply.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let response = conversation
        .execute(ConversationRequest {
            session_id: session_id.to_string(),
            user_id: user_id.to_string(),
            message: message.to_string(),
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
    Ok(answer)
}

async fn build_audio_reply(
    conversation: &ConversationCapability,
    english_learning: &EnglishLearningCapability,
    session_id: &str,
    user_id: &str,
    transcript: &str,
) -> Result<String> {
    if let Some(reply) = english_learning
        .maybe_handle_message(session_id, transcript)
        .await?
    {
        let trimmed = reply.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Some(reply) = english_learning
        .maybe_handle_shadowing_audio(session_id, transcript)
        .await?
    {
        let trimmed = reply.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let response = conversation
        .execute(ConversationRequest {
            session_id: session_id.to_string(),
            user_id: user_id.to_string(),
            message: transcript.to_string(),
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
    Ok(answer)
}
