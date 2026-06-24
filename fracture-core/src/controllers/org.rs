use axum::response::Redirect;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use sea_orm::TransactionTrait;

use crate::controllers::middleware;
use crate::mailers::invite::InviteMailer;
use crate::models::_entities::{org_members, organizations, users as users_entity};
use crate::models::org_members::{MemberWriteError, OrgRole};
use crate::models::{org_invites, organizations as org_model, uploads as upload_model};
use crate::views;
use crate::{require_role, require_staff, require_user};

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

/// Maps a refused membership write to the right HTTP outcome: 404 for
/// missing/forbidden (never 403, per the IDOR policy), a user-visible
/// message for the last-owner guard.
fn member_write_error(e: MemberWriteError) -> Error {
    match e {
        MemberWriteError::NotFound | MemberWriteError::Forbidden => Error::NotFound,
        MemberWriteError::LastOwner(_) => Error::Message(e.to_string()),
        MemberWriteError::Db(db) => db.into(),
    }
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
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    // Resolve the viewer's role in each org so the list only offers the
    // Members/Settings actions they can actually use (Admin+ in that org, or
    // platform admin). Platform admins see every org but may not be a member
    // of it — they manage all of them.
    let is_staff = org_ctx.as_ref().is_some_and(|o| o.is_staff);
    let mut items = Vec::with_capacity(user_orgs.len());
    for org in &user_orgs {
        let role = org_members::Model::find_membership(&ctx.db, org.id, user.id)
            .await
            .ok()
            .flatten()
            .map(|m| m.role);
        let can_manage = is_staff
            || role
                .as_deref()
                .and_then(OrgRole::from_str_role)
                .is_some_and(|r| r.at_least(OrgRole::Admin));
        items.push(views::org::OrgListItem {
            org,
            role,
            can_manage,
        });
    }
    views::org::list(&v, &user, org_ctx.as_ref(), &items)
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
    // Org creation is staff-only; clients request additional orgs out of band.
    require_staff!(org_ctx);
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    views::org::new(&v, &user, org_ctx.as_ref(), &user_orgs)
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
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    // Org creation is staff-only.
    require_staff!(org_ctx);

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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id)
        .await
        .unwrap_or_default();
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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Viewer);

    let member_users = org_members::Model::find_members_with_users(&ctx.db, org.id).await?;
    let pending_invites = org_invites::Model::find_pending_by_org(&ctx.db, org.id).await?;
    let user_orgs = org_model::Model::find_visible_orgs(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    // Flag which members are platform staff so the template renders them
    // read-only — a tenant admin can't manage a platform operator.
    let mut staff_user_ids = std::collections::HashSet::new();
    for (_, member) in &member_users {
        if org_model::Model::is_user_staff(&ctx.db, member.id).await {
            staff_user_ids.insert(member.id);
        }
    }
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
            staff_user_ids: &staff_user_ids,
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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let invite_role = OrgRole::from_str_role(&params.role).unwrap_or(OrgRole::Member);
    // An inviter may not grant a role above their own — otherwise an Admin could
    // invite a confederate as Owner and bypass the Owner guard in `update_role`.
    if !org_ctx.role.at_least(invite_role) {
        return Err(Error::NotFound);
    }
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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let target_user = users_entity::Model::find_by_pid(&ctx.db, &user_pid)
        .await
        .map_err(|_| Error::NotFound)?;
    // Platform staff are managed by the platform, not by tenant admins — their
    // org role is cosmetic next to the platform-admin ceiling. Refuse here too,
    // not just in the UI, so a crafted request can't change it either.
    if org_model::Model::is_user_staff(&ctx.db, target_user.id).await {
        return Err(Error::BadRequest(
            "staff members are managed by the platform, not the organization".to_string(),
        ));
    }
    let new_role = OrgRole::from_str_role(&params.role).unwrap_or(OrgRole::Member);

    // The role ceiling (an actor may not touch a member above their own rank,
    // nor grant a role above it) is enforced inside the model's transaction,
    // against the membership row as it exists at write time — a check here
    // would race a concurrent promotion of the target.
    org_members::Model::update_role(&ctx.db, org.id, target_user.id, org_ctx.role, new_role)
        .await
        .map_err(member_write_error)?;

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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;
    require_role!(org_ctx, OrgRole::Admin);

    let target_user = users_entity::Model::find_by_pid(&ctx.db, &user_pid)
        .await
        .map_err(|_| Error::NotFound)?;
    // Tenant admins can't remove platform staff (see `update_role`).
    if org_model::Model::is_user_staff(&ctx.db, target_user.id).await {
        return Err(Error::BadRequest(
            "staff members are managed by the platform, not the organization".to_string(),
        ));
    }
    // Role ceiling enforced in the model's transaction; see `update_role`.
    org_members::Model::remove_member(&ctx.db, org.id, target_user.id, org_ctx.role)
        .await
        .map_err(member_write_error)?;

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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let org_ctx =
        middleware::OrgContext::from_membership(&ctx.db, org.clone(), membership, user.id).await;

    // Platform admins can delete any org. Org owners can delete their own non-personal org.
    let is_owner = org_ctx.role.at_least(OrgRole::Owner);
    if !org_ctx.is_staff && !is_owner {
        return Ok(crate::controllers::errors::forbidden());
    }

    // Cannot delete platform admin orgs
    if org.is_staff {
        return Ok(crate::controllers::errors::error_page(
            axum::http::StatusCode::FORBIDDEN,
            "Can't delete this organization",
            "The platform admin organization can't be deleted.",
        ));
    }

    // Refuse if any member's only org is this one. With no personal orgs to
    // fall back to, deleting it would leave them with no organization.
    if org_model::Model::has_member_whose_only_org_is(&ctx.db, org.id).await? {
        return Ok(crate::controllers::errors::error_page(
            axum::http::StatusCode::CONFLICT,
            "Can't delete this organization",
            "A member would be left with no organization. Move members to another org first.",
        ));
    }

    // For personal orgs: also delete the associated user
    let delete_user_id = if org.is_personal {
        // Find the owner of the personal org
        let owner = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org.id))
            .filter(org_members::Column::Role.eq("owner"))
            .one(&ctx.db)
            .await?;
        owner.map(|m| m.user_id)
    } else {
        None
    };

    // Cannot delete yourself via personal org deletion
    if delete_user_id == Some(user.id) {
        return Ok(crate::controllers::errors::error_page(
            axum::http::StatusCode::FORBIDDEN,
            "Can't delete this organization",
            "You can't delete your own account this way.",
        ));
    }

    let org_id = org.id;

    // Capture every upload row this deletion will cascade away, so the
    // on-disk files can be removed after commit: the org's own uploads, plus
    // — when the personal org's user is deleted too — that user's uploads in
    // every other org (`fk-uploads-uploaded_by` cascades on user delete).
    // A failed snapshot aborts before anything is deleted; proceeding with a
    // partial list would orphan the missing files forever.
    let mut doomed_uploads = upload_model::Model::find_by_org(&ctx.db, org_id).await?;
    if let Some(uid) = delete_user_id {
        let known: std::collections::HashSet<i32> = doomed_uploads.iter().map(|u| u.id).collect();
        let user_uploads = upload_model::Model::find_by_uploader(&ctx.db, uid).await?;
        doomed_uploads.extend(user_uploads.into_iter().filter(|u| !known.contains(&u.id)));
    }

    // Perform all deletions atomically so a mid-way failure can't leave the org
    // half-deleted (orphaned members/invites or a dangling user).
    let txn = ctx.db.begin().await?;

    org_members::Entity::delete_many()
        .filter(org_members::Column::OrgId.eq(org_id))
        .exec(&txn)
        .await?;
    crate::models::_entities::org_invites::Entity::delete_many()
        .filter(crate::models::_entities::org_invites::Column::OrgId.eq(org_id))
        .exec(&txn)
        .await?;

    let org_active: organizations::ActiveModel = org.into();
    org_active.delete(&txn).await?;

    // If personal org, also delete the user and their other memberships.
    if let Some(uid) = delete_user_id {
        org_members::Entity::delete_many()
            .filter(org_members::Column::UserId.eq(uid))
            .exec(&txn)
            .await?;
        if let Some(u) = users_entity::Entity::find_by_id(uid).one(&txn).await? {
            let u_active: users_entity::ActiveModel = u.into();
            u_active.delete(&txn).await?;
        }
    }

    txn.commit().await?;

    // Best-effort: remove the cascaded uploads' files from disk now that the
    // rows are gone. A failure here only leaves orphaned files, never
    // inconsistent data.
    if !doomed_uploads.is_empty() {
        if let Ok(service) = crate::controllers::uploads::get_upload_service(&ctx).await {
            for upload in &doomed_uploads {
                let _ = service.delete_file(upload).await;
            }
        }
    }

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
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let _membership = org_members::Model::find_membership_or_admin(&ctx.db, org.id, user.id)
        .await?
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
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Verify the authenticated user's email matches the invite recipient
    if user.email != invite.email {
        return Ok(crate::controllers::errors::forbidden());
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
