//! Shared styled error responses.
//!
//! Permission denials used to return a raw `"Forbidden"` string (or a bare
//! `<h1>`), dropping the user onto an unstyled browser-default page. This
//! renders a styled page matching the static 404 (same `.error-page` classes),
//! so a 403 looks like part of the app. The message text is static — no user
//! input — so emitting it as HTML is safe.

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

/// Builds a self-contained styled error page. Links the stylesheets without
/// SRI (an error page can't depend on the boot-time hash index); the strict
/// CSP `style-src 'self'` still constrains them.
fn error_html(code: &str, message: &str, hint: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\" data-theme=\"dark\"><head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\
<title>{code} — {message}</title>\
<link rel=\"stylesheet\" href=\"/static/oat.min.css\">\
<link rel=\"stylesheet\" href=\"/static/app.css\">\
</head><body><main class=\"error-page\" role=\"main\">\
<h1 class=\"error-code\" aria-label=\"Error {code}\">{code}</h1>\
<p class=\"error-message\">{message}</p>\
<p class=\"error-hint\">{hint}</p>\
<a href=\"/\" class=\"button\">Back to Home</a>\
</main></body></html>"
    )
}

/// A styled error page with a custom status, heading, and hint. Use for
/// business-rule refusals that carry a specific message (the text must be
/// static / trusted — it is emitted as HTML).
#[must_use]
pub fn error_page(status: StatusCode, message: &str, hint: &str) -> Response {
    let code = status.as_u16().to_string();
    (status, Html(error_html(&code, message, hint))).into_response()
}

/// A styled `403 Forbidden` response. Use everywhere a role/permission check
/// fails (and the resource's existence isn't being hidden — those still 404).
#[must_use]
pub fn forbidden() -> Response {
    error_page(
        StatusCode::FORBIDDEN,
        "Forbidden",
        "You don't have permission to view this page.",
    )
}
