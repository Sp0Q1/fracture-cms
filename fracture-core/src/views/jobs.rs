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

/// Renders the org job definitions list.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn org_index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    definitions: &[job_definitions::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definitions"] = json!(definitions.iter().map(definition_json).collect::<Vec<_>>());
    format::render().view(v, "jobs/org_index.html", data!(ctx))
}

/// Renders a single job definition with its runs.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn org_show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    definition: &job_definitions::Model,
    runs: &[job_runs::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definition"] = definition_json(definition);
    ctx["runs"] = json!(runs.iter().map(run_json).collect::<Vec<_>>());
    format::render().view(v, "jobs/org_show.html", data!(ctx))
}

/// Renders a single job run with its diffs.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn org_run_show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    definition: &job_definitions::Model,
    run: &job_runs::Model,
    diffs: &[job_run_diffs::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definition"] = definition_json(definition);
    ctx["run"] = run_json(run);
    ctx["diffs"] = json!(diffs.iter().map(diff_json).collect::<Vec<_>>());
    format::render().view(v, "jobs/org_run_show.html", data!(ctx))
}

/// Renders the admin cross-org job definitions list.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn admin_index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    definitions: &[job_definitions::Model],
    orgs: &[organizations::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["definitions"] = json!(definitions
        .iter()
        .map(|def| {
            let mut d = definition_json(def);
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
