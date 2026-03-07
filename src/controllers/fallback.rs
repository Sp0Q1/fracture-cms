use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};

const ERROR_PAGE: &str = include_str!("../../assets/static/404.html");

/// Returns the custom 404 error page for unmatched routes.
pub async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Html(ERROR_PAGE))
}
