//! 启动日志模块，负责记录服务启动和关键配置加载情况。

use tracing::{info, warn};

/// 记录服务启动时的基础信息。
pub fn log_service_startup(server_addr: &str, provider: &str, model: &str) {
    info!(
        server_addr = %server_addr,
        provider = %provider,
        model = %model,
        "starting tool-call service"
    );
}

/// 记录飞书接入相关配置是否已经正确加载。
pub fn log_feishu_integration_config(
    open_base_url: &str,
    verification_token_configured: bool,
    encrypt_key_configured: bool,
    app_id_configured: bool,
    app_secret_configured: bool,
    require_mention: bool,
    exec_command_tool_enabled: bool,
    exec_command_tool_shell: &str,
) {
    info!(
        feishu_open_base_url = %open_base_url,
        feishu_verification_token_configured = verification_token_configured,
        feishu_encrypt_key_configured = encrypt_key_configured,
        feishu_app_id_configured = app_id_configured,
        feishu_app_secret_configured = app_secret_configured,
        feishu_require_mention = require_mention,
        exec_command_tool_enabled = exec_command_tool_enabled,
        exec_command_tool_shell = %exec_command_tool_shell,
        "loaded feishu integration config"
    );
}

/// 记录飞书回复能力未启用的原因。
pub fn log_feishu_reply_disabled() {
    warn!("feishu message reply is disabled because FEISHU_APP_ID or FEISHU_APP_SECRET is missing");
}
