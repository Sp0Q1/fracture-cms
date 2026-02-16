#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unused_async)]
use axum_extra::extract::CookieJar;
use loco_rs::prelude::*;

use super::middleware;
use crate::views;

#[debug_handler]
pub async fn index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user_name = user.map(|u| u.name);
    views::home::index(&v, &user_name)
}

pub fn routes() -> Routes {
    Routes::new().prefix("/").add("/", get(index))
}
