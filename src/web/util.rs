//! Web 工具模块，负责处理 HTTP 层通用的小型辅助逻辑。

use salvo::{Response, http::StatusCode, prelude::Json};
use serde_json::{Map, Value};

use super::types::{ErrorBody, ErrorPayload};

/// 当请求同时携带 `action` 和 `args` 时，把 `action` 合并进参数对象。
pub(crate) fn merge_action_into_args(args: Value, action: Option<String>) -> Value {
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

/// 按统一格式渲染 HTTP 错误响应。
pub(crate) fn render_error(
    res: &mut Response,
    status: StatusCode,
    error_type: &'static str,
    message: &str,
) {
    res.status_code(status);
    res.render(Json(ErrorBody {
        ok: false,
        error: ErrorPayload {
            r#type: error_type,
            message: message.to_string(),
        },
    }));
}
