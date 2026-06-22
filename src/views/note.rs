use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{notes, organizations, projects, users};

/// Render the new-note form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn create(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    project: &projects::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["project"] = serde_json::json!(project);
    format::render().view(v, "note/create.html", data!(ctx))
}

/// Bundled inputs for the note detail page: the note, the viewer's note
/// capabilities, and the comment timeline (each pre-rendered with its own
/// `can_edit`/`can_delete`).
pub struct ShowData<'a> {
    pub project: &'a projects::Model,
    pub note: &'a notes::Model,
    pub caps: &'a fracture_core::permissions::Capabilities,
    pub can_comment: bool,
    pub comments: Vec<serde_json::Value>,
}

/// Render the note detail page.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    data: &ShowData<'_>,
) -> Result<Response> {
    use fracture_core::permissions::{DELETE, EDIT};
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["project"] = serde_json::json!(data.project);
    ctx["item"] = serde_json::json!(data.note);
    ctx["can_edit"] = serde_json::json!(data.caps.allows(EDIT));
    ctx["can_delete"] = serde_json::json!(data.caps.allows(DELETE));
    ctx["can_comment"] = serde_json::json!(data.can_comment);
    ctx["comments"] = serde_json::json!(data.comments);
    format::render().view(v, "note/show.html", data!(ctx))
}

/// Render the edit-note form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn edit(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    project: &projects::Model,
    item: &notes::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["project"] = serde_json::json!(project);
    ctx["item"] = serde_json::json!(item);
    format::render().view(v, "note/edit.html", data!(ctx))
}
