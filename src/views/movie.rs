use loco_rs::prelude::*;

use crate::models::_entities::movies;

/// Render a list view of `movies`.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn list(
    v: &impl ViewRenderer,
    items: &Vec<movies::Model>,
    user_name: &Option<String>,
) -> Result<Response> {
    format::render().view(
        v,
        "movie/list.html",
        data!({"items": items, "user_name": user_name}),
    )
}

/// Render a single `movie` view.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn show(
    v: &impl ViewRenderer,
    item: &movies::Model,
    user_name: &Option<String>,
) -> Result<Response> {
    format::render().view(
        v,
        "movie/show.html",
        data!({"item": item, "user_name": user_name}),
    )
}

/// Render a `movie` create form.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn create(v: &impl ViewRenderer, user_name: &Option<String>) -> Result<Response> {
    format::render().view(v, "movie/create.html", data!({"user_name": user_name}))
}

/// Render a `movie` edit form.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn edit(
    v: &impl ViewRenderer,
    item: &movies::Model,
    user_name: &Option<String>,
) -> Result<Response> {
    format::render().view(
        v,
        "movie/edit.html",
        data!({"item": item, "user_name": user_name}),
    )
}
