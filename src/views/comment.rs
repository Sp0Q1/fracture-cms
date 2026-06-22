use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{note_comments, notes, organizations, users};

/// Render the comment edit form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn edit(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    project_pid: &str,
    note: &notes::Model,
    comment: &note_comments::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["project_pid"] = serde_json::json!(project_pid);
    ctx["note"] = serde_json::json!({
        "pid": note.pid.to_string(),
        "title": note.title,
    });
    ctx["comment"] = serde_json::json!({
        "pid": comment.pid.to_string(),
        "body": comment.body,
    });
    format::render().view(v, "comment/edit.html", data!(ctx))
}
