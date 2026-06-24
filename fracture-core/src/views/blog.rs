use std::fmt::Write;

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
    nav: Option<&serde_json::Value>,
) -> Result<Response> {
    let posts_json: Vec<_> = posts.iter().map(post_json).collect();
    let ctx = super::with_nav(
        json!({
            "posts": posts_json,
            "base_url": base_url,
        }),
        nav,
    );
    format::render().view(v, "blog/public_index.html", data!(ctx))
}

/// Renders a single blog post with the public template. `preview` shows the
/// draft-preview banner (admin preview of unpublished posts).
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn public_show(
    v: &impl ViewRenderer,
    post: &blog_posts::Model,
    author_name: &str,
    base_url: &str,
    preview: bool,
    nav: Option<&serde_json::Value>,
) -> Result<Response> {
    let post_data = post_json(post);
    let ctx = super::with_nav(
        json!({
            "post": post_data,
            "author_name": author_name,
            "base_url": base_url,
            "preview": preview,
            // JSON-LD built server-side: serde_json escapes the string values
            // correctly for a JSON/script context (HTML autoescape would not),
            // and `<` is escaped so no value can break out of the <script>.
            "structured_data": blog_jsonld(post, author_name, base_url),
        }),
        nav,
    );
    format::render().view(v, "blog/public_show.html", data!(ctx))
}

/// Serializes the `BlogPosting` JSON-LD for a post, safe to emit inside a
/// `<script type="application/ld+json">` (all values JSON-encoded; `<`
/// escaped to `<` so `</script>` / `<!--` can't terminate the block).
fn blog_jsonld(post: &blog_posts::Model, author_name: &str, base_url: &str) -> String {
    let mut doc = json!({
        "@context": "https://schema.org",
        "@type": "BlogPosting",
        "headline": post.title,
        "url": format!("{base_url}/blog/{}", post.slug),
        "dateModified": post.updated_at.to_rfc3339(),
        "author": { "@type": "Person", "name": author_name },
    });
    if let Some(published) = post.published_at {
        doc["datePublished"] = json!(published.to_rfc3339());
    }
    if let Some(desc) = &post.meta_description {
        doc["description"] = json!(desc);
    }
    doc.to_string().replace('<', "\\u003c")
}

/// Minimal XML text escaping for the Atom feed.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Builds an Atom feed document for the published posts (newest first).
#[must_use]
pub fn atom_feed(posts: &[blog_posts::Model], base_url: &str) -> String {
    let feed_updated = posts
        .iter()
        .filter_map(|p| p.published_at)
        .max()
        .map_or_else(|| chrono::Utc::now().to_rfc3339(), |d| d.to_rfc3339());
    let mut entries = String::new();
    for post in posts {
        let url = format!("{base_url}/blog/{}", post.slug);
        let updated = post
            .published_at
            .map_or_else(|| post.updated_at.to_rfc3339(), |d| d.to_rfc3339());
        let summary = post.excerpt.as_deref().map_or_else(String::new, |e| {
            format!("\n    <summary>{}</summary>", xml_escape(e))
        });
        let _ = write!(
            entries,
            r#"  <entry>
    <title>{title}</title>
    <id>{url}</id>
    <link rel="alternate" type="text/html" href="{url}"/>
    <updated>{updated}</updated>{summary}
    <content type="html">{content}</content>
  </entry>
"#,
            title = xml_escape(&post.title),
            content = xml_escape(&post.body_html),
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Blog</title>
  <id>{base_url}/blog</id>
  <link rel="alternate" type="text/html" href="{base_url}/blog"/>
  <link rel="self" type="application/atom+xml" href="{base_url}/blog/feed.xml"/>
  <updated>{feed_updated}</updated>
{entries}</feed>
"#
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
    org_ctx: Option<&OrgContext>,
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
    org_ctx: Option<&OrgContext>,
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
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    post: &blog_posts::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["post"] = post_json(post);
    format::render().view(v, "blog/admin_edit.html", data!(ctx))
}
