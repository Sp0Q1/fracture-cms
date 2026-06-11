//! Views for the public site surface: landing page and static pages.

use loco_rs::prelude::*;

/// Renders the landing (sales) page with the public layout. Pass the
/// signed-in user's name (if any) so the layout shows the Dashboard CTA
/// instead of Sign in.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn landing(v: &impl ViewRenderer, user_name: Option<&str>) -> Result<Response> {
    format::render().view(v, "site/landing.html", data!({ "user_name": user_name }))
}

/// Renders a static page fragment (`site/pages/{slug}.html`) wrapped in the
/// public layout via `site/page_frame.html`.
///
/// Fragments are plain HTML written by the app author — no `extends`
/// needed, which is what lets apps and downstream repos add pages by
/// dropping in a single file. The fragment is trusted server-side template
/// content, never user input.
///
/// # Errors
///
/// Returns an error if the fragment template does not exist or rendering fails.
pub fn page(
    v: &impl ViewRenderer,
    slug: &str,
    user_name: Option<&str>,
    base_url: &str,
) -> Result<Response> {
    let body = v.render(
        &format!("site/pages/{slug}.html"),
        data!({ "user_name": user_name }),
    )?;
    let title = slug
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    format::render().view(
        v,
        "site/page_frame.html",
        data!({
            "body": body,
            "title": title,
            "slug": slug,
            "user_name": user_name,
            "base_url": base_url,
        }),
    )
}
