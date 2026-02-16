#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unused_async)]
use axum_extra::extract::CookieJar;
use loco_rs::prelude::*;

use super::middleware;
use crate::{models::movies, views};

#[debug_handler]
pub async fn index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    match user {
        Some(user) => {
            let items = movies::Model::find_by_user(&ctx.db, user.id).await;
            let user_name = Some(user.name);
            views::home::index(&v, &user_name, &items)
        }
        None => views::home::index(&v, &None, &vec![]),
    }
}

pub fn routes() -> Routes {
    Routes::new().prefix("/").add("/", get(index))
}
