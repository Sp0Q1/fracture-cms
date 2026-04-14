use axum_extra::extract::CookieJar;
use loco_rs::prelude::*;

use super::middleware;
use crate::models::{organizations as org_model, projects};
use crate::views;

/// Render the home page (authenticated or guest).
///
/// # Errors
///
/// Returns an error if the database query fails or template rendering fails.
#[debug_handler]
pub async fn index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    match user {
        Some(user) => {
            let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
            let user_orgs = org_model::Model::find_orgs_for_user(&ctx.db, user.id)
                .await
                .unwrap_or_default();
            let project_count = if let Some(ref oc) = org_ctx {
                projects::Model::find_by_org(&ctx.db, oc.org.id)
                    .await
                    .unwrap_or_default()
                    .len()
            } else {
                0
            };
            views::home::index(&v, &user, org_ctx.as_ref(), &user_orgs, project_count)
        }
        None => views::home::index_guest(&v),
    }
}

pub fn routes() -> Routes {
    Routes::new().prefix("/").add("", get(index))
}
