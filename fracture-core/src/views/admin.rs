use loco_rs::prelude::*;
use serde::Serialize;
use serde_json::json;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, users};

/// A single entity stat for the admin dashboard.
#[derive(Serialize)]
pub struct EntityStat {
    pub name: String,
    pub count: u64,
    pub url: String,
    pub description: String,
    pub action_label: String,
}

/// Renders the platform admin dashboard.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn dashboard(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    stats: &[EntityStat],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["entity_stats"] = json!(stats);
    format::render().view(v, "admin/dashboard.html", data!(ctx))
}

/// Renders the admin organizations list.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn orgs(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    all_orgs: &[organizations::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["all_orgs"] = json!(all_orgs
        .iter()
        .map(|o| json!({
            "pid": o.pid.to_string(),
            "name": o.name,
            "slug": o.slug,
            "is_personal": o.is_personal,
            "is_platform_admin": o.is_platform_admin,
        }))
        .collect::<Vec<_>>());
    format::render().view(v, "admin/orgs.html", data!(ctx))
}
