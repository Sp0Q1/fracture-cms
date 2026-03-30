use axum::response::Redirect;
use axum_extra::extract::cookie::CookieJar;
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, QueryOrder};

use crate::controllers::middleware;
use crate::models::_entities::{job_definitions, organizations};
use crate::models::{
    job_definitions as job_def_model, job_run_diffs as job_diff_model, job_runs as job_run_model,
    organizations as org_model,
};
use crate::views;
use crate::{require_platform_admin, require_user};

/// GET /jobs — list job definitions for the current org.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn org_index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;

    let definitions = match org_ctx {
        Some(ref oc) => job_def_model::Model::find_all_by_org(&ctx.db, oc.org.id).await,
        None => vec![],
    };
    views::jobs::org_index(&v, &user, &org_ctx, &user_orgs, &definitions)
}

/// GET `/jobs/:pid` — show a job definition and its runs.
///
/// # Errors
///
/// Returns an error if the definition is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_show(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;

    let org_id = org_ctx.as_ref().map_or(0, |oc| oc.org.id);
    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_id)
        .await
        .ok_or_else(|| Error::NotFound)?;

    let runs = job_run_model::Model::find_by_definition(&ctx.db, definition.id).await;
    views::jobs::org_show(&v, &user, &org_ctx, &user_orgs, &definition, &runs)
}

/// POST `/jobs/:pid/run` — trigger a new queued run for a job definition.
///
/// # Errors
///
/// Returns an error if the definition is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_trigger(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;

    let org_id = org_ctx.as_ref().map_or(0, |oc| oc.org.id);
    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_id)
        .await
        .ok_or_else(|| Error::NotFound)?;

    job_run_model::Model::create_queued(&ctx.db, definition.id, org_id).await?;

    Ok(Redirect::to(&format!("/jobs/{pid}")).into_response())
}

/// GET `/jobs/:pid/runs/:run_pid` — show a specific run and its diffs.
///
/// # Errors
///
/// Returns an error if the run is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_run_show(
    Path((pid, run_pid)): Path<(String, String)>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;

    let org_id = org_ctx.as_ref().map_or(0, |oc| oc.org.id);
    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_id)
        .await
        .ok_or_else(|| Error::NotFound)?;

    let run = job_run_model::Model::find_by_pid(&ctx.db, &run_pid)
        .await
        .ok_or_else(|| Error::NotFound)?;

    // Verify the run belongs to this definition
    if run.job_definition_id != definition.id {
        return Err(Error::NotFound);
    }

    let diffs = job_diff_model::Model::find_by_run(&ctx.db, run.id).await;
    views::jobs::org_run_show(&v, &user, &org_ctx, &user_orgs, &definition, &run, &diffs)
}

/// GET /admin/jobs — list all job definitions across all orgs (platform admin only).
///
/// # Errors
///
/// Returns an error if the user is not a platform admin.
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
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;

    let definitions = job_definitions::Entity::find()
        .order_by_asc(job_definitions::Column::Name)
        .all(&ctx.db)
        .await
        .unwrap_or_default();

    let orgs = organizations::Entity::find()
        .all(&ctx.db)
        .await
        .unwrap_or_default();

    views::jobs::admin_index(&v, &user, &org_ctx, &user_orgs, &definitions, &orgs)
}

pub fn org_routes() -> Routes {
    Routes::new()
        .prefix("/jobs")
        .add("/", get(org_index))
        .add("/{pid}", get(org_show))
        .add("/{pid}/run", post(org_trigger))
        .add("/{pid}/runs/{run_pid}", get(org_run_show))
}

pub fn admin_routes() -> Routes {
    Routes::new()
        .prefix("/admin/jobs")
        .add("/", get(admin_index))
}
