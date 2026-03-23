//! 媒体翻译配置模块，负责声明阿里百炼实时翻译接口所需的配置。

/// 媒体翻译接口配置。
#[derive(Debug, Clone)]
pub struct MediaTranslateConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}
