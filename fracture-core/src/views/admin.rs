use loco_rs::prelude::*;
use serde_json::json;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, users};

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
    total_orgs: u64,
    total_users: u64,
    total_blog_posts: u64,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["total_orgs"] = json!(total_orgs);
    ctx["total_users"] = json!(total_users);
    ctx["total_blog_posts"] = json!(total_blog_posts);
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
