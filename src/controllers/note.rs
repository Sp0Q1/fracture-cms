use axum::response::Redirect;
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use fracture_core::permissions::{DELETE, EDIT};

use super::middleware;
use crate::models::_entities::notes::{ActiveModel, Model};
use crate::models::org_members::OrgRole;
use crate::models::organizations as org_model;
use crate::models::projects;
use crate::{require_capability, require_platform_admin, require_role, require_user, views};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub title: String,
    pub body: Option<String>,
}

impl Params {
    fn update(&self, item: &mut ActiveModel) {
        item.title = Set(self.title.clone());
        item.body = Set(self.body.clone());
    }
}

/// Helper: resolve project from `pid`, scoped to current org.
async fn resolve_project(
    db: &DatabaseConnection,
    project_pid: &str,
    org_id: i32,
) -> Result<projects::Model> {
    projects::Model::find_by_pid_and_org(db, project_pid, org_id)
        .await?
        .ok_or_else(|| Error::NotFound)
}

/// `GET /projects/:project_pid/notes/new` -- new note form.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn new(
    Path(project_pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    // Notes are staff-authored; clients have read-only access. Creation is
    // therefore staff-only (an app that wants org authoring relaxes this).
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let org_ctx = org_ctx.ok_or_else(|| Error::NotFound)?;
    let project = resolve_project(&ctx.db, &project_pid, org_ctx.org.id).await?;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::note::create(&v, &user, &org_ctx, &user_orgs, &project)
}

/// `POST /projects/:project_pid/notes/` -- create note.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn add(
    Path(project_pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    // Staff-only authoring (see `new`).
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let org_ctx = org_ctx.ok_or_else(|| Error::NotFound)?;
    let project = resolve_project(&ctx.db, &project_pid, org_ctx.org.id).await?;

    // SeaORM generates multiple default() impls for ActiveModel
    #[allow(clippy::default_trait_access)]
    let mut item: ActiveModel = Default::default();
    params.update(&mut item);
    item.project_id = Set(project.id);
    item.org_id = Set(org_ctx.org.id);
    item.created_by = Set(Some(user.id));
    // Authored by staff, so it's staff-owned: clients keep read-only access.
    item.owner_tier = Set("staff".to_string());
    item.insert(&ctx.db).await?;
    Ok(Redirect::to(&format!("/projects/{project_pid}")).into_response())
}

/// `GET /projects/:project_pid/notes/:pid` -- show note.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn show(
    Path((project_pid, pid)): Path<(String, String)>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Viewer);
    let project = resolve_project(&ctx.db, &project_pid, org_ctx.org.id).await?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Only offer Edit/Delete the viewer can actually use (org members can't
    // mutate staff-owned notes) — no buttons that lead to a denial.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &item).await?;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::note::show(&v, &user, &org_ctx, &user_orgs, &project, &item, &caps)
}

/// `GET /projects/:project_pid/notes/:pid/edit` -- edit note form.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn edit(
    Path((project_pid, pid)): Path<(String, String)>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let project = resolve_project(&ctx.db, &project_pid, org_ctx.org.id).await?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Notes are read-only for clients; only staff (or a per-record grant) edit.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &item).await?;
    require_capability!(caps, EDIT);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::note::edit(&v, &user, &org_ctx, &user_orgs, &project, &item)
}

/// `POST /projects/:project_pid/notes/:pid` -- update note.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn update(
    Path((project_pid, pid)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let _project = resolve_project(&ctx.db, &project_pid, org_ctx.org.id).await?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Notes are read-only for clients; only staff (or a per-record grant) edit.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &item).await?;
    require_capability!(caps, EDIT);
    let mut item = item.into_active_model();
    params.update(&mut item);
    item.update(&ctx.db).await?;
    Ok(Redirect::to(&format!("/projects/{project_pid}")).into_response())
}

/// `DELETE /projects/:project_pid/notes/:pid` -- delete note.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn remove(
    Path((project_pid, pid)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let _project = resolve_project(&ctx.db, &project_pid, org_ctx.org.id).await?;
    let item = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Notes are read-only for clients; only staff (or a per-record grant) delete.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &item).await?;
    require_capability!(caps, DELETE);
    item.delete(&ctx.db).await?;
    format::empty()
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/projects/{project_pid}/notes")
        .add("/", post(add))
        .add("/new", get(new))
        .add("/{pid}", get(show))
        .add("/{pid}/edit", get(edit))
        .add("/{pid}", delete(remove))
        .add("/{pid}", post(update))
}
