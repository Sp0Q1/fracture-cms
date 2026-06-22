//! Public contact form (Altcha-protected) and the platform-admin inbox.
//!
//! Messages are stored in the database (no mail dependency — production
//! deployments default to mail off); platform admins read and delete them
//! at /admin/contact.

use axum::response::Redirect;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::Form;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::controllers::middleware;
use crate::models::{contact_messages, organizations as org_model};
use crate::views;
use crate::{captcha, require_platform_admin, require_user};

const MAX_NAME: usize = 200;
const MAX_EMAIL: usize = 320;
const MAX_MESSAGE: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContactParams {
    pub name: String,
    pub email: String,
    pub message: String,
    /// Base64 Altcha solution payload, set by the widget.
    pub altcha: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContactQuery {
    pub sent: Option<u8>,
}

/// GET /contact — public contact form.
///
/// # Errors
///
/// Returns an error if template rendering fails.
#[debug_handler]
pub async fn show(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    Query(query): Query<ContactQuery>,
    jar: CookieJar,
) -> Result<Response> {
    let nav = middleware::public_nav_context(&jar, &ctx).await;
    // Not cacheable even for guests: the embedded state (sent banner) and
    // the single-use captcha challenge flow make it per-visit.
    views::contact::form(&v, nav.as_ref(), query.sent == Some(1))
}

/// POST /contact — submit the form. The Altcha payload is verified
/// server-side before anything is stored.
///
/// # Errors
///
/// Returns `BadRequest` for failed captcha or invalid fields.
#[debug_handler]
pub async fn submit(
    State(ctx): State<AppContext>,
    Form(params): Form<ContactParams>,
) -> Result<Response> {
    let payload = params.altcha.as_deref().unwrap_or_default();
    if let Err(e) = captcha::verify_payload(payload) {
        tracing::info!(reason = %e, "contact form rejected by captcha");
        return Err(Error::BadRequest(
            "captcha verification failed — please try again".to_string(),
        ));
    }

    let name = params.name.trim();
    let email = params.email.trim();
    let message = params.message.trim();
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(Error::BadRequest("please provide your name".to_string()));
    }
    if email.is_empty() || email.len() > MAX_EMAIL || !email.contains('@') {
        return Err(Error::BadRequest(
            "please provide a valid email address".to_string(),
        ));
    }
    if message.is_empty() || message.len() > MAX_MESSAGE {
        return Err(Error::BadRequest(format!(
            "message must be between 1 and {MAX_MESSAGE} characters"
        )));
    }

    contact_messages::Model::create(&ctx.db, name, email, message).await?;
    Ok(Redirect::to("/contact?sent=1").into_response())
}

/// GET /admin/contact — list received messages (platform admin only).
///
/// # Errors
///
/// Returns an error if the user is not a platform admin or a query fails.
#[debug_handler]
pub async fn admin_index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);
    let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
        .await
        .unwrap_or_default();

    let messages = contact_messages::Model::find_recent(&ctx.db, 200).await?;
    views::contact::admin_index(&v, &user, org_ctx.as_ref(), &user_orgs, &messages)
}

/// POST /admin/contact/{pid}/delete — delete a message (platform admin only).
///
/// # Errors
///
/// Returns an error if the message is not found or the user is not a
/// platform admin.
#[debug_handler]
pub async fn admin_delete(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
    require_platform_admin!(org_ctx);

    let message = contact_messages::Model::find_by_pid(&ctx.db, &pid)
        .await?
        .ok_or_else(|| Error::NotFound)?;
    let active: contact_messages::ActiveModel = message.into();
    active.delete(&ctx.db).await?;

    Ok(Redirect::to("/admin/contact").into_response())
}

pub fn public_routes() -> Routes {
    Routes::new()
        .prefix("/contact")
        .add("/", get(show))
        .add("/", post(submit))
}

pub fn admin_routes() -> Routes {
    Routes::new()
        .prefix("/admin/contact")
        .add("/", get(admin_index))
        .add("/{pid}/delete", post(admin_delete))
}
