//! Public site pages: the landing page view hook and static marketing pages.
//!
//! The public surface (layout, landing, blog, static pages) is owned by
//! fracture-core so downstream apps carry no files for it; everything is
//! overridable per template. Static pages are plain HTML *fragments* at
//! `site/pages/{slug}.html` — core wraps them in the public layout, so an
//! app (or downstream repo) adds a page by dropping in one file with no
//! `extends` and no layout copy.

use axum_extra::extract::cookie::CookieJar;
use loco_rs::prelude::*;

use crate::controllers::middleware;
use crate::views;

/// Returns true for slugs that can only name a page template, never escape
/// the `site/pages/` template namespace.
fn valid_page_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// GET /pages/{slug} — render a static marketing page fragment in the
/// public layout. Unknown slugs (no matching template) are 404.
///
/// # Errors
///
/// Returns `NotFound` for invalid slugs or missing page templates.
#[debug_handler]
pub async fn page(
    Path(slug): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    if !valid_page_slug(&slug) {
        return Err(Error::NotFound);
    }
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user_name = user.map(|u| u.name);
    let base_url = ctx.config.server.host.clone();
    let res = views::site::page(&v, &slug, user_name.as_deref(), &base_url).map_err(|e| {
        // A missing fragment template is the normal 404 path; real template
        // errors surface in the log without leaking internals.
        tracing::debug!(slug, error = %e, "static page not rendered");
        Error::NotFound
    })?;
    Ok(if user_name.is_none() {
        super::cache_public(res, 300)
    } else {
        res
    })
}

pub fn routes() -> Routes {
    Routes::new().prefix("/pages").add("/{slug}", get(page))
}
