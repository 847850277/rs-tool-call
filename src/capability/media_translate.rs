//! 媒体翻译能力模块，负责接入阿里百炼 Qwen3 LiveTranslate 系列接口。
//! 该能力独立于现有 chat loop，直接通过 HTTP 调用模型兼容接口。

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use serde_json::{Value, json};

use crate::config::MediaTranslateConfig;

/// 媒体翻译输入。
#[derive(Debug, Clone)]
pub enum MediaTranslateInput {
    Audio { data: String, format: String },
    VideoUrl { url: String },
}

/// 可选的音频输出配置。
#[derive(Debug, Clone)]
pub struct MediaTranslateAudioOutput {
    pub format: String,
    pub voice: String,
}

/// 媒体翻译能力请求。
#[derive(Debug, Clone)]
pub struct MediaTranslateRequest {
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub input: MediaTranslateInput,
    pub output_audio: Option<MediaTranslateAudioOutput>,
    pub include_usage: bool,
}

/// 媒体翻译能力响应。
#[derive(Debug, Clone)]
pub struct MediaTranslateResponse {
    pub model: String,
    pub translated_text: String,
    pub audio_base64: Option<String>,
    pub audio_id: Option<String>,
    pub usage: Option<Value>,
    pub request_id: Option<String>,
    pub finish_reason: Option<String>,
}

/// 媒体翻译能力。
#[derive(Clone)]
pub struct MediaTranslateCapability {
    client: Client,
    config: MediaTranslateConfig,
}

impl MediaTranslateCapability {
    /// 基于媒体翻译配置创建能力实例。
    pub fn new(config: MediaTranslateConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// 执行一次媒体翻译请求。
    pub async fn execute(&self, request: MediaTranslateRequest) -> Result<MediaTranslateResponse> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "MEDIA_TRANSLATE_API_KEY is not configured and no DashScope-compatible fallback key was found"
                )
            })?;

        let target_lang = request.target_lang.trim();
        if target_lang.is_empty() {
            bail!("target_lang must not be empty");
        }

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = build_request_body(&self.config.model, request)?;

        let response = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("failed to call media translate endpoint: {url}"))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("failed to read media translate response body")?;

        if !status.is_success() {
            bail!(
                "media translate endpoint returned status {}: {}",
                status,
                response_text
            );
        }

        parse_stream_response(&response_text).context("failed to parse media translate stream")
    }
}

fn build_request_body(model: &str, request: MediaTranslateRequest) -> Result<Value> {
    let source_lang = request
        .source_lang
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    let content = match request.input {
        MediaTranslateInput::Audio { data, format } => {
            if data.trim().is_empty() {
                bail!("audio.data must not be empty");
            }
            if format.trim().is_empty() {
                bail!("audio.format must not be empty");
            }
            json!([{
                "type": "input_audio",
                "input_audio": {
                    "data": data,
                    "format": format
                }
            }])
        }
        MediaTranslateInput::VideoUrl { url } => {
            if url.trim().is_empty() {
                bail!("video_url must not be empty");
            }
            json!([{
                "type": "video_url",
                "video_url": {
                    "url": url
                }
            }])
        }
    };

    let modalities = if request.output_audio.is_some() {
        json!(["text", "audio"])
    } else {
        json!(["text"])
    };

    let mut body = json!({
        "model": model,
        "stream": true,
        "modalities": modalities,
        "messages": [{
            "role": "user",
            "content": content
        }],
        "translation_options": {
            "target_lang": request.target_lang.trim(),
            "source_lang": source_lang
        }
    });

    if request.include_usage {
        body["stream_options"] = json!({ "include_usage": true });
    }

    if let Some(audio) = request.output_audio {
        if audio.format.trim().is_empty() || audio.voice.trim().is_empty() {
            bail!("audio output requires both format and voice");
        }
        body["audio"] = json!({
            "format": audio.format,
            "voice": audio.voice
        });
    }

    Ok(body)
}

fn parse_stream_response(raw: &str) -> Result<MediaTranslateResponse> {
    let mut request_id = None;
    let mut model = None;
    let mut translated_text = String::new();
    let mut audio_base64 = String::new();
    let mut audio_id = None;
    let mut usage = None;
    let mut finish_reason = None;
    let mut saw_payload = false;

    for payload in iter_sse_data_payloads(raw) {
        if payload == "[DONE]" {
            continue;
        }
        saw_payload = true;
        let value = serde_json::from_str::<Value>(payload)
            .with_context(|| format!("invalid JSON payload in SSE stream: {payload}"))?;

        if request_id.is_none() {
            request_id = value
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        if model.is_none() {
            model = value
                .get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        if usage.is_none() {
            usage = value.get("usage").cloned().filter(|entry| !entry.is_null());
        }

        if let Some(choices) = value.get("choices").and_then(Value::as_array) {
            for choice in choices {
                if finish_reason.is_none() {
                    finish_reason = choice
                        .get("finish_reason")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string);
                }

                let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
                    continue;
                };

                if let Some(text) = delta.get("content").and_then(Value::as_str) {
                    translated_text.push_str(text);
                }

                if let Some(audio) = delta.get("audio").and_then(Value::as_object) {
                    if let Some(data) = audio.get("data").and_then(Value::as_str) {
                        audio_base64.push_str(data);
                    }
                    if audio_id.is_none() {
                        audio_id = audio
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                    }
                }
            }
        }
    }

    if !saw_payload {
        bail!("stream response did not contain any data payloads");
    }

    if translated_text.trim().is_empty() && audio_base64.is_empty() {
        bail!("stream response did not contain translated text or audio");
    }

    Ok(MediaTranslateResponse {
        model: model.unwrap_or_default(),
        translated_text: translated_text.trim().to_string(),
        audio_base64: if audio_base64.is_empty() {
            None
        } else {
            Some(audio_base64)
        },
        audio_id,
        usage,
        request_id,
        finish_reason,
    })
}

fn iter_sse_data_payloads(raw: &str) -> impl Iterator<Item = &str> {
    raw.split("\n\n").flat_map(|chunk| {
        chunk.lines().filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("data:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_and_audio_from_sse_stream() {
        let raw = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"qwen3-livetranslate-flash\",\"choices\":[{\"delta\":{\"content\":\"Hello \"},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"qwen3-livetranslate-flash\",\"choices\":[{\"delta\":{\"content\":\"world\",\"audio\":{\"id\":\"aud_1\",\"data\":\"YWJj\"}},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
            "data: [DONE]\n\n"
        );

        let parsed = parse_stream_response(raw).expect("stream should parse");

        assert_eq!(parsed.request_id.as_deref(), Some("chatcmpl-1"));
        assert_eq!(parsed.model, "qwen3-livetranslate-flash");
        assert_eq!(parsed.translated_text, "Hello world");
        assert_eq!(parsed.audio_id.as_deref(), Some("aud_1"));
        assert_eq!(parsed.audio_base64.as_deref(), Some("YWJj"));
        assert_eq!(parsed.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            parsed.usage.as_ref().and_then(|v| v.get("total_tokens")),
            Some(&json!(15))
        );
    }

    #[test]
    fn builds_audio_request_body() {
        let body = build_request_body(
            "qwen3-livetranslate-flash",
            MediaTranslateRequest {
                source_lang: Some("Chinese".to_string()),
                target_lang: "English".to_string(),
                input: MediaTranslateInput::Audio {
                    data: "https://example.com/audio.wav".to_string(),
                    format: "wav".to_string(),
                },
                output_audio: Some(MediaTranslateAudioOutput {
                    format: "wav".to_string(),
                    voice: "Chelsie".to_string(),
                }),
                include_usage: true,
            },
        )
        .expect("request body should build");

        assert_eq!(body["model"], "qwen3-livetranslate-flash");
        assert_eq!(body["stream"], true);
        assert_eq!(body["modalities"], json!(["text", "audio"]));
        assert_eq!(body["translation_options"]["target_lang"], "English");
        assert_eq!(body["translation_options"]["source_lang"], "Chinese");
        assert_eq!(body["audio"]["voice"], "Chelsie");
    }
}
