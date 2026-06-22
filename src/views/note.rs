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
    project: &projects::Model,
    item: &notes::Model,
    caps: &fracture_core::permissions::Capabilities,
) -> Result<Response> {
    use fracture_core::permissions::{DELETE, EDIT};
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["project"] = serde_json::json!(project);
    ctx["item"] = serde_json::json!(item);
    ctx["can_edit"] = serde_json::json!(caps.allows(EDIT));
    ctx["can_delete"] = serde_json::json!(caps.allows(DELETE));
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
