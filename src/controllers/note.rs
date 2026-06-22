use axum::response::Redirect;
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use fracture_core::permissions::{COMMENT, DELETE, EDIT};

use super::middleware;
use crate::models::_entities::notes::{ActiveModel, Model};
use crate::models::_entities::users as users_entity;
use crate::models::org_members::OrgRole;
use crate::models::organizations as org_model;
use crate::models::{note_comments, projects};
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
    // Only offer Edit/Delete the viewer can actually use (clients can't mutate
    // notes) — no buttons that lead to a denial. COMMENT gates the reply box.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &item).await?;
    let can_comment = caps.allows(COMMENT);

    // Build the comment timeline: author name, timestamp, an "edited" marker,
    // and per-comment edit/delete capability (author or staff).
    let comments =
        note_comments::Model::find_by_note_and_org(&ctx.db, item.id, org_ctx.org.id).await?;
    let author_ids: Vec<i32> = comments.iter().map(|c| c.author_id).collect();
    let authors = users_entity::Entity::find()
        .filter(users_entity::Column::Id.is_in(author_ids))
        .all(&ctx.db)
        .await?;
    let name_by_id: std::collections::HashMap<i32, String> =
        authors.into_iter().map(|u| (u.id, u.name)).collect();
    let mut comment_views = Vec::with_capacity(comments.len());
    for c in &comments {
        let comment_caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, c).await?;
        comment_views.push(serde_json::json!({
            "pid": c.pid.to_string(),
            "body": c.body,
            "author": name_by_id.get(&c.author_id).cloned().unwrap_or_else(|| "Unknown".to_string()),
            "created_at": c.created_at.to_string(),
            "edited": c.updated_at > c.created_at,
            "can_edit": comment_caps.allows(EDIT),
            "can_delete": comment_caps.allows(DELETE),
        }));
    }

    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    let data = views::note::ShowData {
        project: &project,
        note: &item,
        caps: &caps,
        can_comment,
        comments: comment_views,
    };
    views::note::show(&v, &user, &org_ctx, &user_orgs, &data)
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
