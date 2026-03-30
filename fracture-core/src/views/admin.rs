use loco_rs::prelude::*;
use serde_json::json;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, users};

/// Stats for the admin dashboard.
pub struct DashboardStats {
    pub total_orgs: u64,
    pub total_users: u64,
    pub total_blog_posts: u64,
    pub total_jobs: u64,
    pub total_job_runs: u64,
}

/// Renders the platform admin dashboard.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn dashboard(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
    stats: &DashboardStats,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["total_orgs"] = json!(stats.total_orgs);
    ctx["total_users"] = json!(stats.total_users);
    ctx["total_blog_posts"] = json!(stats.total_blog_posts);
    ctx["total_jobs"] = json!(stats.total_jobs);
    ctx["total_job_runs"] = json!(stats.total_job_runs);
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
    org_ctx: &Option<OrgContext>,
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
