use std::collections::HashMap;
use std::str::FromStr;

use axum::response::Redirect;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::Form;
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::controllers::middleware;
use crate::jobs::{try_job_registry, JobAccess, JobPermissions};
use crate::listing::{FieldKind, FormField};
use crate::models::_entities::{job_definitions, job_runs, organizations};
use crate::models::{
    job_definitions as job_def_model, job_run_diffs as job_diff_model, job_runs as job_run_model,
    organizations as org_model,
};
use crate::views;
use crate::{require_staff, require_user};

/// A 403 response for a denied job action — mirrors `require_role!`, but the
/// threshold is the configurable [`JobPermissions`] policy, not a fixed role.
fn forbidden() -> Response {
    axum::response::Response::builder()
        .status(axum::http::StatusCode::FORBIDDEN)
        .body(axum::body::Body::from("Forbidden"))
        .unwrap()
        .into_response()
}

/// Loads the policy and resolves it for the current org context. A missing
/// context (no org) clears nothing.
async fn job_access(
    db: &sea_orm::DatabaseConnection,
    org_ctx: Option<&middleware::OrgContext>,
) -> JobAccess {
    let perms = JobPermissions::load(db).await;
    org_ctx.map_or(
        JobAccess {
            can_view: false,
            can_run: false,
            can_manage: false,
        },
        |oc| perms.access(oc.role, oc.is_staff),
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditJobParams {
    pub name: String,
    pub schedule: Option<String>,
    pub config: Option<String>,
}

/// A validated set of editable job fields, or a user-facing error message.
struct ValidatedEdit {
    name: String,
    schedule: Option<String>,
    config: String,
}

/// Validates the common name/schedule/config fields shared by create and edit.
/// `exclude_id` skips the row being edited in the duplicate-name check.
async fn validate_job_fields(
    db: &sea_orm::DatabaseConnection,
    org_id: i32,
    name: &str,
    schedule: Option<String>,
    config: Option<String>,
    exclude_id: Option<i32>,
) -> std::result::Result<ValidatedEdit, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("job name must not be empty".to_string());
    }
    let schedule = schedule
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(s) = &schedule {
        if cron::Schedule::from_str(s).is_err() {
            return Err(format!(
                "invalid cron schedule '{s}' (expected e.g. '0 0 * * * *' — sec min hour dom mon dow)"
            ));
        }
    }
    let config = config
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "{}".to_string());
    if serde_json::from_str::<serde_json::Value>(&config).is_err() {
        return Err("config must be valid JSON".to_string());
    }
    // Friendly error for the unique (org_id, name) index; the constraint
    // itself remains the race-safe backstop.
    let mut dup_query = job_definitions::Entity::find()
        .filter(job_definitions::Column::OrgId.eq(org_id))
        .filter(job_definitions::Column::Name.eq(&name));
    if let Some(id) = exclude_id {
        dup_query = dup_query.filter(job_definitions::Column::Id.ne(id));
    }
    let duplicate = dup_query
        .one(db)
        .await
        .map_err(|_| "database error".to_string())?
        .is_some();
    if duplicate {
        return Err(format!(
            "a job named '{name}' already exists in this organization"
        ));
    }
    Ok(ValidatedEdit {
        name,
        schedule,
        config,
    })
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
    if org_ctx.is_some_and(|oc| oc.is_staff) {
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

    let access = job_access(&ctx.db, org_ctx.as_ref()).await;
    // A user with an org but below the configured view threshold is forbidden;
    // a user with no org at all just sees an empty list.
    if org_ctx.is_some() && !access.can_view {
        return Ok(forbidden());
    }

    let definitions = match org_ctx {
        Some(ref oc) => job_def_model::Model::find_all_by_org(&ctx.db, oc.org.id).await?,
        None => vec![],
    };
    let latest_runs = latest_runs_by_definition(&ctx.db, &definitions).await?;
    let job_types = try_job_registry()
        .map(crate::jobs::JobRegistry::job_type_infos)
        .unwrap_or_default();
    views::jobs::org_index(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &definitions,
        &latest_runs,
        &job_types,
        access,
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

    let access = job_access(&ctx.db, org_ctx.as_ref()).await;
    if !access.can_view {
        return Ok(forbidden());
    }
    let definition = find_definition_for_view(&ctx.db, &pid, org_ctx.as_ref()).await?;

    let runs = job_run_model::Model::find_by_definition(&ctx.db, definition.id).await?;
    views::jobs::org_show(
        &v,
        &user,
        org_ctx.as_ref(),
        &user_orgs,
        &definition,
        &runs,
        access,
    )
}

/// Builds a definition's `config` JSON from the submitted form body, driven by
/// the job type's declared [`config_form`](crate::jobs::JobExecutor::config_form).
/// Job types that declare no form fall back to a raw `config` JSON field, so
/// advanced or container-style jobs aren't constrained by a fixed UI.
fn build_config(
    fields: &[FormField],
    body: &HashMap<String, String>,
) -> std::result::Result<String, String> {
    if fields.is_empty() {
        let raw = body
            .get("config")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "{}".to_string());
        if serde_json::from_str::<serde_json::Value>(&raw).is_err() {
            return Err("config must be valid JSON".to_string());
        }
        return Ok(raw);
    }
    let mut obj = serde_json::Map::new();
    for f in fields {
        if f.kind == FieldKind::Checkbox {
            obj.insert(
                f.name.to_string(),
                serde_json::Value::Bool(body.contains_key(f.name)),
            );
            continue;
        }
        let val = body
            .get(f.name)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if f.required && val.is_empty() {
            return Err(format!("{} is required.", f.label));
        }
        obj.insert(f.name.to_string(), serde_json::Value::String(val));
    }
    Ok(serde_json::Value::Object(obj).to_string())
}

/// GET `/jobs/new/{job_type}` — friendly create form for one job type, with the
/// fields that job declares (org admins only).
///
/// # Errors
///
/// Returns an error if the user is not authorized or the job type is unknown.
#[debug_handler]
pub async fn org_new_form(
    Path(job_type): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_manage {
        return Ok(forbidden());
    }
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let registry = try_job_registry().ok_or_else(|| Error::NotFound)?;
    let executor = registry.get(&job_type).ok_or_else(|| Error::NotFound)?;
    let fields = executor
        .config_form(&ctx.db, org_ctx.org.id)
        .await
        .unwrap_or_default();

    views::jobs::org_new(
        &v,
        &user,
        Some(&org_ctx),
        &user_orgs,
        &job_type,
        executor.label(),
        &fields,
        None,
    )
}

/// POST /jobs — create a job definition from the friendly form (org admins only).
///
/// # Errors
///
/// Returns an error if validation fails, the database insert fails, or the
/// user is not authenticated.
#[debug_handler]
#[allow(clippy::implicit_hasher)]
pub async fn org_create(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(body): Form<HashMap<String, String>>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_manage {
        return Ok(forbidden());
    }

    let job_type = body.get("job_type").cloned().unwrap_or_default();
    // Only registered job types may be created — an unknown type would produce
    // runs that can only ever fail.
    let registry = try_job_registry().ok_or_else(|| Error::NotFound)?;
    let executor = registry
        .get(&job_type)
        .ok_or_else(|| Error::BadRequest(format!("unknown job type '{job_type}'")))?;

    let fields = executor
        .config_form(&ctx.db, org_ctx.org.id)
        .await
        .unwrap_or_default();
    let name = body.get("name").cloned().unwrap_or_default();
    let schedule = body.get("schedule").cloned();
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    // Re-render the friendly form with the submitted values + a message on any
    // validation failure, so org owners never see a raw error page.
    let rerender = |error: String, fields: Vec<FormField>| {
        views::jobs::org_new(
            &v,
            &user,
            Some(&org_ctx),
            &user_orgs,
            &job_type,
            executor.label(),
            &fields,
            Some(&error),
        )
    };

    let config = match build_config(&fields, &body) {
        Ok(c) => c,
        Err(e) => return rerender(e, prefill(&fields, &body)),
    };
    let valid =
        match validate_job_fields(&ctx.db, org_ctx.org.id, &name, schedule, Some(config), None)
            .await
        {
            Ok(v) => v,
            Err(e) => return rerender(e, prefill(&fields, &body)),
        };

    job_definitions::ActiveModel {
        org_id: sea_orm::ActiveValue::Set(org_ctx.org.id),
        name: sea_orm::ActiveValue::Set(valid.name),
        job_type: sea_orm::ActiveValue::Set(job_type.clone()),
        schedule: sea_orm::ActiveValue::Set(valid.schedule),
        enabled: sea_orm::ActiveValue::Set(true),
        config: sea_orm::ActiveValue::Set(valid.config),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await?;

    Ok(Redirect::to("/jobs").into_response())
}

/// Re-fills config-form fields with the submitted values so a rejected create
/// keeps what the user typed/selected.
fn prefill(fields: &[FormField], body: &HashMap<String, String>) -> Vec<FormField> {
    fields
        .iter()
        .map(|f| {
            let mut f = f.clone();
            f.value = body.get(f.name).cloned().unwrap_or_default();
            f
        })
        .collect()
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
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_manage {
        return Ok(forbidden());
    }

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
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_run {
        return Ok(forbidden());
    }

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

/// GET `/jobs/:pid/edit` — edit form for a job definition (org admins only).
///
/// # Errors
///
/// Returns an error if the definition is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_edit(
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
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_manage {
        return Ok(forbidden());
    }
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    views::jobs::org_edit(&v, &user, Some(&org_ctx), &user_orgs, &definition, None)
}

/// POST `/jobs/:pid/edit` — apply an edit to a job definition (org admins only).
///
/// # Errors
///
/// Returns an error if the definition is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_update(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<EditJobParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_manage {
        return Ok(forbidden());
    }

    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    match validate_job_fields(
        &ctx.db,
        org_ctx.org.id,
        &params.name,
        params.schedule,
        params.config,
        Some(definition.id),
    )
    .await
    {
        Ok(valid) => {
            let mut active: job_definitions::ActiveModel = definition.into();
            active.name = sea_orm::ActiveValue::Set(valid.name);
            active.schedule = sea_orm::ActiveValue::Set(valid.schedule);
            active.config = sea_orm::ActiveValue::Set(valid.config);
            active.update(&ctx.db).await?;
            Ok(Redirect::to(&format!("/jobs/{pid}")).into_response())
        }
        Err(msg) => {
            // Re-render the form with the error (the submitted definition still
            // carries the old persisted values for any untouched fields).
            let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
                .await
                .unwrap_or_default();
            views::jobs::org_edit(
                &v,
                &user,
                Some(&org_ctx),
                &user_orgs,
                &definition,
                Some(&msg),
            )
        }
    }
}

/// POST `/jobs/:pid/delete` — delete a job definition and its run history
/// (org admins only). Runs and diffs cascade via their foreign keys.
///
/// # Errors
///
/// Returns an error if the definition is not found or the user is not authenticated.
#[debug_handler]
pub async fn org_delete(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;
    if !job_access(&ctx.db, Some(&org_ctx)).await.can_manage {
        return Ok(forbidden());
    }

    let definition = job_def_model::Model::find_by_pid_and_org(&ctx.db, &pid, org_ctx.org.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    job_definitions::Entity::delete_by_id(definition.id)
        .exec(&ctx.db)
        .await?;

    Ok(Redirect::to("/jobs").into_response())
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

    let access = job_access(&ctx.db, org_ctx.as_ref()).await;
    if !access.can_view {
        return Ok(forbidden());
    }
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
        access,
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
    require_staff!(org_ctx);
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
        .add("/new/{job_type}", get(org_new_form))
        .add("/{pid}", get(org_show))
        .add("/{pid}/edit", get(org_edit).post(org_update))
        .add("/{pid}/delete", post(org_delete))
        .add("/{pid}/toggle", post(org_toggle))
        .add("/{pid}/run", post(org_trigger))
        .add("/{pid}/runs/{run_pid}", get(org_run_show))
}

pub fn admin_routes() -> Routes {
    Routes::new()
        .prefix("/admin/jobs")
        .add("/", get(admin_index))
}
