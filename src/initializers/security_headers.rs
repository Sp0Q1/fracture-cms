use async_trait::async_trait;
use axum::{
    middleware::{self, Next},
    Router as AxumRouter,
};
use loco_rs::{
    app::{AppContext, Initializer},
    Result,
};

pub struct SecurityHeadersInitializer;

async fn set_security_headers(
    request: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; \
         font-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'self'; \
         frame-ancestors 'none'"
            .parse()
            .expect("valid header value"),
    );
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("valid header value"),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        "DENY".parse().expect("valid header value"),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        "strict-origin-when-cross-origin"
            .parse()
            .expect("valid header value"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("x-permitted-cross-domain-policies"),
        "none".parse().expect("valid header value"),
    );
    response
}

#[async_trait]
impl Initializer for SecurityHeadersInitializer {
    fn name(&self) -> String {
        "security-headers".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        Ok(router.layer(middleware::from_fn(set_security_headers)))
    }
}
