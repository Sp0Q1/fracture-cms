#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
pub mod org;

use serde_json::{json, Value};

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, users};

/// Builds the common template context shared by all authenticated pages.
#[must_use]
pub fn base_context(
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
) -> Value {
    json!({
        "user_name": user.name,
        "user_pid": user.pid.to_string(),
        "org_name": org_ctx.as_ref().map(|o| o.org.name.clone()),
        "org_pid": org_ctx.as_ref().map(|o| o.org.pid.to_string()),
        "user_role": org_ctx.as_ref().map(|o| o.role.to_string()),
        "user_orgs": user_orgs.iter().map(|o| json!({
            "name": o.name,
            "pid": o.pid.to_string(),
            "is_personal": o.is_personal,
        })).collect::<Vec<_>>(),
    })
}
