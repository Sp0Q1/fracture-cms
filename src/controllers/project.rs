use std::collections::HashMap;

use axum::extract::Query;
use axum::response::Redirect;
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use sea_orm::{ColumnTrait, Condition, Order, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};

use fracture_core::listing::{paginate_models, ListColumn, ListQuery};
use fracture_core::permissions::{DELETE, EDIT, VIEW};

use super::middleware;
use crate::authz;
use crate::models::_entities::projects::{ActiveModel, Model};
use crate::models::org_members::OrgRole;
use crate::models::organizations as org_model;
use crate::{require_role, require_user, views};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub title: String,
    pub description: Option<String>,
}

impl Params {
    fn update(&self, item: &mut ActiveModel) {
        item.title = Set(self.title.clone());
        item.description = Set(self.description.clone());
    }
}

/// `GET /projects/` -- list org projects.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
#[allow(clippy::implicit_hasher)] // axum's Query extractor requires a concrete HashMap.
pub async fn list(
    Query(params): Query<HashMap<String, String>>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    use crate::models::_entities::projects::{Column, Entity};
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Viewer);

    // Org-scoped, searchable, sortable, paginated — through the same shared
    // listing framework as every other table (admin changelist included).
    let q = ListQuery::from_params(&params).with_default_sort("title", false);
    let mut query = Entity::find().filter(Column::OrgId.eq(org_ctx.org.id));
    if let Some(s) = &q.q {
        query = query.filter(
            Condition::any()
                .add(Column::Title.contains(s))
                .add(Column::Description.contains(s)),
        );
    }
    let dir = if q.desc { Order::Desc } else { Order::Asc };
    query = match q.sort.as_deref() {
        Some("description") => query.order_by(Column::Description, dir),
        Some("created_at") => query.order_by(Column::CreatedAt, dir),
        _ => query.order_by(Column::Title, dir),
    };
    let columns = vec![
        ListColumn::sortable("title", "Title"),
        ListColumn::plain("description", "Description"),
        ListColumn::sortable("created_at", "Created"),
    ];
    let page = paginate_models(&ctx.db, query, &q, columns, |m| {
        serde_json::json!({
            "title": m.title,
            "description": m.description,
            "created_at": m.created_at.format("%Y-%m-%d").to_string(),
            "_url": format!("/projects/{}", m.pid),
        })
    })
    .await?;

    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::project::list(&v, &user, &org_ctx, &user_orgs, &page)
}

/// `GET /projects/new` -- new project form.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn new(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Member);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::project::create(&v, &user, &org_ctx, &user_orgs)
}

/// `POST /projects/` -- create project.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn add(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Member);

    // SeaORM generates multiple default() impls for ActiveModel
    #[allow(clippy::default_trait_access)]
    let mut item: ActiveModel = Default::default();
    params.update(&mut item);
    item.org_id = Set(org_ctx.org.id);
    item.created_by = Set(Some(user.id));
    // A project created by platform/staff is owned at the platform tier, which
    // caps the local org tiers (even Owner) per ProjectPolicy; a member-created
    // one is org-owned. This is the both-directions switch.
    item.owner_tier = Set(if org_ctx.is_staff {
        "platform".to_string()
    } else {
        "org".to_string()
    });
    item.insert(&ctx.db).await?;
    Ok(Redirect::to("/projects").into_response())
}

/// `GET /projects/:pid` -- show project.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
#[allow(clippy::implicit_hasher)] // axum's Query extractor requires a concrete HashMap.
pub async fn show(
    Path(pid): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Per-resource capabilities replace the blanket role check: a staff-owned
    // project caps even the org Owner at view+comment (see src/authz.rs).
    let caps = authz::project_capabilities(&ctx.db, user.id, org_ctx.is_staff, org_ctx.role, &item)
        .await?;
    if !caps.allows(VIEW) {
        return Err(Error::NotFound);
    }

    // The project's notes render through the same shared listing framework
    // (search + sort + paginate), scoped to this project.
    let q = ListQuery::from_params(&params).with_default_sort("title", false);
    let project_pid = item.pid.to_string();
    let notes_page = {
        use crate::models::_entities::notes::{Column, Entity};
        let mut query = Entity::find()
            .filter(Column::ProjectId.eq(item.id))
            .filter(Column::OrgId.eq(org_ctx.org.id));
        if let Some(s) = &q.q {
            query = query.filter(Column::Title.contains(s));
        }
        let dir = if q.desc { Order::Desc } else { Order::Asc };
        query = match q.sort.as_deref() {
            Some("created_at") => query.order_by(Column::CreatedAt, dir),
            _ => query.order_by(Column::Title, dir),
        };
        let columns = vec![
            ListColumn::sortable("title", "Title"),
            ListColumn::sortable("created_at", "Created"),
        ];
        paginate_models(&ctx.db, query, &q, columns, |m| {
            serde_json::json!({
                "title": m.title,
                "created_at": m.created_at.format("%Y-%m-%d").to_string(),
                "_url": format!("/projects/{project_pid}/notes/{}", m.pid),
            })
        })
        .await?
    };

    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::project::show(&v, &user, &org_ctx, &user_orgs, &item, &notes_page, &caps)
}

/// `GET /projects/:pid/edit` -- edit project form.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn edit(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let caps = authz::project_capabilities(&ctx.db, user.id, org_ctx.is_staff, org_ctx.role, &item)
        .await?;
    if !caps.allows(EDIT) {
        return Err(Error::NotFound);
    }
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::project::edit(&v, &user, &org_ctx, &user_orgs, &item)
}

/// `POST /projects/:pid` -- update project.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn update(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let caps = authz::project_capabilities(&ctx.db, user.id, org_ctx.is_staff, org_ctx.role, &item)
        .await?;
    if !caps.allows(EDIT) {
        return Err(Error::NotFound);
    }
    let mut item = item.into_active_model();
    params.update(&mut item);
    item.update(&ctx.db).await?;
    Ok(Redirect::to("/projects").into_response())
}

/// `DELETE /projects/:pid` -- delete project.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn remove(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let caps = authz::project_capabilities(&ctx.db, user.id, org_ctx.is_staff, org_ctx.role, &item)
        .await?;
    if !caps.allows(DELETE) {
        return Err(Error::NotFound);
    }
    item.delete(&ctx.db).await?;
    format::empty()
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/projects")
        .add("/", get(list))
        .add("/", post(add))
        .add("/new", get(new))
        .add("/{pid}", get(show))
        .add("/{pid}/edit", get(edit))
        .add("/{pid}", delete(remove))
        .add("/{pid}", post(update))
}
