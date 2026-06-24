use std::collections::HashMap;

use loco_rs::prelude::*;
use serde_json::json;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{job_definitions, job_run_diffs, job_runs, organizations, users};

/// Helper to serialize a job definition into JSON for templates.
fn definition_json(def: &job_definitions::Model) -> serde_json::Value {
    json!({
        "id": def.id,
        "pid": def.pid.to_string(),
        "name": def.name,
        "job_type": def.job_type,
        "schedule": def.schedule,
        "enabled": def.enabled,
        "config": def.config,
        "created_at": def.created_at.to_string(),
        "updated_at": def.updated_at.to_string(),
    })
}

/// Serializes a definition together with its most recent run (if any), so
/// list pages can show what each job is currently doing.
fn definition_with_latest_json(
    def: &job_definitions::Model,
    latest_runs: &HashMap<i32, job_runs::Model>,
) -> serde_json::Value {
    let mut d = definition_json(def);
    d["latest_run"] = latest_runs.get(&def.id).map_or(json!(null), run_json);
    d
}

/// Helper to serialize a job run into JSON for templates.
fn run_json(run: &job_runs::Model) -> serde_json::Value {
    json!({
        "id": run.id,
        "pid": run.pid.to_string(),
        "status": run.status,
        "started_at": run.started_at.map(|d| d.to_string()),
        "completed_at": run.completed_at.map(|d| d.to_string()),
        "error_message": run.error_message,
        "result_summary": run.result_summary,
        "created_at": run.created_at.to_string(),
        "updated_at": run.updated_at.to_string(),
    })
}

/// Helper to serialize a job run diff into JSON for templates.
fn diff_json(diff: &job_run_diffs::Model) -> serde_json::Value {
    json!({
        "id": diff.id,
        "diff_type": diff.diff_type,
        "entity_key": diff.entity_key,
        "old_value": diff.old_value,
        "new_value": diff.new_value,
        "created_at": diff.created_at.to_string(),
    })
}

/// Capability flags for the jobs pages, taken from the resolved
/// [`JobAccess`] (the configurable policy), so templates never re-encode the
/// role hierarchy in string comparisons.
fn add_capabilities(ctx: &mut serde_json::Value, access: crate::jobs::JobAccess) {
    ctx["can_trigger_jobs"] = json!(access.can_run);
    ctx["can_manage_jobs"] = json!(access.can_manage);
}

/// Renders the org job definitions list.
///
/// # Errors
///
/// Returns an error if template rendering fails.
#[allow(clippy::implicit_hasher)] // Reason: view-layer map, never hashed generically
#[allow(clippy::too_many_arguments)] // View threads display context + capabilities.
pub fn org_index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    definitions: &[job_definitions::Model],
    latest_runs: &HashMap<i32, job_runs::Model>,
    job_types: &[crate::jobs::JobTypeInfo],
    access: crate::jobs::JobAccess,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definitions"] = json!(definitions
        .iter()
        .map(|d| definition_with_latest_json(d, latest_runs))
        .collect::<Vec<_>>());
    ctx["job_types"] = json!(job_types);
    add_capabilities(&mut ctx, access);
    format::render().view(v, "jobs/org_index.html", data!(ctx))
}

/// Renders the friendly create form for one job type.
///
/// # Errors
///
/// Returns an error if template rendering fails.
#[allow(clippy::too_many_arguments)] // View threads display context + the job's fields.
pub fn org_new(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    job_type: &str,
    label: &str,
    fields: &[crate::listing::FormField],
    error: Option<&str>,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["job_type"] = json!(job_type);
    ctx["job_label"] = json!(label);
    ctx["fields"] = json!(fields);
    ctx["error"] = json!(error);
    format::render().view(v, "jobs/org_new.html", data!(ctx))
}

/// Renders a single job definition with its runs.
///
/// # Errors
///
/// Returns an error if template rendering fails.
#[allow(clippy::too_many_arguments)] // View threads display context + capabilities.
pub fn org_show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    definition: &job_definitions::Model,
    runs: &[job_runs::Model],
    access: crate::jobs::JobAccess,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definition"] = definition_json(definition);
    ctx["runs"] = json!(runs.iter().map(run_json).collect::<Vec<_>>());
    // Drives the page's auto-refresh and the trigger button state.
    ctx["has_active_run"] = json!(runs
        .iter()
        .any(|r| r.status == "queued" || r.status == "running"));
    add_capabilities(&mut ctx, access);
    format::render().view(v, "jobs/org_show.html", data!(ctx))
}

/// Renders the edit form for a job definition.
///
/// `error` is a user-visible validation message shown above the form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn org_edit(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    definition: &job_definitions::Model,
    error: Option<&str>,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definition"] = definition_json(definition);
    ctx["error"] = json!(error);
    format::render().view(v, "jobs/org_edit.html", data!(ctx))
}

/// Renders a single job run with its diffs.
///
/// # Errors
///
/// Returns an error if template rendering fails.
#[allow(clippy::too_many_arguments)] // View threads display context + capabilities.
pub fn org_run_show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    definition: &job_definitions::Model,
    run: &job_runs::Model,
    diffs: &[job_run_diffs::Model],
    access: crate::jobs::JobAccess,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definition"] = definition_json(definition);
    ctx["run"] = run_json(run);
    ctx["run_active"] = json!(run.status == "queued" || run.status == "running");
    ctx["diffs"] = json!(diffs.iter().map(diff_json).collect::<Vec<_>>());
    add_capabilities(&mut ctx, access);
    format::render().view(v, "jobs/org_run_show.html", data!(ctx))
}

/// Renders the admin cross-org job definitions list.
///
/// # Errors
///
/// Returns an error if template rendering fails.
#[allow(clippy::implicit_hasher)] // Reason: view-layer map, never hashed generically
pub fn admin_index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    definitions: &[job_definitions::Model],
    latest_runs: &HashMap<i32, job_runs::Model>,
    orgs: &[organizations::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definitions"] = json!(definitions
        .iter()
        .map(|def| {
            let mut d = definition_with_latest_json(def, latest_runs);
            let org_name = orgs
                .iter()
                .find(|o| o.id == def.org_id)
                .map_or("Unknown", |o| o.name.as_str());
            d["org_name"] = json!(org_name);
            d
        })
        .collect::<Vec<_>>());
    format::render().view(v, "jobs/admin_index.html", data!(ctx))
}
