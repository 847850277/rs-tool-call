//! Web CORS 模块，负责为浏览器前端补充跨域访问所需的响应头。

use async_trait::async_trait;
use salvo::{
    Depot, FlowCtrl, Handler, Request, Response,
    http::{
        Method, StatusCode,
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_MAX_AGE, HeaderValue,
        },
    },
};

/// CORS 中间件，为所有响应统一追加跨域头，并处理预检请求。
#[derive(Debug, Clone, Copy)]
pub(crate) struct CorsHandler;

#[async_trait]
impl Handler for CorsHandler {
    /// 处理跨域头注入；对于浏览器预检请求直接返回 204。
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        apply_cors_headers(res);

        if req.method() == Method::OPTIONS {
            res.status_code(StatusCode::NO_CONTENT);
            return;
        }

        ctrl.call_next(req, depot, res).await;
        apply_cors_headers(res);
    }
}

fn apply_cors_headers(res: &mut Response) {
    res.headers_mut()
        .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    res.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    res.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Authorization"),
    );
    res.headers_mut()
        .insert(ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
}
