use axum_extra::extract::CookieJar;
use loco_rs::{auth::jwt, prelude::*};

use crate::models::_entities::{org_members, organizations, users};
use crate::models::org_members::OrgRole;

/// Extracts and validates the current user from the JWT cookie.
/// Returns `None` if the cookie is missing, the JWT is invalid, or the session has been invalidated.
pub async fn get_current_user(jar: &CookieJar, ctx: &AppContext) -> Option<users::Model> {
    let token = jar.get("jwt")?.value().to_string();
    let jwt_config = ctx.config.get_jwt_config().ok()?;
    let claims = jwt::JWT::new(&jwt_config.secret).validate(&token).ok()?;
    let user = users::Model::find_by_pid(&ctx.db, &claims.claims.pid)
        .await
        .ok()?;
    if user.session_invalidated_at.is_some() {
        return None;
    }
    Some(user)
}

/// Organization context resolved on every authenticated request.
#[derive(Debug, Clone)]
pub struct OrgContext {
    pub org: organizations::Model,
    pub membership: org_members::Model,
    pub role: OrgRole,
    /// True if the user is a member of any org with `is_staff`.
    pub is_staff: bool,
}

impl OrgContext {
    /// Constructs an `OrgContext` for a specific org/membership pair.
    /// Use this in handlers that resolve the org from a path parameter
    /// rather than the cookie.
    pub async fn from_membership(
        db: &DatabaseConnection,
        org: organizations::Model,
        membership: org_members::Model,
        user_id: i32,
    ) -> Self {
        let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
        let is_staff = organizations::Model::is_user_staff(db, user_id).await;
        Self {
            org,
            membership,
            role,
            is_staff,
        }
    }
}

/// Resolves the active org from the `org_pid` cookie.
/// Falls back to the user's first org if the cookie is missing or invalid.
pub async fn get_org_context_or_default(
    jar: &CookieJar,
    db: &DatabaseConnection,
    user: &users::Model,
) -> Option<OrgContext> {
    let is_staff = organizations::Model::is_user_staff(db, user.id).await;

    // Try cookie first
    if let Some(cookie) = jar.get("org_pid") {
        let org_pid = cookie.value();
        if let Some(org) = organizations::Model::find_by_pid(db, org_pid)
            .await
            .ok()
            .flatten()
        {
            if let Some(membership) = org_members::Model::find_membership(db, org.id, user.id)
                .await
                .ok()
                .flatten()
            {
                let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
                return Some(OrgContext {
                    org,
                    membership,
                    role,
                    is_staff,
                });
            }
            // Platform admins can access any org even without membership
            if is_staff {
                return Some(OrgContext {
                    membership: org_members::Model::virtual_admin(org.id, user.id),
                    org,
                    role: OrgRole::Owner,
                    is_staff,
                });
            }
        }
    }

    // Fall back to first org
    let orgs = organizations::Model::find_visible_orgs(db, user.id)
        .await
        .unwrap_or_default();
    let org = orgs.into_iter().next()?;
    let membership = org_members::Model::find_membership(db, org.id, user.id)
        .await
        .ok()
        .flatten();
    // Resolve the role lazily: only bail when the member genuinely has no role
    // and isn't a platform admin. (A previous `unwrap_or(... return None)`
    // evaluated the `return None` eagerly, dropping every non-admin member that
    // reached the fallback — e.g. any member without a valid `org_pid` cookie.)
    let role = match membership
        .as_ref()
        .and_then(|m| OrgRole::from_str_role(&m.role))
    {
        Some(role) => role,
        None if is_staff => OrgRole::Admin,
        None => return None,
    };
    let membership =
        membership.unwrap_or_else(|| org_members::Model::virtual_admin(org.id, user.id));
    Some(OrgContext {
        org,
        membership,
        role,
        is_staff,
    })
}

/// Builds the navigation context for a *public* page.
///
/// Returns `None` for guests — the caller keeps the page a cacheable marketing
/// shell. For a signed-in visitor it returns the full authenticated nav context
/// (the same shape [`crate::views::base_context`] builds for in-app pages: org
/// switcher, account menu, admin link), so public pages reachable while logged
/// in (e.g. the blog) keep nav continuity instead of dropping the switcher.
pub async fn public_nav_context(jar: &CookieJar, ctx: &AppContext) -> Option<serde_json::Value> {
    let user = get_current_user(jar, ctx).await?;
    let org_ctx = get_org_context_or_default(jar, &ctx.db, &user).await;
    let user_orgs = organizations::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();
    Some(crate::views::base_context(
        &user,
        org_ctx.as_ref(),
        &user_orgs,
    ))
}

/// Resolves the actor's capabilities on `resource` in the current org context.
///
/// The ergonomic wrapper around [`crate::permissions::capabilities`] that pulls
/// `is_staff`/`role` from the [`OrgContext`]. Pair with
/// `require_capability!` to enforce.
///
/// # Errors
///
/// Returns an error if reading per-user grants fails.
pub async fn capabilities<R>(
    db: &DatabaseConnection,
    org_ctx: &OrgContext,
    user_id: i32,
    resource: &R,
) -> Result<crate::permissions::Capabilities, sea_orm::DbErr>
where
    R: crate::permissions::Authorizable + Sync,
{
    crate::permissions::capabilities(db, user_id, org_ctx.is_staff, org_ctx.role, resource).await
}

/// Enforce a capability on an already-resolved set of capabilities.
///
/// Returns 404 (not 403) on denial so resource existence isn't leaked — the
/// project convention. Use instead of an inline `if` in the handler:
/// `require_capability!(caps, EDIT);`
#[macro_export]
macro_rules! require_capability {
    ($caps:expr, $cap:expr) => {
        if !$caps.allows($cap) {
            return Err(loco_rs::Error::NotFound);
        }
    };
}

/// Macro to require platform admin. Returns 403 if the user is not a platform admin.
#[macro_export]
macro_rules! require_staff {
    ($org_ctx:expr) => {
        if !$org_ctx.as_ref().is_some_and(|ctx| ctx.is_staff) {
            return Ok(axum::response::Response::builder()
                .status(axum::http::StatusCode::FORBIDDEN)
                .header(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(axum::body::Body::from(
                    "<h1>403 Forbidden</h1><p>You do not have admin access.</p>",
                ))
                .expect("static response body")
                .into_response());
        }
    };
}

/// Macro to require a minimum role. Returns 403 if the user's role is insufficient.
#[macro_export]
macro_rules! require_role {
    ($org_ctx:expr, $minimum:expr) => {
        if !$org_ctx.role.at_least($minimum) {
            return Ok(axum::response::Response::builder()
                .status(axum::http::StatusCode::FORBIDDEN)
                .body(axum::body::Body::from("Forbidden"))
                .unwrap()
                .into_response());
        }
    };
}

/// Macro to require an authenticated user, redirecting to OIDC login if not present.
#[macro_export]
macro_rules! require_user {
    ($user:expr) => {
        match $user {
            Some(u) => u,
            None => {
                return Ok(
                    axum::response::Redirect::temporary("/api/auth/oidc/authorize").into_response(),
                )
            }
        }
    };
}
