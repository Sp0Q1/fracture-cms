use axum_extra::extract::CookieJar;
use loco_rs::{auth::jwt, prelude::*};

use crate::models::_entities::users;

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
