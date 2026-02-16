#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unused_async)]
use loco_rs::prelude::*;

use crate::views;

#[debug_handler]
pub async fn index(ViewEngine(v): ViewEngine<TeraView>) -> Result<Response> {
    views::home::index(&v)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/")
        .add("/", get(index))
}
