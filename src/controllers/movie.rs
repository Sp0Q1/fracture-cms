#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use axum::response::Redirect;
use axum_extra::extract::{CookieJar, Form};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use super::middleware;
use crate::{
    models::_entities::movies::{ActiveModel, Model},
    views,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub title: Option<String>,
}

impl Params {
    fn update(&self, item: &mut ActiveModel) {
        item.title = Set(self.title.clone());
    }
}

const LOGIN_REDIRECT: &str = "/api/auth/oidc/authorize";

macro_rules! require_user {
    ($user:expr) => {
        match $user {
            Some(u) => u,
            None => return Ok(Redirect::temporary(LOGIN_REDIRECT).into_response()),
        }
    };
}

#[debug_handler]
pub async fn list(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let user_name = Some(user.name.clone());
    let items = Model::find_by_user(&ctx.db, user.id).await;
    views::movie::list(&v, &items, &user_name)
}

#[debug_handler]
pub async fn new(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let user_name = Some(user.name);
    views::movie::create(&v, &user_name)
}

#[debug_handler]
pub async fn update(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let item = Model::find_by_id_and_user(&ctx.db, id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    let mut item = item.into_active_model();
    params.update(&mut item);
    item.update(&ctx.db).await?;
    Ok(Redirect::to("../movies").into_response())
}

#[debug_handler]
pub async fn edit(
    Path(id): Path<i32>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let user_name = Some(user.name.clone());
    let item = Model::find_by_id_and_user(&ctx.db, id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    views::movie::edit(&v, &item, &user_name)
}

#[debug_handler]
pub async fn show(
    Path(id): Path<i32>,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let user_name = Some(user.name.clone());
    let item = Model::find_by_id_and_user(&ctx.db, id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    views::movie::show(&v, &item, &user_name)
}

#[debug_handler]
pub async fn add(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Form(params): Form<Params>,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let mut item = ActiveModel {
        ..Default::default()
    };
    params.update(&mut item);
    item.user_id = Set(Some(user.id));
    item.insert(&ctx.db).await?;
    Ok(Redirect::to("movies").into_response())
}

#[debug_handler]
pub async fn remove(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let item = Model::find_by_id_and_user(&ctx.db, id, user.id)
        .await
        .ok_or_else(|| Error::NotFound)?;
    item.delete(&ctx.db).await?;
    format::empty()
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("movies/")
        .add("/", get(list))
        .add("/", post(add))
        .add("new", get(new))
        .add("{id}", get(show))
        .add("{id}/edit", get(edit))
        .add("{id}", delete(remove))
        .add("{id}", post(update))
}
