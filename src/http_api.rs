use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use salvo::{
    Depot, FlowCtrl, Handler, Listener, Request, Response, Router,
    http::StatusCode,
    prelude::{Json, Server, TcpListener, handler},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tracing::{debug, error, info};

use crate::{
    config::AppConfig,
    engine::{ChatTurnRequest, ToolCallEngine},
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub engine: Arc<ToolCallEngine>,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    #[serde(default = "default_session_id")]
    session_id: String,
    #[serde(default = "default_user_id")]
    user_id: String,
    message: String,
    #[serde(default)]
    system_prompt: Option<String>,
    #[serde(default)]
    max_iterations: Option<usize>,
    #[serde(default = "default_persist")]
    persist: bool,
}

#[derive(Debug, Deserialize)]
struct ToolInvokeRequest {
    tool: String,
    #[serde(default = "default_session_id")]
    session_id: String,
    #[serde(default = "default_user_id")]
    user_id: String,
    #[serde(default)]
    action: Option<String>,
    #[serde(default = "default_args")]
    args: Value,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    ok: bool,
    error: ErrorPayload,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    r#type: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct ToolInvokeResponse {
    ok: bool,
    result: Value,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    app_name: String,
    provider: String,
    model: String,
}

pub async fn run_http(state: Arc<AppState>) -> Result<()> {
    let router = build_router(state.clone());
    let acceptor = TcpListener::new(state.config.server_addr.clone())
        .bind()
        .await;
    Server::new(acceptor).serve(router).await;
    Ok(())
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .hoop(StateInjector { state })
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("tools").get(list_tools))
        .push(Router::with_path("tools/invoke").post(invoke_tool))
        .push(Router::with_path("chat").post(chat))
        .push(Router::with_path("sessions").get(list_sessions))
        .push(Router::with_path("sessions/{session_id}/history").get(session_history))
}

#[derive(Clone)]
struct StateInjector {
    state: Arc<AppState>,
}

#[async_trait]
impl Handler for StateInjector {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        depot.inject(self.state.clone());
        ctrl.call_next(req, depot, res).await;
    }
}

#[handler]
async fn health(depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    res.render(Json(HealthResponse {
        status: "ok",
        app_name: state.config.app_name.clone(),
        provider: state.config.llm.provider.as_str().to_string(),
        model: state.config.llm.model.clone(),
    }));
}

#[handler]
async fn list_tools(depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    let tools = state.engine.tools();
    debug!(tool_count = tools.len(), "listing tools");
    res.render(Json(tools));
}

#[handler]
async fn list_sessions(depot: &mut Depot, res: &mut Response) {
    let state = app_state(depot);
    let sessions = state.engine.list_sessions().await;
    debug!(session_count = sessions.len(), "listing sessions");
    res.render(Json(sessions));
}

#[handler]
async fn session_history(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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
    let history = state.engine.session_history(&session_id, limit).await;
    debug!(
        session_id = %session_id,
        limit = ?limit,
        message_count = history.len(),
        "loaded session history"
    );
    res.render(Json(history));
}

#[handler]
async fn chat(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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
    info!(
        session_id = %session_id,
        user_id = %user_id,
        persist = body.persist,
        requested_max_iterations = ?body.max_iterations,
        message_preview = %preview_text(&body.message, 160),
        "received chat request"
    );

    match state
        .engine
        .run_turn(ChatTurnRequest {
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
            info!(
                session_id = %response.session_id,
                user_id = %response.user_id,
                iterations = response.iterations,
                tool_call_count = response.tool_calls.len(),
                finish_reason = ?response.finish_reason,
                answer_preview = %preview_text(&response.answer, 160),
                "completed chat request"
            );
            res.render(Json(response));
        }
        Err(error) => {
            error!(
                session_id = %session_id,
                user_id = %user_id,
                error = %error,
                "chat request failed"
            );
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "tool_loop_failed",
                &error.to_string(),
            )
        }
    }
}

#[handler]
async fn invoke_tool(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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
    info!(
        session_id = %body.session_id,
        user_id = %body.user_id,
        tool = %body.tool,
        args_preview = %preview_value(&args, 200),
        "received direct tool invocation"
    );

    match state
        .engine
        .invoke_tool(body.user_id, body.session_id, body.tool, args)
        .await
    {
        Ok(result) => {
            info!(
                tool = %result.tool_name,
                function_call_id = %result.function_call_id,
                output_preview = %preview_value(&result.output, 200),
                "completed direct tool invocation"
            );
            res.render(Json(ToolInvokeResponse {
                ok: true,
                result: result.output,
            }))
        }
        Err(error) => {
            error!(error = %error, "direct tool invocation failed");
            render_error(
                res,
                StatusCode::BAD_REQUEST,
                "tool_execution_failed",
                &error.to_string(),
            )
        }
    }
}

fn app_state(depot: &Depot) -> Arc<AppState> {
    match depot.obtain::<Arc<AppState>>() {
        Ok(state) => state.clone(),
        Err(_) => panic!("app state is missing"),
    }
}

fn merge_action_into_args(args: Value, action: Option<String>) -> Value {
    match (args, action) {
        (Value::Object(mut object), Some(action_value)) => {
            object
                .entry("action".to_string())
                .or_insert(Value::String(action_value));
            Value::Object(object)
        }
        (Value::Null, Some(action_value)) => {
            let mut object = Map::new();
            object.insert("action".to_string(), Value::String(action_value));
            Value::Object(object)
        }
        (value, _) => value,
    }
}

fn render_error(res: &mut Response, status: StatusCode, error_type: &'static str, message: &str) {
    res.status_code(status);
    res.render(Json(ErrorBody {
        ok: false,
        error: ErrorPayload {
            r#type: error_type,
            message: message.to_string(),
        },
    }));
}

fn preview_text(input: &str, limit: usize) -> String {
    let mut preview = input.trim().replace('\n', "\\n");
    if preview.chars().count() > limit {
        preview = preview.chars().take(limit).collect::<String>();
        preview.push_str("...");
    }
    preview
}

fn preview_value(value: &Value, limit: usize) -> String {
    preview_text(
        &serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
        limit,
    )
}

fn default_args() -> Value {
    Value::Object(Map::new())
}

fn default_session_id() -> String {
    "main".to_string()
}

fn default_user_id() -> String {
    "anonymous".to_string()
}

fn default_persist() -> bool {
    true
}
