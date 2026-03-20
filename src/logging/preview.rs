//! 日志预览工具模块，负责把长文本、JSON 和字节流压缩成适合日志展示的短摘要。

use serde_json::Value;

/// 生成适合日志输出的文本预览。
pub fn preview_text(input: &str, limit: usize) -> String {
    let mut preview = input.trim().replace('\n', "\\n");
    if preview.chars().count() > limit {
        preview = preview.chars().take(limit).collect::<String>();
        preview.push_str("...");
    }
    preview
}

/// 生成适合日志输出的 JSON 预览。
pub fn preview_json(value: &Value, limit: usize) -> String {
    preview_text(
        &serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
        limit,
    )
}

/// 生成适合日志输出的字节流预览。
pub fn preview_bytes(bytes: &[u8], limit: usize) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => preview_text(text, limit),
        Err(_) => format!("<non-utf8:{} bytes>", bytes.len()),
    }
}
