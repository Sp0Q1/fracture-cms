use std::collections::HashMap;

use axum::extract::Query;
use axum::response::Redirect;
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, QueryOrder};

use crate::controllers::middleware;
use crate::entity_registry::{entity_registry, AdminEntity, FormField, ListQuery};
use crate::models::organizations as org_model;
use crate::views;
use crate::views::admin::EntityStat;
use crate::{require_staff, require_user};

/// Returns the entity's form fields with values pre-filled from `values`
/// (submitted body or a loaded record), so create errors and the edit form
/// both round-trip the user's input.
fn prefill(entity: &dyn AdminEntity, values: &serde_json::Value) -> Vec<FormField> {
    entity
        .form_fields()
        .into_iter()
        .map(|f| {
            let v = match values.get(f.name) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Bool(b)) => {
                    if *b {
                        "on".to_string()
                    } else {
                        String::new()
                    }
                }
                Some(other) if !other.is_null() => other.to_string(),
                _ => String::new(),
            };
            f.with_value(v)
        })
        .collect()
}

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
        entity.creatable(),
    )
}

/// `GET /admin/list/{slug}/{pid}` — generic detail page for one row.
///
/// # Errors
///
/// Returns an error if the user is not staff or the row is not found.
#[debug_handler]
pub async fn detail(
    Path((slug, pid)): Path<(String, String)>,
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

    let entity = entity_registry().find(&slug).ok_or(Error::NotFound)?;
    let record = entity
        .load(&ctx.db, &pid)
        .await
        .map_err(|_| Error::NotFound)?
        .ok_or(Error::NotFound)?;

    views::admin::detail(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &slug,
        entity.entity_name(),
        &record,
        entity.editable(),
        // Deletability is enforced in `delete`; the button is shown whenever the
        // entity supports the generic forms at all.
        entity.editable(),
    )
}

/// `GET /admin/list/{slug}/new` — generic create form.
///
/// # Errors
///
/// Returns an error if the user is not staff or the entity is not creatable.
#[debug_handler]
pub async fn new_form(
    Path(slug): Path<String>,
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

    let entity = entity_registry().find(&slug).ok_or(Error::NotFound)?;
    if !entity.creatable() {
        return Err(Error::NotFound);
    }

    views::admin::form(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &slug,
        entity.entity_name(),
        &entity.form_fields(),
        None,
        None,
    )
}

/// `POST /admin/list/{slug}` — create a row from the generic form.
///
/// # Errors
///
/// Returns an error if the user is not staff or the entity is not creatable.
#[debug_handler]
#[allow(clippy::implicit_hasher)]
pub async fn create(
    Path(slug): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(body): Form<HashMap<String, String>>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_staff!(org_ctx);

    let entity = entity_registry().find(&slug).ok_or(Error::NotFound)?;
    if !entity.creatable() {
        return Err(Error::NotFound);
    }

    match entity.create(&ctx.db, user.id, &body).await {
        Ok(()) => Ok(Redirect::to(&format!("/admin/list/{slug}")).into_response()),
        Err(e) => {
            // Re-render the form with the submitted values and the error.
            let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
                .await
                .unwrap_or_default();
            let fields = prefill(entity, &serde_json::json!(body));
            views::admin::form(
                &v,
                &user,
                org_ctx.as_ref(),
                &user_orgs,
                &slug,
                entity.entity_name(),
                &fields,
                None,
                Some(&db_err_message(&e)),
            )
        }
    }
}

/// `GET /admin/list/{slug}/{pid}/edit` — generic edit form.
///
/// # Errors
///
/// Returns an error if the user is not staff or the row is not found.
#[debug_handler]
pub async fn edit_form(
    Path((slug, pid)): Path<(String, String)>,
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

    let entity = entity_registry().find(&slug).ok_or(Error::NotFound)?;
    if !entity.editable() {
        return Err(Error::NotFound);
    }
    let record = entity
        .load(&ctx.db, &pid)
        .await
        .map_err(|_| Error::NotFound)?
        .ok_or(Error::NotFound)?;
    let fields = prefill(entity, &record);

    views::admin::form(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &slug,
        entity.entity_name(),
        &fields,
        Some(&pid),
        None,
    )
}

/// `POST /admin/list/{slug}/{pid}` — apply an edit from the generic form.
///
/// # Errors
///
/// Returns an error if the user is not staff or the row is not found.
#[debug_handler]
#[allow(clippy::implicit_hasher)]
pub async fn update(
    Path((slug, pid)): Path<(String, String)>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(body): Form<HashMap<String, String>>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_staff!(org_ctx);

    let entity = entity_registry().find(&slug).ok_or(Error::NotFound)?;
    if !entity.editable() {
        return Err(Error::NotFound);
    }

    match entity.update(&ctx.db, &pid, &body).await {
        Ok(()) => Ok(Redirect::to(&format!("/admin/list/{slug}/{pid}")).into_response()),
        Err(e) => {
            let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
                .await
                .unwrap_or_default();
            let fields = prefill(entity, &serde_json::json!(body));
            views::admin::form(
                &v,
                &user,
                org_ctx.as_ref(),
                &user_orgs,
                &slug,
                entity.entity_name(),
                &fields,
                Some(&pid),
                Some(&db_err_message(&e)),
            )
        }
    }
}

/// `POST /admin/list/{slug}/{pid}/delete` — delete a row.
///
/// # Errors
///
/// Returns an error if the user is not staff or the row is not found.
#[debug_handler]
pub async fn delete(
    Path((slug, pid)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_staff!(org_ctx);

    let entity = entity_registry().find(&slug).ok_or(Error::NotFound)?;
    match entity.delete(&ctx.db, &pid).await {
        // On success, back to the list. On a guard failure, back to the detail
        // page (the message is surfaced there on the next load).
        Ok(()) => Ok(Redirect::to(&format!("/admin/list/{slug}")).into_response()),
        Err(_) => Ok(Redirect::to(&format!("/admin/list/{slug}/{pid}")).into_response()),
    }
}

/// Extracts the user-facing message from a `DbErr::Custom`, falling back to a
/// generic message for other database errors (which shouldn't leak internals).
fn db_err_message(e: &sea_orm::DbErr) -> String {
    match e {
        sea_orm::DbErr::Custom(msg) => msg.clone(),
        _ => "Could not save changes.".to_string(),
    }
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/admin")
        .add("/", get(dashboard))
        .add("/orgs", get(orgs))
        .add("/list/{slug}", get(list).post(create))
        .add("/list/{slug}/new", get(new_form))
        .add("/list/{slug}/{pid}", get(detail).post(update))
        .add("/list/{slug}/{pid}/edit", get(edit_form))
        .add("/list/{slug}/{pid}/delete", post(delete))
}
