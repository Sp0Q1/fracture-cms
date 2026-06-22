use axum::response::Redirect;
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use fracture_core::permissions::{COMMENT, DELETE, EDIT};

use super::middleware;
use crate::models::_entities::note_comments::{ActiveModel, Model};
use crate::models::notes;
use crate::models::organizations as org_model;
use crate::{require_capability, require_user, views};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub body: String,
}

/// Resolves a note by pid, scoped to the current org.
async fn resolve_note(
    db: &DatabaseConnection,
    note_pid: &str,
    org_id: i32,
) -> Result<notes::Model> {
    notes::Model::find_by_pid_and_org(db, note_pid, org_id)
        .await?
        .ok_or_else(|| Error::NotFound)
}

/// `POST /projects/:project_pid/notes/:note_pid/comments` — add a comment.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn add(
    Path((project_pid, note_pid)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let note = resolve_note(&ctx.db, &note_pid, org_ctx.org.id).await?;
    // Posting a comment requires the COMMENT capability on the note.
    let note_caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &note).await?;
    require_capability!(note_caps, COMMENT);

    ActiveModel {
        note_id: Set(note.id),
        org_id: Set(org_ctx.org.id),
        author_id: Set(user.id),
        body: Set(params.body),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await?;
    Ok(Redirect::to(&format!("/projects/{project_pid}/notes/{note_pid}")).into_response())
}

/// `GET /projects/:project_pid/notes/:note_pid/comments/:pid/edit` — edit form.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn edit(
    Path((project_pid, note_pid, pid)): Path<(String, String, String)>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let note = resolve_note(&ctx.db, &note_pid, org_ctx.org.id).await?;
    let comment = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Only the author (or staff) may edit a comment.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &comment).await?;
    require_capability!(caps, EDIT);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::comment::edit(
        &v,
        &user,
        &org_ctx,
        &user_orgs,
        &project_pid,
        &note,
        &comment,
    )
}

/// `POST /projects/:project_pid/notes/:note_pid/comments/:pid` — update.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn update(
    Path((project_pid, note_pid, pid)): Path<(String, String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let comment = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Only the author (or staff) may edit a comment.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &comment).await?;
    require_capability!(caps, EDIT);
    let mut comment = comment.into_active_model();
    comment.body = Set(params.body);
    comment.update(&ctx.db).await?;
    Ok(Redirect::to(&format!("/projects/{project_pid}/notes/{note_pid}")).into_response())
}

/// `DELETE /projects/:project_pid/notes/:note_pid/comments/:pid` — delete.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn remove(
    Path((_project_pid, _note_pid, pid)): Path<(String, String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let comment = Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    // Only the author (or staff) may delete a comment.
    let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &comment).await?;
    require_capability!(caps, DELETE);
    comment.delete(&ctx.db).await?;
    format::empty()
}

pub fn routes() -> Routes {
    // The note segment must use the SAME capture name as the notes controller
    // (`{pid}`); a different name (`{note_pid}`) collides in the router and
    // breaks the whole /projects subtree. The comment id is `{comment_pid}`.
    Routes::new()
        .prefix("/projects/{project_pid}/notes/{pid}/comments")
        .add("/", post(add))
        .add("/{comment_pid}/edit", get(edit))
        .add("/{comment_pid}", post(update))
        .add("/{comment_pid}", delete(remove))
}
