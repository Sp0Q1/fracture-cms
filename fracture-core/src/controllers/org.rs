use axum::response::Redirect;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::controllers::middleware;
use crate::mailers::invite::InviteMailer;
use crate::models::_entities::{org_members, organizations, users as users_entity};
use crate::models::org_members::OrgRole;
use crate::models::{org_invites, organizations as org_model};
use crate::views;
use crate::{require_role, require_user};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewOrgParams {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrgSettingsParams {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InviteParams {
    pub email: String,
    pub role: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoleParams {
    pub role: String,
}

/// GET /orgs/ — list user's organizations
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn list(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id).await;
    views::org::list(&v, &user, &org_ctx, &user_orgs)
}

/// GET /orgs/new — new org form
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn new(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id).await;
    views::org::new(&v, &user, &org_ctx, &user_orgs)
}

/// POST /orgs/ — create organization
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn create(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<NewOrgParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);

    let base_slug = slug::slugify(&params.name);
    let mut slug = base_slug.clone();
    let mut suffix = 1u32;
    while organizations::Entity::find()
        .filter(organizations::Column::Slug.eq(&slug))
        .one(&ctx.db)
        .await?
        .is_some()
    {
        suffix += 1;
        slug = format!("{base_slug}-{suffix}");
    }
    let org = organizations::ActiveModel {
        name: sea_orm::ActiveValue::Set(params.name),
        slug: sea_orm::ActiveValue::Set(slug),
        is_personal: sea_orm::ActiveValue::Set(false),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await?;

    org_members::Model::add_member(&ctx.db, org.id, user.id, OrgRole::Owner).await?;

    Ok(Redirect::to(&format!("/orgs/switch/{}", org.pid)).into_response())
}

/// GET /orgs/:pid/settings — org settings page
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn settings(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id).await;
    views::org::settings(&v, &user, &org_ctx, &user_orgs, &org)
}

/// POST /orgs/:pid/settings — update org settings
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn update_settings(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<OrgSettingsParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let mut active: organizations::ActiveModel = org.into();
    active.name = sea_orm::ActiveValue::Set(params.name);
    active.update(&ctx.db).await?;

    Ok(Redirect::to(&format!("/orgs/{pid}/settings")).into_response())
}

/// GET /orgs/:pid/members — members list
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn members(
    Path(pid): Path<String>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Viewer);

    let members_list = org_members::Model::find_members(&ctx.db, org.id).await;
    let mut member_users: Vec<(org_members::Model, users_entity::Model)> = Vec::new();
    for m in members_list {
        if let Some(u) = users_entity::Entity::find_by_id(m.user_id)
            .one(&ctx.db)
            .await
            .ok()
            .flatten()
        {
            member_users.push((m, u));
        }
    }
    let pending_invites = org_invites::Model::find_pending_by_org(&ctx.db, org.id).await;
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id).await;
    let app_url = ctx.config.server.host.clone();
    views::org::members(
        &v,
        &user,
        &org_ctx,
        &user_orgs,
        &views::org::MembersViewData {
            org: &org,
            member_users: &member_users,
            pending_invites: &pending_invites,
            app_url: &app_url,
        },
    )
}

/// POST /orgs/:pid/members/invite — invite a member
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn invite(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<InviteParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let invite_role = OrgRole::from_str_role(&params.role).unwrap_or(OrgRole::Member);
    let invite =
        org_invites::Model::create_invite(&ctx.db, org.id, &params.email, invite_role, user.id)
            .await?;

    let host = ctx.config.server.host.clone();
    let accept_url = format!("{host}/invites/{}/accept", invite.pid);
    let _ = InviteMailer::send_invite(
        &ctx,
        &params.email,
        &org.name,
        &user.name,
        &params.role,
        &accept_url,
    )
    .await;

    Ok(Redirect::to(&format!("/orgs/{pid}/members")).into_response())
}

/// POST `/orgs/:pid/members/:user_pid/role` — update member role
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
///
/// # Panics
///
/// Panics if the HTTP response builder fails, which should not occur with valid status codes.
#[debug_handler]
pub async fn update_role(
    Path((pid, user_pid)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<RoleParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let target_user = users_entity::Model::find_by_pid(&ctx.db, &user_pid)
        .await
        .map_err(|_| Error::NotFound)?;
    let target_membership = org_members::Model::find_membership(&ctx.db, org.id, target_user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let new_role = OrgRole::from_str_role(&params.role).unwrap_or(OrgRole::Member);
    let target_current_role =
        OrgRole::from_str_role(&target_membership.role).unwrap_or(OrgRole::Viewer);

    // Only Owners can grant/revoke the Owner role or modify other Owners
    if (new_role == OrgRole::Owner || target_current_role == OrgRole::Owner)
        && !org_ctx.role.at_least(OrgRole::Owner)
    {
        return Ok(axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(axum::body::Body::from("Forbidden"))
            .unwrap()
            .into_response());
    }

    org_members::Model::update_role(&ctx.db, target_membership, new_role).await?;

    Ok(Redirect::to(&format!("/orgs/{pid}/members")).into_response())
}

/// POST `/orgs/:pid/members/:user_pid/remove` — remove member
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
#[debug_handler]
pub async fn remove_member(
    Path((pid, user_pid)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let target_user = users_entity::Model::find_by_pid(&ctx.db, &user_pid)
        .await
        .map_err(|_| Error::NotFound)?;
    let target_membership = org_members::Model::find_membership(&ctx.db, org.id, target_user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    org_members::Model::remove_member(&ctx.db, target_membership)
        .await
        .map_err(|e| Error::Message(e.to_string()))?;

    Ok(Redirect::to(&format!("/orgs/{pid}/members")).into_response())
}

/// POST /orgs/:pid/delete — delete organization (platform admin + org owner only)
///
/// Cannot delete platform admin orgs or personal orgs.
/// Deletes all org members and invites, then the org itself.
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
///
/// # Panics
///
/// Panics if the HTTP response builder fails, which should not occur with valid status codes.
#[debug_handler]
pub async fn delete(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;

    // Require both Owner role on the org AND platform admin status
    require_role!(org_ctx, OrgRole::Owner);
    if !org_ctx.is_platform_admin {
        return Ok(axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(axum::body::Body::from("Forbidden"))
            .unwrap()
            .into_response());
    }

    // Cannot delete platform admin orgs
    if org.is_platform_admin {
        return Ok(axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(axum::body::Body::from(
                "Cannot delete the platform admin organization",
            ))
            .unwrap()
            .into_response());
    }

    // Cannot delete personal orgs
    if org.is_personal {
        return Ok(axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(axum::body::Body::from(
                "Cannot delete personal organizations",
            ))
            .unwrap()
            .into_response());
    }

    // Delete all org members
    org_members::Entity::delete_many()
        .filter(org_members::Column::OrgId.eq(org.id))
        .exec(&ctx.db)
        .await?;

    // Delete all org invites
    crate::models::_entities::org_invites::Entity::delete_many()
        .filter(crate::models::_entities::org_invites::Column::OrgId.eq(org.id))
        .exec(&ctx.db)
        .await?;

    // Delete the organization itself
    let active: organizations::ActiveModel = org.into();
    active.delete(&ctx.db).await?;

    Ok(Redirect::to("/orgs").into_response())
}

/// GET /orgs/switch/:pid — switch active org
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
///
/// # Panics
///
/// Panics if the cookie value is not valid ASCII.
#[debug_handler]
pub async fn switch(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);

    // Verify membership (admins can access any org)
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let _membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;

    let cookie = Cookie::build(("org_pid", pid))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true)
        .build();

    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().expect("cookie is valid ASCII"),
    );
    Ok(response)
}

/// GET /invites/:token/accept — accept an invite
///
/// # Errors
///
/// Returns an error if the database query fails or the user is not authenticated.
///
/// # Panics
///
/// Panics if the HTTP response builder fails, which should not occur with valid status codes.
#[debug_handler]
pub async fn accept_invite(
    Path(token): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);

    let invite = org_invites::Model::find_by_pid(&ctx.db, &token)
        .await
        .ok_or_else(|| Error::NotFound)?;

    // Verify the authenticated user's email matches the invite recipient
    if user.email != invite.email {
        return Ok(axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(axum::body::Body::from("Forbidden"))
            .unwrap()
            .into_response());
    }

    org_invites::Model::accept_invite(&ctx.db, invite, user.id)
        .await
        .map_err(|e| Error::Message(e.to_string()))?;

    Ok(Redirect::to("/orgs").into_response())
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/orgs")
        .add("/", get(list))
        .add("/", post(create))
        .add("/new", get(new))
        .add("/{pid}/settings", get(settings))
        .add("/{pid}/settings", post(update_settings))
        .add("/{pid}/members", get(members))
        .add("/{pid}/members/invite", post(invite))
        .add("/{pid}/members/{user_pid}/role", post(update_role))
        .add("/{pid}/members/{user_pid}/remove", post(remove_member))
        .add("/{pid}/delete", post(delete))
        .add("/switch/{pid}", get(switch))
}

pub fn invite_routes() -> Routes {
    Routes::new()
        .prefix("/invites")
        .add("/{token}/accept", get(accept_invite))
}
