use fracture_core::permissions::{Capabilities, COMMENT, DELETE, EDIT};
use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{organizations, projects, users};

/// Render the project list page.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn list(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    page: &fracture_core::listing::ListPage,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["page"] = serde_json::json!(page);
    format::render().view(v, "project/list.html", data!(ctx))
}

/// Render the project detail page.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    item: &projects::Model,
    page: &fracture_core::listing::ListPage,
    caps: &Capabilities,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["item"] = serde_json::json!(item);
    ctx["page"] = serde_json::json!(page);
    // Capability flags drive which actions the template offers — the same
    // resolver result the controller gates on (see src/authz.rs).
    ctx["can_edit"] = serde_json::json!(caps.allows(EDIT));
    ctx["can_delete"] = serde_json::json!(caps.allows(DELETE));
    ctx["can_comment"] = serde_json::json!(caps.allows(COMMENT));
    format::render().view(v, "project/show.html", data!(ctx))
}

/// Render the new-project form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn create(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
) -> Result<Response> {
    let ctx = super::base_context(user, Some(org_ctx), user_orgs);
    format::render().view(v, "project/create.html", data!(ctx))
}

/// Render the edit-project form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn edit(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    item: &projects::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["item"] = serde_json::json!(item);
    format::render().view(v, "project/edit.html", data!(ctx))
}
