pub mod admin;
pub mod blog;
pub mod jobs;
pub mod middleware;
pub mod oidc;
pub mod oidc_state;
pub mod org;
pub mod site;
pub mod uploads;

/// Marks a public response cacheable. Only call this for the guest variant
/// of a page — anything rendered with session state must not carry it.
pub(crate) fn cache_public(
    mut res: axum::response::Response,
    max_age_secs: u32,
) -> axum::response::Response {
    if let Ok(value) = format!("public, max-age={max_age_secs}").parse() {
        res.headers_mut()
            .insert(axum::http::header::CACHE_CONTROL, value);
    }
    res
}
