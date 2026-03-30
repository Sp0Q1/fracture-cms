use loco_rs::prelude::*;
use serde_json::json;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{blog_posts, organizations, users};

/// Helper to serialize a blog post into JSON for templates.
fn post_json(post: &blog_posts::Model) -> serde_json::Value {
    json!({
        "id": post.id,
        "pid": post.pid.to_string(),
        "title": post.title,
        "slug": post.slug,
        "body": post.body,
        "body_html": post.body_html,
        "excerpt": post.excerpt,
        "status": post.status,
        "published_at": post.published_at.map(|d| d.to_string()),
        "meta_title": post.meta_title,
        "meta_description": post.meta_description,
        "created_at": post.created_at.to_string(),
        "updated_at": post.updated_at.to_string(),
    })
}

/// Renders the public blog index (list of published posts).
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn public_index(
    v: &impl ViewRenderer,
    posts: &[blog_posts::Model],
    base_url: &str,
) -> Result<Response> {
    let posts_json: Vec<_> = posts.iter().map(post_json).collect();
    format::render().view(
        v,
        "blog/public_index.html",
        data!({
            "posts": posts_json,
            "base_url": base_url,
        }),
    )
}

/// Renders a single published blog post.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn public_show(
    v: &impl ViewRenderer,
    post: &blog_posts::Model,
    author_name: &str,
    base_url: &str,
) -> Result<Response> {
    let post_data = post_json(post);
    format::render().view(
        v,
        "blog/public_show.html",
        data!({
            "post": post_data,
            "author_name": author_name,
            "base_url": base_url,
        }),
    )
}

/// Renders the admin blog post list.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn admin_index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    posts: &[blog_posts::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["posts"] = json!(posts.iter().map(post_json).collect::<Vec<_>>());
    format::render().view(v, "blog/admin_index.html", data!(ctx))
}

/// Renders the admin new blog post form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn admin_new(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
) -> Result<Response> {
    let ctx = super::base_context(user, org_ctx, user_orgs);
    format::render().view(v, "blog/admin_new.html", data!(ctx))
}

/// Renders the admin edit blog post form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn admin_edit(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    post: &blog_posts::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["post"] = post_json(post);
    format::render().view(v, "blog/admin_edit.html", data!(ctx))
}
