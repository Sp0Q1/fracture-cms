use loco_rs::prelude::*;

use crate::controllers::middleware::OrgContext;
use crate::models::_entities::{org_invites, org_members, organizations, users};

/// One row of the organization list: the org plus the viewer's role in it and
/// whether they may manage it (Admin+ in that org, or platform admin).
pub struct OrgListItem<'a> {
    pub org: &'a organizations::Model,
    pub role: Option<String>,
    pub can_manage: bool,
}

/// Renders the organization list page.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn list(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    items: &[OrgListItem<'_>],
) -> Result<Response> {
    let orgs: Vec<organizations::Model> = items.iter().map(|i| i.org.clone()).collect();
    let mut ctx = super::base_context(user, org_ctx, &orgs);
    // Replace the nav's bare org list with entries enriched for the table:
    // the viewer's role and whether they can manage each org.
    ctx["user_orgs"] = serde_json::json!(items
        .iter()
        .map(|i| serde_json::json!({
            "name": i.org.name,
            "pid": i.org.pid.to_string(),
            "is_personal": i.org.is_personal,
            "role": i.role,
            "can_manage": i.can_manage,
        }))
        .collect::<Vec<_>>());
    format::render().view(v, "org/list.html", data!(ctx))
}

/// Renders the new organization form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn new(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: Option<&OrgContext>,
    user_orgs: &[organizations::Model],
) -> Result<Response> {
    let ctx = super::base_context(user, org_ctx, user_orgs);
    format::render().view(v, "org/new.html", data!(ctx))
}

/// Renders the organization settings page.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn settings(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    org: &organizations::Model,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
    ctx["org"] = serde_json::json!({
        "name": org.name,
        "pid": org.pid.to_string(),
        "slug": org.slug,
        "is_personal": org.is_personal,
        "is_staff_org": org.is_staff,
    });
    format::render().view(v, "org/settings.html", data!(ctx))
}

pub struct MembersViewData<'a> {
    pub org: &'a organizations::Model,
    pub member_users: &'a [(org_members::Model, users::Model)],
    pub pending_invites: &'a [org_invites::Model],
    pub app_url: &'a str,
}

/// Renders the organization members page.
///
/// # Errors
///
/// Returns an error if template rendering fails.
pub fn members(
    v: &impl ViewRenderer,
    user: &users::Model,
    org_ctx: &OrgContext,
    user_orgs: &[organizations::Model],
    data: &MembersViewData<'_>,
) -> Result<Response> {
    let mut ctx = super::base_context(user, Some(org_ctx), user_orgs);
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
