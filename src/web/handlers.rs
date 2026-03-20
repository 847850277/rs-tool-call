//! Web handler 模块，负责处理具体 HTTP 请求并把结果渲染为响应。

use salvo::{
    Depot, Request, Response,
    http::StatusCode,
    prelude::{Json, handler},
};
use serde_json::{Map, Value};
use tracing::debug;

use crate::{
    capability::{ConversationRequest, DirectToolInvocationRequest},
    channel::InboundMessageParseOutcome,
    channel::feishu::{
        FeishuCallbackErrorKind, callback_ack, extract_event_type, handle_text_message_event,
        parse_message_event, process_callback as process_feishu_callback,
    },
    logging,
};

use super::{
    state::app_state,
    types::{ChatRequest, HealthResponse, ToolInvokeRequest, ToolInvokeResponse},
    util::{merge_action_into_args, render_error},
};

/// 处理健康检查请求。
#[handler]
pub(crate) async fn health(depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    res.render(Json(HealthResponse {
        status: "ok",
        app_name: state.config.app_name.clone(),
        provider: state.config.llm.provider.as_str().to_string(),
        model: state.config.llm.model.clone(),
    }));
}

/// 处理飞书回调请求，并在识别到文本消息时转交给飞书通道服务。
#[handler]
pub(crate) async fn feishu_callback(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let method = req.method().as_str().to_string();
    let uri = req.uri().to_string();
    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();
    let content_type = req
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();
    let request_id = req
        .headers()
        .get("x-request-id")
        .or_else(|| req.headers().get("x-tt-logid"))
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();

    let payload = match req.payload().await {
        Ok(bytes) => bytes.clone(),
        Err(error) => {
            logging::log_channel_callback_body_read_error(
                "feishu",
                &method,
                &uri,
                &user_agent,
                &content_type,
                &request_id,
                &error.to_string(),
            );
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request_body",
                &error.to_string(),
            );
            return;
        }
    };
    let raw_body_preview = logging::preview_bytes(&payload, 320);
    logging::log_channel_callback_ingress(
        "feishu",
        &method,
        &uri,
        &user_agent,
        &content_type,
        &request_id,
        &payload,
    );
    let body = if payload.is_empty() {
        Value::Object(Map::new())
    } else {
        match serde_json::from_slice::<Value>(&payload) {
            Ok(value) => value,
            Err(error) => {
                logging::log_channel_callback_json_error(
                    "feishu",
                    &method,
                    &uri,
                    &request_id,
                    &raw_body_preview,
                    &error.to_string(),
                );
                render_error(
                    res,
                    StatusCode::BAD_REQUEST,
                    "invalid_json",
                    &error.to_string(),
                );
                return;
            }
        }
    };

    let state = app_state(depot);
    match process_feishu_callback(body, &state.config.feishu_callback) {
        Ok(outcome) => {
            let event_type = extract_event_type(&outcome.payload).map(str::to_string);
            logging::log_channel_callback_processed(
                "feishu",
                outcome.encrypted,
                event_type.as_deref(),
                &outcome.payload,
            );
            match parse_message_event(&outcome.payload, &state.config.feishu_callback) {
                Ok(InboundMessageParseOutcome::Text(event)) => {
                    logging::log_channel_text_message_received(
                        event.channel.as_str(),
                        event.event_id.as_deref(),
                        &event.message_id,
                        event.chat_id.as_deref(),
                        event.chat_type.as_deref(),
                        &event.session_id,
                        &event.user_id,
                        &event.text,
                    );
                    let background_state = state.clone();
                    let event_channel = event.channel;
                    tokio::spawn(async move {
                        if let Err(error) = handle_text_message_event(
                            background_state.capabilities.conversation().clone(),
                            background_state.config.feishu_callback.clone(),
                            event,
                        )
                        .await
                        {
                            logging::log_channel_background_error(
                                event_channel.as_str(),
                                &error.to_string(),
                            );
                        }
                    });
                    res.render(Json(callback_ack()));
                }
                Ok(InboundMessageParseOutcome::Ignored { reason }) => {
                    logging::log_channel_message_ignored("feishu", reason);
                    res.render(Json(callback_ack()));
                }
                Ok(InboundMessageParseOutcome::NotMessageEvent) => {
                    res.render(Json(outcome.response_body));
                }
                Err(error) => {
                    logging::log_channel_message_parse_error("feishu", &error.to_string());
                    res.render(Json(callback_ack()));
                }
            }
        }
        Err(error) => {
            let status = match error.kind {
                FeishuCallbackErrorKind::BadRequest => StatusCode::BAD_REQUEST,
                FeishuCallbackErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
            };
            let error_type = match error.kind {
                FeishuCallbackErrorKind::BadRequest => "feishu_callback_invalid",
                FeishuCallbackErrorKind::Unauthorized => "feishu_callback_unauthorized",
            };
            logging::log_channel_callback_process_error("feishu", status.as_u16(), &error.message);
            render_error(res, status, error_type, &error.message);
        }
    }
}

/// 返回当前已注册工具列表。
#[handler]
pub(crate) async fn list_tools(depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    let tools = state.capabilities.tools().list_descriptors();
    debug!(tool_count = tools.len(), "listing tools");
    res.render(Json(tools));
}

/// 返回当前内存中的会话摘要列表。
#[handler]
pub(crate) async fn list_sessions(depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    let sessions = state.capabilities.sessions().list().await;
    debug!(session_count = sessions.len(), "listing sessions");
    res.render(Json(sessions));
}

/// 返回指定会话的历史消息。
#[handler]
pub(crate) async fn session_history(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    let session_id = match req.param::<String>("session_id") {
        Some(value) => value,
        None => {
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "missing session_id",
            );
            return;
        }
    };
    let limit = req.query::<usize>("limit");
    let history = state
        .capabilities
        .sessions()
        .history(&session_id, limit)
        .await;
    debug!(
        session_id = %session_id,
        limit = ?limit,
        message_count = history.len(),
        "loaded session history"
    );
    res.render(Json(history));
}

/// 处理标准聊天请求，并驱动一次引擎回合。
#[handler]
pub(crate) async fn chat(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body = match req.parse_json::<ChatRequest>().await {
        Ok(value) => value,
        Err(error) => {
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_json",
                &error.to_string(),
            );
            return;
        }
    };
    let state = app_state(depot);
    let session_id = body.session_id.clone();
    let user_id = body.user_id.clone();
    logging::log_http_chat_request(
        &session_id,
        &user_id,
        body.persist,
        body.max_iterations,
        &body.message,
    );

    match state
        .capabilities
        .conversation()
        .execute(ConversationRequest {
            session_id: body.session_id,
            user_id: body.user_id,
            message: body.message,
            system_prompt: body.system_prompt,
            max_iterations: body.max_iterations,
            persist: body.persist,
        })
        .await
    {
        Ok(response) => {
            logging::log_http_chat_complete(
                &response.session_id,
                &response.user_id,
                response.iterations,
                response.tool_calls.len(),
                response.finish_reason.as_deref(),
                &response.answer,
            );
            res.render(Json(response));
        }
        Err(error) => {
            logging::log_http_chat_failed(&session_id, &user_id, &error.to_string());
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "tool_loop_failed",
                &error.to_string(),
            )
        }
    }
}

/// 处理直接工具调用请求。
#[handler]
pub(crate) async fn invoke_tool(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let body = match req.parse_json::<ToolInvokeRequest>().await {
        Ok(value) => value,
        Err(error) => {
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_json",
                &error.to_string(),
            );
            return;
        }
    };
    let state = app_state(depot);
    let args = merge_action_into_args(body.args, body.action);
    logging::log_http_tool_invoke_request(&body.session_id, &body.user_id, &body.tool, &args);

    match state
        .capabilities
        .tools()
        .invoke(DirectToolInvocationRequest {
            tool: body.tool,
            session_id: body.session_id,
            user_id: body.user_id,
            args,
        })
        .await
    {
        Ok(result) => {
            logging::log_http_tool_invoke_complete(
                &result.tool_name,
                &result.function_call_id,
                &result.output,
            );
            res.render(Json(ToolInvokeResponse {
                ok: true,
                result: result.output,
            }))
        }
        Err(error) => {
            logging::log_http_tool_invoke_failed(&error.to_string());
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "tool_execution_failed",
                &error.to_string(),
            )
        }
    }
}
