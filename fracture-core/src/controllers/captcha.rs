//! Serves Altcha proof-of-work challenges for public forms.

use loco_rs::prelude::*;

use crate::captcha;

/// GET /captcha/challenge — a fresh signed challenge for the Altcha widget.
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
#[debug_handler]
pub async fn challenge() -> Result<Response> {
    let mut res = format::json(captcha::create_challenge())?;
    // Each challenge is single-use; a cached one would always fail.
    if let Ok(value) = "no-store".parse() {
        res.headers_mut()
            .insert(axum::http::header::CACHE_CONTROL, value);
    }
    Ok(res)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/captcha")
        .add("/challenge", get(challenge))
}
