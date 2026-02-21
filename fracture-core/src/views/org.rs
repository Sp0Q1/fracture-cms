use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{org_invites, org_members, organizations, users};

pub fn list(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
) -> Result<Response> {
    let ctx = super::base_context(user, org_ctx, user_orgs);
    format::render().view(v, "org/list.html", data!(ctx))
}

pub fn new(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &Option<OrgContext>,
    user_orgs: &[organizations::Model],
) -> Result<Response> {
    let ctx = super::base_context(user, org_ctx, user_orgs);
    format::render().view(v, "org/new.html", data!(ctx))
}

pub fn settings(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    org: &organizations::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, &Some(org_ctx.clone()), user_orgs);
    ctx["org"] = serde_json::json!({
        "name": org.name,
        "pid": org.pid.to_string(),
        "slug": org.slug,
        "is_personal": org.is_personal,
    });
    format::render().view(v, "org/settings.html", data!(ctx))
}

pub struct MembersViewData<'a> {
    pub org: &'a organizations::Model,
    pub member_users: &'a [(org_members::Model, users::Model)],
    pub pending_invites: &'a [org_invites::Model],
    pub app_url: &'a str,
}

pub fn members(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    data: &MembersViewData<'_>,
) -> Result<Response> {
    let mut ctx = super::base_context(user, &Some(org_ctx.clone()), user_orgs);
    ctx["org"] = serde_json::json!({
        "name": data.org.name,
        "pid": data.org.pid.to_string(),
        "slug": data.org.slug,
        "is_personal": data.org.is_personal,
    });
    ctx["members"] = serde_json::json!(data
        .member_users
        .iter()
        .map(|(m, u)| {
            serde_json::json!({
                "user_name": u.name,
                "user_email": u.email,
                "user_pid": u.pid.to_string(),
                "role": m.role,
            })
        })
        .collect::<Vec<_>>());
    ctx["pending_invites"] = serde_json::json!(data
        .pending_invites
        .iter()
        .map(|i| {
            serde_json::json!({
                "email": i.email,
                "role": i.role,
                "pid": i.pid.to_string(),
                "accept_url": format!("{}/invites/{}/accept", data.app_url, i.pid),
            })
        })
        .collect::<Vec<_>>());
    format::render().view(v, "org/members.html", data!(ctx))
}
