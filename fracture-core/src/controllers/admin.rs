use axum_extra::extract::CookieJar;
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, PaginatorTrait, QueryOrder};

use crate::controllers::middleware;
use crate::models::organizations as org_model;
use crate::views;
use crate::{require_platform_admin, require_user};

/// `GET /admin` — platform admin dashboard.
///
/// # Errors
///
/// Returns an error if the user is not authenticated or not a platform admin.
#[debug_handler]
pub async fn dashboard(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;

    let total_orgs = org_model::Entity::find().count(&ctx.db).await.unwrap_or(0);
    let total_users = crate::models::_entities::users::Entity::find()
        .count(&ctx.db)
        .await
        .unwrap_or(0);
    let total_blog_posts = crate::models::_entities::blog_posts::Entity::find()
        .count(&ctx.db)
        .await
        .unwrap_or(0);

    views::admin::dashboard(
        &v,
        &user,
        &org_ctx,
        &user_orgs,
        total_orgs,
        total_users,
        total_blog_posts,
    )
}

/// `GET /admin/orgs` — list all organizations (platform admin).
///
/// # Errors
///
/// Returns an error if the user is not authenticated or not a platform admin.
#[debug_handler]
pub async fn orgs(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;

    let all_orgs = org_model::Entity::find()
        .order_by_asc(org_model::Column::Name)
        .all(&ctx.db)
        .await
        .unwrap_or_default();

    views::admin::orgs(&v, &user, &org_ctx, &user_orgs, &all_orgs)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/admin")
        .add("/", get(dashboard))
        .add("/orgs", get(orgs))
}
