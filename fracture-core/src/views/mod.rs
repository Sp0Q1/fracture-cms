pub mod admin;
pub mod blog;
pub mod contact;
pub mod jobs;
pub mod org;
pub mod site;
pub mod sri;

use serde_json::{json, Value};

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, users};

/// Builds the common template context shared by all authenticated pages.
#[must_use]
pub fn base_context(
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
) -> Value {
    json!({
        "user_name": user.name,
        "user_pid": user.pid.to_string(),
        "org_name": org_ctx.map(|o| o.org.name.clone()),
        "org_pid": org_ctx.map(|o| o.org.pid.to_string()),
        "user_role": org_ctx.map(|o| o.role.to_string()),
        "is_platform_admin": org_ctx.is_some_and(|o| o.is_platform_admin),
        "user_orgs": user_orgs.iter().map(|o| json!({
            "name": o.name,
            "pid": o.pid.to_string(),
            "is_personal": o.is_personal,
        })).collect::<Vec<_>>(),
    })
}
