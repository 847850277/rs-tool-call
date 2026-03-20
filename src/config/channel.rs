//! 通道配置模块，负责描述飞书等消息通道的接入配置。

/// 飞书回调和 IM 回复所需的配置集合。
#[derive(Debug, Clone, Default)]
pub struct FeishuCallbackConfig {
    pub verification_token: Option<String>,
    pub encrypt_key: Option<String>,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub open_base_url: String,
    pub require_mention: bool,
}
