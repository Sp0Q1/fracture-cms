use axum_extra::extract::CookieJar;
use loco_rs::{auth::jwt, prelude::*};

use crate::models::_entities::{org_members, organizations, users};
use crate::models::org_members::OrgRole;

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
}

/// Resolves the active org from the `org_pid` cookie.
/// Falls back to the user's first org if the cookie is missing or invalid.
pub async fn get_org_context_or_default(
    jar: &CookieJar,
    db: &DatabaseConnection,
    user: &users::Model,
) -> Option<OrgContext> {
    // Try cookie first
    if let Some(cookie) = jar.get("org_pid") {
        let org_pid = cookie.value();
        if let Some(org) = organizations::Model::find_by_pid(db, org_pid).await {
            if let Some(membership) = org_members::Model::find_membership(db, org.id, user.id).await
            {
                let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
                return Some(OrgContext {
                    org,
                    membership,
                    role,
                });
            }
        }
    }

    // Fall back to first org
    let orgs = organizations::Model::find_orgs_for_user(db, user.id).await;
    let org = orgs.into_iter().next()?;
    let membership = org_members::Model::find_membership(db, org.id, user.id).await?;
    let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
    Some(OrgContext {
        org,
        membership,
        role,
    })
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
