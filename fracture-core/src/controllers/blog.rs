use axum::response::Redirect;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::Form;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::controllers::middleware;
use crate::models::_entities::{blog_posts, organizations, users as users_entity};
use crate::models::{blog_posts as blog_model, organizations as org_model};
use crate::views;
use crate::{require_platform_admin, require_user};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlogPostParams {
    pub title: String,
    pub slug: Option<String>,
    pub body: String,
    pub excerpt: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
}

/// Resolves the blog org from the config setting `settings.blog.org_slug`.
/// `Ok(None)` means the blog is not configured; database errors propagate
/// instead of silently rendering an empty blog.
async fn resolve_blog_org(ctx: &AppContext) -> Result<Option<organizations::Model>> {
    let Some(slug) = blog_model::Model::get_blog_org_slug(&ctx.config) else {
        return Ok(None);
    };
    Ok(org_model::Model::find_by_slug(&ctx.db, &slug).await?)
}

/// Resolves the blog org for admin mutations, failing loudly when unset.
async fn require_blog_org(ctx: &AppContext) -> Result<organizations::Model> {
    resolve_blog_org(ctx)
        .await?
        .ok_or_else(|| Error::Message("Blog org not configured".to_string()))
}

/// GET /blog/ — public blog index (no auth required)
///
/// # Errors
///
/// Returns an error if the database query fails.
#[debug_handler]
pub async fn public_index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let org = resolve_blog_org(&ctx).await?;
    let posts = match org {
        Some(ref o) => blog_model::Model::find_published_by_org(&ctx.db, o.id).await?,
        None => vec![],
    };
    let base_url = ctx.config.server.host.clone();
    // Signed-in visitors get the Dashboard CTA in the nav; only the guest
    // variant (identical for everyone) is cacheable.
    let user_name = middleware::get_current_user(&jar, &ctx)
        .await
        .map(|u| u.name);
    let res = views::blog::public_index(&v, &posts, &base_url, user_name.as_deref())?;
    Ok(if user_name.is_none() {
        super::cache_public(res, 60)
    } else {
        res
    })
}

/// GET /blog/feed.xml — Atom feed of published posts (no auth required)
///
/// # Errors
///
/// Returns an error if the database query fails.
#[debug_handler]
pub async fn public_feed(State(ctx): State<AppContext>) -> Result<Response> {
    let org = resolve_blog_org(&ctx).await?;
    let posts = match org {
        Some(ref o) => blog_model::Model::find_published_by_org(&ctx.db, o.id).await?,
        None => vec![],
    };
    let base_url = ctx.config.server.host.clone();
    let xml = views::blog::atom_feed(&posts, &base_url);
    let res = Response::builder()
        .header("content-type", "application/atom+xml; charset=utf-8")
        .body(axum::body::Body::from(xml))
        .map_err(|e| Error::Message(format!("feed response: {e}")))?;
    Ok(super::cache_public(res.into_response(), 300))
}

/// GET /blog/:slug — public blog post (no auth required)
///
/// # Errors
///
/// Returns an error if the database query fails.
#[debug_handler]
pub async fn public_show(
    Path(slug): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let org = resolve_blog_org(&ctx)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let post = blog_model::Model::find_published_by_slug(&ctx.db, org.id, &slug)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let author = users_entity::Entity::find_by_id(post.author_id)
        .one(&ctx.db)
        .await?;
    let author_name = author.map_or_else(|| "Unknown".to_string(), |a| a.name);
    let base_url = ctx.config.server.host.clone();
    let user_name = middleware::get_current_user(&jar, &ctx)
        .await
        .map(|u| u.name);
    let res = views::blog::public_show(
        &v,
        &post,
        &author_name,
        &base_url,
        false,
        user_name.as_deref(),
    )?;
    Ok(if user_name.is_none() {
        super::cache_public(res, 60)
    } else {
        res
    })
}

/// GET /admin/blog/ — admin blog post list
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not a platform admin.
#[debug_handler]
pub async fn admin_index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let blog_org = resolve_blog_org(&ctx).await?;
    let posts = match blog_org {
        Some(ref o) => blog_model::Model::find_all_by_org(&ctx.db, o.id).await?,
        None => vec![],
    };
    views::blog::admin_index(&v, &user, org_ctx.as_ref(), &user_orgs, &posts)
}

/// GET /admin/blog/new — new blog post form
///
/// # Errors
///
/// Returns an error if the user is not a platform admin.
#[debug_handler]
pub async fn admin_new(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::blog::admin_new(&v, &user, org_ctx.as_ref(), &user_orgs)
}

/// POST /admin/blog/ — create a new blog post
///
/// # Errors
///
/// Returns an error if the database operation fails or the user is not a platform admin.
#[debug_handler]
pub async fn admin_create(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<BlogPostParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let blog_org = require_blog_org(&ctx).await?;

    // Generate slug from title if not provided
    let base_slug = if let Some(ref s) = params.slug {
        if s.is_empty() {
            slug::slugify(&params.title)
        } else {
            slug::slugify(s)
        }
    } else {
        slug::slugify(&params.title)
    };

    let mut post_slug = base_slug.clone();
    let mut suffix = 1u32;
    while blog_posts::Entity::find()
        .filter(blog_posts::Column::OrgId.eq(blog_org.id))
        .filter(blog_posts::Column::Slug.eq(&post_slug))
        .one(&ctx.db)
        .await?
        .is_some()
    {
        suffix += 1;
        post_slug = format!("{base_slug}-{suffix}");
    }

    blog_posts::ActiveModel {
        org_id: sea_orm::ActiveValue::Set(blog_org.id),
        author_id: sea_orm::ActiveValue::Set(user.id),
        title: sea_orm::ActiveValue::Set(params.title),
        slug: sea_orm::ActiveValue::Set(post_slug),
        body: sea_orm::ActiveValue::Set(params.body),
        excerpt: sea_orm::ActiveValue::Set(params.excerpt.filter(|s| !s.is_empty())),
        meta_title: sea_orm::ActiveValue::Set(params.meta_title.filter(|s| !s.is_empty())),
        meta_description: sea_orm::ActiveValue::Set(
            params.meta_description.filter(|s| !s.is_empty()),
        ),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await?;

    Ok(Redirect::to("/admin/blog").into_response())
}

/// GET /admin/blog/:pid/edit — edit blog post form
///
/// # Errors
///
/// Returns an error if the post is not found or the user is not a platform admin.
#[debug_handler]
pub async fn admin_edit(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let blog_org = require_blog_org(&ctx).await?;
    let post = blog_model::Model::find_by_pid_and_org(&ctx.db, &pid, blog_org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    views::blog::admin_edit(&v, &user, org_ctx.as_ref(), &user_orgs, &post)
}

/// POST /admin/blog/:pid — update a blog post
///
/// # Errors
///
/// Returns an error if the post is not found or the user is not a platform admin.
#[debug_handler]
pub async fn admin_update(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<BlogPostParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let blog_org = require_blog_org(&ctx).await?;
    let post = blog_model::Model::find_by_pid_and_org(&ctx.db, &pid, blog_org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    let mut active: blog_posts::ActiveModel = post.into();
    active.title = sea_orm::ActiveValue::Set(params.title);
    active.body = sea_orm::ActiveValue::Set(params.body);
    active.excerpt = sea_orm::ActiveValue::Set(params.excerpt.filter(|s| !s.is_empty()));
    active.meta_title = sea_orm::ActiveValue::Set(params.meta_title.filter(|s| !s.is_empty()));
    active.meta_description =
        sea_orm::ActiveValue::Set(params.meta_description.filter(|s| !s.is_empty()));
    active.update(&ctx.db).await?;

    Ok(Redirect::to("/admin/blog").into_response())
}

/// POST /admin/blog/:pid/publish — publish a blog post
///
/// # Errors
///
/// Returns an error if the post is not found or the user is not a platform admin.
#[debug_handler]
pub async fn admin_publish(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let blog_org = require_blog_org(&ctx).await?;
    let post = blog_model::Model::find_by_pid_and_org(&ctx.db, &pid, blog_org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // The first publication date is the post's permanent date — republishing
    // after a temporary unpublish must not bump it (feed/order stability).
    let first_published = post.published_at;
    let mut active: blog_posts::ActiveModel = post.into();
    active.status = sea_orm::ActiveValue::Set("published".to_string());
    if first_published.is_none() {
        active.published_at = sea_orm::ActiveValue::Set(Some(chrono::Utc::now().into()));
    }
    active.update(&ctx.db).await?;

    Ok(Redirect::to("/admin/blog").into_response())
}

/// POST /admin/blog/:pid/unpublish — unpublish a blog post
///
/// # Errors
///
/// Returns an error if the post is not found or the user is not a platform admin.
#[debug_handler]
pub async fn admin_unpublish(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let blog_org = require_blog_org(&ctx).await?;
    let post = blog_model::Model::find_by_pid_and_org(&ctx.db, &pid, blog_org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Only the status changes; published_at stays as the first publication
    // date so republishing doesn't reorder the blog or the feed.
    let mut active: blog_posts::ActiveModel = post.into();
    active.status = sea_orm::ActiveValue::Set("draft".to_string());
    active.update(&ctx.db).await?;

    Ok(Redirect::to("/admin/blog").into_response())
}

/// GET /admin/blog/:pid/preview — render a post (any status) with the public
/// template, marked as a preview.
///
/// # Errors
///
/// Returns an error if the post is not found or the user is not a platform admin.
#[debug_handler]
pub async fn admin_preview(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let blog_org = require_blog_org(&ctx).await?;
    let post = blog_model::Model::find_by_pid_and_org(&ctx.db, &pid, blog_org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let author = users_entity::Entity::find_by_id(post.author_id)
        .one(&ctx.db)
        .await?;
    let author_name = author.map_or_else(|| "Unknown".to_string(), |a| a.name);
    let base_url = ctx.config.server.host.clone();
    views::blog::public_show(&v, &post, &author_name, &base_url, true, Some(&user.name))
}

/// POST /admin/blog/:pid/delete — permanently delete a blog post.
///
/// # Errors
///
/// Returns an error if the post is not found or the user is not a platform admin.
#[debug_handler]
pub async fn admin_delete(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let blog_org = require_blog_org(&ctx).await?;
    let post = blog_model::Model::find_by_pid_and_org(&ctx.db, &pid, blog_org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let active: blog_posts::ActiveModel = post.into();
    active.delete(&ctx.db).await?;

    Ok(Redirect::to("/admin/blog").into_response())
}

pub fn public_routes() -> Routes {
    Routes::new()
        .prefix("/blog")
        .add("/", get(public_index))
        .add("/feed.xml", get(public_feed))
        .add("/{slug}", get(public_show))
}

pub fn admin_routes() -> Routes {
    Routes::new()
        .prefix("/admin/blog")
        .add("/", get(admin_index))
        .add("/", post(admin_create))
        .add("/new", get(admin_new))
        .add("/{pid}/edit", get(admin_edit))
        .add("/{pid}", post(admin_update))
        .add("/{pid}/publish", post(admin_publish))
        .add("/{pid}/unpublish", post(admin_unpublish))
        .add("/{pid}/preview", get(admin_preview))
        .add("/{pid}/delete", post(admin_delete))
}
