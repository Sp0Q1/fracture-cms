use loco_rs::prelude::*;

/// Render the home landing page.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn index(v: &impl ViewRenderer) -> Result<Response> {
    format::render().view(v, "home/index.html", data!({}))
}
