use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, users};

/// Render the home page for an authenticated user.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    project_count: usize,
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["project_count"] = serde_json::json!(project_count);
    format::render().view(v, "home/index.html", data!(ctx))
}
