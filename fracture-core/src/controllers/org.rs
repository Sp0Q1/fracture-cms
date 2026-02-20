#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
#![allow(clippy::doc_markdown)]
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
use crate::{require_role, require_user};
use crate::views;

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
#[debug_handler]
pub async fn list(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;
    views::org::list(&v, &user, &org_ctx, &user_orgs)
}

/// GET /orgs/new — new org form
#[debug_handler]
pub async fn new(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;
    views::org::new(&v, &user, &org_ctx, &user_orgs)
}

/// POST /orgs/ — create organization
#[debug_handler]
pub async fn create(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<NewOrgParams>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);

    let slug = slug::slugify(&params.name);
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
    let membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    let org_ctx = middleware::OrgContext {
        org: org.clone(),
        membership,
        role,
    };
    require_role!(org_ctx, OrgRole::Admin);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;
    views::org::settings(&v, &user, &org_ctx, &user_orgs, &org)
}

/// POST /orgs/:pid/settings — update org settings
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
    let membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    let org_ctx = middleware::OrgContext {
        org: org.clone(),
        membership,
        role,
    };
    require_role!(org_ctx, OrgRole::Admin);

    let mut active: organizations::ActiveModel = org.into();
    active.name = sea_orm::ActiveValue::Set(params.name);
    active.update(&ctx.db).await?;

    Ok(Redirect::to(&format!("/orgs/{pid}/settings")).into_response())
}

/// GET /orgs/:pid/members — members list
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
    let membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    let org_ctx = middleware::OrgContext {
        org: org.clone(),
        membership,
        role,
    };
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
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id).await;
    let app_url = ctx.config.server.host.clone();
    views::org::members(
        &v,
        &user,
        &org_ctx,
        &user_orgs,
        &org,
        &member_users,
        &pending_invites,
        &app_url,
    )
}

/// POST /orgs/:pid/members/invite — invite a member
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
    let membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    let org_ctx = middleware::OrgContext {
        org: org.clone(),
        membership,
        role,
    };
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

/// POST /orgs/:pid/members/:user_pid/role — update member role
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
    let membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    let org_ctx = middleware::OrgContext {
        org: org.clone(),
        membership,
        role,
    };
    require_role!(org_ctx, OrgRole::Admin);

    let target_user = users_entity::Model::find_by_pid(&ctx.db, &user_pid)
        .await
        .map_err(|_| Error::NotFound)?;
    let target_membership = org_members::Model::find_membership(&ctx.db, org.id, target_user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let new_role = OrgRole::from_str_role(&params.role).unwrap_or(OrgRole::Member);
    org_members::Model::update_role(&ctx.db, target_membership, new_role).await?;

    Ok(Redirect::to(&format!("/orgs/{pid}/members")).into_response())
}

/// POST /orgs/:pid/members/:user_pid/remove — remove member
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
    let membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    let org_ctx = middleware::OrgContext {
        org: org.clone(),
        membership,
        role,
    };
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

/// GET /orgs/switch/:pid — switch active org
#[debug_handler]
pub async fn switch(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);

    // Verify membership
    let org = org_model::Model::find_by_pid(&ctx.db, &pid)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let _membership = org_members::Model::find_membership(&ctx.db, org.id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;

    let cookie = Cookie::build(("org_pid", pid))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().expect("cookie is valid ASCII"),
    );
    Ok(response)
}

/// GET /invites/:token/accept — accept an invite
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
        .add("/switch/{pid}", get(switch))
}

pub fn invite_routes() -> Routes {
    Routes::new()
        .prefix("/invites")
        .add("/{token}/accept", get(accept_invite))
}
