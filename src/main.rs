mod config;
mod engine;
mod http_api;
mod models;
mod session_store;
mod tools;

use std::sync::Arc;

use anyhow::Result;
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    config::AppConfig,
    engine::ToolCallEngine,
    http_api::{AppState, run_http},
    models::build_llm,
    session_store::SessionStore,
    tools::build_builtin_registry,
};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let llm = build_llm(&config.llm)?;
    let session_store = SessionStore::default();
    let registry = build_builtin_registry(session_store.clone())?;
    let engine = Arc::new(ToolCallEngine::new(
        config.app_name.clone(),
        llm,
        registry,
        session_store,
        config.default_system_prompt.clone(),
        config.max_iterations,
    ));

    tracing::info!(
        server_addr = %config.server_addr,
        provider = %config.llm.provider.as_str(),
        model = %config.llm.model,
        "starting tool-call service"
    );

    run_http(Arc::new(AppState { config, engine })).await
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,rs_tool_call=debug"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
