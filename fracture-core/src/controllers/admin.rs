use std::collections::HashMap;

use axum::extract::Query;
use axum_extra::extract::CookieJar;
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, QueryOrder};

use crate::controllers::middleware;
use crate::entity_registry::{entity_registry, ListQuery};
use crate::models::organizations as org_model;
use crate::views;
use crate::views::admin::EntityStat;
use crate::{require_staff, require_user};

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
    require_staff!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let registry = entity_registry();
    let mut stats = Vec::new();
    for entity in registry.entities() {
        let count = entity.count_all(&ctx.db).await;
        // Prefer the generic changelist for listable entities; otherwise fall
        // back to a bespoke management page (e.g. blog) if one exists.
        let url = if entity.listable() {
            format!("/admin/list/{}", entity.slug())
        } else {
            entity.url_prefix().to_string()
        };
        stats.push(EntityStat {
            name: entity.entity_name().to_string(),
            count,
            url,
            description: entity.description().to_string(),
            action_label: entity.action_label().to_string(),
        });
    }

    views::admin::dashboard(&v, &user, org_ctx.as_ref(), &user_orgs, &stats)
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
    require_staff!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let all_orgs = org_model::Entity::find()
        .order_by_asc(org_model::Column::Name)
        .all(&ctx.db)
        .await
        .unwrap_or_default();

    views::admin::orgs(&v, &user, org_ctx.as_ref(), &user_orgs, &all_orgs)
}

/// `GET /admin/list/{slug}` — generic, staff-only changelist for a registered
/// entity: search + sortable columns + pagination (Django's changelist).
///
/// # Errors
///
/// Returns an error if the user is not staff or the slug is unknown.
#[debug_handler]
// axum's Query extractor requires a concrete HashMap (no custom hasher).
#[allow(clippy::implicit_hasher)]
pub async fn list(
    Path(slug): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_staff!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let entity = entity_registry()
        .find(&slug)
        .ok_or_else(|| Error::NotFound)?;
    let query = ListQuery::from_params(&params);
    let page = entity
        .list(&ctx.db, &query)
        .await
        .map_err(|_| Error::NotFound)?;

    views::admin::list(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &slug,
        entity.entity_name(),
        &page,
    )
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/admin")
        .add("/", get(dashboard))
        .add("/orgs", get(orgs))
        .add("/list/{slug}", get(list))
}
