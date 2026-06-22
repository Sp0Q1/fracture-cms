//! Views for the public contact form and the admin inbox.

use loco_rs::prelude::*;
use serde_json::json;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{contact_messages, organizations, users};

/// Renders the public contact form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn form(
    v: &impl ViewRenderer,
    nav: Option<&serde_json::Value>,
    sent: bool,
) -> Result<Response> {
    let ctx = super::with_nav(json!({ "sent": sent }), nav);
    format::render().view(v, "site/contact.html", data!(ctx))
}

/// Renders the admin contact inbox.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn admin_index(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
    messages: &[contact_messages::Model],
) -> Result<Response> {
    let mut ctx = super::base_context(user, org_ctx, user_orgs);
    ctx["messages"] = json!(messages
        .iter()
        .map(|m| json!({
            "pid": m.pid.to_string(),
            "name": m.name,
            "email": m.email,
            "message": m.message,
            "created_at": m.created_at.to_string(),
        }))
        .collect::<Vec<_>>());
    format::render().view(v, "admin/contact.html", data!(ctx))
}
