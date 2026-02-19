use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{notes, organizations, projects, users};

pub fn create(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    project: &projects::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, &Some(org_ctx.clone()), user_orgs);
    ctx["project"] = serde_json::json!(project);
    format::render().view(v, "note/create.html", data!(ctx))
}

pub fn show(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    project: &projects::Model,
    item: &notes::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, &Some(org_ctx.clone()), user_orgs);
    ctx["project"] = serde_json::json!(project);
    ctx["item"] = serde_json::json!(item);
    format::render().view(v, "note/show.html", data!(ctx))
}

pub fn edit(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    project: &projects::Model,
    item: &notes::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, &Some(org_ctx.clone()), user_orgs);
    ctx["project"] = serde_json::json!(project);
    ctx["item"] = serde_json::json!(item);
    format::render().view(v, "note/edit.html", data!(ctx))
}
