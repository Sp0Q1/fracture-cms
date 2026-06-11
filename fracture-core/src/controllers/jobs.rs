use std::collections::HashMap;
use std::str::FromStr;

use axum::response::Redirect;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::Form;
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::controllers::middleware;
use crate::jobs::try_job_registry;
use crate::models::_entities::{job_definitions, job_runs, organizations};
use crate::models::org_members::OrgRole;
use crate::models::{
    job_definitions as job_def_model, job_run_diffs as job_diff_model, job_runs as job_run_model,
    organizations as org_model,
};
use crate::views;
use crate::{require_platform_admin, require_role, require_user};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewJobParams {
    pub name: String,
    pub job_type: String,
    pub schedule: Option<String>,
    pub config: Option<String>,
}

/// Resolves a definition for read-only pages: org-scoped for members, with a
/// global fallback for platform admins (the /admin/jobs list links to
/// definitions in other orgs).
async fn find_definition_for_view(
    db: &sea_orm::DatabaseConnection,
    pid: &str,
    org_ctx: Option<&middleware::OrgContext>,
) -> Result<job_definitions::Model> {
    let org_id = org_ctx.map_or(0, |oc| oc.org.id);
    if let Some(definition) = job_def_model::Model::find_by_pid_and_org(db, pid, org_id).await? {
        return Ok(definition);
    }
    if org_ctx.is_some_and(|oc| oc.is_platform_admin) {
        if let Some(definition) = job_def_model::Model::find_by_pid(db, pid).await? {
            return Ok(definition);
        }
    }
    Err(Error::NotFound)
}

/// Loads the most recent run for each of the given definitions in one query.
async fn latest_runs_by_definition(
    db: &sea_orm::DatabaseConnection,
    definitions: &[job_definitions::Model],
) -> Result<HashMap<i32, job_runs::Model>> {
    if definitions.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<i32> = definitions.iter().map(|d| d.id).collect();
    let runs = job_runs::Entity::find()
        .filter(job_runs::Column::JobDefinitionId.is_in(ids))
        .order_by_desc(job_runs::Column::CreatedAt)
        .all(db)
        .await?;
    let mut latest = HashMap::new();
    for run in runs {
        latest.entry(run.job_definition_id).or_insert(run);
    }
    Ok(latest)
}

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
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let definitions = match org_ctx {
        Some(ref oc) => job_def_model::Model::find_all_by_org(&ctx.db, oc.org.id).await?,
        None => vec![],
    };
    let latest_runs = latest_runs_by_definition(&ctx.db, &definitions).await?;
    let job_types: Vec<String> = try_job_registry()
        .map(|r| r.job_types().iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    views::jobs::org_index(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &definitions,
        &latest_runs,
        &job_types,
    )
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
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let definition = find_definition_for_view(&ctx.db, &pid, org_ctx.as_ref()).await?;

    let runs = job_run_model::Model::find_by_definition(&ctx.db, definition.id).await?;
    views::jobs::org_show(&v, &user, org_ctx.as_ref(), &user_orgs, &definition, &runs)
}

/// POST /jobs — create a job definition (org admins only).
///
/// # Errors
///
/// Returns an error if validation fails, the database insert fails, or the
/// user is not authenticated.
#[debug_handler]
pub async fn org_create(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<NewJobParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Admin);

    let name = params.name.trim();
    if name.is_empty() {
        return Err(Error::BadRequest("job name must not be empty".to_string()));
    }
    // Only registered job types may be created — an unknown type would
    // produce runs that can only ever fail.
    let known = try_job_registry().is_some_and(|r| r.get(&params.job_type).is_some());
    if !known {
        return Err(Error::BadRequest(format!(
            "unknown job type '{}'",
            params.job_type
        )));
    }
    let schedule = params
        .schedule
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(s) = &schedule {
        if cron::Schedule::from_str(s).is_err() {
            return Err(Error::BadRequest(format!(
                "invalid cron schedule '{s}' (expected e.g. '0 0 * * * *' — sec min hour dom mon dow)"
            )));
        }
    }
    let config = params
        .config
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "{}".to_string());
    if serde_json::from_str::<serde_json::Value>(&config).is_err() {
        return Err(Error::BadRequest("config must be valid JSON".to_string()));
    }
    // Friendly error for the unique (org_id, name) index; the constraint
    // itself remains the race-safe backstop.
    let duplicate = job_definitions::Entity::find()
        .filter(job_definitions::Column::OrgId.eq(org_ctx.org.id))
        .filter(job_definitions::Column::Name.eq(name))
        .one(&ctx.db)
        .await?
        .is_some();
    if duplicate {
        return Err(Error::BadRequest(format!(
            "a job named '{name}' already exists in this organization"
        )));
    }

    job_definitions::ActiveModel {
        org_id: sea_orm::ActiveValue::Set(org_ctx.org.id),
        name: sea_orm::ActiveValue::Set(name.to_string()),
        job_type: sea_orm::ActiveValue::Set(params.job_type),
        schedule: sea_orm::ActiveValue::Set(schedule),
        enabled: sea_orm::ActiveValue::Set(true),
        config: sea_orm::ActiveValue::Set(config),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await?;

    Ok(Redirect::to("/jobs").into_response())
}

/// POST `/jobs/:pid/toggle` — enable/disable a job definition (org admins only).
///
/// # Errors
///
/// Returns an error if the definition is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_toggle(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Admin);

    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let enabled = definition.enabled;
    let mut active: job_definitions::ActiveModel = definition.into();
    active.enabled = sea_orm::ActiveValue::Set(!enabled);
    active.update(&ctx.db).await?;

    Ok(Redirect::to(&format!("/jobs/{pid}")).into_response())
}

/// POST `/jobs/:pid/run` — trigger a new queued run for a job definition
/// (org members and above).
///
/// # Errors
///
/// Returns an error if the definition is not found, is disabled, or the user
/// is not authenticated.
#[debug_handler]
pub async fn org_trigger(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    require_role!(org_ctx, OrgRole::Member);

    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    if !definition.enabled {
        return Err(Error::BadRequest(
            "this job is disabled; enable it before triggering a run".to_string(),
        ));
    }
    // A queued or running run already covers the next execution; triggering
    // again would only stack duplicates (double-click safety).
    if !job_run_model::Model::has_active_run(&ctx.db, definition.id).await? {
        job_run_model::Model::create_queued(&ctx.db, definition.id, org_ctx.org.id).await?;
    }

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
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let definition = find_definition_for_view(&ctx.db, &pid, org_ctx.as_ref()).await?;

    let run = job_run_model::Model::find_by_pid(&ctx.db, &run_pid)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Verify the run belongs to this definition
    if run.job_definition_id != definition.id {
        return Err(Error::NotFound);
    }

    let diffs = job_diff_model::Model::find_by_run(&ctx.db, run.id).await?;
    views::jobs::org_run_show(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &definition,
        &run,
        &diffs,
    )
}

/// GET /admin/jobs — list all job definitions across all orgs (platform admin only).
///
/// # Errors
///
/// Returns an error if the user is not a platform admin or a query fails.
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

    let definitions = job_definitions::Entity::find()
        .order_by_asc(job_definitions::Column::Name)
        .all(&ctx.db)
        .await?;
    let latest_runs = latest_runs_by_definition(&ctx.db, &definitions).await?;
    let orgs = organizations::Entity::find().all(&ctx.db).await?;

    views::jobs::admin_index(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &definitions,
        &latest_runs,
        &orgs,
    )
}

pub fn org_routes() -> Routes {
    Routes::new()
        .prefix("/jobs")
        .add("/", get(org_index))
        .add("/", post(org_create))
        .add("/{pid}", get(org_show))
        .add("/{pid}/toggle", post(org_toggle))
        .add("/{pid}/run", post(org_trigger))
        .add("/{pid}/runs/{run_pid}", get(org_run_show))
}

pub fn admin_routes() -> Routes {
    Routes::new()
        .prefix("/admin/jobs")
        .add("/", get(admin_index))
}
