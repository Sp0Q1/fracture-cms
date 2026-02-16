use loco_rs::prelude::*;

use crate::models::_entities::movies;

/// Render the home page.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn index(
    v: &impl ViewRenderer,
    user_name: &Option<String>,
    items: &Vec<movies::Model>,
) -> Result<Response> {
    format::render().view(
        v,
        "home/index.html",
        data!({"user_name": user_name, "items": items}),
    )
}
